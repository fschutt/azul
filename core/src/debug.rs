//! Debug logging types and macros for Azul.
//!
//! Provides [`DebugLevel`], [`DebugCategory`], and convenience macros
//! (`log_trace!`, `log_debug!`, `log_info!`, `log_warn!`, `log_error!`)
//! for structured logging throughout the codebase.
//!
//! The HTTP debug server implementation lives in
//! `dll/src/desktop/shell2/common/debug_server.rs`.

/// Debug message severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum DebugLevel {
    /// Very detailed tracing information
    Trace,
    /// Debugging information
    #[default]
    Debug,
    /// General information
    Info,
    /// Warnings (potential issues)
    Warn,
    /// Errors
    Error,
}


/// Categories for debug messages to enable filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum DebugCategory {
    /// General/uncategorized
    #[default]
    General,
    /// Window creation and management
    Window,
    /// Event loop processing
    EventLoop,
    /// Input events (mouse, keyboard, touch)
    Input,
    /// Layout calculation
    Layout,
    /// Text shaping and rendering
    Text,
    /// Display list generation
    DisplayList,
    /// WebRender scene building
    SceneBuilding,
    /// GPU rendering
    Rendering,
    /// Resource loading (fonts, images)
    Resources,
    /// Callbacks and user code
    Callbacks,
    /// Timer and animation
    Timer,
    /// HTTP debug server
    DebugServer,
    /// Platform-specific (macOS, Windows, Linux)
    Platform,
    /// Icon resolution
    Icon,
}


// Convenience macros for logging with automatic category and format.
//
// Usage:
//   log_debug!(logger, Layout, "Processing {} nodes", count);
//   log_info!(logger, Window, "Window created with id {}", id);

/// Log a message at trace level
#[macro_export]
macro_rules! log_trace {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.trace($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

/// Log a message at debug level
#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.debug($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

/// Log a message at info level
#[macro_export]
macro_rules! log_info {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.info($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

/// Log a message at warn level
#[macro_export]
macro_rules! log_warn {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.warn($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

/// Log a message at error level
#[macro_export]
macro_rules! log_error {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.error($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

