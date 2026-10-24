//! A scrollbar inside a `VirtualView`'s child dom is pressed where it is
//! painted.
//!
//! A child dom is laid out 0-relative and composited where its host's
//! `VirtualView` item puts it (`headless::nested_dom_window_origin`, the
//! one lift the raster, the hit tester and the IME rects share). The scroll
//! manager builds the bars every shell hit-tests BEFORE the content
//! (`hit_test_scrollbars`, through `LayoutWindow::route_press`) from each
//! box's registered scrollport - for a child dom, child-local coordinates -
//! and moved them only by the scroll frames inside their own dom. So a
//! child's bar was pressed near the WINDOW's top-left corner, over whatever
//! the page shows there, and not where it is painted.
//!
//! The page puts a 240x200 `VirtualView` at (100, 150); its child dom holds
//! one 160x100 `overflow-y: scroll` box at its own origin.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId, OptionDom},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollbarHitId,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 500.0;
const H: f32 = 400.0;
/// Where the page puts the `VirtualView`'s box.
const VIEW_X: f32 = 100.0;
const VIEW_Y: f32 = 150.0;
/// The child dom's scroll box: body(0) > box(1) > 400px(2).
const CHILD_BOX: NodeId = NodeId::new(1);

extern "C" fn child_page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    // Nothing virtualized: the whole child document is materialized.
    let page = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    VirtualViewReturn {
        dom: OptionDom::Some(
            Dom::create_body().with_css("margin: 0;").with_child(
                Dom::create_div()
                    .with_css("width: 160px; height: 100px; overflow-y: scroll;")
                    .with_child(Dom::create_div().with_css("height: 400px;")),
            ),
        ),
        materialized: page,
        virtual_rect: page,
    }
}

/// The page laid out once, the way the shells run a pass: the window state
/// first, then the funnel, which invokes the `VirtualView` and publishes the
/// scroll states of both doms.
fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let page = Dom::create_body()
        .with_css(&format!(
            "margin: 0; padding-left: {VIEW_X}px; padding-top: {VIEW_Y}px;"
        ))
        .with_child(
            Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(child_page))
                .with_css("width: 240px; height: 200px;"),
        );
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
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

/// The child box's vertical bar as the child's display list paints it, in
/// the child dom's own 0-relative space.
fn painted_bar(lw: &LayoutWindow, child: DomId) -> LogicalRect {
    let lr = lw
        .get_layout_result(&child)
        .expect("the child dom is laid out");
    lr.display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::ScrollBarStyled { info }
                if info.hit_id == Some(ScrollbarHitId::VerticalThumb(child, CHILD_BOX)) =>
            {
                Some(*info.bounds.inner())
            }
            _ => None,
        })
        .expect("the child's scroll box paints a vertical bar")
}

fn centre(r: LogicalRect) -> LogicalPosition {
    LogicalPosition::new(
        r.origin.x + r.size.width / 2.0,
        r.origin.y + r.size.height / 2.0,
    )
}

#[test]
fn a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted() {
    let lw = window();
    let child = child_dom(&lw);
    let local = painted_bar(&lw, child);
    // Where the raster composites the child dom - and where the hit tester
    // maps clicks into it.
    let origin = lw.window_space_offset_of_dom(child);
    assert!(
        origin.x > 50.0 && origin.y > 50.0,
        "harness: the child dom is composited away from the window's origin, got {origin:?}"
    );
    let on_screen = LogicalRect::new(
        LogicalPosition::new(local.origin.x + origin.x, local.origin.y + origin.y),
        local.size,
    );

    let hit = lw
        .scroll_manager
        .hit_test_scrollbars(centre(on_screen))
        .map(|h| (h.dom_id, h.node_id));
    assert_eq!(
        hit,
        Some((child, CHILD_BOX)),
        "the child's bar is painted at {on_screen:?}: a press at its centre must take it"
    );

    let stray = lw
        .scroll_manager
        .hit_test_scrollbars(centre(local))
        .map(|h| (h.dom_id, h.node_id));
    assert_ne!(
        stray,
        Some((child, CHILD_BOX)),
        "{local:?} is the bar's CHILD-LOCAL rect - on screen that is the page's padding, which \
         a press must reach"
    );
}
