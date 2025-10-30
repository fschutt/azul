//! Common platform-agnostic code for shell2.

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod window;

// V2 unified cross-platform modules
pub mod event_v2;

// TODO: These modules need refactoring to avoid borrow checker issues
// They require direct field access instead of trait methods (same issue we solved in invoke_callbacks_v2)
// Uncomment and fix when needed:
// pub mod layout_v2;
// pub mod scrollbar_v2;

// Re-exports for convenience
pub use compositor::{
    select_compositor_mode, Compositor, CompositorMode, RenderContext, SystemCapabilities,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
pub use window::{PlatformWindow, WindowProperties};

// V2 re-exports
pub use event_v2::{CallbackTarget, HitTestNode, PlatformWindowV2};

// TODO: Re-enable when layout_v2 and scrollbar_v2 are fixed:
// pub use layout_v2::regenerate_layout;
// pub use scrollbar_v2::{handle_scrollbar_click, handle_scrollbar_drag, perform_scrollbar_hit_test, ScrollbarAction};
