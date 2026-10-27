//! 窗口管理：悬浮小组件 / 管理窗口的创建、几何持久化、行为同步。
//!
//! 重要约定：窗口创建（含 `.build()`）绝不在主线程事件回调（菜单/托盘）中同步执行，
//! 否则会阻塞 WebView2 controller 初始化（与 sync command 死锁同构）。
//! 所有入口统一走 `spawn_window_op` 投递到后台线程。

use chrono::Local;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::models::AppSettings;

/// 几何保存防抖：窗口停止移动/缩放后再落库，确保"终点"一定被保存。
const GEOMETRY_SAVE_DEBOUNCE_MS: u64 = 500;

/// 悬浮窗收起高度、展开最小高度与默认尺寸（逻辑像素）
const COLLAPSED_HEIGHT: f64 = 64.0;
const EXPANDED_MIN_W: f64 = 300.0;
const EXPANDED_MIN_H: f64 = 200.0;
const DEFAULT_WIDTH: f64 = 340.0;
const DEFAULT_HEIGHT: f64 = 440.0;

/// 收起阈值：高度不大于 64+8 视为收起态。
const COLLAPSED_EPSILON: f64 = 8.0;

/// 初始化悬浮窗事件：移动/缩放停止后防抖保存几何；关闭按钮 → 隐藏或退出。
///
/// 之前使用"前沿节流"（800ms 内只保存第一次事件），会漏掉用户拖拽结束时的
/// 最终位置。现在把事件投递到独立保存线程，静默 500ms 后保存最终几何。
pub fn setup_widget_events(app: &AppHandle) {
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };

    let (geometry_tx, geometry_rx) = std::sync::mpsc::channel::<()>();
    let saver_handle = app.clone();
    std::thread::Builder::new()
        .name("widget-geometry-saver".into())
        .spawn(move || geometry_saver_loop(saver_handle, geometry_rx))
        .ok();

    let handle = app.clone();
    widget.on_window_event(move |event| {
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                let _ = geometry_tx.send(());
            }
            WindowEvent::CloseRequested { api, .. } => {
                // 关闭前同步落一次最终几何，避免 500ms 防抖窗口内的移动/缩放丢失。
                save_widget_geometry(&handle);
                // 读取设置：默认隐藏到托盘继续提醒；"关闭即退出"则退出整个进程
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
                } else {
                    // close_to_tray = false：用户语义是"关闭即退出"。
                    // 直接退出进程（否则 manager 存在时进程会驻留、widget 无法重建）
                    api.prevent_close();
                    handle.exit(0);
                }
            }
            _ => {}
        }
    });
}

/// 独立线程：收到第一个事件后持续吞并后续事件，直到静默 500ms 再保存。
fn geometry_saver_loop(app: AppHandle, rx: std::sync::mpsc::Receiver<()>) {
    loop {
        if rx.recv().is_err() {
            return;
        }
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(GEOMETRY_SAVE_DEBOUNCE_MS)) {
                Ok(()) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    save_widget_geometry(&app);
                    return;
                }
            }
        }
        save_widget_geometry(&app);
    }
}

/// 计算应持久化的物理高度：
/// - 展开态：保存真实高度
/// - 收起态：保留此前保存的展开高度（无有效记录则用默认展开高度）
///
/// 这样数据库永远只保存"正常展开尺寸"，启动时统一按展开态恢复，
/// 不会出现"DB 高度 64 + React collapsed=false"的裁切组合。
fn persisted_height(current_phys_h: f64, scale: f64, existing_phys_h: Option<f64>) -> f64 {
    let logical_h = current_phys_h / scale;
    if logical_h > COLLAPSED_HEIGHT + COLLAPSED_EPSILON {
        return current_phys_h;
    }
    existing_phys_h
        .filter(|h| h / scale > COLLAPSED_HEIGHT + COLLAPSED_EPSILON)
        .unwrap_or(DEFAULT_HEIGHT * scale)
}

