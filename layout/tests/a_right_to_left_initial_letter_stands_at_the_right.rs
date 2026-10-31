//! A right-to-left paragraph's initial letter stands at its start, the
//! right, and the lines beside it make room for it there.
//!
//! The initial letter takes its room at the inline start of the first
//! lines (CSS Inline 3, 7). The room was always taken at x 0, the left:
//! a right-to-left paragraph's first lines kept their right edge and lost
//! room at the end of the line instead.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::mono_lines::{page_glyphs, LINE};

/// Plenty of short words.
const WORDS: &str = "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz";

#[test]
fn a_right_to_left_paragraphs_initial_letter_takes_its_room_at_the_right() {
    let pens = page_glyphs(
        ".p { width: 400px; direction: rtl; initial-letter: 3 }",
        &format!("<p class=\"p\">{WORDS}</p>"),
    );
    let first_lines: Vec<(f32, f32)> = pens
        .iter()
        .copied()
        .filter(|&(_, top)| top > LINE - 0.01 && top < 3.0 * LINE)
        .collect();
    assert!(!first_lines.is_empty(), "the paragraph has lines beside the letter: {pens:?}");
    let right = first_lines.iter().map(|&(x, _)| x + 10.0).fold(f32::MIN, f32::max);
    assert!(
        right < 400.0 - 20.0,
        "the lines beside a right-to-left initial letter end left of it, not at the box's \
         right edge ({right}): {first_lines:?}"
    );
}
