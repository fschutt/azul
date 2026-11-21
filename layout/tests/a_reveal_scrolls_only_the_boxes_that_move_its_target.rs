//! A reveal scrolls only the boxes whose scroll moves its target.
//!
//! `scroll_node_into_view` (a keyboard focus move, an a11y `Focus` or
//! `ScrollIntoView`, an app's request) asks `scroll_into_view::
//! find_scrollable_ancestors` which scroll boxes to move, innermost first.
//! That walk followed DOM parents, and after crossing into a `VirtualView`'s
//! host dom, the host's DOM parents. Paint follows CONTAINING BLOCKS (the
//! `ScrollChain`): a `fixed` box is moved by no scroll frame, the page's
//! included, and an `absolute` box is not moved by a non-positioned scroll
//! box between it and its containing block. Revealing such a box scrolled a
//! box that does not move it - focusing a button in a fixed toolbar threw a
//! scrolled page back to the top.
//!
//! Every page is 400x300 without margins.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, DomNodeId, NodeId, OptionDom},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, managers::scroll_into_view::ScrollIntoViewOptions,
    window::LayoutWindow, window_state::FullWindowState,
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

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn node_in(dom: DomId, index: usize) -> DomNodeId {
    DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// Scrolls the page (the viewport's frame, on the root) to `y`.
fn scroll_page_to(lw: &mut LayoutWindow, y: f32) {
    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, NodeId::new(0))
            .is_some(),
        "harness: the page overflows the window, so the viewport scrolls it"
    );
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(0),
        LogicalPosition::new(0.0, y),
        now(),
    );
    assert_eq!(page_offset(lw), y, "harness: the page is scrolled");
}

fn page_offset(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_current_offset(DomId::ROOT_ID, NodeId::new(0))
        .map_or(0.0, |o| o.y)
}

/// body(0) > [fixed toolbar(1), top item(2), 1000px(3)]: a page that
/// scrolls under a toolbar fixed to the window's top edge.
#[test]
fn revealing_a_fixed_box_does_not_scroll_the_page_under_it() {
    let mut lw = window_with(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(
                Dom::create_div()
                    .with_css("position: fixed; top: 0; left: 0; width: 100px; height: 30px;"),
            )
            .with_child(Dom::create_div().with_css("height: 20px;"))
            .with_child(Dom::create_div().with_css("height: 1000px;")),
    );
    scroll_page_to(&mut lw, 500.0);

    let adjustments = lw.scroll_node_into_view(
        node_in(DomId::ROOT_ID, 1),
        ScrollIntoViewOptions::nearest(),
        now(),
    );
    assert!(
        adjustments.is_empty(),
        "a fixed box is on screen wherever the page is scrolled: nothing may scroll to reveal it \
         (got {adjustments:?})"
    );
    assert_eq!(
        page_offset(&lw),
        500.0,
        "focusing the toolbar must not throw the page back to its top"
    );

    // Control: the in-flow item at the page's top IS moved by the page, and
    // revealing it scrolls the page back up.
    lw.scroll_node_into_view(
        node_in(DomId::ROOT_ID, 2),
        ScrollIntoViewOptions::nearest(),
        now(),
    );
    assert_eq!(
        page_offset(&lw),
        0.0,
        "an in-flow item is revealed through the page"
    );
}

/// body(0) > scroller(1) `overflow-y: scroll`, NOT positioned >
/// [400px(2), absolute(3)]. The absolute box's containing block is the
/// initial one: it is painted outside the scroller's frame, at (250,150),
/// and does not scroll with it. The page itself fits.
#[test]
fn revealing_an_absolute_box_does_not_scroll_a_box_it_escapes() {
    let mut lw = window_with(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 400px;"))
                .with_child(Dom::create_div().with_css(
                    "position: absolute; top: 150px; left: 250px; width: 50px; height: 20px;",
                )),
        ),
    );
    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, NodeId::new(1))
            .is_some(),
        "harness: the scroller overflows and is registered"
    );

    let adjustments = lw.scroll_node_into_view(
        node_in(DomId::ROOT_ID, 3),
        ScrollIntoViewOptions::nearest(),
        now(),
    );
    assert!(
        adjustments.is_empty(),
        "the absolute box is painted outside the scroller's frame: scrolling the scroller does \
         not move it (got {adjustments:?})"
    );
    assert_eq!(
        lw.scroll_manager
            .get_current_offset(DomId::ROOT_ID, NodeId::new(1))
            .map_or(0.0, |o| o.y),
        0.0,
        "the scroller stays where it was"
    );
}

extern "C" fn child_page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    // Nothing virtualized: the whole child document is materialized.
    let page = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    VirtualViewReturn {
        dom: OptionDom::Some(
            Dom::create_body()
                .with_css("margin: 0;")
                .with_child(Dom::create_div().with_css("height: 20px;"))
                .with_child(Dom::create_div().with_css("height: 20px;")),
        ),
        materialized: page,
        virtual_rect: page,
    }
}

/// The dom the page's `VirtualView` mounted.
fn child_dom(lw: &LayoutWindow) -> DomId {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let n = lr.styled_dom.node_data.as_container().len();
    (0..n)
        .map(NodeId::new)
        .find_map(|node| {
            lw.virtual_view_manager
                .get_nested_dom_id(DomId::ROOT_ID, node)
        })
        .expect("the page mounted its VirtualView")
}

/// body(0) > [VirtualView(1) fixed to the window's top edge, 1000px(2)]; the
/// view's child dom is body(0) > [20px(1), target(2)]. The reveal crosses
/// from the child dom into the page through the view - and the view, fixed,
/// is moved by no frame of the page.
#[test]
fn revealing_a_node_in_a_fixed_virtual_view_does_not_scroll_the_page() {
    let mut lw = window_with(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(child_page))
                    .with_css("position: fixed; top: 0; left: 0; width: 200px; height: 100px;"),
            )
            .with_child(Dom::create_div().with_css("height: 1000px;")),
    );
    let child = child_dom(&lw);
    scroll_page_to(&mut lw, 500.0);

    let adjustments =
        lw.scroll_node_into_view(node_in(child, 2), ScrollIntoViewOptions::nearest(), now());
    assert!(
        adjustments.is_empty(),
        "the target is on screen in a view the page's scroll does not move (got {adjustments:?})"
    );
    assert_eq!(page_offset(&lw), 500.0, "the page stays where it was");
}
