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
//! └── stub/            Headless testing backend
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use shell2::{PlatformWindow, WindowCreateOptions};
//!
//! let window = PlatformWindow::new(WindowCreateOptions::default())?;
//! ```
//!
//! # Feature Flags
//!
//! - `shell2` - Enable new shell2 implementation (default)
//! - `x11` - Enable X11 backend (Linux)
//! - `wayland` - Enable Wayland backend (Linux)
//!
//! # Environment Variables
//!
//! - `AZUL_COMPOSITOR` - Force compositor mode: "cpu", "gpu", "auto" (default)
//! - `AZUL_BACKEND` - Force Linux backend: "x11", "wayland" (auto-detect default)

pub mod common;

// Platform-specific modules
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "ios")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

// Always available for testing
pub mod stub;
// Main event loop implementation
pub mod run;

// Re-export common types
pub use common::{
    select_compositor_mode, Compositor, CompositorError, CompositorMode, CpuCompositor, DlError,
    DynamicLibrary, PlatformWindow, RenderContext, SystemCapabilities, WindowError,
    WindowProperties,
};
// Re-export run function
pub use run::run;

// Platform-specific window type selection
cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub use macos::MacOSWindow as Window;
        pub use macos::MacOSEvent as WindowEvent;
    } else if #[cfg(target_os = "ios")] {
        pub use ios::IOSWindow as Window;
        pub use ios::IOSEvent as WindowEvent;
    } else if #[cfg(target_os = "windows")] {
        pub use windows::Win32Window as Window;
        pub use windows::Win32Event as WindowEvent;
    } else if #[cfg(target_os = "linux")] {
        pub use linux::LinuxWindow as Window;
        pub use linux::LinuxEvent as WindowEvent;
    } else {
        // Unknown platform - use stub
        pub use stub::StubWindow as Window;
        pub use stub::StubEvent as WindowEvent;
    }
}

/// Get the current windowing backend name.
pub fn get_backend_name() -> &'static str {
    #[cfg(target_os = "macos")]
    return "macos-appkit";

    #[cfg(target_os = "windows")]
    return "windows-win32";

    #[cfg(target_os = "ios")]
    return "ios-uikit";

    #[cfg(target_os = "linux")]
    {
        // Runtime detection on Linux
        if std::env::var("AZUL_BACKEND").as_deref() == Ok("x11") {
            return "linux-x11";
        }
        if std::env::var("AZUL_BACKEND").as_deref() == Ok("wayland") {
            return "linux-wayland";
        }
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return "linux-wayland";
        } else if std::env::var("DISPLAY").is_ok() {
            return "linux-x11";
        } else {
            return "linux-headless";
        }
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )))]
    return "stub";
}

/// Get shell2 version information.
pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
