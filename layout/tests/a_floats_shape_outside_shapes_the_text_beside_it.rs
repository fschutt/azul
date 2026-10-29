//! A float's `shape-outside` is the float area the text beside it wraps
//! around; on any other box it does nothing.
//!
//! CSS Shapes 1, 2.1: `shape-outside` defines the float area of a float, the
//! area line boxes are shortened around, clipped to the float's margin box;
//! a left float still lets lines flow on its right only. The engine read
//! `shape-outside` from the box holding the text instead - a paragraph cut
//! its own lines with it - and wrapped the text beside every float around
//! the float's border box, so its shape and its margins were ignored.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::mono_lines::{page_glyphs, LINE};

/// Plenty of short words.
pub const WORDS: &str = "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx \
                         yy zz aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv \
                         ww xx yy zz";

/// How far right of a circle around (60, 60) of `radius` a glyph in the line
/// box at `top` has to start.
pub fn right_of_circle(radius: f32, top: f32) -> Option<f32> {
    let dy = 60.0_f32.clamp(top, top + LINE) - 60.0;
    (dy.abs() < radius).then(|| 60.0 + (radius * radius - dy * dy).max(0.0).sqrt())
}

#[test]
fn text_beside_a_round_float_follows_its_circle() {
    let pens = page_glyphs(
        ".c { width: 400px } .fl { float: left; width: 120px; height: 120px; shape-outside: \
         circle(60px at 60px 60px) }",
        &format!("<div class=\"c\"><div class=\"fl\"></div>{WORDS}</div>"),
    );
    assert!(!pens.is_empty(), "the text is laid out");
    for &(left, top) in &pens {
        if let Some(edge) = right_of_circle(60.0, top) {
            assert!(
                left >= edge - 0.01,
                "a glyph at {left} on the line at {top} enters the circle (it reaches {edge})"
            );
        }
    }
    // Next to the circle's narrow bottom the lines start further left than
    // the 120px square would let them.
    assert!(
        pens.iter().any(|&(left, top)| top < 120.0 && left < 110.0),
        "a line beside the circle's lower part starts left of the square's edge: {pens:?}"
    );
}

#[test]
fn text_beside_a_float_keeps_off_its_margin() {
    let pens = page_glyphs(
        ".c { width: 400px } .fl { float: left; width: 100px; height: 60px; margin-right: 30px }",
        &format!("<div class=\"c\"><div class=\"fl\"></div>{WORDS}</div>"),
    );
    for &(left, top) in pens.iter().filter(|(_, top)| *top < 60.0) {
        assert!(
            left >= 130.0 - 0.01,
            "a glyph at {left} on the line at {top} lies in the float's margin (it ends at 130)"
        );
    }
}

#[test]
fn shape_outside_on_a_box_that_is_no_float_does_nothing() {
    let cut = page_glyphs(
        ".p { width: 400px; shape-outside: inset(0px 150px 40px 150px) }",
        &format!("<p class=\"p\">{WORDS}</p>"),
    );
    let plain = page_glyphs(".p { width: 400px }", &format!("<p class=\"p\">{WORDS}</p>"));
    assert_eq!(cut, plain, "the paragraph's own lines are not cut by its shape-outside");
    assert!(
        cut.iter()
            .any(|&(left, top)| top < 40.0 && (150.0..250.0).contains(&left)),
        "its text runs through where the inset would be"
    );
}
