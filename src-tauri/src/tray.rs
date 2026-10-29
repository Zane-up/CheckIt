//! 系统托盘：打开 / 添加任务 / 置顶切换 / 设置 / 退出。
//!
//! 菜单事件在主线程回调中派发：所有窗口创建/显示操作必须投递到后台线程，
//! 避免在主线程同步 `.build()` 阻塞 WebView2 初始化（死锁）。

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// 置顶菜单项（托管状态：点击后同步勾选）
pub struct TrayPinItem(pub std::sync::Mutex<Option<CheckMenuItem<tauri::Wry>>>);

struct TrayTexts { items: Vec<(MenuItem<tauri::Wry>, &'static str, &'static str)>, pin: CheckMenuItem<tauri::Wry> }

pub fn sync_language(app: &AppHandle, language: &str) {
    let english=language=="en";
    if let Some(texts)=app.try_state::<TrayTexts>() {
        for (item,zh,en) in &texts.items { let _=item.set_text(if english {*en} else {*zh}); }
        let _=texts.pin.set_text(if english {"Always on top"} else {"窗口置顶"});
    }
    if let Some(w)=app.get_webview_window("manager") {let _=w.set_title(if english {"CheckIt - Task manager"} else {"CheckIt - 任务管理"});}
    if let Some(w)=app.get_webview_window("reminder") {let _=w.set_title(if english {"CheckIt - Task reminders"} else {"CheckIt · 任务提醒"});}
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "打开 CheckIt", true, None::<&str>)?;
    let add_item = MenuItem::with_id(app, "add", "添加任务", true, None::<&str>)?;
    let manager_item = MenuItem::with_id(app, "manager", "任务管理", true, None::<&str>)?;
    let pinned = app
        .get_webview_window("widget")
        .and_then(|w| w.is_always_on_top().ok())
        .unwrap_or(true);
    let pin_item = CheckMenuItem::with_id(app, "pin", "窗口置顶", true, pinned, None::<&str>)?;
    app.manage(TrayPinItem(std::sync::Mutex::new(Some(pin_item.clone()))));
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 CheckIt", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &add_item,
            &manager_item,
            &separator,
            &pin_item,
            &settings_item,
            &separator,
            &quit_item,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("CheckIt")
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            // 左键单击托盘图标 → 显示悬浮窗（同样走后台线程，规避主线程窗口操作）
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::windows::show_widget(&app) {
                        log::error!("显示悬浮窗失败: {e}");
                    }
                });
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    app.manage(TrayTexts {items:vec![(open_item,"打开 CheckIt","Open CheckIt"),(add_item,"添加任务","Add task"),(manager_item,"任务管理","Task manager"),(settings_item,"设置","Settings"),(quit_item,"退出 CheckIt","Quit CheckIt")],pin:pin_item});
    if let Ok(settings)=app.state::<crate::commands::CheckItState>().db.load_settings(){sync_language(app,&settings.language);}
    Ok(())
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::windows::show_widget(&app) {
                    log::error!("显示悬浮窗失败: {e}");
                }
            });
        }
        "add" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::windows::show_widget(&app) {
                    log::error!("显示悬浮窗失败: {e}");
                }
                let _ = app.emit("widget-quick-add", ());
            });
        }
        "manager" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::windows::open_manager(&app) {
                    log::error!("打开管理窗口失败: {e}");
                }
            });
        }
        "settings" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::navigation::open_target(&app, crate::navigation::ManagerTarget::Settings) {
                    log::error!("打开管理窗口失败: {e}");
                }
            });
        }
        "pin" => {
            // 统一置顶路径：设置持久化 + 窗口应用 + 托盘勾选 + 广播（内部完成）
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::windows::toggle_always_on_top(&app) {
                    log::error!("切换置顶失败: {e}");
                }
            });
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
