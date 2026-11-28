//! The first layout of a window waits for the font registry's scan only when its DOM needs a system font.
//!
//! The registry here is one whose scan the test starts itself, or NEVER: the wait the layout used to make before it built
//! its DOM (`wait_for_fonts`, five seconds at most) cannot end while the scan has not started, so a layout of a DOM that
//! is drawn by the faces the app ships must not make it, and one that needs the system must (the scan is started a
//! moment later, over no directory: it completes at once). The counters of the frame report say which happened.

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

/// The `font-family` value of a DOM set in the face the app ships.
const SHIPPED_STACK: &str = "\"Shipped Face\"";

/// The same, with the generic family after it that nearly every stylesheet names.
const SHIPPED_THEN_GENERIC: &str = "\"Shipped Face\", sans-serif";

/// What the layout callback shows, and in which `font-family` value.
struct Page {
    text: String,
    stack: String,
}

extern "C" fn page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (text, stack) = data
        .downcast_ref::<Page>()
        .map_or((String::new(), String::new()), |p| (p.text.clone(), p.stack.clone()));
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(&format!("font-family: {stack}; font-size: 16px;"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text.as_str())),
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

/// Starts the scan of `registry` after `after` (it has no directory to read, so it is done at once): the layout that is
/// waiting for it goes on then.
fn start_scan_after(registry: &Arc<FcFontRegistry>, after: Duration) -> std::thread::JoinHandle<()> {
    let registry = Arc::clone(registry);
    std::thread::spawn(move || {
        std::thread::sleep(after);
        registry.spawn_scout_and_builders();
    })
}

/// The wait for a scan that never starts ends at five seconds: a layout that took less did not make it. The layout of a
/// debug build takes a good part of the rest on a runner.
const NOT_THE_FIVE_SECONDS: Duration = Duration::from_millis(4500);

/// How long the scan that a waiting layout depends on is held back, and the least the layout must have waited for it.
const SCAN_STARTS_AFTER: Duration = Duration::from_millis(300);
const WAITED_AT_LEAST: Duration = Duration::from_millis(200);

/// A window showing `text` set in the `font-family` value `stack`, over `registry`, with the app's own fonts when
/// `shipped` is set.
fn window(
    text: &str,
    stack: &str,
    registry: &Arc<FcFontRegistry>,
    shipped: bool,
) -> (HeadlessWindow, Arc<RefCell<RefAny>>) {
    let state = Arc::new(RefCell::new(RefAny::new(Page {
        text: text.to_string(),
        stack: stack.to_string(),
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
        // A static face with Basic Latin (the layout tests' proportional mock): a variable one would be drawn as an
        // instance made on the way.
        let face = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../layout/tests/fonts/azul-mock-prop.ttf"),
        )
        .expect("the mock face of the layout tests");
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
    let (mut window, _state) = window("Hello, world 123", SHIPPED_STACK, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let took = started.elapsed();
    assert!(
        took < NOT_THE_FIVE_SECONDS,
        "the first layout waited {took:?}: the wait for a scan that never starts is five seconds"
    );
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");
    assert!(!registry.is_build_complete(), "premise: the scan has not run");
}

#[test]
fn a_stack_that_names_a_generic_family_after_the_shipped_face_does_not_wait_for_the_scan_either() {
    // Nearly every stylesheet ends its stack in a generic family. The registry's probe for that name waits half a
    // second for a first match when the scout has listed nothing (iOS lists everything in one batch, at the end):
    // the resolver does not ask it while the listing is under way and the faces in memory draw the text, and what
    // the first layout may spend on the name is nothing - not the five seconds of the scan's wait either.
    let registry = idle_registry();
    let started = Instant::now();
    let (mut window, _state) = window("Hello, world 123", SHIPPED_THEN_GENERIC, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let took = started.elapsed();
    assert!(
        took < NOT_THE_FIVE_SECONDS,
        "the first layout waited {took:?}: the generic family after the shipped face brought the scan's wait back"
    );
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");
    assert!(!registry.is_build_complete(), "premise: the scan has not run");
}

#[test]
fn text_that_arrives_later_and_no_registered_face_draws_takes_the_wait_as_before() {
    let registry = idle_registry();
    let (mut window, state) = window("Hello, world", SHIPPED_STACK, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");

    // The app shows a character none of its faces draws; the scan starts a moment later (it completes at once), and
    // the wait that is owed returns when it has.
    state
        .borrow_mut()
        .downcast_mut::<Page>()
        .expect("the page state")
        .text = "Hello, \u{6f22}\u{5b57}".to_string();
    let late = start_scan_after(&registry, SCAN_STARTS_AFTER);
    let started = Instant::now();
    window.request_regeneration(RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("the second layout");
    let took = started.elapsed();
    late.join().expect("the late start");
    let (skipped_after, blocked_after) = counters(&window);
    assert!(
        blocked_after >= 1 && skipped_after == skipped,
        "the second DOM holds text the faces in memory cannot draw: it takes the wait, and the skips stay \
         (skipped {skipped} -> {skipped_after}, blocked {blocked_after})"
    );
    assert!(took >= WAITED_AT_LEAST, "the second layout took {took:?}: it did not wait for the scan");
}

#[test]
fn a_dom_set_in_a_system_family_takes_the_wait_as_before() {
    let registry = idle_registry();
    let late = start_scan_after(&registry, SCAN_STARTS_AFTER);
    let started = Instant::now();
    let (mut window, _state) = window("Hello", "\"Some Installed Family\"", &registry, true);
    window.regenerate_layout().expect("the first layout");
    let took = started.elapsed();
    late.join().expect("the late start");
    let (skipped, blocked) = counters(&window);
    assert!(skipped == 0 && blocked >= 1, "skipped {skipped}, blocked {blocked}");
    assert!(took >= WAITED_AT_LEAST, "the first layout took {took:?}: it did not wait for the scan");
}

#[test]
fn an_app_that_ships_no_face_takes_the_wait_as_before() {
    let registry = idle_registry();
    let late = start_scan_after(&registry, SCAN_STARTS_AFTER);
    let (mut window, _state) = window("Hello", SHIPPED_STACK, &registry, false);
    window.regenerate_layout().expect("the first layout");
    late.join().expect("the late start");
    let (skipped, blocked) = counters(&window);
    assert!(skipped == 0 && blocked >= 1, "nothing shipped draws the text: skipped {skipped}, blocked {blocked}");
}

#[test]
fn a_registry_that_has_finished_its_scan_has_nothing_to_wait_for_and_nothing_is_decided() {
    let registry = idle_registry();
    registry.spawn_scout_and_builders();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !registry.is_build_complete() {
        assert!(Instant::now() < deadline, "an empty scan did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
    let (mut window, _state) = window("Hello", "\"Some Installed Family\"", &registry, true);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped == 0 && blocked == 0, "skipped {skipped}, blocked {blocked}");
}
