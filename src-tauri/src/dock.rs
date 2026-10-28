//! 贴边收纳球：窗口模式与几何的纯逻辑层。
//!
//! 职责边界：本模块只做**几何判断与状态建模**，不触碰任何 Tauri 窗口 API，
//! 从而可以在单元测试中覆盖四边吸附、角落择边、单击/拖动区分、
//! 临时高度不覆盖正常几何等规则。
//!
//! 单位约定：`Rect` 一律使用**物理像素**（与持久化及显示器工作区一致）；
//! 吸附阈值、圆圈尺寸、单击位移阈值等用户可感知尺寸以**逻辑像素**给出，
//! 由调用方乘以对应显示器 scale 换算。

use serde::{Deserialize, Serialize};

/// 吸附阈值（逻辑像素）：窗口边缘距工作区边缘小于该值时收纳。
pub const EDGE_DOCK_THRESHOLD: f64 = 12.0;

/// 收纳球直径（逻辑像素）。
pub const DOCK_BUBBLE_SIZE: f64 = 40.0;

/// 标题栏收起高度（逻辑像素），与前端保持一致。
pub const COLLAPSED_HEIGHT: f64 = 64.0;

/// 判定"窗口尺寸在拖动会话中发生变化"的容差（物理像素）。
/// 超过该变化说明用户在缩放窗口而不是移动窗口，不参与吸附判定。
pub const RESIZE_EPSILON: f64 = 2.0;

/// 判定圆圈"单击（未拖动）"的最大位移（逻辑像素）。
pub const CLICK_DISPLACEMENT: f64 = 4.0;

/// 悬浮窗窗口模式：
/// - Expanded：完整面板（正常展开，唯一允许写回正常几何的模式）
/// - Collapsed：标题栏收起（64 逻辑像素高）
/// - Docked：贴边收纳球（40 逻辑像素圆圈）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetMode {
    Expanded,
    Collapsed,
    Docked,
}

impl WidgetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WidgetMode::Expanded => "expanded",
            WidgetMode::Collapsed => "collapsed",
            WidgetMode::Docked => "docked",
        }
    }

    /// 从持久化字符串解析；非法值回退 Expanded（旧数据没有该键时同样回退）。
    pub fn parse(s: &str) -> Self {
        match s {
            "collapsed" => WidgetMode::Collapsed,
            "docked" => WidgetMode::Docked,
            _ => WidgetMode::Expanded,
        }
    }
}

/// 贴边方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn as_str(self) -> &'static str {
        match self {
            Edge::Left => "left",
            Edge::Right => "right",
            Edge::Top => "top",
            Edge::Bottom => "bottom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Edge::Left),
            "right" => Some(Edge::Right),
            "top" => Some(Edge::Top),
            "bottom" => Some(Edge::Bottom),
            _ => None,
        }
    }
}

/// 物理像素矩形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// 窗口几何权威状态（物理像素）。
///
/// 不变量：
/// 1. `expanded` 永远是"最后一次正常展开"的位置与尺寸；
///    收起（64）、圆圈（40）、快速添加临时抬高（≥320）都不写回该值。
/// 2. `temp_height` 为 Some 时表示窗口当前停在快速添加临时高度，
///    期间几何保存与吸附判定全部跳过。
/// 3. `dock` 记录圆圈所在边缘与锚点，供重启恢复与 DPI 变化后重贴边。
/// 4. `quick_add_open` 独立于临时高度；编辑期间即使无需抬高也不自动收纳。
#[derive(Debug, Clone)]
pub struct WidgetGeometryState {
    pub mode: WidgetMode,
    pub expanded: Option<Rect>,
    /// 快速添加临时抬高的逻辑高度（当前生效时为 Some）
    pub temp_height: Option<f64>,
    /// 仅内存中的编辑状态，不持久化草稿或改变数据库 schema。
    pub quick_add_open: bool,
    /// 圆圈锚点：边缘 + 物理坐标
    pub dock: Option<(Edge, f64, f64)>,
}

impl Default for WidgetGeometryState {
    fn default() -> Self {
        Self {
            mode: WidgetMode::Expanded,
            expanded: None,
            temp_height: None,
            quick_add_open: false,
            dock: None,
        }
    }
}