/// 恢复时使用的逻辑高度：旧版可能存了 64（收起高度），统一回退到展开高度。
fn restored_logical_height(saved_phys_h: f64, scale: f64) -> f64 {
    let logical_h = saved_phys_h / scale;
    if logical_h <= COLLAPSED_HEIGHT + COLLAPSED_EPSILON {
        DEFAULT_HEIGHT
    } else {
        logical_h.clamp(EXPANDED_MIN_H, 900.0)
    }
}

fn save_widget_geometry(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return;
    };
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    let (Ok(position), Ok(size)) = (widget.outer_position(), widget.outer_size()) else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return; // 最小化/销毁过程中的无效尺寸
    }
    let scale = widget.scale_factor().unwrap_or(1.0);
    let existing_h = state
        .db
        .load_window_geometry("widget")
        .map(|(_, _, _, h)| h);
    let height = persisted_height(size.height as f64, scale, existing_h);
    if let Err(e) = state.db.save_window_geometry(
        "widget",
        position.x as f64,
        position.y as f64,
        size.width as f64,
        height,
    ) {
        log::warn!("保存窗口几何失败: {e}");
    }
}

/// 恢复悬浮窗保存的几何（物理像素）。
/// 多显示器：校验标题栏区域与任一显示器可见区域相交（支持负坐标 / 混合 DPI / 拔屏），
/// 不相交则回退居中。
///
/// 只持久化展开尺寸 + 启动统一展开：若读到旧版 64 收起高度，回退 DEFAULT_HEIGHT。
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

    let scale = widget.scale_factor().unwrap_or(1.0);
    let w_logical = (w / scale).clamp(EXPANDED_MIN_W, 700.0);
    let h_logical = restored_logical_height(h, scale);
    // 先放宽最小尺寸，再恢复尺寸（React 启动即为展开态）。
    let _ = widget.set_min_size(Some(LogicalSize::new(EXPANDED_MIN_W, EXPANDED_MIN_H)));
    let _ = widget.set_size(LogicalSize::new(w_logical, h_logical));

    // 标题栏可见性校验：窗口顶部 52 逻辑像素条必须与某个显示器相交
    let title_h = 52.0 * scale;
    let visible = widget
        .available_monitors()
        .map(|monitors| {
            monitors.iter().any(|m| {
                let mp = m.position();
                let ms = m.size();
                let (ml, mt, mr, mb) = (
                    mp.x as f64,
                    mp.y as f64,
                    (mp.x + ms.width as i32) as f64,
                    (mp.y + ms.height as i32) as f64,
                );
                // 矩形相交：标题栏条 vs 显示器
                x < mr && x + w > ml && y < mb && y + title_h > mt
            })
        })
        .unwrap_or(false);

    if visible {
        let _ = widget.set_position(PhysicalPosition::new(x as i32, y as i32));
    } else {
        log::info!("保存的窗口位置不可见（显示器变更？），回退居中");
        let _ = widget.center();
    }
}

/// 打开（或聚焦）管理窗口。仅可从后台线程调用（勿在主线程事件回调同步调用）。
pub fn open_manager(app: &AppHandle) -> tauri::Result<()> {
    if let Some(existing) = app.get_webview_window("manager") {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, "manager", WebviewUrl::App("index.html".into()))
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

/// 显示悬浮窗（窗口已销毁时尝试按默认配置重建，保证托盘入口始终可用）。
pub fn show_widget(app: &AppHandle) -> tauri::Result<()> {
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = widget.unminimize();
        let _ = widget.show();
        let _ = widget.set_focus();
        return Ok(());
    }
    // widget 被销毁（异常路径）：重建。统一使用展开尺寸；
    // 若存在保存的几何，最后再恢复位置/宽高（不会把 64 收起高度带回来）。
    log::warn!("悬浮窗不存在，尝试重建");
    let settings = app
        .try_state::<crate::commands::CheckItState>()
        .and_then(|s| s.db.load_settings().ok())
        .unwrap_or_default();
    let win = WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("index.html".into()))
        .title("CheckIt")
        .inner_size(DEFAULT_WIDTH, DEFAULT_HEIGHT)
        .min_inner_size(EXPANDED_MIN_W, EXPANDED_MIN_H)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(settings.always_on_top)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .center()
        .build()?;
    setup_widget_events(app);
    restore_widget_geometry(app);
    apply_glass_effect(&win, settings.glass_enabled);
    Ok(())
}

