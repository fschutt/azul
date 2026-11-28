//! `WindowFlags::display_preset`, the test hook of the cutout / system bar API: a window made to sit on
//! a Pixel 8 or an iPhone takes the insets and the features of that device from the preset, for its
//! own size, in the layout the shell's insets would feed: the root is inset by them (unless the window
//! draws into the safe area), a rotation moves them, and what only the real device knows (the keyboard)
//! stays real.
//!
//! Beside `safe_area_inset.rs` (the insets of a shell), not in it.

use azul_core::{
    display_features::{DisplayFeatures, DisplayPreset, SystemBarMode},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn state(preset: DisplayPreset, width: f32, height: f32, extend: bool) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window_state.flags.display_preset = preset;
    window_state.flags.extend_into_safe_area = extend;
    window_state
}

fn layout_with(reported: SafeAreaInsets, window_state: &FullWindowState) -> LayoutWindow {
    let (css, _) =
        azul_css::parser2::new_from_str("body { margin: 0; width: 100%; height: 100%; }");
    let mut dom = Dom::create_body().with_child(Dom::create_div());
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_window.safe_area_insets = reported;
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// The laid-out rect of the ROOT node, in window coordinates.
fn root_rect(w: &LayoutWindow) -> LogicalRect {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(0))
        .and_then(|v| v.first())
        .expect("root has a layout node");
    let hot = lr.layout_tree.get(idx).expect("layout node");
    LogicalRect {
        origin: lr.calculated_positions[idx.index()],
        size: hot.used_size.expect("root sized"),
    }
}

fn px(v: f32) -> OptionPixelValue {
    OptionPixelValue::Some(PixelValue::px(v))
}

#[test]
fn a_window_on_a_pixel_8_is_inset_by_its_status_bar_and_its_gesture_bar() {
    let s = state(DisplayPreset::Pixel8, 411.0, 914.0, false);
    let w = layout_with(SafeAreaInsets::default(), &s);
    let r = root_rect(&w);
    assert_eq!(r.origin.y, 48.0, "below the status bar the camera sits in");
    assert_eq!(
        r.size.height,
        914.0 - 48.0 - 24.0,
        "and above the gesture bar"
    );
    assert_eq!(r.origin.x, 0.0);
    assert_eq!(r.size.width, 411.0);
}

#[test]
fn a_window_that_draws_into_the_safe_area_fills_the_surface_on_a_pixel_8_too() {
    let s = state(DisplayPreset::Pixel8, 411.0, 914.0, true);
    let w = layout_with(SafeAreaInsets::default(), &s);
    let r = root_rect(&w);
    assert_eq!((r.origin.x, r.origin.y), (0.0, 0.0));
    assert_eq!((r.size.width, r.size.height), (411.0, 914.0));
}

#[test]
fn the_phone_on_its_side_moves_the_insets_to_the_sides() {
    let s = state(DisplayPreset::Pixel8, 914.0, 411.0, false);
    let w = layout_with(SafeAreaInsets::default(), &s);
    let r = root_rect(&w);
    assert_eq!(r.origin.x, 36.0, "the camera is at the left edge");
    assert_eq!(r.origin.y, 24.0);
    assert_eq!(r.size.width, 914.0 - 36.0);
}

#[test]
fn the_simulation_replaces_what_the_shell_reports_and_no_simulation_leaves_it() {
    let reported = SafeAreaInsets {
        top: px(10.0),
        ..SafeAreaInsets::default()
    };
    let simulated = state(DisplayPreset::IPhoneDynamicIsland, 393.0, 852.0, false);
    let w = layout_with(reported, &simulated);
    assert_eq!(
        root_rect(&w).origin.y,
        59.0,
        "the island's inset, not the 10 the shell said"
    );

    let plain = state(DisplayPreset::Off, 393.0, 852.0, false);
    let w = layout_with(reported, &plain);
    assert_eq!(root_rect(&w).origin.y, 10.0, "no preset: the shell's own");
}

#[test]
fn the_keyboard_inset_is_always_the_real_one() {
    let reported = SafeAreaInsets {
        keyboard: px(300.0),
        ..SafeAreaInsets::default()
    };
    let w = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut lw = w;
    lw.safe_area_insets = reported;
    let s = state(DisplayPreset::Pixel8, 411.0, 914.0, false);
    let insets = lw.effective_safe_area_insets(&s);
    assert_eq!(insets.keyboard, px(300.0));
    assert_eq!(insets.top, px(48.0));
}

#[test]
fn the_features_are_the_presets_or_the_shells_and_a_report_that_changed_says_so() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    // A shell reports a hole; the same report twice is no change.
    let reported =
        DisplayFeatures::simulate(DisplayPreset::CornerHole, LogicalSize::new(360.0, 720.0));
    assert!(lw.report_display_features(reported.clone()));
    assert!(!lw.report_display_features(reported.clone()));

    let plain = state(DisplayPreset::Off, 360.0, 720.0, false);
    assert_eq!(
        lw.effective_display_features(&plain),
        reported,
        "no preset: what the shell said"
    );

    let pixel = state(DisplayPreset::Pixel8, 411.0, 914.0, false);
    let features = lw.effective_display_features(&pixel);
    assert_eq!(features.cutouts.as_ref().len(), 1);
    let hole = features.cutouts.as_ref()[0].rect;
    assert!(
        (hole.origin.x + hole.size.width / 2.0 - 205.5).abs() < 0.01,
        "the middle of the top edge"
    );
}

#[test]
fn a_resize_moves_the_simulated_hole_with_the_window() {
    let lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    for width in [320.0_f32, 411.0, 700.0] {
        let s = state(DisplayPreset::Pixel8, width, 900.0, false);
        let hole = lw.effective_display_features(&s).cutouts.as_ref()[0].rect;
        assert!((hole.origin.x + hole.size.width / 2.0 - width / 2.0).abs() < 0.01);
    }
}

#[test]
fn the_bars_an_app_hides_are_a_flag_the_window_carries() {
    let mut s = state(DisplayPreset::Off, 411.0, 914.0, false);
    assert_eq!(s.flags.system_bars.status, SystemBarMode::Visible);
    s.flags.system_bars.status = SystemBarMode::HiddenTransient;
    s.flags.system_bars.draw_into_cutout = true;
    // A copy of the flags (what the shells diff) is the same flags.
    let copy = s.flags;
    assert_eq!(copy.system_bars, s.flags.system_bars);
    assert_ne!(copy, state(DisplayPreset::Off, 411.0, 914.0, false).flags);
}
