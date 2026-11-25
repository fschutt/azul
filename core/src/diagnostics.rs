//! Framework diagnostics: warnings the engine emits about the app's own DOM,
//! recorded in-process so a test can assert on them.
//!
//! Lints like `image-churn` and `text-without-block` exist to tell a developer
//! that something they built will misbehave. Printing to stderr makes them
//! visible to a human and invisible to everything else: an e2e scenario cannot
//! assert that a warning fired, and — more useful — cannot assert that one did
//! NOT fire after a fix.
//!
//! So every diagnostic goes to BOTH places: a bounded in-process ring, where
//! `assert_stderr` in the e2e harness can read it, and an installable SINK.
//!
//! The sink exists because `eprintln!` is not something an application can
//! control. A shipped app wants these on its own logger — with levels, with a
//! category, and in production or QA forwarded to a remote store (Loki,
//! Prometheus) rather than dumped on a stderr nobody reads. [`set_sink`]
//! replaces the default, so the engine keeps ONE place that decides where a
//! diagnostic goes and every lint keeps using [`emit`].
//!
//! The ring is capped. A lint that fires every frame must not turn into an
//! unbounded allocation in a long-running app — the point is to catch churn,
//! not to become it.
//!
//! # The recorder is a value
//!
//! What holds the ring, the sink, the scenario tag and the findings said once is a
//! [`Diagnostics`]; a test of the recorder itself makes one of its own. The free functions of
//! this module ask the process's: its emitters are lints and warnings at some twenty places of
//! azul-core and azul-layout that carry no window and no app (a lint that runs on a DOM, a
//! warning of a getter), and the e2e harness reads what they said through
//! [`recorded`]. Giving the ring to a window would take a `&Diagnostics` (or the findings as
//! data, emitted at the shell's edge) through each of those emitters; until then the tests
//! that read the process's ring share [`test_lock`].

#[cfg(feature = "std")]
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Mutex, OnceLock},
};

use crate::settings::DiagSettings;

/// The dials of an environment that asked for nothing.
static NO_SETTINGS: DiagSettings = DiagSettings::new();

/// Most diagnostics retained. Older ones are dropped.
#[cfg(feature = "std")]
const CAPACITY: usize = 256;

/// Where diagnostics go, besides the ring.
///
/// Default writes to stderr so a developer running the app sees warnings with
/// no setup. An application replaces it with [`set_sink`] to route through its
/// own logger — and from there to a remote store.
#[cfg(feature = "std")]
pub type DiagnosticSink = fn(&str);

/// Write one diagnostic line, tolerating a destination that has gone away.
///
/// `eprintln!` PANICS when the write fails, and a closed stderr is not exotic:
/// piping a running app into `head` closes the pipe as soon as `head` has its
/// lines, and the very next diagnostic kills the app. Observed exactly that
/// way on 2026-09-04 while capturing a log:
///
/// ```text
/// failed printing to stderr: Broken pipe (os error 32)
///   std::io::stdio::_eprint
///   azul_core::diagnostics::default_sink        @ diagnostics.rs:50
///   azul_core::diagnostics::emit                @ diagnostics.rs:116
///   azul_layout::widgets::warn_widget_needs_a_name
///   ...
///   azul::desktop::app::App::run
/// ```
///
/// A framework warning about a missing accessible name took the whole window
/// down. The ring below already refuses to do that ("a poisoned diagnostics
/// ring must never take the app down"); the printing half never got the same
/// rule. Diagnostics are advisory by definition - failing to deliver one is
/// never worth the process.
#[cfg(feature = "std")]
fn write_diagnostic(mut out: impl std::io::Write, message: &str) {
    use std::io::Write as _;
    // `drop`, not `let _ =`: this crate denies `let_underscore_drop`, and
    // `io::Result` owns a boxed error. Discarding it is the POINT here - see
    // above - so say so explicitly.
    drop(writeln!(out, "{message}"));
}

#[cfg(feature = "std")]
fn default_sink(message: &str) {
    write_diagnostic(std::io::stderr().lock(), message);
}

