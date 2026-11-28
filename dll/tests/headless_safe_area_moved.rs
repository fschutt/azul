//! The bars of a phone arrive after its first frame, and a static UI must still end up inside them.
//!
//! Android hands the window its insets (`View.onApplyWindowInsets`) once the window has focus,
//! which is AFTER the first frame; a rotation moves them again. The shell writes them into the
//! `LayoutWindow` and asks for a regeneration - and `regenerate_layout` kept the retained layout
//! whenever the app's DOM fingerprint was the one it had: the fingerprint says nothing of the
//! insets, and the exits asked about the window's SIZE only. A UI that does not change by itself
//! (a map, a canvas, a ribbon: AzMaps, AzPaint and AzWriter on the 2026-11-26 emulator run) stayed
//! at y = 0, under the status bar and the clock, for good; the apps whose DOM happened to change
//! between the two passes (AzMeet's device list) were placed correctly, which is why it looked
//! like an app problem.
//!
//! The mirror of `headless_decoration_flip.rs` (a flip of the window's flags that the app's DOM
//! cannot show): one more input of the layout that no fingerprint of the app's DOM covers.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{
    common::{event::PlatformWindow, layout::LayoutRegenerateResult},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

struct AppState;

/// A UI that does not change between passes: a title row and a body that fills the rest, like the
/// columns of AzPaint and AzWriter. Exactly the case the pre-cascade skip is built for.
extern "C" fn a_static_ui(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
        .with_css("margin: 0px; display: flex; flex-direction: column; height: 100%;")
        .with_child(Dom::create_div().with_css("height: 56px;"))
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
}

fn phone(width: f32, height: f32, extend_into_safe_area: bool) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(AppState)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width, height };
    options.window_state.flags.extend_into_safe_area = extend_into_safe_area;
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = a_static_ui;
    options.window_state.layout_callback = LayoutCallback::create(cb);

    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

fn insets(top: f32, bottom: f32, left: f32, right: f32) -> SafeAreaInsets {
    let px = |v: f32| {
        if v > 0.0 {
            OptionPixelValue::Some(PixelValue::px(v))
        } else {
            OptionPixelValue::None
        }
    };
    SafeAreaInsets {
        top: px(top),
        bottom: px(bottom),
        left: px(left),
        right: px(right),
        ..SafeAreaInsets::default()
    }
}

/// What the shell does when the system reports the bars: writes them into the window.
fn the_system_reports(window: &mut HeadlessWindow, insets: SafeAreaInsets) {
    window
        .common
        .layout_window
        .as_mut()
        .expect("the window has a LayoutWindow after regenerate_layout")
        .safe_area_insets = insets;
}

/// The laid-out rect of the ROOT node, in window coordinates.
fn root_rect(window: &HeadlessWindow) -> LogicalRect {
    let lw = window.common.layout_window.as_ref().expect("a layout window");
    let lr = lw.layout_results.get(&DomId::ROOT_ID).expect("the root was laid out");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(0))
        .and_then(|v| v.first())
        .expect("the root has a layout node");
    let hot = lr.layout_tree.get(idx).expect("layout node");
    LogicalRect {
        origin: lr.calculated_positions[idx.index()],
        size: hot.used_size.expect("the root has a size"),
    }
}

fn warm_up(window: &mut HeadlessWindow) {
    // The way `headless_decoration_flip.rs` does: the caches the skip needs (fingerprints, the
    // retained StyledDom) are populated, and the pass that follows is a steady-state one.
    for _ in 0..3 {
        window.regenerate_layout().expect("warm-up regenerate_layout");
    }
}

/// An iPhone 15 Pro in portrait: 393 x 852 pt, 59 pt on top (the Dynamic Island), 34 pt at the bottom.
const IPHONE: (f32, f32, f32, f32) = (393.0, 852.0, 59.0, 34.0);
/// A 360 x 800 dp Android phone: a 24 dp status bar, a 48 dp navigation bar.
const ANDROID: (f32, f32, f32, f32) = (360.0, 800.0, 24.0, 48.0);

