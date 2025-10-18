//! Platform window trait - abstraction over native windowing systems.

use azul_core::geom::{PhysicalPosition, PhysicalSize, PhysicalSizeU32};
use azul_layout::window_state::{WindowCreateOptions, WindowState};

use super::{compositor::RenderContext, error::WindowError};

/// Platform-agnostic window properties.
#[derive(Debug, Clone)]
pub struct WindowProperties {
    pub title: Option<String>,
    pub size: Option<PhysicalSizeU32>,
    pub position: Option<PhysicalPosition<u32>>,
    pub visible: Option<bool>,
    pub resizable: Option<bool>,
    pub minimized: Option<bool>,
    pub maximized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub decorated: Option<bool>,
    pub always_on_top: Option<bool>,
}

impl WindowProperties {
    pub fn new() -> Self {
        Self {
            title: None,
            size: None,
            position: None,
            visible: None,
            resizable: None,
            minimized: None,
            maximized: None,
            fullscreen: None,
            decorated: None,
            always_on_top: None,
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_size(mut self, size: PhysicalSizeU32) -> Self {
        self.size = Some(size);
        self
    }
}

impl Default for WindowProperties {
    fn default() -> Self {
        Self::new()
    }
}

/// Platform window trait - implemented by each backend.
pub trait PlatformWindow {
    /// Platform-specific event type
    type EventType;

    /// Create a new window with given options.
    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized;

    /// Get current window state (size, position, DPI, etc.).
    fn get_state(&self) -> WindowState;

    /// Set window properties (title, size, etc.).
    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError>;

    /// Poll for the next event (non-blocking).
    /// Returns None if no events available.
    fn poll_event(&mut self) -> Option<Self::EventType>;

    /// Wait for the next event (blocking).
    /// Returns None only if window is closed.
    fn wait_event(&mut self) -> Option<Self::EventType>;

    /// Get rendering context for this window.
    fn get_render_context(&self) -> RenderContext;

    /// Swap buffers / present frame.
    /// For GPU rendering, this presents the backbuffer.
    /// For CPU rendering, this updates the window surface.
    fn present(&mut self) -> Result<(), WindowError>;

    /// Check if window is still open.
    fn is_open(&self) -> bool;

    /// Close the window.
    fn close(&mut self);

    /// Request window redraw.
    fn request_redraw(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_properties_builder() {
        let props = WindowProperties::new()
            .with_title("Test Window".into())
            .with_size(PhysicalSizeU32 {
                width: 800,
                height: 600,
            });

        assert_eq!(props.title, Some("Test Window".into()));
        assert_eq!(
            props.size,
            Some(PhysicalSizeU32 {
                width: 800,
                height: 600
            })
        );
    }
}
