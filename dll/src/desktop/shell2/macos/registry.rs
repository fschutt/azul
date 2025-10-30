//! Window registry for multi-window support on macOS
//!
//! This module provides a centralized registry for managing multiple macOS windows.
//! Uses thread-local storage for simplicity and to avoid complex Rc<RefCell> patterns.
//! Based on the Windows registry implementation.

use std::{cell::RefCell, collections::BTreeMap};

use objc2::runtime::AnyObject;

thread_local! {
    /// Thread-local registry of all active windows (NSWindow -> raw pointer)
    ///
    /// SAFETY: Pointers are valid for the lifetime of the window.
    /// Windows are created and destroyed on the same thread (main thread).
    static WINDOW_REGISTRY: RefCell<WindowRegistry> = RefCell::new(WindowRegistry::new());
}

/// Window ID wrapper for type safety
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId {
    /// Pointer to NSWindow object (used as unique identifier)
    pub ns_window: *mut AnyObject,
}

impl WindowId {
    pub fn from_ns_window(ns_window: *mut AnyObject) -> Self {
        Self { ns_window }
    }

    pub fn as_i64(&self) -> i64 {
        self.ns_window as i64
    }
}

/// Registry of active windows for the current thread (main thread)
struct WindowRegistry {
    /// Map of NSWindow pointer to raw MacOSWindow pointer
    /// SAFETY: Pointers must remain valid while in the map
    windows: BTreeMap<*mut AnyObject, *mut super::MacOSWindow>,
}

impl WindowRegistry {
    fn new() -> Self {
        Self {
            windows: BTreeMap::new(),
        }
    }

    fn add(&mut self, ns_window: *mut AnyObject, window_ptr: *mut super::MacOSWindow) {
        self.windows.insert(ns_window, window_ptr);
    }

    fn remove(&mut self, ns_window: *mut AnyObject) -> Option<*mut super::MacOSWindow> {
        self.windows.remove(&ns_window)
    }

    fn get(&self, ns_window: *mut AnyObject) -> Option<*mut super::MacOSWindow> {
        self.windows.get(&ns_window).copied()
    }

    fn get_all_ns_windows(&self) -> Vec<*mut AnyObject> {
        self.windows.keys().copied().collect()
    }

    fn get_all_window_ptrs(&self) -> Vec<*mut super::MacOSWindow> {
        self.windows.values().copied().collect()
    }

    fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    fn len(&self) -> usize {
        self.windows.len()
    }
}

/// Add a window to the global registry
///
/// SAFETY: window_ptr must be valid for the lifetime of the window
pub unsafe fn register_window(ns_window: *mut AnyObject, window_ptr: *mut super::MacOSWindow) {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().add(ns_window, window_ptr);
    });
    eprintln!(
        "[macOS Registry] Registered window {:p} -> {:p} (total: {})",
        ns_window,
        window_ptr,
        window_count()
    );
}

/// Remove a window from the global registry
pub fn unregister_window(ns_window: *mut AnyObject) -> Option<*mut super::MacOSWindow> {
    let result = WINDOW_REGISTRY.with(|registry| registry.borrow_mut().remove(ns_window));
    eprintln!(
        "[macOS Registry] Unregistered window {:p} (total: {})",
        ns_window,
        window_count()
    );
    result
}

/// Get a window pointer from the registry
///
/// Returns None if window is not registered
pub fn get_window(ns_window: *mut AnyObject) -> Option<*mut super::MacOSWindow> {
    WINDOW_REGISTRY.with(|registry| registry.borrow().get(ns_window))
}

/// Get all registered NSWindow pointers
pub fn get_all_ns_window_handles() -> Vec<*mut AnyObject> {
    WINDOW_REGISTRY.with(|registry| registry.borrow().get_all_ns_windows())
}

/// Get all registered MacOSWindow pointers
pub fn get_all_window_ptrs() -> Vec<*mut super::MacOSWindow> {
    WINDOW_REGISTRY.with(|registry| registry.borrow().get_all_window_ptrs())
}

/// Check if registry is empty
pub fn is_empty() -> bool {
    WINDOW_REGISTRY.with(|registry| registry.borrow().is_empty())
}

/// Get number of registered windows
pub fn window_count() -> usize {
    WINDOW_REGISTRY.with(|registry| registry.borrow().len())
}
