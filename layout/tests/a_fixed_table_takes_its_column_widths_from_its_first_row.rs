//! A fixed table takes its column widths from its first row.
//!
//! CSS 2.2 17.5.2.1 (fixed table layout), as browsers apply it:
//!
//! - it applies to a table whose `width` is not `auto` - an auto-width table
//!   with `table-layout: fixed` is laid out automatically;
//! - a `<col>` with a width sets its column; otherwise the first row's cell
//!   with a width does - its width plus its horizontal padding and border
//!   (a percentage of the columns' share of the table, then the same);
//! - the other columns share what is left, equally, and a cell is exactly as
//!   wide as its column, padding or not;
//! - and in automatic layout a `<col>` width is the column's width too
//!   (17.5.2.2 step 2).
//!
//! The fixed algorithm ignored `<col>`, gave a cell its width without its
//! padding and border, and ran even for auto-width tables.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{
    laid_out_page as laid_out, near_tenth as near, page, pixels_differing, rect,
};

fn green_square(size: u32) -> String {
    page(
        "",
        &format!("<div style=\"width: {size}px; height: {size}px; background: green\"></div>"),
    )
}

#[test]
fn a_first_row_percentage_is_the_content_width_and_the_rest_is_shared() {
    // WPT fixed-table-layout-025: the 50% cell plus its 2 x 25px padding is
    // the whole 100px table; the red cells get nothing.
    let test = page(
        "table { border-spacing: 0; table-layout: fixed; width: 100px } \
         td { padding: 50px 25px; background: red } td.g { background: green; width: 50% }",
        "<table><tr><td></td><td class=\"g\"></td><td></td></tr></table>",
    );
    assert_eq!(pixels_differing(&test, &green_square(100)), 0);
}

#[test]
fn a_first_row_cells_border_counts_in_its_column() {
    // WPT fixed-table-layout-026.
    let test = page(
        "table { border-spacing: 0; table-layout: fixed; width: 100px } \
         td { padding: 50px 0; background: red } \
         td.g { background: green; border-left: 25px solid green; \
         border-right: 25px solid green; width: 50% }",
        "<table><tr><td></td><td class=\"g\"></td><td></td></tr></table>",
    );
    assert_eq!(pixels_differing(&test, &green_square(100)), 0);
}

#[test]
fn a_col_width_sets_its_column_and_the_cells_follow() {
    let lw = laid_out(&page(
        "table { border-spacing: 0; table-layout: fixed; width: 300px } \
         td { padding: 0 }",
        "<table><col style=\"width: 200px\"/><col/>\
         <tr><td id=\"a\">a</td><td id=\"b\">b</td></tr></table>",
    ));
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        near(a.size.width, 200.0),
        "the col's 200px: {}",
        a.size.width
    );
    assert!(near(b.size.width, 100.0), "the rest: {}", b.size.width);
}

#[test]
fn an_auto_width_fixed_table_is_laid_out_automatically_with_its_col_widths() {
    // WPT table-visual-layout-026a: two 50px columns of cells with 50px of
    // right padding each: 100px square under the green.
    let lw = laid_out(&page(
        "table { border-spacing: 0; table-layout: fixed } col { width: 50px } \
         td { height: 50px; padding: 0 50px 0 0 }",
        "<table id=\"t\"><col/><col/><tr><td id=\"a\"></td><td></td></tr>\
         <tr><td></td><td></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    assert!(near(t.size.width, 100.0), "table width: {}", t.size.width);
    assert!(
        near(t.size.height, 100.0),
        "table height: {}",
        t.size.height
    );
    assert!(
        near(a.size.width, 50.0),
        "a column of 50px: {}",
        a.size.width
    );
}
