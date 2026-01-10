//! Unified Debug Logging System for Azul
//!
//! This module provides a thread-safe debug logging infrastructure that can be
//! passed through function arguments to collect debug messages during execution.
//!
//! ## Design Philosophy
//!
//! 1. **No global state for logging** - All logging goes through explicit parameters
//! 2. **Zero-cost when disabled** - `Option<&mut DebugLogger>` is `None` in production
//! 3. **Structured messages** - Each message has level, category, and location
//! 4. **Thread-safe collection** - Can collect messages from multiple threads
//!
//! ## Usage Pattern
//!
//! Functions that need debug logging accept `debug_log: &mut Option<DebugLogger>`:
//!
//! ```rust,ignore
//! fn do_layout(
//!     dom: &Dom,
//!     // ... other params ...
//!     debug_log: &mut Option<DebugLogger>,
//! ) {
//!     log_debug!(debug_log, Layout, "Starting layout for {} nodes", dom.len());
//!     // ... do work ...
//! }
//! ```
//!
//! ## Integration with HTTP Debug Server
//!
//! When `AZUL_DEBUG` is set, the debug server creates a `DebugLogger` for each
//! incoming request, which collects all messages until the frame is rendered,
//! then returns them in the HTTP response.

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::sync::{Arc, Mutex};

/// Debug message severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum DebugLevel {
    /// Very detailed tracing information
    Trace,
    /// Debugging information
    Debug,
    /// General information
    Info,
    /// Warnings (potential issues)
    Warn,
    /// Errors
    Error,
}

impl Default for DebugLevel {
    fn default() -> Self {
        DebugLevel::Debug
    }
}

/// Categories for debug messages to enable filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum DebugCategory {
    /// General/uncategorized
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
}

impl Default for DebugCategory {
    fn default() -> Self {
        DebugCategory::General
    }
}

/// A structured log message for the debug logger
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct LogMessage {
    /// Severity level
    pub level: DebugLevel,
    /// Category for filtering
    pub category: DebugCategory,
    /// The message content
    pub message: String,
    /// Source file and line (from #[track_caller])
    pub location: String,
    /// Elapsed time in microseconds since logger creation
    pub elapsed_us: u64,
    /// Optional window ID this message relates to
    pub window_id: Option<String>,
}

/// Debug logger that collects messages during execution.
///
/// Passed as `&mut Option<DebugLogger>` to functions:
/// - `None` = logging disabled (production mode)
/// - `Some(logger)` = logging enabled (debug mode)
#[cfg(feature = "std")]
pub struct DebugLogger {
    messages: Vec<LogMessage>,
    start_time: std::time::Instant,
    /// Minimum level to record (messages below this are ignored)
    min_level: DebugLevel,
    /// If set, only messages from these categories are recorded
    category_filter: Option<Vec<DebugCategory>>,
    /// Current window context (set when processing window-specific events)
    current_window_id: Option<String>,
}

#[cfg(feature = "std")]
impl DebugLogger {
    /// Create a new debug logger that records all messages
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            start_time: std::time::Instant::now(),
            min_level: DebugLevel::Trace,
            category_filter: None,
            current_window_id: None,
        }
    }

    /// Create a logger with minimum level filter
    pub fn with_min_level(min_level: DebugLevel) -> Self {
        Self {
            messages: Vec::new(),
            start_time: std::time::Instant::now(),
            min_level,
            category_filter: None,
            current_window_id: None,
        }
    }

    /// Create a logger that only records specific categories
    pub fn with_categories(categories: Vec<DebugCategory>) -> Self {
        Self {
            messages: Vec::new(),
            start_time: std::time::Instant::now(),
            min_level: DebugLevel::Trace,
            category_filter: Some(categories),
            current_window_id: None,
        }
    }

    /// Set the current window context for subsequent messages
    pub fn set_window_context(&mut self, window_id: Option<String>) {
        self.current_window_id = window_id;
    }

    /// Log a message with full control over all fields
    #[track_caller]
    pub fn log(&mut self, level: DebugLevel, category: DebugCategory, message: impl Into<String>) {
        // Check level filter
        if level < self.min_level {
            return;
        }

        // Check category filter
        if let Some(ref allowed) = self.category_filter {
            if !allowed.contains(&category) {
                return;
            }
        }

        let location = core::panic::Location::caller();
        self.messages.push(LogMessage {
            level,
            category,
            message: message.into(),
            location: format!("{}:{}", location.file(), location.line()),
            elapsed_us: self.start_time.elapsed().as_micros() as u64,
            window_id: self.current_window_id.clone(),
        });
    }

    /// Log a trace message
    #[track_caller]
    pub fn trace(&mut self, category: DebugCategory, message: impl Into<String>) {
        self.log(DebugLevel::Trace, category, message);
    }

    /// Log a debug message
    #[track_caller]
    pub fn debug(&mut self, category: DebugCategory, message: impl Into<String>) {
        self.log(DebugLevel::Debug, category, message);
    }

    /// Log an info message
    #[track_caller]
    pub fn info(&mut self, category: DebugCategory, message: impl Into<String>) {
        self.log(DebugLevel::Info, category, message);
    }

    /// Log a warning
    #[track_caller]
    pub fn warn(&mut self, category: DebugCategory, message: impl Into<String>) {
        self.log(DebugLevel::Warn, category, message);
    }

    /// Log an error
    #[track_caller]
    pub fn error(&mut self, category: DebugCategory, message: impl Into<String>) {
        self.log(DebugLevel::Error, category, message);
    }

    /// Take all collected messages (empties the logger)
    pub fn take_messages(&mut self) -> Vec<LogMessage> {
        core::mem::take(&mut self.messages)
    }

    /// Get a reference to collected messages
    pub fn messages(&self) -> &[LogMessage] {
        &self.messages
    }

    /// Get count of collected messages
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if no messages have been collected
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get elapsed time since logger was created
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

#[cfg(feature = "std")]
impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience macro for logging with automatic category and format
///
/// Usage:
/// ```rust,ignore
/// log_debug!(logger, Layout, "Processing {} nodes", count);
/// log_info!(logger, Window, "Window created with id {}", id);
/// ```
#[macro_export]
macro_rules! log_trace {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.trace($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.debug($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.info($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.warn($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($logger:expr, $category:ident, $($arg:tt)*) => {
        if let Some(ref mut logger) = $logger {
            logger.error($crate::debug::DebugCategory::$category, format!($($arg)*));
        }
    };
}

/// Type alias for the debug logger parameter pattern used throughout the codebase
#[cfg(feature = "std")]
pub type DebugLog = Option<DebugLogger>;

/// Helper function to conditionally log (when logger is Some)
#[cfg(feature = "std")]
#[track_caller]
pub fn debug_log(
    logger: &mut Option<DebugLogger>,
    level: DebugLevel,
    category: DebugCategory,
    message: impl Into<String>,
) {
    if let Some(ref mut l) = logger {
        l.log(level, category, message);
    }
}

/// Check if debug mode is enabled via environment variable
#[cfg(feature = "std")]
pub fn is_debug_enabled() -> bool {
    std::env::var("AZUL_DEBUG").is_ok()
}

/// Get the debug server port from environment variable
#[cfg(feature = "std")]
pub fn get_debug_port() -> Option<u16> {
    std::env::var("AZUL_DEBUG")
        .ok()
        .and_then(|s| s.parse().ok())
}
