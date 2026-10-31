//! A paragraph whose columns are full goes on in overflow columns beside
//! them: no line is dropped.
//!
//! CSS Multi-column 1, 8.2: content that does not fit the columns of a
//! multi-column box of a definite height is laid out in overflow columns
//! in the inline direction. The text columns of a paragraph stopped at the
//! last column and dropped the lines left over.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use azul_layout::text3::cache::UnifiedConstraints;

use crate::mono_lines::{box_of_width, glyph_boxes, lay_out};

/// Twelve words of four letters: two per line in a 95px column.
const TWELVE_WORDS: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll";

#[test]
fn the_lines_two_full_columns_leave_go_into_a_third() {
    // Two columns of 95px, 10px apart, two lines tall: four lines fit, the
    // fifth and sixth go on in a third column, at x 210.
    let paragraph = UnifiedConstraints {
        columns: 2,
        column_gap: 10.0,
        available_height: Some(40.0),
        ..box_of_width(200.0)
    };
    let glyphs = glyph_boxes(&lay_out(TWELVE_WORDS, &paragraph));
    assert_eq!(glyphs.len(), 48, "every letter is laid out: {glyphs:?}");
    assert!(
        glyphs.iter().any(|g| (g.left - 210.0).abs() < 0.01),
        "the third column starts at x 210: {glyphs:?}"
    );
    assert!(
        glyphs.iter().all(|g| g.top < 40.0 - 0.01),
        "no line goes below the columns' height: {glyphs:?}"
    );
}
