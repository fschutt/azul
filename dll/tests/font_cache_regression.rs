//! Regression tests for font resolution at startup.
//!
//! Two independent bugs are locked down here.
//!
//! ## 1. "no text renders at all" (macOS) — the `regenerate_layout` guard
//!
//! The condition deciding whether to block on `FcFontRegistry::request_fonts()`
//! used to read `cache_empty || build_complete`. On the first layout of a real
//! macOS app BOTH disjuncts are false — the cache already holds a couple of
//! patterns (not "empty") while the scan of ~370 system fonts is still running
//! (not "complete") — so `request_fonts()` was never called and layout ran
//! against a two-font cache containing none of the platform's UI families.
//! Every family missed and text fell through to LAST-RESORT.
//!
//! ## 2. the disk cache was never written
//!
//! `App` creation calls `load_from_disk_cache()` and logs a hit — but nothing,
//! in azul or in rust-fontconfig, ever called `save_to_disk_cache()`. The
//! manifest never existed, so the load could only ever miss and every launch
//! paid the full cold scan.
//!
//! These are integration tests: they touch the real system font set. They fail
//! loudly rather than skipping when the machine has no fonts at all, because a
//! font test that silently passes by skipping is the exact failure mode being
//! fixed.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo};
use azul_core::dom::{Dom, NodeData};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::registry::FcFontRegistry;
use rust_fontconfig::{
    FcFallbackConfig, FcFont, FcFontCache, FcPattern, GenericFamily, OperatingSystem,
};

use azul::desktop::shell2::common::layout::should_request_fonts;
use azul::desktop::shell2::common::PlatformWindow;
use azul::desktop::shell2::headless::HeadlessWindow;

// ---------------------------------------------------------------------------
// 1. The guard itself
// ---------------------------------------------------------------------------

/// The complete truth table of `should_request_fonts`.
///
/// The shipped bug lived as a boolean expression inside a function that needs a
/// window, a GL context and a live font registry to reach — a place no test
/// could see. Extracting the decision gives it four asserts.
///
/// The middle case is THE bug: build incomplete, cache non-empty.
#[test]
fn should_request_fonts_truth_table() {
    // Build finished → the cache is authoritative, don't re-request.
    assert!(
        !should_request_fonts(true, false),
        "a complete build with a populated cache must not re-request"
    );
    // Build finished but the cache is somehow empty → request anyway; an empty
    // cache renders nothing at all.
    assert!(
        should_request_fonts(true, true),
        "an empty cache must always trigger a request"
    );
    // THE REGRESSION: build still running, cache holds a couple of patterns.
    // The old `cache_empty || build_complete` returned FALSE here, skipped
    // request_fonts(), and laid out against a near-empty cache.
    assert!(
        should_request_fonts(false, false),
        "an INCOMPLETE build must request fonts even when the cache is \
         non-empty — this is the macOS 'no text renders' bug: two stale \
         patterns are not an excuse to skip the blocking request"
    );
    // Build running and cache empty → obviously request.
    assert!(
        should_request_fonts(false, true),
        "an incomplete build with an empty cache must request fonts"
    );
}

// ---------------------------------------------------------------------------
// 2. The same condition, end to end through regenerate_layout
// ---------------------------------------------------------------------------

fn mock_font_bytes() -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("layout")
        .join("tests")
        .join("fonts")
        .join("azul-mock-prop.ttf");
    std::fs::read(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"))
}

/// A cache that is NON-EMPTY but tiny — the exact shape macOS presented on the
/// first layout (2 patterns, none of them a real UI family).
fn two_pattern_cache() -> FcFontCache {
    let bytes = mock_font_bytes();
    let cache = FcFontCache::default();
    cache.with_memory_fonts(vec![
        (
            FcPattern {
                name: Some("AzulMockA".to_string()),
                family: Some("AzulMockA".to_string()),
                ..Default::default()
            },
            FcFont {
                bytes: bytes.clone(),
                font_index: 0,
                id: "azul-mock-a".to_string(),
            },
        ),
        (
            FcPattern {
                name: Some("AzulMockB".to_string()),
                family: Some("AzulMockB".to_string()),
                ..Default::default()
            },
            FcFont {
                bytes,
                font_index: 0,
                id: "azul-mock-b".to_string(),
            },
        ),
    ]);
    cache
}

