//! 10c-v: the ROOT layout is inset by the platform's safe area by default
//! (the browser's `viewport-fit=auto`), and `extend_into_safe_area` opts a
//! window out of it (`viewport-fit=cover`).

use azul_core::{
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

fn layout_with(insets: SafeAreaInsets, extend: bool) -> LayoutWindow {
    let (css, _) =
        azul_css::parser2::new_from_str("body { margin: 0; width: 100%; height: 100%; }");
    let mut dom = Dom::create_body().with_child(Dom::create_div());
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_window.safe_area_insets = insets;
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1280.0, 800.0);
    window_state.flags.extend_into_safe_area = extend;
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// The laid-out rect of the ROOT node, in window coordinates. The body's
/// UA margin is zeroed by the test stylesheet so the rect IS the viewport.
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

fn phone_insets() -> SafeAreaInsets {
    SafeAreaInsets {
        top: OptionPixelValue::Some(PixelValue::px(44.0)),
        bottom: OptionPixelValue::Some(PixelValue::px(34.0)),
        ..SafeAreaInsets::default()
    }
}

#[test]
fn the_root_is_inset_by_the_safe_area_by_default() {
    let w = layout_with(phone_insets(), false);
    let r = root_rect(&w);
    assert_eq!(r.origin.y, 44.0, "pushed below the status bar / notch");
    assert_eq!(r.origin.x, 0.0);
    assert_eq!(
        r.size.height,
        800.0 - 44.0 - 34.0,
        "and shortened above the home indicator"
    );
    assert_eq!(r.size.width, 1280.0);
}

#[test]
fn extend_into_safe_area_fills_the_whole_surface() {
    let w = layout_with(phone_insets(), true);
    let r = root_rect(&w);
    assert_eq!(r.origin.y, 0.0);
    assert_eq!(r.size.height, 800.0);
}

#[test]
fn no_insets_is_the_desktop_no_op() {
    let w = layout_with(SafeAreaInsets::default(), false);
    let r = root_rect(&w);
    assert_eq!(r.origin.y, 0.0);
    assert_eq!(r.size.height, 800.0);
}

/// The pure arithmetic: a window smaller than its insets collapses to zero
/// rather than going negative.
#[test]
fn inset_arithmetic_clamps_at_zero() {
    let full = LogicalRect {
        origin: azul_core::geom::LogicalPosition::zero(),
        size: LogicalSize::new(50.0, 60.0),
    };
    let r = LayoutWindow::inset_by_safe_area(&full, &phone_insets());
    assert_eq!(r.origin.y, 44.0);
    assert_eq!(r.size.height, 0.0, "60 - 44 - 34 < 0 clamps to zero");
    assert_eq!(r.size.width, 50.0);
}

/// The page background still covers the WHOLE surface.
///
/// CSS 2.2 §14.2: the root's background is the CANVAS's, and the canvas
/// lies under everything the window shows - the safe area insets where the
/// root is LAID OUT, not what is painted around it (a browser with
/// `viewport-fit=auto` still paints the page colour under the notch and the
/// home indicator). The canvas rect was the inset viewport's SIZE at the
/// window's ORIGIN: shifted up by the top inset and short by both. Seen on
/// macOS, where a `NoTitle` window reports its 28pt titlebar as an inset: the
/// bottom 28pt showed the bare window through the body's margins.
#[test]
fn the_canvas_background_covers_the_whole_surface() {
    use azul_layout::solver3::display_list::DisplayListItem;

    let (css, _) = azul_css::parser2::new_from_str(
        "body { margin: 8px; height: 100px; background-color: #ff0000; }",
    );
    let mut dom = Dom::create_body();
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_window.safe_area_insets = phone_insets();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1280.0, 800.0);
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut debug_messages,
        )
        .unwrap();

    let lr = layout_window
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root layout");
    let canvas = lr
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if color.r == 255 && color.g == 0 && color.b == 0 =>
            {
                Some(bounds.0)
            }
            _ => None,
        })
        .expect("the body's background is painted");
    assert_eq!(
        (
            canvas.origin.x,
            canvas.origin.y,
            canvas.size.width,
            canvas.size.height
        ),
        (0.0, 0.0, 1280.0, 800.0),
        "the canvas background covers the whole 1280x800 surface, insets included"
    );
}

// ---------------------------------------------------------------------------------------------
// Phone sizes (the 2026-11-26 emulator pictures: AzMaps, AzPaint and AzWriter drew under the
// status and navigation bars): no node of a phone-shaped screen overlaps an inset area unless the
// window asked to draw under the bars.
// ---------------------------------------------------------------------------------------------

/// A phone-shaped screen: a top bar of icon buttons, a page, a bottom row of tabs.
fn a_phone_screen() -> Dom {
    Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_class("bar".into())
                .with_child(Dom::create_div().with_class("btn".into()))
                .with_child(Dom::create_div().with_class("title".into()))
                .with_child(Dom::create_div().with_class("btn".into())),
        )
        .with_child(
            Dom::create_div()
                .with_class("page".into())
                .with_child(Dom::create_div().with_class("card".into())),
        )
        .with_child(
            Dom::create_div()
                .with_class("tabs".into())
                .with_child(Dom::create_div().with_class("tab".into()))
                .with_child(Dom::create_div().with_class("tab".into()))
                .with_child(Dom::create_div().with_class("tab".into())),
        )
}

