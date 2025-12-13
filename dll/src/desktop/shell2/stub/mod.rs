//! Stub/headless backend for testing.
//!
//! This backend doesn't create any actual windows, useful for:
//! - Headless testing
//! - CI environments
//! - Benchmarking without GUI overhead

use azul_core::refany::RefAny;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions};

use crate::desktop::shell2::common::{
    PlatformWindow, RenderContext, WindowError, WindowProperties,
};

/// Stub window that doesn't create any actual window.
pub struct StubWindow {
    state: FullWindowState,
    open: bool,
}

impl PlatformWindow for StubWindow {
    type EventType = StubEvent;

    fn new(_options: WindowCreateOptions, _app_data: RefAny) -> Result<Self, WindowError> {
        Ok(Self {
            state: FullWindowState::default(),
            open: true,
        })
    }

    fn get_state(&self) -> FullWindowState {
        self.state.clone()
    }

    fn set_properties(&mut self, _props: WindowProperties) -> Result<(), WindowError> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        None
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

    fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        // No-op for stub
        clipboard_manager.clear();
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
