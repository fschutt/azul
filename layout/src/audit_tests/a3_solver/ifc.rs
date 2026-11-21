//! A3-solver guards, part `ifc` (helpers: `super::{window, lay_out, lay_out_styled, position_of}`).

#[allow(unused_imports)]
use super::*;

/// [`layer_of_absolute_blocks`] set in the built-in mock font (`text3::mock_fonts`: registered
/// in every font manager, reachable by its family name), the same tree: a positioned block of
/// `k` absolutely positioned blocks with a text. Every label is a line of real glyphs, so its
/// lines are stored (`fc::layout_ifc` stores no layout that placed nothing) and a later visit
/// at the same width reuses them.
fn layer_of_mock_font_labels(k: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        layer = layer.with_child(Dom::create_div_with_text(format!("label {i}")).with_css(
            &format!(
                "position: absolute; left: {}px; top: {}px;",
                (i % 50) * 10,
                (i / 50) * 9
            ),
        ));
    }
    Dom::create_body()
        .with_css("margin: 0px; font-family: \"Azul Mock Mono\"; font-size: 20px;")
        .with_child(layer)
}

/// A3-solver-21. An absolutely positioned block with a text is laid out with its parent's flow
/// (`cache::process_out_of_flow_children`) and again by the positioning pass
/// (`positioning::position_out_of_flow_elements`), at the same shrink-to-fit width: the second
/// visit finds its lines stored and reuses them (`fc::layout_ifc`, Phase 2d). Stored lines of
/// plain text are kept as their dense view alone, and the reuse exit expanded that view into
/// one item per cluster (`CachedInlineLayout::materialized`) to read the lines' extent and
/// last baseline off the items: once per label per pass. It reads them from the measures kept
/// when the lines were stored.
///
/// Mutation: `let kept = None;` at the reuse exit (every reuse expands, as before) expands
/// once per label.
#[test]
fn a_reused_label_reports_its_lines_without_expanding_them() {
    use crate::solver3::fc::{IFC_REUSE_EXITS, IFC_REUSE_EXPANSIONS};
    if crate::solver3::layout_tree::dense_text_mode() != 1 {
        // `AZ_DENSE_TEXT=0` and `verify` keep the items: there is nothing to expand.
        return;
    }
    let k = 200;
    let mut lw = window();
    IFC_REUSE_EXITS.with(|n| n.set(0));
    IFC_REUSE_EXPANSIONS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_mock_font_labels(k));
    let reuses = IFC_REUSE_EXITS.with(std::cell::Cell::get);
    let expansions = IFC_REUSE_EXPANSIONS.with(std::cell::Cell::get);
    assert!(
        reuses >= k,
        "harness: each of the {k} labels was laid out twice and reused its lines ({reuses} reuses)"
    );
    assert!(
        expansions <= 4,
        "{reuses} reuses of stored lines expanded their dense view {expansions} times"
    );
}

/// A3-solver-23. Collecting a text for its lines runs the white-space processing on it
/// (`fc::white_space_runs`): a split at forced breaks, the segment break transform, the
/// collapsing and the trimmed ends, about a dozen allocations for the ten characters of a
/// label - which come out as they went in. A text the processing leaves as it is
/// (`fc::is_collapsed_plain_text`) becomes its run directly.
///
/// Mutation: `is_collapsed_plain_text` answering `false` (no fast path) sends every label
/// through the general processing.
#[test]
fn plain_labels_skip_the_white_space_processing() {
    use crate::solver3::fc::{WHITE_SPACE_GENERAL_RUNS, WHITE_SPACE_RUNS};
    let mut lw = window();
    WHITE_SPACE_RUNS.with(|n| n.set(0));
    WHITE_SPACE_GENERAL_RUNS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_spans(300));
    let asked = WHITE_SPACE_RUNS.with(std::cell::Cell::get);
    let general = WHITE_SPACE_GENERAL_RUNS.with(std::cell::Cell::get);
    assert!(
        asked >= 300,
        "harness: the text of each of the 300 labels was collected ({asked})"
    );
    assert!(
        general <= 4,
        "{general} of {asked} plain label texts went through the whole white-space processing"
    );
}

