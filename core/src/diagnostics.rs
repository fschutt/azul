//! Framework diagnostics: warnings the engine emits about the app's own DOM,
//! recorded in-process so a test can assert on them. Lints like `image-churn` and
//! `text-without-block` exist to tell a developer that something they built will
//! misbehave.

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

/// Where diagnostics go, besides the ring. Default writes to stderr so a developer
/// running the app sees warnings with no setup.
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

/// Route framework diagnostics somewhere other than stderr. Call once at startup.
#[cfg(feature = "std")]
pub fn set_sink(new_sink: DiagnosticSink) {
    if let Ok(mut s) = sink().lock() {
        *s = new_sink;
    }
}

/// The scenario currently running, attached to every diagnostic emitted while it is
/// set. This is the label that makes a Grafana view legible.
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

/// Emit a framework diagnostic: send it to the sink AND record it. Callers pass the
/// fully-formatted message, tag included, exactly as it should appear in a log.
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

/// Record without printing - for the rare diagnostic that has already been printed
/// by other means but should still be assertable.
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

/// One lock for the one shared ring, for tests ANYWHERE in the workspace. The ring
/// is global, so tests that touch it must not run concurrently.
#[cfg(feature = "std")]
#[must_use]
pub fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Drop everything recorded so far. An e2e scenario clears before the step it means
/// to observe, so a warning emitted during startup cannot satisfy - or spoil - a
/// later assertion.
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

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    // Uses the workspace-wide lock: see `diagnostics::test_lock`.

    #[test]
    fn a_recorded_diagnostic_is_findable_and_clearable() {
        let _g = test_lock().lock();
        clear();
        emit("[azul][test-lint] hello".to_string());
        assert!(any_contains("test-lint"));
        assert!(!any_contains("absent-lint"));
        clear();
        assert!(!any_contains("test-lint"), "clear() must empty the ring");
    }

    #[test]
    fn the_ring_is_bounded() {
        let _g = test_lock().lock();
        clear();
        for i in 0..(CAPACITY + 50) {
            record(format!("msg {i}"));
        }
        assert_eq!(
            recorded().len(),
            CAPACITY,
            "an every-frame lint must not grow without bound — the point is to \
             catch churn, not become it"
        );
        assert!(!any_contains("msg 0"), "the oldest entries are dropped first");
        clear();
    }
}
