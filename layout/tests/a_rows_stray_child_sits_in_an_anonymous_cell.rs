//! A row's stray child sits in an anonymous cell.
//!
//! CSS 2.2 17.2.1 ("anonymous table objects"), rule 2: a child of a
//! `table-row` that is not a `table-cell` is wrapped, with its consecutive
//! non-cell siblings, in an anonymous `table-cell`; a child of a `table` (or
//! a row group) that is not a proper table child (a row) is wrapped in an
//! anonymous `table-row`, and inside it the same cell rule applies.
//!
//! The mailgun templates (tests/mail_corpus/mailgun/*.html) center their
//! content with `<td class="container" style="display: block; width: 600px;
//! max-width: 600px; margin: 0 auto">` between two empty cells. The
//! reconciled layout tree (`cache::reconcile_recursive`) built no anonymous
//! table objects, so the block `td` was a direct child of the row, the table
//! grid (`fc::analyze_table_row`) skipped it as "not a cell", and the whole
//! mail body had no box: 19 of 25 boxes missing in mailgun/action, 21 of 27
//! in alert, 40 of 46 in billing (mail_boxes, 2026-10-02).
//!
//! Numbers from Chrome 154 for the same markup. Sizes come from fixed-size
//! boxes, so they do not depend on the machine's fonts.

use crate::table_markup::{body, near, rect, right};

fn span(w: u32) -> String {
    format!("<span style=\"display: inline-block; width: {w}px; height: 10px\"></span>")
}

#[test]
fn a_display_block_td_in_a_row_gets_an_anonymous_cell() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 400px; border-spacing: 0\"><tr>\
         <td id=\"a\" style=\"padding: 0\">{0}</td>\
         <td id=\"b\" style=\"display: block; padding: 0\">\
         <div id=\"inner\" style=\"width: 120px; height: 30px\"></div></td>\
         <td id=\"c\" style=\"padding: 0\">{0}</td></tr></table>",
        span(50)
    ));
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    let inner = rect(&lw, "inner");
    let c = rect(&lw, "c");
    // Columns 50 / 120 / 50 of max-content; the 180px the 400px table has
    // beyond them go to the three auto columns by their max-content (CSS
    // Tables 3 3.9.3): 90.9 / 218.2 / 90.9 - Chrome's numbers.
    assert!(
        near(a.size.width, 90.9, 0.5) && near(c.size.width, 90.9, 0.5),
        "the two real cells share the table with the anonymous one: a {a:?}, c {c:?}"
    );
    assert!(
        near(b.origin.x, right(&a), 0.5) && near(b.size.width, 218.2, 0.5),
        "the block td fills the anonymous cell between them: {b:?} (a {a:?})"
    );
    assert!(
        near(c.origin.x, right(&b), 0.5),
        "the third cell follows the anonymous one: c {c:?}, b {b:?}"
    );
    assert!(
        near(inner.origin.x, b.origin.x, 0.5) && near(inner.size.height, 30.0, 0.5),
        "the block td's content is laid out inside it: {inner:?} in {b:?}"
    );
    assert!(
        near(t.size.height, 30.0, 0.5) && near(a.size.height, 30.0, 0.5),
        "the row is as tall as the anonymous cell's content, every cell fills it: \
         table {t:?}, a {a:?}"
    );
}

#[test]
fn cells_straight_under_a_table_share_one_anonymous_row() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 0\">\
         <td id=\"a\" style=\"padding: 0\">{}</td><td id=\"c\" style=\"padding: 0\">{}</td>\
         </table>",
        span(100),
        span(80)
    ));
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let c = rect(&lw, "c");
    assert!(
        near(c.origin.y, a.origin.y, 0.5) && near(c.origin.x, right(&a), 0.5),
        "consecutive cells under a table sit side by side in ONE anonymous row: a {a:?}, c {c:?}"
    );
    assert!(
        near(t.size.width, 180.0, 0.5),
        "the table is as wide as the row: {t:?}"
    );
}

#[test]
fn a_centered_block_td_between_two_empty_cells_is_centered() {
    // The mailgun "container": UA cell padding 1px, spacing 2px.
    let lw = body(
        "<div style=\"width: 760px\"><table id=\"t\" style=\"width: 100%\"><tr>\
         <td></td>\
         <td id=\"b\" style=\"display: block; width: 600px; max-width: 600px; margin: 0 auto\">\
         <div id=\"inner\" style=\"height: 40px\"></div></td>\
         <td></td></tr></table></div>",
    );
    let t = rect(&lw, "t");
    let b = rect(&lw, "b");
    assert!(
        near(b.size.width, 602.0, 0.5),
        "the block td is 600px plus its 1px padding on each side: {b:?}"
    );
    let left_gap = b.origin.x - t.origin.x;
    let right_gap = right(&t) - right(&b);
    assert!(
        (left_gap - right_gap).abs() < 3.0,
        "margin: 0 auto centers it in the table (Chrome: 79 / 79): {left_gap} / {right_gap}"
    );
    assert!(
        t.size.height >= 44.0,
        "the table holds the 40px content, its padding and the spacing: {t:?}"
    );
}
