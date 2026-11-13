//! A word with punctuation around it is hyphenated where the bare word
//! breaks.
//!
//! The hyphenation dictionary breaks words, and its patterns know where a
//! word starts and ends: "th" at the end of a word is no place to break, and
//! English leaves at least two letters before a break and three after it.
//! A dictionary handed "Month," with its comma sees no word end after "th"
//! and breaks "Mon-th,", counting the comma among the three letters; an
//! elided ending ("deliv-er'd", "the-se'd") and an opening quote ("‘a-head")
//! go wrong the same way.
//!
//! Each word is laid out between "xxx " and " yy" in every box from the
//! width of "xxx" to the width of the whole line, a pixel apart, wrapped and
//! balanced: every line that ends in an inserted hyphen holds the word's
//! letters up to one of the bare word's dictionary breaks.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{Hyphens, TextWrap, UnifiedConstraints};
use hyphenation::{Hyphenator, Language, Load, Standard};

use crate::calmet_lines::{calmet_ui, is_inserted_hyphen, lay_out, lines};

/// The text in front of the word, so the word starts mid-line.
const LEAD: &str = "xxx ";

/// The text after the word, so the word does not end the paragraph.
const TAIL: &str = " yy";

/// The en-US dictionary every paragraph here is hyphenated with.
fn dictionary() -> Standard {
    Standard::from_embedded(Language::EnglishUS).expect("en-US patterns")
}

/// Where the dictionary breaks the bare `word`, as the number of letters
/// before each break.
fn bare_breaks(dictionary: &Standard, word: &str) -> Vec<usize> {
    dictionary
        .hyphenate(word)
        .breaks
        .iter()
        .map(|&byte| word[..byte].chars().count())
        .collect()
}

/// The number of letters in `text`.
fn letters_in(text: &str) -> usize {
    text.chars().filter(|ch| ch.is_alphabetic()).count()
}

/// Hyphenates the paragraph in American English.
fn hyphenate_in_english(paragraph: &mut UnifiedConstraints) {
    paragraph.hyphenation = Hyphens::Auto;
    paragraph.hyphenation_language = Some(Language::EnglishUS);
}

/// How wide `text` is on one line.
fn natural_width(font: &FontRef, text: &str) -> f32 {
    let layout = lay_out(font, text, 10_000.0, |_| {}, hyphenate_in_english);
    let one_line = lines(&layout);
    one_line.first().map_or(0.0, |line| line.end)
}

/// A line that ends in an inserted hyphen.
struct Hyphenated {
    /// The width of the box it was laid out in.
    width: f32,
    /// How its paragraph was broken into lines.
    wrap: TextWrap,
    /// The letters of the word before the hyphen.
    letters: usize,
    /// The line's text, the hyphen included.
    text: String,
}

/// Every line that ends in an inserted hyphen when `form` is laid out
/// between [`LEAD`] and [`TAIL`], wrapped and balanced, in every box from
/// the width of the lead's letters to the width of the whole text.
fn hyphenated_lines(font: &FontRef, form: &str) -> Vec<Hyphenated> {
    let text = format!("{LEAD}{form}{TAIL}");
    let lead_letters = letters_in(LEAD);
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
                    hyphenate_in_english(paragraph);
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
                if line.clusters.last().is_some_and(|c| is_inserted_hyphen(c)) {
                    found.push(Hyphenated {
                        width,
                        wrap,
                        letters: letters.saturating_sub(lead_letters),
                        text: line.text(),
                    });
                }
            }
        }
    }
    found
}

/// Asserts that every hyphenated line of `form` breaks it where the bare
/// `word` breaks, and returns those lines.
fn assert_breaks_as_the_bare_word(
    font: &FontRef,
    dictionary: &Standard,
    word: &str,
    form: &str,
) -> Vec<Hyphenated> {
    let breaks = bare_breaks(dictionary, word);
    let found = hyphenated_lines(font, form);
    for line in &found {
        let Hyphenated {
            width,
            wrap,
            letters,
            text,
        } = line;
        assert!(
            breaks.contains(letters),
            "{wrap:?} in a {width} px box, {form:?} is broken after {letters} letters (the line \
             reads {text:?}), where the bare word {word:?} does not break (its breaks are after \
             {breaks:?} letters)"
        );
    }
    found
}

/// Asserts that a wrapped paragraph hyphenates `form` in some box: its
/// punctuation does not keep it from breaking where its word breaks.
fn assert_hyphenated_somewhere(form: &str, found: &[Hyphenated]) {
    assert!(
        found.iter().any(|line| line.wrap == TextWrap::Wrap),
        "{form:?} is hyphenated in no box from the lead's width to the whole line's: the \
         punctuation beside its word keeps the word from breaking where it breaks bare"
    );
}

