//! A narrow table wraps its cells to fit.
//!
//! R1_MAIL_RENDER's open items (scripts/R1_MAIL_RENDER_2026_09_30.md, "What
//! is left"):
//!
//! - a cell's MIN-content measurement returned its max-content width: the
//!   `TableCell` arm of `calculate_used_size_for_node` sized an auto-width
//!   cell at its max-content width under a min-content constraint too, so the
//!   text inside never wrapped during the measurement, and no column could
//!   shrink below its longest line - a table of prose in a narrow reading
//!   pane ran past its own width;
//! - the table's own intrinsic sizes (`calculate_table_intrinsic_sizes`)
//!   counted a spanning cell in its FIRST column only, so a table under a
//!   shrink-to-fit parent came out as wide as the spanning cell PLUS every
//!   other column.
//!
//! CSS 2.2 17.5.2.2 (automatic table layout): a column's minimum is the
//! largest minimum content width of its cells; a spanning cell's widths are
//! spread over the columns it spans. CSS Tables 3 3.9.3: between the
//! min-content and the max-content guess, every (auto) column gets its
//! min-content plus a share of the rest in proportion to what its
//! max-content wants beyond its min-content.
//!
//! TABLES (wave 5, DEDUP_WIDGETS_API F23): 407cc8c98 loosened the first
//! test to ">= 2 distinct baselines over BOTH cells", which a wrong split
//! passes (one cell left on one line, the other squeezed onto several). The
//! assertion is per cell again: each cell on exactly two baselines, the two
//! cells on the same two, and the columns where the 3.9.3 split puts them -
//! with each cell's min- and max-content measured in the same page (one-cell
//! tables: `width: 1px` is floored at the cell's min-content, `auto` is its
//! max-content), so no number depends on the machine's fonts. Each cell
//! holds two words: whatever the face, a column between the cell's
//! min-content (the longer word) and its max-content (both words) puts
//! exactly one word on each line. Chrome 154 lays this page out as asserted
//! (Times 16px: columns 131.5 / 82.5; Arial: 129.1 / 84.9).

use crate::table_markup::{glyph_runs, laid_out, node, rect, right};

const CELL_A: &str = "magnificent architecture";
const CELL_B: &str = "quiet harbours";

#[test]
fn prose_cells_wrap_inside_a_220px_table() {
    let lw = laid_out(
        &format!(
            "<html><head></head><body style=\"margin: 0\">\
             <table id=\"t\" style=\"width: 220px\"><tr>\
             <td id=\"a\">{CELL_A}</td><td id=\"b\">{CELL_B}</td></tr></table>\
             <table style=\"width: 1px\"><tr><td id=\"a-min\">{CELL_A}</td></tr></table>\
             <table style=\"width: 1px\"><tr><td id=\"b-min\">{CELL_B}</td></tr></table>\
             <table><tr><td id=\"a-max\">{CELL_A}</td></tr></table>\
             <table><tr><td id=\"b-max\">{CELL_B}</td></tr></table>\
             </body></html>"
        ),
        760.0,
        400.0,
    );
    let t = rect(&lw, "t");
    assert!(
        (t.size.width - 220.0).abs() < 1.0,
        "the table keeps its 220px: {}",
        t.size.width
    );

    // The cells' border-box min- and max-content, each measured alone.
    let min = [rect(&lw, "a-min").size.width, rect(&lw, "b-min").size.width];
    let max = [rect(&lw, "a-max").size.width, rect(&lw, "b-max").size.width];
    // The UA spacing (2px) once per gutter, the outer two included.
    let target = 220.0 - 3.0 * 2.0;
    let (sum_min, sum_max) = (min[0] + min[1], max[0] + max[1]);
    assert!(
        sum_min < target && target < sum_max,
        "the page tests the band between the two guesses: min {min:?}, max {max:?}"
    );
    let share = (target - sum_min) / (sum_max - sum_min);
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    for (cell, i, name) in [(&a, 0, "a"), (&b, 1, "b")] {
        let expected = min[i] + share * (max[i] - min[i]);
        assert!(
            (cell.size.width - expected).abs() < 1.5,
            "cell {name} takes its min-content plus its share of the rest (CSS Tables 3 \
             3.9.3): {} vs {expected} (min {min:?}, max {max:?})",
            cell.size.width
        );
    }

    // Each cell's glyph pens: inside its own box, on exactly two baselines,
    // the same two for both cells.
    let pens: Vec<(f32, f32)> = glyph_runs(&lw)
        .into_iter()
        .flatten()
        .filter(|&(_, y)| y > t.origin.y && y < t.origin.y + t.size.height)
        .collect();
    let lines_in = |cell: &azul_core::geom::LogicalRect, name: &str| {
        let own: Vec<(f32, f32)> = pens
            .iter()
            .copied()
            .filter(|&(x, _)| x >= cell.origin.x && x < right(cell))
            .collect();
        assert!(!own.is_empty(), "cell {name} paints its text");
        let mut lines: Vec<i32> = own.iter().map(|&(_, y)| y.round() as i32).collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    };
    for &(x, _) in &pens {
        assert!(
            (x >= a.origin.x && x < right(&a)) || (x >= b.origin.x && x < right(&b)),
            "every word starts inside its own cell: a pen at x={x}, cells {}..{} and {}..{}",
            a.origin.x,
            right(&a),
            b.origin.x,
            right(&b)
        );
    }
    let a_lines = lines_in(&a, "a");
    let b_lines = lines_in(&b, "b");
    assert_eq!(
        a_lines.len(),
        2,
        "cell a wraps once, one word per line: baselines {a_lines:?}"
    );
    assert_eq!(
        b_lines.len(),
        2,
        "cell b wraps once, one word per line: baselines {b_lines:?}"
    );
    assert_eq!(
        a_lines, b_lines,
        "the two cells of one row share their baselines"
    );
}

#[test]
fn a_spanning_header_widens_the_columns_it_spans_not_only_the_first() {
    let lw = laid_out(
        "<html><head></head><body style=\"margin: 0\">\
         <div id=\"shrink\" style=\"display: inline-block\"><table style=\"border-spacing: 0\">\
         <tr><td colspan=\"2\">a spanning header wider than both columns together</td></tr>\
         <tr><td>left cell</td><td>right cell</td></tr>\
         </table></div></body></html>",
        760.0,
        400.0,
    );
    let runs = glyph_runs(&lw);
    let header = runs
        .iter()
        .find(|r| r.len() == "a spanning header wider than both columns together".len())
        .unwrap_or_else(|| panic!("the header paints: {runs:?}"));
    let header_extent = header.last().map_or(0.0, |p| p.0) - header[0].0;
    let wrapper = lw
        .get_node_size(node(&lw, "shrink"))
        .expect("the wrapper has a size")
        .width;
    // Paddings (UA: 1px per cell side) and the header's last letter fit in
    // 30px; the old sum added a whole second column ("right cell", ~70px).
    assert!(
        wrapper < header_extent + 30.0,
        "the shrink-to-fit wrapper is as wide as the spanning header, not the header \
         plus the second column: {wrapper} vs header {header_extent}"
    );
}
