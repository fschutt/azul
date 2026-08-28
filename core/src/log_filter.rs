//! Runtime log filtering — atomics, not `#[cfg]`.
//!
//! # Why this module exists
//!
//! azul's platform logging used to be switchable only at COMPILE time, in two
//! independent ways, and on 2026-08-07 both of them silently deleted the one
//! diagnosis that was needed:
//!
//! * `log_*!` reached the `log` facade only under `feature = "logging"`. An app
//!   linking azul-dll with `default-features = false` (the documented lean
//!   `link-static` recipe) got a no-op sink.
//! * In a `debug-server` build the same macros route into the debug server's
//!   in-memory queue instead, which never reaches stderr at all.
//!
//! The result: a Wayland compositor disconnected the client mid-run, azul's
//! dead-connection detector fired correctly, and the user saw nothing but
//! libwayland's bare `Error sending request: Broken pipe`. A diagnostic a build
//! flag can delete is not a diagnostic.
//!
//! USER RULING (2026-08-07): logging must be gated by runtime atomics, the same
//! way perf and memory profiling are — never by a cargo feature. This module is
//! that gate. It is always compiled, costs one relaxed atomic load per call
//! site, and can be changed while the process runs.
//!
//! # Configuring it
//!
//! Parsed once from `AZ_LOG` on first use, then mutable at runtime:
//!
//! ```text
//! AZ_LOG=debug                  every category at Debug and above (the default)
//! AZ_LOG=off                    silence
//! AZ_LOG=trace                  everything, including the per-frame firehose
//! AZ_LOG=warn,+platform         Warn globally, but Platform down to Trace
//! AZ_LOG=debug,-layout          Debug globally, but Layout silenced
//! ```
//!
//! `-<category>` / `+<category>` exist because the levels alone are not a usable
//! filter here: one three-resize run emits 482 547 `[Debug][Layout]` messages
//! against 7 `[Debug][Platform]` ones. Asking for platform detail at Debug means
//! writing 55 MB of layout tracing you did not want, which is slow enough to
//! change the behaviour being measured.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Severity, ordered so `level as u8 >= threshold` is the enabled test.
/// Mirrors `LogLevel` in the shell without depending on it.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

/// Log categories, in the order the shell's `LogCategory` declares them. The
/// discriminants are the bit positions used by [`CATEGORY_MASK`].
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    General = 0,
    Window = 1,
    EventLoop = 2,
    Input = 3,
    Layout = 4,
    Text = 5,
    DisplayList = 6,
    Rendering = 7,
    Resources = 8,
    Callbacks = 9,
    Timer = 10,
    DebugServer = 11,
    Platform = 12,
}

/// Number of [`Category`] variants.
pub const CATEGORY_COUNT: usize = 13;

/// Minimum severity that passes. `u8::MAX` means "logging off entirely".
static MIN_LEVEL: AtomicU8 = AtomicU8::new(u8::MAX);
/// One enable bit per [`Category`]; bit N corresponds to discriminant N.
static CATEGORY_MASK: AtomicU32 = AtomicU32::new(u32::MAX);
/// Mirror of `MIN_LEVEL != u8::MAX`, so the hot path is a single bool load.
static ANY_ENABLED: AtomicBool = AtomicBool::new(false);
/// Whether `AZ_LOG` has been parsed yet.
static INITIALISED: AtomicBool = AtomicBool::new(false);
/// Echo every passing record to stderr, in addition to whatever sink the build
/// has.
///
/// **ON as soon as anything asks for logging, OFF for a plain run.** Asking for
/// debugging and getting silence is the bug this module exists to kill:
/// `AZ_DEBUG=<port>` and `AZ_E2E=<file>` routed every record into the debug
/// server's in-memory queue and printed nothing on the terminal, so the
/// operator saw a blank screen and concluded logging was broken. If you turn a
/// debug mode on, you get logs — [`crate::log_filter`] callers OR the shell's
/// `log_gate`, which also treats an active debug server as "asked for logs".
///
/// It must NOT default to on for an app that asked for nothing. The default
/// level is Debug and the Layout category alone emits 482 547 records on a
/// three-resize run, so an unconditional echo would make every shipped azul app
/// write ~55 MB of layout tracing to stderr on an ordinary run. That is a worse
/// bug than the silence it fixes.
///
/// Set explicitly with `AZ_LOG_STDERR=1` / `AZ_LOG_STDERR=0` (0 keeps the queue
/// for the debugger UI); `AZ_LOG=off` silences everything. Volume is a
/// LEVEL/CATEGORY question — `AZ_LOG=debug,-layout` — not a reason to have no
/// sink.
static STDERR_ECHO: AtomicBool = AtomicBool::new(false);

