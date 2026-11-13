//! A caption's own caption-side places it.
//!
//! CSS 2.2 17.4.1: `caption-side` applies to (and is inherited by)
//! table-caption elements - a caption's computed value decides where it
//! goes. `fc::layout_table_fc` read the TABLE's value only, so `<caption
//! style="caption-side: bottom">` stayed on top.
//!
//! Chrome 154's numbers (probe). Fixed-size boxes only.

use crate::table_markup::{body, near, rect};

#[test]
fn a_caption_with_caption_side_bottom_goes_below_the_rows() {
    let lw = body(
        "<table id=\"t\" style=\"border-spacing: 0\">\
         <caption id=\"cap\" style=\"caption-side: bottom\">\
         <span style=\"display: inline-block; width: 80px; height: 20px\"></span></caption>\
         <tr><td id=\"a\" style=\"padding: 0; height: 30px\"></td></tr></table>",
    );
    let t = rect(&lw, "t");
    let a = rect(&lw, "a");
    let cap = rect(&lw, "cap");
    assert!(
        near(a.origin.y, t.origin.y, 0.5),
        "the row is at the table's top: {a:?} in {t:?}"
    );
    assert!(
        near(cap.origin.y, a.origin.y + 30.0, 0.5),
        "the caption is below the 30px row: {cap:?}"
    );
}

#[test]
fn a_tables_caption_side_reaches_its_caption() {
    let lw = body(
        "<table id=\"t\" style=\"border-spacing: 0; caption-side: bottom\">\
         <caption id=\"cap\">\
         <span style=\"display: inline-block; width: 80px; height: 20px\"></span></caption>\
         <tr><td id=\"a\" style=\"padding: 0; height: 30px\"></td></tr></table>",
    );
    let a = rect(&lw, "a");
    let cap = rect(&lw, "cap");
    assert!(
        near(cap.origin.y, a.origin.y + 30.0, 0.5),
        "caption-side is inherited: the table's bottom puts its caption below the row: {cap:?}"
    );
}
