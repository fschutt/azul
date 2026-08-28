//! Window registry for multi-window support
//!
//! This module provides a centralized registry for managing multiple Win32 windows.
//! Uses thread-local storage for simplicity and to avoid complex `Rc<RefCell>` patterns.
//!
//! Public API: `register_window`, `unregister_window`, `get_window`,
//! `get_all_window_handles`, `is_empty`, `window_count`,
//! `queue_window_free`, `drain_closed_windows`.
//! Primary consumers are `run.rs` (event loop) and `mod.rs` (window lifecycle).

use std::{cell::RefCell, collections::BTreeMap};

use super::super::common::debug_server::LogCategory;
use super::dlopen::HWND;
use crate::log_debug;

thread_local! {
    /// Thread-local registry of all active windows (HWND -> raw pointer)
    ///
    /// SAFETY: Pointers are valid for the lifetime of the window.
    /// Windows are created and destroyed on the same thread.
    static WINDOW_REGISTRY: RefCell<WindowRegistry> = RefCell::new(WindowRegistry::new());

    /// Windows whose `WM_DESTROY` has run and which have been removed from the
    /// registry, parked for deferred drop.
    ///
    /// They cannot be freed at their unregister site: `WM_DESTROY` is dispatched
    /// *inside* `DestroyWindow`, which itself runs inside the window procedure —
    /// and the event loop holds a `&mut Win32Window` across that dispatch, so
    /// dropping the box there frees the window out from under both frames. The
    /// raw `Box` pointers are parked here and reclaimed by
    /// [`drain_closed_windows`] once the event loop is back at a safe point —
    /// the same deferred-drop strategy the macOS backend uses.
    static PENDING_WINDOW_FREES: RefCell<Vec<*mut super::Win32Window>> =
        RefCell::new(Vec::new());
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
    log_debug!(
        LogCategory::Window,
        "[Windows Registry] Registered window {:p} -> {:p} (total: {})",
        hwnd,
        window_ptr,
        window_count()
    );
}

/// Remove a window from the global registry
pub fn unregister_window(hwnd: HWND) -> Option<*mut super::Win32Window> {
    let result = WINDOW_REGISTRY.with(|registry| registry.borrow_mut().remove(hwnd));
    log_debug!(
        LogCategory::Window,
        "[Windows Registry] Unregistered window {:p} (total: {})",
        hwnd,
        window_count()
    );
    result
}

/// Park a window pointer (already removed from the registry) for deferred drop.
pub fn queue_window_free(window_ptr: *mut super::Win32Window) {
    PENDING_WINDOW_FREES.with(|q| q.borrow_mut().push(window_ptr));
    log_debug!(
        LogCategory::Window,
        "[Windows Registry] Parked window {:p} for deferred drop",
        window_ptr
    );
}

/// Reclaim every window parked by [`queue_window_free`], running
/// `Win32Window`'s destructor (GL context, WebRender renderer, layout window).
///
/// Must only be called from the event loop, i.e. outside the window procedure
/// and outside any live `&mut Win32Window` borrow.
pub fn drain_closed_windows() {
    let pending: Vec<*mut super::Win32Window> =
        PENDING_WINDOW_FREES.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for window_ptr in pending {
        // SAFETY: each pointer came from `Box::into_raw(Box::new(Win32Window))`
        // (run.rs) and was removed from the registry exactly once before being
        // queued, so this is its sole owner and the box is freed exactly once.
        unsafe {
            drop(Box::from_raw(window_ptr));
        }
    }
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
