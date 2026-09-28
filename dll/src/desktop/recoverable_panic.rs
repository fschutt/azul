//! Panics the caller recovers from, so the fatal panic dialog stays out of their way.
//!
//! A panic inside a platform callback that azul catches is not the end of the app, but the panic
//! hook runs BEFORE the unwind and cannot tell: it logged "the program has to exit" and opened a
//! modal dialog. That dialog pumps the message queue, which re-enters the very callback the panic
//! came from - an accesskit tree update, say, whose adapter is still borrowed - and the second
//! panic inside the first aborts the process. A recovered panic is now logged and nothing else.

use core::cell::Cell;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Runs `f`, catching a panic the caller handles itself.
pub fn catch<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let _ = DEPTH.try_with(|depth| depth.set(depth.get() + 1));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let _ = DEPTH.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
    result
}

/// Whether this thread is inside [`catch`], i.e. a panic right now is already handled.
#[must_use]
pub fn in_progress() -> bool {
    DEPTH.try_with(|depth| depth.get() > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{catch, in_progress};

    #[test]
    fn a_caught_panic_is_marked_recoverable_only_inside_the_scope() {
        assert!(!in_progress());
        let seen = catch(|| in_progress());
        assert_eq!(seen.ok(), Some(true));
        assert!(!in_progress());
    }

    #[test]
    fn the_depth_unwinds_with_the_panic() {
        let result = catch(|| {
            let _ = catch(|| panic!("inner"));
            in_progress()
        });
        assert_eq!(result.ok(), Some(true));
        assert!(!in_progress());
    }
}
