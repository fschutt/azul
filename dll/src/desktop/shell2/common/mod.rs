//! Common platform-agnostic code for shell2.

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;

// Unified cross-platform modules
pub mod debug_server;
pub mod event;
pub mod layout;

// Re-exports for convenience
pub use compositor::{
    select_compositor_mode, Compositor, CompositorMode, RenderContext, SystemCapabilities,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
pub use event::{CommonWindowState, HitTestNode, PlatformWindow};
pub use layout::{generate_frame, regenerate_layout};