/// Distinct [`Diagnostics::emit_once`] keys remembered. Past this the tap is closed: one
/// notice, then no new once-only finding is printed (a DOM that is wrong in a
/// thousand ways has a handful of bugs, and the log must stay readable).
#[cfg(feature = "std")]
const MAX_ONCE_KEYS: usize = 1024;

/// The [`Diagnostics::emit_once`] keys emitted so far, and whether the cap was reached.
#[cfg(feature = "std")]
#[derive(Debug, Default)]
struct OnceKeys {
    keys: BTreeSet<u64>,
    capped: bool,
}

/// The recorder of framework diagnostics: the bounded ring of what was said, the sink it is
/// sent to, the scenario it is tagged with, and the findings that are said once.
///
/// The process has one (the free functions of this module); a test of the recorder makes its
/// own with [`Diagnostics::new`].
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct Diagnostics {
    /// The last [`CAPACITY`] diagnostics, oldest first.
    ring: Mutex<VecDeque<String>>,
    /// Where a diagnostic goes besides the ring.
    sink: Mutex<DiagnosticSink>,
    /// The scenario currently running, attached to every diagnostic emitted while it is set.
    ///
    /// This is the label that makes a Grafana view legible. Without it the ring and the log
    /// stream are one flat sequence and a failure cannot be attributed to the test that
    /// provoked it; with it, filtering by `test="scroll_momentum"` gives that scenario's whole
    /// story — the lints it tripped, in order, and the assertion that finally failed.
    ///
    /// Deliberately ONE string, not a stack: e2e scenarios run serially precisely so their
    /// output can be read in order, and a nested notion of "current test" would be a
    /// cardinality problem in the metric store for no benefit.
    scope: Mutex<Option<String>>,
    /// The findings said once ([`Self::emit_once`]).
    once: Mutex<OnceKeys>,
    /// What the tests that share this recorder take ([`Self::test_lock`]).
    test_lock: Mutex<()>,
    /// The dials of the environment ([`DiagSettings`]): installed once at the start, then only
    /// read; nothing silenced and no trace until then.
    settings: OnceLock<DiagSettings>,
}

