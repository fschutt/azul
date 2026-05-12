//! Global window registry for multi-window support on Linux
//!
//! Similar to the Windows and macOS registries, this allows us to look up
//! window pointers by their window ID, enabling proper event dispatching
//! across multiple windows.
//!
//! Window IDs are platform-specific:
//! - X11: the X11 Window ID (`u64`)
//! - Wayland: the `wl_surface` pointer cast to `u64` (unique per window)

use std::{
    cell::RefCell,
    collections::HashMap,
};

use super::LinuxWindow;

/// Opaque window identifier used as a registry key.
pub type LinuxWindowId = u64;

thread_local! {
    /// Thread-local registry of all active windows (window ID -> raw pointer)
    ///
    /// SAFETY: Pointers are valid for the lifetime of the window.
    /// Windows are created and destroyed on the same thread.
    static WINDOW_REGISTRY: RefCell<WindowRegistry> = RefCell::new(WindowRegistry {
        windows: HashMap::new(),
    });
}

struct WindowRegistry {
    windows: HashMap<LinuxWindowId, *mut LinuxWindow>,
}

/// Register a Linux window in the global registry.
///
/// # Safety
/// The window pointer must remain valid until it is unregistered.
pub unsafe fn register_window(window_id: LinuxWindowId, window_ptr: *mut LinuxWindow) {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().windows.insert(window_id, window_ptr);
    });
}

/// Unregister a Linux window from the global registry.
///
/// Returns the window pointer if it was found.
pub fn unregister_window(window_id: LinuxWindowId) -> Option<*mut LinuxWindow> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().windows.remove(&window_id)
    })
}

/// Get window pointer for a window ID.
///
/// # Safety
/// The returned pointer is only valid if the window is still registered
/// and has not been dropped.
pub unsafe fn get_window(window_id: LinuxWindowId) -> Option<*mut LinuxWindow> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().windows.get(&window_id).copied()
    })
}

/// Get all registered window IDs.
pub fn get_all_window_ids() -> Vec<LinuxWindowId> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().windows.keys().copied().collect()
    })
}

/// Get count of registered windows.
pub fn window_count() -> usize {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().windows.len()
    })
}
