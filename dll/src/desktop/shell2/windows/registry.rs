//! Window registry for multi-window support
//!
//! This module provides a centralized registry for managing multiple Win32 windows.
//! Uses thread-local storage for simplicity and to avoid complex Rc<RefCell> patterns.

use std::{cell::RefCell, collections::BTreeMap};

use super::dlopen::HWND;

thread_local! {
    /// Thread-local registry of all active windows (HWND -> raw pointer)
    ///
    /// SAFETY: Pointers are valid for the lifetime of the window.
    /// Windows are created and destroyed on the same thread.
    static WINDOW_REGISTRY: RefCell<WindowRegistry> = RefCell::new(WindowRegistry::new());
}

/// Window ID wrapper for type safety
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId {
    pub hwnd: HWND,
}

impl WindowId {
    pub fn from_hwnd(hwnd: HWND) -> Self {
        Self { hwnd }
    }

    pub fn as_i64(&self) -> i64 {
        self.hwnd as i64
    }
}

/// Registry of active windows for the current thread
struct WindowRegistry {
    /// Map of HWND to raw window pointer
    /// SAFETY: Pointers must remain valid while in the map
    windows: BTreeMap<HWND, *mut super::Win32Window>,
}

impl WindowRegistry {
    fn new() -> Self {
        Self {
            windows: BTreeMap::new(),
        }
    }

    fn add(&mut self, hwnd: HWND, window_ptr: *mut super::Win32Window) {
        self.windows.insert(hwnd, window_ptr);
    }

    fn remove(&mut self, hwnd: HWND) -> Option<*mut super::Win32Window> {
        self.windows.remove(&hwnd)
    }

    fn get(&self, hwnd: HWND) -> Option<*mut super::Win32Window> {
        self.windows.get(&hwnd).copied()
    }

    fn get_all_hwnds(&self) -> Vec<HWND> {
        self.windows.keys().copied().collect()
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
pub unsafe fn register_window(hwnd: HWND, window_ptr: *mut super::Win32Window) {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().add(hwnd, window_ptr);
    });
}

/// Remove a window from the global registry
pub fn unregister_window(hwnd: HWND) -> Option<*mut super::Win32Window> {
    WINDOW_REGISTRY.with(|registry| registry.borrow_mut().remove(hwnd))
}

/// Get a window pointer from the registry
///
/// Returns None if window is not registered
pub fn get_window(hwnd: HWND) -> Option<*mut super::Win32Window> {
    WINDOW_REGISTRY.with(|registry| registry.borrow().get(hwnd))
}

/// Get all registered window handles
pub fn get_all_window_handles() -> Vec<HWND> {
    WINDOW_REGISTRY.with(|registry| registry.borrow().get_all_hwnds())
}

/// Check if registry is empty
pub fn is_empty() -> bool {
    WINDOW_REGISTRY.with(|registry| registry.borrow().is_empty())
}

/// Get number of registered windows
pub fn window_count() -> usize {
    WINDOW_REGISTRY.with(|registry| registry.borrow().len())
}
