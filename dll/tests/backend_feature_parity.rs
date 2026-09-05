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

/// A system theme switch must reach the window at runtime.
///
/// The system style is captured when the window is created. A backend that
/// never re-reads it leaves a user toggling dark mode looking at the startup
/// theme until the app restarts: the DOM keeps it, and `prefers-color-scheme`
/// styling never re-evaluates. `EventType::ThemeChange` and
/// `WindowEventFilter::ThemeChanged` are fully wired in azul-core, so all a
/// backend owes is the observation and a regeneration tagged
/// [`RelayoutReason::ThemeChange`].
///
/// Each platform's notification, and none of them are the same API:
///   * Windows — `WM_SETTINGCHANGE | WM_THEMECHANGED`;
///   * macOS   — `NSView::viewDidChangeEffectiveAppearance`, with a slow poll
///     of `NSApp.effectiveAppearance` behind it for the facets (accent colour,
///     UI font) that fire no view callback;
///   * X11 and Wayland — `org.freedesktop.portal.Settings.SettingChanged` for
///     `org.freedesktop.appearance`/`color-scheme`. There is no Wayland
///     protocol and no XSETTINGS key for it; the portal is the mechanism on
///     both, so one watcher thread serves both backends and wakes their
///     `poll(2)` through an eventfd;
///   * iOS     — `UITraitCollection.userInterfaceStyle`;
///   * Android — `onConfigurationChanged` with `UI_MODE_NIGHT_MASK`, latched
///     for the loop thread to drain;
///   * headless — no system theme to observe, so the injection IS the ingress
///     (`HeadlessWindow::set_system_theme`). It is the backend the E2E corpus
///     runs on, so without it no scenario could cover theme-dependent layout.
///
/// SCAN KEY, and why it changed. The original check was `mod.rs` contains
/// `RelayoutReason::ThemeChange`, chosen because requesting a regeneration
/// under that reason is the thing only a real runtime handler does — a
/// constructor calling `discover_system_style()` does not. That stopped being
/// true when the theme-switch POLICY moved into
/// `PlatformWindow::adopt_system_style`: choosing between a full rebuild and a
/// restyle needs both ends of the transition and every backend was making that
/// choice identically, so the four desktop backends now hand the new style to
/// the shared helper and the literal lives only there. The scan went red on
/// windows/macos/x11/wayland — the four that had REAL handlers — while the
/// three that inline it stayed green, i.e. it had inverted.
///
/// So the key is now "requests a ThemeChange regeneration, directly or through
/// the one shared helper that does", and the helper is checked separately for
/// still doing it. That is not a widening: `adopt_system_style` has exactly one
/// behaviour, and a helper that stopped tagging `ThemeChange` would now fail
/// here where before it would have gone unnoticed at every one of its callers.
/// The observation itself — the half a `RelayoutReason` cannot speak for — is
/// pinned by `every_backend_names_the_os_notification_it_observes` below, and
/// the behaviour by `headless_lifecycle`'s
/// `a_theme_switch_changes_the_theme_and_requests_a_frame`.
///
/// Making this pass by shortening the backend list would restore exactly the
/// silence it exists to break.
#[test]
fn every_backend_reacts_to_a_runtime_theme_change() {
    let missing: Vec<&str> = FRAME_DRIVING_BACKENDS
        .iter()
        .copied()
        .filter(|b| {
            let src = backend_src(b);
            !src.contains("RelayoutReason::ThemeChange") && !src.contains("adopt_system_style(")
        })
        .collect();

    assert!(
        missing.is_empty(),
        "these backends never request a regeneration tagged ThemeChange, so a user toggling \
         dark mode sees no change until restart: {missing:?}",
    );

    // The other half of the key above: routing through the shared helper is
    // only as good as what the helper does. If this ever fails, every backend
    // in the list that delegates has silently stopped tagging its rebuild.
    let policy = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/desktop/shell2/common/event.rs");
    let policy_src = std::fs::read_to_string(&policy)
        .unwrap_or_else(|e| panic!("read {}: {e}", policy.display()));
    let adopt = policy_src
        .split_once("fn adopt_system_style(")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| {
            panic!(
                "PlatformWindow::adopt_system_style is gone from {}; the backends that delegate \
                 to it are no longer covered by the scan above",
                policy.display()
            )
        });
    // Bounded to the helper's own body — the next `fn` at the same nesting
    // starts the following method.
    let body = adopt.split("\n    fn ").next().unwrap_or(adopt);
    assert!(
        body.contains("RelayoutReason::ThemeChange"),
        "PlatformWindow::adopt_system_style no longer requests a ThemeChange regeneration, so \
         every backend that delegates its theme switch to it now rebuilds under some other \
         reason (or not at all) — and LayoutCallbackInfo::relayout_reason() stops telling \
         callbacks to re-read system colours",
    );
}

/// Reacting is not observing: each backend must NAME its OS notification.
///
/// `adopt_system_style` is reachable from anywhere, a constructor included, so
/// the scan above cannot tell a backend that HEARS the OS from one that merely
/// owns the machinery to respond if it ever did. This table is the missing
/// half: each backend is pinned to the specific symbol that carries the
/// platform's own announcement into it. They are deliberately all different —
/// there is no cross-platform notification for this, which is the entire reason
/// six backends went without one for so long.
///
/// If a rename makes this fail, update the string to the new name. If a DELETE
/// makes it fail, the backend has lost its ability to hear a theme switch and
/// the string is not the thing to change.
const THEME_OBSERVATION: &[(&str, &str)] = &[
    // The window message Windows broadcasts for an appearance change.
    ("windows/mod.rs", "WM_SETTINGCHANGE"),
    // AppKit's per-view announcement; the backstop poll sits behind it.
    ("macos/mod.rs", "viewDidChangeEffectiveAppearance"),
    // Both Linux backends read the portal watcher's answer through this.
    ("linux/x11/mod.rs", "adopt_observed_theme"),
    ("linux/wayland/mod.rs", "adopt_observed_theme"),
    // UIKit's trait collection, polled on the main-thread tick.
    ("ios/mod.rs", "adopt_device_appearance"),
    // Latched by the Java UI thread's onConfigurationChanged, drained here.
    ("android/mod.rs", "drain_pending_theme"),
    // No OS to hear: the injection is the ingress.
    ("headless/mod.rs", "set_system_theme"),
];

#[test]
fn every_backend_names_the_os_notification_it_observes() {
    // Every frame-driving backend must appear, or a backend could be added
    // with no observation at all and this test would say nothing about it.
    let untabled: Vec<&str> = FRAME_DRIVING_BACKENDS
        .iter()
        .copied()
        .filter(|b| !THEME_OBSERVATION.iter().any(|(name, _)| name == b))
        .collect();
    assert!(
        untabled.is_empty(),
        "these frame-driving backends have no entry in THEME_OBSERVATION, so nothing checks \
         that they can hear a theme switch at all: {untabled:?}",
    );

    let missing: Vec<(&str, &str)> = THEME_OBSERVATION
        .iter()
        .copied()
        .filter(|(backend, symbol)| !backend_src(backend).contains(symbol))
        .collect();

    assert!(
        missing.is_empty(),
        "these backends no longer name the OS notification that tells them the theme changed, \
         so they can still ADOPT a new system style but nothing will ever hand them one: \
         {missing:?}",
    );
}
