//! Thread callback information and utilities for azul-layout
//!
//! This module provides thread-related callback structures for background tasks
//! that need to interact with the UI thread and query layout information.

use azul_core::callbacks::{RefAny, Update};

use crate::callbacks::CallbackInfo;

/// Callback that runs when a thread receives a `WriteBack` message
///
/// This callback runs on the main UI thread and has access to:
/// - The thread's original data
/// - Data sent back from the background thread
/// - Full CallbackInfo for DOM queries and UI updates
pub type WriteBackCallbackType = extern "C" fn(
    /* original thread data */ &mut RefAny,
    /* data to write back */ &mut RefAny,
    /* callback info */ &mut CallbackInfo,
) -> Update;

/// Callback that can run when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback {
    pub cb: WriteBackCallbackType,
}

impl WriteBackCallback {
    /// Create a new WriteBackCallback
    pub fn new(cb: WriteBackCallbackType) -> Self {
        Self { cb }
    }

    /// Invoke the callback
    pub fn invoke(
        &self,
        thread_data: &mut RefAny,
        writeback_data: &mut RefAny,
        callback_info: &mut CallbackInfo,
    ) -> Update {
        (self.cb)(thread_data, writeback_data, callback_info)
    }
}

impl core::fmt::Debug for WriteBackCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "WriteBackCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for WriteBackCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

impl PartialEq for WriteBackCallback {
    fn eq(&self, other: &Self) -> bool {
        self.cb as usize == other.cb as usize
    }
}

impl Eq for WriteBackCallback {}

impl PartialOrd for WriteBackCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as usize).partial_cmp(&(other.cb as usize))
    }
}

impl Ord for WriteBackCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as usize).cmp(&(other.cb as usize))
    }
}

impl core::hash::Hash for WriteBackCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as usize).hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn test_callback(
        _thread_data: &mut RefAny,
        _writeback_data: &mut RefAny,
        _callback_info: &mut CallbackInfo,
    ) -> Update {
        Update::DoNothing
    }

    #[test]
    fn test_writeback_callback_creation() {
        let callback = WriteBackCallback::new(test_callback);
        assert_eq!(callback.cb as usize, test_callback as usize);
    }

    #[test]
    fn test_writeback_callback_clone() {
        let callback = WriteBackCallback::new(test_callback);
        let cloned = callback.clone();
        assert_eq!(callback, cloned);
    }

    #[test]
    fn test_writeback_callback_hash() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let callback = WriteBackCallback::new(test_callback);
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        callback.hash(&mut hasher1);
        callback.clone().hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }
}
