//! The runtime gate every `log_*!` macro passes through, plus SPANS.
//!
//! # The gate
//!
//! Two things must both be true before a log record is worth building:
//!
//! 1. **A sink exists.** Either the debug server is collecting
//!    (`log_active()`), or the stderr echo is on (`AZ_LOG_STDERR=1`). Without
//!    this check we would `format!` 482 547 strings on a three-resize run and
//!    then throw every one of them away — that is not a hypothetical figure,
//!    it is the measured `[Debug][Layout]` count from 2026-08-07.
//! 2. **The runtime filter passes** — [`azul_core::log_filter`], atomics for
//!    level and category, changeable while the process runs.
//!
//! Logging is NEVER gated by a cargo feature here. That is a standing user
//! ruling, and it exists because a compile-time gate silently deleted azul's
//! dead-connection diagnosis during the 2026-08-07 Wayland investigation: the
//! detector fired, `log_warn!` expanded to nothing, and the only evidence left
//! was libwayland's bare `Error sending request: Broken pipe`.
//!
//! # Spans
//!
//! [`LogSpan`] is an RAII enter/exit pair: it logs `→ name` on construction and
//! `← name (1.23ms)` on drop, indented by nesting depth, so a frame's structure
//! and where its time went are both readable straight out of the log. Use
//! [`crate::log_span!`].
//!
//! This is deliberately NOT `azul_layout::probe::Probe::span`. That one records
//! durations into a drained event buffer for `AZ_PROFILE=cpu`, is behind
//! `feature = "probe"` (the same compile-time-gating defect), and prints
//! nothing at the moment it happens. When a run dies mid-frame — exactly the
//! 2026-08-07 case — a buffer that is drained at the end of the frame has
//! nothing to say. These spans stream.

use azul_core::log_filter::{self};
/// Re-exported so the exported `log_*!` macros can name a level through
/// `$crate::…` and downstream users do not need `azul_core` in scope.
pub use azul_core::log_filter::Level;

use super::debug_server::{LogCategory, LogLevel};

/// Map the shell's `LogCategory` onto the filter's category enum.
///
/// The two enums are declared in different crates (and `LogCategory` itself has
/// two definitions — the lean stub and `azul_layout::e2e`), so this match is
/// the seam that keeps them in step. It is exhaustive on purpose: adding a
/// category without deciding its filter identity should be a compile error.
#[must_use]
pub fn category_of(category: LogCategory) -> log_filter::Category {
    match category {
        LogCategory::General => log_filter::Category::General,
        LogCategory::Window => log_filter::Category::Window,
        LogCategory::EventLoop => log_filter::Category::EventLoop,
        LogCategory::Input => log_filter::Category::Input,
        LogCategory::Layout => log_filter::Category::Layout,
        LogCategory::Text => log_filter::Category::Text,
        LogCategory::DisplayList => log_filter::Category::DisplayList,
        LogCategory::Rendering => log_filter::Category::Rendering,
        LogCategory::Resources => log_filter::Category::Resources,
        LogCategory::Callbacks => log_filter::Category::Callbacks,
        LogCategory::Timer => log_filter::Category::Timer,
        LogCategory::DebugServer => log_filter::Category::DebugServer,
        LogCategory::Platform => log_filter::Category::Platform,
    }
}

/// Map the shell's `LogLevel` onto the filter's level enum.
#[must_use]
pub fn level_of(level: LogLevel) -> Level {
    match level {
        LogLevel::Trace => Level::Trace,
        LogLevel::Debug => Level::Debug,
        LogLevel::Info => Level::Info,
        LogLevel::Warn => Level::Warn,
        LogLevel::Error => Level::Error,
    }
}

/// Whether a sink is listening at all. Checked before the filter because it is
/// the cheaper of the two and the more often false.
#[must_use]
pub fn sink_available() -> bool {
    super::debug_server::log_active() || log_filter::stderr_echo() || log_file().is_some()
}

/// Whether a passing record should go to stderr.
///
/// `AZ_LOG` being set turns this on inside `log_filter`. An ACTIVE DEBUG SERVER
/// counts as the same request and can only be detected here, because
/// `log_active()` lives in the shell: `AZ_DEBUG=<port>` / `AZ_E2E=<file>` used
/// to fill the debug server's queue and print nothing, which is precisely the
/// "I asked for debugging and got silence" complaint.
///
/// `AZ_LOG_STDERR=0` overrides both — it clears the flag in `log_filter`, and
/// `log_active()` is then the only thing that could re-enable it, so the check
/// below deliberately reads the explicit setting first.
fn echo_to_stderr() -> bool {
    if log_filter::stderr_echo() {
        return true;
    }
    // Not explicitly on: a debug server that is collecting means someone asked.
    super::debug_server::log_active() && !stderr_explicitly_disabled()
}

