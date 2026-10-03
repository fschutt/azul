//! A row's height is its tallest cell or its own height.
//!
//! CSS 2.2 17.5.3 (table height algorithms), as browsers apply them:
//!
//! - a row is as tall as the largest of its own `height` and its cells'
//!   heights - each cell's content plus padding and border, or its own
//!   `height` (a content height under `box-sizing: content-box`, the border
//!   box under `border-box`) when that is larger;
//! - a table taller than its rows (its own `height`) gives the extra to its
//!   rows;
//! - the cells with `vertical-align: baseline` line their first lines up on
//!   the row's baseline, whatever their padding or the padding of the block
//!   their text sits in, and the row grows to hold the shifted cells.
//!
//! The row's own `height` was never read, a cell's `height` only for a cell
//! of block content (a cell of text was as tall as its text), a table's
//! `height` left its rows short, and only cells of loose text were baseline
//! aligned - a `<td><div>data</div></td>` stayed at its top.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{
    laid_out_page as laid_out, near_tenth as near, page, pixels_differing, rect,
};

#[test]
fn a_rows_own_height_makes_it_taller_than_its_cells() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 0; width: 20px }",
        "<table><tr id=\"r\" style=\"height: 96px\"><td id=\"c\"></td></tr></table>",
    ));
    let c = rect(&lw, "c");
    assert!(
        near(c.size.height, 96.0),
        "the cell fills the 96px row: {}",
        c.size.height
    );
}

#[test]
fn a_text_cells_height_is_its_content_height_plus_padding_and_border() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 5px; border: 2px solid black }",
        "<table><tr><td id=\"t\" style=\"height: 60px\">text</td></tr></table>\
         <table><tr><td id=\"b\" style=\"height: 60px; box-sizing: border-box\">text</td>\
         </tr></table>",
    ));
    let t = rect(&lw, "t");
    let b = rect(&lw, "b");
    assert!(
        near(t.size.height, 74.0),
        "60px of content, 10px of padding, 4px of border: {}",
        t.size.height
    );
    assert!(
        near(b.size.height, 60.0),
        "a border-box height is the cell's whole height: {}",
        b.size.height
    );
}

#[test]
fn a_tall_table_gives_its_extra_height_to_its_rows() {
    let lw = laid_out(&page(
        "table { border-spacing: 0; height: 200px } td { padding: 0; width: 20px }",
        "<table id=\"t\"><tr><td id=\"a\"><div style=\"height: 20px\"></div></td></tr>\
         <tr><td id=\"b\"><div style=\"height: 20px\"></div></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        near(t.size.height, 200.0),
        "table height: {}",
        t.size.height
    );
    assert!(
        near(a.size.height + b.size.height, 200.0),
        "the two rows share the table's 200px: {} + {}",
        a.size.height,
        b.size.height
    );
    assert!(
        near(a.size.height, b.size.height),
        "equal rows get equal shares: {} vs {}",
        a.size.height,
        b.size.height
    );
}

#[test]
fn baseline_cells_line_up_the_text_of_their_blocks() {
    // WPT table-vertical-align-baseline-001: the cells' own top padding
    // differs; on the row's baseline every "data" sits 40px down.
    let test = page(
        "td { vertical-align: baseline }",
        "<table><tbody><tr>\
         <td style=\"padding-top: 40px\"><div>data</div></td>\
         <td style=\"padding-top: 20px\"><div>data</div></td>\
         <td style=\"padding-top: 0\"><div>data</div></td>\
         </tr></tbody></table>",
    );
    let reference = page(
        "td { padding-top: 0 } td div { padding-top: 40px }",
        "<table><tbody><tr>\
         <td><div>data</div></td><td><div>data</div></td><td><div>data</div></td>\
         </tr></tbody></table>",
    );
    assert_eq!(pixels_differing(&test, &reference), 0);
}
