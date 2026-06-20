//! Linux windowing backend selector.
//!
//! Automatically selects between X11 and Wayland at runtime,
//! or allows manual selection via environment variable.
//! See [`BackendType`] and [`LinuxWindow::select_backend`] for the selection logic.

pub mod common;
pub mod registry;
pub mod resources;
pub(crate) mod system_style;
pub mod timer;

/// DBus dynamic loading (for GNOME menus)
pub mod dbus;

/// GNOME native menu integration (DBus with dlopen)
pub mod gnome_menu;

/// Wayland implementation
pub mod wayland;
/// X11 implementation
pub mod x11;

use std::{cell::RefCell, sync::Arc};

use azul_core::refany::RefAny;
use azul_layout::window_state::WindowCreateOptions;
pub use resources::AppResources;

use super::common::WindowError;
use super::common::event::SharedUndoManager;

use super::common::debug_server::LogCategory;
use crate::{log_info, log_warn};

/// Linux window - supports both X11 and Wayland
pub enum LinuxWindow {
    X11(x11::X11Window),
    Wayland(wayland::WaylandWindow),
}

/// The event type for Linux windows.
#[derive(Debug, Clone, Copy)]
pub enum LinuxEvent {
    X11(x11::X11Event),
    Wayland(wayland::WaylandEvent),
}

// Lifecycle methods (formerly on PlatformWindow V1 trait)
impl LinuxWindow {
    pub fn poll_event(&mut self) -> Option<LinuxEvent> {
        match self {
            LinuxWindow::X11(w) => w.poll_event().map(LinuxEvent::X11),
            LinuxWindow::Wayland(w) => w.poll_event().map(LinuxEvent::Wayland),
        }
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        match self {
            LinuxWindow::X11(w) => w.present(),
            LinuxWindow::Wayland(w) => w.present(),
        }
    }

    pub fn is_open(&self) -> bool {
        match self {
            LinuxWindow::X11(w) => w.is_open(),
            LinuxWindow::Wayland(w) => w.is_open(),
        }
    }

    /// `true` if a callback requested this window close (`flags.close_requested`).
    pub fn close_requested(&self) -> bool {
        match self {
            LinuxWindow::X11(w) => w.close_requested(),
            LinuxWindow::Wayland(w) => w.close_requested(),
        }
    }

    pub fn close(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.close(),
            LinuxWindow::Wayland(w) => w.close(),
        }
    }

    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.process_accessibility_actions(),
            LinuxWindow::Wayland(w) => w.process_accessibility_actions(),
        }
    }

    pub fn request_redraw(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.request_redraw(),
            LinuxWindow::Wayland(w) => w.request_redraw(),
        }
    }

    pub fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        match self {
            LinuxWindow::X11(w) => w.sync_clipboard(clipboard_manager),
            LinuxWindow::Wayland(w) => w.sync_clipboard(clipboard_manager),
        }
    }
}

impl LinuxWindow {
    /// Create a new Linux window with shared resources.
    ///
    /// Allows sharing font cache, app data, and system styling across windows.
    pub fn new_with_resources(
        options: WindowCreateOptions,
        app_data: Arc<std::cell::RefCell<RefAny>>,
        undo_manager: SharedUndoManager,
        resources: Arc<AppResources>,
    ) -> Result<Self, WindowError> {
        // Clone resources and update app_data + undo_manager so every window
        // created from these resources shares the App's owned manager.
        let mut updated = (*resources).clone();
        updated.app_data = app_data;
        updated.undo_manager = undo_manager;
        let resources = Arc::new(updated);

        match Self::select_backend()? {
            BackendType::X11 => Ok(LinuxWindow::X11(x11::X11Window::new_with_resources(
                options, resources,
            )?)),
            BackendType::Wayland => {
                // F3: try Wayland, but fall back to X11 if it fails to initialise
                // (missing/old libwayland, no compositor, etc.) instead of aborting
                // the whole app. Only a HARD override (AZ_BACKEND=wayland) propagates
                // the error, so a user who explicitly asked for Wayland still sees it.
                match wayland::WaylandWindow::new(options.clone(), resources.clone()) {
                    Ok(w) => Ok(LinuxWindow::Wayland(w)),
                    Err(e) => {
                        let forced = std::env::var("AZ_WINDOW")
                            .or_else(|_| std::env::var("AZ_BACKEND"))
                            .map(|b| b.eq_ignore_ascii_case("wayland"))
                            .unwrap_or(false);
                        if forced {
                            return Err(e);
                        }
                        log_warn!(
                            LogCategory::Platform,
                            "[Linux] Wayland init failed ({:?}); falling back to X11",
                            e
                        );
                        Ok(LinuxWindow::X11(x11::X11Window::new_with_resources(
                            options, resources,
                        )?))
                    }
                }
            }
        }
    }

    /// Detect and select the windowing backend (X11 vs Wayland).
    ///
    /// This is a SEPARATE axis from the render backend (CPU vs GPU), which is read from
    /// `AZ_BACKEND` by [`AzBackend::resolve`]. The two used to collide in a single
    /// `AZ_BACKEND` variable, so "X11 + CPU" could not be expressed. Now:
    ///   - `AZ_WINDOW=x11|wayland|auto` selects the windowing backend (highest priority).
    ///   - `AZ_BACKEND=x11|wayland` is still honored for backward compatibility, but
    ///     `AZ_WINDOW` wins. (`AZ_BACKEND`'s render values cpu/gpu/auto are ignored here.)
    ///   - Otherwise auto-detect: Wayland if `WAYLAND_DISPLAY`, else X11 if `DISPLAY`.
    ///
    /// e.g. `AZ_WINDOW=x11 AZ_BACKEND=cpu` → X11 windowing + CPU rendering.
    pub fn select_backend() -> Result<BackendType, WindowError> {
        // Windowing preference: AZ_WINDOW first, then legacy AZ_BACKEND=x11|wayland.
        let win_pref = std::env::var("AZ_WINDOW").ok().or_else(|| {
            std::env::var("AZ_BACKEND")
                .ok()
                .filter(|b| matches!(b.to_lowercase().as_str(), "x11" | "wayland"))
        });
        if let Some(pref) = win_pref {
            match pref.to_lowercase().as_str() {
                "x11" => return Ok(BackendType::X11),
                "wayland" => return Ok(BackendType::Wayland),
                "auto" => {} // explicit auto-detect → fall through
                other => log_warn!(
                    LogCategory::Platform,
                    "Warning: Invalid AZ_WINDOW='{}', auto-detecting",
                    other
                ),
            }
        }

        // Try Wayland first (check for WAYLAND_DISPLAY)
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            log_info!(
                LogCategory::Platform,
                "[Linux] Detected Wayland session, using Wayland backend"
            );
            return Ok(BackendType::Wayland);
        }

        // Use X11
        if std::env::var("DISPLAY").is_ok() {
            log_info!(LogCategory::Platform, "[Linux] Using X11 backend");
            return Ok(BackendType::X11);
        }

        Err(WindowError::NoBackendAvailable)
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        match self {
            LinuxWindow::X11(w) => w.wait_for_events(),
            LinuxWindow::Wayland(w) => w.wait_for_events(),
        }
    }
}

/// Which display server protocol to use on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendType {
    X11,
    Wayland,
}

