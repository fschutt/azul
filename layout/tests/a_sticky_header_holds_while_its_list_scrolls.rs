//! A sticky header holds while its list scrolls - without a relayout.
//!
//! AzDrive's file list is the design's: a scrolling `.view` holding a
//! `.view-inner`, in it the column header (`position: sticky; top: 0`) and
//! the rows, where every folder's head is sticky too (`top:` the header's
//! height). Scrolling the list moves its scroll frame and nothing else - no
//! layout runs - so a box that sticks has to follow the scroll offset the
//! way a scrollbar thumb does, through a value the frame reads. azul placed
//! sticky boxes once, at the offsets of the layout pass: the first scroll
//! took the header away with the rows.
//!
//! The checks ask where things are ON SCREEN after a scroll of 150px
//! (`get_node_hit_test_bounds`, the same mapping the renderers and the hit
//! tester use) and what a click at the top of the list lands on.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const VIEW: usize = 1;
const HEADER_CELL: usize = 4;
const FOLDER_CELL: usize = 8;
/// The first row's div (every row is a div and its text).
const FIRST_ROW: usize = 10;
const HEADER: f32 = 20.0;
const FOLDER: f32 = 26.0;
const ROW: f32 = 30.0;
const ROWS: usize = 10;
const SCROLLED: f32 = 150.0;

fn text_div(css: &str, text: &str) -> Dom {
    Dom::create_div()
        .with_css(css)
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

/// body (0) > .view (1) > .view-inner (2) > [.dhead (3) > cell (4) > text
/// (5), .rows (6) > [.group-head (7) > cell (8) > text (9), rows (10, 12,
/// ...) each > text]]
fn list() -> LayoutWindow {
    let mut rows = Dom::create_div().with_child(
        Dom::create_div()
            .with_css(&format!("position: sticky; top: {HEADER}px; height: {FOLDER}px;"))
            .with_child(text_div(&format!("height: {FOLDER}px;"), "Folder")),
    );
    for i in 0..ROWS {
        rows = rows.with_child(text_div(&format!("height: {ROW}px;"), &format!("row {i}")));
    }
    let dom = Dom::create_body()
        .with_css("margin: 0px; font-size: 12px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow-y: auto;")
                .with_child(
                    Dom::create_div()
                        .with_css("position: relative; min-height: 100%;")
                        .with_child(
                            Dom::create_div()
                                .with_css(&format!(
                                    "position: sticky; top: 0px; height: {HEADER}px; \
                                     background: #EEE;"
                                ))
                                .with_child(text_div(&format!("height: {HEADER}px;"), "Name")),
                        )
                        .with_child(rows),
                ),
        );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the list lays out");
    lw
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

fn on_screen_y(lw: &LayoutWindow, index: usize) -> f32 {
    lw.get_node_hit_test_bounds(node(index))
        .unwrap_or_else(|| panic!("node {index} is painted"))
        .origin
        .y
}

/// Scroll the list by `y` the way the wheel does - the offset changes, no
/// layout runs - and give the frame its scroll-driven values (what every
/// host does before it paints: `refresh_scrollbar_transforms`).
fn scroll_to(lw: &mut LayoutWindow, y: f32) {
    let content = HEADER + FOLDER + ROW * ROWS as f32;
    lw.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(VIEW),
        ScrollPosition {
            parent_rect: LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(200.0, 100.0)),
            children_rect: LogicalRect::new(
                LogicalPosition::new(0.0, y),
                LogicalSize::new(200.0, content),
            ),
        },
    );
    lw.refresh_scrollbar_transforms();
}

#[test]
fn premise_unscrolled_everything_is_where_normal_flow_put_it() {
    let lw = list();
    let (header, folder, row) = (
        on_screen_y(&lw, HEADER_CELL),
        on_screen_y(&lw, FOLDER_CELL),
        on_screen_y(&lw, FIRST_ROW),
    );
    assert!(header.abs() < 0.5, "the header at the top: {header}");
    assert!((folder - HEADER).abs() < 0.5, "the folder head under the header: {folder}");
    assert!((row - (HEADER + FOLDER)).abs() < 0.5, "the first row under the folder head: {row}");
}

#[test]
fn the_rows_scroll_and_the_sticky_heads_hold_at_their_insets() {
    let mut lw = list();
    scroll_to(&mut lw, SCROLLED);
    let row = on_screen_y(&lw, FIRST_ROW);
    assert!(
        (row - (HEADER + FOLDER - SCROLLED)).abs() < 0.5,
        "premise: the rows scrolled by {SCROLLED}px, the first is at {row}"
    );
    let header = on_screen_y(&lw, HEADER_CELL);
    assert!(header.abs() < 0.5, "the column header holds at the top: {header}");
    let folder = on_screen_y(&lw, FOLDER_CELL);
    assert!(
        (folder - HEADER).abs() < 0.5,
        "the folder head holds under the column header: {folder}"
    );
}

#[test]
fn a_click_at_the_top_of_the_scrolled_list_lands_on_the_column_header() {
    let mut lw = list();
    scroll_to(&mut lw, SCROLLED);
    let mut hit_tester = CpuHitTester::new();
    hit_tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    let hits = hit_tester.hit_test_scrolled(
        LogicalPosition::new(10.0, 5.0),
        &|d, n| lw.scroll_manager.get_current_offset(d, n),
        &|d, n| lw.gpu_state_manager.painted_transform_of(d, n),
    );
    let front = hits.first().map(|(_, n, _)| n.index());
    assert!(
        matches!(front, Some(3..=5)),
        "the header (3), its cell (4) or its text (5) is in front, not a row: {:?}",
        hits.iter().map(|(_, n, _)| n.index()).collect::<Vec<_>>()
    );
}
