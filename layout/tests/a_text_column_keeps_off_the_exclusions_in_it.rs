//! Each column of a multi-column paragraph keeps off the exclusions that lie
//! in it, and only those.
//!
//! The exclusions of an inline formatting context are placed in its own
//! space; a paragraph with `column-count` lays each column's lines out from
//! the column's start. The columns read the exclusions as if each column
//! started where the paragraph does, so a float at the paragraph's left
//! edge cut the left edge of every column, and one in the second column cut
//! nothing.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use azul_layout::text3::cache::{Rect, ShapeBoundary, UnifiedConstraints};

use crate::mono_lines::{box_of_width, glyph_boxes, lay_out, GlyphBox};

/// Plenty of short words.
const WORDS: &str = "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz \
                     aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz";

/// Two 190px columns 20px apart, 100px tall, around the rectangle `x0`..`x1`
/// from the top down 60px.
fn columns_around(x0: f32, x1: f32) -> Vec<GlyphBox> {
    let mut paragraph = UnifiedConstraints {
        columns: 2,
        column_gap: 20.0,
        available_height: Some(100.0),
        ..box_of_width(400.0)
    };
    paragraph.shape_exclusions = vec![ShapeBoundary::Rectangle(Rect {
        x: x0,
        y: 0.0,
        width: x1 - x0,
        height: 60.0,
    })
    .into()];
    let glyphs = glyph_boxes(&lay_out(WORDS, &paragraph));
    assert!(
        glyphs.iter().any(|g| g.left >= 210.0),
        "the text runs into the second column: {glyphs:?}"
    );
    glyphs
}

#[test]
fn an_exclusion_in_the_first_column_leaves_the_second_alone() {
    let glyphs = columns_around(0.0, 50.0);
    for g in glyphs.iter().filter(|g| g.top < 60.0) {
        assert!(
            g.left >= 50.0 - 0.01,
            "{:?} on line {} at {}..{} lies on the exclusion",
            g.text,
            g.line,
            g.left,
            g.right
        );
    }
    let second_column_start = glyphs
        .iter()
        .filter(|g| g.left >= 200.0 && g.top < 60.0)
        .map(|g| g.left)
        .fold(f32::MAX, f32::min);
    assert!(
        (second_column_start - 210.0).abs() < 0.01,
        "the second column's lines start at its own edge, 210, not {second_column_start}"
    );
}

#[test]
fn an_exclusion_in_the_second_column_cuts_its_lines() {
    let glyphs = columns_around(250.0, 300.0);
    for g in glyphs.iter().filter(|g| g.top < 60.0) {
        assert!(
            g.right <= 250.0 + 0.01 || g.left >= 300.0 - 0.01,
            "{:?} on line {} at {}..{} lies on the exclusion at 250..300",
            g.text,
            g.line,
            g.left,
            g.right
        );
    }
}
