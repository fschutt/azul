//! An inline-block inside a word is never lost to hyphenation.
//!
//! The line breaker hands the hyphenator the word's whole unbreakable unit,
//! which can hold an atomic inline between its letters. The hyphenated line
//! part and the remainder were rebuilt from the unit's text clusters alone,
//! while the whole unit was taken off the line, so the inline-block between
//! them vanished without a trace.

use crate::table_markup::{body, rects_of_color};

#[test]
fn an_inline_block_inside_a_word_is_never_lost_to_hyphenation() {
    let mark = "<span style=\"display: inline-block; width: 6px; height: 6px; background-color: \
                rgb(255, 0, 0)\"></span>";
    let lw = body(&format!(
        "<p lang=\"en\" style=\"width: 60px; margin: 0; font-size: 20px; line-height: 30px; \
         hyphens: auto\">Adver{mark}tisement</p>"
    ));
    assert_eq!(
        rects_of_color(&lw, (255, 0, 0)).len(),
        1,
        "the inline-block between \"Adver\" and \"tisement\" is painted once"
    );
}