#[test]
fn a_word_followed_by_punctuation_breaks_where_the_bare_word_breaks() {
    let font = calmet_ui();
    let dictionary = dictionary();
    for word in ["Month", "Heaven", "difficulties"] {
        for ending in [",", ".", ";", ":"] {
            let form = format!("{word}{ending}");
            let found = assert_breaks_as_the_bare_word(&font, &dictionary, word, &form);
            if word == "difficulties" {
                assert_hyphenated_somewhere(&form, &found);
            }
        }
    }
    for (word, form) in [("Heaven", "Heaven\u{2019}"), ("Woman", "(Woman)")] {
        assert_breaks_as_the_bare_word(&font, &dictionary, word, form);
    }
}

#[test]
fn a_word_with_an_elided_ending_breaks_only_in_its_letters() {
    let font = calmet_ui();
    let dictionary = dictionary();
    for (word, ending) in [("these", "d"), ("Month", "s"), ("deliver", "d")] {
        for apostrophe in ['\'', '\u{2019}'] {
            let form = format!("{word}{apostrophe}{ending}");
            let found = assert_breaks_as_the_bare_word(&font, &dictionary, word, &form);
            if word == "deliver" {
                assert_hyphenated_somewhere(&form, &found);
            }
        }
    }
}

#[test]
fn a_word_after_an_opening_quote_or_bracket_breaks_where_the_bare_word_breaks() {
    let font = calmet_ui();
    let dictionary = dictionary();
    for (word, form) in [
        ("ahead", "\u{2018}ahead"),
        ("Heaven", "\u{2018}Heaven"),
        ("Egypt", "(Egypt"),
        ("Abraham", "(Abraham)"),
    ] {
        assert_breaks_as_the_bare_word(&font, &dictionary, word, form);
    }
    let form = "\u{2018}Abraham";
    let found = assert_breaks_as_the_bare_word(&font, &dictionary, "Abraham", form);
    assert_hyphenated_somewhere(form, &found);
}

#[test]
fn an_english_word_keeps_three_letters_after_its_hyphen() {
    let dictionary = dictionary();
    assert_eq!(
        dictionary.minima,
        (2, 3),
        "harness: the en-US dictionary leaves two letters before a break and three after it"
    );
    let font = calmet_ui();
    for (word, form) in [
        ("situated", "situated,"),
        ("descended", "descended."),
        ("printed", "printed."),
    ] {
        for line in hyphenated_lines(&font, form) {
            let Hyphenated {
                width,
                wrap,
                letters,
                text,
            } = line;
            let after = letters_in(word).saturating_sub(letters);
            assert!(
                after >= 3,
                "{wrap:?} in a {width} px box, {form:?} is broken with {after} letters after its \
                 hyphen (the line reads {text:?}), and English leaves at least three"
            );
        }
    }
}

#[test]
fn a_bare_word_and_a_hyphenated_compound_break_as_before() {
    let font = calmet_ui();
    let dictionary = dictionary();
    let found = assert_breaks_as_the_bare_word(&font, &dictionary, "difficulties", "difficulties");
    assert_hyphenated_somewhere("difficulties", &found);

    // The compound breaks after its own hyphen, and inside its second word
    // where that word breaks bare.
    let (first, second) = ("self", "sufficient");
    let form = format!("{first}-{second}");
    let breaks: Vec<usize> = bare_breaks(&dictionary, second)
        .into_iter()
        .map(|letters| letters_in(first) + letters)
        .collect();
    let found = hyphenated_lines(&font, &form);
    for line in &found {
        let Hyphenated {
            width,
            wrap,
            letters,
            text,
        } = line;
        assert!(
            breaks.contains(letters),
            "{wrap:?} in a {width} px box, {form:?} is broken after {letters} letters (the line \
             reads {text:?}), where neither of its words breaks (its breaks are after {breaks:?} \
             letters)"
        );
    }
    assert_hyphenated_somewhere(&form, &found);
}

#[test]
fn a_soft_hyphen_stays_the_only_break_of_its_balanced_word() {
    // The balanced breaker hands the dictionary the whole word, soft hyphen
    // and comma included; a word with a soft hyphen breaks there and nowhere
    // else.
    let font = calmet_ui();
    let form = "hy\u{AD}phenation,";
    for line in hyphenated_lines(&font, form) {
        let Hyphenated {
            width,
            wrap,
            letters,
            text,
        } = line;
        if wrap != TextWrap::Balance {
            continue;
        }
        assert_eq!(
            letters, 2,
            "balanced in a {width} px box, {form:?} is broken after {letters} letters (the line \
             reads {text:?}), not at its soft hyphen"
        );
    }
}