#[cfg(feature = "std")]
impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl Diagnostics {
    /// A recorder with an empty ring, the default sink (stderr), no scenario and nothing said.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ring: Mutex::new(VecDeque::new()),
            sink: Mutex::new(default_sink),
            scope: Mutex::new(None),
            once: Mutex::new(OnceKeys {
                keys: BTreeSet::new(),
                capped: false,
            }),
            test_lock: Mutex::new(()),
            settings: OnceLock::new(),
        }
    }

    /// Installs the dials of the environment; the first install wins, so what is silenced never
    /// changes while a process runs.
    pub fn install_settings(&self, settings: DiagSettings) {
        drop(self.settings.set(settings));
    }

    /// The dials of the environment: [`DiagSettings::new`] (nothing silenced, no trace) until
    /// they were installed.
    #[must_use]
    pub fn settings(&self) -> &DiagSettings {
        self.settings.get().unwrap_or(&NO_SETTINGS)
    }

    /// Route diagnostics somewhere other than stderr.
    ///
    /// Passing a sink that does nothing silences them entirely while STILL recording to the
    /// ring, so tests keep working when an app has muted its output.
    pub fn set_sink(&self, new_sink: DiagnosticSink) {
        if let Ok(mut s) = self.sink.lock() {
            *s = new_sink;
        }
    }

    /// Name the scenario now running. Pass `None` when it ends.
    pub fn set_scope(&self, name: Option<String>) {
        if let Ok(mut sc) = self.scope.lock() {
            *sc = name;
        }
    }

    /// The scenario now running, if any.
    #[must_use]
    pub fn current_scope(&self) -> Option<String> {
        self.scope.lock().ok().and_then(|sc| sc.clone())
    }

    /// Emit a framework diagnostic: send it to the sink AND record it.
    ///
    /// Callers pass the fully-formatted message, tag included, exactly as it should
    /// appear in a log.
    pub fn emit(&self, message: String) {
        // Tag with the running scenario so a Grafana query can follow ONE test.
        let tagged = match self.current_scope() {
            Some(test) => format!("[test={test}] {message}"),
            None => message,
        };
        let f = self.sink.lock().map(|s| *s).unwrap_or(default_sink);
        f(&tagged);
        self.record(tagged);
    }

    /// [`Self::emit`] a diagnostic ONCE per `key`: a key already emitted prints nothing.
    ///
    /// `key` names the FINDING (the problem, not the pass that saw it - a lint keys
    /// it by what the node is and how it is selected). A lint that runs after
    /// layout passes otherwise repeats its lines for as long as the app shows the
    /// shape. `message` is built only when it is printed. [`Self::clear`] forgets the
    /// keys with the ring, so a test or an e2e step that clears sees a finding
    /// again. Returns whether the diagnostic was emitted.
    pub fn emit_once(&self, key: u64, message: impl FnOnce() -> String) -> bool {
        let newly_capped = {
            let Ok(mut once) = self.once.lock() else {
                return false; // a poisoned set must never take the app down
            };
            if once.capped || once.keys.contains(&key) {
                return false;
            }
            if once.keys.len() >= MAX_ONCE_KEYS {
                once.capped = true;
                true
            } else {
                once.keys.insert(key);
                false
            }
        };
        if newly_capped {
            self.emit(format!(
                "[azul][diagnostics] {MAX_ONCE_KEYS} distinct findings reported - further \
                 findings are not printed"
            ));
            return false;
        }
        self.emit(message());
        true
    }

    /// Record without printing — for the rare diagnostic that has already been
    /// printed by other means but should still be assertable.
    pub fn record(&self, message: String) {
        let Ok(mut r) = self.ring.lock() else {
            return; // a poisoned diagnostics ring must never take the app down
        };
        if r.len() == CAPACITY {
            r.pop_front();
        }
        r.push_back(message);
    }

    /// Every diagnostic recorded so far, oldest first.
    #[must_use]
    pub fn recorded(&self) -> Vec<String> {
        self.ring
            .lock()
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Does any recorded diagnostic contain `needle`?
    #[must_use]
    pub fn any_contains(&self, needle: &str) -> bool {
        self.ring
            .lock()
            .map(|r| r.iter().any(|m| m.contains(needle)))
            .unwrap_or(false)
    }

    /// Drop everything recorded so far.
    ///
    /// An e2e scenario clears before the step it means to observe, so a warning
    /// emitted during startup cannot satisfy — or spoil — a later assertion.
    pub fn clear(&self) {
        if let Ok(mut r) = self.ring.lock() {
            r.clear();
        }
        // The once-only findings with it: a step that clears to observe a lint
        // must see it fire again (`emit_once`).
        if let Ok(mut once) = self.once.lock() {
            once.keys.clear();
            once.capped = false;
        }
    }

    /// One lock for the tests that share this recorder.
    ///
    /// A shared ring needs tests that touch it not to run concurrently, and two modules each
    /// having their own private lock does NOT achieve that — it was tried, and `dom_lint`'s tests
    /// and the e2e `assert_stderr` tests promptly raced, passing alone and failing together. A
    /// test that makes a recorder of its own needs none.
    #[must_use]
    pub const fn test_lock(&self) -> &Mutex<()> {
        &self.test_lock
    }
}

// The recorder of the process: the one the free functions below ask. Process-wide because its
// emitters (lints and warnings of azul-core and azul-layout) carry no window or app to ask, and
// the e2e harness reads what they said through `recorded`; see the module docs for what it would
// take to give it an owner.
#[cfg(feature = "std")]
static PROCESS: Diagnostics = Diagnostics::new();

/// Route framework diagnostics somewhere other than stderr.
///
/// Call once at startup. Passing a sink that does nothing silences them
/// entirely while STILL recording to the ring, so tests keep working when an
/// app has muted its output.
#[cfg(feature = "std")]
pub fn set_sink(new_sink: DiagnosticSink) {
    PROCESS.set_sink(new_sink);
}

/// Name the scenario now running. Pass `None` when it ends.
#[cfg(feature = "std")]
pub fn set_scope(name: Option<String>) {
    PROCESS.set_scope(name);
}

/// The scenario now running, if any.
#[cfg(feature = "std")]
#[must_use]
pub fn current_scope() -> Option<String> {
    PROCESS.current_scope()
}

