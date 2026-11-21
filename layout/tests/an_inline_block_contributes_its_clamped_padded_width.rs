//! An inline-block contributes its clamped, padded width to its line.
//!
//! CSS Sizing 3 5.1 / 5.2 and CSS 2.2 10.3.9: an atomic inline's
//! contribution to the min- and max-content of the inline formatting
//! context it sits in is its OUTER size (padding, border and margins
//! included) under that constraint - its min-content under the min-content
//! constraint, its max-content under the max-content one - clamped by its
//! `max-width` and floored by its `min-width` (which wins). The same clamp
//! applies to a block child's contribution to its parent.
//!
//! The cerberus "hybrid" newsletter (tests/mail_corpus/cerberus) lays two
//! or three `display: inline-block; max-width: 220px / 440px` columns into
//! a `width: 100%` table cell; azul measured each column at its UNCLAMPED
//! max-content and, under the min-content constraint too, at its
//! max-content, so the table's minimum was ~1061px: the 680px newsletter
//! came out 1081px wide (mail_boxes, 2026-10-02). Found by TABLES (wave 5);
//! every number below is Chrome 154's for the same page (probe), and every
//! size comes from fixed-size boxes, so none depends on the fonts.

use crate::table_markup::{body, near, rect, words};

#[test]
fn a_max_width_caps_an_inline_blocks_max_content_contribution() {
    let lw = body(&format!(
        "<div style=\"width: 600px\"><div id=\"w\" style=\"display: inline-block\">\
         <div style=\"display: inline-block; max-width: 200px\">{}</div></div></div>",
        words(10, 50)
    ));
    let w = rect(&lw, "w");
    assert!(
        near(w.size.width, 200.0, 0.5),
        "the shrink-to-fit wrapper is as wide as its child's max-width: {}",
        w.size.width
    );
}

#[test]
fn a_cell_of_an_inline_block_shrinks_to_the_inline_blocks_min_content() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 1px; border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <div style=\"display: inline-block\">{}</div></td></tr></table>",
        words(5, 40)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 40.0, 0.5),
        "under the min-content constraint the inline-block contributes its min-content \
         (one 40px box), not its max-content: {}",
        t.size.width
    );
}

#[test]
fn min_width_floors_an_inline_blocks_min_content_contribution() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 1px; border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <div style=\"display: inline-block; min-width: 120px\">{}</div></td></tr></table>",
        words(5, 40)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 120.0, 0.5),
        "min-width: 120px floors the inline-block's min-content contribution: {}",
        t.size.width
    );
}

#[test]
fn padding_and_margins_of_inline_blocks_count_in_their_line() {
    // `font-size: 0`: the space between the boxes has no width.
    let one = "<span style=\"display: inline-block; width: 100px; height: 10px; \
               padding: 0 10px; margin: 0 5px\"></span>";
    let lw = body(&format!(
        "<div style=\"width: 600px\"><div id=\"w\" style=\"display: inline-block; \
         font-size: 0\">{one} {one}</div></div>"
    ));
    let w = rect(&lw, "w");
    assert!(
        near(w.size.width, 260.0, 0.5),
        "two boxes of 100px + 2 x 10px padding + 2 x 5px margin: {}",
        w.size.width
    );
}

#[test]
fn a_max_width_caps_a_block_childs_max_content_contribution() {
    let lw = body(&format!(
        "<div style=\"width: 700px\"><div id=\"w\" style=\"display: inline-block\">\
         <div style=\"max-width: 300px\">{}</div></div></div>",
        words(10, 50)
    ));
    let w = rect(&lw, "w");
    assert!(
        near(w.size.width, 300.0, 0.5),
        "the shrink-to-fit wrapper is as wide as its block child's max-width: {}",
        w.size.width
    );
}

#[test]
fn hybrid_columns_fit_a_full_width_table_and_wrap() {
    let lw = body(&format!(
        "<div style=\"width: 660px\"><table id=\"t\" style=\"width: 100%; border-spacing: 0\">\
         <tr><td style=\"padding: 0\">\
         <div style=\"display: inline-block; max-width: 220px; min-width: 160px; width: 100%\">\
         {}</div> \
         <div style=\"display: inline-block; max-width: 440px; min-width: 280px\">{}</div>\
         </td></tr></table></div>",
        words(2, 50),
        words(20, 50)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 660.0, 0.5),
        "the columns' minimum is the wider column's min-width (280px), so the table keeps \
         its 100%: {}",
        t.size.width
    );
    assert!(
        t.size.height > 35.0,
        "220 + a space + 440 > 660: the second column wraps under the first (its three \
         10px lines below the first's one): table height {}",
        t.size.height
    );
}
