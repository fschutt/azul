//! Stub/headless backend for testing.
//!
//! This backend doesn't create any actual windows, useful for:
//! - Headless testing
//! - CI environments
//! - Benchmarking without GUI overhead

use azul_layout::window_state::{WindowCreateOptions, WindowState};

use crate::desktop::shell2::common::{
    PlatformWindow, RenderContext, WindowError, WindowProperties,
};

/// Stub window that doesn't create any actual window.
pub struct StubWindow {
    state: WindowState,
    open: bool,
}

impl PlatformWindow for StubWindow {
    type EventType = StubEvent;

    fn new(_options: WindowCreateOptions) -> Result<Self, WindowError> {
        Ok(Self {
            state: WindowState::default(),
            open: true,
        })
    }

    fn get_state(&self) -> WindowState {
        self.state.clone()
    }

    fn set_properties(&mut self, _props: WindowProperties) -> Result<(), WindowError> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        None
    }

    fn wait_event(&mut self) -> Option<Self::EventType> {
        if self.open {
            Some(StubEvent::Close)
        } else {
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        RenderContext::CPU
    }

    fn present(&mut self) -> Result<(), WindowError> {
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn request_redraw(&mut self) {
        // No-op for stub
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StubEvent {
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_window_creation() {
        let window = StubWindow::new(WindowCreateOptions::default()).unwrap();
        assert!(window.is_open());
    }

    #[test]
    fn test_stub_window_close() {
        let mut window = StubWindow::new(WindowCreateOptions::default()).unwrap();
        window.close();
        assert!(!window.is_open());
    }
}
