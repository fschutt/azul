//! Every windowing backend must wire the shared per-frame machinery.
//!
//! These are SOURCE-SCANNING tests, deliberately. The alternative — a trait
//! method every backend must implement — is the better design, but it does not
//! exist today, and until it does the absence of a call site is completely
//! silent: nothing fails to compile, no test goes red, and the feature is simply
//! missing on that platform forever. That is exactly how these were found:
//!
//!   * `process_timers_and_threads()` had ZERO call sites on iOS and Android, so
//!     no Timer fired, no background Thread writeback was collected and no
//!     animation advanced on either platform. Fixed in 822c2a7fd.
//!   * `process_accessibility_actions()` had ZERO implementations on iOS,
//!     Android and headless — not even a field to receive an action — so a
//!     screen reader's request did nothing on any of the three, and headless
//!     being one of them meant the E2E corpus could not observe accessibility
//!     at all. Fixed by giving headless an injectable queue, iOS a
//!     `UIAccessibilityContainer` bridge and Android an
//!     `AccessibilityNodeProvider` bridge.
//!
//! A scan is a weak check, but a weak check that goes red beats a strong
//! abstraction nobody has written. When the trait exists, delete this file.

use std::path::PathBuf;

fn backend_src(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/desktop/shell2")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Backends that drive their own frame loop and therefore owe the shared pumps.
/// headless is included: it is the backend the E2E suite runs on, so a gap there
/// is a gap in everything the corpus claims to prove.
const FRAME_DRIVING_BACKENDS: &[&str] = &[
    "windows/mod.rs",
    "macos/mod.rs",
    "linux/x11/mod.rs",
    "linux/wayland/mod.rs",
    "ios/mod.rs",
    "android/mod.rs",
    "headless/mod.rs",
];

#[test]
fn every_backend_pumps_timers_and_threads() {
    let missing: Vec<&str> = FRAME_DRIVING_BACKENDS
        .iter()
        .copied()
        .filter(|b| !backend_src(b).contains("process_timers_and_threads()"))
        .collect();

    assert!(
        missing.is_empty(),
        "these backends never pump timers/threads, so no Timer fires, no Thread writeback is \
         collected and no animation advances on them: {missing:?}",
    );
}

/// Accessibility actions must dispatch on every backend that drives frames.
///
/// `process_accessibility_actions` is an inherent per-platform method. iOS,
/// Android and headless never grew one, so a11y actions are inert there —
/// headless especially, since it is the backend the E2E corpus runs on.
///
/// Fix by implementing the dispatch, or by giving those backends an explicit stub
/// that documents why they cannot have one. Narrowing the backend list to make
/// this pass would only restore the silence it exists to break.
#[test]
fn every_backend_dispatches_accessibility_actions() {
    let missing: Vec<&str> = FRAME_DRIVING_BACKENDS
        .iter()
        .copied()
        .filter(|b| !backend_src(b).contains("process_accessibility_actions"))
        .collect();

    assert!(
        missing.is_empty(),
        "these backends never dispatch accessibility actions, so a11y is inert on them: \
         {missing:?}",
    );
}
