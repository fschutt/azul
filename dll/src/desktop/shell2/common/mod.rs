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

// Unified cross-platform modules
pub mod debug_server;
pub mod event;
pub mod layout;

// Re-exports for convenience
pub use compositor::{
    AzBackend, Compositor,
};

pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
pub use event::{CommonWindowState, HitTestNode, PlatformWindow};
pub use layout::{generate_frame, regenerate_layout};
