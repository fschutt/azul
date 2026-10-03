//! An inline-block sits on its line by the baseline of its LAST in-flow line
//! box (CSS 2.2 10.8.1), searched the way Chrome searches it: through its
//! in-flow block children from the last one up, a child whose `overflow` is
//! not `visible` answering with its bottom margin edge, and a TABLE answering
//! nothing (Blink's `LayoutTable::InlineBlockBaseline` is -1, LayoutNG skips
//! tables for the inline-block baseline). With no line box at all the
//! baseline is the inline-block's bottom margin edge.
//!
//! Azul had no baseline for an inline-block holding blocks (`layout_bfc`
//! reported none), so every such box sat on its bottom edge. AzMail's paper
//! is `display: inline-block`; the Cerberus templates open with a clipped
//! preheader (`max-height: 0; overflow: hidden`) and continue in tables, so in
//! Chrome the paper's baseline is its TOP and the paper sits one strut ascent
//! (14px for 16px Arial) below the body's top - azul drew it at the top, and
//! every one of the ~390 boxes of the three Cerberus mails was 14px high
//! (scripts/refci/mail_boxes.py, MAILREF8 group A).
//!
//! Each case puts a zero-height empty inline-block `#ref` on the same line:
//! it has no line box, so its baseline is its bottom (= top) edge, and its
//! top therefore marks the line's baseline - no font metric is asserted.
//! Chrome 154 (16px Arial; scripts/refci probe): case 1 ib 14 / ref 14,
//! case 2 ref 32 (l2 18..36), case 3 ref 30 (oh 0..30), case 4 ref 14
//! (l1 0..18). Not compiled by the author (house rule); cases 1, 2 and 4 are
//! RED before the fix, case 3 pins what already holds.

use crate::table_markup::{body, near, rect};

const REF: &str = "<div id=\"ref\" style=\"display: inline-block; width: 10px; height: 0\"></div>";

#[test]
fn a_clipped_preheader_before_a_table_puts_the_baseline_at_the_inline_blocks_top() {
    let lw = body(&format!(
        "<div id=\"wrap\"><div id=\"ib\" style=\"display: inline-block; width: 300px\">\
         <div id=\"pre\" style=\"max-height: 0; overflow: hidden\">hidden text</div>\
         <table><tr><td style=\"height: 40px\"></td></tr></table></div>{REF}</div>"
    ));
    let ib = rect(&lw, "ib");
    let pre = rect(&lw, "pre");
    let reference = rect(&lw, "ref");
    assert!(
        near(reference.origin.y, ib.origin.y, 0.5),
        "the clipped preheader's bottom margin edge (the inline-block's top) is the baseline, \
         the table after it has none: the line's baseline (#ref) is at the inline-block's top \
         (Chrome 14 / 14): ib {ib:?}, ref {reference:?}"
    );
    assert!(
        ib.origin.y > 5.0,
        "the strut's ascent above that baseline pushes the inline-block down (Chrome 14px for \
         16px Arial): {ib:?}"
    );
    assert!(near(pre.origin.y, ib.origin.y, 0.5), "{pre:?} {ib:?}");
}

#[test]
fn the_baseline_is_the_last_line_not_the_bottom_edge() {
    let lw = body(&format!(
        "<div id=\"wrap\"><div id=\"ib\" style=\"display: inline-block; width: 300px\">\
         <div id=\"l1\">line one</div><div id=\"l2\">line two</div></div>{REF}</div>"
    ));
    let l2 = rect(&lw, "l2");
    let reference = rect(&lw, "ref");
    assert!(
        reference.origin.y > l2.origin.y + 1.0
            && reference.origin.y < l2.origin.y + l2.size.height - 1.0,
        "the baseline is the second line's, inside its line box (Chrome: ref 32, l2 18..36), \
         not the inline-block's bottom edge: l2 {l2:?}, ref {reference:?}"
    );
}

#[test]
fn an_overflow_hidden_child_answers_with_its_bottom_margin_edge() {
    let lw = body(&format!(
        "<div id=\"wrap\"><div id=\"ib\" style=\"display: inline-block; width: 300px\">\
         <div id=\"oh\" style=\"overflow: hidden; height: 30px\">a</div></div>{REF}</div>"
    ));
    let oh = rect(&lw, "oh");
    let reference = rect(&lw, "ref");
    assert!(
        near(reference.origin.y, oh.origin.y + 30.0, 0.5),
        "a clipped child's baseline is its bottom margin edge, not its line's (Chrome: ref 30): \
         oh {oh:?}, ref {reference:?}"
    );
}

#[test]
fn a_table_after_the_last_line_gives_the_inline_block_no_baseline() {
    let lw = body(&format!(
        "<div id=\"wrap\"><div id=\"ib\" style=\"display: inline-block; width: 300px\">\
         <div id=\"l1\">text</div><table><tr><td style=\"height: 40px\">x</td></tr></table>\
         </div>{REF}</div>"
    ));
    let l1 = rect(&lw, "l1");
    let reference = rect(&lw, "ref");
    assert!(
        reference.origin.y > l1.origin.y + 1.0
            && reference.origin.y < l1.origin.y + l1.size.height - 1.0,
        "the table is skipped: the baseline is the line before it (Chrome: ref 14, l1 0..18), \
         not the table's row nor the bottom edge: l1 {l1:?}, ref {reference:?}"
    );
}
