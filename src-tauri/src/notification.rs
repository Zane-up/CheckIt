//! 系统通知提交适配层。
//!
//! 背景（已核对锁定版本源码）：
//! - `tauri-plugin-notification 2.7.0` 的 Windows 桌面实现把
//!   `notify_rust::Notification::show()` 放进 `tauri::async_runtime::spawn`，
//!   随后 `let _ = notification.show();` **丢弃底层 WinRT 提交错误**。
//!   因此插件外层 `show()` 返回 `Ok` 只代表"参数准备完成、发送任务已入队"，
//!   不能证明操作系统受理了 Toast。
//! - 本适配层复用同一底层库 `notify-rust`，但**同步调用** WinRT Toast 的
//!   `Show()`，把提交结果（成功或错误）返回给提醒状态机。
//!
//! 语义边界：
//! - `submit() == Ok`：Windows Toast API 已受理（submitted）。仍不保证用户看到横幅，
//!   也不保证"专注助手/通知权限/勿扰模式"允许展示。
//! - `submit() == Err`：底层提交失败，可进入 failed/retry 状态机。
//! - 调用发生在后台调度线程，不阻塞 UI 线程。

use std::path::Path;

/// 通知文本（标题 + 正文），与 UI 框架无关，便于测试注入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationContent {
    pub title: String,
    pub body: String,
}

/// 通知提交接口。测试可注入失败的假实现，观察状态机是否进入重试。
pub trait NotificationTransport: Send + Sync {
    fn submit(&self, content: &NotificationContent) -> Result<(), String>;
}

/// 真实系统通知发送器：使用 `notify-rust` 同步执行 WinRT Toast 提交。
pub struct SystemNotificationTransport {
    /// 仅安装版设置 AppUserModelID；开发版沿用库默认（PowerShell AUMID），
    /// 与 tauri-plugin-notification 的行为保持一致。
    app_id: Option<String>,
}

impl SystemNotificationTransport {
    pub fn new(app_id: Option<String>) -> Self {
        Self { app_id }
    }

    /// 根据可执行文件位置判断是否为开发构建（target/debug|release）。
    fn is_dev_binary(exe: &Path) -> bool {
        let Some(parent) = exe.parent() else {
            return false;
        };
        let run_dir = parent.file_name().and_then(|s| s.to_str());
        if run_dir != Some("debug") && run_dir != Some("release") {
            return false;
        }
        parent
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some("target")
    }

    /// 从应用标识构造发送器。安装版使用产品 identifier 作为 AUMID。
    pub fn from_identifier(identifier: &str) -> Self {
        let app_id = std::env::current_exe()
            .ok()
            .filter(|exe| !Self::is_dev_binary(exe))
            .map(|_| identifier.to_string());
        Self::new(app_id)
    }
}

impl NotificationTransport for SystemNotificationTransport {
    fn submit(&self, content: &NotificationContent) -> Result<(), String> {
        let mut toast = notify_rust::Notification::new();
        toast
            .summary(&content.title)
            .body(&content.body)
            .auto_icon();
        #[cfg(target_os = "windows")]
        if let Some(app_id) = &self.app_id {
            toast.app_id(app_id);
        }
        // 关键：同步调用并保留 WinRT/DBus 的真实提交结果。
        // 失败必须返回给调用方，不能像插件后端那样 `let _ =` 丢弃。
        toast
            .show()
            .map(|_| ())
            .map_err(|e| format!("系统通知提交失败: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_binary_detection() {
        assert!(SystemNotificationTransport::is_dev_binary(Path::new(
            r"C:\work\CheckIt\src-tauri\target\debug\checkit.exe"
        )));
        assert!(SystemNotificationTransport::is_dev_binary(Path::new(
            "/work/checkit/src-tauri/target/release/checkit"
        )));
        assert!(!SystemNotificationTransport::is_dev_binary(Path::new(
            r"C:\Users\me\AppData\Local\CheckIt\checkit.exe"
        )));
    }
}
