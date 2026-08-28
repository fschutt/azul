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

#[cfg(feature = "std")]
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};

/// Most diagnostics retained. Older ones are dropped.
#[cfg(feature = "std")]
const CAPACITY: usize = 256;

#[cfg(feature = "std")]
fn ring() -> &'static Mutex<VecDeque<String>> {
    static RING: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Where diagnostics go, besides the ring.
///
/// Default writes to stderr so a developer running the app sees warnings with
/// no setup. An application replaces it with [`set_sink`] to route through its
/// own logger — and from there to a remote store.
#[cfg(feature = "std")]
pub type DiagnosticSink = fn(&str);

#[cfg(feature = "std")]
fn default_sink(message: &str) {
    eprintln!("{message}");
}

#[cfg(feature = "std")]
fn sink() -> &'static Mutex<DiagnosticSink> {
    static SINK: OnceLock<Mutex<DiagnosticSink>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(default_sink))
}

/// Route framework diagnostics somewhere other than stderr.
///
/// Call once at startup. Passing a sink that does nothing silences them
/// entirely while STILL recording to the ring, so tests keep working when an
/// app has muted its output.
#[cfg(feature = "std")]
pub fn set_sink(new_sink: DiagnosticSink) {
    if let Ok(mut s) = sink().lock() {
        *s = new_sink;
    }
}

/// The scenario currently running, attached to every diagnostic emitted while
/// it is set.
///
/// This is the label that makes a Grafana view legible. Without it the ring and
/// the log stream are one flat sequence and a failure cannot be attributed to
/// the test that provoked it; with it, filtering by `test="scroll_momentum"`
/// gives that scenario's whole story — the lints it tripped, in order, and the
/// assertion that finally failed.
///
/// Deliberately ONE string, not a stack: e2e scenarios run serially precisely
/// so their output can be read in order, and a nested notion of "current test"
/// would be a cardinality problem in the metric store for no benefit.
#[cfg(feature = "std")]
fn scope() -> &'static Mutex<Option<String>> {
    static SCOPE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SCOPE.get_or_init(|| Mutex::new(None))
}

/// Name the scenario now running. Pass `None` when it ends.
#[cfg(feature = "std")]
pub fn set_scope(name: Option<String>) {
    if let Ok(mut sc) = scope().lock() {
        *sc = name;
    }
}

/// The scenario now running, if any.
#[cfg(feature = "std")]
#[must_use]
pub fn current_scope() -> Option<String> {
    scope().lock().ok().and_then(|sc| sc.clone())
}

/// Emit a framework diagnostic: send it to the sink AND record it.
///
/// Callers pass the fully-formatted message, tag included, exactly as it should
/// appear in a log.
#[cfg(feature = "std")]
pub fn emit(message: String) {
    // Tag with the running scenario so a Grafana query can follow ONE test.
    let tagged = match current_scope() {
        Some(test) => format!("[test={test}] {message}"),
        None => message,
    };
    let f = sink().lock().map(|s| *s).unwrap_or(default_sink);
    f(&tagged);
    record(tagged);
}

/// Record without printing — for the rare diagnostic that has already been
/// printed by other means but should still be assertable.
#[cfg(feature = "std")]
pub fn record(message: String) {
    let Ok(mut r) = ring().lock() else {
        return; // a poisoned diagnostics ring must never take the app down
    };
    if r.len() == CAPACITY {
        r.pop_front();
    }
    r.push_back(message);
}

/// Every diagnostic recorded so far, oldest first.
#[cfg(feature = "std")]
#[must_use]
pub fn recorded() -> Vec<String> {
    ring()
        .lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Does any recorded diagnostic contain `needle`?
#[cfg(feature = "std")]
#[must_use]
pub fn any_contains(needle: &str) -> bool {
    ring()
        .lock()
        .map(|r| r.iter().any(|m| m.contains(needle)))
        .unwrap_or(false)
}

/// One lock for the one shared ring, for tests ANYWHERE in the workspace.
///
/// The ring is global, so tests that touch it must not run concurrently. Two
/// modules each having their own private lock does NOT achieve that — it was
/// tried, and `dom_lint`'s tests and the e2e `assert_stderr` tests promptly
/// raced, passing alone and failing together. A shared resource needs a shared
/// lock, and the honest place for it is beside the resource.
#[cfg(feature = "std")]
#[must_use]
pub fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Drop everything recorded so far.
///
/// An e2e scenario clears before the step it means to observe, so a warning
/// emitted during startup cannot satisfy — or spoil — a later assertion.
#[cfg(feature = "std")]
pub fn clear() {
    if let Ok(mut r) = ring().lock() {
        r.clear();
    }
}

#[cfg(not(feature = "std"))]
pub fn emit(_message: alloc::string::String) {}
#[cfg(not(feature = "std"))]
pub fn record(_message: alloc::string::String) {}
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
