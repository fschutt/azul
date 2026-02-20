//! Linux windowing backend selector.
//!
//! Automatically selects between X11 and Wayland at runtime,
//! or allows manual selection via environment variable.

pub mod common;
pub mod registry;
pub mod resources;
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

use super::WindowError;

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

    pub fn close(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.close(),
            LinuxWindow::Wayland(w) => w.close(),
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
        mut resources: Arc<AppResources>,
    ) -> Result<Self, WindowError> {
        // Update the app_data in resources
        let resources = Arc::new(AppResources {
            app_data,
            config: resources.config.clone(),
            fc_cache: resources.fc_cache.clone(),
            font_registry: resources.font_registry.clone(),
            system_style: resources.system_style.clone(),
            icon_provider: resources.icon_provider.clone(),
        });

        match Self::select_backend()? {
            BackendType::X11 => Ok(LinuxWindow::X11(x11::X11Window::new_with_resources(
                options, resources,
            )?)),
            BackendType::Wayland => Ok(LinuxWindow::Wayland(wayland::WaylandWindow::new(
                options, resources,
            )?)),
        }
    }

    /// Detect and select appropriate backend.
    ///
    /// Priority:
    /// 1. Check AZUL_BACKEND environment variable
    /// 2. Try Wayland (if WAYLAND_DISPLAY set and feature enabled)
    /// 3. Fall back to X11 (if DISPLAY set)
    pub fn select_backend() -> Result<BackendType, WindowError> {
        // Check environment variable override
        if let Ok(backend) = std::env::var("AZUL_BACKEND") {
            match backend.to_lowercase().as_str() {
                "x11" => return Ok(BackendType::X11),
                "wayland" => return Ok(BackendType::Wayland),
                _ => {
                    log_warn!(
                        LogCategory::Platform,
                        "Warning: Invalid AZUL_BACKEND='{}', auto-detecting",
                        backend
                    );
                }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    X11,
    Wayland,
}

