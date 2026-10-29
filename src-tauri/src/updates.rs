/// 只允许打开固定官方下载页，不接受来自 WebView 的任意 URL 或命令。
#[tauri::command]
pub async fn open_release_page() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let url: Vec<u16> = "https://github.com/Zane-up/CheckIt/releases/latest\0"
            .encode_utf16()
            .collect();
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err("无法打开官方下载页，请检查默认浏览器设置。".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    Err("请手动访问 https://github.com/Zane-up/CheckIt/releases/latest".into())
}
