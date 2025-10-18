//! Common platform-agnostic code for shell2.

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod window;

// Re-exports for convenience
pub use compositor::{
    select_compositor_mode, Compositor, CompositorMode, RenderContext, SystemCapabilities,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
pub use window::{PlatformWindow, WindowProperties};
