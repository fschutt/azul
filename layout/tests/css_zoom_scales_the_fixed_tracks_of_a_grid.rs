//! `zoom` scales the fixed tracks of a grid with the rest of the box: `grid-template-columns: 200px`
//! under `zoom: 50%` is a column of 100 px, as a `width: 200px` is a width of 100 px. The sizes and
//! the margins of the items of a grid were scaled (see
//! `css_zoom_scales_flex_items_grid_items_and_blocks_alike`); the track lists of the grid
//! (`grid-template-columns`, `grid-template-rows`, `grid-auto-columns`, `grid-auto-rows`) were handed
//! to taffy as the author wrote them, so a zoomed grid kept the size of its tracks and its items
//! stayed as big as before.
//!
//! The case is the one of the probe of the flex items: a box `a` (`zoom: 50%`, 400 x 300) laid out
//! as a grid, here with the tracks the author gave it and four cells in it. Only the lengths are
//! scaled: `fr`, `auto`, `min-content` and `max-content` follow what they are of.

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

/// body (0) > a (1) > the cells (2, 3, 4, 5).
const CELLS: [NodeId; 4] = [NodeId::new(2), NodeId::new(3), NodeId::new(4), NodeId::new(5)];

/// A grid `a` of 400 x 300 under `zoom` (`""`: none) with the declarations `grid`, holding four
/// cells of `cell` each.
fn laid_out(zoom: &str, grid: &str, cell: &str) -> LayoutWindow {
    let mut a = Dom::create_div().with_css(&format!(
        "{zoom} display: grid; width: 400px; height: 300px; {grid}"
    ));
    for _ in 0..4 {
        a = a.with_child(Dom::create_div().with_css(cell));
    }
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(a);

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// The box of `node`: `(x, y, width, height)`.
fn rect(lw: &LayoutWindow, node: NodeId) -> (f32, f32, f32, f32) {
    let r = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .expect("harness: the node has a box");
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

fn cells(lw: &LayoutWindow) -> [(f32, f32, f32, f32); 4] {
    CELLS.map(|cell| rect(lw, cell))
}

fn assert_cells(lw: &LayoutWindow, expected: [(f32, f32, f32, f32); 4], what: &str) {
    let found = cells(lw);
    for (i, (f, e)) in found.iter().zip(&expected).enumerate() {
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(
            near(f.0, e.0) && near(f.1, e.1) && near(f.2, e.2) && near(f.3, e.3),
            "{what}: cell {i} is {f:?}, expected {e:?} (all: {found:?})"
        );
    }
}

/// The tracks 200 px and 100 px by 100 px and 50 px, unzoomed: four cells of the size of their
/// tracks.
#[test]
fn the_fixed_tracks_of_an_unzoomed_grid_size_its_cells() {
    let lw = laid_out(
        "",
        "grid-template-columns: 200px 100px; grid-template-rows: 100px 50px;",
        "",
    );
    assert_cells(
        &lw,
        [
            (0.0, 0.0, 200.0, 100.0),
            (200.0, 0.0, 100.0, 100.0),
            (0.0, 100.0, 200.0, 50.0),
            (200.0, 100.0, 100.0, 50.0),
        ],
        "no zoom",
    );
}

/// At half the size the tracks are 100 px and 50 px by 50 px and 25 px.
#[test]
fn the_fixed_tracks_of_a_zoomed_grid_are_scaled() {
    let lw = laid_out(
        "zoom: 50%;",
        "grid-template-columns: 200px 100px; grid-template-rows: 100px 50px;",
        "",
    );
    assert_cells(
        &lw,
        [
            (0.0, 0.0, 100.0, 50.0),
            (100.0, 0.0, 50.0, 50.0),
            (0.0, 50.0, 100.0, 25.0),
            (100.0, 50.0, 50.0, 25.0),
        ],
        "zoom 50%",
    );
    let lw = laid_out(
        "zoom: 2;",
        "grid-template-columns: 50px 25px; grid-template-rows: 10px 20px;",
        "",
    );
    assert_cells(
        &lw,
        [
            (0.0, 0.0, 100.0, 20.0),
            (100.0, 0.0, 50.0, 20.0),
            (0.0, 20.0, 100.0, 40.0),
            (100.0, 20.0, 50.0, 40.0),
        ],
        "zoom 2",
    );
}

/// The tracks a grid makes for items that have none of their own (`grid-auto-*`) are lengths too.
#[test]
fn the_implicit_tracks_of_a_zoomed_grid_are_scaled() {
    let lw = laid_out(
        "zoom: 50%;",
        "grid-template-columns: 200px 100px; grid-auto-rows: 60px;",
        "",
    );
    assert_cells(
        &lw,
        [
            (0.0, 0.0, 100.0, 30.0),
            (100.0, 0.0, 50.0, 30.0),
            (0.0, 30.0, 100.0, 30.0),
            (100.0, 30.0, 50.0, 30.0),
        ],
        "grid-auto-rows",
    );
    let lw = laid_out("zoom: 50%;", "grid-auto-columns: 80px; grid-auto-flow: column;", "");
    let found = cells(&lw);
    for (i, cell) in found.iter().enumerate() {
        assert!(
            (cell.0 - 40.0 * i as f32).abs() < 0.01 && (cell.2 - 40.0).abs() < 0.01,
            "grid-auto-columns: cell {i} is {cell:?} (all: {found:?})"
        );
    }
}

/// What is not a length follows what it is of: `fr` takes what the fixed tracks leave of the zoomed
/// box (200 px wide), `%` is a part of it.
#[test]
fn fr_and_percentages_follow_the_zoomed_box() {
    let lw = laid_out(
        "zoom: 50%;",
        "grid-template-columns: 1fr 100px; grid-template-rows: 100px 50px;",
        "",
    );
    let found = cells(&lw);
    assert!((found[0].2 - 150.0).abs() < 0.01, "the fr track is what the 50 px leave: {found:?}");
    assert!((found[1].2 - 50.0).abs() < 0.01, "the 100 px track is 50: {found:?}");
    assert!((found[0].3 - 50.0).abs() < 0.01, "a row of 100 px is 50: {found:?}");
}

/// The cells inside a zoomed grid take the zoom of the grid, not their own again.
#[test]
fn the_cells_are_scaled_once() {
    let lw = laid_out(
        "zoom: 50%;",
        "grid-template-columns: 200px 200px; grid-template-rows: 100px 100px;",
        "width: 100px; height: 40px;",
    );
    let found = cells(&lw);
    for cell in found {
        assert!((cell.2 - 50.0).abs() < 0.01 && (cell.3 - 20.0).abs() < 0.01, "{found:?}");
    }
}
