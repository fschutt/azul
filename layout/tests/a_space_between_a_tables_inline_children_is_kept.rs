//! A space between a table's inline children is kept.
//!
//! CSS 2.2 17.2.1 rule 1: a whitespace-only text child of a table, row
//! group or row is "irrelevant" - dropped - only when its siblings on both
//! sides (where there are any) are table-internal boxes or captions: the
//! newlines between `<tr>`s and `<td>`s. Between two inline-level children
//! it is a space inside the anonymous cell that wraps them, and takes part
//! in their line like any space. The reconciler dropped EVERY
//! whitespace-only child of a table-structural parent (WPT css-tables
//! whitespace-001 and anonymous-table-ws-001).
//!
//! Fixed-size boxes and glyph counts, no font-dependent numbers.

use crate::table_markup::{body, glyph_runs, rect};

#[test]
fn two_half_width_inline_blocks_and_a_space_wrap_in_an_anonymous_cell() {
    let lw = body(
        "<div id=\"t\" style=\"display: table; width: 300px\">\
         <span style=\"display: inline-block; width: 150px; height: 10px\"></span> \
         <span style=\"display: inline-block; width: 150px; height: 10px\"></span></div>",
    );
    let t = rect(&lw, "t");
    assert!(
        t.size.height > 15.0,
        "150 + a space + 150 does not fit 300px: the second box wraps onto a second line \
         (whitespace-001): table height {}",
        t.size.height
    );
}

#[test]
fn the_space_between_two_spans_in_a_table_is_painted() {
    let lw = body("<div style=\"display: table\"><span>a</span> <span>b</span></div>");
    let glyphs: usize = glyph_runs(&lw).iter().map(Vec::len).sum();
    assert_eq!(
        glyphs, 3,
        "\"a\", the space and \"b\" (anonymous-table-ws-001)"
    );
}
