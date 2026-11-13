//! A line broken at a soft hyphen ends in a hyphen.
//!
//! A soft hyphen (U+00AD) is invisible where its word stays whole; where a
//! line breaks at it, a hyphen is shown at the end of the line (CSS Text),
//! with `hyphens: manual` and `auto` alike, and the line fits its box with
//! the hyphen. The dictionary's breaks and the soft hyphen's share the one
//! hyphen the line breakers put at a break inside a word.
//!
//! "hyphen\u{AD}ation" is laid out between "xxx " and " yy" in every box
//! from the width of "xxx" to the width of the whole line, a pixel apart,
//! wrapped and balanced.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{Hyphens, TextWrap, UnifiedConstraints};
use hyphenation::Language;

use crate::calmet_lines::{calmet_ui, is_inserted_hyphen, lay_out, lines};

/// The text in front of the word, so the word starts mid-line.
const LEAD: &str = "xxx ";

/// The text after the word, so the word does not end the paragraph.
const TAIL: &str = " yy";

/// A word with a soft hyphen after its sixth letter.
const WORD: &str = "hyphen\u{AD}ation";

/// The number of letters in `text`.
fn letters_in(text: &str) -> usize {
    text.chars().filter(|ch| ch.is_alphabetic()).count()
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
    /// The line's text, an inserted hyphen included.
    text: String,
    /// Whether the line ends in a hyphen the line breaker put there.
    ends_in_a_hyphen: bool,
    /// Where the line's last cluster ends.
    end: f32,
}

/// Every line that ends inside [`WORD`] when it is laid out with `hyphens`,
/// wrapped and balanced, in every box from the width of the lead's letters
/// to the width of the whole text.
fn lines_broken_inside(font: &FontRef, hyphens: Hyphens) -> Vec<BrokenInside> {
    let text = format!("{LEAD}{WORD}{TAIL}");
    let lead_letters = letters_in(LEAD);
    let word_letters = letters_in(WORD);
    let narrowest = natural_width(font, LEAD.trim_end()).floor();
    let widest = natural_width(font, &text).ceil();
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
                |paragraph: &mut UnifiedConstraints| {
                    paragraph.hyphenation = hyphens;
                    paragraph.hyphenation_language = Some(Language::EnglishUS);
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
                        text: line.text(),
                        ends_in_a_hyphen: line
                            .clusters
                            .last()
                            .is_some_and(|c| is_inserted_hyphen(c)),
                        end: line.end,
                    });
                }
            }
        }
    }
    found
}

/// Asserts that every line broken inside the word with `hyphens` ends in a
/// hyphen and fits its box, and that some wrapped box breaks the word.
fn assert_broken_lines_end_in_a_hyphen(hyphens: Hyphens) {
    let font = calmet_ui();
    let found = lines_broken_inside(&font, hyphens);
    for line in &found {
        let BrokenInside {
            width,
            wrap,
            text,
            ends_in_a_hyphen,
            end,
        } = line;
        assert!(
            *ends_in_a_hyphen,
            "hyphens: {hyphens:?}, {wrap:?} in a {width} px box: the line {text:?} breaks \
             {WORD:?} at its soft hyphen and shows no hyphen"
        );
        assert!(
            *end <= *width + 0.5,
            "hyphens: {hyphens:?}, {wrap:?} in a {width} px box: the line {text:?} ends at \
             {end:.2}, past its box, with its hyphen"
        );
    }
    assert!(
        found.iter().any(|line| line.wrap == TextWrap::Wrap),
        "hyphens: {hyphens:?}: {WORD:?} is broken at its soft hyphen in no box from the lead's \
         width to the whole line's"
    );
}

#[test]
fn a_line_broken_at_a_soft_hyphen_ends_in_a_hyphen_with_hyphens_manual() {
    assert_broken_lines_end_in_a_hyphen(Hyphens::Manual);
}

#[test]
fn a_line_broken_at_a_soft_hyphen_ends_in_a_hyphen_with_hyphens_auto() {
    assert_broken_lines_end_in_a_hyphen(Hyphens::Auto);
}
