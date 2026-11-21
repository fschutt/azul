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

/// `body(0) > div(1) > row(2) > [row(3) > .. > row(depth + 1) > 2 x item, item(depth + 4)]`:
/// `depth` flex rows nested in each other under a block. The outermost is 40 px high and holds
/// a 20 x 10 item behind the rows nested in it; the innermost holds two 30 x 10 items
/// (`depth + 2`, `depth + 3`). Nothing shrinks or grows: every nested row is as wide as the two
/// items, 60 px.
fn nested_flex_rows(depth: usize) -> Dom {
    let item = |width: usize| {
        Dom::create_div().with_css(&format!("width: {width}px; height: 10px; flex-shrink: 0;"))
    };
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(item(30))
        .with_child(item(30));
    for _ in 2..depth {
        row = Dom::create_div().with_css("display: flex; flex-direction: row;").with_child(row);
    }
    let outermost = Dom::create_div()
        .with_css("display: flex; flex-direction: row; height: 40px;")
        .with_child(row)
        .with_child(item(20));
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_div().with_css("width: 600px;").with_child(outermost))
}

/// A3-solver-17. A flex row asks each row among its items how wide it is twice per run (its
/// flex basis, its min-content width; a third time for its content contribution), and such a
/// width-only answer runs the asked row's algorithm over its own items. The answers were kept
/// only while the row's taffy cache held a measurement - which a width-only run never stores,
/// and inside a width-only run of its parent the row is asked nothing else. On a first layout
/// every one of those asks ran the row again: twice per run of the parent, to the power of the
/// nesting (2, 8, 16, 32, 64 runs for the rows 2 to 6 of six). A bridge now finds the answers
/// it gave itself while the cache was still empty: two runs per row, one per question.
///
/// Mutation: `width_only_answer_kept` refusing every node whose taffy cache is empty (no
/// `width_only_fresh`) runs the rows of the deep page 122 times for 10 of the shallow one.
#[test]
fn a_nested_flex_row_answers_each_width_question_of_a_first_layout_once() {
    use crate::solver3::taffy_bridge::WIDTH_ONLY_ROW_RUNS;
    let runs = |depth: usize| {
        let mut lw = window();
        WIDTH_ONLY_ROW_RUNS.with(|n| n.set(0));
        lay_out(&mut lw, nested_flex_rows(depth));
        WIDTH_ONLY_ROW_RUNS.with(std::cell::Cell::get)
    };
    // Two nested rows asked for their width, and five.
    let (shallow, deep) = (runs(3), runs(6));
    assert!(
        shallow >= 2,
        "harness: each of the two rows nested in the outermost was asked how wide it is \
         ({shallow} runs)"
    );
    assert!(
        deep < shallow * 4,
        "five nested rows ran their algorithm {deep} times to say how wide they are, two \
         nested rows {shallow} times: every run of a row runs the rows in it again"
    );
}

/// A3-solver-17, the result: the widths the nested rows answer are the widths they are laid out
/// with. The item behind them in the outermost row starts where they end, 60 px in, and the
/// innermost row's items stand side by side at its start.
#[test]
fn nested_flex_rows_are_as_wide_as_their_innermost_items() {
    let depth = 4;
    let mut lw = window();
    lay_out(&mut lw, nested_flex_rows(depth));
    let outermost = position_of(&lw, 2);
    let (first, second) = (position_of(&lw, depth + 2), position_of(&lw, depth + 3));
    let behind = position_of(&lw, depth + 4);
    assert!(
        (first.x - outermost.x).abs() < 0.01 && (second.x - first.x - 30.0).abs() < 0.01,
        "the innermost items stand at {} and {}, the outermost row starts at {}",
        first.x,
        second.x,
        outermost.x
    );
    assert!(
        (behind.x - outermost.x - 60.0).abs() < 0.01,
        "the item behind the nested rows stands {} px into the outermost row, the rows are 60 \
         px wide",
        behind.x - outermost.x
    );
    assert!(
        (first.y - outermost.y).abs() < 0.01 && (behind.y - outermost.y).abs() < 0.01,
        "the items stand at the top of the outermost row: {}, {} and {}",
        first.y,
        behind.y,
        outermost.y
    );
}
