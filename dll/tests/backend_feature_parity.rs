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
//!   * `process_accessibility_actions()` still has no implementation on iOS,
//!     Android or headless, so accessibility actions never dispatch there.
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

/// EXPECTED RED. iOS, Android and headless have no accessibility dispatch at
/// all — it is an inherent per-platform method that those three simply never
/// grew, so a11y actions are silently inert there.
///
/// Committed failing on purpose. This is a real gap, and a red test is the only
/// form of it anyone will see; a comment or a task entry is not. Do not
/// `#[ignore]` it and do not narrow the backend list to make it pass — implement
/// the dispatch, or give the three of them an explicit, documented stub that
/// says why they cannot have one.
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
