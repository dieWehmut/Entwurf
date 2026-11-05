// Kept independent of the native runtime so recovery and guarded close are testable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Show,
    Unminimize,
    Focus,
    Close,
}
pub fn restore<E>(mut apply: impl FnMut(Action) -> Result<(), E>, close: bool) -> Result<(), E> {
    apply(Action::Show)?;
    apply(Action::Unminimize)?;
    apply(Action::Focus)?;
    if close {
        apply(Action::Close)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_same_window_before_requesting_guarded_close() {
        let mut calls = Vec::new();
        restore(
            |a| {
                calls.push(a);
                Ok::<_, ()>(())
            },
            true,
        )
        .unwrap();
        assert_eq!(
            calls,
            [
                Action::Show,
                Action::Unminimize,
                Action::Focus,
                Action::Close
            ]
        );
    }
    #[test]
    fn restore_does_not_close() {
        let mut calls = Vec::new();
        restore(
            |a| {
                calls.push(a);
                Ok::<_, ()>(())
            },
            false,
        )
        .unwrap();
        assert_eq!(calls, [Action::Show, Action::Unminimize, Action::Focus]);
    }
    #[test]
    fn failure_prevents_close_request() {
        for failure in [Action::Show, Action::Unminimize, Action::Focus] {
            let mut calls = Vec::new();
            let result = restore(
                |a| {
                    calls.push(a);
                    if a == failure {
                        Err(())
                    } else {
                        Ok(())
                    }
                },
                true,
            );
            assert!(result.is_err());
            assert!(!calls.contains(&Action::Close));
        }
    }
}
