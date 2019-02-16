//! Simplistic async IO / Task system

use std::{
    sync::{Arc, Mutex, Weak},
    thread::{spawn, JoinHandle},
};
use daemon::{Daemon, DaemonId};

pub struct Task<T> {
    // Task is in progress
    join_handle: Option<JoinHandle<()>>,
    dropcheck: Weak<()>,
    /// Daemons that run directly after completion of this task
    pub(crate) after_completion_daemons: Vec<(DaemonId, Daemon<T>)>
}

impl<T> Task<T> {
    pub fn new<U: Send + 'static>(app_state: &Arc<Mutex<U>>, callback: fn(Arc<Mutex<U>>, Arc<()>)) -> Self {
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
    pub(crate) fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }

    /// Stores daemons that will run after the task has finished.
    ///
    /// Often necessary to "clean up" or copy data from the background task into the UI.
    #[inline]
    pub fn then(mut self, deamons: &[(DaemonId, Daemon<T>)]) -> Self {
        self.after_completion_daemons.extend(deamons.iter().cloned());
        self
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let Some(thread_handle) = self.join_handle.take() {
            let _ = thread_handle.join().unwrap();
        }
    }
}
