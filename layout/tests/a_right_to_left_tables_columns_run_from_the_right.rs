//! A right-to-left table's columns run from the right.
//!
//! CSS 2.2 17.5 / 9.10: the `direction` of a table decides the order of
//! its columns - in a `rtl` table the first column is the rightmost. The
//! cerberus newsletters swap a two-column row with `<td dir="rtl">` around
//! a table whose cells read `dir="ltr"` (an image right, its text left);
//! azul placed the columns left to right whatever the direction
//! (mail_boxes, 2026-10-02: cerberus-responsive's reversed row, x +-387).
//!
//! Chrome 154's numbers (probe). Fixed-size boxes only.

use crate::table_markup::{body, near, rect};

#[test]
fn an_rtl_tables_first_column_is_on_the_right() {
    let lw = body(
        "<table id=\"t\" dir=\"rtl\" style=\"border-spacing: 4px\"><tr>\
         <td id=\"a\" style=\"padding: 0\">\
         <span style=\"display: inline-block; width: 100px; height: 10px\"></span></td>\
         <td id=\"b\" style=\"padding: 0\">\
         <span style=\"display: inline-block; width: 50px; height: 10px\"></span></td></tr>\
         <tr><td id=\"s\" colspan=\"2\" style=\"padding: 0\">\
         <span style=\"display: inline-block; width: 20px; height: 10px\"></span></td></tr>\
         </table>",
    );
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    let s = rect(&lw, "s");
    assert!(
        near(b.origin.x - t.origin.x, 4.0, 0.5) && near(a.origin.x - t.origin.x, 58.0, 0.5),
        "the 100px first column is on the right (x 58), the 50px second one on the left \
         (x 4): a {a:?}, b {b:?}"
    );
    assert!(
        near(s.origin.x - t.origin.x, 4.0, 0.5) && near(s.size.width, 154.0, 0.5),
        "a cell spanning both columns covers both: {s:?}"
    );
}

#[test]
fn a_tables_direction_is_inherited_from_its_cell() {
    let lw = body(
        "<table style=\"border-collapse: collapse; width: 300px\"><tr>\
         <td dir=\"rtl\" style=\"padding: 0\">\
         <table id=\"t\" style=\"width: 100%; border-collapse: collapse\"><tr>\
         <th id=\"a\" style=\"width: 33.33%; padding: 0\"></th>\
         <th id=\"b\" style=\"width: 66.67%; padding: 0\"></th>\
         </tr></table></td></tr></table>",
    );
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        near(a.origin.x - t.origin.x, 200.0, 0.5) && near(b.origin.x, t.origin.x, 0.5),
        "the inner table is rtl (from its cell's dir): its first, 100px column on the right: \
         a {a:?}, b {b:?} in {t:?}"
    );
}