extern "C" fn text_layout_cb(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(Dom::create_from_data(
        NodeData::create_text_do_not_use_without_block_level_wrapper(
            "The quick brown fox jumps over the lazy dog",
        ),
    ))
}

/// End-to-end proof of the guard: a registry whose build is INCOMPLETE, paired
/// with a NON-EMPTY font cache, must still end up with the system's fonts in
/// the cache after one layout pass.
///
/// The incomplete state is made deterministic with lazy-scout mode: the scout
/// enumerates font files but queues nothing, so `build_complete` never flips on
/// its own and only `request_fonts()` can populate the cache. If the guard is
/// wrong, `request_fonts()` is never called and no system family ever enters
/// the cache — which is precisely what shipped.
#[test]
fn incomplete_build_with_nonempty_cache_still_loads_system_fonts() {
    // Ground truth FIRST, from an independent full scan: which of this
    // platform's UI families actually exist here. Establishing it up front is
    // what keeps the skip honest — the test can only be skipped because the
    // machine has no such fonts, never because the code under test failed to
    // find them.
    let Some(expected) = installed_platform_ui_family() else {
        eprintln!(
            "\n=====================================================================\n\
             SKIPPED: none of {:?}'s declared sans-serif UI families\n\
             ({:?})\n\
             are installed on this machine, so there is no family whose\n\
             resolution this test could assert. Install any one of them to get\n\
             real coverage here.\n\
             =====================================================================\n",
            OperatingSystem::current(),
            platform_sans_serif_families(),
        );
        return;
    };
    let expected_family: &str = &expected;
    eprintln!("ground truth: {expected_family:?} is installed on this machine");

    let registry = FcFontRegistry::new();
    // Lazy scout: enumerate paths, queue no build jobs. `build_complete` can
    // then only be reached via an explicit request, making "incomplete" stable
    // instead of a race against a 190ms background scan.
    registry.set_scout_lazy(true);
    registry.spawn_scout_and_builders();

    let deadline = Instant::now() + Duration::from_secs(60);
    while !registry.is_scan_complete() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        registry.is_scan_complete(),
        "the font scout did not finish enumerating within 60s"
    );
    assert!(
        !registry.is_build_complete(),
        "lazy-scout mode must leave the build INCOMPLETE; without that this \
         test would not reproduce the buggy condition at all"
    );

    let seeded = two_pattern_cache();
    assert_eq!(seeded.len(), 2, "seed cache must hold exactly 2 patterns");

    let app_data = Arc::new(RefCell::new(RefAny::new(0u32)));
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: text_layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };

    let mut window = HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(seeded),
        Some(Arc::clone(&registry)),
    )
    .expect("HeadlessWindow construction must succeed");

    // NOT `== 2`: `LayoutWindow::new` registers azul's built-in mock faces into
    // whatever cache it is handed, so the pre-layout count is a few above the
    // two seeded patterns. That inflation is also why a bare "the cache grew"
    // assertion is worthless here — it passes without the fix. The real
    // assertion is by FAMILY NAME, below.
    let before = families_in(&window);
    assert!(
        !before.contains(expected_family),
        "the pre-layout cache already contains {expected_family:?}; this test \
         cannot then prove that regenerate_layout is what put it there"
    );

    window.regenerate_layout().expect("regenerate_layout");

    let after = families_in(&window);
    assert!(
        after.contains(expected_family),
        "after one layout pass the font cache still does NOT contain the \
         system family {expected_family:?}, which an independent full scan \
         proves IS installed on this machine.\n\
         regenerate_layout never called request_fonts(): the registry build \
         was incomplete and the cache non-empty, which is exactly the \
         condition the old `cache_empty || build_complete` guard got wrong. \
         Every UI family missed and text fell through to LAST-RESORT.\n\
         cache holds {} families after layout.",
        after.len()
    );
    eprintln!("resolved {expected_family:?} through an INCOMPLETE registry build");
}

/// The families currently present in the window's layout font cache.
fn families_in(window: &HeadlessWindow) -> std::collections::BTreeSet<String> {
    window
        .common
        .layout_window
        .as_ref()
        .expect("layout window")
        .font_manager
        .fc_cache
        .list()
        .into_iter()
        .filter_map(|(p, _)| p.family.clone())
        .collect()
}

