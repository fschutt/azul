//! A cell's vertical-align counts its last child's bottom margin.
//!
//! CSS 2.2 17.5.3 / 10.6.7: a table cell is a block formatting context
//! root, so the bottom margin of its last in-flow child stays inside it -
//! the cell's content is as tall as that margin's bottom edge, and
//! `vertical-align: middle` / `bottom` place THAT content in the cell. The
//! row height already counted the margin (the cell's laid-out content
//! height); the alignment measured only the children's border boxes, so a
//! `<td><h3 style="margin-top: 0">` in a row it fills exactly (the postmark
//! invoice's item table) was pushed down by half its own bottom margin
//! (mail_boxes, 2026-10-02: postmark/invoice `<h3>` 8px low).
//!
//! Chrome 154's numbers for the same pages (probe). Fixed-size boxes only.

use crate::table_markup::{body, near, rect};

#[test]
fn a_middle_cell_centers_its_content_with_the_last_bottom_margin() {
    let lw = body(
        "<table style=\"border-spacing: 0\"><tr><td id=\"c\" style=\"padding: 0; height: 60px; \
         vertical-align: middle\"><div id=\"d\" style=\"width: 50px; height: 20px; \
         margin-bottom: 20px\"></div></td></tr></table>",
    );
    let c = rect(&lw, "c");
    let d = rect(&lw, "d");
    assert!(
        near(d.origin.y - c.origin.y, 10.0, 0.5),
        "40px of content (20px box + 20px margin) centered in 60px: 10px down, not 20: {d:?} \
         in {c:?}"
    );
}

#[test]
fn a_bottom_cell_puts_the_last_bottom_margin_at_its_bottom() {
    let lw = body(
        "<table style=\"border-spacing: 0\"><tr><td id=\"c\" style=\"padding: 0; height: 60px; \
         vertical-align: bottom\"><div id=\"d\" style=\"width: 50px; height: 20px; \
         margin-bottom: 10px\"></div></td></tr></table>",
    );
    let c = rect(&lw, "c");
    let d = rect(&lw, "d");
    assert!(
        near(d.origin.y - c.origin.y, 30.0, 0.5),
        "the margin's bottom edge sits on the cell's bottom: 60 - 10 - 20 = 30: {d:?} in {c:?}"
    );
}

#[test]
fn a_cell_its_content_fills_is_not_shifted() {
    // The UA's `vertical-align: middle`; the row is 28px, the first cell's
    // content 14px + 14px margin fills it exactly.
    let lw = body(
        "<table style=\"border-spacing: 0\"><tr><td id=\"a\" style=\"padding: 0\">\
         <div id=\"h\" style=\"width: 50px; height: 14px; margin-bottom: 14px\"></div></td>\
         <td style=\"padding: 0\"><div style=\"width: 50px; height: 28px\"></div></td>\
         </tr></table>",
    );
    let a = rect(&lw, "a");
    let h = rect(&lw, "h");
    assert!(
        near(a.size.height, 28.0, 0.5) && near(h.origin.y, a.origin.y, 0.5),
        "content that fills its cell stays at the top: {h:?} in {a:?}"
    );
}
