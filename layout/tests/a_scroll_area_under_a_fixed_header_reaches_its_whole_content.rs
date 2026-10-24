//! A scroll area that fills what a fixed header leaves of a flex column reaches all of its
//! content.
//!
//! The shape is AzCalendar's week view (`examples/azul-calendar`): a window-high flex column
//! (toolbar, the week, a footer), the week a flex row item holding a flex column of a fixed
//! day-header row and a scroll area, and in the scroll area the whole day, 24 hours at 48 px.
//! The header row stays where it is; the scroll area takes the rest of the column
//! (`flex-grow: 1; flex-basis: 0; min-height: 0`) and scrolls over the day (`overflow-y: auto`),
//! from 00:00 at offset 0 down to 24:00 at the bottom edge.
//!
//! CSS Flexbox 1 §4.5: `min-height: 0` turns off a flex item's automatic minimum size (its
//! content height), so the item can be shorter than its content and the content overflows it.
//! §9.8: an item stretched across a single-line row container with a definite cross size is
//! definite, so the week's column has a height for the scroll area to grow into. CSS Overflow 3
//! §2.2: the scroll range is the scrollable overflow minus the scrollport.
//!
//! The user could not scroll AzCalendar's day to its end. The app had no scroll area at all
//! (and showed 08:00 - 20:00 only); this test pins that the engine gives the structure the app
//! now uses exactly the scroll range it asks for.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > toolbar(1), main(2) > week(3) > header(4), scroll(5) > grid(6) > gutter(7),
/// day(8); footer(9)`.
const HEADER: NodeId = NodeId::new(4);
const SCROLL: NodeId = NodeId::new(5);
const GRID: NodeId = NodeId::new(6);

const TOOLBAR_PX: f32 = 50.0;
const HEADER_PX: f32 = 40.0;
const FOOTER_PX: f32 = 20.0;
/// 24 hours at 48 px an hour.
const DAY_PX: f32 = 24.0 * 48.0;

/// Lays out the week view's structure in a `width` x `height` window.
fn lay_out(width: f32, height: f32) -> LayoutWindow {
    let day = format!("flex-grow: 1; flex-basis: 0px; height: {DAY_PX}px;");
    let grid = Dom::create_div()
        .with_css(&format!(
            "display: flex; flex-direction: row; height: {DAY_PX}px; flex-shrink: 0;"
        ))
        .with_child(Dom::create_div().with_css("width: 56px; flex-shrink: 0;"))
        .with_child(Dom::create_div().with_css(&day));
    let scroll = Dom::create_div()
        .with_css(
            "flex-grow: 1; flex-basis: 0px; min-height: 0; overflow-y: auto; overflow-x: hidden;",
        )
        .with_child(grid);
    let week = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0; min-height: 0;",
        )
        .with_child(Dom::create_div().with_css(&format!("height: {HEADER_PX}px; flex-shrink: 0;")))
        .with_child(scroll);
    let main = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0;")
        .with_child(week);
    let body = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column; height: 100%;")
        .with_child(Dom::create_div().with_css(&format!("height: {TOOLBAR_PX}px; flex-shrink: 0;")))
        .with_child(main)
        .with_child(Dom::create_div().with_css(&format!("height: {FOOTER_PX}px; flex-shrink: 0;")));

    let styled = StyledDom::create_from_dom(body);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the week lays out");
    lw
}

/// A node's laid-out top and height.
fn top_and_height(lw: &LayoutWindow, node: NodeId) -> (f32, f32) {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the week is laid out");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&node)
        .and_then(|boxes| boxes.first())
        .expect("harness: the node has a box");
    let top = lr
        .calculated_positions
        .get(idx.index())
        .map(|p| p.y)
        .expect("harness: the box is positioned");
    let height = lr.layout_tree.nodes[idx.index()]
        .used_size
        .expect("harness: the box is sized")
        .height;
    (top, height)
}

/// How far the scroll area scrolls down.
fn scroll_range(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_scroll_state(DomId::ROOT_ID, SCROLL)
        .expect("the day overflows the scroll area, so it is a registered scroll box")
        .max_scroll_offsets()
        .1
}

#[test]
fn the_week_grid_scrolls_over_the_whole_day_below_its_fixed_header() {
    let (width, height) = (800.0, 600.0);
    let lw = lay_out(width, height);

    // The header row sits under the toolbar and is not inside the scroll area.
    let (header_top, header_height) = top_and_height(&lw, HEADER);
    assert!(
        (header_top - TOOLBAR_PX).abs() < 0.5 && (header_height - HEADER_PX).abs() < 0.5,
        "the day-header row must sit under the toolbar, {HEADER_PX} px high; it is at \
         {header_top}, {header_height} px high"
    );

    // The scroll area takes what the toolbar, the header and the footer leave.
    let view = height - TOOLBAR_PX - HEADER_PX - FOOTER_PX;
    let (scroll_top, scroll_height) = top_and_height(&lw, SCROLL);
    assert!(
        (scroll_top - (TOOLBAR_PX + HEADER_PX)).abs() < 0.5,
        "the scroll area must start under the header, at {}; it starts at {scroll_top}",
        TOOLBAR_PX + HEADER_PX
    );
    assert!(
        (scroll_height - view).abs() < 0.5,
        "the scroll area must take the {view} px the fixed rows leave (min-height: 0 lets it be \
         shorter than the day); it is {scroll_height} px high"
    );

    // The day keeps its height inside it, and all of it is reachable.
    let (_, grid_height) = top_and_height(&lw, GRID);
    assert!(
        (grid_height - DAY_PX).abs() < 0.5,
        "the day must keep its {DAY_PX} px inside the scroll area; it is {grid_height} px"
    );
    let range = scroll_range(&lw);
    assert!(
        (range - (DAY_PX - view)).abs() < 0.5,
        "scrolled to the end, 24:00 must meet the scroll area's bottom edge: the range must be \
         the day ({DAY_PX} px) minus the scrollport ({view} px) = {} px; it is {range} px",
        DAY_PX - view
    );
}

#[test]
fn a_taller_window_gives_the_week_more_room_and_scrolls_less() {
    let (width, height) = (800.0, 900.0);
    let lw = lay_out(width, height);
    let view = height - TOOLBAR_PX - HEADER_PX - FOOTER_PX;
    let (_, scroll_height) = top_and_height(&lw, SCROLL);
    assert!(
        (scroll_height - view).abs() < 0.5,
        "in a {height} px window the scroll area must be {view} px high; it is {scroll_height} px"
    );
    let range = scroll_range(&lw);
    assert!(
        (range - (DAY_PX - view)).abs() < 0.5,
        "the range must shrink with the taller scrollport to {} px; it is {range} px",
        DAY_PX - view
    );
}
