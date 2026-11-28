//! The layout of a window waits for the font registry's scan only where it needs a face from the system.
//!
//! The registry here is one whose scan the test starts itself, or NEVER: the wait the layout used to make before it built
//! its DOM (`wait_for_fonts`, five seconds at most) cannot end while the scan has not started, so a layout of a DOM that
//! is drawn by the faces the app ships must not make it, and one that needs the system must (the scan is started a
//! while later, over no directory: it completes at once). The counters of the frame report say which happened.

use std::{
    cell::RefCell,
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::Dom,
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::{AppConfig, NamedFont},
};
use azul_layout::{text3::fontreg::RegisteredFonts, window_state::WindowCreateOptions};
use rust_fontconfig::{config::FcScanConfig, registry::FcFontRegistry, FcFontCache};

use azul_layout::text3::cache::SystemFontsGate;

use super::RegistryGate;
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

/// What the layout callback shows, and in which `font-family` value: a block of `text`, and a second block of `more`
/// when there is any.
struct Page {
    text: String,
    more: String,
    stack: String,
}

extern "C" fn page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (text, more, stack) = data
        .downcast_ref::<Page>()
        .map_or((String::new(), String::new(), String::new()), |p| {
            (p.text.clone(), p.more.clone(), p.stack.clone())
        });
    let block = |text: &str| {
        Dom::create_div()
            .with_css(&format!("font-family: {stack}; font-size: 16px;"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
    };
    let mut body = Dom::create_body().with_child(block(text.as_str()));
    if !more.is_empty() {
        body = body.with_child(block(more.as_str()));
    }
    body
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

/// How long the scan that a waiting layout depends on is held back, and the least the layout must have waited for it. The
/// layout asks for the wait where it resolves its chains, after the app's callback and the style of the DOM: in a debug
/// build on a runner that is a good part of a second in, and the scan must not have started by then.
const SCAN_STARTS_AFTER: Duration = Duration::from_millis(3000);
const WAITED_AT_LEAST: Duration = Duration::from_millis(1500);

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
        more: String::new(),
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

/// The characters of the window's DOM as the layout holds them now, and whether its font manager has a gate: what a failing
/// assertion says about the pass it is about.
fn state_of(window: &HeadlessWindow, registry: &FcFontRegistry) -> String {
    let layout_window = window.get_layout_window().expect("a layout window");
    let resolutions = layout_window.work.font_resolutions();
    let left_over = layout_window
        .font_manager
        .system_fonts_gate
        .as_deref()
        .map(|gate| gate.take_waits().len());
    let chars: String = layout_window
        .layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
        .map(|result| azul_layout::window::CharSet::of_dom(&result.styled_dom).to_set())
        .unwrap_or_default()
        .into_iter()
        .collect();
    format!(
        "the DOM holds {chars:?}, a gate is {}, the scan is {}, {} layout results, {resolutions} font resolutions so far, \
         {left_over:?} waits left in the gate",
        if layout_window.font_manager.system_fonts_gate.is_some() { "installed" } else { "missing" },
        if registry.is_build_complete() { "complete" } else { "not complete" },
        layout_window.layout_results.len(),
    )
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
    // Nearly every stylesheet ends its stack in a generic family: the faces in memory draw the text, and the names after
    // them are not what the first frame is drawn with.
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

/// The second layout of a window whose first was drawn by the app's face: `change` makes the app show text that no face of
/// its draws, the scan starts a while later (it completes at once), and the wait that is owed returns when it has.
fn the_second_layout_takes_the_wait(change: impl FnOnce(&mut Page)) {
    let registry = idle_registry();
    let (mut window, state) = window("Hello, world", SHIPPED_STACK, &registry, true);
    window.regenerate_layout().expect("the first layout");
    let (skipped, blocked) = counters(&window);
    assert!(skipped >= 1 && blocked == 0, "skipped {skipped}, blocked {blocked}");

    let resolutions_before = window.get_layout_window().expect("a layout window").work.font_resolutions();
    change(&mut state.borrow_mut().downcast_mut::<Page>().expect("the page state"));
    let late = start_scan_after(&registry, SCAN_STARTS_AFTER);
    let started = Instant::now();
    window.regenerate_layout().expect("the second layout");
    let took = started.elapsed();
    late.join().expect("the late start");
    let (skipped_after, blocked_after) = counters(&window);
    assert!(
        blocked_after >= 1 && skipped_after == skipped,
        "the second DOM holds text the faces in memory cannot draw: it takes the wait, and the skips stay \
         (skipped {skipped} -> {skipped_after}, blocked {blocked_after}; {} font resolutions before the second layout; {})",
        resolutions_before,
        state_of(&window, &registry)
    );
    assert!(took >= WAITED_AT_LEAST, "the second layout took {took:?}: it did not wait for the scan");
}

#[test]
fn text_that_arrives_later_and_no_registered_face_draws_takes_the_wait_as_before() {
    the_second_layout_takes_the_wait(|page| page.text = "Hello, \u{6f22}\u{5b57}".to_string());
}

#[test]
fn a_second_block_of_such_text_takes_the_wait_as_before() {
    the_second_layout_takes_the_wait(|page| page.more = "\u{6f22}\u{5b57}".to_string());
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

#[test]
fn the_gate_holds_the_layout_until_the_scan_has_listed_the_fonts_and_writes_the_wait_down() {
    let registry = idle_registry();
    let gate = RegistryGate::new(Arc::clone(&registry), Vec::new());
    assert!(gate.pending(), "no manifest and a scan that has not run: a wait could be owed");
    let late = start_scan_after(&registry, Duration::from_millis(250));
    let started = Instant::now();
    gate.wait_for(&[vec!["Some Installed Family".to_string()]], "the reason");
    let took = started.elapsed();
    late.join().expect("the late start");
    assert!(took >= Duration::from_millis(200), "the gate returned after {took:?}");
    let deadline = Instant::now() + Duration::from_secs(30);
    while gate.pending() {
        assert!(Instant::now() < deadline, "the scan did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
    let waits = gate.take_waits();
    assert_eq!(waits.len(), 1, "one wait: {waits:?}");
    assert_eq!(waits[0].why, "the reason");
    assert!(waits[0].millis >= 200.0, "the wait took {} ms", waits[0].millis);
    assert!(gate.take_waits().is_empty(), "what was taken is gone");
    // The scan is over: a wait is no more owed and writes nothing down.
    gate.wait_for(&[vec!["Some Installed Family".to_string()]], "again");
    assert!(gate.take_waits().is_empty());
}
