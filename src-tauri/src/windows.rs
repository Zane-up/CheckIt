//! 窗口管理：悬浮小组件 / 管理窗口的创建、几何持久化、行为同步。

use chrono::Local;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::models::AppSettings;

/// 悬浮窗几何节流保存的最小间隔
const GEOMETRY_SAVE_INTERVAL_MS: u128 = 800;

/// 悬浮窗几何监听器的内部状态（节流）
struct GeometryThrottle {
    last_save: std::time::Instant,
}

/// 初始化悬浮窗事件：拖动/缩放后节流保存几何；关闭按钮 → 隐藏或退出
pub fn setup_widget_events(app: &AppHandle) {
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };

    let throttle = std::sync::Mutex::new(GeometryThrottle {
        last_save: std::time::Instant::now() - std::time::Duration::from_secs(10),
    });

    let handle = app.clone();
    widget.on_window_event(move |event| {
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                save_geometry_throttled(&handle, &throttle);
            }
            WindowEvent::CloseRequested { api, .. } => {
                // 关闭按钮：默认隐藏到托盘继续提醒；设置允许真正退出
                let close_to_tray = handle
                    .try_state::<crate::commands::CheckItState>()
                    .and_then(|s| s.db.load_settings().ok())
                    .map(|s: AppSettings| s.close_to_tray)
                    .unwrap_or(true);
                if close_to_tray {
                    api.prevent_close();
                    if let Some(w) = handle.get_webview_window("widget") {
                        let _ = w.hide();
                    }
                }
                // close_to_tray = false 时放行默认关闭行为（窗口销毁；
                // 由于主进程在托盘常驻，应用退出必须显式走 quit_app）
            }
            _ => {}
        }
    });
}

fn save_geometry_throttled(
    app: &AppHandle,
    throttle: &std::sync::Mutex<GeometryThrottle>,
) {
    let mut guard = match throttle.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.last_save.elapsed().as_millis() < GEOMETRY_SAVE_INTERVAL_MS {
        return;
    }
    guard.last_save = std::time::Instant::now();

    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return;
    };
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    // 物理像素 → 逻辑像素（跨 DPI 保存一致坐标）
    let scale = widget.scale_factor().unwrap_or(1.0);
    if let (Ok(position), Ok(size)) = (widget.outer_position(), widget.inner_size()) {
        let _ = state.db.save_window_geometry(
            "widget",
            position.x as f64 / scale,
            position.y as f64 / scale,
            size.width as f64 / scale,
            size.height as f64 / scale,
        );
    }
}

/// 恢复悬浮窗保存的几何（多显示器环境下钳制到可见区域）
pub fn restore_widget_geometry(app: &AppHandle) {
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return;
    };
    let Some((x, y, w, h)) = state.db.load_window_geometry("widget") else {
        return;
    };

    // 钳制尺寸
    let w = w.clamp(300.0, 600.0);
    let h = h.clamp(56.0, 800.0);
    let _ = widget.set_size(LogicalSize::new(w, h));

    // 位置钳制：确保窗口至少有 80px 落在某个显示器可见区域内
    let mut valid = false;
    if let Ok(Some(m)) = widget.current_monitor() {
        let m_pos = m.position();
        let m_size = m.size();
        let px = m_pos.x as f64;
        let py = m_pos.y as f64;
        let pw = m_size.width as f64;
        let ph = m_size.height as f64;
        valid = x >= px - w + 80.0
            && y >= py - h + 80.0
            && x <= px + pw - 80.0
            && y <= py + ph - 80.0;
    }
    if valid {
        let _ = widget.set_position(LogicalPosition::new(x, y));
    } else {
        // 位置失效（显示器拔出等）：居中显示
        let _ = widget.center();
    }
}

/// 打开（或聚焦）管理窗口
pub fn open_manager(app: &AppHandle) -> tauri::Result<()> {
    if let Some(existing) = app.get_webview_window("manager") {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(
        app,
        "manager",
        WebviewUrl::App("index.html".into()),
    )
    .title("CheckIt - 任务管理")
    .inner_size(880.0, 620.0)
    .min_inner_size(760.0, 520.0)
    .resizable(true)
    .decorations(true)
    .center()
    .build()?;

    // 管理窗口也可应用玻璃效果（Mica / Acrylic）
    apply_glass_effect(&win, true);
    Ok(())
}

/// 显示悬浮窗
pub fn show_widget(app: &AppHandle) -> tauri::Result<()> {
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = widget.unminimize();
        let _ = widget.show();
        let _ = widget.set_focus();
    }
    Ok(())
}

/// 应用窗口行为：置顶、玻璃效果
pub fn apply_window_behaviors(app: &AppHandle, settings: &AppSettings) {
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = widget.set_always_on_top(settings.always_on_top);
        apply_glass_effect(&widget, settings.glass_enabled);
    }
    if let Some(manager) = app.get_webview_window("manager") {
        apply_glass_effect(&manager, settings.glass_enabled);
    }
}

/// 尝试启用系统原生玻璃效果（Windows Acrylic），失败则降级为 CSS 半透明。
pub fn apply_glass_effect(window: &tauri::WebviewWindow, enabled: bool) {
    #[cfg(target_os = "windows")]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Color, Effect, EffectState};
        if enabled {
            let config = WindowEffectsConfig {
                effects: vec![Effect::Acrylic],
                state: Some(EffectState::Active),
                radius: None,
                color: Some(Color(0, 0, 0, 0)),
                interactive: false,
            };
            if let Err(e) = window.set_effects(config) {
                log::warn!("原生玻璃效果不可用，使用 CSS 半透明降级: {e}");
            }
        } else {
            let cleared = WindowEffectsConfig {
                effects: vec![],
                state: None,
                radius: None,
                color: Some(Color(0, 0, 0, 0)),
                interactive: false,
            };
            if let Err(e) = window.set_effects(cleared) {
                log::warn!("清除窗口效果失败: {e}");
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, enabled);
    }
}

/// 当前本地时间描述（托盘提示等场景备用）
pub fn local_date_text() -> String {
    Local::now().format("%m月%d日 %H:%M").to_string()
}
