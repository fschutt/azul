//! A block of inline content is as tall as its LINE BOXES (CSS 2.2 10.6.3),
//! and every line box holds the block's strut (10.8.1: a zero-width inline
//! box with the block's font and line-height) - so a line of text in a
//! smaller font than its block is as tall as the block's own line.
//!
//! Azul measured an inline formatting context by the bounds of its items
//! (text3 `perform_fragment_layout`): the line box `position_one_line`
//! computes with the strut decided where the baseline sits, but the IFC's
//! height was only as tall as the glyphs. A `<td>` of `<span style="font-size:
//! 13px">` in a 16px document was 31px for Chrome's 34 (04_receipt: every
//! row 3px short, the totals line 17px high), a `<p>` of 10px text 12px for
//! Chrome's 18 (scripts/refci/mail_boxes.py, MAILREF8 group E).
//!
//! Not changed here (a deliberate deviation kept from before, LAYOUT7's
//! "seen broken"): a line holding only atomic inlines (an icon, an
//! inline-block) is still measured by its items - Chrome adds the strut's
//! descent below them (a 10px inline-block makes an 18px line); that moves
//! every icon button of the apps and needs a look pass of its own.
//!
//! Chrome 154 (16px Arial; scripts/refci probe): ref 18; small 18 (azul 11);
//! two lines 36 (azul 29); `line-height: 30px` block of a 12px-line span 30
//! (azul 12); the cell 34 = 18 + 2 x 8 (azul 31). Font-independent: compared
//! with a line of the block's own font. Not compiled by the author (house
//! rule); RED before the fix.

use crate::table_markup::{body, near, rect};

const REF: &str = "<div id=\"ref\" style=\"font-size: 16px\">x</div>";

#[test]
fn a_line_of_smaller_text_is_as_tall_as_the_blocks_own_line() {
    let lw = body(&format!(
        "{REF}<div id=\"small\" style=\"font-size: 16px\"><span style=\"font-size: 10px\">small \
         text</span></div>"
    ));
    let reference = rect(&lw, "ref");
    let small = rect(&lw, "small");
    assert!(
        near(small.size.height, reference.size.height, 0.5),
        "the line box holds the block's strut: as tall as a line of the block's own font \
         (Chrome 18 / 18): small {small:?}, ref {reference:?}"
    );
}

#[test]
fn every_line_box_counts_the_last_one_too() {
    let lw = body(&format!(
        "{REF}<div id=\"two\" style=\"font-size: 16px\"><span style=\"font-size: 10px\">one\
         </span><br/><span style=\"font-size: 10px\">two</span></div>"
    ));
    let reference = rect(&lw, "ref");
    let two = rect(&lw, "two");
    assert!(
        near(two.size.height, 2.0 * reference.size.height, 0.5),
        "two line boxes of the block's line height (Chrome 36): two {two:?}, ref {reference:?}"
    );
}

#[test]
fn the_blocks_line_height_is_the_minimum_line_box() {
    let lw = body(
        "<div id=\"lh\" style=\"font-size: 16px; line-height: 30px\"><span style=\"font-size: \
         10px; line-height: 12px\">small</span></div>",
    );
    let lh = rect(&lw, "lh");
    assert!(
        near(lh.size.height, 30.0, 0.5),
        "the strut is 30px tall, the span's 12px line sits inside it (Chrome 30): {lh:?}"
    );
}

#[test]
fn a_cell_of_smaller_text_is_as_tall_as_its_own_line_plus_its_padding() {
    let lw = body(&format!(
        "{REF}<table cellpadding=\"8\" cellspacing=\"0\" style=\"font-size: 16px\"><tr>\
         <td id=\"cell\"><span style=\"font-size: 13px\">Date</span></td></tr></table>"
    ));
    let reference = rect(&lw, "ref");
    let cell = rect(&lw, "cell");
    assert!(
        near(cell.size.height, reference.size.height + 16.0, 0.5),
        "the cell's line box is its own 16px line, plus 2 x 8 padding (Chrome 34): cell \
         {cell:?}, ref {reference:?}"
    );
}
