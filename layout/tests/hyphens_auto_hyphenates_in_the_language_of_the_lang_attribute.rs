//! `hyphens: auto` hyphenates in the language of the `lang` attribute.
//!
//! CSS Text 3 5.4: automatic hyphenation uses "a language-appropriate
//! hyphenation resource", and the language is the CONTENT LANGUAGE of the
//! element - in HTML the nearest `lang` (`xml:lang` in XML) on it or an
//! ancestor. azul took the language only from its own
//! `-azul-hyphenation-language` property: `<div lang="en" style="hyphens:
//! auto">Advertisement</div>` in a narrow box was not hyphenated and
//! overflowed, while the same box with `-azul-hyphenation-language: en` broke
//! the word into "Adver-" / "tisement" (pdfocr engine issue 3, Calmet vol. 1).
//! The azul property stays as an override of `lang`.
//!
//! Font-independent: a 60px box holds no font's "Advertisement" at 20px on
//! one line, so a hyphenated box is several 30px lines tall and an
//! unhyphenated one (it overflows) one line. Every box is compared with the
//! same box hyphenated by the azul property.
//!
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, laid_out, near, rect};

/// A 60px wide box of 30px lines, `extra` appended to its style.
fn narrow(id: &str, attributes: &str, extra: &str) -> String {
    format!(
        "<p id=\"{id}\" {attributes} style=\"width: 60px; font-size: 20px; line-height: 30px; \
         margin: 0; {extra}\">Advertisement</p>"
    )
}

fn height(lw: &azul_layout::window::LayoutWindow, id: &str) -> f32 {
    rect(lw, id).size.height
}

/// The heights of a box nobody hyphenates and of one the azul property
/// hyphenates (the reference): the second is several lines.
fn one_line_and_hyphenated(lw: &azul_layout::window::LayoutWindow) -> (f32, f32) {
    let one_line = height(lw, "plain");
    let hyphenated = height(lw, "by-property");
    assert!(
        one_line > 0.0 && one_line < 45.0,
        "an unhyphenated word overflows its box on one 30px line: {one_line}"
    );
    assert!(
        hyphenated >= one_line + 25.0,
        "-azul-hyphenation-language: en hyphenates the word over several lines: \
         {hyphenated} (one line: {one_line})"
    );
    (one_line, hyphenated)
}

fn reference_boxes() -> String {
    format!(
        "{}{}",
        narrow("plain", "", ""),
        narrow(
            "by-property",
            "",
            "hyphens: auto; -azul-hyphenation-language: en"
        ),
    )
}

#[test]
fn hyphens_auto_hyphenates_in_the_language_of_an_ancestors_lang_attribute() {
    // The repro's shape: `lang` and `hyphens` on the region, the text in a
    // paragraph inside it.
    let lw = body(&format!(
        "{}<div lang=\"en\" style=\"hyphens: auto\">{}</div>",
        reference_boxes(),
        narrow("by-lang", "", ""),
    ));
    let (_, hyphenated) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, hyphenated, 0.5),
        "lang=\"en\" hyphenates like -azul-hyphenation-language: en ({hyphenated}px), \
         not one overflowing line: {by_lang}"
    );
}

#[test]
fn hyphens_auto_hyphenates_in_the_language_of_the_elements_own_lang_attribute() {
    let lw = body(&format!(
        "{}{}",
        reference_boxes(),
        narrow("by-lang", "lang=\"en-US\"", "hyphens: auto"),
    ));
    let (_, hyphenated) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, hyphenated, 0.5),
        "lang=\"en-US\" hyphenates: {by_lang} (the reference: {hyphenated})"
    );
}

#[test]
fn the_nearest_lang_attribute_names_the_language() {
    // An outer language with no hyphenation resource (`x-none` is no
    // language at all), the inner one English: the inner one is the
    // paragraph's content language.
    let lw = body(&format!(
        "{}<div lang=\"x-none\">{}</div>",
        reference_boxes(),
        narrow("by-lang", "lang=\"EN\"", "hyphens: auto"),
    ));
    let (_, hyphenated) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, hyphenated, 0.5),
        "the paragraph's own lang=\"EN\" (language tags ignore case) wins over the outer \
         one: {by_lang} (the reference: {hyphenated})"
    );
}

#[test]
fn a_documents_lang_on_its_html_element_is_the_language_of_every_paragraph() {
    // Every page of the pdfocr book is `<html lang="en">`.
    let lw = laid_out(
        &format!(
            "<html lang=\"en\"><head></head><body style=\"margin: 0\">{}{}</body></html>",
            reference_boxes(),
            narrow("by-lang", "", "hyphens: auto"),
        ),
        800.0,
        600.0,
    );
    let (_, hyphenated) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, hyphenated, 0.5),
        "<html lang=\"en\"> hyphenates the paragraph: {by_lang} (the reference: {hyphenated})"
    );
}

#[test]
fn hyphens_manual_does_not_hyphenate_a_word_in_a_lang_attributes_language() {
    // `lang` names the language; only `hyphens: auto` asks for hyphenation.
    let lw = body(&format!(
        "{}{}",
        reference_boxes(),
        narrow("by-lang", "lang=\"en\"", ""),
    ));
    let (one_line, _) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, one_line, 0.5),
        "no hyphens: auto, no hyphenation: {by_lang}"
    );
}

#[test]
fn the_azul_hyphenation_language_property_overrides_the_lang_attribute() {
    let lw = body(&format!(
        "{}{}",
        reference_boxes(),
        narrow(
            "by-lang",
            "lang=\"x-none\"",
            "hyphens: auto; -azul-hyphenation-language: en"
        ),
    ));
    let (_, hyphenated) = one_line_and_hyphenated(&lw);
    let by_lang = height(&lw, "by-lang");
    assert!(
        near(by_lang, hyphenated, 0.5),
        "-azul-hyphenation-language: en hyphenates whatever lang says: {by_lang}"
    );
}