impl WidgetGeometryState {
    /// 打开/关闭快速添加与临时高度分开记录；返回这次展开需要的逻辑高度。
    pub fn configure_quick_add(&mut self, normal_height: f64, min_height: Option<f64>) -> f64 {
        self.quick_add_open = min_height.is_some();
        self.temp_height = min_height.filter(|height| *height > normal_height + 0.5);
        self.temp_height.unwrap_or(normal_height)
    }

    pub fn allows_edge_dock(&self) -> bool {
        matches!(self.mode, WidgetMode::Expanded | WidgetMode::Collapsed)
            && !self.quick_add_open
            && self.temp_height.is_none()
    }
}

/// 是否允许把当前窗口几何写回"正常展开几何"。
///
/// 只有完整面板模式且不在临时高度时才允许——这是"320 临时高度 /
/// 64 收起高度 / 40 圆圈都不能覆盖正常展开尺寸"的统一判定点。
pub fn should_persist_expanded(mode: WidgetMode, temp_height: Option<f64>) -> bool {
    mode == WidgetMode::Expanded && temp_height.is_none()
}

/// 窗口边缘到工作区边缘的最近距离；返回 (距离, 边缘)。
///
/// 规则（要求"靠近角落时选择最近边缘，规则稳定"）：
/// - 距离取窗口矩形到工作区矩形四边的间隙，越界（负值）按 0 处理；
/// - 四边中取距离最小者；完全相等时按 Left → Right → Top → Bottom
///   的固定顺序取第一个（角落在阈值内时总有唯一最小边或稳定并列解）。
pub fn nearest_edge(win: &Rect, wa: &Rect) -> (f64, Edge) {
    let candidates = [
        ((win.x - wa.x).max(0.0), Edge::Left),
        ((wa.right() - win.right()).max(0.0), Edge::Right),
        ((win.y - wa.y).max(0.0), Edge::Top),
        ((wa.bottom() - win.bottom()).max(0.0), Edge::Bottom),
    ];
    let mut best = candidates[0];
    for cand in candidates.iter().skip(1) {
        if cand.0 < best.0 {
            best = *cand;
        }
    }
    best
}

/// 判定窗口松手时是否应贴边收纳，以及贴到哪条边。
///
/// `threshold` 为物理像素（调用方用 12 逻辑像素 × scale 换算）。
pub fn select_edge(win: &Rect, wa: &Rect, threshold: f64) -> Option<Edge> {
    let (dist, edge) = nearest_edge(win, wa);
    (dist <= threshold).then_some(edge)
}

/// 计算圆圈吸附到边缘后的锚点（物理像素）。
///
/// `bubble` 为圆圈物理边长；`ref_point` 为窗口中心（用于沿边定位）。
/// 圆圈完整落在工作区内、紧贴目标边缘。
pub fn dock_anchor(edge: Edge, bubble: f64, wa: &Rect, ref_point: (f64, f64)) -> (f64, f64) {
    let (rx, ry) = ref_point;
    match edge {
        Edge::Left => (
            wa.x,
            (ry - bubble / 2.0).clamp(wa.y, (wa.bottom() - bubble).max(wa.y)),
        ),
        Edge::Right => (
            (wa.right() - bubble).max(wa.x),
            (ry - bubble / 2.0).clamp(wa.y, (wa.bottom() - bubble).max(wa.y)),
        ),
        Edge::Top => (
            (rx - bubble / 2.0).clamp(wa.x, (wa.right() - bubble).max(wa.x)),
            wa.y,
        ),
        Edge::Bottom => (
            (rx - bubble / 2.0).clamp(wa.x, (wa.right() - bubble).max(wa.x)),
            (wa.bottom() - bubble).max(wa.y),
        ),
    }
}

