//! A balanced paragraph fits its box wherever the wrapped paragraph does.
//!
//! Both line breakers break at the same places: the soft wrap opportunities
//! between the words and inside them, and the hyphenation breaks. The
//! balanced one only chooses among them differently, so a box in which the
//! wrapped paragraph fits every line holds a balanced paragraph too. A word
//! the dictionary breaks keeps its other break points: every letter with
//! `word-break: break-all`, the place before each CJK ideograph.
//!
//! Each text is laid out wrapped and balanced, with `hyphens: auto`, in
//! every box from the width of "xxx" to the width of the whole line, a pixel
//! apart.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{Hyphens, TextWrap, UnifiedConstraints, WordBreak};
use hyphenation::Language;

use crate::calmet_lines::{calmet_ui, lay_out, lines};

/// How wide `text` is on one line.
fn natural_width(font: &FontRef, text: &str) -> f32 {
    let layout = lay_out(font, text, 10_000.0, |_| {}, |_| {});
    let one_line = lines(&layout);
    one_line.first().map_or(0.0, |line| line.end)
}

/// The texts of the lines of `text` in a `width` px box that end past it.
fn lines_past_the_box(
    font: &FontRef,
    text: &str,
    width: f32,
    wrap: TextWrap,
    word_break: WordBreak,
) -> Vec<String> {
    let layout = lay_out(
        font,
        text,
        width,
        |_| {},
        |paragraph: &mut UnifiedConstraints| {
            paragraph.hyphenation = Hyphens::Auto;
            paragraph.hyphenation_language = Some(Language::EnglishUS);
            paragraph.word_break = word_break;
            paragraph.text_wrap = wrap;
        },
    );
    lines(&layout)
        .iter()
        .filter(|line| line.end > width + 0.5)
        .map(|line| line.text())
        .collect()
}

/// Asserts that in every box where the wrapped `text` fits, the balanced
/// one fits too, and that there are such boxes narrower than the text.
fn assert_balanced_fits_where_wrapped_fits(text: &str, word_break: WordBreak) {
    let font = calmet_ui();
    let narrowest = natural_width(&font, "xxx").floor();
    let widest = natural_width(&font, text).ceil();
    let steps = (widest - narrowest) as usize;
    let mut wrapped_fits = 0;
    for step in 0..steps {
        let width = narrowest + step as f32;
        if !lines_past_the_box(&font, text, width, TextWrap::Wrap, word_break).is_empty() {
            continue;
        }
        wrapped_fits += 1;
        let past = lines_past_the_box(&font, text, width, TextWrap::Balance, word_break);
        assert!(
            past.is_empty(),
            "word-break: {word_break:?}, in a {width} px box every wrapped line of {text:?} \
             fits, but the balanced lines {past:?} end past the box: the balanced breaker lost \
             break points of a word the dictionary breaks"
        );
    }
    assert!(
        wrapped_fits > 0,
        "harness: the wrapped {text:?} fits some box narrower than its line"
    );
}

#[test]
fn a_balanced_word_the_dictionary_breaks_keeps_breaking_between_any_letters() {
    assert_balanced_fits_where_wrapped_fits("xxx hyphenation yy", WordBreak::BreakAll);
}

#[test]
fn a_balanced_word_the_dictionary_breaks_keeps_breaking_before_cjk() {
    assert_balanced_fits_where_wrapped_fits(
        "xxx hyphenation\u{65E5}\u{672C}\u{8A9E}\u{65E5}\u{672C}\u{8A9E} yy",
        WordBreak::Normal,
    );
}
