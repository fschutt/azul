//! An `overflow: auto` box that turns out to need a scrollbar reserves the
//! gutter for its own children - wherever it sits in the tree.
//!
//! Reserving it takes a second pass: whether the bar is needed is only known
//! once the children have been laid out, and by then a `width: 100%` child has
//! already been sized against the full width. The block formatting context
//! raised that need into a local variable, so it reached the document-level
//! layout loop only from a node that happened to BE that loop's layout root -
//! i.e. from the root element and nothing else. Every scroll box below it kept
//! laying its children out 12px too wide, and they were clipped or scrolled
//! sideways for no reason.

use azul_core::{
    dom::{Dom, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body > .box > .fill`, laid out in a 600x400 window; returns `.fill`'s width.
fn filler_width(css: &str, fill_height: &str) -> f32 {
    let mut dom = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div().with_css(css).with_child(
            Dom::create_div().with_css(&format!("width: 100%; height: {fill_height};")),
        ),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let dom_id = styled.dom_id;

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    // body(0) > .box(1) > .fill(2)
    lw.get_node_layout_rect(DomNodeId {
        dom: dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    })
    .expect("the filler was laid out")
    .size
    .width
}

const BOX_CSS: &str = "width: 200px; height: 200px; overflow: auto;";

#[test]
fn a_nested_scroll_box_reserves_the_gutter_its_scrollbar_takes() {
    let roomy = filler_width(BOX_CSS, "100px");
    let overflowing = filler_width(BOX_CSS, "500px");
    assert!(
        (roomy - 200.0).abs() < 0.5,
        "nothing overflows, so the filler has the whole box: {roomy}"
    );
    assert!(
        overflowing < roomy - 1.0,
        "the vertical scrollbar takes its gutter out of the filler's 100%: {overflowing} vs \
         {roomy}"
    );
}

#[test]
fn a_nested_scroll_box_reserves_the_same_gutter_it_would_with_overflow_scroll() {
    // `overflow: scroll` reserves the gutter in the FIRST pass - the bar is
    // there whether or not anything overflows - so it is the oracle for what
    // `overflow: auto` has to arrive at once it discovers the same bar.
    let always = filler_width("width: 200px; height: 200px; overflow: scroll;", "500px");
    let discovered = filler_width(BOX_CSS, "500px");
    assert!(
        (discovered - always).abs() < 0.5,
        "the discovered gutter ({discovered}) is the declared one ({always})"
    );
}

/// `body > .box > (.fill, .tail)` in a `window_w` x 300 window: the filler's
/// width and the box's scroll frame clip (its scrollport without the bars).
fn full_width_box(window_w: f32, box_css: &str) -> (f32, LogicalSize) {
    let mut dom = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css(box_css)
            .with_child(Dom::create_div().with_css("height: 400px;"))
            .with_child(Dom::create_div().with_css("width: 300px; height: 150px;")),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let dom_id = styled.dom_id;

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(window_w, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    let fill = lw
        .get_node_layout_rect(DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        })
        .expect("the filler was laid out")
        .size
        .width;
    let clip = lw
        .layout_results
        .get(&dom_id)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            azul_layout::solver3::display_list::DisplayListItem::PushScrollFrame {
                clip_bounds,
                ..
            } => Some(clip_bounds.inner().size),
            _ => None,
        })
        .expect("the box is a scroll frame");
    (fill, clip)
}

/// A scroll box as wide as the window that overflows only downwards: its
/// auto-width child is laid out again inside the vertical bar's gutter, and no
/// horizontal bar takes height from the scrollport - as in Chrome, and as the
/// same box does in a wider window. (Found by a `<webview>` in such a box,
/// clipped 12px short: the box kept the child as wide as itself and reserved a
/// horizontal bar for it.)
#[test]
fn a_scroll_box_as_wide_as_the_window_reserves_its_gutter_and_no_horizontal_bar() {
    const CSS: &str = "overflow: auto; width: 400px; height: 200px;";
    let (wide_fill, wide_clip) = full_width_box(800.0, CSS);
    let (fill, clip) = full_width_box(400.0, CSS);
    assert!(
        (fill - wide_fill).abs() < 0.5 && fill < 399.0,
        "the filler gives the gutter back in both windows: {fill} vs {wide_fill}"
    );
    assert!(
        (clip.height - 200.0).abs() < 0.5,
        "no horizontal bar under content that overflows only downwards: {clip:?} (wider \
         window: {wide_clip:?})"
    );
}
