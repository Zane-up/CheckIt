//! Tauri 命令层：前端 invoke 的全部入口。
//! 每个写操作完成后广播 data-changed 事件，驱动所有窗口同步刷新，
//! 并唤醒提醒调度器立即重扫（加速编辑/删除任务后的队列更新）。

use std::sync::mpsc::Sender;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::database::{Database, DbError};
use crate::models::{AppSettings, Category, SettingsPatch, Task, TaskInput};

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
    // 开机启动是系统副作用：在写库前执行。失败则不落库，前端可准确回滚开关。
    // 补丁本身由数据库层在同一把锁/事务内读取→应用→保存，绝不会覆盖其他字段。
    if let Some(enable) = patch.launch_at_login {
        crate::settings::apply_autostart(&app, enable)
            .map_err(|e| format!("开机启动设置失败：{e}"))?;
    }

    let updated = state.db.apply_settings_patch(&patch).map_err(to_invoke_error)?;

    // 系统窗口/托盘调用位于数据库提交之后；失败只记录，不回滚已提交的设置，
    // 避免出现"数据库成功但前端收到失败并回滚"的假失败。下次应用启动会重新应用。
    crate::windows::apply_window_behaviors(&app, &updated);

    broadcast(&app, "settings");
    Ok(updated)
}

/// 切换悬浮窗置顶（唯一路径：原子设置翻转 + 窗口 + 托盘勾选 + 广播）
#[tauri::command]
pub fn toggle_always_on_top(app: AppHandle) -> Result<bool, String> {
    crate::windows::toggle_always_on_top(&app)
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
    // 窗口显示/重建与事件发送都放到阻塞线程：命令 async 上下文不承担 WebView2 初始化。
    tauri::async_runtime::spawn_blocking(move || {
        crate::windows::show_widget(&app).map_err(|e| e.to_string())?;
        let _ = app.emit("widget-quick-add", ());
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 悬浮窗切换到完整面板模式。
/// `min_height`：快速添加所需最小逻辑高度（临时抬高，不写回正常几何）。
#[tauri::command]
pub async fn widget_expand(app: AppHandle, min_height: Option<f64>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let widget = app
            .get_webview_window("widget")
            .ok_or_else(|| "悬浮窗不存在".to_string())?;
        crate::windows::expand_widget_inner(&app, &widget, min_height)
            .map(|m| m.as_str().to_string())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 悬浮窗收起为标题栏模式。
#[tauri::command]
pub async fn widget_collapse(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let widget = app
            .get_webview_window("widget")
            .ok_or_else(|| "悬浮窗不存在".to_string())?;
        crate::windows::collapse_widget_inner(&app, &widget)
            .map(|m| m.as_str().to_string())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 快速添加面板关闭：清除临时高度并按需恢复（详见 windows::quick_add_closed_inner）。
#[tauri::command]
pub async fn widget_quick_add_closed(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let widget = app
            .get_webview_window("widget")
            .ok_or_else(|| "悬浮窗不存在".to_string())?;
        crate::windows::quick_add_closed_inner(&app, &widget)
            .map(|mode| mode.as_str().to_string())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 查询悬浮窗当前模式（expanded / collapsed / docked），前端启动时同步界面。
#[tauri::command]
pub fn widget_get_mode(app: AppHandle) -> Result<String, String> {
    Ok(crate::windows::current_mode(&app).as_str().to_string())
}

/// 真正退出应用进程
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}
