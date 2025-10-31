//! Common platform-agnostic code for shell2.

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod window;

// V2 unified cross-platform modules
pub mod callback_processing;
pub mod event_v2;
pub mod layout_v2;

// TODO: This module needs refactoring to avoid borrow checker issues
// It requires direct field access instead of trait methods (same issue we solved in
// invoke_callbacks_v2). Uncomment and fix when needed:
// pub mod scrollbar_v2;

// Re-exports for convenience
pub use compositor::{
    select_compositor_mode, Compositor, CompositorMode, RenderContext, SystemCapabilities,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
// V2 re-exports
pub use event_v2::{CallbackTarget, HitTestNode, PlatformWindowV2};
pub use layout_v2::{generate_frame, regenerate_layout};
pub use window::{PlatformWindow, WindowProperties};

// TODO: Re-enable when scrollbar_v2 is fixed:
// pub use scrollbar_v2::{handle_scrollbar_click, handle_scrollbar_drag, perform_scrollbar_hit_test,
// ScrollbarAction};
