//! A drag autoscrolls the box its anchor is scrolled in - by CONTAINING
//! BLOCK, the rule the display list paints by (`ScrollChain`), not by DOM
//! parent.
//!
//! "Which scroll box does this node live in?" had one answer left from
//! before the ScrollChain (R5 of SELECTION_WHEN_CLIPPED §2.2):
//! `ScrollManager::find_scroll_parent`, the nearest DOM ancestor with a
//! scroll state. The drag autoscroll (`LayoutWindow::drag_autoscroll_box`),
//! `CallbackInfo::find_scroll_parent` / `find_scroll_target` and the
//! momentum hand-off asked it; the caret reveal's
//! `find_scrollable_ancestor` walked layout parents instead. Both disagree
//! with paint for an out-of-flow box: an `absolute` box whose containing
//! block is outside a non-positioned scroll box is not scrolled by that box,
//! and a `fixed` box is scrolled by nothing, not even the page. Dragging
//! over such a box scrolled a box that does not move it.
//!
//! Every page is 400x300 without margins.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    spaces::Inclusivity,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn window_with(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// body(0) > scroller(1) `overflow-y: scroll`, NOT positioned >
/// [400px(2), in-flow(3), absolute(4)]. The absolute box's containing block
/// is the initial one: it is painted outside the scroller's frame and does
/// not scroll with it. The page itself fits.
fn a_scroller_with_an_absolute_child() -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css("width: 200px; height: 100px; overflow-y: scroll;")
            .with_child(Dom::create_div().with_css("height: 400px;"))
            .with_child(Dom::create_div().with_css("height: 20px;"))
            .with_child(Dom::create_div().with_css(
                "position: absolute; top: 10px; left: 250px; width: 50px; height: 20px;",
            )),
    )
}

#[test]
fn a_drag_over_an_absolute_box_does_not_scroll_a_box_that_does_not_move_it() {
    let lw = window_with(a_scroller_with_an_absolute_child());
    let scroller = node(1);
    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, NodeId::new(1))
            .is_some(),
        "harness: the scroller overflows and is registered"
    );
    // Control: an in-flow child lives in the scroller, by either rule.
    assert_eq!(
        lw.drag_autoscroll_box(node(3)),
        Some(scroller),
        "an in-flow child is scrolled by the scroller"
    );
    assert_eq!(lw.find_scrollable_ancestor(node(3)), Some(scroller));
    // The two questions `inclusivity` tells apart: the scroller is the box
    // it lives in itself, and chains to nothing (the page fits).
    assert_eq!(
        lw.scroll_box_of_node(DomId::ROOT_ID, NodeId::new(1), Inclusivity::SelfAndAncestors),
        Some(NodeId::new(1))
    );
    assert_eq!(
        lw.scroll_box_of_node(DomId::ROOT_ID, NodeId::new(1), Inclusivity::AncestorsOnly),
        None
    );

    assert_eq!(
        lw.drag_autoscroll_box(node(4)),
        None,
        "the absolute box is painted outside the scroller's frame (its containing block is the \
         page, which fits): a drag over it must not scroll the scroller"
    );
    assert_eq!(
        lw.find_scrollable_ancestor(node(4)),
        None,
        "and nothing scrolls it into view"
    );
}

/// body(0) > [fixed(1), 1000px(2)]: a page that scrolls under a fixed
/// header.
#[test]
fn a_drag_over_a_fixed_box_does_not_scroll_the_page_under_it() {
    let lw = window_with(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css(
                "position: fixed; top: 0; left: 0; width: 100px; height: 30px;",
            ))
            .with_child(Dom::create_div().with_css("height: 1000px;")),
    );
    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, NodeId::new(0))
            .is_some(),
        "harness: the page overflows the window, so the viewport scrolls it"
    );
    // Control: the in-flow block is on the page.
    assert_eq!(lw.drag_autoscroll_box(node(2)), Some(node(0)));

    assert_eq!(
        lw.drag_autoscroll_box(node(1)),
        None,
        "a fixed box is scrolled by no frame - the page's included: a drag over it scrolls \
         nothing"
    );
    assert_eq!(lw.find_scrollable_ancestor(node(1)), None);
}
