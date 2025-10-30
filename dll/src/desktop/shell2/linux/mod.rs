//! Linux windowing backend selector.
//!
//! Automatically selects between X11 and Wayland at runtime,
//! or allows manual selection via environment variable.

pub mod common;
pub mod registry;
pub mod resources;

/// GNOME native menu integration (DBus)
#[cfg(feature = "gnome-menus")]
pub mod gnome_menu;

/// Wayland implementation
pub mod wayland;
/// X11 implementation
pub mod x11;

use std::{cell::RefCell, sync::Arc};

use azul_core::resources::AppConfig;
use azul_css::props::basic::{LayoutPoint, LayoutRect, LayoutSize};
use azul_layout::window_state::{WindowCreateOptions, WindowState};
pub use resources::AppResources;
use rust_fontconfig::FcFontCache;

use super::{PlatformWindow, WindowError, WindowProperties};
use crate::desktop::shell2::common::RenderContext;

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

impl PlatformWindow for LinuxWindow {
    type EventType = LinuxEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        // Create default resources - this is a fallback for tests
        // In production, use new_with_resources() from run()
        let resources = Arc::new(AppResources::default_for_testing());
        Self::new_with_resources(options, resources)
    }

    fn get_state(&self) -> WindowState {
        match self {
            LinuxWindow::X11(w) => w.get_state(),
            LinuxWindow::Wayland(w) => w.get_state(),
        }
    }

    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        match self {
            LinuxWindow::X11(w) => w.set_properties(props),
            LinuxWindow::Wayland(w) => w.set_properties(props),
        }
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        match self {
            LinuxWindow::X11(w) => w.poll_event().map(LinuxEvent::X11),
            LinuxWindow::Wayland(w) => w.poll_event().map(LinuxEvent::Wayland),
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match self {
            LinuxWindow::X11(w) => w.get_render_context(),
            LinuxWindow::Wayland(w) => w.get_render_context(),
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match self {
            LinuxWindow::X11(w) => w.present(),
            LinuxWindow::Wayland(w) => w.present(),
        }
    }

    fn is_open(&self) -> bool {
        match self {
            LinuxWindow::X11(w) => w.is_open(),
            LinuxWindow::Wayland(w) => w.is_open(),
        }
    }

    fn close(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.close(),
            LinuxWindow::Wayland(w) => w.close(),
        }
    }

    fn request_redraw(&mut self) {
        match self {
            LinuxWindow::X11(w) => w.request_redraw(),
            LinuxWindow::Wayland(w) => w.request_redraw(),
        }
    }
}

impl LinuxWindow {
    /// Create a new Linux window with shared resources
    ///
    /// This is the preferred way to create windows in production,
    /// as it allows sharing font cache, app data, and system styling.
    pub fn new_with_resources(
        options: WindowCreateOptions,
        resources: Arc<AppResources>,
    ) -> Result<Self, WindowError> {
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
                    eprintln!(
                        "Warning: Invalid AZUL_BACKEND='{}', auto-detecting",
                        backend
                    );
                }
            }
        }

        // Try Wayland first (check for WAYLAND_DISPLAY)
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            eprintln!("[Linux] Detected Wayland session, using Wayland backend");
            return Ok(BackendType::Wayland);
        }

        // Use X11
        if std::env::var("DISPLAY").is_ok() {
            eprintln!("[Linux] Using X11 backend");
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

// Update the main run function to use this LinuxWindow
pub fn run(
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    // Create shared resources for all windows
    let resources = Arc::new(AppResources::new(config, fc_cache));
    let mut window = LinuxWindow::new_with_resources(root_window, resources)?;

    while window.is_open() {
        while let Some(_event) = window.poll_event() {
            // Event handling is done within poll_event for X11
        }
        // In a real loop, you'd also check for other work, timers, etc.
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    Ok(())
}

/// Query X11 for screen dimensions
/// Returns (width, height, scale_factor) if successful
///
/// Note: XDisplayWidth/XDisplayHeight are macros that aren't available via dlopen.
/// A full implementation would use XRandR for proper multi-monitor support.
/// For now, we try environment variables and provide reasonable defaults.
fn query_x11_screen_dimensions() -> Option<(i32, i32, f64)> {
    // Try common environment variables that desktop environments set
    let width = std::env::var("DISPLAY_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| std::env::var("XWIDTH").ok().and_then(|s| s.parse().ok()));

    let height = std::env::var("DISPLAY_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| std::env::var("XHEIGHT").ok().and_then(|s| s.parse().ok()));

    let scale = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            std::env::var("QT_SCALE_FACTOR")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(1.0);

    match (width, height) {
        (Some(w), Some(h)) => Some((w, h, scale)),
        _ => None, // Return None to fall back to defaults
    }
}

/// Get available monitors on Linux
///
/// This function detects the backend (X11/Wayland) and returns monitor information.
/// For now, returns a single primary monitor based on environment variables or defaults.
pub fn get_monitors() -> azul_core::window::MonitorVec {
    use azul_core::window::{Monitor, VideoModeVec};
    use azul_css::props::basic::{LayoutPoint, LayoutSize};

    // Try to get display dimensions from environment variables or X11
    let (width, height, scale_factor) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
        // Wayland session
        let width = std::env::var("WAYLAND_DISPLAY_WIDTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920);
        let height = std::env::var("WAYLAND_DISPLAY_HEIGHT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1080);
        let scale = std::env::var("GDK_SCALE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        (width, height, scale)
    } else if std::env::var("DISPLAY").is_ok() {
        // X11 session - try to query actual screen dimensions
        match query_x11_screen_dimensions() {
            Some((w, h, s)) => (w, h, s),
            None => {
                // Fallback to environment variables or defaults
                let width = std::env::var("DISPLAY_WIDTH")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1920);
                let height = std::env::var("DISPLAY_HEIGHT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1080);
                (width, height, 1.0)
            }
        }
    } else {
        // Fallback to reasonable defaults
        (1920, 1080, 1.0)
    };

    let monitor = Monitor {
        id: azul_core::window::MonitorId::new(0),
        name: if std::env::var("WAYLAND_DISPLAY").is_ok() {
            Some("wayland-0".to_string().into()).into()
        } else {
            Some("x11-0".to_string().into()).into()
        },
        size: LayoutSize::round(width as f32, height as f32),
        position: LayoutPoint::zero(),
        scale_factor,
        work_area: LayoutRect::new(
            LayoutPoint::zero(),
            LayoutSize::round(width as f32, (height as i32 - 24).max(0) as f32),
        ),
        video_modes: VideoModeVec::from_const_slice(&[]),
        is_primary_monitor: true,
    };

    azul_core::window::MonitorVec::from_vec(vec![monitor])
}
