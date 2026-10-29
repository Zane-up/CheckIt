//! CheckIt 库入口：组装数据库、调度器、托盘与命令。

pub mod commands;
mod updates;
mod backups;
pub mod database;
pub mod diagnostics;
pub mod dock;
pub mod models;
mod native_drag;
mod window_shape;
pub mod notification;
pub mod popup;
pub mod reminders;
pub mod settings;
pub mod tray;
pub mod windows;
mod window_transition;

use std::sync::mpsc;
use std::time::Duration;

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

use crate::commands::CheckItState;
use crate::database::Database;
use crate::notification::{NotificationTransport, SystemNotificationTransport};
use crate::reminders::{
    Clock, DeliveryOutcome, ReminderAction, ReminderEngine, SystemClock,
};

/// 提醒扫描间隔（空闲时）；可通过 wake 通道立即唤醒
const SCHEDULER_INTERVAL_SECS: u64 = 5;

/// 启动提醒调度线程：独立于任何窗口的显示状态，窗口隐藏时照常工作。
/// 每次扫描都基于真实当前时间重算，天然覆盖：休眠恢复、系统时间调整、重启补查。
fn spawn_scheduler(app: AppHandle, wake_rx: mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        let db = match app.try_state::<CheckItState>() {
            Some(state) => state.db.clone(),
            None => return,
        };
        let engine = ReminderEngine::new(db);
        // 真实提交适配器：同步调用 OS 通知 API 并保留底层结果。
        // 安装版使用产品 identifier 作为 AUMID，开发版沿用底层库默认值。
        let transport = SystemNotificationTransport::from_identifier(&app.config().identifier);

        loop {
            {
            let state = app.state::<CheckItState>();
            let _maintenance = state.maintenance.lock();
            // 一次扫描共用一个时钟：领取、恢复、宽限判断使用完全相同的 now。
            let now = Utc::now();
            match engine.run_once(now) {
                Ok(actions) => {
                    for action in actions {
                        // 扫描共用 now；每条动作真正提交前由 deliver_with_clock
                        // 重新读取实时钟并再次校验宽限，避免睡眠/阻塞后沿用旧时间。
                        let delivery = popup::ReminderTransport { app: &app, action: &action, system: &transport };
                        dispatch_action(&app, &engine, action.clone(), &SystemClock, &delivery);
                    }
                }
                Err(e) => log::error!("提醒扫描失败: {e}"),
            }

            // 恢复尚未确认的弹窗，并剔除已完成、删除、改期的任务。
            if let Err(e) = popup::refresh(&app) {
                log::warn!("刷新居中提醒失败: {e}");
            }
            }

            // 等待下一次：定时或唤醒信号
            match wake_rx.recv_timeout(Duration::from_secs(SCHEDULER_INTERVAL_SECS)) {
                Ok(()) => continue, // 被唤醒，立即重扫
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    log::info!("调度器通道关闭，线程退出");
                    return;
                }
            }
        }
    });
}

/// 执行提醒动作：实时取时 → 统一校验 → 按设置提交通知/弹窗 → 状态机落库 → 广播。
///
/// 状态流转：
/// - pending --所选渠道受理--> submitted
/// - pending --底层提交失败--> failed(attempts+1，30s 后可重试，最多 3 次)
/// - 任务完成/删除/改期/移除 offset --> cancelled
/// - 通知关闭/超出 15 分钟宽限 --> skipped
///
/// submitted 不代表用户确认；系统通知崩溃窗口可能重复提交（at-least-once），弹窗按事件 ID 幂等。
fn dispatch_action(
    app: &AppHandle,
    engine: &ReminderEngine,
    action: ReminderAction,
    clock: &dyn Clock,
    transport: &dyn NotificationTransport,
) {
    match engine.deliver_with_clock(&action, clock, transport) {
        Ok(DeliveryOutcome::Submitted { title }) => {
            // 所选渠道已受理后再广播（语义与 submitted 对齐）。
            let _ = app.emit(
                "reminder-fired",
                serde_json::json!({
                    "task_id": action.task_id,
                    "offset_seconds": action.offset_seconds,
                    "title": title,
                }),
            );
        }
        Ok(DeliveryOutcome::Failed { error }) => {
            log::error!("提醒提交失败，已进入重试状态机: {error}");
        }
        Ok(DeliveryOutcome::Skipped) => {
            log::info!(
                "提醒静默跳过（通知关闭或超出补发宽限）: task={} offset={}",
                action.task_id,
                action.offset_seconds
            );
        }
        Ok(DeliveryOutcome::Cancelled) => {
            log::info!(
                "提醒动作已失效并取消: task={} offset={}",
                action.task_id,
                action.offset_seconds
            );
        }
        Ok(DeliveryOutcome::Deferred) => {
            log::debug!(
                "提醒尚未到期，保留队列: task={} offset={}",
                action.task_id,
                action.offset_seconds
            );
        }
        Err(e) => {
            // 数据库错误：不擅自标记失败，保留 pending 供下一轮崩溃恢复。
            log::error!("提醒状态更新失败（保留 pending 待下轮恢复）: {e}");
        }
    }
}

