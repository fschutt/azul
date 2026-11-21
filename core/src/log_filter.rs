//! Runtime log filtering — atomics, not `#[cfg]`.
//!
//! # Why this module exists
//!
//! azul's platform logging used to be switchable only at COMPILE time, in two
//! independent ways, and on 2026-08-07 both of them silently deleted the one
//! diagnosis that was needed:
//!
//! * `log_*!` reached the `log` facade only under `feature = "logging"`. An app linking azul-dll
//!   with `default-features = false` (the documented lean `link-static` recipe) got a no-op sink.
//! * In a `debug-server` build the same macros route into the debug server's in-memory queue
//!   instead, which never reaches stderr at all.
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
//! (unset)                       errors only: a run that asked for nothing prints nothing else
//! AZ_LOG=1 / AZ_LOG=debug       every category at Debug and above
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
//!
//! # The filter is a value
//!
//! What the filter holds is a [`LogFilter`], and what an environment asks of one is a
//! [`LogSettings`] (a pure function of the variables it is given, so a test hands it the
//! environment it means). The process has ONE [`LogFilter`], which the free functions of this
//! module ask: the `log_*!` macros stand at some seven hundred call sites that carry no window
//! and no app, and a logger is the configuration of the whole process by nature, as the `log`
//! crate's is. A test makes a filter of its own and never touches the process's.

use alloc::vec::Vec;
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
/// discriminants are the bit positions of a [`LogFilter`]'s category mask.
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

/// What passes: the lowest severity, the categories that are on, and whether a passing record is
/// echoed to stderr.
///
/// The process has one (the free functions of this module read and write it); a test makes its
/// own with [`LogFilter::new`] and configures it with [`LogFilter::set_min_level`],
/// [`LogFilter::set_category`] or [`LogFilter::apply`].
#[derive(Debug)]
pub struct LogFilter {
    /// Minimum severity that passes. `u8::MAX` means "logging off entirely".
    min_level: AtomicU8,
    /// One enable bit per [`Category`]; bit N corresponds to discriminant N.
    category_mask: AtomicU32,
    /// Mirror of `min_level != u8::MAX`, so the hot path is a single bool load.
    any_enabled: AtomicBool,
    /// Whether the filter has been configured (the process's by `AZ_LOG`, on the first
    /// question, unless something set a level before).
    initialised: AtomicBool,
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
    stderr_echo: AtomicBool,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl LogFilter {
    /// A filter that passes nothing yet: logging off, every category on, no echo.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            min_level: AtomicU8::new(u8::MAX),
            category_mask: AtomicU32::new(u32::MAX),
            any_enabled: AtomicBool::new(false),
            initialised: AtomicBool::new(false),
            stderr_echo: AtomicBool::new(false),
        }
    }

    /// Set the minimum level at runtime. `None` disables logging entirely.
    pub fn set_min_level(&self, level: Option<Level>) {
        let min = level.map_or(u8::MAX, |l| l as u8);
        self.min_level.store(min, Ordering::Relaxed);
        self.any_enabled.store(min != u8::MAX, Ordering::Relaxed);
        self.initialised.store(true, Ordering::Relaxed);
    }

    /// Current minimum level, or `None` when logging is off.
    #[must_use]
    pub fn min_level(&self) -> Option<Level> {
        match self.min_level.load(Ordering::Relaxed) {
            0 => Some(Level::Trace),
            1 => Some(Level::Debug),
            2 => Some(Level::Info),
            3 => Some(Level::Warn),
            4 => Some(Level::Error),
            _ => None,
        }
    }

    /// Enable or silence one category at runtime, independently of the level.
    pub fn set_category(&self, category: Category, enabled: bool) {
        let bit = 1u32 << (category as u8);
        let mut cur = self.category_mask.load(Ordering::Relaxed);
        loop {
            let next = if enabled { cur | bit } else { cur & !bit };
            match self.category_mask.compare_exchange_weak(
                cur,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Whether `category` is currently enabled, ignoring the level.
    #[must_use]
    pub fn category_enabled(&self, category: Category) -> bool {
        self.category_mask.load(Ordering::Relaxed) & (1u32 << (category as u8)) != 0
    }

    /// Whether records at `level` in `category` should be emitted.
    ///
    /// This is THE hot path — it runs before the `format!` at every one of the 700+
    /// `log_*!` call sites, so it must stay two relaxed loads and no allocation.
    #[must_use]
    pub fn enabled(&self, category: Category, level: Level) -> bool {
        if !self.any_enabled.load(Ordering::Relaxed) {
            return false;
        }
        (level as u8) >= self.min_level.load(Ordering::Relaxed) && self.category_enabled(category)
    }

    /// Level-only test, for call sites that have no category.
    #[must_use]
    pub fn level_enabled(&self, level: Level) -> bool {
        self.any_enabled.load(Ordering::Relaxed)
            && (level as u8) >= self.min_level.load(Ordering::Relaxed)
    }

    /// Whether passing records should also be echoed to stderr.
    #[must_use]
    pub fn stderr_echo(&self) -> bool {
        self.stderr_echo.load(Ordering::Relaxed)
    }

    /// Turn the stderr echo on or off at runtime.
    pub fn set_stderr_echo(&self, on: bool) {
        self.stderr_echo.store(on, Ordering::Relaxed);
    }

    /// Configure the filter as `settings` say, and count it configured (so the process's is not
    /// configured from the environment again).
    pub fn apply(&self, settings: &LogSettings) {
        let min = settings.level.map_or(u8::MAX, |l| l as u8);
        self.min_level.store(min, Ordering::Relaxed);
        self.any_enabled.store(min != u8::MAX, Ordering::Relaxed);
        for &(category, enabled) in &settings.overrides {
            self.set_category(category, enabled);
        }
        if let Some(echo) = settings.stderr_echo {
            self.stderr_echo.store(echo, Ordering::Relaxed);
        }
        self.initialised.store(true, Ordering::Relaxed);
    }
}

/// What an environment asks of a [`LogFilter`]: [`LogSettings::from_environment`] reads it off the
/// variables it is given, [`LogFilter::apply`] puts it on a filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSettings {
    /// The minimum level; `None` is "off".
    pub level: Option<Level>,
    /// The categories the environment turned on (`+name`) or off (`-name`), in order.
    pub overrides: Vec<(Category, bool)>,
    /// Whether to echo passing records to stderr; `None` leaves the filter's echo as it is.
    pub stderr_echo: Option<bool>,
}

impl LogSettings {
    /// The settings of a build with no environment to read: Debug, so an embedder that installs a
    /// sink still gets records, matching the `std` default for a run that asked for logs.
    #[must_use]
    pub const fn without_environment() -> Self {
        Self {
            level: Some(Level::Debug),
            overrides: Vec::new(),
            stderr_echo: None,
        }
    }

    /// What the environment `var` (a variable's value, `None` where it is not set) asks for:
    /// `AZ_LOG` and `AZ_LOG_STDERR`, and which of [`LOG_REQUEST_VARS`] are set at all.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn from_environment(var: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Self {
        let raw = var("AZ_LOG")
            .and_then(|value| value.into_string().ok())
            .unwrap_or_default();
        // Setting AZ_LOG AT ALL is a request to see logs, so it turns the echo on. Leaving it unset
        // is not, which is what keeps an ordinary app run quiet. (`log_gate` additionally treats an
        // active debug server as such a request; it can see `log_active()` and this crate cannot.)
        let mut stderr_echo = (!raw.trim().is_empty()).then_some(true);
        let (parsed, overrides) = parse(&raw);
        let level = level_for(asked_for_logs_in(var), parsed);
        // Explicit override only. The default is ON (see `LogFilter::stderr_echo`) — this exists so a
        // build that genuinely wants the queue and nothing else can say so, not as the switch that
        // makes logging work.
        if let Some(value) = var("AZ_LOG_STDERR").and_then(|value| value.into_string().ok()) {
            let value = value.trim().to_ascii_lowercase();
            let off = matches!(value.as_str(), "0" | "off" | "false" | "no" | "none");
            stderr_echo = Some(!off);
        }
        // AZ_LOG=off means off, including the echo.
        if level.is_none() {
            stderr_echo = Some(false);
        }
        Self {
            level,
            overrides,
            stderr_echo,
        }
    }
}