/// 应用窗口行为：置顶、玻璃效果、托盘勾选同步。
///
/// 这些是系统副作用，失败不回滚已提交的设置（下次启动会重新应用），
/// 但必须留下日志，不能静默吞掉错误。
pub fn apply_window_behaviors(app: &AppHandle, settings: &AppSettings) {
    if let Some(widget) = app.get_webview_window("widget") {
        if let Err(e) = widget.set_always_on_top(settings.always_on_top) {
            log::warn!("应用窗口置顶失败（设置已保存，重启后重试）: {e}");
        }
        apply_glass_effect(&widget, settings.glass_enabled);
    }
    if let Some(manager) = app.get_webview_window("manager") {
        apply_glass_effect(&manager, settings.glass_enabled);
    }
    sync_tray_pin(app, settings.always_on_top);
}

/// 在锁内只取出菜单项副本，锁必须在调用任何 Tauri 菜单 API 前释放。
///
/// `CheckMenuItem::set_checked()` 会通过 `run_on_main_thread` 同步等待主线程；
/// 若持有 `TrayPinItem` 锁调用，后台托盘事件与主线程设置命令可能形成 ABBA 死锁。
fn clone_tray_pin_item<T: Clone>(
    slot: &std::sync::Mutex<Option<T>>,
) -> Result<Option<T>, ()> {
    let guard = slot.lock().map_err(|_| ())?;
    Ok((*guard).clone())
}

/// 同步托盘置顶菜单项的勾选状态。
///
/// 注意：锁内只克隆 `CheckMenuItem`，系统调用在锁释放后执行。
pub fn sync_tray_pin(app: &AppHandle, pinned: bool) {
    let Some(pin_state) = app.try_state::<crate::tray::TrayPinItem>() else {
        return;
    };
    let item = match clone_tray_pin_item(&pin_state.0) {
        Ok(item) => item,
        Err(()) => {
            log::warn!("托盘置顶状态锁已中毒，跳过勾选同步");
            return;
        }
    };
    let Some(item) = item else {
        return;
    };
    // 此时 TrayPinItem 锁已释放：set_checked 可以安全等待主线程。
    if let Err(e) = item.set_checked(pinned) {
        log::warn!("同步托盘置顶勾选失败: {e}");
    }
}