/// 应用主入口
pub fn run() {
    tauri::Builder::default()
        // 产品策略：单实例。必须最先注册，第二实例会在插件 setup 阶段
        // 转发唤醒请求并退出，不进入应用 setup（数据库迁移/托盘/调度器）。
        //
        // 提醒引擎本身不提供跨进程原子领取租约；若绕过此保护启动第二个调度器，
        // 恢复动作可能被重复提交。因此“支持多实例”不在本版本保证范围内。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("检测到第二实例启动，唤醒已有窗口并退出第二实例");
            let app = app.clone();
            // 回调运行在已有实例的主线程：窗口显示/重建必须投递到后台，
            // 避免同步 WebView2 初始化阻塞主线程。
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::windows::show_widget(&app) {
                    log::error!("第二实例唤醒已有窗口失败: {e}");
                }
            });
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // 诊断日志：先于数据库/调度器初始化，确保失败能够落盘并保留 HRESULT。
            if let Ok(log_dir) = app.path().app_log_dir() {
                diagnostics::init(&log_dir);
            }

            // 数据库：写入系统应用数据目录
            let data_dir = app.path().app_data_dir()?;
            let db = Database::open(&data_dir.join("checkit.db"))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            db.ensure_default_categories()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            let (wake_tx, wake_rx) = mpsc::channel::<()>();
            app.manage(CheckItState {
                maintenance: parking_lot::Mutex::new(()),
                db: db.clone(),
                wake: parking_lot::Mutex::new(wake_tx),
            });

            // 开机启动对账（以系统实际状态修正本地记录，不执行系统写入）
            let settings =
                settings::reconcile_autostart_on_startup(app.handle(), &db).unwrap_or_else(|e| {
                    log::warn!("开机启动对账失败，使用本地设置: {e}");
                    db.load_settings().unwrap_or_default()
                });
            // 应用启动行为（置顶/玻璃）与窗口几何/模式恢复
            windows::apply_window_behaviors(app.handle(), &settings);
            windows::init_widget_state(app.handle());
            windows::restore_widget_geometry(app.handle());
            windows::setup_widget_events(app.handle());

            tray::setup_tray(app.handle())?;

            app.manage(popup::PopupState::default());
            popup::setup(app.handle());

            spawn_scheduler(app.handle().clone(), wake_rx);

            // 配置中的 WebView 可能先于 setup 完成加载。通知已订阅的首屏重取，
            // 避免增加提醒窗口后更容易遇到“state not managed”的初始化竞争。
            let _ = app.emit("data-changed", serde_json::json!({ "entity": "settings" }));

            log::info!("CheckIt 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            updates::open_release_page,
            backups::create_backup,
            backups::list_backups,
            backups::restore_backup,
            backups::open_backup_folder,
            popup::snooze_popup_reminders,
            popup::get_popup_reminders,
            popup::dismiss_popup_reminders,
            popup::preview_popup_reminder,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::set_task_status,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::update_settings,
            commands::toggle_always_on_top,
            commands::check_reminders_now,
            commands::get_db_path,
            commands::open_manager,
            commands::show_widget,
            commands::trigger_quick_add,
            commands::widget_expand,
            commands::widget_collapse,
            commands::widget_quick_add_closed,
            commands::widget_get_mode,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("CheckIt 启动失败");
}
