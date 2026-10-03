//! A table cell holding only inline-level boxes - no loose text - is one
//! inline formatting context, and its `text-align` places them (MAILENG6
//! item 5, MAILHTML's "for TABLES" note 1).
//!
//! CSS 2.2 9.4.2: a block container that contains no block-level boxes
//! establishes an inline formatting context - with or without loose text.
//! `fc::cell_is_inline_formatting_context` also required a TEXT child, so
//! `<td><span>$10.00</span></td>` (the receipt's price column) and Postmark's
//! button cell (`<td align="center"><a style="display: inline-block">`, when
//! written without whitespace around the link) took the block branch: the
//! box became a block-level box at the cell's left edge and the cell's
//! `text-align` never applied. Chrome 154 centres / right-aligns them.
//!
//! Box sizes are fixed, so the numbers do not depend on the machine's fonts.
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

/// `<table>` of one 200px cell (no spacing, no padding) holding `content`,
/// aligned by `align`.
fn cell(align: &str, content: &str) -> String {
    format!(
        "<table style=\"border-spacing: 0\"><tr><td style=\"width: 200px; padding: 0; \
         text-align: {align}\">{content}</td></tr></table>"
    )
}

/// A 50 x 10 inline-block.
const BOX: &str = "<span id=\"b\" style=\"display: inline-block; width: 50px; height: 10px; \
                   background: rgb(0, 128, 0)\"></span>";

#[test]
fn an_inline_block_alone_in_a_centered_cell_is_centered() {
    let lw = body(&cell("center", BOX));
    let b = rect(&lw, "b");
    assert!(
        near(b.origin.x, 75.0, 0.5),
        "the 50px box sits in the middle of the 200px cell (x 75, as in Chrome): {b:?}"
    );
}

#[test]
fn an_inline_block_alone_in_a_right_aligned_cell_is_at_the_right() {
    let lw = body(&cell("right", BOX));
    let b = rect(&lw, "b");
    assert!(
        near(b.origin.x, 150.0, 0.5),
        "the 50px box ends at the cell's right edge (x 150, as in Chrome): {b:?}"
    );
}

#[test]
fn an_inline_block_inside_a_link_in_a_centered_cell_is_centered() {
    // Postmark's button: `<td align="center"><a class="button" style="display:
    // inline-block">`, here without the whitespace around the link.
    let lw = body(&cell(
        "center",
        "<a id=\"b\" href=\"#\" style=\"display: inline-block; width: 50px; height: 10px; \
         background: rgb(0, 128, 0)\"></a>",
    ));
    let b = rect(&lw, "b");
    assert!(
        near(b.origin.x, 75.0, 0.5),
        "the link's box is centred in the cell: {b:?}"
    );
}
