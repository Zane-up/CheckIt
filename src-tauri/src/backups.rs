use crate::commands::CheckItState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups"))
}

#[derive(Serialize)]
pub struct BackupFile {
    name: String,
    bytes: u64,
}

#[tauri::command]
pub async fn create_backup(app: AppHandle) -> Result<String, String> {
    let path = app
        .state::<CheckItState>()
        .db
        .create_backup(&directory(&app)?, "manual")
        .map_err(|e| e.to_string())?;
    Ok(path.file_name().unwrap().to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn list_backups(app: AppHandle) -> Result<Vec<BackupFile>, String> {
    let folder = directory(&app)?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut files = vec![];
    for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map_err(|e| e.to_string())?.is_file()
            && name.starts_with("checkit-")
            && name.ends_with(".db")
        {
            files.push(BackupFile {
                name,
                bytes: entry.metadata().map_err(|e| e.to_string())?.len(),
            });
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(files)
}

#[tauri::command]
pub async fn restore_backup(app: AppHandle, name: String) -> Result<(), String> {
    // 前端仅能选择本机备份列表中的文件名，不能读取任意路径。
    if !name.starts_with("checkit-")
        || !name.ends_with(".db")
        || name.contains(['/', '\\', ':'])
        || name.contains("..")
    {
        return Err("无效的备份名称".into());
    }
    let folder = directory(&app)?.canonicalize().map_err(|e| e.to_string())?;
    let source = folder
        .join(&name)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if source.parent() != Some(folder.as_path()) {
        return Err("备份必须位于本机备份目录".into());
    }
    let state = app.state::<CheckItState>();
    let _guard = state.maintenance.lock();
    state
        .db
        .restore_backup(&source, &folder)
        .map_err(|e| e.to_string())?;
    // 恢复后立即重启，确保系统自启对账、窗口行为、调度器都使用恢复后的状态。
    // 不向前端报告恢复失败后继续运行；数据库恢复成功即由新进程接管。
    app.restart();
}

#[tauri::command]
pub async fn open_backup_folder(app: AppHandle) -> Result<(), String> {
    let folder = directory(&app)?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    crate::updates::open_target(&folder.to_string_lossy())
}
