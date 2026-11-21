//! A line breaks after a hyphen, never before it, and not at a solidus
//! between ASCII letters.
//!
//! UAX 14 breaks after a hyphen (U+002D, U+2010) and a solidus, not before
//! them, and keeps a minus sign with its digits ("-12"). Chrome breaks a
//! solidus only where ICU's rules apply, before a letter outside ASCII
//! ("Ελλάδα/Κύπρος"): its table for ASCII text keeps "and/or", a path and
//! a URL whole.
//!
//! Each text is laid out wrapped and balanced in every box from the width
//! of "xxx" to the width of the whole line, a pixel apart.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{TextWrap, UnifiedConstraints};

use crate::calmet_lines::{calmet_ui, lay_out, lines};

/// How wide `text` is on one line.
fn natural_width(font: &FontRef, text: &str) -> f32 {
    let layout = lay_out(font, text, 10_000.0, |_| {}, |_| {});
    let one_line = lines(&layout);
    one_line.first().map_or(0.0, |line| line.end)
}

/// The lines of `text`, wrapped and balanced, in every box from the width of
/// "xxx" to the width of the whole text: (box width, how it wraps, the
/// lines' texts).
fn every_layout(font: &FontRef, text: &str) -> Vec<(f32, TextWrap, Vec<String>)> {
    let narrowest = natural_width(font, "xxx").floor();
    let widest = natural_width(font, text).ceil();
    let steps = (widest - narrowest) as usize;
    let mut found = Vec::new();
    for wrap in [TextWrap::Wrap, TextWrap::Balance] {
        for step in 0..=steps {
            let width = narrowest + step as f32;
            let layout = lay_out(
                font,
                text,
                width,
                |_| {},
                |paragraph: &mut UnifiedConstraints| paragraph.text_wrap = wrap,
            );
            let texts = lines(&layout).iter().map(|line| line.text()).collect();
            found.push((width, wrap, texts));
        }
    }
    found
}

/// Asserts that no line of `text` starts with `mark`: the line never breaks
/// before it.
fn assert_no_break_before(font: &FontRef, text: &str, mark: char) {
    for (width, wrap, texts) in every_layout(font, text) {
        assert!(
            texts.iter().skip(1).all(|line| !line.starts_with(mark)),
            "{wrap:?} in a {width} px box, {text:?} breaks before {mark:?}: {texts:?}"
        );
    }
}

/// Asserts that some wrapped line of `text` ends with `part`: the line may
/// break after it.
fn assert_breaks_after(font: &FontRef, text: &str, part: &str) {
    let breaks = every_layout(font, text).into_iter().any(|(_, wrap, texts)| {
        wrap == TextWrap::Wrap && texts.iter().any(|line| line.ends_with(part))
    });
    assert!(
        breaks,
        "{text:?} breaks after {part:?} in no box from the width of \"xxx\" to its own"
    );
}

/// Asserts that `whole` is never broken across two lines of `text`.
fn assert_kept_whole(font: &FontRef, text: &str, whole: &str) {
    for (width, wrap, texts) in every_layout(font, text) {
        assert!(
            texts.iter().any(|line| line.contains(whole)),
            "{wrap:?} in a {width} px box, {text:?} breaks {whole:?}: {texts:?}"
        );
    }
}

#[test]
fn a_line_breaks_after_a_hyphen_and_never_before_it() {
    let font = calmet_ui();
    assert_no_break_before(&font, "xxx well-known yy", '-');
    assert_breaks_after(&font, "xxx well-known yy", "well-");
    assert_no_break_before(&font, "xxx well\u{2010}known yy", '\u{2010}');
    assert_breaks_after(&font, "xxx well\u{2010}known yy", "well\u{2010}");
    assert_breaks_after(&font, "xxx ab-12 yy", "ab-");
}

#[test]
fn a_minus_sign_keeps_its_digits() {
    assert_kept_whole(&calmet_ui(), "xxx -12 yy", "-12");
}

#[test]
fn a_solidus_between_ascii_letters_keeps_its_words_together() {
    let font = calmet_ui();
    assert_kept_whole(&font, "xxx and/or yy", "and/or");
    assert_kept_whole(&font, "xxx path/to/file yy", "path/to/file");
}

#[test]
fn a_line_breaks_after_a_solidus_before_a_letter_outside_ascii() {
    let font = calmet_ui();
    let text = "xxx Ελλάδα/Κύπρος yy";
    assert_no_break_before(&font, text, '/');
    assert_breaks_after(&font, text, "/");
}
