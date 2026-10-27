//! 从 Windows 的原生移动/缩放会话捕获起终点，窗口回调只采样和投递消息。
//! 禁止在回调中查询 Tauri 窗口、改尺寸或访问数据库，以免阻塞原生移动消息循环。

use crate::dock::Rect;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompletedDrag {
    pub start: Rect,
    pub end: Rect,
    pub start_cursor: Option<(f64, f64)>,
    pub end_cursor: Option<(f64, f64)>,
}

pub(crate) enum DragEvent {
    Finished(CompletedDrag),
    Resnap,
}

#[derive(Default)]
struct DragTracker {
    start: Option<(Rect, Option<(f64, f64)>)>,
    moved: bool,
    resized: bool,
}

impl DragTracker {
    fn begin(&mut self, rect: Rect, cursor: Option<(f64, f64)>) {
        self.start = Some((rect, cursor));
        self.moved = false;
        self.resized = false;
    }
    fn moving(&mut self) {
        self.moved = true;
    }
    fn sizing(&mut self) {
        self.resized = true;
    }
    fn finish(&mut self, end: Rect, end_cursor: Option<(f64, f64)>) -> Option<CompletedDrag> {
        let (start, start_cursor) = self.start.take()?;
        if !self.moved || self.resized {
            return None;
        }
        Some(CompletedDrag {
            start,
            end,
            start_cursor,
            end_cursor,
        })
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::sync::mpsc::Sender;
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        UI::{
            Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetCursorPos, GetWindowRect, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING,
                WM_NCDESTROY, WM_SIZING,
            },
        },
    };

    const SUBCLASS_ID: usize = 0x43484B49;

    struct Hook {
        tracker: DragTracker,
        tx: Sender<DragEvent>,
    }

    fn rect(hwnd: HWND) -> Option<Rect> {
        let mut raw = RECT::default();
        // SAFETY: 句柄来自正在处理的窗口消息；raw 指向本栈的有效可写结构。
        if unsafe { GetWindowRect(hwnd, &mut raw) } == 0 {
            return None;
        }
        Some(Rect::new(
            raw.left as f64,
            raw.top as f64,
            (raw.right - raw.left) as f64,
            (raw.bottom - raw.top) as f64,
        ))
    }

    fn cursor() -> Option<(f64, f64)> {
        let mut point = POINT::default();
        // SAFETY: GetCursorPos 只写入本栈的 point。
        (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x as f64, point.y as f64))
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        if message == WM_NCDESTROY {
            // SAFETY: 回调及数据在所属窗口的 UI 线程上访问；此处是唯一释放点。
            // 先移除回调，再释放数据，随后调用默认过程时不再借用 Hook。
            RemoveWindowSubclass(hwnd, Some(subclass), id);
            drop(Box::from_raw(data as *mut Hook));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
        {
            // SAFETY: data 由 install 成功时移交给窗口，直到 WM_NCDESTROY 才释放。
            // 此借用在 DefSubclassProc 前结束，允许默认过程重入窗口消息。
            let hook = &mut *(data as *mut Hook);
            match message {
                WM_ENTERSIZEMOVE => {
                    hook.tracker = DragTracker::default();
                    if let Some(start) = rect(hwnd) {
                        hook.tracker.begin(start, cursor());
                    }
                }
                WM_MOVING => hook.tracker.moving(),
                WM_SIZING => hook.tracker.sizing(),
                WM_EXITSIZEMOVE => {
                    if let Some(end) = rect(hwnd) {
                        if let Some(drag) = hook.tracker.finish(end, cursor()) {
                            let _ = hook.tx.send(DragEvent::Finished(drag));
                        }
                    } else {
                        hook.tracker = DragTracker::default();
                    }
                }
                _ => {}
            }
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }

    /// 必须在 HWND 所属线程执行（SetWindowSubclass 不支持跨线程安装）。
    fn install(hwnd: HWND, tx: Sender<DragEvent>) {
        let mut existing = 0;
        // SAFETY: 由 run_on_main_thread 保证位于窗口所属线程。
        if unsafe { GetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, &mut existing) } != 0 {
            return;
        }
        let data = Box::into_raw(Box::new(Hook {
            tracker: DragTracker::default(),
            tx,
        }));
        // SAFETY: 成功后数据归回调所有；失败时立即取回释放，不泄漏。
        if unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, data as usize) } == 0 {
            unsafe {
                drop(Box::from_raw(data));
            }
            log::error!("安装原生拖动监听失败: {}", std::io::Error::last_os_error());
        } else {
            log::info!("原生拖动监听已安装");
        }
    }

    pub(super) fn setup(widget: &tauri::WebviewWindow, tx: Sender<DragEvent>) -> tauri::Result<()> {
        let hwnd = widget.hwnd()?.0 as usize;
        widget.run_on_main_thread(move || install(hwnd as HWND, tx))
    }
}

pub(crate) fn setup(
    widget: &tauri::WebviewWindow,
    tx: std::sync::mpsc::Sender<DragEvent>,
) -> tauri::Result<()> {
    #[cfg(target_os = "windows")]
    {
        platform::setup(widget, tx)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (widget, tx);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn panel() -> Rect {
        Rect::new(700.0, 300.0, 510.0, 660.0)
    }
    #[test]
    fn fast_drag_retains_start_even_when_worker_receives_only_after_release() {
        let mut tracker = DragTracker::default();
        tracker.begin(panel(), Some((720.0, 320.0)));
        tracker.moving();
        let end = Rect::new(0.0, 300.0, 510.0, 660.0);
        let result = tracker.finish(end, Some((20.0, 320.0))).unwrap();
        assert_eq!(result.start, panel());
        assert_eq!(result.end, end);
        assert!(tracker.finish(end, None).is_none());
    }
    #[test]
    fn programmatic_moves_do_not_create_drag_sessions() {
        let mut tracker = DragTracker::default();
        tracker.moving();
        assert!(tracker.finish(panel(), None).is_none());
    }
    #[test]
    fn resizing_is_excluded_even_when_position_changes() {
        let mut tracker = DragTracker::default();
        tracker.begin(panel(), None);
        tracker.moving();
        tracker.sizing();
        assert!(tracker
            .finish(Rect::new(0.0, 300.0, 600.0, 660.0), None)
            .is_none());
    }
    #[test]
    fn dpi_size_change_during_move_is_not_manual_resize() {
        let mut tracker = DragTracker::default();
        tracker.begin(panel(), None);
        tracker.moving();
        assert!(tracker
            .finish(Rect::new(0.0, 300.0, 340.0, 440.0), None)
            .is_some());
    }
    #[test]
    fn pause_does_not_finish_until_native_exit() {
        let mut tracker = DragTracker::default();
        tracker.begin(panel(), None);
        tracker.moving();
        // 停顿及多次移动不会消费起点；只在原生退出消息调用 finish。
        for _ in 0..100 {
            tracker.moving();
        }
        assert_eq!(tracker.start.unwrap().0, panel());
        assert!(tracker.finish(panel(), None).is_some());
    }
    #[test]
    fn click_without_a_native_move_is_ignored() {
        let mut tracker = DragTracker::default();
        tracker.begin(panel(), None);
        assert!(tracker.finish(panel(), None).is_none());
    }
}
