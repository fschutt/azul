//! A spanning cell's width is spread over the columns it spans.
//!
//! CSS 2.2 17.5.2.2 (and CSS Tables 3 3.8): a cell spanning several columns
//! contributes its min- and max-content AND its own `width` to them - the
//! columns grow until together (with the spacing between them) they are as
//! wide as the cell, the auto columns first, in proportion to their
//! max-content; a column with a fixed width keeps it while an auto column
//! can take the rest.
//!
//! In the replay onto TABLE-A's column model (table_width), a cell's
//! `width` was read for one-column cells only: a `colspan="2"` header with
//! `width: 200px` above two 20px cells left the table 40px wide (WPT
//! colspan-004 needs the same rule).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{laid_out_page as laid_out, near_tenth as near, page, rect};

#[test]
fn a_wide_spanning_header_widens_the_auto_columns_below_it_evenly() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 0 } td div { width: 20px; height: 10px }",
        "<table id=\"t\"><tr><td colspan=\"2\" style=\"width: 200px\"></td></tr>\
         <tr><td id=\"a\"><div></div></td><td id=\"b\"><div></div></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(near(t.size.width, 200.0), "table width: {}", t.size.width);
    assert!(
        near(a.size.width, 100.0) && near(b.size.width, 100.0),
        "the 160px the header adds go to the two 20px columns in proportion: {} / {}",
        a.size.width,
        b.size.width
    );
}

#[test]
fn a_fixed_column_keeps_its_width_and_the_auto_column_takes_the_rest() {
    let lw = laid_out(&page(
        "table { border-spacing: 0 } td { padding: 0 } td div { width: 20px; height: 10px }",
        "<table id=\"t\"><tr><td colspan=\"2\" style=\"width: 200px\"></td></tr>\
         <tr><td id=\"c\" style=\"width: 50px\"></td><td id=\"d\"><div></div></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let c = rect(&lw, "c");
    let d = rect(&lw, "d");
    assert!(near(t.size.width, 200.0), "table width: {}", t.size.width);
    assert!(near(c.size.width, 50.0), "the fixed column: {}", c.size.width);
    assert!(near(d.size.width, 150.0), "the auto column: {}", d.size.width);
}