/// `AZ_LOG_STDERR=0`-style opt-out, read once.
fn stderr_explicitly_disabled() -> bool {
    #[cfg(feature = "std")]
    {
        static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        return *OFF.get_or_init(|| {
            std::env::var("AZ_LOG_STDERR")
                .map(|v| {
                    let v = v.trim().to_ascii_lowercase();
                    matches!(v.as_str(), "0" | "off" | "false" | "no" | "none")
                })
                .unwrap_or(false)
        });
    }
    #[cfg(not(feature = "std"))]
    false
}

/// THE gate. Runs before the `format!` at every `log_*!` call site, so it stays
/// branch-and-atomic-loads only.
#[must_use]
pub fn should_log(category: LogCategory, level: Level) -> bool {
    sink_available() && log_filter::enabled(category_of(category), level)
}

#[cfg(feature = "std")]
thread_local! {
    /// Nesting depth for span indentation, per thread.
    static SPAN_DEPTH: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Current span nesting depth on this thread.
#[cfg(feature = "std")]
#[must_use]
pub fn span_depth() -> usize {
    SPAN_DEPTH.with(core::cell::Cell::get)
}

#[cfg(not(feature = "std"))]
#[must_use]
pub fn span_depth() -> usize {
    0
}

/// Two spaces per nesting level, capped so a runaway recursion cannot produce
/// megabyte-wide lines.
#[must_use]
pub fn span_indent() -> &'static str {
    const PAD: &str = "                                ";
    let n = (span_depth() * 2).min(PAD.len());
    &PAD[..n]
}

/// RAII enter/exit span. See the module docs.
///
/// Construct with [`crate::log_span!`]; the guard must be bound to a named
/// local (`let _s = log_span!(...)`) or it drops immediately and reports a
/// zero-length span. Binding to `_` alone is the classic version of that bug.
#[must_use = "a span that is not bound to a local drops immediately and measures nothing"]
pub struct LogSpan {
    name: &'static str,
    category: LogCategory,
    #[cfg(feature = "std")]
    start: std::time::Instant,
    /// False when the span was filtered out; drop then does nothing.
    active: bool,
}

impl LogSpan {
    /// Open a span. Logs `→ name` and indents everything until the guard drops.
    ///
    /// Spans log at Debug: they describe control flow, which is what you want
    /// when a run dies somewhere unknown, and they are one line per entry
    /// rather than per iteration.
    pub fn enter(category: LogCategory, name: &'static str) -> Self {
        let active = should_log(category, Level::Debug);
        if active {
            let indent = span_indent();
            emit(LogLevel::Debug, category, format!("{indent}→ {name}"));
            #[cfg(feature = "std")]
            SPAN_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        }
        Self {
            name,
            category,
            #[cfg(feature = "std")]
            start: std::time::Instant::now(),
            active,
        }
    }

    /// Add a note inside the span, at the span's indentation.
    pub fn note(&self, message: impl core::fmt::Display) {
        if !self.active {
            return;
        }
        let indent = span_indent();
        emit(LogLevel::Debug, self.category, format!("{indent}· {message}"));
    }
}

impl Drop for LogSpan {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        #[cfg(feature = "std")]
        {
            SPAN_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            let ms = self.start.elapsed().as_secs_f64() * 1000.0;
            let indent = span_indent();
            emit(
                LogLevel::Debug,
                self.category,
                format!("{indent}← {} ({ms:.3}ms)", self.name),
            );
        }
        #[cfg(not(feature = "std"))]
        {
            emit(LogLevel::Debug, self.category, format!("← {}", self.name));
        }
    }
}

/// Push one already-formatted record into whatever sinks are active.
///
/// Separate from the `log_*!` macros because spans build their own strings
/// (indentation, timing) and must not re-run the gate.
pub fn emit(level: LogLevel, category: LogCategory, message: String) {
    emit_at(level, category, message, None);
}

/// [`emit`] with the optional window id the two-argument `log_*!` arms carry.
///
/// This is what every `log_*!` call site lands in once the gate passes. It
/// writes to BOTH sinks: stderr (on by default — see
/// `azul_core::log_filter::STDERR_ECHO`) and the debug server's queue, so
/// `AZ_DEBUG=<port>` gives you a terminal log AND a populated debugger UI
/// rather than a silent one and an invisible other.
pub fn emit_at(
    level: LogLevel,
    category: LogCategory,
    message: String,
    window_id: Option<&str>,
) {
    // Timestamp FIRST. A log without one cannot answer "why was it slow",
    // which is the question these traces exist for: the 2026-08-07 mouse-resize
    // capture recorded 373 configures and 4 258 lines and could not say how
    // long anything took, because the lines had no clock on them.
    let us = micros_since_start();
    if let Some(file) = log_file() {
        use std::io::Write;
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "[{us:>12}us][{level:?}][{category:?}] {message}");
        }
    }
    if echo_to_stderr() {
        eprintln!("[{us:>12}us][{level:?}][{category:?}] {message}");
    }
    super::debug_server::log(level, category, message, window_id);
}

/// Microseconds since the first log record. Monotonic, cheap, and stable across
/// threads, which is what makes gaps between lines comparable.
#[cfg(feature = "std")]
pub fn micros_since_start() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_micros()
}

#[cfg(not(feature = "std"))]
pub fn micros_since_start() -> u128 {
    0
}