/// 切换置顶（唯一修改路径）：
/// 原子翻转设置 → 应用到悬浮窗 → 广播 settings 变更 → 同步托盘勾选。
/// 前端按钮与托盘菜单都调用本函数，保证界面/原生窗口/托盘/数据库四处一致。
///
/// 数据库操作在独立事务内完成，窗口调用在锁外执行；若窗口调用失败，
/// 用同样的原子补丁回滚置顶位，避免"数据库已翻转但窗口没变"的假状态。
pub fn toggle_always_on_top(app: &AppHandle) -> Result<bool, String> {
    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return Err("应用状态未初始化".to_string());
    };
    let updated = state.db.toggle_always_on_top().map_err(|e| e.to_string())?;
    let desired = updated.always_on_top;

    let widget = app.get_webview_window("widget");
    if let Some(widget) = &widget {
        if let Err(e) = widget.set_always_on_top(desired) {
            log::error!("应用置顶失败，回滚设置: {e}");
            let rollback = state
                .db
                .update_settings_atomic(|s| s.always_on_top = !desired)
                .map_err(|re| format!("应用窗口置顶失败：{e}；回滚置顶设置也失败：{re}"))?;
            if let Err(re) = widget.set_always_on_top(rollback.always_on_top) {
                log::warn!("回滚窗口置顶失败: {re}");
            }
            sync_tray_pin(app, rollback.always_on_top);
            use tauri::Emitter as _;
            let _ = app.emit("data-changed", serde_json::json!({ "entity": "settings" }));
            return Err(format!("应用窗口置顶失败：{e}"));
        }
    }

    use tauri::Emitter as _;
    let _ = app.emit("data-changed", serde_json::json!({ "entity": "settings" }));
    sync_tray_pin(app, desired);
    Ok(desired)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_height_keeps_last_expanded_height() {
        // 收起态：保存原有展开高度（440 * 1.25 = 550 物理像素）
        assert_eq!(persisted_height(64.0, 1.0, Some(440.0)), 440.0);
        assert_eq!(persisted_height(80.0, 1.25, Some(550.0)), 550.0);
    }

    #[test]
    fn collapsed_without_expanded_record_falls_back_to_default() {
        assert_eq!(persisted_height(64.0, 1.0, None), DEFAULT_HEIGHT);
        assert_eq!(
            persisted_height(80.0, 1.25, Some(80.0)),
            DEFAULT_HEIGHT * 1.25
        );
    }

    #[test]
    fn expanded_height_is_saved_verbatim() {
        assert_eq!(persisted_height(512.0, 1.0, Some(440.0)), 512.0);
        assert_eq!(persisted_height(640.0, 1.25, Some(500.0)), 640.0);
        // 边界：刚好超过收起阈值即视为展开
        assert_eq!(persisted_height(72.5, 1.0, Some(440.0)), 72.5);
    }

    #[test]
    fn legacy_collapsed_geometry_restores_expanded() {
        assert_eq!(restored_logical_height(64.0, 1.0), DEFAULT_HEIGHT);
        assert_eq!(restored_logical_height(72.0, 1.0), DEFAULT_HEIGHT);
        assert_eq!(restored_logical_height(80.0, 1.25), DEFAULT_HEIGHT);
    }

    #[test]
    fn restored_height_clamps_into_expanded_range() {
        assert_eq!(restored_logical_height(440.0, 1.0), 440.0);
        assert_eq!(restored_logical_height(150.0, 1.0), EXPANDED_MIN_H);
        assert_eq!(restored_logical_height(5000.0, 1.0), 900.0);
        // 高 DPI：物理 550 → 逻辑 440
        assert_eq!(restored_logical_height(550.0, 1.25), 440.0);
    }

    /// 模拟“主线程菜单更新”需要获取同一把锁的可控交错：
    /// 后台必须先克隆出菜单项并释放锁，模拟主线程才能拿到锁继续执行。
    #[derive(Clone)]
    struct LockProbe {
        slot: std::sync::Weak<std::sync::Mutex<Option<LockProbe>>>,
    }

    impl LockProbe {
        fn apply_on_simulated_main_thread(&self) -> bool {
            let Some(slot) = self.slot.upgrade() else {
                return false;
            };
            // try_lock 避免测试自身死等；若生产代码仍持锁，这里返回 false。
            let acquired = slot.try_lock().is_ok();
            acquired
        }
    }

    #[test]
    fn tray_pin_item_is_cloned_before_calling_menu_api() {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None::<LockProbe>));
        let probe = LockProbe {
            slot: std::sync::Arc::downgrade(&slot),
        };
        *slot.lock().unwrap() = Some(probe);

        let item = clone_tray_pin_item(&slot)
            .expect("锁未中毒")
            .expect("菜单项存在");
        let simulated_main_thread = std::thread::spawn(move || {
            item.apply_on_simulated_main_thread()
        });
        assert!(
            simulated_main_thread.join().unwrap(),
            "克隆菜单项后必须已释放锁，否则 set_checked 会等待主线程死锁"
        );
    }
}