/// A3-solver-24. The reconcile keeps a clean node by reading it out of the old tree
/// (`LayoutTree::get_full_node`: a copy of its hot, warm and cold parts and of its child list)
/// and handing that copy to the builder, which CLONED it again
/// (`LayoutTreeBuilder::clone_node_from_old`) and dropped the first copy with its boxes and
/// lists. The copy is read without the child list and moved into the builder
/// (`adopt_node_from_old`).
///
/// Mutation: `clone_node_from_old(&old_full_node, ..)` in `cache::reconcile_node` clones once
/// per kept node.
#[test]
fn the_reconcile_moves_a_kept_node_into_the_new_tree_without_cloning_it_again() {
    use crate::solver3::layout_tree::BUILDER_NODE_CLONES;
    let mut lw = window();
    lay_out(&mut lw, layer_of_absolute_spans(100));
    BUILDER_NODE_CLONES.with(|n| n.set(0));
    // The same page again: every box is kept.
    lay_out(&mut lw, layer_of_absolute_spans(100));
    let kept = lw.layout_cache.last_reconcile_reused;
    assert!(kept >= 200, "harness: the second layout kept the boxes of the first ({kept})");
    assert_eq!(
        BUILDER_NODE_CLONES.with(std::cell::Cell::get),
        0,
        "{kept} kept boxes were cloned a second time on their way into the new tree"
    );
}

/// A table of `rows` x `columns` cells with a text, built of blocks with `display: table`,
/// `table-row` and `table-cell`, in the built-in mock font: every cell is an inline formatting
/// context of one line of real glyphs.
fn table_of_mock_font_cells(rows: usize, columns: usize) -> Dom {
    let mut table = Dom::create_div().with_css("display: table;");
    for r in 0..rows {
        let mut row = Dom::create_div().with_css("display: table-row;");
        for c in 0..columns {
            row = row.with_child(
                Dom::create_div_with_text(format!("cell {r} {c}")).with_css("display: table-cell;"),
            );
        }
        table = table.with_child(row);
    }
    Dom::create_body()
        .with_css("margin: 0px; font-family: \"Azul Mock Mono\"; font-size: 10px;")
        .with_child(table)
}

/// A3-solver-22. The auto table layout measures every cell at its min- and at its max-content
/// width (`fc::measure_cell_widths`): two layouts of the cell's lines, each stored
/// (`fc::layout_ifc`, the measure slots) - and each store grouped the glyphs into paint runs
/// and compacted them, for lines that are dropped from the cell's final slot right after the
/// measure and never painted from the measure slot. A measure's paint runs are built when
/// they are first read (`CachedInlineLayout::new_measure_with_constraints`): here never.
///
/// Mutation: `CachedInlineLayout::new_with_constraints` for a measure at the store of
/// `layout_ifc` builds the runs of every stored measure.
#[test]
fn a_measure_of_a_table_cell_builds_no_paint_runs() {
    use crate::solver3::{
        fc::{IFC_MEASURE_STORES, IFC_MEASURE_STORE_RUN_BUILDS},
        layout_tree::GLYPH_RUNS_BUILT_ON_READ,
    };
    if crate::solver3::layout_tree::dense_text_mode() != 1 {
        // `AZ_DENSE_TEXT=0` and `verify` build the runs when a layout is stored.
        return;
    }
    let (rows, columns) = (20, 4);
    let mut lw = window();
    IFC_MEASURE_STORES.with(|n| n.set(0));
    IFC_MEASURE_STORE_RUN_BUILDS.with(|n| n.set(0));
    GLYPH_RUNS_BUILT_ON_READ.with(|n| n.set(0));
    lay_out(&mut lw, table_of_mock_font_cells(rows, columns));
    let measures = IFC_MEASURE_STORES.with(std::cell::Cell::get);
    let built_at_store = IFC_MEASURE_STORE_RUN_BUILDS.with(std::cell::Cell::get);
    let built_on_read = GLYPH_RUNS_BUILT_ON_READ.with(std::cell::Cell::get);
    assert!(
        measures >= rows * columns,
        "harness: each of the {} cells was measured and its lines stored ({measures} stores)",
        rows * columns
    );
    assert!(
        built_at_store <= 4,
        "{built_at_store} of {measures} stored measures built paint runs when they were stored"
    );
    assert!(
        built_on_read <= 4,
        "{built_on_read} of {measures} stored measures were painted: the cells' final layouts \
         were expected to be their definite ones"
    );
}
