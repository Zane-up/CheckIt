//! 原生窗口与持久化状态的提交边界。所有调用者必须持有窗口转换锁。

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layout {
    pub min: (f64, f64),
    pub size: (f64, f64),
    pub position: (i32, i32),
    pub resizable: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Step {
    Min(Option<(f64, f64)>),
    Resizable(bool),
    Size((f64, f64)),
    Position((i32, i32)),
}

pub(crate) trait WindowOps {
    fn apply(&self, step: Step) -> Result<(), String>;
    fn verify(&self, layout: Layout) -> Result<(), String>;
}

#[derive(Debug)]
pub(crate) struct TransitionError {
    message: String,
    pub rollback_failed: bool,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// 原生操作全部受理后才提交数据库/内存。失败时尽力恢复每一项，保留回滚错误。
pub(crate) fn transition(
    window: &impl WindowOps,
    before: Layout,
    target: Layout,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), TransitionError> {
    let result = (|| {
        for step in [
            Step::Min(Some(target.min)),
            Step::Resizable(target.resizable),
            Step::Size(target.size),
            Step::Position(target.position),
        ] {
            window.apply(step).map_err(|e| format!("{step:?}: {e}"))?;
        }
        window.verify(target)?;
        commit()
    })();
    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        // 先解除约束，确保从 40px 圆圈也能恢复原尺寸；一项失败仍尝试其余项。
        for step in [
            Step::Min(None),
            Step::Resizable(true),
            Step::Size(before.size),
            Step::Position(before.position),
            Step::Min(Some(before.min)),
            Step::Resizable(before.resizable),
        ] {
            if let Err(e) = window.apply(step) {
                rollback_errors.push(format!("{step:?}: {e}"));
            }
        }
        if let Err(e) = window.verify(before) {
            rollback_errors.push(e);
        }
        let rollback_failed = !rollback_errors.is_empty();
        let message = if rollback_failed {
            format!("{error}; 原生窗口回滚失败: {}", rollback_errors.join("; "))
        } else {
            error
        };
        return Err(TransitionError {
            message,
            rollback_failed,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const PANEL: Layout = Layout {
        min: (300.0, 200.0),
        size: (340.0, 440.0),
        position: (100, 200),
        resizable: true,
    };
    const BUBBLE: Layout = Layout {
        min: (40.0, 40.0),
        size: (40.0, 40.0),
        position: (1880, 200),
        resizable: false,
    };
    struct FakeWindow {
        layout: RefCell<Layout>,
        calls: Cell<usize>,
        fail_at: Vec<usize>,
    }
    impl WindowOps for FakeWindow {
        fn apply(&self, step: Step) -> Result<(), String> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            if self.fail_at.contains(&call) {
                return Err("injected".into());
            }
            let mut layout = self.layout.borrow_mut();
            match step {
                Step::Min(Some(min)) => layout.min = min,
                Step::Min(None) => layout.min = (0.0, 0.0),
                Step::Resizable(value) => layout.resizable = value,
                Step::Size(size) => layout.size = size,
                Step::Position(position) => layout.position = position,
            }
            Ok(())
        }
        fn verify(&self, layout: Layout) -> Result<(), String> {
            if *self.layout.borrow() == layout {
                Ok(())
            } else {
                Err("窗口实际状态与目标不符".into())
            }
        }
    }
    #[test]
    fn every_native_failure_restores_layout_without_committing() {
        // 同时覆盖展开、收纳、收起和临时高度恢复的共同提交边界。
        let collapsed = Layout {
            min: (300.0, 64.0),
            size: (340.0, 64.0),
            ..PANEL
        };
        let temporary = Layout {
            size: (340.0, 520.0),
            ..PANEL
        };
        for (before, target) in [
            (PANEL, BUBBLE),
            (BUBBLE, PANEL),
            (PANEL, collapsed),
            (temporary, PANEL),
        ] {
            for fail_at in 1..=4 {
                let window = FakeWindow {
                    layout: RefCell::new(before),
                    calls: Cell::new(0),
                    fail_at: vec![fail_at],
                };
                let committed = Cell::new(false);
                assert!(transition(&window, before, target, || {
                    committed.set(true);
                    Ok(())
                })
                .is_err());
                assert!(!committed.get());
                assert_eq!(*window.layout.borrow(), before);
            }
        }
    }
    #[test]
    fn database_failure_rolls_back_native_layout() {
        let window = FakeWindow {
            layout: RefCell::new(PANEL),
            calls: Cell::new(0),
            fail_at: vec![],
        };
        assert!(transition(&window, PANEL, BUBBLE, || Err("database failure".into())).is_err());
        assert_eq!(*window.layout.borrow(), PANEL);
    }
    #[test]
    fn successful_transition_commits_once() {
        let window = FakeWindow {
            layout: RefCell::new(PANEL),
            calls: Cell::new(0),
            fail_at: vec![],
        };
        let count = Cell::new(0);
        transition(&window, PANEL, BUBBLE, || {
            count.set(count.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(count.get(), 1);
        assert_eq!(*window.layout.borrow(), BUBBLE);
    }
    #[test]
    fn rollback_failure_is_reported_and_remaining_steps_are_attempted() {
        let window = FakeWindow {
            layout: RefCell::new(PANEL),
            calls: Cell::new(0),
            fail_at: vec![3, 4],
        };
        let error = transition(&window, PANEL, BUBBLE, || Ok(())).unwrap_err();
        assert!(error.rollback_failed);
        assert!(error.to_string().contains("回滚失败"));
        assert_eq!(window.calls.get(), 9);
        assert_eq!(*window.layout.borrow(), PANEL);
    }

    #[test]
    fn accepted_but_ineffective_native_operation_does_not_commit() {
        struct SilentFailure(FakeWindow);
        impl WindowOps for SilentFailure {
            fn apply(&self, step: Step) -> Result<(), String> {
                if matches!(step, Step::Size(size) if size == BUBBLE.size) {
                    return Ok(());
                }
                self.0.apply(step)
            }
            fn verify(&self, layout: Layout) -> Result<(), String> {
                self.0.verify(layout)
            }
        }
        let window = SilentFailure(FakeWindow {
            layout: RefCell::new(PANEL),
            calls: Cell::new(0),
            fail_at: vec![],
        });
        let committed = Cell::new(false);
        assert!(transition(&window, PANEL, BUBBLE, || {
            committed.set(true);
            Ok(())
        })
        .is_err());
        assert!(!committed.get());
        assert_eq!(*window.0.layout.borrow(), PANEL);
    }
}
