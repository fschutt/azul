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

/// Whether the `log_*!` macros should fire in the lean build. Unlike the full
/// server (which gates on `is_debug_enabled()` to feed its queue), the lean
/// build forwards messages to the `log` facade, so this follows `AZ_LOG` and is
/// ON BY DEFAULT — see `desktop::logging::az_log_level`. This is what makes the
/// 100+ existing `log_*!` platform traces visible in the shipped `.deb`/dylib,
/// so an app never "just exits with no error". `AZ_LOG=off` returns false here
/// (and the macro bodies stay dead), so logging can be fully silenced.
#[cfg(feature = "std")]
pub fn log_active() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        match std::env::var("AZ_LOG")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "0" | "off" | "false" | "none" | "no" | "disable" | "disabled" => false,
            _ => true, // unset / 1 / true / on / a level name -> ON (testing default)
        }
    })
}

/// no_std lean builds have no env / logger — logging is unavailable.
#[cfg(not(feature = "std"))]
#[inline(always)]
pub fn log_active() -> bool {
    false
}

/// No-op: response recording requires the debug server.
#[inline(always)]
pub fn init_recording() {}

/// Lean-build logging: forward to the `log` facade (the built-in stderr logger
/// installed by `desktop::logging::init_default_logger`, or a host logger).
/// Gated by `log_active()` so direct callers (App::run etc.) honor `AZ_LOG`.
#[cfg(all(feature = "std", feature = "logging"))]
pub fn log(
    level: LogLevel,
    category: LogCategory,
    message: impl Into<String>,
    _window_id: Option<&str>,
) {
    if !log_active() {
        return;
    }
    let lvl = match level {
        LogLevel::Trace => log::Level::Trace,
        LogLevel::Debug => log::Level::Debug,
        LogLevel::Info => log::Level::Info,
        LogLevel::Warn => log::Level::Warn,
        LogLevel::Error => log::Level::Error,
    };
    let target: &str = match category {
        LogCategory::General => "azul",
        LogCategory::Window => "azul::window",
        LogCategory::EventLoop => "azul::eventloop",
        LogCategory::Input => "azul::input",
        LogCategory::Layout => "azul::layout",
        LogCategory::Text => "azul::text",
        LogCategory::DisplayList => "azul::displaylist",
        LogCategory::Rendering => "azul::render",
        LogCategory::Resources => "azul::resources",
        LogCategory::Callbacks => "azul::callbacks",
        LogCategory::Timer => "azul::timer",
        LogCategory::DebugServer => "azul::debug",
        LogCategory::Platform => "azul::platform",
    };
    log::log!(target: target, lvl, "{}", message.into());
}

/// No-op when the `log` facade is unavailable (no `logging` feature / no_std).
#[cfg(not(all(feature = "std", feature = "logging")))]
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
