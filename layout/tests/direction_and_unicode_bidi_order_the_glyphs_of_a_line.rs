//! `direction` and `unicode-bidi` decide in which order the glyphs of a line stand and where the
//! line starts: in a right-to-left block the line is aligned to the right edge, Arabic words run
//! from the right in a block of either direction, and `bidi-override` makes the letters run in the
//! direction of the block whatever their own.
//!
//! The faces are `Azul Mock Mono` (10 px a glyph at 20 px; the glyph id of an ASCII character is
//! `code point - 0x1F`) and `Azul Mock Arabic` (every glyph 10 px at 20 px; `tests/fonts/README.md`
//! lists its glyph ids: `beh teh` is the glyphs 8 and 14, `meem lam` the glyphs 20 and 18). The
//! windows are 100 px wide.

use crate::text_rig::{laid_out, painted, ARABIC, MONO};

const WIDTH: f32 = 100.0;

/// The pen of the glyph with the id `id` in the page of `text` set in `family` under `css`.
fn pen_of_id(family: &str, css: &str, text: &str, id: u32) -> f32 {
    let (_lw, dl) = laid_out(family, css, text, WIDTH);
    let found: Vec<f32> = painted(&dl).iter().filter(|g| g.id == id).map(|g| g.x).collect();
    assert_eq!(found.len(), 1, "`{css}` {text:?}: one glyph {id} in {found:?}");
    found[0]
}

/// The pen of the glyph of the ASCII character `ch` in the mono face.
fn pen_of(css: &str, text: &str, ch: char) -> f32 {
    pen_of_id(MONO, css, text, u32::from(ch) - 0x1F)
}

/// The pen after the last glyph of the page: where the line ends.
fn line_end(family: &str, css: &str, text: &str) -> f32 {
    let (_lw, dl) = laid_out(family, css, text, WIDTH);
    painted(&dl).iter().map(|g| g.x).fold(f32::MIN, f32::max) + 10.0
}

// beh, teh, space, meem, lam: two words
const ARABIC_TEXT: &str = "\u{628}\u{62A} \u{645}\u{644}";
const BEH_INIT: u32 = 8;
const TEH_FINA: u32 = 14;
const MEEM_INIT: u32 = 20;
const LAM_FINA: u32 = 18;

#[test]
fn a_left_to_right_block_starts_its_line_at_the_left_and_a_right_to_left_one_at_the_right() {
    for css in ["", "direction: ltr"] {
        assert!((pen_of(css, "abc", 'a') - 0.0).abs() < 0.01, "`{css}`");
    }
    assert!((pen_of("direction: rtl", "abc", 'a') - 70.0).abs() < 0.01);
    assert!(
        (pen_of("direction: rtl; text-align: left", "abc", 'a') - 0.0).abs() < 0.01,
        "`text-align: left` does not follow the direction"
    );
    assert!((pen_of("direction: rtl; text-align: start", "abc", 'a') - 70.0).abs() < 0.01);
    assert!((pen_of("direction: ltr; text-align: end", "abc", 'a') - 70.0).abs() < 0.01);
}

/// The letters of a Latin word keep their order in a right-to-left block.
#[test]
fn latin_letters_keep_their_order_in_a_right_to_left_block() {
    for css in ["direction: rtl", "direction: rtl; unicode-bidi: normal", "direction: rtl; unicode-bidi: embed"] {
        assert!(pen_of(css, "abc", 'a') < pen_of(css, "abc", 'b'), "`{css}`");
        assert!(pen_of(css, "abc", 'b') < pen_of(css, "abc", 'c'), "`{css}`");
    }
}

/// Arabic words run from the right whatever the direction of the block: the first word of the
/// text stands on the right.
#[test]
fn arabic_words_run_from_the_right_in_a_block_of_either_direction() {
    for css in ["", "direction: rtl"] {
        let beh = pen_of_id(ARABIC, css, ARABIC_TEXT, BEH_INIT);
        let teh = pen_of_id(ARABIC, css, ARABIC_TEXT, TEH_FINA);
        let meem = pen_of_id(ARABIC, css, ARABIC_TEXT, MEEM_INIT);
        let lam = pen_of_id(ARABIC, css, ARABIC_TEXT, LAM_FINA);
        assert!(lam < meem && meem < teh && teh < beh, "`{css}`: {lam} {meem} {teh} {beh}");
    }
}

/// Where the Arabic line stands follows the direction of the block: at the left edge in a
/// left-to-right block, ending at the right edge in a right-to-left one.
#[test]
fn the_direction_of_the_block_places_an_arabic_line() {
    let start = |css: &str| pen_of_id(ARABIC, css, ARABIC_TEXT, LAM_FINA);
    assert!((start("") - 0.0).abs() < 0.01);
    assert!((line_end(ARABIC, "direction: rtl", ARABIC_TEXT) - WIDTH).abs() < 0.01);
}

/// `bidi-override` makes the letters run in the direction of the block: reversed in a
/// right-to-left one, in the order they are written in a left-to-right one - also Arabic ones.
#[test]
fn bidi_override_makes_the_letters_run_in_the_direction_of_the_block() {
    let rtl = "direction: rtl; unicode-bidi: bidi-override";
    assert!(pen_of(rtl, "abc", 'c') < pen_of(rtl, "abc", 'b'), "reversed");
    assert!(pen_of(rtl, "abc", 'b') < pen_of(rtl, "abc", 'a'), "reversed");
    assert!((line_end(MONO, rtl, "abc") - WIDTH).abs() < 0.01, "and aligned to the right");

    let ltr = "direction: ltr; unicode-bidi: bidi-override";
    assert!(pen_of(ltr, "abc", 'a') < pen_of(ltr, "abc", 'b'));
    let beh = pen_of_id(ARABIC, ltr, ARABIC_TEXT, BEH_INIT);
    let teh = pen_of_id(ARABIC, ltr, ARABIC_TEXT, TEH_FINA);
    let meem = pen_of_id(ARABIC, ltr, ARABIC_TEXT, MEEM_INIT);
    let lam = pen_of_id(ARABIC, ltr, ARABIC_TEXT, LAM_FINA);
    assert!(beh < teh && teh < meem && meem < lam, "left to right as written: {beh} {teh} {meem} {lam}");
}

#[test]
fn isolate_override_overrides_like_bidi_override() {
    let css = "direction: rtl; unicode-bidi: isolate-override";
    assert!(pen_of(css, "abc", 'c') < pen_of(css, "abc", 'a'));
}

/// `plaintext` takes the base direction of the block from its first strong letter: an Arabic line
/// in a left-to-right block ends at the right edge, a Latin one still starts at the left.
#[test]
fn plaintext_takes_the_direction_of_the_block_from_the_text() {
    let css = "unicode-bidi: plaintext";
    assert!((line_end(ARABIC, css, ARABIC_TEXT) - WIDTH).abs() < 0.01);
    assert!((pen_of(css, "abc", 'a') - 0.0).abs() < 0.01);
    // without it the Arabic line starts at the left, as the block does
    assert!((line_end(ARABIC, "", ARABIC_TEXT) - WIDTH).abs() > 1.0);
}
