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
#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

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
    } else if #[cfg(target_os = "windows")] {
        // TODO: Implement in Phase 4
        // pub use windows::Win32Window as Window;
        pub use stub::StubWindow as Window;
        pub use stub::StubEvent as WindowEvent;
    } else if #[cfg(target_os = "linux")] {
        // TODO: Implement in Phase 3 (X11) and Phase 5 (Wayland)
        // pub use linux::LinuxWindow as Window;
        pub use stub::StubWindow as Window;
        pub use stub::StubEvent as WindowEvent;
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

    #[cfg(target_os = "linux")]
    {
        // Runtime detection on Linux
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return "linux-wayland";
        } else {
            return "linux-x11";
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return "stub";
}

/// Get shell2 version information.
pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_name() {
        let backend = get_backend_name();
        assert!(!backend.is_empty());
        println!("Backend: {}", backend);
    }

    #[test]
    fn test_version() {
        let version = get_version();
        assert!(!version.is_empty());
        println!("shell2 version: {}", version);
    }

    #[test]
    fn test_compositor_mode_from_env() {
        // Should not panic
        let _ = CompositorMode::from_env();
    }

    #[test]
    fn test_capabilities_detection() {
        let caps = SystemCapabilities::detect();
        println!("System capabilities: {:?}", caps);
    }

    #[test]
    fn test_stub_window_creation() {
        use azul_layout::window_state::WindowCreateOptions;

        let window = Window::new(WindowCreateOptions::default());
        assert!(window.is_ok());
    }
}
