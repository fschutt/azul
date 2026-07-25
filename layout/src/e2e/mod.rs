//! Debug server module.
//!
//! With the `debug-server` feature ON, the full HTTP debug/inspector server +
//! E2E runner live in [`full`] (the ~10k-line implementation). With it OFF (the
//! default, shipped lean `azul.*`), only the tiny [`stub`] is compiled: AZ_DEBUG
//! is a no-op, no server thread, no request handlers, no scaffold generators —
//! removing several MB and an attacker-reachable port from customer builds.
//! Build `azuldbg.*` with `--features build-dll,debug-server` to get the server.
//!
//! The `log_*` macros are defined here (always compiled, 700+ call sites). Their
//! body keeps the `if is_debug_enabled() { log(..., format!(...), ...) }` shape
//! so the format arguments still type-check in the lean build (no unused-var
//! warnings); `is_debug_enabled()` is a compile-time-constant `false` there, so
//! the branch is dead and the logging machinery is never reached.

#[cfg(feature = "debug-server")]
mod full;
#[cfg(feature = "debug-server")]
pub use full::*;

#[cfg(not(feature = "debug-server"))]
mod stub;
#[cfg(not(feature = "debug-server"))]
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

/// Log a trace message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_trace {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a debug message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_debug {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an info message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a warning message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an error message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::log_active() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}
