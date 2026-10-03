//! A cell's specified width is its column's width.
//!
//! CSS 2.2 17.5.2.2 (automatic table layout): "If the specified 'width' (W)
//! of the cell is greater than MCW, W is the minimum cell width", and every
//! browser reads a fixed cell width as the column's width outright - a
//! `width: 50px` cell of prose stays 50px wide and wraps (every mail's
//! `<td width>` relies on that). The column measurement
//! (`calculate_column_widths_auto_with_width` in `layout/src/solver3/fc.rs`)
//! read only the laid-out content extent, so an empty `width: 100px` cell
//! was as wide as its padding, and a cell of prose took its whole line. The
//! table's own intrinsic sizes (`sizing.rs`) had the opposite bug: there a
//! cell's width REPLACED its content, so a 1px-wide cell of one long word
//! made a 1px table.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{laid_out_page as laid_out, near_tenth as near, page, rect};

#[test]
fn an_empty_cell_with_a_width_is_that_wide_plus_its_padding_and_border() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 10px; border: 5px solid black }",
        "<table id=\"t\"><tr>\
         <td id=\"a\" style=\"width: 100px\"></td><td id=\"b\" style=\"width: 30px\"></td>\
         </tr></table>",
    ));
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    let t = rect(&lw, "t");
    assert!(
        near(a.size.width, 130.0),
        "100px wide + 2 x 10px padding + 2 x 5px border: {}",
        a.size.width
    );
    assert!(
        near(b.size.width, 60.0),
        "30px wide + 30px of padding and border: {}",
        b.size.width
    );
    assert!(
        near(t.size.width, 190.0),
        "the auto-width table is its two columns wide: {}",
        t.size.width
    );
    assert!(
        near(b.origin.x, a.origin.x + 130.0),
        "the second column starts where the first ends: {} vs {}",
        b.origin.x,
        a.origin.x
    );
}

#[test]
fn a_cell_with_a_width_wraps_its_text_inside_that_width() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 0 }",
        "<table><tr><td id=\"one\">a</td></tr></table>\
         <table><tr><td id=\"narrow\" style=\"width: 60px\">\
         a b c d e f g h i j k l m n o p q r s t u v w x y z</td></tr></table>",
    ));
    let one = rect(&lw, "one");
    let narrow = rect(&lw, "narrow");
    assert!(
        near(narrow.size.width, 60.0),
        "the cell keeps its 60px instead of taking its whole line: {}",
        narrow.size.width
    );
    assert!(
        narrow.size.height > 2.0 * one.size.height,
        "the alphabet wraps onto several lines inside 60px: {} vs one line {}",
        narrow.size.height,
        one.size.height
    );
}

#[test]
fn a_cell_narrower_than_its_longest_word_grows_to_the_word() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 0 }",
        "<table id=\"t\"><tr><td id=\"word\" style=\"width: 1px\">unbreakable</td></tr></table>\
         <table><tr><td id=\"free\">unbreakable</td></tr></table>",
    ));
    let word = rect(&lw, "word");
    let free = rect(&lw, "free");
    let t = rect(&lw, "t");
    assert!(
        near(word.size.width, free.size.width),
        "W below the min-content width: the word's width wins ({} vs {})",
        word.size.width,
        free.size.width
    );
    assert!(
        t.size.width + 0.11 >= word.size.width,
        "the table holds its cell (the intrinsic size kept the word): table {} cell {}",
        t.size.width,
        word.size.width
    );
}
