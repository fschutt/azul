//! shell2 - Modern windowing system abstraction.
//!
//! This module provides a clean, platform-agnostic windowing API with:
//! - Dynamic library loading (Linux, Windows) to avoid linker errors
//! - CPU/GPU compositor selection per window
//! - Clean trait-based architecture
//! - Support for macOS, Windows, Linux (X11 + Wayland)
//!
//! # Architecture
//!
//! ```text
//! shell2/
//! ├── common/          Platform-agnostic traits and types
//! ├── macos/           AppKit implementation (static linking)
//! ├── windows/         Win32 implementation (dynamic loading)
//! ├── linux/
//! │   ├── x11/         X11 implementation (dynamic loading)
//! │   └── wayland/     Wayland implementation (dynamic loading)
//! └── headless/        Headless testing backend
//! ```
//!
//! # Environment Variables
//!
//! - `AZ_BACKEND` - Rendering backend: "cpu" (default), "gpu", "auto", "headless"
//! - `AZ_WINDOW` - Windowing backend on Linux: "x11", "wayland", "auto" (`AZ_BACKEND=x11|wayland`
//!   is the legacy spelling). On macOS `x11` selects the X11 backend (XQuartz) in a build with the
//!   `x11-macos` feature - see `common::x11_host`.

pub mod common;

// Platform-specific modules
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;
/// Linux: X11 + Wayland. `az_x11` is Linux, or macOS with the `x11-macos`
/// feature - where this module is built WITHOUT Wayland and the Linux desktop
/// integrations, so that the X11 backend can run against XQuartz.
#[cfg(az_x11)]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

// Always available — headless window (no native window, CPU rendering)
pub mod headless;
// Main event loop implementation
pub mod run;

// Re-export common types
pub use common::{
    check_gpu_blacklist, AzBackend, CompositorMode, CpuCompositor, GpuCheckResult, GpuInfo,
    RenderContext, WindowError,
};
// Re-export run function
pub use run::run;
#[cfg(target_os = "macos")]
pub use run::run_tray_only;

// Platform-specific window type selection
cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub use macos::MacOSWindow as Window;
        pub use macos::MacOSEvent as WindowEvent;
    } else if #[cfg(target_os = "ios")] {
        pub use ios::IOSWindow as Window;
        pub use ios::IOSEvent as WindowEvent;
    } else if #[cfg(target_os = "android")] {
        pub use android::AndroidWindow as Window;
        pub use android::AndroidEvent as WindowEvent;
    } else if #[cfg(target_os = "windows")] {
        pub use windows::Win32Window as Window;
        pub use windows::Win32Event as WindowEvent;
    } else if #[cfg(target_os = "linux")] {
        pub use linux::LinuxWindow as Window;
        pub use linux::LinuxEvent as WindowEvent;
    } else {
        // Unknown platform - use headless
        pub use headless::HeadlessWindow as Window;
        pub use headless::HeadlessEvent as WindowEvent;
    }
}