const PHONE_CSS: &str = "body { margin: 0; width: 100%; height: 100%; display: flex; \
    flex-direction: column; } \
    .bar { height: 56px; display: flex; flex-direction: row; align-items: center; } \
    .btn { width: 48px; height: 48px; } .title { flex-grow: 1; height: 24px; } \
    .page { flex-grow: 1; } .card { height: 100px; } \
    .tabs { height: 64px; display: flex; flex-direction: row; } .tab { flex-grow: 1; height: 64px; }";

fn phone_state(width: f32, height: f32, extend: bool) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window_state.flags.extend_into_safe_area = extend;
    window_state
}

fn lay_out_phone(
    width: f32,
    height: f32,
    insets: SafeAreaInsets,
    extend: bool,
) -> (LayoutWindow, FullWindowState) {
    let (css, _) = azul_css::parser2::new_from_str(PHONE_CSS);
    let mut dom = a_phone_screen();
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_window.safe_area_insets = insets;
    let window_state = phone_state(width, height, extend);
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut debug_messages,
        )
        .unwrap();
    (layout_window, window_state)
}

/// Every laid-out node of the root document, in window coordinates.
fn every_rect(w: &LayoutWindow) -> Vec<(usize, LogicalRect)> {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let mut out = Vec::new();
    for (dom_node, indices) in lr.layout_tree.dom_to_layout.iter() {
        for idx in indices {
            let hot = lr.layout_tree.get(*idx).expect("layout node");
            if let Some(size) = hot.used_size {
                out.push((
                    dom_node.index(),
                    LogicalRect { origin: lr.calculated_positions[idx.index()], size },
                ));
            }
        }
    }
    out
}

fn side_insets(top: f32, bottom: f32, left: f32, right: f32) -> SafeAreaInsets {
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

#[test]
fn no_node_of_a_phone_screen_overlaps_a_bar_unless_the_window_drew_under_them() {
    // (width, height, top, bottom, left, right), logical px
    let phones = [
        (393.0_f32, 852.0_f32, 59.0_f32, 34.0_f32, 0.0_f32, 0.0_f32), // iPhone 15 Pro, portrait
        (852.0, 393.0, 0.0, 21.0, 59.0, 59.0),                         // the same, landscape
        (360.0, 800.0, 24.0, 48.0, 0.0, 0.0),                          // Android, 3-button navigation
        (411.0, 914.0, 48.8, 48.0, 0.0, 0.0),                          // the emulator of the runs
    ];
    for (width, height, top, bottom, left, right) in phones {
        let (w, _) = lay_out_phone(width, height, side_insets(top, bottom, left, right), false);
        let rects = every_rect(&w);
        assert!(rects.len() >= 11, "the screen has its nodes: {}", rects.len());
        for (node, r) in rects {
            let eps = 0.01;
            assert!(
                r.origin.y >= top - eps
                    && r.origin.y + r.size.height <= height - bottom + eps
                    && r.origin.x >= left - eps
                    && r.origin.x + r.size.width <= width - right + eps,
                "{width}x{height} (insets {top}/{bottom}/{left}/{right}): node {node} at \
                 ({}, {}) {} x {} lies in a bar",
                r.origin.x,
                r.origin.y,
                r.size.width,
                r.size.height,
            );
        }
    }
}

#[test]
fn a_window_that_draws_under_the_bars_has_a_root_that_is_the_whole_screen() {
    for (width, height, top, bottom) in [(393.0, 852.0, 59.0, 34.0), (360.0, 800.0, 24.0, 48.0)] {
        let (w, _) = lay_out_phone(width, height, side_insets(top, bottom, 0.0, 0.0), true);
        let (_, root) = every_rect(&w)
            .into_iter()
            .min_by_key(|(node, _)| *node)
            .expect("a root");
        assert_eq!(
            (root.origin.x, root.origin.y, root.size.width, root.size.height),
            (0.0, 0.0, width, height),
            "full-bleed: the app places its own content with get_safe_area_insets()"
        );
    }
}

#[test]
fn the_window_knows_when_its_insets_are_not_the_ones_its_root_was_placed_for() {
    let (mut w, state) = lay_out_phone(393.0, 852.0, SafeAreaInsets::default(), false);
    assert!(!w.safe_area_moved_since_layout(&state), "laid out under the insets it has");

    w.safe_area_insets = side_insets(59.0, 34.0, 0.0, 0.0);
    assert!(
        w.safe_area_moved_since_layout(&state),
        "the bars arrived after the first frame: the retained layout stands on other insets"
    );
    // a window that draws under the bars stands on none, whatever arrives
    assert!(!w.safe_area_moved_since_layout(&phone_state(393.0, 852.0, true)));
    // a keyboard is not part of the root's insets
    w.safe_area_insets = SafeAreaInsets {
        keyboard: OptionPixelValue::Some(PixelValue::px(291.0)),
        ..SafeAreaInsets::default()
    };
    assert!(
        !w.safe_area_moved_since_layout(&state),
        "the root stood on no insets and the keyboard is none of them: nothing moved"
    );
}
