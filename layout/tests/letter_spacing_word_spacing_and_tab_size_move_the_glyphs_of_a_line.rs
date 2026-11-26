//! `letter-spacing`, `word-spacing` and `tab-size` move the glyphs of a line: the spacing is added
//! after every glyph (every space), the tab ends at the next multiple of its size.
//!
//! The face is `Azul Mock Mono`: every glyph is 10 px wide at the 20 px these pages are set in, so
//! the pen of each glyph is a number a test can write down. Every page is laid out in a window
//! wide enough to hold the line.

use crate::text_rig::{block, laid_out, lay_out, painted, span, window, MONO};

/// The pen x of every glyph of `text` set in the mono face under `css`.
fn pens(css: &str, text: &str) -> Vec<f32> {
    let (_lw, dl) = laid_out(MONO, css, text, 800.0);
    painted(&dl).iter().map(|g| g.x).collect()
}

fn assert_pens(css: &str, text: &str, expected: &[f32]) {
    let got = pens(css, text);
    assert_eq!(got.len(), expected.len(), "`{css}` {text:?}: {got:?}");
    for (pen, want) in got.iter().zip(expected) {
        assert!((pen - want).abs() < 0.01, "`{css}` {text:?}: the pens are {got:?}, not {expected:?}");
    }
}

#[test]
fn the_premise_is_a_pen_of_ten_px_a_glyph() {
    assert_pens("", "abc", &[0.0, 10.0, 20.0]);
    assert_pens("letter-spacing: normal; word-spacing: normal", "abc", &[0.0, 10.0, 20.0]);
}

#[test]
fn letter_spacing_is_added_after_every_glyph() {
    assert_pens("letter-spacing: 4px", "abc", &[0.0, 14.0, 28.0]);
    assert_pens("letter-spacing: 0.1em", "abc", &[0.0, 12.0, 24.0]);
    assert_pens("letter-spacing: -2px", "abc", &[0.0, 8.0, 16.0]);
}

#[test]
fn word_spacing_is_added_after_every_space() {
    // a, the space, b: the space is one glyph, the spacing comes after it
    assert_pens("word-spacing: 6px", "a b", &[0.0, 10.0, 26.0]);
    assert_pens("word-spacing: 1em", "a b", &[0.0, 10.0, 40.0]);
    assert_pens("word-spacing: -5px", "a b", &[0.0, 10.0, 15.0]);
    // the spaces a `white-space: pre` text keeps each have it
    assert_pens("word-spacing: 6px; white-space: pre", "a  b", &[0.0, 10.0, 26.0, 42.0]);
}

#[test]
fn the_two_spacings_add_up() {
    assert_pens("letter-spacing: 4px; word-spacing: 6px", "a b", &[0.0, 14.0, 34.0]);
}

/// Both are inherited: the text of a span takes the spacing of the block.
#[test]
fn the_spacings_are_inherited_and_a_child_can_reset_them() {
    let mut lw = window();
    let dom = block(
        MONO,
        "letter-spacing: 4px; word-spacing: 6px",
        vec![span("", "ab"), span("letter-spacing: normal; word-spacing: normal", "cd")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    let pens: Vec<f32> = painted(&dl).iter().map(|g| g.x).collect();
    // `ab` at the block's spacing, then `cd` right after it on the pen the first span ended on
    assert_eq!(pens.len(), 4);
    assert!((pens[1] - pens[0] - 14.0).abs() < 0.01, "{pens:?}");
    assert!((pens[3] - pens[2] - 10.0).abs() < 0.01, "{pens:?}");
}

/// A tab ends at the next multiple of `tab-size` space widths from the start of the line: the
/// space of the mono face is 10 px wide, `a` is one glyph, so `b` follows at the tab stop.
#[test]
fn a_tab_ends_at_the_next_multiple_of_the_tab_size() {
    // the initial value is 8
    assert_pens("white-space: pre", "a\tb", &[0.0, 80.0]);
    assert_pens("white-space: pre; tab-size: 4", "a\tb", &[0.0, 40.0]);
    assert_pens("white-space: pre; tab-size: 2", "a\tb", &[0.0, 20.0]);
    // a pen that is ON a stop moves to the next one: `aaaa` ends at 40, the tab runs to 80
    assert_pens("white-space: pre; tab-size: 4", "aaaa\tb", &[0.0, 10.0, 20.0, 30.0, 80.0]);
    assert_pens("white-space: pre; tab-size: 4", "aaa\tb", &[0.0, 10.0, 20.0, 40.0]);
}

#[test]
fn a_tab_size_of_zero_draws_no_tab() {
    assert_pens("white-space: pre; tab-size: 0", "a\tb", &[0.0, 10.0]);
}

/// `tab-size` as a length is the width of the tab stops in px, not a number of spaces.
#[test]
fn a_tab_size_that_is_a_length_is_the_distance_between_the_stops() {
    assert_pens("white-space: pre; tab-size: 30px", "a\tb", &[0.0, 30.0]);
    assert_pens("white-space: pre; tab-size: 50px", "a\tb", &[0.0, 50.0]);
}

/// The tab size is inherited.
#[test]
fn the_tab_size_is_inherited() {
    let mut lw = window();
    let dom = block(MONO, "white-space: pre; tab-size: 4", vec![span("", "a\tb")]);
    let dl = lay_out(&mut lw, dom, 800.0);
    let pens: Vec<f32> = painted(&dl).iter().map(|g| g.x).collect();
    assert_eq!(pens.len(), 2);
    assert!((pens[1] - 40.0).abs() < 0.01, "{pens:?}");
}