/// The five level switches, exposed individually.
///
/// Individual statics are how these are usually flipped from a debugger or a
/// callback: `log_filter::DEBUG.store(true, ..)`.
/// They are derived from [`MIN_LEVEL`] and kept in step by [`recompute`].
pub static TRACE: AtomicBool = AtomicBool::new(false);
/// See [`TRACE`].
pub static DEBUG: AtomicBool = AtomicBool::new(false);
/// See [`TRACE`].
pub static INFO: AtomicBool = AtomicBool::new(false);
/// See [`TRACE`].
pub static WARN: AtomicBool = AtomicBool::new(false);
/// See [`TRACE`].
pub static ERROR: AtomicBool = AtomicBool::new(false);

fn recompute(min: u8) {
    let on = |l: Level| min != u8::MAX && (l as u8) >= min;
    TRACE.store(on(Level::Trace), Ordering::Relaxed);
    DEBUG.store(on(Level::Debug), Ordering::Relaxed);
    INFO.store(on(Level::Info), Ordering::Relaxed);
    WARN.store(on(Level::Warn), Ordering::Relaxed);
    ERROR.store(on(Level::Error), Ordering::Relaxed);
    ANY_ENABLED.store(min != u8::MAX, Ordering::Relaxed);
}

/// Set the global minimum level at runtime. `None` disables logging entirely.
pub fn set_min_level(level: Option<Level>) {
    let min = level.map_or(u8::MAX, |l| l as u8);
    MIN_LEVEL.store(min, Ordering::Relaxed);
    recompute(min);
    INITIALISED.store(true, Ordering::Relaxed);
}

/// Current global minimum level, or `None` when logging is off.
#[must_use]
pub fn min_level() -> Option<Level> {
    match MIN_LEVEL.load(Ordering::Relaxed) {
        0 => Some(Level::Trace),
        1 => Some(Level::Debug),
        2 => Some(Level::Info),
        3 => Some(Level::Warn),
        4 => Some(Level::Error),
        _ => None,
    }
}

