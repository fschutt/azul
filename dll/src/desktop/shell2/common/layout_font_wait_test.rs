//! The first layout of a window waits for the font registry's scan only when its DOM needs a system font.
//!
//! The registry here is one whose scan NEVER STARTS: the wait the layout used to make before it built its DOM
//! (`wait_for_fonts`, five seconds at most) cannot end, so a layout of a DOM that is drawn by the faces the app ships
//! must not make it, and one that needs the system must (the scan is started, over no directory, for those cases: it
//! completes at once). The counters of the frame report say which happened.

use std::{
    cell::RefCell,
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason},
    dom::Dom,
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::{AppConfig, NamedFont},
};
use azul_layout::{text3::fontreg::RegisteredFonts, window_state::WindowCreateOptions};
use rust_fontconfig::{config::FcScanConfig, registry::FcFontRegistry, FcFontCache};

use crate::desktop::shell2::{
    common::event::{PlatformWindow, SharedUndoManager},
    headless::HeadlessWindow,
};

/// The family the app ships.
const SHIPPED: &str = "Shipped Face";

/// What the layout callback shows, and in which family.
struct Page {
    text: String,
    family: String,
}

extern "C" fn page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (text, family) = data
        .downcast_ref::<Page>()
        .map_or((String::new(), String::new()), |p| (p.text.clone(), p.family.clone()));
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(&format!("font-family: \"{family}\"; font-size: 16px;"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(&text)),
    )
}

/// A registry that scans nothing (no directory) and writes nothing; its scan is not started.
fn idle_registry() -> Arc<FcFontRegistry> {
    let registry = FcFontRegistry::new_with_config(FcScanConfig {
        font_dirs: Vec::new(),
        priority_families: Vec::new(),
    });
    registry.set_persist_on_complete(false);
    registry
}

/// Runs the scan to its end (it has no directory to read).
fn finish_scan(registry: &Arc<FcFontRegistry>) {
    registry.spawn_scout_and_builders();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !registry.is_build_complete() {
        assert!(Instant::now() < deadline, "an empty scan did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A window showing `text` in `family`, over `registry`, with the app's own fonts when `shipped` is set.
fn window(
    text: &str,
    family: &str,
    registry: &Arc<FcFontRegistry>,
    shipped: bool,
) -> (HeadlessWindow, Arc<RefCell<RefAny>>) {
    let state = Arc::new(RefCell::new(RefAny::new(Page {
        text: text.to_string(),
        family: family.to_string(),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
    let callback: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = page;
    options.window_state.layout_callback = LayoutCallback::create(callback);
    let mut window = HeadlessWindow::new(
        options,
        Arc::clone(&state),
        SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        Some(Arc::clone(registry)),
    )
    .expect("a headless window");
    if shipped {
        let face = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../doc/fonts/RedHatMono-VariableFont_wght.ttf"),
        )
        .expect("the variable font of the repository");
        let fonts = Arc::new(RegisteredFonts::default());
        fonts.set_app_fonts(&[NamedFont::new(SHIPPED.into(), face.into())]);
        window.share_registered_fonts(&fonts);
    }
    (window, state)
}

fn counters(window: &HeadlessWindow) -> (u32, u32) {
    let report = &window
        .get_layout_window()
        .expect("a layout window")
        .frame_report;
    (report.font_waits_skipped, report.font_waits_blocked)
}

#[test]
fn a_dom_the_apps_faces_draw_does_not_wait_for_a_scan_that_has_not_started() {
    let registry = idle_registry();
    // The window's own construction may lay out too: the clock runs from before it.
    let started = Instant::now();
    let (mut window, _state) = window("Hello, world 123", SHIPPED, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(3),
        "the first layout waited {took:?}: the wait for a scan that never starts is five seconds"
    );
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");
    assert!(!registry.is_build_complete(), "premise: the scan has not run");
}

#[test]
fn text_that_arrives_later_and_no_registered_face_draws_takes_the_wait_as_before() {
    let registry = idle_registry();
    let (mut window, state) = window("Hello, world", SHIPPED, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");

    // The app shows a character none of its faces draws; the scan runs meanwhile (it completes at once, so the
    // wait that is owed returns).
    state
        .borrow_mut()
        .downcast_mut::<Page>()
        .expect("the page state")
        .text = "Hello, \u{6f22}\u{5b57}".to_string();
    finish_scan(&registry);
    window.request_regeneration(RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("the second layout");
    let (skipped_after, blocked_after) = counters(&window);
    assert!(
        blocked_after >= 1 && skipped_after == skipped,
        "the second DOM holds text the faces in memory cannot draw: it takes the wait, and the skips stay \
         (skipped {skipped} -> {skipped_after}, blocked {blocked_after})"
    );
}

#[test]
fn a_dom_set_in_a_system_family_takes_the_wait_as_before() {
    let registry = idle_registry();
    finish_scan(&registry);
    let (mut window, _state) = window("Hello", "Some Installed Family", &registry, true);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped == 0 && blocked >= 1, "skipped {skipped}, blocked {blocked}");
}

#[test]
fn an_app_that_ships_no_face_takes_the_wait_as_before() {
    let registry = idle_registry();
    finish_scan(&registry);
    let (mut window, _state) = window("Hello", SHIPPED, &registry, false);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped == 0 && blocked >= 1, "nothing shipped draws the text: skipped {skipped}, blocked {blocked}");
}
