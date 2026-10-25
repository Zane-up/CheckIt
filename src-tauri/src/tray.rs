//! 系统托盘：打开 / 添加任务 / 置顶切换 / 设置 / 退出。

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// 置顶菜单项 ID
const PIN_ITEM_ID: &str = "pin";

/// 置顶菜单项（保存在托管状态中以便点击后同步勾选状态）
pub struct TrayPinItem(pub std::sync::Mutex<Option<CheckMenuItem<tauri::Wry>>>);

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "打开 CheckIt", true, None::<&str>)?;
    let add_item = MenuItem::with_id(app, "add", "添加任务", true, None::<&str>)?;
    let manager_item = MenuItem::with_id(app, "manager", "任务管理", true, None::<&str>)?;
    let pinned = app
        .get_webview_window("widget")
        .and_then(|w| w.is_always_on_top().ok())
        .unwrap_or(true);
    let pin_item = CheckMenuItem::with_id(app, PIN_ITEM_ID, "窗口置顶", true, pinned, None::<&str>)?;
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
            // 左键单击托盘图标 → 显示悬浮窗
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle();
                let _ = crate::windows::show_widget(app);
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => {
            let _ = crate::windows::show_widget(app);
        }
        "add" => {
            let _ = crate::windows::show_widget(app);
            let _ = app.emit("widget-quick-add", ());
        }
        "manager" => {
            let _ = crate::windows::open_manager(app);
        }
        "settings" => {
            let _ = crate::windows::open_manager(app);
        }
        "pin" => {
            // 切换悬浮窗置顶，并同步到设置持久化
            if let Some(widget) = app.get_webview_window("widget") {
                let next = !widget.is_always_on_top().unwrap_or(false);
                let _ = widget.set_always_on_top(next);
                if let Some(state) = app.try_state::<crate::commands::CheckItState>() {
                    if let Ok(mut settings) = state.db.load_settings() {
                        settings.always_on_top = next;
                        let _ = state.db.save_settings(&settings);
                        let _ = app.emit("data-changed", serde_json::json!({ "entity": "settings" }));
                    }
                }
                // 同步托盘勾选状态
                if let Some(pin_state) = app.try_state::<TrayPinItem>() {
                    if let Ok(guard) = pin_state.0.lock() {
                        if let Some(item) = guard.as_ref() {
                            let _ = item.set_checked(next);
                        }
                    }
                }
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
