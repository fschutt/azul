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
//! - `AZ_BACKEND` - Rendering backend: "auto" (default), "gpu", "cpu", "headless"

pub mod common;

// Platform-specific modules
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "ios")]
pub mod ios;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

// Always available — headless window (no native window, CPU rendering)
pub mod headless;
// Main event loop implementation
pub mod run;

// Re-export common types
pub use common::AzBackend;
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
        // Unknown platform - use headless
        pub use headless::HeadlessWindow as Window;
        pub use headless::HeadlessEvent as WindowEvent;
    }
}

