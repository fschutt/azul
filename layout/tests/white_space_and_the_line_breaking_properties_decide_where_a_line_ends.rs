//! `white-space`, `word-break`, `overflow-wrap`, `line-break` and `hyphens` decide what a line
//! holds: where it ends, which breaks it may take inside a word, what becomes of its spaces.
//!
//! The face is `Azul Mock Mono` (10 px a glyph at 20 px) in a window 100 px wide: a line holds
//! ten glyphs. The text of a line is read back from its glyph ids (`code point - 0x1F`), a line
//! as the glyphs on one baseline; the spaces at the end of a line hang and are not told.

use crate::text_rig::{laid_out, line_texts, painted, MONO};

/// The lines `text` breaks into under `css` in a window 100 px wide, the spaces at their ends
/// taken off.
fn broken(css: &str, text: &str) -> Vec<String> {
    let (_lw, dl) = laid_out(MONO, css, text, 100.0);
    line_texts(&dl).into_iter().map(|line| line.trim_end().to_string()).collect()
}

/// The lines as they are, the spaces at their ends kept.
fn broken_keeping_spaces(css: &str, text: &str) -> Vec<String> {
    let (_lw, dl) = laid_out(MONO, css, text, 100.0);
    line_texts(&dl)
}

#[test]
fn the_premise_is_ten_glyphs_to_a_line() {
    assert_eq!(broken("", "aaaa bbbb cccc"), ["aaaa bbbb", "cccc"]);
    assert_eq!(broken("white-space: normal", "aaaa bbbb cccc"), ["aaaa bbbb", "cccc"]);
}

#[test]
fn nowrap_keeps_the_text_on_one_line() {
    assert_eq!(broken("white-space: nowrap", "aaaa bbbb cccc"), ["aaaa bbbb cccc"]);
}

#[test]
fn pre_keeps_the_newlines_and_the_spaces_and_does_not_wrap() {
    assert_eq!(broken("white-space: pre", "aa\nbb"), ["aa", "bb"]);
    assert_eq!(broken("white-space: pre", "a   b"), ["a   b"], "the spaces are kept");
    assert_eq!(broken("white-space: pre", "aaaa bbbb cccc"), ["aaaa bbbb cccc"], "no wrap");
}

#[test]
fn pre_wrap_keeps_the_spaces_and_wraps() {
    assert_eq!(
        broken("white-space: pre-wrap", "a   b cccccccc dddd"),
        ["a   b", "cccccccc", "dddd"]
    );
}

#[test]
fn pre_line_collapses_the_spaces_and_keeps_the_newlines() {
    assert_eq!(broken("white-space: pre-line", "a   b\nc"), ["a b", "c"]);
}

#[test]
fn break_spaces_keeps_the_spaces_at_the_end_of_a_line_and_breaks_after_each() {
    let lines = broken_keeping_spaces("white-space: break-spaces", "a          b");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0].len(), 10, "{lines:?}");
    assert!(lines[1].ends_with('b'), "{lines:?}");
}

/// A word that is longer than the line overflows it, unless a property lets it break.
#[test]
fn a_long_word_overflows_its_line_by_default() {
    assert_eq!(broken("", "aaaaaaaaaaaaaaa"), ["aaaaaaaaaaaaaaa"]);
    assert_eq!(broken("word-break: keep-all", "aaaaaaaaaaaaaaa"), ["aaaaaaaaaaaaaaa"]);
    assert_eq!(broken("overflow-wrap: normal", "aaaaaaaaaaaaaaa"), ["aaaaaaaaaaaaaaa"]);
}

#[test]
fn break_all_breaks_between_any_two_letters() {
    assert_eq!(broken("word-break: break-all", "aaaaaaaaaaaaaaa"), ["aaaaaaaaaa", "aaaaa"]);
    // `break-all` fills the line with the next word instead of moving it down whole
    assert_eq!(broken("word-break: break-all", "aaaa bbbbbbb"), ["aaaa bbbbb", "bb"]);
    assert_eq!(broken("", "aaaa bbbbbbb"), ["aaaa", "bbbbbbb"]);
}

#[test]
fn overflow_wrap_breaks_a_word_that_does_not_fit_a_line_of_its_own() {
    for css in ["overflow-wrap: break-word", "overflow-wrap: anywhere", "word-wrap: break-word"] {
        assert_eq!(broken(css, "aaaaaaaaaaaaaaa"), ["aaaaaaaaaa", "aaaaa"], "`{css}`");
        // a word that fits a line of its own moves down whole
        assert_eq!(broken(css, "aaaa bbbbbbb"), ["aaaa", "bbbbbbb"], "`{css}`");
    }
}

#[test]
fn line_break_anywhere_breaks_between_any_two_letters() {
    assert_eq!(broken("line-break: anywhere", "aaaaaaaaaaaaaaa"), ["aaaaaaaaaa", "aaaaa"]);
    assert_eq!(broken("line-break: strict", "aaaa bbbb cccc"), ["aaaa bbbb", "cccc"]);
    assert_eq!(broken("line-break: loose", "aaaa bbbb cccc"), ["aaaa bbbb", "cccc"]);
}

/// A soft hyphen is a break the text offers: `hyphens: manual` takes it and draws a hyphen at the
/// end of the line, `none` does not.
#[test]
fn a_soft_hyphen_is_a_break_under_manual_and_none_under_none() {
    let word = "aaaaaaa\u{AD}bbbbbbb";
    let manual = broken("hyphens: manual", word);
    assert_eq!(manual.len(), 2, "{manual:?}");
    assert!(manual[0].starts_with("aaaaaaa") && manual[0].ends_with('-'), "{manual:?}");
    assert!(manual[1].ends_with("bbbbbbb"), "{manual:?}");
    assert_eq!(broken("hyphens: none", word).len(), 1);
}

/// A soft hyphen is drawn as nothing and takes no room, at a break too (the face has no glyph for
/// U+00AD: it was drawn as its .notdef box, 10 px wide, inside the word and at the start of the
/// next line).
#[test]
fn a_soft_hyphen_draws_nothing_and_takes_no_room() {
    let word = "aaaaaaa\u{AD}bbbbbbb";
    let b = u32::from('b') - 0x1F;
    let first_b = |css: &str| {
        let (_lw, dl) = laid_out(MONO, css, word, 100.0);
        let glyphs = painted(&dl);
        assert!(glyphs.iter().all(|g| g.id != 0), "`{css}`: no .notdef box in {glyphs:?}");
        glyphs.iter().filter(|g| g.id == b).map(|g| g.x).fold(f32::MAX, f32::min)
    };
    // the word stays whole: the b's follow the seven a's
    assert!((first_b("hyphens: none") - 70.0).abs() < 0.05);
    // the word breaks at the soft hyphen: the b's start the second line
    assert!(first_b("hyphens: manual").abs() < 0.05);
}
