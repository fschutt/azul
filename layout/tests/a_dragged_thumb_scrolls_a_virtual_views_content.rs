//! A dragged scrollbar thumb scrolls a `VirtualView`'s content, not only its
//! bar.
//!
//! A `VirtualView` has no scroll frame. Its content moves with the
//! `content_offset` of its display-list item, and a scroll past the rows it
//! rendered asks its callback for new ones. The wheel reaches the offset
//! through `CallbackChange::ScrollTo`, which does both. The thumb reaches it
//! through `LayoutWindow::route_move`, which only moved the offset: the bar
//! followed the pointer, and the rows stayed where they were until the next
//! wheel step re-pointed the item (AzDrive's file list, AzTerm).
//!
//! The page is one 300x200 `VirtualView` over 500 rows of 20 px. Its callback
//! renders the rows in view and ten either side, the way AzDrive's does.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId, OptionDom, ScrollbarOrientation},
    events::MouseButton,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIN_W: f32 = 400.0;
const WIN_H: f32 = 300.0;
const ROW: f32 = 20.0;
const ROWS: usize = 500;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
extern "C" fn rows(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let size = info.bounds.get_logical_size();
    let first = ((info.scroll_offset.y.max(0.0) / ROW) as usize).saturating_sub(10);
    let last = (first + 30).min(ROWS);
    let mut list = Dom::create_div().with_css("display: flex; flex-direction: column;");
    for _ in first..last {
        list.add_child(Dom::create_div().with_css(&format!("height: {ROW}px;")));
    }
    VirtualViewReturn {
        dom: OptionDom::Some(list),
        materialized: LogicalRect::new(
            LogicalPosition::new(0.0, first as f32 * ROW),
            LogicalSize::new(size.width, (last - first) as f32 * ROW),
        ),
        virtual_rect: LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(size.width, ROWS as f32 * ROW),
        ),
    }
}

/// The page, laid out twice: the way a window stands once it has shown a
/// frame, with the view's extent published and its bar painted.
pub fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIN_W, WIN_H);
    lw.current_window_state = ws.clone();
    for _ in 0..2 {
        let page = Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(rows))
                .with_css("width: 300px; height: 200px;"),
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
    }
    lw
}

/// The page's `VirtualView` node and the dom it mounted.
pub fn host(lw: &LayoutWindow) -> (NodeId, DomId) {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let n = lr.styled_dom.node_data.as_container().len();
    (0..n)
        .map(NodeId::new)
        .find_map(|node| {
            lw.virtual_view_manager
                .get_nested_dom_id(DomId::ROOT_ID, node)
                .map(|child| (node, child))
        })
        .expect("the page mounted its VirtualView")
}

/// The `content_offset` the page's display list composites the view with.
pub fn item_content_offset(lw: &LayoutWindow, child: DomId) -> LogicalPosition {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::VirtualView {
                child_dom_id,
                content_offset,
                ..
            } if *child_dom_id == child => Some(*content_offset),
            _ => None,
        })
        .expect("the page's display list composites the view")
}

/// The middle of the view's vertical thumb, in window space.
fn thumb_centre(lw: &LayoutWindow, node: NodeId) -> LogicalPosition {
    let bar = lw
        .scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, node, ScrollbarOrientation::Vertical)
        .filter(|b| b.visible)
        .expect("harness: the view's 10000 px of rows give it a vertical bar");
    LogicalPosition::new(
        bar.track_rect.origin.x + bar.track_rect.size.width / 2.0,
        bar.track_rect.origin.y + bar.button_size + bar.thumb_offset + bar.thumb_length / 2.0,
    )
}

/// Press the thumb and drag it `dy` px down.
fn drag_thumb(lw: &mut LayoutWindow, node: NodeId, dy: f32) {
    let at = thumb_centre(lw, node);
    assert!(
        lw.route_press(at, MouseButton::Left, Instant::now())
            .is_scrollbar(),
        "harness: a press at {at:?} lands on the view's thumb"
    );
    assert!(
        lw.scrollbar_drag().is_some(),
        "harness: the press holds the thumb"
    );
    assert!(lw.route_move(LogicalPosition::new(at.x, at.y + dy), Instant::now()));
}

#[test]
fn a_dragged_thumb_moves_the_rows_with_the_bar() {
    let mut lw = window();
    let (node, child) = host(&lw);
    drag_thumb(&mut lw, node, 4.0);
    let offset = lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, node)
        .unwrap_or_default();
    assert!(
        offset.y > 1.0,
        "harness: the drag scrolled the view, offset {offset:?}"
    );
    assert_eq!(
        item_content_offset(&lw, child),
        lw.virtual_view_content_offset(DomId::ROOT_ID, node),
        "the thumb scrolled the view to {offset:?}: the rows it shows must move with it, in the \
         same pointer move, as they do for a wheel step"
    );
}

#[test]
fn a_thumb_dragged_past_the_rendered_rows_asks_the_view_for_new_ones() {
    let mut lw = window();
    let (node, _child) = host(&lw);
    // A third of the bar's travel: some 3000 px down, far past the 40 rows
    // (800 px) the view rendered around the top.
    drag_thumb(&mut lw, node, 60.0);
    let offset = lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, node)
        .unwrap_or_default();
    assert!(
        offset.y > 1000.0,
        "harness: the drag carried the view past its rows, offset {offset:?}"
    );
    assert!(
        lw.pending_virtual_view_updates
            .get(&DomId::ROOT_ID)
            .is_some_and(|nodes| nodes.contains_key(&node)),
        "the view shows {offset:?}, where it rendered no rows: the drag must queue its callback \
         for the next frame, as a wheel jump there does"
    );
}
