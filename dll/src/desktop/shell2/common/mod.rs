//! Common platform-agnostic code shared by all shell2 platform backends
//! (macOS, Linux/Wayland, Linux/X11, Windows).
//!
//! # Submodules
//!
//! - **compositor** — GPU/software compositor selection and rendering context
//! - **cpu_compositor** — CPU-only fallback compositor
//! - **dlopen** — Runtime dynamic library loading
//! - **error** — Error types for compositor, dlopen, and window operations
//! - **debug_server** — Built-in debug/inspector server
//! - **event** — Window event handling and hit-testing
//! - **layout** — Layout generation and incremental relayout

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod gl_loader;

// Unified cross-platform modules
pub mod capability_pump;
pub mod debug_server;
#[cfg(feature = "e2e-test")]
pub mod e2e_test;
pub mod event;
pub mod layout;

// Re-exports for convenience
pub use compositor::{
    AzBackend, Compositor, CompositorMode, GpuCheckResult, GpuInfo,
    RenderContext, check_gpu_blacklist,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;

pub const CSS_BREAKPOINTS: &[f32] = &[320.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0];
pub use error::{CompositorError, DlError, WindowError};
pub use event::{CommonWindowState, HitTestNode, PlatformWindow};
pub use layout::{generate_frame, regenerate_layout};
