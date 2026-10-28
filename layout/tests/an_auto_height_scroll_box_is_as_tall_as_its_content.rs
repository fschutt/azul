//! An auto-height scroll box is as tall as its content.
//!
//! `overflow: auto` makes a box a scroll container, but in block flow its
//! `height: auto` is still its content's height, clamped by `max-height`
//! (CSS 2.2 10.6.3, 10.7); only what exceeds that height scrolls. Only where
//! the height is someone else's answer - a flex or grid item - does the
//! box keep the size it was given. A box that took its containing block's
//! leftover instead collapsed to 0 px when nothing came above it: the root
//! of a popover with `overflow: auto`.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `dom` laid out in a 400 x 300 window: the height of node `node`, and
/// whether the display list has a scroll frame.
fn height_of(mut dom: Dom, node: usize) -> (f32, bool) {
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    let height = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        })
        .expect("laid out")
        .size
        .height;
    let scrolls = lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .any(|i| {
            matches!(
                i,
                azul_layout::solver3::display_list::DisplayListItem::PushScrollFrame { .. }
            )
        });
    (height, scrolls)
}

/// `div(box_css) > 3 x div(20 px)`, alone or inside a body.
fn scroll_box(box_css: &str, as_root: bool) -> (f32, bool) {
    let mut scroller = Dom::create_div().with_css(box_css);
    for _ in 0..3 {
        scroller = scroller.with_child(Dom::create_div().with_css("width: 120px; height: 20px;"));
    }
    if as_root {
        height_of(scroller, 0)
    } else {
        height_of(Dom::create_body().with_css("margin: 0;").with_child(scroller), 1)
    }
}

#[test]
fn a_scroll_box_in_block_flow_is_as_tall_as_its_content() {
    assert_eq!(scroll_box("overflow: auto;", false).0, 60.0);
    assert_eq!(scroll_box("overflow-y: scroll;", false).0, 60.0);
    assert_eq!(scroll_box("overflow: hidden;", false).0, 60.0, "as a clipping box is");
}

#[test]
fn a_scroll_box_as_the_root_of_its_document_is_as_tall_as_its_content() {
    assert_eq!(scroll_box("overflow: auto;", true).0, 60.0);
}

#[test]
fn a_scroll_boxs_max_height_caps_it_and_the_rest_scrolls() {
    let (height, scrolls) = scroll_box("overflow: auto; max-height: 40px;", false);
    assert_eq!(height, 40.0);
    assert!(scrolls, "the 20 px past its max-height scroll");
}

#[test]
fn a_scrolling_flex_item_keeps_the_height_its_container_gave_it() {
    let mut list = Dom::create_div().with_css("flex-grow: 1; min-height: 0; overflow: auto;");
    for _ in 0..30 {
        list = list.with_child(Dom::create_div().with_css("height: 20px;"));
    }
    // The header does not shrink: a shrinkable one gives up its share of the
    // 400 px the column is short by (100 of 700 px of bases), and the list
    // ends at 257 px, as in a browser.
    let page = Dom::create_body()
        .with_css("margin: 0; height: 300px; display: flex; flex-direction: column;")
        .with_child(Dom::create_div().with_css("height: 100px; flex-shrink: 0;"))
        .with_child(list);
    // body = 0, header = 1, list = 2
    assert_eq!(height_of(page, 2).0, 200.0, "the column's rest, not its 600 px of rows");
}
