//! CheckIt 库入口：组装数据库、调度器、托盘与命令。

pub mod commands;
pub mod database;
pub mod models;
pub mod reminders;
pub mod settings;
pub mod tray;
pub mod windows;

use std::sync::mpsc;
use std::time::Duration;

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;

use crate::commands::CheckItState;
use crate::database::Database;
use crate::reminders::ReminderEngine;

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

        loop {
            // 立即扫描一次（启动补查 / 唤醒触发）
            match engine.run_once(Utc::now()) {
                Ok(actions) => {
                    for action in actions {
                        dispatch_action(&app, &engine, action);
                    }
                }
                Err(e) => log::error!("提醒扫描失败: {e}"),
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

/// 执行提醒动作：发送 Windows 原生通知
fn dispatch_action(app: &AppHandle, engine: &ReminderEngine, action: reminders::ReminderAction) {
    let notifications_on = engine
        .db()
        .load_settings()
        .map(|s| s.notifications_enabled)
        .unwrap_or(true);

    let (task_id, offset) = match &action {
        reminders::ReminderAction::Notify { task_id, offset_seconds } => (*task_id, *offset_seconds),
        reminders::ReminderAction::MarkMissed { task_id, .. } => {
            log::info!("任务 {} 的过期提醒已静默标记", task_id);
            return;
        }
    };

    let Ok(task) = engine.db().get_task(task_id) else {
        return;
    };
    // 通知构造期间任务可能已完成，二次确认
    if task.status != models::TaskStatus::Pending {
        return;
    }

    // 广播到前端（供未来应用内横幅使用；当前依赖系统通知）
    let _ = app.emit("reminder-fired", serde_json::json!({
        "task_id": task_id,
        "offset_seconds": offset,
        "title": task.title,
    }));

    if !notifications_on {
        return;
    }

    let due_text = task
        .due_at
        .as_deref()
        .and_then(reminders::parse_rfc3339)
        .map(|due| reminders::remaining_text(due, Utc::now()))
        .unwrap_or_default();

    let body = if due_text.is_empty() {
        "该任务需要处理".to_string()
    } else {
        format!("「{}」{}后截止", truncate_title(&task.title, 40), due_text)
    };

    if let Err(e) = app
        .notification()
        .builder()
        .title("CheckIt")
        .body(body.as_str())
        .show()
    {
        log::error!("发送通知失败: {e}");
    }
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

/// 应用主入口
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // 数据库：写入系统应用数据目录
            let data_dir = app.path().app_data_dir()?;
            let db = Database::open(&data_dir.join("checkit.db"))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            db.ensure_default_categories()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            let (wake_tx, wake_rx) = mpsc::channel::<()>();
            app.manage(CheckItState {
                db: db.clone(),
                wake: parking_lot::Mutex::new(wake_tx),
            });

            // 应用启动行为（置顶/玻璃）与窗口几何恢复
            let settings = db.load_settings().unwrap_or_default();
            windows::apply_window_behaviors(app.handle(), &settings);
            windows::restore_widget_geometry(app.handle());
            windows::setup_widget_events(app.handle());

            tray::setup_tray(app.handle())?;

            spawn_scheduler(app.handle().clone(), wake_rx);

            log::info!("CheckIt 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::set_task_status,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::update_settings,
            commands::check_reminders_now,
            commands::get_db_path,
            commands::open_manager,
            commands::show_widget,
            commands::trigger_quick_add,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("CheckIt 启动失败");
}
