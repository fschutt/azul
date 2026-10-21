//! The app's UI thread never waits for a network answer: a wait on it - a request of its own,
//! or a lock another thread holds across requests - would stop every frame meanwhile. Debug
//! builds say so at the wait.

use std::sync::Arc;

use crate::ui_thread::{is_ui_thread, mark_ui_thread, NetworkLock};

/// The panic's words, of a thread that panicked.
fn said(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default()
}

#[test]
fn only_the_thread_that_was_marked_is_the_ui_thread() {
    let marked = std::thread::spawn(|| {
        mark_ui_thread();
        is_ui_thread()
    })
    .join()
    .unwrap();
    assert!(marked);
    assert!(
        !std::thread::spawn(is_ui_thread).join().unwrap(),
        "another thread is not"
    );
}

#[test]
fn another_thread_takes_a_network_lock_as_any_lock() {
    let lock = NetworkLock::new(1);
    *lock.lock() += 1;
    assert_eq!(*lock.lock(), 2);
}

#[cfg(debug_assertions)]
#[test]
fn the_ui_thread_waiting_for_a_lock_held_across_network_requests_is_a_debug_assertion() {
    let lock = Arc::new(NetworkLock::new(()));
    let ui = std::thread::spawn(move || {
        mark_ui_thread();
        let _held = lock.lock();
    });
    let panic = ui.join().expect_err("the UI thread took a network lock");
    let words = said(panic);
    assert!(words.contains("UI thread"), "{words}");
}
