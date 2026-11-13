//! A flex item's negative margin overhangs its row without growing it.
//!
//! CSS Flexbox 1 §9.4: a single-line flex container's auto cross size is its
//! line's, and the line is as tall as its items' OUTER (margin-box) cross
//! sizes - an item with `margin-bottom: -2px` counts 2 px less, and aligned to
//! the end its border box reaches 2 px past the line, into the container's
//! border. That is what taffy computes; the box grew afterwards, to the
//! scrollable overflow (the overhanging border box), so the row came out 2 px
//! taller than in a browser and the item never reached its border. A flora
//! tab row is exactly this: the selected tab reaches down over the strip's
//! rule (`margin-bottom: -2px`) so no metal line runs under it - and the rule
//! moved down with the grown strip and stayed visible under the tab.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The row (`display: flex`, items at its end, a 2 px bottom border) holding a
/// 32 px item and a 28 px item styled `item`, and their rects.
fn laid_out(item: &str, row: &str) -> (LogicalRect, LogicalRect, LogicalRect) {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_id("row".into())
            .with_css(&format!(
                "display: flex; flex-direction: row; align-items: flex-end; \
                 border-bottom: 2px solid transparent; width: 300px; {row}"
            ))
            .with_child(
                Dom::create_div()
                    .with_id("tall".into())
                    .with_css("width: 50px; height: 32px; flex-shrink: 0;"),
            )
            .with_child(
                Dom::create_div()
                    .with_id("item".into())
                    .with_css(&format!("width: 50px; height: 28px; flex-shrink: 0; {item}")),
            ),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the row lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let rect = |id: &str| {
        let index = result
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|n| n.has_id(id))
            .unwrap_or_else(|| panic!("no #{id}"));
        lw.get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        })
        .unwrap_or_else(|| panic!("#{id} has no rect"))
    };
    (rect("row"), rect("tall"), rect("item"))
}

fn bottom(r: &LogicalRect) -> f32 {
    r.origin.y + r.size.height
}

#[test]
fn a_negative_end_margin_lets_an_item_overhang_its_row_without_growing_it() {
    let (row, tall, item) = laid_out("margin-bottom: -2px;", "");
    assert!(
        (row.size.height - 34.0).abs() < 0.01,
        "the row is its line (the tallest margin box, 32) and its border (2): {row:?}"
    );
    assert!((bottom(&tall) - 32.0).abs() < 0.01, "{tall:?}");
    assert!(
        (bottom(&item) - 34.0).abs() < 0.01,
        "the item's margin box ends at the line's end, its border box 2 px past it, over the \
         row's border: {item:?}"
    );
}

#[test]
fn a_relatively_shifted_item_moves_without_growing_its_row() {
    // `position: relative` moves an item where it is painted, not the line.
    let (row, _, item) = laid_out("position: relative; top: 10px;", "");
    assert!(
        (row.size.height - 34.0).abs() < 0.01,
        "the shift is no part of the row's size: {row:?}"
    );
    assert!((bottom(&item) - 42.0).abs() < 0.01, "{item:?}");
}

#[test]
fn a_row_still_grows_to_an_item_taller_than_its_others() {
    let (row, _, item) = laid_out("height: 40px;", "");
    assert!((row.size.height - 42.0).abs() < 0.01, "{row:?}");
    assert!((bottom(&item) - 40.0).abs() < 0.01, "{item:?}");
}
