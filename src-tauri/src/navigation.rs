use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "view", rename_all = "snake_case")]
pub enum ManagerTarget {
    Settings,
    Task { task_id: i64 },
}

#[derive(Clone, Debug, Serialize)]
pub struct NavigationRequest {
    id: u64,
    target: ManagerTarget,
}

#[derive(Default)]
pub struct ManagerNavigation(Mutex<(u64, Option<NavigationRequest>)>);

pub fn open_target(app: &AppHandle, target: ManagerTarget) -> Result<(), String> {
    if let ManagerTarget::Task { task_id } = &target {
        app.state::<crate::commands::CheckItState>()
            .db
            .get_task(*task_id)
            .map_err(|e| e.to_string())?;
    }
    // 请求先入缓存，新 WebView 注册监听之前的请求也能在首次读取时取得。
    let navigation = app.state::<ManagerNavigation>();
    {
        let mut value = navigation.0.lock();
        value.0 += 1;
        value.1 = Some(NavigationRequest {
            id: value.0,
            target,
        });
    }
    crate::windows::open_manager(app).map_err(|e| e.to_string())?;
    let _ = app.emit_to("manager", "manager-navigation-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn open_manager_target(app: AppHandle, target: ManagerTarget) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_target(&app, target))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_manager_navigation(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<NavigationRequest>, String> {
    if window.label() != "manager" {
        return Err("Only the manager window can receive navigation requests".into());
    }
    Ok(app.state::<ManagerNavigation>().0.lock().1.clone())
}

#[tauri::command]
pub fn acknowledge_manager_navigation(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), String> {
    if window.label() != "manager" {
        return Err("Only the manager window can acknowledge navigation".into());
    }
    let navigation = app.state::<ManagerNavigation>();
    let mut value = navigation.0.lock();
    if value.1.as_ref().is_some_and(|request| request.id == id) {
        value.1 = None;
    }
    Ok(())
}