/// Destination file for the log, from `AZ_LOG_FILE=<path>`.
///
/// Writing to a file rather than stdout/stderr matters for the runs these
/// traces are for: a drag-resize emits thousands of lines, and a terminal both
/// truncates them and slows the app down enough to change what is measured.
/// `AZ_LOG_FILE` also survives the app being killed, unlike a pipe into `tee`.
///
/// Setting it does NOT disable the stderr echo — set `AZ_LOG_STDERR=0` for
/// file-only, which is the combination you usually want.
#[cfg(feature = "std")]
pub fn log_file() -> Option<&'static std::sync::Mutex<std::fs::File>> {
    static FILE: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> =
        std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        let path = std::env::var("AZ_LOG_FILE").ok()?;
        match std::fs::File::create(&path) {
            Ok(f) => {
                // Announced on stderr, because a log file you cannot find is
                // indistinguishable from logging being broken.
                eprintln!("[azul] logging to {path}");
                Some(std::sync::Mutex::new(f))
            }
            Err(e) => {
                eprintln!("[azul] AZ_LOG_FILE={path} could not be opened: {e} — stderr only");
                None
            }
        }
    })
    .as_ref()
}

#[cfg(not(feature = "std"))]
pub fn log_file() -> Option<&'static core::marker::PhantomData<()>> {
    None
}

/// Open a [`LogSpan`]. Bind the result: `let _span = log_span!(cat, "name");`.
#[macro_export]
macro_rules! log_span {
    ($cat:expr, $name:literal $(,)?) => {
        $crate::desktop::shell2::common::log_gate::LogSpan::enter($cat, $name)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_maps_to_a_distinct_filter_category() {
        let all = [
            LogCategory::General, LogCategory::Window, LogCategory::EventLoop,
            LogCategory::Input, LogCategory::Layout, LogCategory::Text,
            LogCategory::DisplayList, LogCategory::Rendering, LogCategory::Resources,
            LogCategory::Callbacks, LogCategory::Timer, LogCategory::DebugServer,
            LogCategory::Platform,
        ];
        assert_eq!(all.len(), log_filter::CATEGORY_COUNT);
        let mut seen = 0u32;
        for c in all {
            let bit = 1u32 << (category_of(c) as u8);
            assert_eq!(seen & bit, 0, "two LogCategory values map to one filter category: {c:?}");
            seen |= bit;
        }
    }

    #[test]
    fn levels_map_in_order() {
        assert!(level_of(LogLevel::Error) > level_of(LogLevel::Warn));
        assert!(level_of(LogLevel::Warn) > level_of(LogLevel::Info));
        assert!(level_of(LogLevel::Info) > level_of(LogLevel::Debug));
        assert!(level_of(LogLevel::Debug) > level_of(LogLevel::Trace));
    }

    /// The gate must be false when nothing is listening, however permissive the
    /// filter is — otherwise every call site formats a string for the bin.
    #[test]
    fn no_sink_means_no_logging_however_open_the_filter() {
        log_filter::set_min_level(Some(Level::Trace));
        log_filter::set_stderr_echo(false);
        if !super::super::debug_server::log_active() {
            assert!(!should_log(LogCategory::Platform, Level::Error));
        }
        // Turning the echo on is enough of a sink on its own.
        log_filter::set_stderr_echo(true);
        assert!(should_log(LogCategory::Platform, Level::Error));
        log_filter::set_stderr_echo(false);
    }

    #[test]
    fn a_silenced_category_is_gated_even_at_error() {
        log_filter::set_stderr_echo(true);
        log_filter::set_min_level(Some(Level::Trace));
        log_filter::set_category(log_filter::Category::Layout, false);
        assert!(!should_log(LogCategory::Layout, Level::Error));
        assert!(should_log(LogCategory::Platform, Level::Error));
        log_filter::set_category(log_filter::Category::Layout, true);
        log_filter::set_stderr_echo(false);
    }

    #[test]
    fn a_filtered_span_is_inert_and_does_not_move_the_depth() {
        log_filter::set_stderr_echo(false);
        log_filter::set_min_level(None);
        let before = span_depth();
        {
            let _s = LogSpan::enter(LogCategory::Platform, "inert");
            assert_eq!(span_depth(), before, "a filtered span must not indent");
        }
        assert_eq!(span_depth(), before);
        log_filter::set_min_level(Some(Level::Debug));
    }

    #[test]
    fn an_active_span_indents_and_restores_depth() {
        log_filter::set_stderr_echo(true);
        log_filter::set_min_level(Some(Level::Debug));
        let before = span_depth();
        {
            let _outer = LogSpan::enter(LogCategory::Platform, "outer");
            assert_eq!(span_depth(), before + 1);
            {
                let _inner = LogSpan::enter(LogCategory::Platform, "inner");
                assert_eq!(span_depth(), before + 2);
            }
            assert_eq!(span_depth(), before + 1);
        }
        assert_eq!(span_depth(), before, "depth must return to where it started");
        log_filter::set_stderr_echo(false);
    }
}