/// Ground truth, established by a SEPARATE full eager scan: one of the
/// platform's declared sans-serif UI families that is genuinely installed
/// here.
///
/// This must not be derived from the cache under test — that would make the
/// assertion circular. `None` means the machine has none of them, which is the
/// only case where skipping is legitimate.
fn installed_platform_ui_family() -> Option<String> {
    let probe = FcFontRegistry::new();
    probe.spawn_scout_and_builders();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !probe.is_build_complete() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        probe.is_build_complete(),
        "the ground-truth font scan did not finish within 60s"
    );
    assert!(
        !probe.list().is_empty(),
        "the ground-truth scan found ZERO fonts on this machine. This test          cannot run without any installed fonts — failing loudly rather than          passing vacuously."
    );
    let installed: std::collections::BTreeSet<String> = probe
        .list()
        .into_iter()
        .filter_map(|(p, _)| p.family.clone())
        .collect();
    platform_sans_serif_families()
        .into_iter()
        .find(|f| installed.contains(f))
}

/// The per-OS `sans-serif` table (rust-fontconfig's own defaults for this
/// platform, without any fonts.conf aliases - the families the crate
/// DECLARES as this OS's UI fonts).
fn platform_sans_serif_families() -> Vec<String> {
    FcFallbackConfig::os_defaults(OperatingSystem::current())
        .expand_generic(GenericFamily::SansSerif, &[])
}

// ---------------------------------------------------------------------------
// 3. App startup must LEAVE a font cache behind
// ---------------------------------------------------------------------------

/// Creating an `App` must eventually write the on-disk font manifest, so the
/// next launch can take the ~10-20ms `load_from_disk_cache()` path that
/// `AppInternal::create` already contains.
///
/// Runs in a child process with `HOME` / `XDG_CACHE_HOME` redirected at a temp
/// directory: the manifest must be one this run created, not one that happened
/// to be lying around, and the developer's real font cache must not be touched.
#[test]
fn app_startup_persists_the_font_cache() {
    const CHILD_ENV: &str = "AZUL_FONT_CACHE_PERSIST_CHILD";
    const TEST_NAME: &str = "app_startup_persists_the_font_cache";

    if std::env::var_os(CHILD_ENV).is_some() {
        // ---- child ----
        let _app = azul::desktop::app::AppInternal::create(RefAny::new(0u32), AppConfig::default());

        let path = rust_fontconfig::disk_cache::get_font_cache_path()
            .expect("child: get_font_cache_path returned None");
        println!("child: expecting manifest at {path:?}");

        let deadline = Instant::now() + Duration::from_secs(90);
        while !path.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            path.exists(),
            "child: App creation did not persist a font manifest at {path:?}. \
             load_from_disk_cache() in AppInternal::create can therefore never \
             hit, and every launch re-scans every font on the system."
        );
        assert!(
            std::fs::metadata(&path).unwrap().len() > 0,
            "child: manifest exists but is empty"
        );
        return;
    }

    // ---- parent ----
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "azul-font-cache-test-{}-{nanos}",
        std::process::id()
    ));
    let home = root.join("home");
    std::fs::create_dir_all(home.join("Library").join("Caches")).unwrap();
    let xdg = root.join("xdg-cache");
    std::fs::create_dir_all(&xdg).unwrap();

    let exe = std::env::current_exe().expect("current_exe");
    let out = std::process::Command::new(exe)
        .args(["--exact", TEST_NAME, "--nocapture", "--test-threads", "1"])
        .env(CHILD_ENV, "1")
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", &xdg)
        .env("AZ_BACKEND", "headless")
        .output()
        .expect("spawn child test process");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let ok = out.status.success();

    let candidates = [
        home.join("Library")
            .join("Caches")
            .join("rfc")
            .join("fonts")
            .join("manifest.bin"),
        xdg.join("rfc").join("fonts").join("manifest.bin"),
        home.join(".cache")
            .join("rfc")
            .join("fonts")
            .join("manifest.bin"),
    ];
    let found: Vec<_> = candidates.iter().filter(|p| p.exists()).cloned().collect();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        ok,
        "child process failed.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        !found.is_empty(),
        "App startup wrote no font manifest under the redirected cache roots.\n\
         checked: {candidates:?}\n--- child stdout ---\n{stdout}"
    );
}
