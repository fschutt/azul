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
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; width: 100%; height: 100%; }");
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
    assert_eq!(r.size.height, 800.0 - 44.0 - 34.0, "and shortened above the home indicator");
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
