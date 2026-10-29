//! 窗口管理：悬浮小组件 / 管理窗口的创建、几何持久化、行为同步。
//!
//! 重要约定：窗口创建（含 `.build()`）绝不在主线程事件回调（菜单/托盘）中同步执行，
//! 否则会阻塞 WebView2 controller 初始化（与 sync command 死锁同构）。
//! 所有入口统一投递到后台线程。
//!
//! ## 窗口模式与几何权威状态（2026-10-27 重构）
//!
//! 悬浮窗有三种模式：完整面板（Expanded）/ 标题栏收起（Collapsed）/ 贴边圆圈（Docked），
//! 外加一个仅在 Expanded 模式内生效的"快速添加临时高度"标记。
//! 权威状态保存在 [`WidgetGeometry`]（Rust 侧托管状态），不再依赖前端 ref：
//!
//! - `expanded`：最后一次**正常展开**的位置与尺寸（物理像素），是唯一写回数据库
//!   `widget_px_*` 的几何；64（收起）、40（圆圈）、快速添加临时抬高（≥320）都不覆盖它。
//! - `temp_height`：快速添加临时抬高的逻辑高度；生效期间跳过几何保存与吸附判定。
//! - `quick_add_open`：编辑期间禁止自动收纳，不依赖是否需要临时抬高。
//! - `dock`：圆圈所在边缘与锚点（物理像素），持久化为 `widget_mode`/`widget_dock_*`。
//!
//! Windows 原生移动会话由 native_drag 的窗口子类回调采样，WM_EXITSIZEMOVE 后
//! 才把完整起终点投递给后台线程；程序化移动没有原生会话，不触发吸附。

use chrono::Local;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::dock::{
    clamp_into_work_area, dock_anchor, expanded_position_from_dock, expanded_rect_from_collapsed,
    intersects, nearest_edge, select_edge, should_persist_expanded, Edge, Rect, WidgetGeometryState, WidgetMode,
    CLICK_DISPLACEMENT, COLLAPSED_HEIGHT, DOCK_BUBBLE_SIZE, EDGE_DOCK_THRESHOLD,
};
use crate::models::AppSettings;
use crate::native_drag::{CompletedDrag, DragEvent};
use crate::window_transition::{transition, Layout, Step, WindowOps};

/// 几何保存防抖：窗口停止移动/缩放后再落库，确保"终点"一定被保存。
const GEOMETRY_SAVE_DEBOUNCE_MS: u64 = 500;

/// 悬浮窗展开最小尺寸与默认尺寸（逻辑像素）
const EXPANDED_MIN_W: f64 = 300.0;
const EXPANDED_MIN_H: f64 = 200.0;
const DEFAULT_WIDTH: f64 = 340.0;
const DEFAULT_HEIGHT: f64 = 440.0;
/// 快速添加临时高度上限；正常收纳恢复不使用此上限。
const EXPANDED_MAX_H: f64 = 900.0;

/// 持久化键（app_settings 表）
const KEY_MODE: &str = "widget_mode";
const KEY_DOCK_EDGE: &str = "widget_dock_edge";
const KEY_DOCK_X: &str = "widget_dock_x";
const KEY_DOCK_Y: &str = "widget_dock_y";

/// 悬浮窗几何权威状态（托管于 Tauri State）。
pub struct WidgetGeometry(pub Mutex<WidgetGeometryState>, Mutex<()>, AtomicBool);

struct NativeWindow<'a>(&'a WebviewWindow);

impl WindowOps for NativeWindow<'_> {
    fn apply(&self, step: Step) -> Result<(), String> {
        match step {
            Step::Min(size) => self
                .0
                .set_min_size(size.map(|(w, h)| LogicalSize::new(w, h))),
            Step::Resizable(value) => self.0.set_resizable(value),
            Step::Size((w, h)) => self.0.set_size(LogicalSize::new(w, h)),
            Step::Position((x, y)) => self.0.set_position(PhysicalPosition::new(x, y)),
        }
        .map_err(|e| e.to_string())
    }
    fn verify(&self, layout: Layout) -> Result<(), String> {
        // setters 可能只把操作投递到事件循环；随后查询，核对系统实际应用的结果。
        let scale = self.0.scale_factor().map_err(|e| e.to_string())?;
        let size = self.0.outer_size().map_err(|e| e.to_string())?;
        let position = self.0.outer_position().map_err(|e| e.to_string())?;
        let resizable = self.0.is_resizable().map_err(|e| e.to_string())?;
        if (size.width as f64 - layout.size.0 * scale).abs() > 2.0
            || (size.height as f64 - layout.size.1 * scale).abs() > 2.0
            || (position.x as f64 - layout.position.0 as f64).abs() > 2.0
            || (position.y as f64 - layout.position.1 as f64).abs() > 2.0
            || resizable != layout.resizable
        {
            return Err(format!(
                "窗口实际几何与目标不符: size={size:?}, position={position:?}, target={layout:?}"
            ));
        }
        Ok(())
    }
}

fn window_error(message: impl Into<String>) -> tauri::Error {
    tauri::Error::Io(std::io::Error::other(message.into()))
}

fn mode_min(mode: WidgetMode) -> (f64, f64) {
    match mode {
        WidgetMode::Docked => (DOCK_BUBBLE_SIZE, DOCK_BUBBLE_SIZE),
        WidgetMode::Collapsed => (EXPANDED_MIN_W, COLLAPSED_HEIGHT),
        WidgetMode::Expanded => (EXPANDED_MIN_W, EXPANDED_MIN_H),
    }
}

