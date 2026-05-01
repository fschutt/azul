//! Global window registry for multi-window support on Linux (X11)
//!
//! Similar to the Windows and macOS registries, this allows us to look up
//! window pointers by their X11 Window ID, enabling proper event dispatching
//! across multiple windows.

use std::{
    cell::RefCell,
    collections::HashMap,
};

use super::x11::defines::Window as X11WindowId;

thread_local! {
    /// Thread-local registry of all active windows (X11 Window ID -> raw pointer)
    ///
    /// SAFETY: Pointers are valid for the lifetime of the window.
    /// Windows are created and destroyed on the same thread.
    static WINDOW_REGISTRY: RefCell<WindowRegistry> = RefCell::new(WindowRegistry {
        x11_windows: HashMap::new(),
    });
}

struct WindowRegistry {
    /// Maps X11 window ID to window pointer
    x11_windows: HashMap<X11WindowId, *mut super::x11::X11Window>,
}

/// Register an X11 window in the global registry
///
/// # Safety
/// The window pointer must remain valid until it is unregistered.
pub unsafe fn register_x11_window(window_id: X11WindowId, window_ptr: *mut super::x11::X11Window) {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().x11_windows.insert(window_id, window_ptr);
    });
}

/// Unregister an X11 window from the global registry
///
/// Returns the window pointer if it was found.
pub fn unregister_x11_window(window_id: X11WindowId) -> Option<*mut super::x11::X11Window> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow_mut().x11_windows.remove(&window_id)
    })
}

/// Get window pointer for an X11 window ID
///
/// # Safety
/// The returned pointer is only valid if the window is still registered
/// and has not been dropped.
pub unsafe fn get_x11_window(window_id: X11WindowId) -> Option<*mut super::x11::X11Window> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().x11_windows.get(&window_id).copied()
    })
}

/// Get all registered X11 window IDs
pub fn get_all_x11_window_ids() -> Vec<X11WindowId> {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().x11_windows.keys().copied().collect()
    })
}

/// Get count of registered windows
pub fn window_count() -> usize {
    WINDOW_REGISTRY.with(|registry| {
        registry.borrow().x11_windows.len()
    })
}
