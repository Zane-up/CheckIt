//! 设置辅助：开机启动同步。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt as _;

use crate::models::AppSettings;

/// 将 launch_at_login 设置同步到系统注册表（仅在用户显式修改时调用，绝不默认开启）。
pub fn sync_autostart(app: &AppHandle, settings: &AppSettings) {
    let autostart = app.autolaunch();
    let enabled = autostart.is_enabled().unwrap_or(false);
    if settings.launch_at_login && !enabled {
        if let Err(e) = autostart.enable() {
            log::warn!("开启开机启动失败: {e}");
        }
    } else if !settings.launch_at_login && enabled {
        if let Err(e) = autostart.disable() {
            log::warn!("关闭开机启动失败: {e}");
        }
    }
}