/// 调用时持有 geom.1；落库为单个 SQLite 事务，成功后才发布内存状态和事件。
fn commit_layout(
    app: &AppHandle,
    widget: &WebviewWindow,
    geom: &WidgetGeometry,
    next: WidgetGeometryState,
    target: Layout,
) -> tauri::Result<WidgetMode> {
    let old = geom.0.lock().clone();
    let scale = widget.scale_factor()?;
    let size = widget.outer_size()?;
    let position = widget.outer_position()?;
    let before = Layout {
        min: mode_min(old.mode),
        size: (size.width as f64 / scale, size.height as f64 / scale),
        position: (position.x, position.y),
        resizable: widget.is_resizable()?,
    };
    let result = transition(&NativeWindow(widget), before, target, || {
        let state = app
            .try_state::<crate::commands::CheckItState>()
            .ok_or_else(|| "数据库状态尚未初始化".to_string())?;
        let mut entries = vec![(KEY_MODE.to_string(), next.mode.as_str().to_string())];
        if let Some(rect) = next.expanded {
            entries.extend([
                ("widget_px_x".into(), rect.x.to_string()),
                ("widget_px_y".into(), rect.y.to_string()),
                ("widget_px_w".into(), rect.w.to_string()),
                ("widget_px_h".into(), rect.h.to_string()),
            ]);
        }
        if let Some((edge, x, y)) = next.dock {
            entries.extend([
                (KEY_DOCK_EDGE.into(), edge.as_str().into()),
                (KEY_DOCK_X.into(), x.to_string()),
                (KEY_DOCK_Y.into(), y.to_string()),
            ]);
        }
        state
            .db
            .set_setting_entries(&entries)
            .map_err(|e| e.to_string())?;
        *geom.0.lock() = next.clone();
        Ok(())
    });
    if let Err(error) = result {
        if error.rollback_failed {
            // 不能让防抖保存器把回滚失败留下的 40/64/临时尺寸覆盖正常几何。
            geom.2.store(true, Ordering::Relaxed);
        }
        return Err(window_error(error.to_string()));
    }
    geom.2.store(false, Ordering::Relaxed);
    // Acrylic 会覆盖整个矩形 HWND；圆球必须清除底层效果，才能露出真正透明的四角。
    // 返回面板时再按用户设置恢复，重贴边不重复切换效果。
    if (old.mode == WidgetMode::Docked) != (next.mode == WidgetMode::Docked) {
        let enabled = app
            .try_state::<crate::commands::CheckItState>()
            .and_then(|s| s.db.load_settings().ok())
            .map(|s| s.glass_enabled)
            .unwrap_or(true);
        apply_glass_effect(widget, enabled);
    }
    emit_mode(app, next.mode);
    Ok(next.mode)
}

fn widget_rect(widget: &WebviewWindow) -> Option<Rect> {
    let (Ok(pos), Ok(size)) = (widget.outer_position(), widget.outer_size()) else {
        return None;
    };
    Some(Rect::new(
        pos.x as f64,
        pos.y as f64,
        size.width as f64,
        size.height as f64,
    ))
}

/// 窗口中心所在的显示器工作区（物理像素）与该显示器 scale。
/// 中心不在任何显示器内时取距离最近的（拔屏 / 跨屏拖动途中）。
fn work_area_for(widget: &WebviewWindow, center: (f64, f64)) -> Option<(Rect, f64)> {
    let monitors = widget.available_monitors().ok()?;
    let mut best: Option<(f64, Rect, f64)> = None;
    for m in monitors {
        let mp = m.position();
        let ms = m.size();
        let bounds = Rect::new(mp.x as f64, mp.y as f64, ms.width as f64, ms.height as f64);
        let (mx, my) = bounds.center();
        let dist = (mx - center.0).hypot(my - center.1);
        let wa = m.work_area();
        let wa_rect = Rect::new(
            wa.position.x as f64,
            wa.position.y as f64,
            wa.size.width as f64,
            wa.size.height as f64,
        );
        let probe = Rect::new(center.0 - 1.0, center.1 - 1.0, 2.0, 2.0);
        let contained = intersects(&probe, &bounds);
        let score = if contained { -1.0 } else { dist };
        if best.map(|(s, _, _)| score < s).unwrap_or(true) {
            best = Some((score, wa_rect, m.scale_factor()));
        }
    }
    best.map(|(_, wa, scale)| (wa, scale))
}

fn emit_mode(app: &AppHandle, mode: WidgetMode) {
    use tauri::Emitter as _;
    let _ = app.emit("widget-mode", serde_json::json!({ "mode": mode.as_str() }));
}