/// Emit a framework diagnostic: send it to the sink AND record it.
///
/// Callers pass the fully-formatted message, tag included, exactly as it should
/// appear in a log.
#[cfg(feature = "std")]
pub fn emit(message: String) {
    PROCESS.emit(message);
}

/// [`emit`] a diagnostic ONCE per `key`: a key already emitted prints nothing.
///
/// `key` names the FINDING (the problem, not the pass that saw it - a lint keys
/// it by what the node is and how it is selected). A lint that runs after
/// layout passes otherwise repeats its lines for as long as the app shows the
/// shape. `message` is built only when it is printed. [`clear`] forgets the
/// keys with the ring, so a test or an e2e step that clears sees a finding
/// again. Returns whether the diagnostic was emitted.
#[cfg(feature = "std")]
pub fn emit_once(key: u64, message: impl FnOnce() -> String) -> bool {
    PROCESS.emit_once(key, message)
}

/// Record without printing — for the rare diagnostic that has already been
/// printed by other means but should still be assertable.
#[cfg(feature = "std")]
pub fn record(message: String) {
    PROCESS.record(message);
}

/// Every diagnostic recorded so far, oldest first.
#[cfg(feature = "std")]
#[must_use]
pub fn recorded() -> Vec<String> {
    PROCESS.recorded()
}

/// Does any recorded diagnostic contain `needle`?
#[cfg(feature = "std")]
#[must_use]
pub fn any_contains(needle: &str) -> bool {
    PROCESS.any_contains(needle)
}

/// One lock for the one shared ring, for tests ANYWHERE in the workspace.
///
/// The ring is global, so tests that touch it must not run concurrently. Two
/// modules each having their own private lock does NOT achieve that — it was
/// tried, and `dom_lint`'s tests and the e2e `assert_stderr` tests promptly
/// raced, passing alone and failing together. A shared resource needs a shared
/// lock, and the honest place for it is beside the resource.
///
/// A test of the recorder itself makes a [`Diagnostics`] and needs none.
#[cfg(feature = "std")]
#[must_use]
pub fn test_lock() -> &'static Mutex<()> {
    PROCESS.test_lock()
}

/// Drop everything recorded so far.
///
/// An e2e scenario clears before the step it means to observe, so a warning
/// emitted during startup cannot satisfy — or spoil — a later assertion.
#[cfg(feature = "std")]
pub fn clear() {
    PROCESS.clear();
}

/// Installs the diagnostic dials of the process's environment, once at the start.
///
/// That is [`DiagSettings::from_lookup`]; the first install wins.
#[cfg(feature = "std")]
pub fn install_settings(settings: DiagSettings) {
    PROCESS.install_settings(settings);
}

/// The diagnostic dials of the process: what was installed ([`install_settings`]), nothing
/// silenced and no trace before that.
#[cfg(feature = "std")]
#[must_use]
pub fn settings() -> &'static DiagSettings {
    PROCESS.settings()
}

/// `no_std` builds have no environment: the dials of an empty one.
#[cfg(not(feature = "std"))]
pub fn install_settings(_settings: DiagSettings) {}

/// `no_std` builds have no environment: the dials of an empty one.
#[cfg(not(feature = "std"))]
#[must_use]
pub fn settings() -> &'static DiagSettings {
    &NO_SETTINGS
}

#[cfg(not(feature = "std"))]
pub fn emit(_message: alloc::string::String) {}
#[cfg(not(feature = "std"))]
pub fn record(_message: alloc::string::String) {}
#[cfg(not(feature = "std"))]
pub fn emit_once(_key: u64, _message: impl FnOnce() -> alloc::string::String) -> bool {
    false
}
#[cfg(not(feature = "std"))]
#[must_use]
pub fn recorded() -> alloc::vec::Vec<alloc::string::String> {
    alloc::vec::Vec::new()
}
#[cfg(not(feature = "std"))]
#[must_use]
pub fn any_contains(_needle: &str) -> bool {
    false
}
#[cfg(not(feature = "std"))]
pub fn clear() {}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod diagnostics_test;
