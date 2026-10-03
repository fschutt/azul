//! Ending the process from a thread that is not the UI thread (HEADLESS6).
//!
//! `AZ_E2E` prints its verdict on a worker thread (`e2e-result-printer` in
//! `run.rs`), and that thread used to call `exit()` itself - while the UI loop,
//! the window's worker threads and the debug server were still running. libc
//! `exit()` runs the atexit handlers (an instrumented build's profile writer,
//! the system frameworks' teardown) under those threads, which then touch
//! torn-down state: the AZ_E2E host died with SIGSEGV (exit 139) on the
//! instrumented build, and a normal build only got away with it by timing.
//!
//! Now the worker ASKS: it records the exit code here and wakes the loops; a
//! loop that takes exit requests (the headless loop) stops, joins its windows'
//! threads and the debug server, and exits with that code on the UI thread. A
//! backend whose loop does not take requests keeps the old way - the worker
//! exits itself - so nothing waits on a request nobody reads.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

/// A request to end the process with a code, made off the UI thread.
#[derive(Debug, Default)]
pub struct ExitRequest {
    /// The first code asked for (`None`: no request).
    code: Mutex<Option<i32>>,
    /// A run loop that honours requests is running.
    taken_by_loop: AtomicBool,
}

impl ExitRequest {
    /// No request, no loop.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            code: Mutex::new(None),
            taken_by_loop: AtomicBool::new(false),
        }
    }

    /// Ask the process to end with `code`. The FIRST request wins: a later
    /// one (a second failure path racing the first) does not change the
    /// verdict already handed over.
    pub fn request(&self, code: i32) {
        let _ = code;
    }

    /// The code asked for, if any (read by the run loop every turn).
    #[must_use]
    pub fn requested(&self) -> Option<i32> {
        None
    }

    /// A run loop that ends the process on a request is running (called by
    /// that loop before its first turn).
    pub fn loop_takes_requests(&self) {}

    /// Must the asking worker end the process itself? Only when no loop takes
    /// requests - otherwise the loop exits on the UI thread with everything
    /// else stopped, and the worker waits for that.
    #[must_use]
    pub fn worker_must_exit_itself(&self) -> bool {
        true
    }
}

/// The process's one request (`run.rs` asks, the headless loop honours).
pub static EXIT_REQUEST: ExitRequest = ExitRequest::new();

#[cfg(test)]
mod tests {
    use super::ExitRequest;

    #[test]
    fn an_exit_asked_for_off_the_ui_thread_reaches_the_loop_with_its_code() {
        let r = ExitRequest::new();
        assert_eq!(r.requested(), None, "no request yet");
        r.request(1);
        assert_eq!(r.requested(), Some(1), "the loop reads the code asked for");
    }

    #[test]
    fn the_first_exit_code_asked_for_wins() {
        let r = ExitRequest::new();
        r.request(1);
        r.request(0);
        assert_eq!(
            r.requested(),
            Some(1),
            "a later request does not overturn the verdict"
        );
    }

    #[test]
    fn a_worker_leaves_the_exit_to_a_loop_that_takes_requests() {
        let r = ExitRequest::new();
        assert!(
            r.worker_must_exit_itself(),
            "no loop takes requests: the worker ends the process itself"
        );
        r.loop_takes_requests();
        assert!(
            !r.worker_must_exit_itself(),
            "the loop exits on the UI thread, the worker waits"
        );
    }
}
