//! Stub/headless backend for testing.
//!
//! This backend doesn't create any actual windows, useful for:
//! - Headless testing
//! - CI environments
//! - Benchmarking without GUI overhead

use azul_core::refany::RefAny;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions};

use crate::desktop::shell2::common::WindowError;

/// Stub window that doesn't create any actual window.
pub struct StubWindow {
    state: FullWindowState,
    open: bool,
}

impl StubWindow {
    pub fn new(_options: WindowCreateOptions, _app_data: RefAny) -> Result<Self, WindowError> {
        Ok(Self {
            state: FullWindowState::default(),
            open: true,
        })
    }

    pub fn poll_event(&mut self) -> Option<StubEvent> {
        None
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn request_redraw(&mut self) {
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
    use azul_core::refany::RefAny;

    #[test]
    fn test_stub_window_creation() {
        let app_data = RefAny::new(());
        let window = StubWindow::new(WindowCreateOptions::default(), app_data).unwrap();
        assert!(window.is_open());
    }

    #[test]
    fn test_stub_window_close() {
        let app_data = RefAny::new(());
        let mut window = StubWindow::new(WindowCreateOptions::default(), app_data).unwrap();
        window.close();
        assert!(!window.is_open());
    }
}
