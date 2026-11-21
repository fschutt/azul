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

use super::shared_queue::SharedQueue;

/// The raise requests of one app, by `registry_window_id`: the app's windows and the
/// threads that ask for a raise share one (a clone is another handle on it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RaiseRequests {
    pending: SharedQueue<u64>,
}

impl RaiseRequests {
    /// A new set of requests of its own, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask for a window to be raised, from any thread.
    ///
    /// A target already pending is DROPPED rather than queued twice: raising is
    /// idempotent, and a desktop that sends `Raise` twice while the loop is busy
    /// means one raise, not two. Same argument the media-key queue makes.
    pub fn request_raise(&self, window_id: u64) {
        if window_id == 0 {
            // 0 is "no registry id" (headless, or a backend with no native
            // handle). Queuing it would make every window match.
            return;
        }
        let _ = self.pending.push_unique_up_to(window_id, usize::MAX);
    }

    /// Does this window have a raise waiting? Removes it if so.
    ///
    /// Per-window rather than a plain drain, because the loop that asks is one
    /// window's and the request may be for another's - taking the whole queue
    /// would swallow a sibling's raise.
    #[must_use]
    pub fn take_raise_request(&self, window_id: u64) -> bool {
        if window_id == 0 {
            return false;
        }
        self.pending.take_first(|id| *id == window_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_taken_once_by_the_window_it_names() {
        let requests = RaiseRequests::new();
        requests.request_raise(42);
        assert!(requests.take_raise_request(42));
        assert!(
            !requests.take_raise_request(42),
            "a raise must not fire twice for one request"
        );
    }

    /// THE POINT OF KEYING BY WINDOW. A two-window app must not have one
    /// window answer the other's raise - the media session belongs to whichever
    /// window registered it.
    #[test]
    fn another_window_does_not_swallow_the_request() {
        let requests = RaiseRequests::new();
        requests.request_raise(7);
        assert!(
            !requests.take_raise_request(9),
            "window 9 answered window 7's raise"
        );
        assert!(requests.take_raise_request(7), "and then 7's request was gone");
    }

    #[test]
    fn a_repeated_request_raises_once_and_two_windows_are_independent() {
        let requests = RaiseRequests::new();
        requests.request_raise(1);
        requests.request_raise(1);
        assert!(requests.take_raise_request(1));
        assert!(!requests.take_raise_request(1), "a repeat must coalesce");

        requests.request_raise(1);
        requests.request_raise(2);
        assert!(requests.take_raise_request(2));
        assert!(requests.take_raise_request(1), "taking 2 must not drop 1");
    }

    /// `0` means "this backend has no native handle" (headless). Accepting it
    /// would make every window match every request.
    #[test]
    fn the_zero_id_is_never_queued_and_never_matches() {
        let requests = RaiseRequests::new();
        requests.request_raise(0);
        assert!(!requests.take_raise_request(0));
        requests.request_raise(5);
        assert!(
            !requests.take_raise_request(0),
            "id 0 must not match a real request"
        );
        assert!(requests.take_raise_request(5));
    }

    /// The requests belong to the app: another app's windows never see them.
    #[test]
    fn the_requests_of_one_app_are_not_another_apps() {
        let ours = RaiseRequests::new();
        let theirs = RaiseRequests::new();
        ours.request_raise(3);
        assert!(!theirs.take_raise_request(3));
        assert!(ours.clone().take_raise_request(3), "a clone is the same set");
    }
}