/// 从圆圈位置恢复完整面板时的窗口位置（物理像素）：
/// 保留圆圈所在边缘贴边，向工作区内部展开，保证面板完整可见。
pub fn expanded_position_from_dock(
    edge: Edge,
    circle: &Rect,
    size: (f64, f64),
    wa: &Rect,
) -> (f64, f64) {
    let (w, h) = size;
    match edge {
        Edge::Left => (
            wa.x,
            (circle.y + circle.h / 2.0 - h / 2.0).clamp(wa.y, (wa.bottom() - h).max(wa.y)),
        ),
        Edge::Right => (
            (wa.right() - w).max(wa.x),
            (circle.y + circle.h / 2.0 - h / 2.0).clamp(wa.y, (wa.bottom() - h).max(wa.y)),
        ),
        Edge::Top => (
            (circle.x + circle.w / 2.0 - w / 2.0).clamp(wa.x, (wa.right() - w).max(wa.x)),
            wa.y,
        ),
        Edge::Bottom => (
            (circle.x + circle.w / 2.0 - w / 2.0).clamp(wa.x, (wa.right() - w).max(wa.x)),
            (wa.bottom() - h).max(wa.y),
        ),
    }
}

/// 把矩形整体夹回工作区（显示器断开 / 负坐标副屏 / 任务栏变化后的兜底）。
pub fn clamp_into_work_area(rect: &Rect, wa: &Rect) -> Rect {
    let x = rect.x.clamp(wa.x, (wa.right() - rect.w).max(wa.x));
    let y = rect.y.clamp(wa.y, (wa.bottom() - rect.h).max(wa.y));
    Rect::new(x, y, rect.w, rect.h)
}

/// 收起条展开后沿用当前左上角与宽度，只恢复正常高度，再夹回可见工作区。
pub fn expanded_rect_from_collapsed(current: Rect, expanded_height: f64, wa: &Rect) -> Rect {
    clamp_into_work_area(&Rect::new(current.x, current.y, current.w, expanded_height), wa)
}

/// 矩形是否与工作区有实际重叠（用于恢复位置可见性校验）。
pub fn intersects(rect: &Rect, wa: &Rect) -> bool {
    rect.x < wa.right() && rect.right() > wa.x && rect.y < wa.bottom() && rect.bottom() > wa.y
}

/// 判定圆圈拖动会话是否应视为"单击"（位移在容差内）。
/// `displacement_threshold` 为物理像素（4 逻辑像素 × scale）。
pub fn is_click(start: (f64, f64), end: (f64, f64), displacement_threshold: f64) -> bool {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    dx.abs().max(dy.abs()) <= displacement_threshold
}

/// 判定拖动会话期间窗口是否被缩放（缩放不参与吸附）。
pub fn size_changed(start: &Rect, end: &Rect) -> bool {
    (start.w - end.w).abs() > RESIZE_EPSILON || (start.h - end.h).abs() > RESIZE_EPSILON
}

/// 圆圈计数显示：0 显示 0；1..=99 显示原值；>99 显示 99+。
pub fn format_bubble_count(count: i64) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

/// 拖动会话判定结果（供拖动观察线程使用，纯逻辑便于测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// 会话开始前鼠标已松开：程序化移动或偶然事件，直接忽略（不递归吸附）
    Ignored,
    /// 左键仍按住：拖动尚未结束，继续等待（拖动暂停 / 跨屏途中都走这里）
    Pending,
    /// 左键已松开：拖动结束，进入吸附/展开判定
    Finished,
}