/// 初始化悬浮窗事件：
/// - Moved/Resized → 防抖保存几何（按模式分流，临时尺寸永不写回正常几何）
/// - 原生 WM_EXITSIZEMOVE → 拖动观察线程（判定贴边 / 展开）
/// - ScaleChanged → 圆圈重新贴边（投递后台线程执行）
/// - 关闭按钮 → 落盘最终几何，隐藏到托盘或退出
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

    let (drag_tx, drag_rx) = std::sync::mpsc::channel::<DragEvent>();
    let drag_handle = app.clone();
    std::thread::Builder::new()
        .name("widget-drag-watcher".into())
        .spawn(move || drag_watcher_loop(drag_handle, drag_rx))
        .ok();

    if let Err(e) = crate::native_drag::setup(&widget, drag_tx.clone()) {
        log::error!("启用原生拖动监听失败: {e}");
    }
    let handle = app.clone();
    widget.on_window_event(move |event| {
        match event {
            WindowEvent::Moved(_pos) => {
                let _ = geometry_tx.send(());
            }
            WindowEvent::Resized(_) => {
                let _ = geometry_tx.send(());
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let _ = drag_tx.send(DragEvent::Resnap);
            }
            WindowEvent::CloseRequested { api, .. } => {
                // 关闭前同步落一次最终几何（按模式分流），避免防抖窗口内的移动/缩放丢失。
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

/// 原生回调已完成采样：此线程才允许查询 Tauri 窗口及进行尺寸/数据库操作。
fn drag_watcher_loop(app: AppHandle, rx: std::sync::mpsc::Receiver<DragEvent>) {
    for event in rx {
        match event {
            DragEvent::Finished(drag) => {
                log::info!("原生拖动结束: start={:?}, end={:?}", drag.start, drag.end);
                handle_drag_end(&app, drag);
            }
            DragEvent::Resnap => resnap_dock(&app),
        }
    }
}

/// 本次拖动位移（物理像素）：鼠标净位移与窗口净位移取较大者。
///
/// 只用"按下→松开"的净位移，不受跨屏 DPI 变化导致的窗口整体缩放影响。
fn drag_move_distance(
    start: Rect,
    cur: Rect,
    start_cursor: Option<(f64, f64)>,
    end_cursor: Option<(f64, f64)>,
) -> f64 {
    let mouse = match (start_cursor, end_cursor) {
        (Some(a), Some(b)) => (a.0 - b.0).abs().max((a.1 - b.1).abs()),
        _ => 0.0,
    };
    let win = (start.x - cur.x).abs().max((start.y - cur.y).abs());
    mouse.max(win)
}

/// 拖动结束后的统一判定：
/// - 完整面板：纯移动且贴近工作区边缘（≤12 逻辑像素）→ 收纳成圆圈；
///   缩放会话、快速添加临时高度期间不吸附。
/// - 圆圈：按下位移 ≤4 逻辑像素视为单击（前端处理展开，此处忽略）；
///   靠近任一边缘 → 重新贴边；离开边缘 → 恢复完整面板。
fn handle_drag_end(app: &AppHandle, drag: CompletedDrag) {
    let start = drag.start;
    let cur = drag.end;
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    let Some(state) = app.try_state::<WidgetGeometry>() else {
        return;
    };
    let _transition = state.1.lock();
    if state.2.load(Ordering::Relaxed) {
        return; // 回滚失败后只允许显式展开恢复，不能采纳当前异常小尺寸。
    }
    // 只在读取瞬间持锁：后续 set_size / set_position 会再写同一把锁
    let Some(st) = state.0.try_lock().map(|g| g.clone()) else {
        log::warn!("拖动结束判定跳过：几何状态锁被占用");
        return;
    };
    // 若后一轮拖动已改变窗口位置，丢弃旧终点，等待队列里对应的新会话。
    if widget_rect(&widget)
        .map(|actual| (actual.x - cur.x).abs() > 2.0 || (actual.y - cur.y).abs() > 2.0)
        .unwrap_or(true)
    {
        log::info!("忽略已过时的原生拖动终点");
        return;
    }
    let scale = widget.scale_factor().unwrap_or(1.0);

    // 本次拖动位移（物理像素）：鼠标净位移与窗口净位移取较大者。
    // 只用"按下→松开"的位移，不受跨屏 DPI 变化导致的窗口整体缩放影响。
    let moved = drag_move_distance(start, cur, drag.start_cursor, drag.end_cursor);

    match st.mode {
        WidgetMode::Expanded | WidgetMode::Collapsed => {
            // 原生 WM_SIZING 已排除手动缩放；DPI 导致的尺寸变化不能误排除真实拖动。
            if !st.allows_edge_dock() || moved <= 0.5 {
                return;
            }
            let enabled = app
                .try_state::<crate::commands::CheckItState>()
                .and_then(|s| s.db.load_settings().ok())
                .map(|s: AppSettings| s.edge_dock_enabled)
                .unwrap_or(true);
            if !enabled {
                return;
            }
            let Some((wa, m_scale)) = work_area_for(&widget, cur.center()) else {
                return;
            };
            let threshold = EDGE_DOCK_THRESHOLD * m_scale;
            if let Some(edge) = select_edge(&cur, &wa, threshold) {
                if let Err(e) = dock_to(app, &widget, edge, &cur, &wa) {
                    log::error!("贴边收纳失败: {e}");
                }
            }
        }
        WidgetMode::Docked => {
            if moved <= CLICK_DISPLACEMENT * scale {
                // 单击：由前端 onClick 负责展开（无位移的纯点击不会走到这里，
                // 有轻微抖动时走到这里也必须忽略，避免与前端双触发）
                return;
            }
            let Some((wa, m_scale)) = work_area_for(&widget, cur.center()) else {
                return;
            };
            let threshold = EDGE_DOCK_THRESHOLD * m_scale;
            let (_, nearest) = nearest_edge(&cur, &wa);
            let (x, y) = dock_anchor(nearest, DOCK_BUBBLE_SIZE * scale, &wa, cur.center());
            if select_edge(&cur, &wa, threshold).is_some() {
                // 仍在边缘：保持圆圈，仅重贴边并更新锚点
                if let Err(e) = reanchor_dock(app, &widget, nearest, x, y) {
                    log::error!("圆圈重新贴边失败: {e}");
                }
            } else {
                // 拖离边缘：恢复完整面板
                if let Err(e) = expand_widget_locked(app, &widget, None) {
                    log::error!("恢复完整面板失败: {e}");
                }
            }
        }
    }
}

/// 把完整面板收纳为 40 逻辑像素圆圈（真正缩小原生窗口与最小尺寸）。
fn dock_to(
    app: &AppHandle,
    widget: &WebviewWindow,
    edge: Edge,
    cur: &Rect,
    wa: &Rect,
) -> tauri::Result<WidgetMode> {
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let scale = widget.scale_factor()?;
    let (x, y) = dock_anchor(edge, DOCK_BUBBLE_SIZE * scale, wa, cur.center());
    let next = WidgetGeometryState {
        mode: WidgetMode::Docked,
        expanded: Some(expanded_before_dock(&geom.0.lock(), *cur, scale)),
        temp_height: None,
        quick_add_open: false,
        dock: Some((edge, x, y)),
    };
    commit_layout(
        app,
        widget,
        &geom,
        next,
        Layout {
            min: mode_min(WidgetMode::Docked),
            size: (DOCK_BUBBLE_SIZE, DOCK_BUBBLE_SIZE),
            position: (x as i32, y as i32),
            resizable: false,
        },
    )
}

fn expanded_before_dock(state: &WidgetGeometryState, current: Rect, scale: f64) -> Rect {
    let normal = if state.mode == WidgetMode::Collapsed {
        let height = state.expanded.map(|r| r.h).unwrap_or(DEFAULT_HEIGHT * scale);
        Rect::new(current.x, current.y, current.w, height)
    } else {
        current
    };
    compact_expanded_rect(normal, scale)
}

/// 收纳是尺寸重置边界：不把系统贴靠、意外拉大或旧 DPI 下的巨型尺寸带回来。
/// 更小的有效用户尺寸仍保留；物理/逻辑换算只进行一次。
fn compact_expanded_rect(rect: Rect, scale: f64) -> Rect {
    Rect::new(
        rect.x,
        rect.y,
        (rect.w / scale).clamp(EXPANDED_MIN_W, DEFAULT_WIDTH) * scale,
        restored_logical_height(rect.h, scale) * scale,
    )
}

/// 调用者持有转换锁，移动失败不会提交新的锚点。
fn reanchor_dock(
    app: &AppHandle,
    widget: &WebviewWindow,
    edge: Edge,
    x: f64,
    y: f64,
) -> tauri::Result<WidgetMode> {
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let mut next = geom.0.lock().clone();
    next.dock = Some((edge, x, y));
    commit_layout(
        app,
        widget,
        &geom,
        next,
        Layout {
            min: mode_min(WidgetMode::Docked),
            size: (DOCK_BUBBLE_SIZE, DOCK_BUBBLE_SIZE),
            position: (x as i32, y as i32),
            resizable: false,
        },
    )
}

/// 显示器 DPI / 数量变化后：圆圈按原边缘重新贴进当前工作区（不可见则展开回退）。
fn resnap_dock(app: &AppHandle) {
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    let Some(state) = app.try_state::<WidgetGeometry>() else {
        return;
    };
    let _transition = state.1.lock();
    let Some(st) = state.0.try_lock().map(|g| g.clone()) else {
        return;
    };
    if st.mode != WidgetMode::Docked {
        return;
    }
    let Some(cur) = widget_rect(&widget) else {
        return;
    };
    let Some((wa, _)) = work_area_for(&widget, cur.center()) else {
        return;
    };
    let Some((edge, _, _)) = st.dock else {
        return;
    };
    if !intersects(&cur, &wa) {
        // 锚点已不可见（拔屏）：退回完整面板，保留托盘恢复入口
        log::info!("圆圈锚点不可见（显示器变更），回退完整面板");
        if let Err(e) = expand_widget_locked(app, &widget, None) {
            log::error!("显示器变更后恢复窗口失败: {e}");
        }
        return;
    }
    let scale = widget.scale_factor().unwrap_or(1.0);
    let (x, y) = dock_anchor(edge, DOCK_BUBBLE_SIZE * scale, &wa, cur.center());
    if let Err(e) = reanchor_dock(app, &widget, edge, x, y) {
        log::error!("显示器变更后重新贴边失败: {e}");
    }
}

/// 按模式分流保存几何（防抖触发与关闭前强制落盘共用）：
/// - Expanded 且非临时高度：保存完整正常几何
/// - Collapsed：只保存位置（保留库存的展开宽高）
/// - Docked：保存圆圈锚点
/// - Expanded 且临时高度：跳过（临时 320 绝不写回正常几何）
fn save_widget_geometry(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return;
    };
    let Some(geom) = app.try_state::<WidgetGeometry>() else {
        return;
    };
    let Some(_transition) = geom.1.try_lock() else {
        return;
    };
    if geom.2.load(Ordering::Relaxed) {
        return;
    }
    let Some(widget) = app.get_webview_window("widget") else {
        return;
    };
    let st = geom.0.lock().clone();
    let Some(rect) = widget_rect(&widget) else {
        return;
    };
    if rect.w == 0.0 || rect.h == 0.0 {
        return; // 最小化/销毁过程中的无效尺寸
    }

    match st.mode {
        WidgetMode::Expanded if should_persist_expanded(st.mode, st.temp_height) => {
            if let Err(e) = state
                .db
                .save_window_geometry("widget", rect.x, rect.y, rect.w, rect.h)
            {
                log::warn!("保存窗口几何失败: {e}");
            }
            if let Some(mut s) = geom.0.try_lock() {
                s.expanded = Some(rect);
            }
        }
        WidgetMode::Collapsed => {
            // 只写位置两键，库存宽高保持为正常展开尺寸
            if let Err(e) = state.db.set_setting_entries(&[
                ("widget_px_x".to_string(), rect.x.to_string()),
                ("widget_px_y".to_string(), rect.y.to_string()),
            ]) {
                log::warn!("保存收起位置失败: {e}");
            }
        }
        WidgetMode::Docked => {
            // 只更新锚点坐标（边缘键由 dock_to / 重贴边时写入）
            if st.dock.is_some() {
                if let Err(e) = state.db.set_setting_entries(&[
                    (KEY_DOCK_X.to_string(), rect.x.to_string()),
                    (KEY_DOCK_Y.to_string(), rect.y.to_string()),
                ]) {
                    log::warn!("保存圆圈锚点失败: {e}");
                }
            }
        }
        _ => {}
    }
}

/// 恢复时使用的逻辑高度：旧版可能存了 64（收起高度），统一回退到展开高度。
fn restored_logical_height(saved_phys_h: f64, scale: f64) -> f64 {
    let logical_h = saved_phys_h / scale;
    if logical_h <= COLLAPSED_HEIGHT + 8.0 {
        DEFAULT_HEIGHT
    } else {
        logical_h.clamp(EXPANDED_MIN_H, DEFAULT_HEIGHT)
    }
}

/// 启动时初始化托管几何状态（先给默认值，restore 再按持久化数据覆盖）。
pub fn init_widget_state(app: &AppHandle) {
    if app.try_state::<WidgetGeometry>().is_none() {
        app.manage(WidgetGeometry(
            Mutex::new(WidgetGeometryState::default()),
            Mutex::new(()),
            AtomicBool::new(false),
        ));
    }
}

/// 读取持久化的窗口模式与圆圈锚点。
fn load_persisted_dock(app: &AppHandle) -> (WidgetMode, Option<(Edge, f64, f64)>) {
    let Some(state) = app.try_state::<crate::commands::CheckItState>() else {
        return (WidgetMode::Expanded, None);
    };
    let mode = state
        .db
        .get_setting_value(KEY_MODE)
        .map(|s| WidgetMode::parse(&s))
        .unwrap_or(WidgetMode::Expanded);
    let dock = state
        .db
        .get_setting_value(KEY_DOCK_EDGE)
        .and_then(|s| Edge::parse(&s))
        .and_then(|edge| {
            let x = state.db.get_setting_value(KEY_DOCK_X)?.parse().ok()?;
            let y = state.db.get_setting_value(KEY_DOCK_Y)?.parse().ok()?;
            Some((edge, x, y))
        });
    (mode, dock)
}

/// 恢复悬浮窗保存的几何与模式（物理像素）。
///
/// - Expanded：恢复位置与宽高（多显示器可见性校验，不可见回退居中）
/// - Collapsed：64 逻辑高，恢复位置（同样校验）
/// - Docked：40 逻辑圆圈，恢复锚点（锚点不可见时回退完整面板）
///
/// 只持久化展开尺寸 + 模式键；旧版 64 收起高度回退 DEFAULT_HEIGHT。
pub fn restore_widget_geometry(app: &AppHandle) {
    if let Err(e) = restore_widget_geometry_inner(app) {
        log::error!("恢复窗口几何失败，保留当前窗口与已保存数据: {e}");
    }
}

fn restore_widget_geometry_inner(app: &AppHandle) -> tauri::Result<()> {
    init_widget_state(app);
    let widget = app
        .get_webview_window("widget")
        .ok_or_else(|| window_error("悬浮窗不存在"))?;
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let _transition = geom.1.lock();
    // 此入口只用于新创建的默认展开窗口（启动/销毁后重建）。回滚基线必须对应新窗口。
    *geom.0.lock() = WidgetGeometryState::default();
    let state = app
        .try_state::<crate::commands::CheckItState>()
        .ok_or_else(|| window_error("数据库状态尚未初始化"))?;
    let saved = state
        .db
        .load_window_geometry("widget")
        .filter(|(x, y, w, h)| {
            [*x, *y, *w, *h].iter().all(|v| v.is_finite()) && *w > 0.0 && *h > 0.0
        });
    let (mut mode, dock) = load_persisted_dock(app);
    let scale = widget.scale_factor()?;
    let normal = if let Some((x, y, w, h)) = saved {
        Rect::new(
            x,
            y,
            (w / scale).clamp(EXPANDED_MIN_W, DEFAULT_WIDTH) * scale,
            restored_logical_height(h, scale) * scale,
        )
    } else {
        mode = WidgetMode::Expanded;
        widget.center()?;
        widget_rect(&widget).ok_or_else(|| window_error("无法读取初始窗口几何"))?
    };
    let normal = work_area_for(&widget, normal.center())
        .map(|(wa, _)| {
            let (x, y) = clamp_into_work_area(&normal, &wa).x_y();
            Rect::new(x, y, normal.w, normal.h)
        })
        .unwrap_or(normal);
    let mut next = WidgetGeometryState {
        mode,
        expanded: Some(normal),
        temp_height: None,
        quick_add_open: false,
        dock: None,
    };
    let mut target = Layout {
        min: mode_min(mode),
        size: (normal.w / scale, normal.h / scale),
        position: (normal.x as i32, normal.y as i32),
        resizable: true,
    };
    if mode == WidgetMode::Docked {
        let anchor = dock.and_then(|(edge, x, y)| {
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            let bubble = Rect::new(x, y, DOCK_BUBBLE_SIZE * scale, DOCK_BUBBLE_SIZE * scale);
            let (wa, _) = work_area_for(&widget, bubble.center())?;
            if !intersects(&bubble, &wa) {
                return None;
            }
            let (x, y) = dock_anchor(edge, DOCK_BUBBLE_SIZE * scale, &wa, bubble.center());
            Some((edge, x, y))
        });
        if let Some((edge, x, y)) = anchor {
            next.dock = Some((edge, x, y));
            target.size = (DOCK_BUBBLE_SIZE, DOCK_BUBBLE_SIZE);
            target.position = (x as i32, y as i32);
            target.resizable = false;
        } else {
            // 无效锚点：本次恢复及数据库模式一并改为完整面板。
            next.mode = WidgetMode::Expanded;
            target.min = mode_min(next.mode);
        }
    } else if mode == WidgetMode::Collapsed {
        target.size.1 = COLLAPSED_HEIGHT;
    }
    commit_layout(app, &widget, &geom, next, target)?;
    Ok(())
}

/// 展开（完整面板）。`min_h` 为快速添加所需最小逻辑高度（可选）。
///
/// - 从圆圈展开：按原边缘向工作区内部展开，保证完整可见
/// - 从收起展开：恢复位置与宽高
/// - 已展开：仅按需临时抬高（快速添加），并记录临时高度
///
/// 返回最终模式。
pub fn expand_widget_inner(
    app: &AppHandle,
    widget: &WebviewWindow,
    min_h: Option<f64>,
) -> tauri::Result<WidgetMode> {
    if min_h.is_some_and(|height| {
        !height.is_finite() || !(EXPANDED_MIN_H..=EXPANDED_MAX_H).contains(&height)
    }) {
        return Err(window_error("临时窗口高度必须在 200–900 逻辑像素之间"));
    }
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let _transition = geom.1.lock();
    expand_widget_locked(app, widget, min_h)
}

fn expand_widget_locked(
    app: &AppHandle,
    widget: &WebviewWindow,
    min_h: Option<f64>,
) -> tauri::Result<WidgetMode> {
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let st = geom.0.lock().clone();
    let scale = widget.scale_factor()?;

    // 已在展开且无临时高度：把当前几何固化为权威展开几何（用户可能手动移动/缩放过）
    let expanded = if st.mode == WidgetMode::Expanded
        && st.temp_height.is_none()
        && !geom.2.load(Ordering::Relaxed)
    {
        widget_rect(widget)
    } else {
        None
    }
    .or(st.expanded);

    // 默认几何兜底
    let default_rect = || Rect::new(0.0, 0.0, DEFAULT_WIDTH * scale, DEFAULT_HEIGHT * scale);
    let normal = compact_expanded_rect(expanded.unwrap_or_else(default_rect), scale);
    let normal_w_logical = normal.w / scale;
    let normal_h_logical = normal.h / scale;

    // 编辑状态与临时高度分别记录：正常高度足够时同样禁止自动收纳。
    let mut next = st.clone();
    let target_h = next.configure_quick_add(normal_h_logical, min_h);

    // 位置：
    // - 从圆圈：向工作区内部展开（以圆心为参照）
    // - 已展开：保持当前左上角（用户可能刚拖动过）
    // - 从标题栏展开：沿用收起条当前左上角；
    //   再整体夹回工作区，保证展开后完整可见。
    let cur = widget_rect(widget).unwrap_or_else(default_rect);
    let (w_logical, position) = if st.mode == WidgetMode::Docked {
        let edge = st.dock.map(|(e, _, _)| e).unwrap_or(Edge::Right);
        let pos = work_area_for(widget, cur.center())
            .map(|(wa, _)| {
                expanded_position_from_dock(
                    edge,
                    &cur,
                    (normal_w_logical * scale, target_h * scale),
                    &wa,
                )
            })
            .unwrap_or((cur.x, cur.y));
        (normal_w_logical, pos)
    } else if st.mode == WidgetMode::Collapsed {
        // 收起条可能被横向拖动/缩放：宽度沿用当前宽度，避免展开后横向跳变
        let cur_w = (cur.w / scale).clamp(EXPANDED_MIN_W, DEFAULT_WIDTH);
        let anchor = (cur.x, cur.y);
        let pos = work_area_for(
            widget,
            (
                anchor.0 + cur_w * scale / 2.0,
                anchor.1 + target_h * scale / 2.0,
            ),
        )
        .map(|(wa, _)| {
            expanded_rect_from_collapsed(
                Rect::new(cur.x, cur.y, cur_w * scale, cur.h),
                target_h * scale,
                &wa,
            )
            .x_y()
        })
        .unwrap_or(anchor);
        (cur_w, pos)
    } else {
        (normal_w_logical, (cur.x, cur.y))
    };

    let persisted = Rect::new(
        position.0,
        position.1,
        w_logical * scale,
        normal_h_logical * scale,
    );
    next.mode = WidgetMode::Expanded;
    next.expanded = Some(persisted);
    next.dock = None;
    commit_layout(
        app,
        widget,
        &geom,
        next,
        Layout {
            min: mode_min(WidgetMode::Expanded),
            size: (w_logical, target_h),
            position: (position.0 as i32, position.1 as i32),
            resizable: true,
        },
    )
}

/// 收起为标题栏（64 逻辑像素）；失败时保留原状态及正常几何。
pub fn collapse_widget_inner(app: &AppHandle, widget: &WebviewWindow) -> tauri::Result<WidgetMode> {
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let _transition = geom.1.lock();
    if geom.2.load(Ordering::Relaxed) {
        return Err(window_error("窗口状态异常，请通过托盘重新打开窗口后再试"));
    }
    let mut next = geom.0.lock().clone();
    if next.mode == WidgetMode::Docked {
        return expand_widget_locked(app, widget, None);
    }
    if next.mode == WidgetMode::Collapsed {
        return Ok(next.mode);
    }
    let cur = widget_rect(widget).ok_or_else(|| window_error("无法读取窗口几何"))?;
    let scale = widget.scale_factor()?;
    if next.temp_height.is_none() {
        next.expanded = Some(compact_expanded_rect(cur, scale));
    }
    next.mode = WidgetMode::Collapsed;
    next.temp_height = None;
    next.quick_add_open = false;
    next.dock = None;
    commit_layout(
        app,
        widget,
        &geom,
        next,
        Layout {
            min: mode_min(WidgetMode::Collapsed),
            size: (
                (cur.w / scale).clamp(EXPANDED_MIN_W, DEFAULT_WIDTH),
                COLLAPSED_HEIGHT,
            ),
            position: (cur.x as i32, cur.y as i32),
            resizable: true,
        },
    )
}

/// 快速添加关闭：成功恢复原生高度后才清除临时标记，失败可重试。
pub fn quick_add_closed_inner(
    app: &AppHandle,
    widget: &WebviewWindow,
) -> tauri::Result<WidgetMode> {
    let geom = app
        .try_state::<WidgetGeometry>()
        .ok_or_else(|| window_error("窗口状态尚未初始化"))?;
    let _transition = geom.1.lock();
    if geom.2.load(Ordering::Relaxed) {
        return Err(window_error("窗口状态异常，请通过托盘重新打开窗口后再试"));
    }
    let mut next = geom.0.lock().clone();
    if next.mode != WidgetMode::Expanded {
        return Ok(next.mode);
    }
    if !next.quick_add_open && next.temp_height.is_none() {
        return Ok(next.mode);
    }
    let cur = widget_rect(widget).ok_or_else(|| window_error("无法读取窗口几何"))?;
    let scale = widget.scale_factor()?;
    let height = if next
        .temp_height
        .is_some_and(|temp| (cur.h - temp * scale).abs() <= 2.0 * scale)
    {
        next.expanded
            .map(|r| (r.h / scale).clamp(EXPANDED_MIN_H, EXPANDED_MAX_H))
            .unwrap_or(DEFAULT_HEIGHT)
    } else {
        (cur.h / scale).clamp(EXPANDED_MIN_H, EXPANDED_MAX_H)
    };
    let width = (cur.w / scale).clamp(EXPANDED_MIN_W, 700.0);
    next.configure_quick_add(height, None);
    next.expanded = Some(Rect::new(cur.x, cur.y, width * scale, height * scale));
    commit_layout(
        app,
        widget,
        &geom,
        next,
        Layout {
            min: mode_min(WidgetMode::Expanded),
            size: (width, height),
            position: (cur.x as i32, cur.y as i32),
            resizable: true,
        },
    )
}

/// 当前模式（前端启动时查询）。
pub fn current_mode(app: &AppHandle) -> WidgetMode {
    app.try_state::<WidgetGeometry>()
        .map(|g| g.0.lock().mode)
        .unwrap_or(WidgetMode::Expanded)
}

impl Rect {
    fn x_y(&self) -> (f64, f64) {
        (self.x, self.y)
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
    let settings=app.state::<crate::commands::CheckItState>().db.load_settings().unwrap_or_default();
    apply_glass_effect(&win, settings.glass_enabled);
    crate::tray::sync_language(app, &settings.language);
    Ok(())
}

/// 显示悬浮窗（窗口已销毁时尝试按默认配置重建，保证托盘入口始终可用）。
///
/// 贴边圆圈状态下，托盘打开 / 第二实例唤醒都恢复到完整可用面板。
pub fn show_widget(app: &AppHandle) -> tauri::Result<()> {
    if let Some(widget) = app.get_webview_window("widget") {
        widget.unminimize()?;
        widget.show()?;
        widget.set_focus()?;
        // 圆圈 → 完整面板（托盘"打开"与第二实例唤醒的语义是恢复可用界面）
        let needs_recovery = app
            .try_state::<WidgetGeometry>()
            .is_some_and(|geom| geom.2.load(Ordering::Relaxed));
        if current_mode(app) == WidgetMode::Docked || needs_recovery {
            expand_widget_inner(app, &widget, None)?;
        }
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
        .maximizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(settings.always_on_top)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .center()
        .build()?;
    restore_widget_geometry_inner(app)?;
    setup_widget_events(app);
    if current_mode(app) == WidgetMode::Docked {
        expand_widget_inner(app, &win, None)?;
    }
    apply_glass_effect(&win, settings.glass_enabled);
    crate::tray::sync_language(app, &settings.language);
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
fn clone_tray_pin_item<T: Clone>(slot: &std::sync::Mutex<Option<T>>) -> Result<Option<T>, ()> {
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
    let enabled = glass_effect_enabled(window.label(), current_mode(window.app_handle()), enabled);
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
            // Some(config) + 空列表在 Tauri/Windows 中直接返回，不会清除旧 Acrylic。
            // None 才会进入原生 clear_effects，避免透明圆球四角残留矩形底色。
            if let Err(e) = window.set_effects(None::<WindowEffectsConfig>) {
                log::warn!("清除窗口效果失败: {e}");
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, enabled);
    }
    if window.label() == "widget" || window.label() == "reminder" {
        if let Err(e) = crate::window_shape::setup(window) {
            log::warn!("应用原生圆角裁切失败: {e}");
        }
    }
}

fn glass_effect_enabled(label: &str, _mode: WidgetMode, requested: bool) -> bool {
    // 无边框悬浮窗只由 WebView 绘制背景。Acrylic 的合成底板在部分
    // Windows/DPI 组合下仍会越过圆弧边缘；窗口区域与 CSS 圆角不能可靠消除它。
    // 管理窗口继续使用原生玻璃；悬浮窗背景由前端统一绘制。
    requested && label != "widget"
}

/// 当前本地时间描述（托盘提示等场景备用）
pub fn local_date_text() -> String {
    Local::now().format("%m月%d日 %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_has_no_rectangular_acrylic_backdrop() {
        assert!(!glass_effect_enabled("widget", WidgetMode::Docked, true));
        assert!(!glass_effect_enabled("widget", WidgetMode::Expanded, true));
        assert!(!glass_effect_enabled("widget", WidgetMode::Collapsed, true));
        assert!(glass_effect_enabled("manager", WidgetMode::Docked, true));
    }

    #[test]
    fn changing_window_mode_never_overrides_disabled_glass_preference() {
        for mode in [WidgetMode::Docked, WidgetMode::Expanded, WidgetMode::Collapsed] {
            assert!(!glass_effect_enabled("widget", mode, false));
            assert!(!glass_effect_enabled("manager", mode, false));
        }
    }

    #[test]
    fn docking_collapsed_bar_preserves_normal_expanded_height() {
        let state = WidgetGeometryState {
            mode: WidgetMode::Collapsed,
            expanded: Some(Rect::new(100.0, 200.0, 510.0, 660.0)),
            temp_height: None, quick_add_open: false, dock: None,
        };
        let normal = expanded_before_dock(&state, Rect::new(0.0, 300.0, 450.0, 96.0), 1.5);
        assert_eq!(normal, Rect::new(0.0, 300.0, 450.0, 660.0));
    }

    #[test]
    fn docking_expanded_panel_uses_current_user_geometry() {
        let current = Rect::new(0.0, 300.0, 450.0, 420.0);
        assert_eq!(expanded_before_dock(&WidgetGeometryState::default(), current, 1.5), current);
    }

    #[test]
    fn restored_height_legacy_collapsed_maps_to_default() {
        assert_eq!(restored_logical_height(64.0, 1.0), DEFAULT_HEIGHT);
        assert_eq!(restored_logical_height(72.0, 1.0), DEFAULT_HEIGHT);
        assert_eq!(restored_logical_height(80.0, 1.25), DEFAULT_HEIGHT);
    }

    #[test]
    fn restored_height_clamps_into_expanded_range() {
        assert_eq!(restored_logical_height(440.0, 1.0), 440.0);
        assert_eq!(restored_logical_height(150.0, 1.0), EXPANDED_MIN_H);
        assert_eq!(restored_logical_height(5000.0, 1.0), DEFAULT_HEIGHT);
        // 高 DPI：物理 550 → 逻辑 440
        assert_eq!(restored_logical_height(550.0, 1.25), 440.0);
        // 正常较小高度保留，只有巨型窗口被压回紧凑区间。
        // 但核心保障是 should_persist_expanded：临时高度根本不会写库
        assert_eq!(restored_logical_height(320.0, 1.0), 320.0);
    }

    #[test]
    fn docking_discards_oversized_geometry_at_every_dpi() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let oversized = Rect::new(100.0, 200.0, 1920.0 * scale, 1080.0 * scale);
            let mut normal = expanded_before_dock(&WidgetGeometryState::default(), oversized, scale);
            for _ in 0..20 {
                assert_eq!(normal.w, DEFAULT_WIDTH * scale);
                assert_eq!(normal.h, DEFAULT_HEIGHT * scale);
                normal = compact_expanded_rect(normal, scale);
            }
        }
    }

    #[test]
    fn compact_restore_keeps_smaller_sizes_and_repairs_legacy_bubble_height() {
        let small = Rect::new(-100.0, 200.0, 310.0 * 1.5, 280.0 * 1.5);
        assert_eq!(compact_expanded_rect(small, 1.5), small);
        let legacy = compact_expanded_rect(Rect::new(0.0, 0.0, 40.0, 40.0), 1.0);
        assert_eq!((legacy.w, legacy.h), (EXPANDED_MIN_W, DEFAULT_HEIGHT));
        // 高 DPI 大窗口移到低 DPI 显示器，不能按物理尺寸再次放大恢复。
        let moved = compact_expanded_rect(Rect::new(10.0, 20.0, 680.0, 880.0), 1.0);
        assert_eq!((moved.w, moved.h), (DEFAULT_WIDTH, DEFAULT_HEIGHT));
    }

    #[test]
    fn click_on_bubble_is_not_a_drag() {
        let r = Rect::new(1600.0, 400.0, 40.0, 40.0);
        let d = drag_move_distance(r, r, Some((1600.0, 400.0)), Some((1602.0, 401.0)));
        assert!(d <= CLICK_DISPLACEMENT, "轻微抖动必须仍算单击");
    }

    #[test]
    fn drag_away_from_edge_exceeds_click_threshold() {
        let start = Rect::new(1600.0, 400.0, 40.0, 40.0);
        let cur = Rect::new(1200.0, 400.0, 40.0, 40.0);
        let d = drag_move_distance(start, cur, Some((1600.0, 400.0)), Some((1200.0, 400.0)));
        assert!(d > CLICK_DISPLACEMENT);
    }

    #[test]
    fn cross_screen_dpi_scale_does_not_look_like_drag() {
        // 窗口因跨屏 DPI（100% → 150%）整体变大，但鼠标几乎没动：
        // 位移必须按鼠标判定，否则会被误判为"拖动"从而误展开/误吸附
        let start = Rect::new(1900.0, 300.0, 340.0, 440.0);
        let cur = Rect::new(1900.0, 300.0, 510.0, 660.0);
        let d = drag_move_distance(start, cur, Some((2000.0, 400.0)), Some((2001.0, 401.0)));
        assert!(
            d <= CLICK_DISPLACEMENT,
            "鼠标未移动时不能把 DPI 缩放当成拖动位移，实际 {d}"
        );
    }

    /// 模拟"主线程菜单更新"需要获取同一把锁的可控交错：
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
        let simulated_main_thread =
            std::thread::spawn(move || item.apply_on_simulated_main_thread());
        assert!(
            simulated_main_thread.join().unwrap(),
            "克隆菜单项后必须已释放锁，否则 set_checked 会等待主线程死锁"
        );
    }
}
