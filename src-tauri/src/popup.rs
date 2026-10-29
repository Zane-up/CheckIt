//! 居中提醒窗口：独立于主面板，队列持久化在 SQLite，前端取快照而不依赖瞬时事件。
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::CheckItState;
use crate::database::PopupReminder;
use crate::notification::{NotificationContent, NotificationTransport};
use crate::reminders::ReminderAction;

/// 串行化展示与确认，避免确认旧提醒时隐藏刚到达的新提醒。bool 仅表示临时预览。
#[derive(Default)]
pub struct PopupState(Mutex<bool>);

#[derive(Serialize)]
pub struct PopupSnapshot {
    reminders: Vec<PopupReminder>,
    preview: bool,
}

fn snapshot(app: &AppHandle, preview: bool) -> Result<PopupSnapshot, String> {
    let db = &app.state::<CheckItState>().db;
    let settings = db.load_settings().map_err(|e| e.to_string())?;
    let reminders = if settings.popup_reminders_enabled {
        db.list_popup_reminders().map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    Ok(PopupSnapshot { reminders, preview })
}

fn refresh_locked(app: &AppHandle, preview: bool) -> Result<(), String> {
    let data = snapshot(app, preview)?;
    let window = app.get_webview_window("reminder").ok_or("提醒窗口不可用")?;
    if data.reminders.is_empty() && !data.preview {
        window.hide().map_err(|e| e.to_string())?;
    } else if !window.is_visible().map_err(|e| e.to_string())? {
        // 优先使用悬浮窗所在显示器；移动到目标屏后再居中，以适配跨屏 DPI。
        if let Some(monitor) = app
            .get_webview_window("widget")
            .and_then(|w| w.current_monitor().ok().flatten())
        {
            window
                .set_position(*monitor.position())
                .map_err(|e| e.to_string())?;
        }
        window.center().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        // 不强制抢走正在输入的焦点，置顶展示即可。
    }
    // 事件只负责加速；首次加载与后续轮询始终从持久化收件箱重取。
    let _ = app.emit_to("reminder", "popup-reminders-changed", ());
    Ok(())
}

pub fn refresh(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<PopupState>();
    let preview = state.0.lock();
    refresh_locked(app, *preview)
}

/// 每条动作使用独立适配器，沿用原调度器的到期校验、宽限、失败重试和去重。
pub struct ReminderTransport<'a> {
    pub app: &'a AppHandle,
    pub action: &'a ReminderAction,
    pub system: &'a dyn NotificationTransport,
}

impl NotificationTransport for ReminderTransport<'_> {
    fn submit(&self, content: &NotificationContent) -> Result<(), String> {
        let db = &self.app.state::<CheckItState>().db;
        let settings = db.load_settings().map_err(|e| e.to_string())?;
        if settings.popup_reminders_enabled {
            let state = self.app.state::<PopupState>();
            let preview = state.0.lock();
            db.enqueue_popup(self.action, content)
                .map_err(|e| e.to_string())?;
            refresh_locked(self.app, *preview)?;
        }
        // 先处理可幂等重试的弹窗，再提交系统通知。
        // 若系统通知失败，原状态机会重试；已确认的弹窗不会重新入箱。
        if settings.notifications_enabled {
            self.system.submit(content)?;
        }
        Ok(())
    }
}

pub fn setup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("reminder") {
        if let Err(e) = crate::window_shape::setup(&window) {
            log::warn!("提醒窗口圆角裁切失败: {e}");
        }
        let handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = handle.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = app.state::<PopupState>();
                    let mut preview = state.0.lock();
                    let result = (|| {
                        let ids: Vec<i64> = snapshot(&app, *preview)?
                            .reminders
                            .iter()
                            .map(|r| r.id)
                            .collect();
                        app.state::<CheckItState>()
                            .db
                            .dismiss_popup_reminders(&ids)
                            .map_err(|e| e.to_string())?;
                        *preview = false;
                        refresh_locked(&app, false)
                    })();
                    if let Err(e) = result {
                        log::error!("确认提醒失败: {e}");
                    }
                });
            }
        });
    }
}

#[tauri::command]
pub async fn get_popup_reminders(app: AppHandle) -> Result<PopupSnapshot, String> {
    let state = app.state::<PopupState>();
    let preview = state.0.lock();
    snapshot(&app, *preview)
}

#[tauri::command]
pub async fn dismiss_popup_reminders(app: AppHandle, ids: Vec<i64>) -> Result<(), String> {
    if ids.len() > 10_000 {
        return Err("一次确认的提醒过多".into());
    }
    let state = app.state::<PopupState>();
    let mut preview = state.0.lock();
    app.state::<CheckItState>()
        .db
        .dismiss_popup_reminders(&ids)
        .map_err(|e| e.to_string())?;
    *preview = false;
    refresh_locked(&app, false)
}

#[tauri::command]
pub async fn preview_popup_reminder(app: AppHandle) -> Result<(), String> {
    let state = app.state::<PopupState>();
    let mut preview = state.0.lock();
    *preview = true;
    refresh_locked(&app, true)
}
