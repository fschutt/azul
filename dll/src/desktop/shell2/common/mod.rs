//! Common platform-agnostic code shared by all shell2 platform backends
//! (macOS, Linux/Wayland, Linux/X11, Windows).
//!
//! # Submodules
//!
//! - **accessibility** — cross-platform accessibility action queue (headless / iOS / Android)
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
pub mod accessibility;
pub mod capability_pump;
pub mod clipboard;
pub mod debug_server;
#[cfg(feature = "e2e-test")]
pub mod e2e_test;
pub mod event;
pub mod layout;
pub mod transient;
/// The runtime gate for every `log_*!` macro, plus RAII enter/exit spans.
/// Logging is gated here by atomics — never by a cargo feature.
pub mod log_gate;

// Re-exports for convenience
pub use compositor::{
    AzBackend, Compositor, CompositorMode, GpuCheckResult, GpuInfo,
    RenderContext, check_gpu_blacklist,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;

/// Re-exported from `azul_core::window` — the list moved there so the
/// headless E2E runner shares the exact resize decision the shells make.
pub use azul_core::window::CSS_BREAKPOINTS;
pub use error::{CompositorError, DlError, WindowError};
pub use event::{CommonWindowState, HitTestNode, PlatformWindow};
pub use layout::{generate_frame, regenerate_layout};
