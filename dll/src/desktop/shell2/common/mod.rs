//! Common platform-agnostic code for shell2.

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod window;

// V2 unified cross-platform modules
pub mod callback_processing;
pub mod debug_server;
pub mod event_v2;
pub mod layout_v2;

// Re-exports for convenience
pub use compositor::{
    select_compositor_mode, Compositor, CompositorMode, RenderContext, SystemCapabilities,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
// V2 re-exports
pub use event_v2::{HitTestNode, PlatformWindowV2};
pub use layout_v2::{generate_frame, regenerate_layout};
pub use window::{PlatformWindow, WindowProperties};
