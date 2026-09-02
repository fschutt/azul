//! Debug server module.
//!
//! With the `debug-server` feature ON, the debug/inspector server + E2E runner
//! come from [`azul_layout::e2e`] (the ~13k-line op dispatcher, the `DebugEvent`
//! schema, the assertion library and the scenario runner), re-exported here so
//! the 180 `debug_server::…` call sites across the shell keep compiling
//! unchanged. Only the pieces that genuinely cannot live in `azul-layout` are
//! local, in [`platform`]: the TCP listener that serves the debugger UI out of
//! this crate's `build.rs` assets, and `register_debug_timer`, which takes a
//! `&mut dyn PlatformWindow`.
//!
//! This module used to carry a SECOND, hand-ported copy of that dispatcher
//! (`full.rs`). The two copies drifted 1,322 lines apart because CI gated only
//! this one while `azul-doc e2e` gated only the layout one, so an assertion
//! fixed in either was silently not fixed in the other. There is now one copy.
//!
//! With the feature OFF (the default, shipped lean `azul.*`), only the tiny
//! [`stub`] is compiled: AZ_DEBUG is a no-op, no server thread, no request
//! handlers, no scaffold generators — removing several MB and an
//! attacker-reachable port from customer builds. Build `azuldbg.*` with
//! `--features build-dll,debug-server` to get the server.
//!
//! The `log_*` macros are defined here (always compiled, 700+ call sites). Their
//! body keeps the `if is_debug_enabled() { log(..., format!(...), ...) }` shape
//! so the format arguments still type-check in the lean build (no unused-var
//! warnings); `is_debug_enabled()` is a compile-time-constant `false` there, so
//! the branch is dead and the logging machinery is never reached.

// THE E2E EXECUTION ENGINE (op dispatch, `E2eTest`, the response types) is
// available whenever EITHER feature is on. `e2e-scripting` exists precisely to
// get this WITHOUT the HTTP server: a script handed over via `AZ_E2E=...`
// needs no socket, and a shipped binary should not carry one. Gating these
// re-exports on `debug-server` alone meant `e2e-scripting` compiled the engine
// into azul-layout and then hid it, so AZ_E2E silently did nothing.
#[cfg(any(feature = "debug-server", feature = "e2e-scripting"))]
pub use azul_layout::e2e::*;

// `platform` carries BOTH the HTTP server and the dll-side pieces the script
// runner needs (the request-pump timer, and the host hooks that give the
// `screenshot` op a real native screenshot). It compiles under either feature;
// the socket itself is gated INSIDE, so `e2e-scripting` links no server.
#[cfg(any(feature = "debug-server", feature = "e2e-scripting"))]
mod platform;
#[cfg(any(feature = "debug-server", feature = "e2e-scripting"))]
pub use platform::*;

// The no-server, no-engine build keeps the stubs.
#[cfg(not(any(feature = "debug-server", feature = "e2e-scripting")))]
mod stub;
#[cfg(not(any(feature = "debug-server", feature = "e2e-scripting")))]
pub use stub::*;

// ==================== Logging Macros ====================

// ==================== Always-on Platform Logging ====================
//
// The `log_*!` macros above are gated on the (compile-time-off-in-lean) debug
// server. The platform device/windowing layer (`shell2/*`, `extra/*`) instead
// needs traces that reach the *standard* `log` facade so they show up in the
// customer/lean build too — wherever a logger is installed (env_logger in
// azul-self-test, android_logger on Android, pyo3-log under Python, or nothing).
//
// `plog_*!` route to `log::<level>!` when the `logging` feature is on (it is in
// `default` and in every real desktop/mobile build), and to an arg-consuming
// no-op otherwise so `--no-default-features` (no `log` crate) still compiles.
// No `LogCategory`/window arg — prefix the message with a `[subsystem]` tag.

/// Always-on platform trace log (routes to the `log` crate facade).
#[macro_export]
macro_rules! plog_trace {
    ($($arg:tt)*) => {{
        #[cfg(feature = "logging")]
        { log::trace!($($arg)*); }
        #[cfg(not(feature = "logging"))]
        { let _ = format_args!($($arg)*); }
    }};
}

/// Always-on platform debug log (routes to the `log` crate facade).
#[macro_export]
macro_rules! plog_debug {
    ($($arg:tt)*) => {{
        #[cfg(feature = "logging")]
        { log::debug!($($arg)*); }
        #[cfg(not(feature = "logging"))]
        { let _ = format_args!($($arg)*); }
    }};
}

/// Always-on platform info log (routes to the `log` crate facade).
#[macro_export]
macro_rules! plog_info {
    ($($arg:tt)*) => {{
        #[cfg(feature = "logging")]
        { log::info!($($arg)*); }
        #[cfg(not(feature = "logging"))]
        { let _ = format_args!($($arg)*); }
    }};
}

/// Always-on platform warning log (routes to the `log` crate facade).
#[macro_export]
macro_rules! plog_warn {
    ($($arg:tt)*) => {{
        #[cfg(feature = "logging")]
        { log::warn!($($arg)*); }
        #[cfg(not(feature = "logging"))]
        { let _ = format_args!($($arg)*); }
    }};
}

/// Always-on platform error log (routes to the `log` crate facade).
#[macro_export]
macro_rules! plog_error {
    ($($arg:tt)*) => {{
        #[cfg(feature = "logging")]
        { log::error!($($arg)*); }
        #[cfg(not(feature = "logging"))]
        { let _ = format_args!($($arg)*); }
    }};
}

/// Log a trace message. Gated at RUNTIME by `log_gate::should_log`
/// (level + category atomics, plus "is any sink listening") — never by a
/// cargo feature. The `format!` runs only if the gate passes.
#[macro_export]
macro_rules! log_trace {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Trace,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Trace,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a debug message. Gated at RUNTIME by `log_gate::should_log`
/// (level + category atomics, plus "is any sink listening") — never by a
/// cargo feature. The `format!` runs only if the gate passes.
#[macro_export]
macro_rules! log_debug {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Debug,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Debug,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an info message. Gated at RUNTIME by `log_gate::should_log`
/// (level + category atomics, plus "is any sink listening") — never by a
/// cargo feature. The `format!` runs only if the gate passes.
#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Info,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Info,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a warning message. Gated at RUNTIME by `log_gate::should_log`
/// (level + category atomics, plus "is any sink listening") — never by a
/// cargo feature. The `format!` runs only if the gate passes.
#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Warn,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Warn,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an error message. Gated at RUNTIME by `log_gate::should_log`
/// (level + category atomics, plus "is any sink listening") — never by a
/// cargo feature. The `format!` runs only if the gate passes.
#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Error,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::log_gate::should_log(
            $cat,
            $crate::desktop::shell2::common::log_gate::Level::Error,
        ) {
            $crate::desktop::shell2::common::log_gate::emit_at(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}
