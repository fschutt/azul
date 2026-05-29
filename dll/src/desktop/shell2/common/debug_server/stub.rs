//! Lean-build stub of the debug server (feature `debug-server` OFF).
//!
//! Provides only the lightweight surface the always-compiled shell code needs
//! (logging enums + functions, `is_debug_enabled`, `register_debug_timer`,
//! `DebugRequest`). Everything is a no-op: there is no HTTP server, no E2E
//! runner, no request handling. `is_debug_enabled()` is a constant `false`, so
//! every `log_*!` macro body and the debug timer registration compile but are
//! dead, and the optimizer drops them.

use alloc::string::String;

/// Severity levels for log messages (mirror of the full server's enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log categories (mirror of the full server's enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogCategory {
    General,
    Window,
    EventLoop,
    Input,
    Layout,
    Text,
    DisplayList,
    Rendering,
    Resources,
    Callbacks,
    Timer,
    DebugServer,
    Platform,
}

// Re-export log categories for convenience (matches the full impl).
pub use LogCategory::*;

/// Opaque request type so signatures referencing `spmc::Receiver<DebugRequest>`
/// (e.g. `run_headless`) still type-check in the lean build. No requests are
/// ever produced — the receiver is always `None`.
#[derive(Debug, Clone, Copy)]
pub struct DebugRequest;

/// Always `false` in the lean build — the debug server is compiled out.
#[inline(always)]
pub fn is_debug_enabled() -> bool {
    false
}

/// No-op: response recording requires the debug server.
#[inline(always)]
pub fn init_recording() {}

/// No-op: logging is disabled in the lean build.
#[inline(always)]
pub fn log(
    _level: LogLevel,
    _category: LogCategory,
    _message: impl Into<String>,
    _window_id: Option<&str>,
) {
}

/// No-op: the debug timer only exists when the server is built.
#[cfg(feature = "std")]
#[inline(always)]
pub fn register_debug_timer(
    _window: &mut dyn crate::desktop::shell2::common::event::PlatformWindow,
    _request_rx: spmc::Receiver<DebugRequest>,
    _component_map: std::sync::Arc<std::sync::Mutex<azul_core::xml::ComponentMap>>,
) {
}