/// 判定一次拖动观察轮询的结果。
///
/// - `button_down_at_start`：会话开始（首个 Moved 事件）时左键是否按下
/// - `button_down_now`：本轮轮询时左键是否按下
pub fn drag_step(button_down_at_start: bool, button_down_now: bool) -> DragOutcome {
    if !button_down_at_start {
        DragOutcome::Ignored
    } else if button_down_now {
        DragOutcome::Pending
    } else {
        DragOutcome::Finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_add_blocks_docking_with_or_without_temporary_growth() {
        for normal_height in [200.0, 320.0, 440.0, 900.0] {
            let normal = Rect::new(300.0, 200.0, 340.0, normal_height);
            let mut state = WidgetGeometryState {
                expanded: Some(normal),
                ..WidgetGeometryState::default()
            };
            assert!(state.allows_edge_dock());
            let actual_height = state.configure_quick_add(normal_height, Some(320.0));
            assert_eq!(actual_height, normal_height.max(320.0));
            assert!(!state.allows_edge_dock(), "编辑期间禁止收纳，正常高度={normal_height}");
            assert_eq!(state.expanded, Some(normal), "临时高度不能覆盖正常几何");
            state.configure_quick_add(normal_height, None);
            assert!(state.allows_edge_dock(), "关闭编辑后恢复收纳");
            assert_eq!(state.expanded, Some(normal));
        }
    }

    #[test]
    fn expanding_moved_collapsed_bar_keeps_current_position_at_each_scale() {
        for scale in [1.0, 1.25, 1.5] {
            let wa = Rect::new(-1920.0 * scale, 0.0, 1920.0 * scale, 1040.0 * scale);
            // 正常面板曾在 x=-1600；收起后移到了 x=-1000，并调整了宽度。
            let current = Rect::new(-1000.0 * scale, 100.0 * scale, 400.0 * scale, 64.0 * scale);
            let result = expanded_rect_from_collapsed(current, 440.0 * scale, &wa);
            assert_eq!(result, Rect::new(current.x, current.y, current.w, 440.0 * scale));
        }
    }

    #[test]
    fn expanding_collapsed_bar_only_clamps_when_full_panel_would_leave_work_area() {
        let wa = Rect::new(0.0, 0.0, 1920.0, 1040.0);
        let current = Rect::new(1700.0, 900.0, 340.0, 64.0);
        assert_eq!(
            expanded_rect_from_collapsed(current, 440.0, &wa),
            Rect::new(1580.0, 600.0, 340.0, 440.0),
        );
    }

    /// 1920×1080 工作区（物理像素，scale=1 时阈值 12）
    fn wa() -> Rect {
        Rect::new(0.0, 0.0, 1920.0, 1040.0)
    }

    // ---- 四边吸附 ----

    #[test]
    fn docks_left_within_threshold() {
        let win = Rect::new(5.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Left));
    }

    #[test]
    fn docks_right_within_threshold() {
        let win = Rect::new(1920.0 - 340.0 - 8.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Right));
    }

    #[test]
    fn docks_top_within_threshold() {
        let win = Rect::new(500.0, 3.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Top));
    }

    #[test]
    fn docks_bottom_within_threshold() {
        let win = Rect::new(500.0, 1040.0 - 440.0 - 10.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Bottom));
    }

    #[test]
    fn no_dock_outside_threshold() {
        let win = Rect::new(100.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), None);
    }

    #[test]
    fn boundary_exactly_at_threshold_docks() {
        let win = Rect::new(12.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Left));
        let win = Rect::new(13.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), None);
    }

    // ---- 角落：取最近边缘，规则稳定 ----

    #[test]
    fn corner_picks_strictly_nearest_edge() {
        // 左上角：距左 6，距上 9 → 左
        let win = Rect::new(6.0, 9.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Left));
        // 左上角：距左 9，距上 6 → 上
        let win = Rect::new(9.0, 6.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Top));
    }

    #[test]
    fn corner_tie_breaks_deterministically() {
        // 左上角完全等距（8, 8）→ 固定顺序 Left 优先
        let win = Rect::new(8.0, 8.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), Some(Edge::Left));
    }

    #[test]
    fn corner_outside_threshold_no_dock() {
        // 左上角但两距都超过阈值
        let win = Rect::new(20.0, 20.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa(), 12.0), None);
    }

    // ---- 高 DPI / 负坐标副屏 ----

    #[test]
    fn threshold_uses_monitor_scale() {
        // scale = 1.5 → 物理阈值 18；距左 15 逻辑 × 1.5 = 22.5 物理 > 18 → 不吸附
        let win = Rect::new(22.5, 200.0, 510.0, 660.0);
        assert_eq!(select_edge(&win, &wa(), 12.0 * 1.5), None);
        // 距左 10 逻辑 × 1.5 = 15 物理 ≤ 18 → 吸附
        let win = Rect::new(15.0, 200.0, 510.0, 660.0);
        assert_eq!(select_edge(&win, &wa(), 12.0 * 1.5), Some(Edge::Left));
    }

    #[test]
    fn negative_coordinate_secondary_monitor() {
        // 副屏在主屏左侧：工作区 x ∈ [-1920, 0)
        let wa2 = Rect::new(-1920.0, 0.0, 1920.0, 1040.0);
        // 贴左边缘（x = -1915）
        let win = Rect::new(-1915.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa2, 12.0), Some(Edge::Left));
        // 贴右边缘（right = -5）
        let win = Rect::new(-345.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa2, 12.0), Some(Edge::Right));
        // 远离边缘
        let win = Rect::new(-1000.0, 200.0, 340.0, 440.0);
        assert_eq!(select_edge(&win, &wa2, 12.0), None);
    }

    // ---- 圆圈锚点 ----

    #[test]
    fn dock_anchor_places_bubble_on_each_edge() {
        let wa = wa();
        let (x, y) = dock_anchor(Edge::Left, 40.0, &wa, (100.0, 500.0));
        assert_eq!((x, y), (0.0, 480.0));

        let (x, y) = dock_anchor(Edge::Right, 40.0, &wa, (1000.0, 500.0));
        assert_eq!((x, y), (1880.0, 480.0));

        let (x, y) = dock_anchor(Edge::Top, 40.0, &wa, (960.0, 500.0));
        assert_eq!((x, y), (940.0, 0.0));

        let (x, y) = dock_anchor(Edge::Bottom, 40.0, &wa, (960.0, 500.0));
        assert_eq!((x, y), (940.0, 1000.0));
    }

    #[test]
    fn dock_anchor_clamps_into_work_area() {
        let wa = wa();
        // 参考点超出底部：圆圈不越过工作区底边
        let (_, y) = dock_anchor(Edge::Left, 40.0, &wa, (0.0, 5000.0));
        assert_eq!(y, wa.bottom() - 40.0);
        // 参考点在顶部之上
        let (_, y) = dock_anchor(Edge::Left, 40.0, &wa, (0.0, -100.0));
        assert_eq!(y, wa.y);
    }

    // ---- 从圆圈恢复面板 ----

    #[test]
    fn expand_from_right_edge_stays_visible() {
        let wa = wa();
        let circle = Rect::new(1880.0, 500.0, 40.0, 40.0);
        let (x, y) = expanded_position_from_dock(Edge::Right, &circle, (340.0, 440.0), &wa);
        assert_eq!(x, wa.right() - 340.0);
        assert!(y >= wa.y && y + 440.0 <= wa.bottom());
    }

    #[test]
    fn expand_from_top_edge_expands_downward() {
        let wa = wa();
        let circle = Rect::new(960.0, 0.0, 40.0, 40.0);
        let (x, y) = expanded_position_from_dock(Edge::Top, &circle, (340.0, 440.0), &wa);
        assert_eq!(y, wa.y);
        assert!(x >= wa.x && x + 340.0 <= wa.right());
    }

    #[test]
    fn expand_position_clamps_near_taskbar() {
        // 任务栏在底部（工作区高 1040）：圆圈贴底边时面板向上展开
        let wa = wa();
        let circle = Rect::new(960.0, 1000.0, 40.0, 40.0);
        let (_, y) = expanded_position_from_dock(Edge::Bottom, &circle, (340.0, 440.0), &wa);
        assert_eq!(y, wa.bottom() - 440.0);
    }

    // ---- 单击/拖动区分 ----

    #[test]
    fn small_displacement_is_click() {
        assert!(is_click((100.0, 100.0), (102.0, 101.0), 4.0));
        assert!(!is_click((100.0, 100.0), (105.0, 100.0), 4.0));
    }

    #[test]
    fn resize_sessions_are_detected() {
        let start = Rect::new(100.0, 100.0, 340.0, 440.0);
        assert!(!size_changed(&start, &Rect::new(300.0, 200.0, 340.0, 440.0)));
        assert!(size_changed(&start, &Rect::new(100.0, 100.0, 500.0, 440.0)));
        assert!(size_changed(&start, &Rect::new(100.0, 100.0, 340.0, 200.0)));
        // 微小抖动不算缩放
        assert!(!size_changed(&start, &Rect::new(100.0, 100.0, 341.0, 441.0)));
    }

    // ---- 模式与几何不变量 ----

    #[test]
    fn only_expanded_without_temp_persists_geometry() {
        use WidgetMode::*;
        assert!(should_persist_expanded(Expanded, None));
        // 快速添加临时高度 / 收起 / 圆圈都不写回正常几何
        assert!(!should_persist_expanded(Expanded, Some(320.0)));
        assert!(!should_persist_expanded(Collapsed, None));
        assert!(!should_persist_expanded(Docked, None));
    }

    #[test]
    fn mode_string_roundtrip() {
        for m in [WidgetMode::Expanded, WidgetMode::Collapsed, WidgetMode::Docked] {
            assert_eq!(WidgetMode::parse(m.as_str()), m);
        }
        // 旧库 / 非法值回退 Expanded
        assert_eq!(WidgetMode::parse("nonsense"), WidgetMode::Expanded);
        assert_eq!(WidgetMode::parse(""), WidgetMode::Expanded);
    }

    #[test]
    fn edge_string_roundtrip() {
        for e in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            assert_eq!(Edge::parse(e.as_str()), Some(e));
        }
        assert_eq!(Edge::parse("middle"), None);
    }

    // ---- 显示器断开兜底 ----

    #[test]
    fn clamp_pulls_offscreen_rect_back() {
        let wa = Rect::new(0.0, 0.0, 1920.0, 1040.0);
        // 副屏被拔掉后窗口停在负坐标
        let clamped = clamp_into_work_area(&Rect::new(-1500.0, -300.0, 340.0, 440.0), &wa);
        assert_eq!(clamped, Rect::new(0.0, 0.0, 340.0, 440.0));
        assert!(intersects(&clamped, &wa));
    }

    #[test]
    fn offscreen_positive_x_falls_back_inside_work_area() {
        // 实机复现：库里保存 x=2437（曾有 2560 宽显示器），当前仅 1707 宽。
        // 位置不可见时若只居中而不钳制，窗口会被推出可见区并被裁成最小尺寸。
        let wa = Rect::new(0.0, 0.0, 1707.0, 1019.0);
        let saved = Rect::new(2437.0, 209.0, 450.0, 300.0);
        assert!(!intersects(&saved, &wa), "保存位置本身不可见");
        let clamped = clamp_into_work_area(&saved, &wa);
        assert_eq!(clamped.x, 1707.0 - 450.0);
        assert_eq!(clamped.y, 209.0);
        // 尺寸必须原样保留，不能退化到最小尺寸
        assert_eq!((clamped.w, clamped.h), (450.0, 300.0));
        assert!(intersects(&clamped, &wa));
    }

    #[test]
    fn clamp_keeps_rect_intact_when_only_position_is_offscreen() {
        // 高度超过工作区时只夹位置、不改尺寸（交由窗口最小/最大尺寸约束）
        let wa = Rect::new(0.0, 0.0, 1707.0, 1019.0);
        let rect = Rect::new(1900.0, 900.0, 340.0, 440.0);
        let clamped = clamp_into_work_area(&rect, &wa);
        assert!((clamped.w, clamped.h) == (340.0, 440.0));
        assert!((clamped.x, clamped.y) == (1707.0 - 340.0, 1019.0 - 440.0));
    }

    // ---- 计数显示 ----

    #[test]
    fn bubble_count_formatting() {
        assert_eq!(format_bubble_count(0), "0");
        assert_eq!(format_bubble_count(9), "9");
        assert_eq!(format_bubble_count(99), "99");
        assert_eq!(format_bubble_count(100), "99+");
        assert_eq!(format_bubble_count(12345), "99+");
    }

    // ---- 拖动会话状态机（拖动暂停 / 跨屏 / 程序化移动）----

    #[test]
    fn programmatic_move_is_ignored() {
        // 程序化 set_position：会话开始时左键未按下 → 忽略，不递归吸附
        assert_eq!(drag_step(false, false), DragOutcome::Ignored);
        assert_eq!(drag_step(false, true), DragOutcome::Ignored);
    }

    #[test]
    fn paused_drag_does_not_finish() {
        // 用户按住左键停在半路（Moved 停止）：仍为 Pending，不会被当成松手
        assert_eq!(drag_step(true, true), DragOutcome::Pending);
    }

    #[test]
    fn drag_finishes_only_when_button_released() {
        assert_eq!(drag_step(true, false), DragOutcome::Finished);
    }

    /// 模拟拖动观察线程的完整时序：按住移动 → 暂停 → 跨屏继续移动 → 松手。
    /// 只有"松手"那一刻才应产生 Finished，中途多少个 Moved 都不能结束会话。
    #[test]
    fn drag_sequence_finishes_only_on_release() {
        let samples = [
            (true, true),   // 首个 Moved，按住
            (true, true),   // 暂停（无 Moved，但轮询仍按住）
            (true, true),   // 继续移动到另一块屏
            (true, true),   // 跨屏途中
            (true, false),  // 松手
        ];
        let mut outcomes: Vec<DragOutcome> = Vec::new();
        for (start, now) in samples {
            outcomes.push(drag_step(start, now));
        }
        assert_eq!(
            outcomes,
            vec![
                DragOutcome::Pending,
                DragOutcome::Pending,
                DragOutcome::Pending,
                DragOutcome::Pending,
                DragOutcome::Finished,
            ]
        );
    }

    // ---- 模式转换：三种模式互转时正常几何不被破坏 ----

    #[test]
    fn mode_transitions_preserve_expanded_geometry() {
        let mut st = WidgetGeometryState::default();
        let expanded = Rect::new(200.0, 150.0, 340.0, 440.0);
        st.expanded = Some(expanded);

        // 收起
        st.mode = WidgetMode::Collapsed;
        assert_eq!(st.expanded, Some(expanded));

        // 从收起打开快速添加（临时 320）
        st.mode = WidgetMode::Expanded;
        st.temp_height = Some(320.0);
        assert_eq!(st.expanded, Some(expanded), "临时高度不改正常几何");

        // 关闭快速添加（清除临时标记）
        st.temp_height = None;
        assert_eq!(st.expanded, Some(expanded));

        // 收纳成圆圈
        st.mode = WidgetMode::Docked;
        st.dock = Some((Edge::Right, 1880.0, 480.0));
        assert_eq!(st.expanded, Some(expanded), "圆圈尺寸不改正常几何");

        // 从圆圈展开回完整面板
        st.mode = WidgetMode::Expanded;
        assert_eq!(st.expanded, Some(expanded));
        assert_eq!(st.expanded.unwrap().w, 340.0);
        assert_eq!(st.expanded.unwrap().h, 440.0);
    }

    // ---- 角落 + 阈值 + 缩放组合 ----

    #[test]
    fn resize_session_never_docks() {
        // 即使用户把右下角拖到贴边附近，若在缩放（尺寸变化）则不吸附
        let wa = wa();
        let start = Rect::new(1500.0, 500.0, 340.0, 440.0);
        // 收缩后右下角贴着工作区右下角（right = 1920, bottom = 1040）
        let end = Rect::new(1620.0, 840.0, 300.0, 200.0);
        assert!(size_changed(&start, &end));
        // 几何本身仍是"贴边"的（距离 0），但调用方会因 size_changed 跳过吸附
        assert_eq!(nearest_edge(&end, &wa).0, 0.0);
        assert_eq!(select_edge(&end, &wa, 12.0), Some(Edge::Right));
    }

    // ---- 混合 DPI：圆圈尺寸按目标显示器 scale 换算 ----

    #[test]
    fn bubble_size_uses_monitor_scale() {
        let wa = wa();
        // 100%：40 物理；150%：60 物理
        let (x, _) = dock_anchor(Edge::Right, 40.0, &wa, (1000.0, 500.0));
        assert_eq!(x, 1880.0);
        let (x, _) = dock_anchor(Edge::Right, 60.0, &wa, (1000.0, 500.0));
        assert_eq!(x, 1860.0);
    }

    // ---- 显示器断开：圆圈锚点不可见判定 ----

    #[test]
    fn dock_anchor_invisible_after_monitor_unplugged() {
        let wa = Rect::new(0.0, 0.0, 1920.0, 1040.0);
        // 圆圈原本在负坐标副屏上
        let bubble = Rect::new(-1920.0, 400.0, 40.0, 40.0);
        assert!(!intersects(&bubble, &wa), "拔屏后锚点不可见，应回退展开");
        // 重新贴边到当前工作区后可见
        let (x, y) = dock_anchor(Edge::Left, 40.0, &wa, (100.0, 400.0));
        assert!(intersects(&Rect::new(x, y, 40.0, 40.0), &wa));
    }
}
