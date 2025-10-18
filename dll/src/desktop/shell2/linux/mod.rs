//! Linux windowing backend selector.
//!
//! Automatically selects between X11 and Wayland at runtime,
//! or allows manual selection via environment variable.

pub mod wayland;
pub mod x11;

use crate::desktop::shell2::common::WindowError;

/// Linux window - either X11 or Wayland.
pub enum LinuxWindow {
    X11(x11::X11Window),
    Wayland(wayland::WaylandWindow),
}

impl LinuxWindow {
    /// Detect and select appropriate backend.
    ///
    /// Priority:
    /// 1. Check AZUL_BACKEND environment variable
    /// 2. Try Wayland (modern)
    /// 3. Fall back to X11 (legacy)
    pub fn select_backend() -> Result<BackendType, WindowError> {
        // Check environment variable override
        if let Ok(backend) = std::env::var("AZUL_BACKEND") {
            match backend.to_lowercase().as_str() {
                "x11" => return Ok(BackendType::X11),
                "wayland" => return Ok(BackendType::Wayland),
                _ => {
                    eprintln!(
                        "Warning: Invalid AZUL_BACKEND='{}', using auto-detection",
                        backend
                    );
                }
            }
        }

        // Try Wayland first (check for WAYLAND_DISPLAY)
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return Ok(BackendType::Wayland);
        }

        // Fall back to X11
        if std::env::var("DISPLAY").is_ok() {
            return Ok(BackendType::X11);
        }

        Err(WindowError::NoBackendAvailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    X11,
    Wayland,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_selection() {
        // Should not panic
        let _ = LinuxWindow::select_backend();
    }
}
