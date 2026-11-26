//! `zoom` scales a box and its whole subtree - its width and height, its padding, margins and gaps -
//! whatever the boxes are laid out by: as blocks, as flex items, as grid items. Flex items were not
//! scaled (taffy was handed the unzoomed lengths): a zoomed flex box kept the size of its flex
//! items, a flex item of an unzoomed parent kept its own, and a page built of flex boxes could not
//! be zoomed at all.
//!
//! The case is the same for every layout: a box `a` (`zoom: 50%`, 400 x 300) holding `b` (200 x 100,
//! padding 20px 0 10px 0) holding `c` (100 x 40), under a parent `p` (600 x 320). The boxes
//! differ only in which of them is a flex container (or a grid container); the answer is the one of
//! the blocks: `a` 200 x 150, `b` 100 x 65, `c` 50 x 20, ten pixels below the top of `b` (half the
//! padding of 20).

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

/// body (0) > p (1) > a (2) > b (3) > c (4).
const A: NodeId = NodeId::new(2);
const B: NodeId = NodeId::new(3);
const C: NodeId = NodeId::new(4);

/// The page, the parent, the zoomed box and the middle box laid out as `parent`, `zoomed` and `inner`
/// (`""`, or a `display` declaration).
fn laid_out(parent: &str, zoomed: &str, inner: &str) -> LayoutWindow {
    let c = Dom::create_div().with_css("width: 100px; height: 40px; flex-shrink: 0;");
    let b = Dom::create_div()
        .with_css(&format!(
            "{inner} position: relative; overflow: hidden; width: 200px; height: 100px; \
             padding: 20px 0px 10px 0px; flex-shrink: 0;"
        ))
        .with_child(c);
    let a = Dom::create_div()
        .with_css(&format!("{zoomed} zoom: 50%; width: 400px; height: 300px; flex-shrink: 0;"))
        .with_child(b);
    let p = Dom::create_div()
        .with_css(&format!("{parent} width: 600px; height: 320px;"))
        .with_child(a);
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(p);

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

fn rect(lw: &LayoutWindow, node: NodeId) -> (f32, f32, f32, f32) {
    let r = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .expect("harness: the node has a box");
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

/// What the blocks give: `a` 200 x 150, `b` 100 x 65, `c` 50 x 20 ten pixels below the top of `b`.
fn assert_zoomed(lw: &LayoutWindow, what: &str) {
    let (_, _, aw, ah) = rect(lw, A);
    let (_, by, bw, bh) = rect(lw, B);
    let (_, cy, cw, ch) = rect(lw, C);
    assert_eq!((aw, ah), (200.0, 150.0), "{what}: the zoomed box");
    assert_eq!((bw, bh), (100.0, 65.0), "{what}: the box inside it");
    assert_eq!((cw, ch), (50.0, 20.0), "{what}: the box inside that");
    assert!(((cy - by) - 10.0).abs() < 0.01, "{what}: half of the padding of 20: {}", cy - by);
}

const FLEX_ROW: &str = "display: flex; flex-direction: row;";
const FLEX_COLUMN: &str = "display: flex; flex-direction: column;";
const GRID: &str = "display: grid;";

#[test]
fn blocks_are_scaled() {
    assert_zoomed(&laid_out("", "", ""), "all blocks");
}

#[test]
fn a_flex_container_that_is_zoomed_scales_its_flex_items() {
    assert_zoomed(&laid_out("", FLEX_COLUMN, ""), "the zoomed box is a flex column");
}

#[test]
fn a_flex_container_inside_the_zoomed_box_scales_its_flex_items() {
    assert_zoomed(&laid_out("", "", FLEX_COLUMN), "the box inside is a flex column");
}

#[test]
fn a_zoomed_flex_item_of_an_unzoomed_flex_parent_is_scaled() {
    assert_zoomed(&laid_out(FLEX_ROW, "", ""), "the parent is a flex row");
}

#[test]
fn a_page_made_of_flex_boxes_is_scaled() {
    assert_zoomed(&laid_out(FLEX_ROW, FLEX_COLUMN, FLEX_COLUMN), "every box is a flex box");
}

#[test]
fn grid_items_and_grid_containers_are_scaled() {
    assert_zoomed(&laid_out(GRID, "", ""), "the parent is a grid");
    assert_zoomed(&laid_out("", GRID, ""), "the zoomed box is a grid");
    assert_zoomed(&laid_out("", "", GRID), "the box inside is a grid");
}

#[test]
fn inline_blocks_are_scaled() {
    assert_zoomed(
        &laid_out("", "display: inline-block;", "display: inline-block;"),
        "inline blocks",
    );
}
