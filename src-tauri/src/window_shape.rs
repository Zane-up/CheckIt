//! CSS 圆角只裁切 WebView 内容；原生区域同时裁切 Acrylic 和窗口背景。
//! 在 HWND 所属线程处理缩放/DPI 消息，不访问数据库或 Tauri 同步查询。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    width: i32,
    height: i32,
    diameter: i32,
    circle: bool,
}

fn shape(width: i32, height: i32, dpi: u32, widget: bool) -> Option<Shape> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let scale = dpi.max(96) as f64 / 96.0;
    let bubble = (crate::dock::DOCK_BUBBLE_SIZE * scale).round() as i32;
    let circle = widget && (width - bubble).abs() <= 1 && (height - bubble).abs() <= 1;
    Some(Shape {
        width,
        height,
        circle,
        // 与 rounded-2xl 的 16 CSS px 半径一致；GDI 参数是直径。
        diameter: ((32.0 * scale).round() as i32).min(width).min(height),
    })
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::cell::Cell;
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{CreateEllipticRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN},
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{GetWindowRect, WM_DPICHANGED, WM_NCDESTROY, WM_SIZE},
        },
    };

    const SUBCLASS_ID: usize = 0x43485244;
    struct Hook {
        widget: bool,
        last: Cell<Option<Shape>>,
    }

    unsafe fn region(s: Shape) -> HRGN {
        if s.circle {
            CreateEllipticRgn(0, 0, s.width + 1, s.height + 1)
        } else {
            CreateRoundRectRgn(0, 0, s.width + 1, s.height + 1, s.diameter, s.diameter)
        }
    }

    fn update(hwnd: HWND, hook: &Hook) {
        let mut rect = RECT::default();
        // SAFETY: 在 HWND 所属线程，rect 是可写栈内存；API 返回的是当前物理尺寸。
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return;
        }
        let Some(next) = shape(
            rect.right - rect.left,
            rect.bottom - rect.top,
            unsafe { GetDpiForWindow(hwnd) },
            hook.widget,
        ) else {
            return;
        };
        if hook.last.get() == Some(next) {
            return;
        }
        // SAFETY: 区域尺寸已校验为正数；创建失败不会获得所有权。
        let rgn = unsafe { region(next) };
        if rgn.is_null() {
            log::warn!("创建窗口圆角区域失败: {}", std::io::Error::last_os_error());
            return;
        }
        // SetWindowRgn 会重入窗口消息，提前缓存避免递归刷新相同区域。
        hook.last.set(Some(next));
        // SAFETY: 成功后 HRGN 所有权交给系统，不再删除；失败才自行释放。
        if unsafe { SetWindowRgn(hwnd, rgn, 1) } == 0 {
            hook.last.set(None);
            unsafe {
                DeleteObject(rgn);
            }
            log::warn!("应用窗口圆角区域失败: {}", std::io::Error::last_os_error());
        }
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
            RemoveWindowSubclass(hwnd, Some(subclass), id);
            drop(Box::from_raw(data as *mut Hook));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        if message == WM_SIZE || message == WM_DPICHANGED {
            // SAFETY: 此窗口 UI 线程独占 Hook 的生命周期，直到 WM_NCDESTROY。
            // Cell 允许 SetWindowRgn 重入时只共享借用，不创建重叠的 &mut。
            let mut live = 0;
            if GetWindowSubclass(hwnd, Some(subclass), id, &mut live) != 0 && live == data {
                update(hwnd, &*(data as *const Hook));
            }
        }
        result
    }

    fn install(hwnd: HWND, widget: bool) {
        let mut existing = 0;
        if unsafe { GetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, &mut existing) } != 0 {
            update(hwnd, unsafe { &*(existing as *const Hook) });
            return;
        }
        let data = Box::into_raw(Box::new(Hook {
            widget,
            last: Cell::new(None),
        }));
        // SAFETY: 安装发生在所属 UI 线程；成功后数据移交回调，失败则立即回收。
        if unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, data as usize) } == 0 {
            unsafe {
                drop(Box::from_raw(data));
            }
            log::error!("安装窗口圆角裁切失败: {}", std::io::Error::last_os_error());
        } else {
            update(hwnd, unsafe { &*data });
        }
    }

    pub(super) fn setup(window: &tauri::WebviewWindow) -> tauri::Result<()> {
        let hwnd = window.hwnd()?.0 as usize;
        let widget = window.label() == "widget";
        window.run_on_main_thread(move || install(hwnd as HWND, widget))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::{
            Graphics::Gdi::{CreateRectRgn, GetWindowRgn, PtInRegion},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, SetWindowPos, SWP_NOMOVE, SWP_NOZORDER, WS_POPUP,
            },
        };

        #[test]
        fn hidden_native_window_updates_regions_across_panel_bar_bubble_sizes() {
            // 真实 HWND 区域回归；无 WS_VISIBLE，不展示窗口或触碰用户应用。
            // 子类回调安装依赖主程序的 ComCtl v6 manifest，留给桌面实例验收。
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    340,
                    440,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            struct HiddenWindow(HWND);
            impl Drop for HiddenWindow {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
            let _window = HiddenWindow(hwnd);
            let hook = Hook {
                widget: true,
                last: Cell::new(None),
            };
            update(hwnd, &hook);
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
            for _ in 0..10 {
                for (w, h) in [(340, 440), (340, 64), (40, 40), (340, 440)] {
                    let w = (w as f64 * dpi as f64 / 96.0).round() as i32;
                    let h = (h as f64 * dpi as f64 / 96.0).round() as i32;
                    assert_ne!(
                        unsafe {
                            SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                0,
                                0,
                                w,
                                h,
                                SWP_NOMOVE | SWP_NOZORDER,
                            )
                        },
                        0
                    );
                    update(hwnd, &hook);
                    let rgn = unsafe { CreateRectRgn(0, 0, 0, 0) };
                    assert!(!rgn.is_null());
                    let copied = unsafe { GetWindowRgn(hwnd, rgn) };
                    let outside = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
                        .iter()
                        .all(|&(x, y)| unsafe { PtInRegion(rgn, x, y) } == 0);
                    let center = unsafe { PtInRegion(rgn, w / 2, h / 2) } != 0;
                    unsafe {
                        DeleteObject(rgn);
                    }
                    assert!(copied != 0 && outside && center, "size={w}x{h}");
                }
            }
        }
        #[test]
        fn native_regions_remove_all_four_corners_and_keep_edge_centers() {
            for dpi in [96, 120, 144, 192] {
                for (w, h, widget) in [
                    (340, 440, true),
                    (340, 64, true),
                    (40, 40, true),
                    (420, 380, false),
                ] {
                    let w = w * dpi / 96;
                    let h = h * dpi / 96;
                    let s = shape(w, h, dpi as u32, widget).unwrap();
                    let rgn = unsafe { region(s) };
                    assert!(!rgn.is_null());
                    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
                    let edges = [(w / 2, 0), (0, h / 2), (w - 1, h / 2), (w / 2, h - 1)];
                    let outside = corners
                        .iter()
                        .all(|&(x, y)| unsafe { PtInRegion(rgn, x, y) } == 0);
                    let inside = edges
                        .iter()
                        .all(|&(x, y)| unsafe { PtInRegion(rgn, x, y) } != 0);
                    unsafe {
                        DeleteObject(rgn);
                    }
                    assert!(outside && inside, "dpi={dpi} shape={s:?}");
                }
            }
        }
    }
}

pub(crate) fn setup(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    #[cfg(target_os = "windows")]
    {
        platform::setup(window)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radius_scales_with_dpi_and_bubble_does_not_clip_like_a_panel() {
        for dpi in [96, 120, 144, 192] {
            let scale = dpi as f64 / 96.0;
            let panel = shape((340.0 * scale) as i32, (440.0 * scale) as i32, dpi, true).unwrap();
            assert!(!panel.circle);
            assert_eq!(panel.diameter, (32.0 * scale) as i32);
            let size = (40.0 * scale) as i32;
            assert!(shape(size, size, dpi, true).unwrap().circle);
            assert!(!shape(size, size, dpi, false).unwrap().circle);
        }
        assert!(shape(0, 440, 96, true).is_none());
    }
}
