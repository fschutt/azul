//! Clipboard error type
//!
//! Simple error type for clipboard operations across all platforms.

use std::fmt;

/// Error type for clipboard operations
#[derive(Debug, Clone)]
pub struct ClipboardError {
    pub message: String,
}

impl ClipboardError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Clipboard error: {}", self.message)
    }
}

impl std::error::Error for ClipboardError {}

// Platform-specific error conversions
#[cfg(target_os = "windows")]
impl From<std::io::Error> for ClipboardError {
    fn from(err: std::io::Error) -> Self {
        Self::new(format!("Windows clipboard error: {}", err))
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd"
))]
impl From<x11_clipboard::error::Error> for ClipboardError {
    fn from(err: x11_clipboard::error::Error) -> Self {
        Self::new(format!("X11 clipboard error: {:?}", err))
    }
}

#[cfg(target_os = "macos")]
impl From<String> for ClipboardError {
    fn from(err: String) -> Self {
        Self::new(err)
    }
}