// The filter of the process: the one the free functions below ask. Process-wide because the
// `log_*!` macros (some seven hundred call sites, and the shells' `log_gate`) carry no window or app
// to ask, and because `AZ_LOG` configures the logger of the whole process, as `RUST_LOG` does the
// `log` crate's.
static PROCESS: LogFilter = LogFilter::new();

/// Set the global minimum level at runtime. `None` disables logging entirely.
pub fn set_min_level(level: Option<Level>) {
    PROCESS.set_min_level(level);
}

/// Current global minimum level, or `None` when logging is off.
#[must_use]
pub fn min_level() -> Option<Level> {
    PROCESS.min_level()
}

/// Enable or silence one category at runtime, independently of the level.
pub fn set_category(category: Category, enabled: bool) {
    PROCESS.set_category(category, enabled);
}

/// Whether `category` is currently enabled, ignoring the level.
#[must_use]
pub fn category_enabled(category: Category) -> bool {
    PROCESS.category_enabled(category)
}

/// Whether records at `level` in `category` should be emitted.
///
/// This is THE hot path — it runs before the `format!` at every one of the 700+
/// `log_*!` call sites, so it must stay two relaxed loads and no allocation.
#[must_use]
pub fn enabled(category: Category, level: Level) -> bool {
    if !PROCESS.initialised.load(Ordering::Relaxed) {
        init_from_env();
    }
    PROCESS.enabled(category, level)
}

