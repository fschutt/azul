//! A3-solver guards, part `taffy` (helpers: `super::{window, lay_out, lay_out_styled, position_of}`).

#[allow(unused_imports)]
use super::*;

/// A3-solver-18. A taffy bridge is made for every flex or grid container a block, an inline or
/// the positioning pass lays out, and each bridge made its own style memos: a cell per node of
/// the WHOLE document and one per box of the layout tree, initialised and dropped per bridge -
/// bridges x document, a square for a canvas whose every node holds a flex row. The memo is the
/// pass's (`solver3::PassMemo::taffy`): its cells are made once, by the first bridge.
///
/// Mutation: cells made per bridge (`TaffyBridge::new` sizing a memo of its own) grow 16x here:
/// four times the bridges, each with four times the cells.
#[test]
fn the_cells_of_the_taffy_style_memo_grow_with_the_document_not_with_its_bridges() {
    use crate::solver3::taffy_bridge::{BRIDGES_MADE, STYLE_CELLS_MADE};
    const ITEMS: usize = 2;
    let cells = |k: usize| {
        let mut lw = window();
        BRIDGES_MADE.with(|n| n.set(0));
        STYLE_CELLS_MADE.with(|n| n.set(0));
        lay_out(&mut lw, canvas_of_flex_nodes(k, ITEMS));
        (
            BRIDGES_MADE.with(std::cell::Cell::get),
            STYLE_CELLS_MADE.with(std::cell::Cell::get),
        )
    };
    let ((small_bridges, small), (large_bridges, large)) = (cells(40), cells(160));
    // body, canvas, and per node: the box, its row, `ITEMS` items with a text each.
    let dom_nodes = 2 + 40 * (2 + 2 * ITEMS);
    assert!(
        small_bridges >= 40 && large_bridges >= 160 && small >= dom_nodes,
        "harness: every flex row got a bridge ({small_bridges}, {large_bridges}) and the pass a \
         cell per DOM node ({small} for {dom_nodes} nodes)"
    );
    assert!(
        large < small * 6,
        "four times the flex rows made {large} style memo cells for {small}: a memo per bridge"
    );
}