/// Enable or silence one category at runtime, independently of the level.
pub fn set_category(category: Category, enabled: bool) {
    let bit = 1u32 << (category as u8);
    let mut cur = CATEGORY_MASK.load(Ordering::Relaxed);
    loop {
        let next = if enabled { cur | bit } else { cur & !bit };
        match CATEGORY_MASK.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

/// Whether `category` is currently enabled, ignoring the level.
#[must_use]
pub fn category_enabled(category: Category) -> bool {
    CATEGORY_MASK.load(Ordering::Relaxed) & (1u32 << (category as u8)) != 0
}

/// Whether records at `level` in `category` should be emitted.
///
/// This is THE hot path — it runs before the `format!` at every one of the 700+
/// `log_*!` call sites, so it must stay two relaxed loads and no allocation.
#[must_use]
pub fn enabled(category: Category, level: Level) -> bool {
    if !INITIALISED.load(Ordering::Relaxed) {
        init_from_env();
    }
    if !ANY_ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    (level as u8) >= MIN_LEVEL.load(Ordering::Relaxed) && category_enabled(category)
}

/// Level-only test, for call sites that have no category.
#[must_use]
pub fn level_enabled(level: Level) -> bool {
    if !INITIALISED.load(Ordering::Relaxed) {
        init_from_env();
    }
    ANY_ENABLED.load(Ordering::Relaxed) && (level as u8) >= MIN_LEVEL.load(Ordering::Relaxed)
}

/// Whether passing records should also be echoed to stderr.
#[must_use]
pub fn stderr_echo() -> bool {
    STDERR_ECHO.load(Ordering::Relaxed)
}

/// Turn the stderr echo on or off at runtime.
pub fn set_stderr_echo(on: bool) {
    STDERR_ECHO.store(on, Ordering::Relaxed);
}

/// Map a category name to its enum value. Accepts the spellings used in
/// `AZ_LOG` (lowercase, no separators) — `eventloop`, `displaylist`, etc.
#[must_use]
pub fn category_from_name(name: &str) -> Option<Category> {
    Some(match name {
        "general" => Category::General,
        "window" => Category::Window,
        "eventloop" | "event_loop" => Category::EventLoop,
        "input" => Category::Input,
        "layout" => Category::Layout,
        "text" => Category::Text,
        "displaylist" | "display_list" => Category::DisplayList,
        "rendering" | "render" => Category::Rendering,
        "resources" => Category::Resources,
        "callbacks" => Category::Callbacks,
        "timer" => Category::Timer,
        "debugserver" | "debug_server" | "debug" => Category::DebugServer,
        "platform" => Category::Platform,
        _ => return None,
    })
}

/// Parse one `AZ_LOG` value into (min level, category overrides).
///
/// Split out from [`init_from_env`] so it is testable without touching the
/// process environment.
#[must_use]
// The path stays QUALIFIED: `use alloc::vec::Vec` below is in scope for the
// BODY, not the signature, and without `std` there is no prelude `Vec`.
// clippy::unused_qualifications sees only the std build, where it resolves.
#[allow(unused_qualifications)]
pub fn parse(raw: &str) -> (Option<Level>, alloc::vec::Vec<(Category, bool)>) {
    use alloc::vec::Vec;
    let mut level = Some(Level::Debug); // unset / unrecognised -> Debug (the historical default)
    let mut overrides: Vec<(Category, bool)> = Vec::new();
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return (level, overrides);
    }
    for token in lower.split([',', ';', ' ']).filter(|t| !t.is_empty()) {
        // +name / -name enable or silence a single category.
        if let Some(name) = token.strip_prefix('+') {
            if let Some(cat) = category_from_name(name) {
                overrides.push((cat, true));
            }
            continue;
        }
        if let Some(name) = token.strip_prefix('-') {
            if let Some(cat) = category_from_name(name) {
                overrides.push((cat, false));
            }
            continue;
        }
        level = match token {
            "0" | "off" | "false" | "none" | "no" | "disable" | "disabled" => None,
            "error" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "trace" | "all" => Some(Level::Trace),
            // "1"/"true"/"on"/"yes"/"debug"/anything unknown -> Debug.
            _ => Some(Level::Debug),
        };
    }
    (level, overrides)
}

/// Parse `AZ_LOG` (and `AZ_LOG_STDERR`) into the atomics. Idempotent; the first
/// caller wins and later ones are no-ops, so an explicit [`set_min_level`] made
/// before any logging is not clobbered.
pub fn init_from_env() {
    if INITIALISED.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(feature = "std")]
    {
        let raw = std::env::var("AZ_LOG").unwrap_or_default();
        // Setting AZ_LOG AT ALL is a request to see logs, so it turns the echo
        // on. Leaving it unset is not, which is what keeps an ordinary app run
        // quiet. (`log_gate` additionally treats an active debug server as such
        // a request; it can see `log_active()` and this crate cannot.)
        if !raw.trim().is_empty() {
            STDERR_ECHO.store(true, Ordering::Relaxed);
        }
        let (level, overrides) = parse(&raw);
        let min = level.map_or(u8::MAX, |l| l as u8);
        MIN_LEVEL.store(min, Ordering::Relaxed);
        recompute(min);
        for (cat, on) in overrides {
            set_category(cat, on);
        }
        // Explicit override only. The default is ON (see STDERR_ECHO) — this
        // exists so a build that genuinely wants the queue and nothing else can
        // say so, not as the switch that makes logging work.
        if let Ok(v) = std::env::var("AZ_LOG_STDERR") {
            let v = v.trim().to_ascii_lowercase();
            let off = matches!(v.as_str(), "0" | "off" | "false" | "no" | "none");
            STDERR_ECHO.store(!off, Ordering::Relaxed);
        }
        // AZ_LOG=off means off, including the echo.
        if level.is_none() {
            STDERR_ECHO.store(false, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "std"))]
    {
        // No environment to read: default to Debug so an embedder that installs
        // a sink still gets records, matching the std default.
        MIN_LEVEL.store(Level::Debug as u8, Ordering::Relaxed);
        recompute(Level::Debug as u8);
    }
}

#[cfg(test)]
#[path = "log_filter_test.rs"]
mod log_filter_test;
