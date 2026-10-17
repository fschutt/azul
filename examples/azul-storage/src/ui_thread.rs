//! The app's UI thread never waits for a network answer.
//!
//! In the apps a request goes out through `AzulTransport` and its answer comes back through
//! the UI thread's pump. So the UI thread waiting for an answer never gets one: neither for a
//! request of its own, nor for a lock another thread holds while that thread waits for its
//! answer. The app marks its UI thread ([`mark_ui_thread`]; `AzulTransport` marks the thread
//! its answers arrive on as well), and a debug build panics at such a wait, every time it
//! happens and not only when it happens to race.

use std::{
    cell::Cell,
    fmt,
    sync::{Mutex, MutexGuard, PoisonError},
};

thread_local! {
    static UI_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// Marks this thread as the one whose pump delivers the network answers.
pub fn mark_ui_thread() {
    UI_THREAD.with(|ui| ui.set(true));
}

/// Whether this thread was marked as the UI thread.
#[must_use]
pub fn is_ui_thread() -> bool {
    UI_THREAD.with(Cell::get)
}

/// Panics in a debug build when this is the UI thread: `what` waits for a network answer,
/// which only this thread's pump delivers.
#[track_caller]
pub fn assert_may_wait(what: &str) {
    debug_assert!(
        !is_ui_thread(),
        "{what} waits for a network answer on the UI thread, but that answer comes through this \
         thread's pump: run it on a worker thread"
    );
}

/// A lock that is held across network requests: the thread holding it waits for an answer
/// while it holds it. The UI thread never takes it (a debug build panics when it does), or it
/// would wait for the pump it stops.
pub struct NetworkLock<T> {
    inner: Mutex<T>,
}

impl<T> NetworkLock<T> {
    pub const fn new(value: T) -> Self {
        NetworkLock {
            inner: Mutex::new(value),
        }
    }

    /// The value, once the thread holding it lets go (also after that thread panicked).
    #[track_caller]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        assert_may_wait("taking a lock held across network requests");
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The value, with no other thread to wait for.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> fmt::Debug for NetworkLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NetworkLock { .. }")
    }
}