#[test]
fn the_bars_that_arrive_after_the_first_frame_move_the_root_of_a_ui_that_never_changes() {
    for (width, height, top, bottom) in [IPHONE, ANDROID] {
        let mut window = phone(width, height, false);
        warm_up(&mut window);
        let before = root_rect(&window);
        assert_eq!(before.origin.y, 0.0, "no bars known yet: the root fills the window");
        assert_eq!(before.size.height, height);

        the_system_reports(&mut window, insets(top, bottom, 0.0, 0.0));
        let result = window
            .regenerate_layout_inner()
            .expect("the regeneration the insets ask for");

        assert_ne!(
            result,
            LayoutRegenerateResult::LayoutUnchanged,
            "the app's DOM is the DOM it was, but the window's insets are not what the root was \
             placed for: keeping the retained layout leaves the UI under the status bar"
        );
        let after = root_rect(&window);
        assert_eq!(after.origin.y, top, "{width}x{height}: below the status bar / the island");
        assert_eq!(
            after.size.height,
            height - top - bottom,
            "{width}x{height}: above the navigation bar / the home indicator"
        );
        assert_eq!(after.origin.x, 0.0);
        assert_eq!(after.size.width, width);
    }
}

#[test]
fn insets_that_moved_with_a_rotation_take_the_root_with_them() {
    let (width, height, top, bottom) = IPHONE;
    let mut window = phone(width, height, false);
    the_system_reports(&mut window, insets(top, bottom, 0.0, 0.0));
    warm_up(&mut window);
    assert_eq!(root_rect(&window).origin.y, top);

    // Landscape: the window turns first (the layout at the new size, still under the bars of the old
    // orientation), and the system tells the new bars afterwards: the island is at the side now, the
    // bars on top and bottom go. A pass that sees the new size and the new insets together would
    // place the root right whatever the guard says - the order is what a phone does.
    window.simulate_resize(height, width);
    window.regenerate_layout().expect("the rotation's first pass, at the new size");
    assert_eq!(root_rect(&window).origin.y, top, "the new bars have not been told yet");
    the_system_reports(&mut window, insets(0.0, 21.0, 59.0, 59.0));
    let result = window.regenerate_layout_inner().expect("the regeneration the new bars ask for");

    assert_ne!(result, LayoutRegenerateResult::LayoutUnchanged);
    let after = root_rect(&window);
    assert_eq!((after.origin.x, after.origin.y), (59.0, 0.0));
    assert_eq!((after.size.width, after.size.height), (height - 118.0, width - 21.0));
}

#[test]
fn insets_that_did_not_move_still_take_the_skip() {
    // The guard costs nothing in the steady state: the same DOM under the same insets is still
    // the unchanged exit, the rest of a phone's frames.
    let (width, height, top, bottom) = ANDROID;
    let mut window = phone(width, height, false);
    the_system_reports(&mut window, insets(top, bottom, 0.0, 0.0));
    warm_up(&mut window);

    let result = window.regenerate_layout_inner().expect("steady-state regenerate_layout");
    assert_eq!(
        result,
        LayoutRegenerateResult::LayoutUnchanged,
        "an identical DOM under unchanged insets must still skip the rebuild"
    );
}

#[test]
fn a_window_that_draws_under_the_bars_ignores_their_arrival() {
    // `extend_into_safe_area` (a full-bleed map, a video): the root is the whole surface whatever
    // the system reports, so the insets arriving move nothing and cost no layout.
    let (width, height, top, bottom) = ANDROID;
    let mut window = phone(width, height, true);
    warm_up(&mut window);

    the_system_reports(&mut window, insets(top, bottom, 0.0, 0.0));
    let result = window.regenerate_layout_inner().expect("regenerate_layout");

    assert_eq!(result, LayoutRegenerateResult::LayoutUnchanged);
    let root = root_rect(&window);
    assert_eq!((root.origin.y, root.size.height), (0.0, height));
}
