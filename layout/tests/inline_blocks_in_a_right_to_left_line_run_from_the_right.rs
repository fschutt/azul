//! Atomic inlines (inline-blocks, images) in a right-to-left paragraph are
//! reordered with it: the first one sits at the RIGHT (UAX #9 rule L2; an
//! atomic inline is a neutral, U+FFFC, and takes the direction of the text
//! around it or, at a line's edge, of the paragraph - rules N1 / N2).
//!
//! `reorder_logical_items` orders the level runs visually but leaves a run's
//! items in logical order, and `apply_l2_visual_reversal` reversed only the
//! CLUSTERS of an RTL run - an object broke the run and was never moved. Two
//! inline-blocks in `<td dir="rtl">` therefore stayed left-to-right (only
//! right-aligned): the Cerberus hybrid template's reversed row (`dir="rtl"`
//! around two `display: inline-block` columns, the image column on the right)
//! came out unreversed, ~45 boxes 220 / 440px off (scripts/refci/
//! mail_boxes.py, MAILREF8 group A residual).
//!
//! Chrome 154 (scripts/refci probe): in a 500px rtl box, a 100px and a 200px
//! inline-block sit at x 400 and 200 (azul 200 and 300); in an rtl cell the
//! same. Font-independent (font-size 0, fixed boxes). Not compiled by the
//! author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

const BOXES: &str = "<div id=\"a\" style=\"display: inline-block; width: 100px; height: 10px\">\
                     </div><div id=\"b\" style=\"display: inline-block; width: 200px; height: \
                     10px\"></div>";

#[test]
fn two_inline_blocks_in_an_rtl_box_run_from_the_right() {
    let lw = body(&format!(
        "<div style=\"width: 500px; font-size: 0; direction: rtl\">{BOXES}</div>"
    ));
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        near(a.origin.x, 400.0, 0.5) && near(b.origin.x, 200.0, 0.5),
        "the first box at the right edge, the second left of it (Chrome 400 / 200): a {a:?}, \
         b {b:?}"
    );
}

#[test]
fn two_inline_blocks_in_an_rtl_cell_run_from_the_right() {
    let lw = body(&format!(
        "<table width=\"500\" cellpadding=\"0\" cellspacing=\"0\"><tr><td dir=\"rtl\" \
         style=\"font-size: 0\">{BOXES}</td></tr></table>"
    ));
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        near(a.origin.x, 400.0, 0.5) && near(b.origin.x, 200.0, 0.5),
        "Cerberus's reversed row: the first column on the right (Chrome 400 / 200): a {a:?}, \
         b {b:?}"
    );
}
