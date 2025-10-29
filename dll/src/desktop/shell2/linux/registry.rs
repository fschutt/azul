//! Global window registry for multi-window support on Linux
//!
//! Similar to Windows registry, this allows us to look up window pointers
//! by their X11 Window ID or Wayland surface, enabling proper event dispatching
//! across multiple windows.
//!
//! This also stores owned menu windows to prevent them from being dropped.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use once_cell::sync::Lazy;

use super::x11::defines::Window as X11WindowId;

/// Global registry mapping X11 window IDs to window pointers
static WINDOW_REGISTRY: Lazy<Mutex<WindowRegistry>> = Lazy::new(|| {
    Mutex::new(WindowRegistry {
        x11_windows: HashMap::new(),
        owned_menu_windows: Vec::new(),
    })
});

struct WindowRegistry {
    /// Maps X11 window ID to window pointer
    x11_windows: HashMap<X11WindowId, *mut super::x11::X11Window>,
    /// Stores owned menu windows to prevent them from being dropped
    /// These windows are also registered in x11_windows for event routing
    owned_menu_windows: Vec<Box<super::x11::X11Window>>,
}

// SAFETY: The registry is only accessed while the main thread is running,
// and window pointers are only valid while they're registered.
unsafe impl Send for WindowRegistry {}
unsafe impl Sync for WindowRegistry {}

/// Register an X11 window in the global registry
///
/// # Safety
/// The window pointer must remain valid until it is unregistered.
pub unsafe fn register_x11_window(window_id: X11WindowId, window_ptr: *mut super::x11::X11Window) {
    if let Ok(mut registry) = WINDOW_REGISTRY.lock() {
        registry.x11_windows.insert(window_id, window_ptr);
    }
}

/// Unregister an X11 window from the global registry
///
/// Returns the window pointer if it was found.
pub fn unregister_x11_window(window_id: X11WindowId) -> Option<*mut super::x11::X11Window> {
    if let Ok(mut registry) = WINDOW_REGISTRY.lock() {
        registry.x11_windows.remove(&window_id)
    } else {
        None
    }
}

/// Get window pointer for an X11 window ID
///
/// # Safety
/// The returned pointer is only valid if the window is still registered
/// and has not been dropped.
pub unsafe fn get_x11_window(window_id: X11WindowId) -> Option<*mut super::x11::X11Window> {
    if let Ok(registry) = WINDOW_REGISTRY.lock() {
        registry.x11_windows.get(&window_id).copied()
    } else {
        None
    }
}

/// Get all registered X11 window IDs
pub fn get_all_x11_window_ids() -> Vec<X11WindowId> {
    if let Ok(registry) = WINDOW_REGISTRY.lock() {
        registry.x11_windows.keys().copied().collect()
    } else {
        Vec::new()
    }
}

/// Get count of registered windows
pub fn window_count() -> usize {
    if let Ok(registry) = WINDOW_REGISTRY.lock() {
        registry.x11_windows.len()
    } else {
        0
    }
}

/// Register an owned menu window
///
/// This takes ownership of the window and stores it to prevent it from being dropped.
/// The window is automatically registered in the x11_windows map.
///
/// # Safety
/// This function assumes the window was created as a menu window and should be
/// managed by the registry until explicitly closed.
pub fn register_owned_menu_window(mut window: Box<super::x11::X11Window>) {
    let window_id = window.window;
    let window_ptr = &mut *window as *mut _;

    if let Ok(mut registry) = WINDOW_REGISTRY.lock() {
        // Register pointer for event routing
        registry.x11_windows.insert(window_id, window_ptr);
        // Store owned window to prevent drop
        registry.owned_menu_windows.push(window);
    }
}

/// Remove and drop an owned menu window
///
/// This closes the window and removes it from both the owned windows list
/// and the x11_windows map.
pub fn close_owned_menu_window(window_id: X11WindowId) -> bool {
    use super::super::common::PlatformWindow;

    if let Ok(mut registry) = WINDOW_REGISTRY.lock() {
        // Find and remove from owned windows
        if let Some(pos) = registry
            .owned_menu_windows
            .iter()
            .position(|w| w.window == window_id)
        {
            let mut window = registry.owned_menu_windows.remove(pos);
            // Close the window explicitly
            PlatformWindow::close(&mut *window);
            // Remove from pointer registry
            registry.x11_windows.remove(&window_id);
            // Window is dropped here
            true
        } else {
            false
        }
    } else {
        false
    }
}
