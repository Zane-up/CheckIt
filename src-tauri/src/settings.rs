//! 设置辅助：开机启动同步。
//!
//! 语义：
//! - apply_autostart：执行系统注册表操作并回读验证，失败返回 Err（调用方不落库）
//! - reconcile_autostart_on_startup：启动时对账——以系统实际状态为准修正本地设置，
//!   绝不未经用户同意自动开启（系统已关而设置记录为开时，把本地改回关闭）

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt as _;

use crate::models::AppSettings;

/// 执行开机启动系统操作；成功后回读系统状态验证一致，否则视为失败。
pub fn apply_autostart(app: &AppHandle, enable: bool) -> Result<(), String> {
    let autostart = app.autolaunch();
    if enable {
        autostart.enable().map_err(|e| format!("写入注册表失败: {e}"))?;
    } else {
        // 关闭时注册表项可能已被用户手动删除，视为已达成目标
        let _ = autostart.disable();
    }
    // 回读验证
    let actual = autostart.is_enabled().map_err(|e| format!("回读状态失败: {e}"))?;
    if actual != enable {
        return Err(format!("系统状态回读不一致（期望 {enable}，实际 {actual}）"));
    }
    Ok(())
}

/// 启动时对账：以系统实际状态修正本地设置记录（不执行任何系统写入）。
pub fn reconcile_autostart_on_startup(
    app: &AppHandle,
    db: &crate::database::Database,
) -> Result<AppSettings, crate::database::DbError> {
    let mut settings = db.load_settings()?;
    let system_on = app.autolaunch().is_enabled().unwrap_or(false);
    if settings.launch_at_login != system_on {
        log::info!(
            "开机启动状态对账：设置记录 {} → 系统实际 {}",
            settings.launch_at_login,
            system_on
        );
        settings.launch_at_login = system_on;
        db.save_settings(&settings)?;
    }
    Ok(settings)
}
