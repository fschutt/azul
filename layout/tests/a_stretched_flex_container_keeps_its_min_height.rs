//! A stretched flex item that is itself a flex container keeps its min-height.
//!
//! CSS Flexbox s9.4: a flex line's cross size comes from its items' hypothetical cross sizes,
//! each already clamped by the item's min / max cross size, and a stretched item's used cross
//! size is the line's, clamped again (step 11). So in a row, an item with `min-height: 22px`
//! and 12 px of content is 22 px tall, and so is the row - Chrome agrees.
//!
//! Measured on the wave-6 engine (the prebuilt AzNotes, the debug server's `mount` op, OFFICE7
//! 2026-10-03), the row `display: flex; width: 400px` around one `flex-grow: 1` item holding an
//! 11 px paragraph:
//! - item `display: block; min-height: 22px` -> 22 px (right);
//! - item `display: flex; flex-direction: column; min-height: 22px` -> 12 px (the content);
//! - the same item with `padding: 1px 2px; border: 1px` -> 16 px; with `align-items: center`
//!   on the row -> 22 px (right).
//! So the min-height of a flex-CONTAINER item is lost when the row stretches it. Every
//! `TextInput` is such an item (`display: flex; flex-direction: column; min-height: 22px`): in a
//! row that stretches - AzNotes' title row - the field is 17 px instead of 22, smaller than the
//! tag field beside it (whose row centres it).
//!
//! Owner: LAYOUT7 (solver3, flex). Not compiled by the author (house rule); RED.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState};
use rust_fontconfig::FcFontCache;

/// A 400 px flex row (`row_css` added) around one growing item (`item_css`)
/// that holds an 11 px paragraph.
fn row(row_css: &str, item_css: &str) -> StyledDom {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!("display: flex; flex-direction: row; width: 400px; {row_css}"))
            .with_id("row".into())
            .with_child(
                Dom::create_div()
                    .with_css(&format!("flex-grow: 1; min-height: 22px; {item_css}"))
                    .with_id("item".into())
                    .with_child(
                        Dom::create_p_with_text("Offsite agenda")
                            .with_css("margin: 0px; font-size: 11px;"),
                    ),
            ),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// The border boxes of `#row` and `#item` once `dom` is laid out at 800 x 600.
fn rects(dom: StyledDom) -> (LogicalRect, LogicalRect) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the row lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let rect = |id: &str| {
        let index = result
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|node| node.has_id(id))
            .unwrap_or_else(|| panic!("no node #{id}"));
        lw.get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        })
        .unwrap_or_else(|| panic!("#{id} has no layout rect"))
    };
    (rect("row"), rect("item"))
}

fn assert_min_height_kept(row_css: &str, item_css: &str) {
    let (row, item) = rects(row(row_css, item_css));
    assert!(
        (item.size.height - 22.0).abs() < 0.5,
        "row {{{row_css}}} item {{{item_css}}}: the item is {} px tall, its min-height is 22 px",
        item.size.height
    );
    assert!(
        (row.size.height - 22.0).abs() < 0.5,
        "row {{{row_css}}} item {{{item_css}}}: the row is {} px tall, its item's min-height is 22 px",
        row.size.height
    );
}

#[test]
fn a_stretched_block_item_keeps_its_min_height() {
    // Right on the wave-6 engine: the reference for the cases below.
    assert_min_height_kept("", "display: block;");
}

#[test]
fn a_stretched_flex_container_keeps_its_min_height() {
    assert_min_height_kept("", "display: flex; flex-direction: column;");
}

#[test]
fn a_stretched_padded_border_box_flex_container_keeps_its_min_height() {
    // A TextInput's container: border-box, padded, bordered, its line centred.
    assert_min_height_kept(
        "",
        "display: flex; flex-direction: column; justify-content: center; box-sizing: border-box; \
         padding: 1px 2px; border: 1px solid black;",
    );
}

#[test]
fn a_centred_flex_container_keeps_its_min_height() {
    // Right on the wave-6 engine (no stretch).
    assert_min_height_kept(
        "align-items: center;",
        "display: flex; flex-direction: column; justify-content: center; box-sizing: border-box; \
         padding: 1px 2px; border: 1px solid black;",
    );
}
