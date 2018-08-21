//! Simplistic async IO / Task system

use std::{
    sync::{Arc, Mutex, Weak},
    thread::{spawn, JoinHandle},
};
use {
    app_state::Daemon,
    traits::Layout,
};

/// Should a daemon terminate or not - used to remove active daemons
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TerminateDaemon {
    Terminate,
    Continue,
}

pub struct Task<T: Layout> {
    // Task is in progress
    join_handle: Option<JoinHandle<()>>,
    dropcheck: Weak<()>,
    /// Deamons that run directly after completion of this task
    pub(crate) after_completion_daemons: Vec<Daemon<T>>
}

impl<T: Layout> Task<T> {
    pub fn new<U>(
        app_state: &Arc<Mutex<U>>,
        callback: fn(Arc<Mutex<U>>, Arc<()>))
    -> Self
    where U: Send + 'static
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
            after_completion_daemons: Vec::new(),
        }
    }

    /// Returns true if the task has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }

    #[inline]
    pub fn then(mut self, deamons: &[Daemon<T>]) -> Self {
        self.after_completion_daemons.extend(deamons.iter().cloned());
        self
    }
}

impl<T: Layout> Drop for Task<T> {
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