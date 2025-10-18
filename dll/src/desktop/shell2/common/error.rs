//! Error types for shell2 windowing system.

use core::fmt;

/// Errors that can occur during window operations.
#[derive(Debug, Clone)]
pub enum WindowError {
    /// Platform-specific error with description
    PlatformError(String),

    /// Failed to create rendering context
    ContextCreationFailed,

    /// Window was closed
    WindowClosed,

    /// Invalid window state for the requested operation
    InvalidState(String),

    /// No backend available (Linux: neither X11 nor Wayland)
    NoBackendAvailable,

    /// Feature not supported on this platform
    Unsupported(String),
}

impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WindowError::PlatformError(msg) => write!(f, "Platform error: {}", msg),
            WindowError::ContextCreationFailed => write!(f, "Failed to create rendering context"),
            WindowError::WindowClosed => write!(f, "Window was closed"),
            WindowError::InvalidState(msg) => write!(f, "Invalid window state: {}", msg),
            WindowError::NoBackendAvailable => write!(f, "No windowing backend available"),
            WindowError::Unsupported(feature) => write!(f, "Feature not supported: {}", feature),
        }
    }
}

impl std::error::Error for WindowError {}

/// Errors that can occur during compositor operations.
#[derive(Debug, Clone)]
pub enum CompositorError {
    /// GPU not available or initialization failed
    NoGPU,

    /// Shader compilation failed
    ShaderError(String),

    /// Out of memory (GPU or system)
    OutOfMemory,

    /// GPU context lost (device reset, driver crash)
    ContextLost,

    /// Unsupported compositor mode on this platform
    UnsupportedMode(String),

    /// Render operation failed
    RenderFailed(String),

    /// Resize operation failed
    ResizeFailed(String),
}

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompositorError::NoGPU => write!(f, "GPU not available"),
            CompositorError::ShaderError(msg) => write!(f, "Shader error: {}", msg),
            CompositorError::OutOfMemory => write!(f, "Out of memory"),
            CompositorError::ContextLost => write!(f, "GPU context lost"),
            CompositorError::UnsupportedMode(mode) => {
                write!(f, "Unsupported compositor mode: {}", mode)
            }
            CompositorError::RenderFailed(msg) => write!(f, "Render failed: {}", msg),
            CompositorError::ResizeFailed(msg) => write!(f, "Resize failed: {}", msg),
        }
    }
}

impl std::error::Error for CompositorError {}

/// Errors that can occur during dynamic library loading.
#[derive(Debug, Clone)]
pub enum DlError {
    /// Library file not found
    LibraryNotFound {
        name: String,
        tried: Vec<String>,
        suggestion: String,
    },

    /// Symbol not found in library
    SymbolNotFound {
        symbol: String,
        library: String,
        suggestion: String,
    },

    /// Invalid library format or architecture mismatch
    InvalidLibrary(String),

    /// Version mismatch between required and found
    VersionMismatch { found: String, required: String },
}

impl fmt::Display for DlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DlError::LibraryNotFound {
                name,
                tried,
                suggestion,
            } => {
                write!(
                    f,
                    "Failed to load library '{}'.\nTried: {:?}\n\nSuggestion: {}",
                    name, tried, suggestion
                )
            }
            DlError::SymbolNotFound {
                symbol,
                library,
                suggestion,
            } => {
                write!(
                    f,
                    "Symbol '{}' not found in library '{}'.\n\nSuggestion: {}",
                    symbol, library, suggestion
                )
            }
            DlError::InvalidLibrary(msg) => write!(f, "Invalid library: {}", msg),
            DlError::VersionMismatch { found, required } => {
                write!(
                    f,
                    "Version mismatch: found {}, required {}",
                    found, required
                )
            }
        }
    }
}

impl std::error::Error for DlError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = WindowError::PlatformError("test".into());
        assert_eq!(format!("{}", err), "Platform error: test");

        let err = CompositorError::NoGPU;
        assert_eq!(format!("{}", err), "GPU not available");

        let err = DlError::LibraryNotFound {
            name: "libX11.so".into(),
            tried: vec!["libX11.so.6".into(), "libX11.so".into()],
            suggestion: "Install X11".into(),
        };
        assert!(format!("{}", err).contains("libX11.so"));
    }
}
