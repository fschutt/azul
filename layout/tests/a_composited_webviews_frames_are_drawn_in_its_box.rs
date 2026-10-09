//! A composited web view's frames are drawn in its box by the display list.
//!
//! A native web view (macOS, Windows) is a view of its own over the window.
//! A composited one (WPE `WebKit` on Linux) renders its page offscreen and
//! hands each frame to the window, which draws it as an image at the web
//! view's content box - so clips, scrolling, stacking and transforms are the
//! window's own (`LayoutWindow::set_webview_frame`). A new frame replaces the
//! last one in place: no layout, no rebuild of the list once one is drawn.
//!
//! Every page is 400x300; the web view's content box is (12, 12) 200x100.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::webview::{WebViewId, WebViewPlatform},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The web view's node: `body(0) > webview(1)`.
const WEBVIEW: NodeId = NodeId::new(1);

fn frame(fill: u8) -> ImageRef {
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![fill; 200 * 100 * 4].into()),
        width: 200,
        height: 100,
        premultiplied_alpha: true,
        data_format: RawImageFormat::BGRA8,
        tag: Vec::new().into(),
    })
    .expect("a well-formed frame")
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.webviews.set_platform(WebViewPlatform::Backend);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, with_webview: bool) {
    let mut body = Dom::create_body().with_css("margin: 0;");
    body = if with_webview {
        body.with_child(
            Dom::create_webview(AzString::from("https://example.com/")).with_css(
                "display: block; width: 200px; height: 100px; padding: 10px; border: 2px solid \
                 black;",
            ),
        )
    } else {
        body.with_child(Dom::create_div())
    };
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

fn the_view(lw: &LayoutWindow) -> WebViewId {
    lw.webviews.views()[0].id
}

/// The images the root list draws for the web view's node: (bounds, image).
fn drawn(lw: &LayoutWindow) -> Vec<(LogicalRect, ImageRef)> {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    dl.items
        .iter()
        .enumerate()
        .filter(|(i, _)| dl.node_mapping.get(*i).copied().flatten() == Some(WEBVIEW))
        .filter_map(|(_, item)| match item {
            DisplayListItem::Image { bounds, image, .. } => Some((*bounds.inner(), image.clone())),
            _ => None,
        })
        .collect()
}

fn content_box() -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(12.0, 12.0), LogicalSize::new(200.0, 100.0))
}

#[test]
fn a_web_view_without_a_frame_draws_nothing_of_its_own() {
    let mut lw = window();
    lay_out(&mut lw, true);
    assert!(drawn(&lw).is_empty());
}

#[test]
fn a_composited_frame_fills_the_web_views_content_box() {
    let mut lw = window();
    lay_out(&mut lw, true);
    let first = frame(10);
    assert!(
        lw.set_webview_frame(the_view(&lw), &first),
        "a frame of a web view on screen is a repaint"
    );
    let images = drawn(&lw);
    assert_eq!(images.len(), 1, "one image for the web view: {images:?}");
    assert_eq!(images[0].0, content_box(), "at its content box");
    assert_eq!(images[0].1.get_hash(), first.get_hash());
}

#[test]
fn the_next_frame_replaces_the_last_in_place_without_a_layout() {
    let mut lw = window();
    lay_out(&mut lw, true);
    let id = the_view(&lw);
    lw.set_webview_frame(id, &frame(10));
    let layouts = lw.frame_report.layout_passes;

    let second = frame(20);
    assert!(lw.set_webview_frame(id, &second));
    let images = drawn(&lw);
    assert_eq!(images.len(), 1, "replaced, not added: {images:?}");
    assert_eq!(images[0].1.get_hash(), second.get_hash());
    assert_eq!(lw.frame_report.layout_passes, layouts, "no layout for a frame");
}

#[test]
fn a_frame_stays_drawn_across_a_relayout() {
    let mut lw = window();
    lay_out(&mut lw, true);
    let first = frame(10);
    lw.set_webview_frame(the_view(&lw), &first);
    lay_out(&mut lw, true);
    let images = drawn(&lw);
    assert_eq!(images.len(), 1, "{images:?}");
    assert_eq!(images[0].1.get_hash(), first.get_hash());
}

#[test]
fn a_frame_for_a_web_view_that_is_gone_draws_nothing() {
    let mut lw = window();
    lay_out(&mut lw, true);
    let id = the_view(&lw);
    lay_out(&mut lw, false);
    assert!(!lw.set_webview_frame(id, &frame(10)));
    assert!(drawn(&lw).is_empty());
}
