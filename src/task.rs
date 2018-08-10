//! Simplistic async IO / Task system

use std::{
    sync::{Arc, Mutex, Weak},
    thread::{spawn, JoinHandle},
};
use {
    traits::Layout,
};

pub struct Task {
    // Task is in progress
    join_handle: Option<JoinHandle<()>>,
    dropcheck: Weak<()>,
}

impl Task {
    pub fn new<T>(
        app_state: &Arc<Mutex<T>>,
        callback: fn(Arc<Mutex<T>>, Arc<()>))
    -> Self
    where T: Layout + Send + 'static
    {
        let thread_check = Arc::new(());
        let thread_weak = Arc::downgrade(&thread_check);
        let app_state_clone = app_state.clone();

        let thread_handle = spawn(move || {
            callback(app_state_clone, thread_check)
        });

        Self {
            join_handle: Some(thread_handle),
            dropcheck: thread_weak,
        }
    }

    /// Returns true if the task has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(thread_handle) = self.join_handle.take() {
            let _ = thread_handle.join().unwrap();
        }
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_task_file() {

}