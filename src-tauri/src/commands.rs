//! Tauri 命令层：前端 invoke 的全部入口。
//! 每个写操作完成后广播 data-changed 事件，驱动所有窗口同步刷新，
//! 并唤醒提醒调度器立即重扫（加速编辑/删除任务后的队列更新）。

use std::sync::mpsc::Sender;

use parking_lot::Mutex;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::database::{Database, DbError};
use crate::models::{AppSettings, Category, Task, TaskInput};

/// 共享应用状态
pub struct CheckItState {
    pub db: Database,
    /// 唤醒调度器的信号通道（mpsc Sender 非 Sync，用 Mutex 包装满足 State 的 Sync 约束）
    pub wake: Mutex<Sender<()>>,
}

fn broadcast(app: &AppHandle, entity: &str) {
    let _ = app.emit("data-changed", serde_json::json!({ "entity": entity }));
    if let Some(state) = app.try_state::<CheckItState>() {
        let _ = state.wake.lock().send(());
    }
}

fn to_invoke_error(e: DbError) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_app_state(state: tauri::State<CheckItState>) -> Result<crate::database::AppState, String> {
    state.db.get_app_state().map_err(to_invoke_error)
}

#[tauri::command]
pub fn create_task(app: AppHandle, state: tauri::State<CheckItState>, input: TaskInput) -> Result<Task, String> {
    let task = state.db.create_task(&input).map_err(to_invoke_error)?;
    broadcast(&app, "tasks");
    Ok(task)
}

#[tauri::command]
pub fn update_task(app: AppHandle, state: tauri::State<CheckItState>, id: i64, input: TaskInput) -> Result<Task, String> {
    let task = state.db.update_task(id, &input).map_err(to_invoke_error)?;
    broadcast(&app, "tasks");
    Ok(task)
}

#[tauri::command]
pub fn delete_task(app: AppHandle, state: tauri::State<CheckItState>, id: i64) -> Result<(), String> {
    state.db.delete_task(id).map_err(to_invoke_error)?;
    broadcast(&app, "tasks");
    Ok(())
}

#[tauri::command]
pub fn set_task_status(app: AppHandle, state: tauri::State<CheckItState>, id: i64, completed: bool) -> Result<Task, String> {
    let task = state.db.set_task_status(id, completed).map_err(to_invoke_error)?;
    broadcast(&app, "tasks");
    Ok(task)
}

#[tauri::command]
pub fn create_category(app: AppHandle, state: tauri::State<CheckItState>, name: String, color: String) -> Result<Category, String> {
    let cat = state.db.create_category(&name, &color).map_err(to_invoke_error)?;
    broadcast(&app, "categories");
    Ok(cat)
}

#[tauri::command]
pub fn update_category(app: AppHandle, state: tauri::State<CheckItState>, id: i64, name: String, color: String) -> Result<Category, String> {
    let cat = state.db.update_category(id, &name, &color).map_err(to_invoke_error)?;
    broadcast(&app, "categories");
    Ok(cat)
}

#[tauri::command]
pub fn delete_category(app: AppHandle, state: tauri::State<CheckItState>, id: i64) -> Result<(), String> {
    state.db.delete_category(id).map_err(to_invoke_error)?;
    broadcast(&app, "categories");
    Ok(())
}

#[tauri::command]
pub fn update_settings(app: AppHandle, state: tauri::State<CheckItState>, patch: SettingsPatch) -> Result<AppSettings, String> {
    // 读取当前设置，应用补丁，保存
    let mut current = state.db.load_settings().map_err(to_invoke_error)?;
    patch.apply(&mut current);
    state.db.save_settings(&current).map_err(to_invoke_error)?;

    // 同步窗口行为
    crate::windows::apply_window_behaviors(&app, &current);
    // 开机启动
    crate::settings::sync_autostart(&app, &current);

    broadcast(&app, "settings");
    Ok(current)
}

/// 设置补丁：只覆盖前端传入的字段（Option 字段 = 不修改）
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettingsPatch {
    theme: Option<String>,
    glass_enabled: Option<bool>,
    glass_opacity: Option<f64>,
    always_on_top: Option<bool>,
    close_to_tray: Option<bool>,
    notifications_enabled: Option<bool>,
    default_reminder_offsets: Option<Vec<i64>>,
    widget_display_count: Option<i64>,
    launch_at_login: Option<bool>,
}

impl SettingsPatch {
    fn apply(self, s: &mut AppSettings) {
        if let Some(v) = self.theme {
            s.theme = v;
        }
        if let Some(v) = self.glass_enabled {
            s.glass_enabled = v;
        }
        if let Some(v) = self.glass_opacity {
            s.glass_opacity = v;
        }
        if let Some(v) = self.always_on_top {
            s.always_on_top = v;
        }
        if let Some(v) = self.close_to_tray {
            s.close_to_tray = v;
        }
        if let Some(v) = self.notifications_enabled {
            s.notifications_enabled = v;
        }
        if let Some(v) = self.default_reminder_offsets {
            s.default_reminder_offsets = v;
        }
        if let Some(v) = self.widget_display_count {
            s.widget_display_count = v;
        }
        if let Some(v) = self.launch_at_login {
            s.launch_at_login = v;
        }
    }
}

/// 手动触发一次提醒检查（前端聚焦窗口 / 显式调用时加速补查）
#[tauri::command]
pub fn check_reminders_now(state: tauri::State<CheckItState>) -> Result<(), String> {
    let _ = state.wake.lock().send(());
    Ok(())
}

#[tauri::command]
pub fn get_db_path(app: AppHandle) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取数据目录: {e}"))?;
    Ok(dir.join("checkit.db").to_string_lossy().to_string())
}

/// 打开（或聚焦）完整管理窗口。
/// 必须为 async：在 sync command 的执行上下文中同步创建窗口会阻塞
/// （WebView2 controller 初始化完成事件无法送达），导致命令永不返回。
#[tauri::command]
pub async fn open_manager(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::windows::open_manager(&app).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 显示悬浮小组件
#[tauri::command]
pub async fn show_widget(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::windows::show_widget(&app).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 托盘请求悬浮窗打开快速添加面板
#[tauri::command]
pub async fn trigger_quick_add(app: AppHandle) -> Result<(), String> {
    crate::windows::show_widget(&app).map_err(|e| e.to_string())?;
    let _ = app.emit("widget-quick-add", ());
    Ok(())
}

/// 真正退出应用进程
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}
