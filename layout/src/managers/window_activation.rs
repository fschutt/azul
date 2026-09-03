//! Cross-thread "bring this window to the front" requests.
//!
//! A raise is asked for from OFF the event loop - the MPRIS thread when a
//! desktop's media widget is clicked (9h-i-a-ii), and in future anything else
//! that has to reach a window it does not own. Activating a window is a
//! platform call that belongs on the loop's own thread, so the request is
//! parked here and drained once per pass, exactly like the media keys.
//!
//! # The request names a WINDOW, not "the app"
//!
//! An app can have several, and the one that registered the media session is
//! not necessarily the one that happens to be drained first. So the request
//! carries the target's `registry_window_id` - the native handle every backend
//! already keys its window registry by - and a window raises itself only when
//! the id matches. Without that, a two-window app would raise whichever window
//! reached the drain first, which is a coin flip.

use alloc::vec::Vec;

/// Pending raise targets, by `registry_window_id`.
static PENDING: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

/// Ask for a window to be raised, from any thread.
///
/// A target already pending is DROPPED rather than queued twice: raising is
/// idempotent, and a desktop that sends `Raise` twice while the loop is busy
/// means one raise, not two. Same argument the media-key channel makes.
pub fn request_raise(window_id: u64) {
    if window_id == 0 {
        // 0 is "no registry id" (headless, or a backend with no native
        // handle). Queuing it would make every window match.
        return;
    }
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.contains(&window_id) {
        return;
    }
    q.push(window_id);
}

/// Does this window have a raise waiting? Removes it if so.
///
/// Per-window rather than a plain drain, because the loop that asks is one
/// window's and the request may be for another's - taking the whole queue
/// would swallow a sibling's raise.
pub fn take_raise_request(window_id: u64) -> bool {
    if window_id == 0 {
        return false;
    }
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match q.iter().position(|id| *id == window_id) {
        Some(i) => {
            q.remove(i);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PENDING` is a process-global and the harness runs these in parallel.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        let g = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut q = PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        q.clear();
        drop(q);
        g
    }

    #[test]
    fn a_request_is_taken_once_by_the_window_it_names() {
        let _guard = exclusive();
        request_raise(42);
        assert!(take_raise_request(42));
        assert!(
            !take_raise_request(42),
            "a raise must not fire twice for one request"
        );
    }

    /// THE POINT OF KEYING BY WINDOW. A two-window app must not have one
    /// window answer the other's raise - the media session belongs to whichever
    /// window registered it.
    #[test]
    fn another_window_does_not_swallow_the_request() {
        let _guard = exclusive();
        request_raise(7);
        assert!(!take_raise_request(9), "window 9 answered window 7's raise");
        assert!(take_raise_request(7), "and then 7's request was gone");
    }

    #[test]
    fn a_repeated_request_raises_once_and_two_windows_are_independent() {
        let _guard = exclusive();
        request_raise(1);
        request_raise(1);
        assert!(take_raise_request(1));
        assert!(!take_raise_request(1), "a repeat must coalesce");

        request_raise(1);
        request_raise(2);
        assert!(take_raise_request(2));
        assert!(take_raise_request(1), "taking 2 must not drop 1");
    }

    /// `0` means "this backend has no native handle" (headless). Accepting it
    /// would make every window match every request.
    #[test]
    fn the_zero_id_is_never_queued_and_never_matches() {
        let _guard = exclusive();
        request_raise(0);
        assert!(!take_raise_request(0));
        request_raise(5);
        assert!(!take_raise_request(0), "id 0 must not match a real request");
        assert!(take_raise_request(5));
    }
}