/// Level-only test, for call sites that have no category.
#[must_use]
pub fn level_enabled(level: Level) -> bool {
    if !PROCESS.initialised.load(Ordering::Relaxed) {
        init_from_env();
    }
    PROCESS.level_enabled(level)
}

/// Whether passing records should also be echoed to stderr.
///
/// Lazily initialises from the environment like [`enabled`] and
/// [`level_enabled`] do. It used to be a bare load, which made the answer
/// depend on whether something had already gone through the level gate: a
/// caller that reaches `log_gate::emit` directly rather than through a
/// `log_*!` macro never triggers `init_from_env`, so `AZ_LOG=debug` had not
/// been read yet, the echo was still its `false` default, and the record
/// was written to NO sink at all. That is what
/// `a_record_reaches_stderr_exactly_once` catches ("got 0").
#[must_use]
pub fn stderr_echo() -> bool {
    if !PROCESS.initialised.load(Ordering::Relaxed) {
        init_from_env();
    }
    PROCESS.stderr_echo()
}

/// Turn the stderr echo on or off at runtime.
pub fn set_stderr_echo(on: bool) {
    PROCESS.set_stderr_echo(on);
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
pub fn parse(raw: &str) -> (Option<Level>, Vec<(Category, bool)>) {
    // An `AZ_LOG` that is set but empty or unrecognised means "logs, please":
    // Debug. (A process that did not set it at all is `level_for`'s business.)
    let mut level = Some(Level::Debug);
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

/// The level of a process that set none of the variables that ask for logs
/// ([`LOG_REQUEST_VARS`]): errors, as the log facade's own default and
/// `AppConfig::log_level` are.
///
/// It used to be Debug: with `AZ_LOG` unset an
/// ordinary run of a lean build echoed every Debug record to stderr (the
/// shell's gate counts its default-on `log_active()` as a sink), which for a
/// frame of a map is 15 000 lines of display-list dump (2026-11-20: 3.4 million
/// lines, 612 MB and half the wall time of a two-minute session of an app that
/// had asked for no logging).
pub const LEVEL_WHEN_NOT_ASKED: Level = Level::Error;

/// The variables that ask for logs: setting one of them is a request to see
/// more than errors (`AZ_LOG` itself, a file or stderr to write to, a debug
/// server, a scripted run, a recording).
pub const LOG_REQUEST_VARS: [&str; 6] =
    ["AZ_LOG", "AZ_LOG_FILE", "AZ_LOG_STDERR", "AZ_DEBUG", "AZ_E2E", "AZ_RECORD"];

/// The level to use for what `AZ_LOG` parsed to: the parsed one when logs were
/// asked for, [`LEVEL_WHEN_NOT_ASKED`] when none of [`LOG_REQUEST_VARS`] is set.
///
/// Split out from [`init_from_env`] so it is testable without touching the
/// process environment.
#[must_use]
pub const fn level_for(asked: bool, parsed: Option<Level>) -> Option<Level> {
    if asked {
        parsed
    } else {
        Some(LEVEL_WHEN_NOT_ASKED)
    }
}

/// Whether the environment asks for logs: one of [`LOG_REQUEST_VARS`] is set.
#[cfg(feature = "std")]
#[must_use]
pub fn asked_for_logs() -> bool {
    asked_for_logs_in(&|name| std::env::var_os(name))
}

/// [`asked_for_logs`] of the environment `var` describes (a variable's value, `None` where it
/// is not set).
#[cfg(feature = "std")]
#[must_use]
pub fn asked_for_logs_in(var: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    LOG_REQUEST_VARS.iter().any(|name| var(name).is_some())
}

/// The level the environment asks for, `None` for "off": what `AZ_LOG` parses
/// to when [`asked_for_logs`], errors otherwise ([`LEVEL_WHEN_NOT_ASKED`]).
///
/// THE one reading of the environment that both log sinks of azul-dll use - the
/// shell's gate (through [`init_from_env`]) and the `log` facade's built-in
/// stderr logger - so a process cannot be at Error for one and at Debug for the
/// other (they used to parse `AZ_LOG` separately: unset was Debug for the gate
/// and Error for the facade, `AZ_LOG=1` the other way round).
#[cfg(feature = "std")]
#[must_use]
pub fn level_from_env() -> Option<Level> {
    let raw = std::env::var("AZ_LOG").unwrap_or_default();
    level_for(asked_for_logs(), parse(&raw).0)
}

/// Configure the process's filter from `AZ_LOG` (and `AZ_LOG_STDERR`). Idempotent; the first
/// caller wins and later ones are no-ops, so an explicit [`set_min_level`] made
/// before any logging is not clobbered.
pub fn init_from_env() {
    if PROCESS.initialised.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(feature = "std")]
    PROCESS.apply(&LogSettings::from_environment(&|name| std::env::var_os(name)));
    #[cfg(not(feature = "std"))]
    PROCESS.apply(&LogSettings::without_environment());
}

#[cfg(test)]
#[path = "log_filter_test.rs"]
mod log_filter_test;
