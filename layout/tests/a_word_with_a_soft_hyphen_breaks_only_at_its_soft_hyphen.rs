//! A word with a soft hyphen breaks only at its soft hyphen.
//!
//! CSS Text: with `hyphens: auto` the dictionary's breaks of a word are
//! ignored when the word holds a soft hyphen (U+00AD), in favour of the soft
//! hyphen; with `hyphens: manual` a word breaks only at its soft hyphens, and
//! with `hyphens: none` not even there. The word is everything between two
//! break opportunities, so both parts of "hyphen\u{AD}ation" and
//! "counter\u{AD}revolutionary" are one word, though the dictionary breaks
//! "hy-phen" and "rev-o-lu-tion-ary" on their own; so is
//! "fore\u{AD}word—revolutionary", whose dash is no break opportunity.
//!
//! Each word is laid out between "xxx " and " yy" in every box from the
//! width of "xxx" to the width of the whole line, a pixel apart, wrapped and
//! balanced: every line that ends inside the word ends at its soft hyphen.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{Hyphens, TextWrap, UnifiedConstraints};
use hyphenation::Language;

use crate::calmet_lines::{calmet_ui, is_inserted_hyphen, lay_out, lines};

/// The text in front of the word, so the word starts mid-line.
const LEAD: &str = "xxx ";

/// The text after the word, so the word does not end the paragraph.
const TAIL: &str = " yy";

/// Words with a soft hyphen whose parts the dictionary breaks on their own.
const WORDS: [&str; 3] = [
    "hyphen\u{AD}ation",
    "counter\u{AD}revolutionary",
    "fore\u{AD}word\u{2014}revolutionary",
];

/// The number of letters in `text`.
fn letters_in(text: &str) -> usize {
    text.chars().filter(|ch| ch.is_alphabetic()).count()
}

/// The number of letters before each soft hyphen of `word`.
fn soft_hyphens(word: &str) -> Vec<usize> {
    word.match_indices('\u{AD}')
        .map(|(byte, _)| letters_in(&word[..byte]))
        .collect()
}

/// Sets the paragraph's `hyphens`, in American English.
fn hyphenate(paragraph: &mut UnifiedConstraints, hyphens: Hyphens) {
    paragraph.hyphenation = hyphens;
    paragraph.hyphenation_language = Some(Language::EnglishUS);
}

/// How wide `text` is on one line.
fn natural_width(font: &FontRef, text: &str) -> f32 {
    let layout = lay_out(font, text, 10_000.0, |_| {}, |_| {});
    let one_line = lines(&layout);
    one_line.first().map_or(0.0, |line| line.end)
}

/// A line that ends inside the word.
struct BrokenInside {
    /// The width of the box it was laid out in.
    width: f32,
    /// How its paragraph was broken into lines.
    wrap: TextWrap,
    /// The letters of the word before the line's end.
    letters: usize,
    /// The line's text.
    text: String,
}

/// Every line that ends inside `word` when it is laid out between [`LEAD`]
/// and [`TAIL`] with `hyphens`, wrapped and balanced, in every box from the
/// width of the lead's letters to the width of the whole text.
fn lines_broken_inside(font: &FontRef, word: &str, hyphens: Hyphens) -> Vec<BrokenInside> {
    let text = format!("{LEAD}{word}{TAIL}");
    let lead_letters = letters_in(LEAD);
    let word_letters = letters_in(word);
    let narrowest = natural_width(font, LEAD.trim_end()).floor();
    let widest = natural_width(font, &text).ceil();
    assert!(
        widest > narrowest,
        "harness: {text:?} is wider than its lead ({widest} px, {narrowest} px)"
    );
    let steps = (widest - narrowest) as usize;
    let mut found = Vec::new();
    for wrap in [TextWrap::Wrap, TextWrap::Balance] {
        for step in 0..=steps {
            let width = narrowest + step as f32;
            let layout = lay_out(
                font,
                &text,
                width,
                |_| {},
                |paragraph| {
                    hyphenate(paragraph, hyphens);
                    paragraph.text_wrap = wrap;
                },
            );
            let mut letters = 0;
            for line in lines(&layout) {
                for cluster in &line.clusters {
                    if !is_inserted_hyphen(cluster) {
                        letters += letters_in(cluster.text());
                    }
                }
                let in_word = letters.saturating_sub(lead_letters);
                if letters > lead_letters && in_word < word_letters {
                    found.push(BrokenInside {
                        width,
                        wrap,
                        letters: in_word,
                        text: line.text(),
                    });
                }
            }
        }
    }
    found
}

/// Asserts that `word` laid out with `hyphens` breaks only at its soft
/// hyphens, and returns the lines that end inside it.
fn assert_breaks_only_at_its_soft_hyphens(
    font: &FontRef,
    word: &str,
    hyphens: Hyphens,
) -> Vec<BrokenInside> {
    let at = soft_hyphens(word);
    let found = lines_broken_inside(font, word, hyphens);
    for line in &found {
        let BrokenInside {
            width,
            wrap,
            letters,
            text,
        } = line;
        assert!(
            at.contains(letters),
            "hyphens: {hyphens:?}, {wrap:?} in a {width} px box: {word:?} is broken after \
             {letters} letters (the line reads {text:?}), not at its soft hyphen (after {at:?} \
             letters)"
        );
    }
    found
}

/// Asserts that a wrapped paragraph breaks `word` at its soft hyphen in some
/// box.
fn assert_broken_at_its_soft_hyphen_somewhere(
    word: &str,
    hyphens: Hyphens,
    found: &[BrokenInside],
) {
    assert!(
        found.iter().any(|line| line.wrap == TextWrap::Wrap),
        "hyphens: {hyphens:?}: {word:?} is broken at its soft hyphen in no box from the lead's \
         width to the whole line's"
    );
}

#[test]
fn a_word_with_a_soft_hyphen_breaks_only_at_its_soft_hyphen() {
    let font = calmet_ui();
    for word in WORDS {
        let found = assert_breaks_only_at_its_soft_hyphens(&font, word, Hyphens::Auto);
        assert_broken_at_its_soft_hyphen_somewhere(word, Hyphens::Auto, &found);
    }
}

#[test]
fn hyphens_manual_breaks_a_word_only_at_its_soft_hyphen() {
    let font = calmet_ui();
    for word in WORDS {
        let found = assert_breaks_only_at_its_soft_hyphens(&font, word, Hyphens::Manual);
        assert_broken_at_its_soft_hyphen_somewhere(word, Hyphens::Manual, &found);
    }
}

#[test]
fn hyphens_none_breaks_a_word_not_even_at_its_soft_hyphen() {
    let font = calmet_ui();
    for word in WORDS {
        let found = lines_broken_inside(&font, word, Hyphens::None);
        if let Some(BrokenInside {
            width,
            wrap,
            letters,
            text,
        }) = found.first()
        {
            panic!(
                "hyphens: none, {wrap:?} in a {width} px box: {word:?} is broken after {letters} \
                 letters (the line reads {text:?}), and with hyphens: none it breaks nowhere"
            );
        }
    }
}
