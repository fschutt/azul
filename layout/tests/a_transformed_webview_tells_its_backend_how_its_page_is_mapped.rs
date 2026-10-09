//! A `<webview>` under a CSS transform tells its backend how its page maps
//! into its placement.
//!
//! A placement is a box on screen: the bounding box of the web view's content
//! box under every transform above it, and the clip. That is all a native
//! view without transforms needs, and it was all a backend heard - so a page
//! under `scale(2)` was laid out at twice its size (a 100px-wide web view
//! became a 200px-wide page) and a turned one showed unturned. The backend
//! now also hears the page's own size and the linear part of the map
//! (`WebViewOp::Transform`): a native view zooms (or turns, where it can), a
//! composited one draws through the same matrix. An untransformed web view
//! hears nothing more than before.
//!
//! Every page is 400x300; the web view is 100x50 inside a 100x50 box that
//! carries the transform (about its centre, the CSS default).
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::Dom,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::webview::{WebViewId, WebViewOp, WebViewPlatform, WebViewTransform},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.webviews.set_platform(WebViewPlatform::Backend);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws;
    lw
}

/// `body(0) > box(1, transform) > webview(2)`.
fn lay_out(lw: &mut LayoutWindow, transform: &str) {
    let dom = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css(&format!("width: 100px; height: 50px; {transform}"))
            .with_child(
                Dom::create_webview(AzString::from("https://example.com/"))
                    .with_css("display: block; width: 100px; height: 50px;"),
            ),
    );
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.sync_webview_placements();
}

fn the_view(lw: &LayoutWindow) -> WebViewId {
    lw.webviews.views()[0].id
}

fn transform_of(ops: &[WebViewOp], of: WebViewId) -> Option<WebViewTransform> {
    ops.iter().rev().find_map(|op| match op {
        WebViewOp::Transform { id, transform } if *id == of => Some(*transform),
        _ => None,
    })
}

fn rect_size_of(ops: &[WebViewOp], of: WebViewId) -> Option<LogicalSize> {
    ops.iter().rev().find_map(|op| match op {
        WebViewOp::Place { id, placement } if *id == of => Some(placement.rect.size),
        _ => None,
    })
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn an_untransformed_webview_tells_its_backend_nothing_beyond_its_placement() {
    let mut lw = window();
    lay_out(&mut lw, "");
    let ops = lw.webviews.take_ops();
    let id = the_view(&lw);
    assert_eq!(transform_of(&ops, id), None, "{ops:?}");
}

#[test]
fn a_scaled_webview_is_placed_at_its_scaled_box_and_its_page_keeps_its_own_size() {
    let mut lw = window();
    lay_out(&mut lw, "transform: scale(2);");
    let ops = lw.webviews.take_ops();
    let id = the_view(&lw);
    let size = rect_size_of(&ops, id).expect("placed");
    assert!(
        near(size.width, 200.0) && near(size.height, 100.0),
        "the placement is the scaled box: {size:?}"
    );
    let t = transform_of(&ops, id).expect("the backend hears the scale");
    assert_eq!(t.size, LogicalSize::new(100.0, 50.0), "the page's own size: {t:?}");
    assert!(
        near(t.sx, 2.0) && near(t.sy, 2.0) && near(t.shx, 0.0) && near(t.shy, 0.0),
        "a scale of two, no turn: {t:?}"
    );
    assert!(t.is_axis_aligned());
}

#[test]
fn a_turned_webview_tells_its_backend_its_turn() {
    let mut lw = window();
    lay_out(&mut lw, "transform: rotate(90deg);");
    let ops = lw.webviews.take_ops();
    let id = the_view(&lw);
    let size = rect_size_of(&ops, id).expect("placed");
    assert!(
        near(size.width, 50.0) && near(size.height, 100.0),
        "the placement is the turned box's bounds: {size:?}"
    );
    let t = transform_of(&ops, id).expect("the backend hears the turn");
    assert!(
        near(t.sx, 0.0) && near(t.sy, 0.0) && near(t.shx.abs(), 1.0) && near(t.shy.abs(), 1.0),
        "a quarter turn: {t:?}"
    );
    assert!(!t.is_axis_aligned());
}

#[test]
fn a_transform_that_goes_away_tells_the_backend_its_page_is_untransformed_again() {
    let mut lw = window();
    lay_out(&mut lw, "transform: scale(2);");
    let id = the_view(&lw);
    let _ = lw.webviews.take_ops();

    lay_out(&mut lw, "");
    let ops = lw.webviews.take_ops();
    assert_eq!(the_view(&lw), id, "the same view");
    assert_eq!(
        transform_of(&ops, id),
        Some(WebViewTransform::untransformed(LogicalSize::new(100.0, 50.0))),
        "{ops:?}"
    );

    lw.sync_webview_placements();
    assert_eq!(lw.webviews.take_ops(), Vec::new(), "said once");
}

#[test]
fn a_page_point_maps_through_the_transform_onto_the_screen_and_back() {
    let mut lw = window();
    lay_out(&mut lw, "transform: scale(2);");
    let ops = lw.webviews.take_ops();
    let id = the_view(&lw);
    let t = transform_of(&ops, id).expect("scaled");
    let placement = lw.webviews.get(id).expect("mounted").placement;
    // Scaled about the box's centre (50, 25): the page's corner lands at
    // (-50, -25), its point (10, 10) at (-30, -5).
    let on_screen = t.to_window(placement.rect, azul_core::geom::LogicalPosition::new(10.0, 10.0));
    assert!(
        near(on_screen.x, -30.0) && near(on_screen.y, -5.0),
        "{on_screen:?} (placed at {:?})",
        placement.rect
    );
    let back = t.to_page(placement.rect, on_screen).expect("invertible");
    assert!(near(back.x, 10.0) && near(back.y, 10.0), "{back:?}");
}
