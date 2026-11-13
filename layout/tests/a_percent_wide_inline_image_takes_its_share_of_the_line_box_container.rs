//! A percent-wide image in an inline formatting context takes its share of
//! the CONTENT BOX of the box that holds its line - and keeps doing so when
//! that box changes size.
//!
//! Found by the resized-canvas premise of
//! `a_render_image_callback_with_unchanged_inputs_is_not_invoked_again`:
//! `<body><img style="width: 25%"></body>` in a 400px window was 100px wide
//! (a quarter of the WINDOW, body's own containing block) instead of 96px
//! (a quarter of body's content box, 400 - 2 * 8px margin), and after the
//! window doubled it stayed at its old width until a fresh layout window was
//! used. Block children already resolve against their parent's content box
//! (`layout_bfc`'s `children_containing_block_size`); an atomic inline was
//! measured against the IFC root's own containing block, and the IFC's
//! cached inline-content collection carried the old measurement across a
//! viewport change.

use azul_core::{
    dom::{Dom, DomId, NodeId, NodeType},
    geom::LogicalSize,
    resources::{ImageRef, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn image() -> ImageRef {
    ImageRef::null_image(4, 4, RawImageFormat::BGRA8, Vec::new())
}

fn lay_out(lw: &mut LayoutWindow, dom: Dom, window_width: f32) {
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(window_width, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("lays out");
}

/// The (first) image's laid-out width and its body's, logical px.
fn widths(lw: &LayoutWindow) -> (f32, f32) {
    let sd = &lw.get_layout_result(&DomId::ROOT_ID).expect("laid out").styled_dom;
    let width_of = |pred: &dyn Fn(&NodeType) -> bool| {
        let index = sd
            .node_data
            .as_ref()
            .iter()
            .position(|nd| pred(nd.get_node_type()))
            .expect("the node exists");
        lw.get_node_bounds(DomId::ROOT_ID, NodeId::new(index))
            .expect("the node has a rect")
            .size
            .width as f32
    };
    (
        width_of(&|t| matches!(t, NodeType::Image(_))),
        width_of(&|t| matches!(t, NodeType::Body)),
    )
}

#[test]
fn a_percent_wide_image_directly_in_body_takes_a_quarter_of_its_content_box() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let dom = Dom::create_body().with_child(Dom::create_image(image()).with_css("width: 25%; height: 50px;"));
    lay_out(&mut lw, dom, 400.0);
    let (img, body) = widths(&lw);
    assert!(
        (img - body / 4.0).abs() <= 1.0,
        "25% of body's {body}px content box, not of the window: {img}px"
    );
}

#[test]
fn a_percent_wide_inline_image_follows_a_window_resize() {
    // ONE image, as an app that renders the same picture into every frame:
    // a fresh ImageRef per frame changes the content's fingerprint and
    // re-collects the line, which hid the stale measurement.
    let img = image();
    let dom = || {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_child(Dom::create_image(img.clone()).with_css("width: 25%; height: 50px;")),
        )
    };
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, dom(), 400.0);
    let (narrow, narrow_body) = widths(&lw);
    lay_out(&mut lw, dom(), 800.0);
    let (wide, wide_body) = widths(&lw);
    assert!(
        (narrow - narrow_body / 4.0).abs() <= 1.0,
        "premise: 25% of {narrow_body}px at 400px: {narrow}px"
    );
    assert!(
        (wide - wide_body / 4.0).abs() <= 1.0,
        "after the window doubles the image is 25% of {wide_body}px, not its old {narrow}px: {wide}px"
    );
}
