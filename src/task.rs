//! Preliminary async IO / Task system

use app_state::AppState;
use traits::Layout;
use std::sync::{Arc, Mutex, Weak};
use std::thread::{spawn, JoinHandle};

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