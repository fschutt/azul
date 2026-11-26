//! `text-transform` changes the letters the layout draws: `uppercase` and `lowercase` the case of
//! every letter, `capitalize` the first letter of each word, `none` nothing; the text is
//! transformed before it is shaped, so the glyphs painted are those of the transformed text.
//!
//! The face is `Azul Mock Mono`: the glyph of an ASCII character has the id `code point - 0x1F`,
//! so the glyph ids painted say which letters were drawn.

use crate::text_rig::{block, ids_of, lay_out, line_texts, span, window, MONO};

/// The characters the layout draws for `text` under `css`, as one string.
fn drawn(css: &str, text: &str) -> String {
    let (_lw, dl) = crate::text_rig::laid_out(MONO, css, text, 800.0);
    line_texts(&dl).join("|")
}

#[test]
fn the_premise_is_that_the_glyph_ids_say_the_letters() {
    assert_eq!(drawn("", "abc d"), "abc d");
    assert_eq!(drawn("", "ABC D"), "ABC D");
    assert_ne!(ids_of(MONO, "", "abc"), ids_of(MONO, "", "ABC"));
}

#[test]
fn uppercase_draws_every_letter_as_a_capital() {
    assert_eq!(drawn("text-transform: uppercase", "abc d1"), "ABC D1");
}

#[test]
fn lowercase_draws_every_letter_as_a_small_letter() {
    assert_eq!(drawn("text-transform: lowercase", "ABC D1"), "abc d1");
}

#[test]
fn capitalize_draws_the_first_letter_of_each_word_as_a_capital() {
    assert_eq!(drawn("text-transform: capitalize", "abc def"), "Abc Def");
    assert_eq!(drawn("text-transform: capitalize", "aBC dEF"), "ABC DEF", "the other letters stay as written");
}

/// `full-width` sets the ASCII letters as their full-width forms (U+FF21...), which the mock face
/// has no glyphs for: what is painted is no longer the letters written.
#[test]
fn full_width_does_not_draw_the_letters_as_they_are_written() {
    assert_ne!(ids_of(MONO, "text-transform: full-width", "abc"), ids_of(MONO, "", "abc"));
}

#[test]
fn none_draws_the_text_as_written() {
    assert_eq!(drawn("text-transform: none", "aBc"), "aBc");
}

/// The value is inherited, and a box that says another one transforms its own text.
#[test]
fn the_transform_is_inherited_and_a_child_can_change_it() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-transform: uppercase",
        vec![span("", "ab"), span("text-transform: lowercase", "CD"), span("text-transform: none", "eF")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    assert_eq!(line_texts(&dl), ["ABcdeF"]);
}
