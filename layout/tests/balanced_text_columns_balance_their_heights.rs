//! A paragraph's balanced text columns are of equal height, not of an
//! equal number of lines.
//!
//! CSS Multi-column 1, 7.1: `column-fill: balance` makes the columns as
//! equal in height as possible. The text columns of a paragraph gave each
//! column the same number of lines, so tall lines piled into one column.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::mono_lines::page_glyphs;

#[test]
fn two_tall_lines_balance_four_short_ones() {
    // Two lines 60px tall (an inline-block each) and four 20px lines: the
    // first column holds the tall two (120px), the second the four short
    // ones (80px). Three and three would make 140 and 60.
    let tall = "<span style=\"display: inline-block; width: 10px; height: 60px\"></span>";
    let pens = page_glyphs(
        ".p { width: 400px; column-count: 2; column-gap: 20px }",
        &format!(
            "<p class=\"p\">{tall}aa<br>{tall}bb<br>cc<br>dd<br>ee<br>ff</p>"
        ),
    );
    let first = pens.iter().filter(|&&(x, _)| x < 200.0).count();
    let second = pens.iter().filter(|&&(x, _)| x >= 200.0).count();
    assert_eq!(
        (first, second),
        (4, 8),
        "the tall lines in the first column, the short ones in the second: {pens:?}"
    );
}
