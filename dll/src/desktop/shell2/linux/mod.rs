//! Linux windowing backend selector.
//!
//! Automatically selects between X11 and Wayland at runtime,
//! or allows manual selection via environment variable.

pub mod common;
pub mod wayland;
pub mod x11;

use std::{cell::RefCell, sync::Arc};

use azul_core::resources::AppConfig;
use azul_layout::window_state::{WindowCreateOptions, WindowState};
use rust_fontconfig::FcFontCache;

use super::{PlatformWindow, WindowError, WindowProperties};
use crate::desktop::shell2::common::RenderContext;

/// Linux window - either X11 or Wayland.
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
        let fc_cache = Arc::new(rust_fontconfig::FcFontCache::build());
        let app_data = Arc::new(std::cell::RefCell::new(azul_core::refany::RefAny::new(())));

        match Self::select_backend()? {
            BackendType::X11 => Ok(LinuxWindow::X11(x11::X11Window::new(
                options, fc_cache, app_data,
            )?)),
            BackendType::Wayland => Err(WindowError::Unsupported(
                "Wayland backend is not yet implemented".into(),
            )),
        }
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
    /// Detect and select appropriate backend.
    ///
    /// Priority:
    /// 1. Check AZUL_BACKEND environment variable
    /// 2. Try Wayland (modern)
    /// 3. Fall back to X11 (legacy)
    pub fn select_backend() -> Result<BackendType, WindowError> {
        // Check environment variable override
        if let Ok(backend) = std::env::var("AZUL_BACKEND") {
            match backend.to_lowercase().as_str() {
                "x11" => return Ok(BackendType::X11),
                "wayland" => return Ok(BackendType::Wayland),
                _ => {
                    eprintln!(
                        "Warning: Invalid AZUL_BACKEND='{}', using auto-detection",
                        backend
                    );
                }
            }
        }

        // Try Wayland first (check for WAYLAND_DISPLAY)
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return Ok(BackendType::Wayland);
        }

        // Fall back to X11
        if std::env::var("DISPLAY").is_ok() {
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
    _config: AppConfig,
    _fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    let mut window = LinuxWindow::new(root_window)?;

    while window.is_open() {
        while let Some(_event) = window.poll_event() {
            // Event handling is done within poll_event for X11
        }
        // In a real loop, you'd also check for other work, timers, etc.
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    Ok(())
}
