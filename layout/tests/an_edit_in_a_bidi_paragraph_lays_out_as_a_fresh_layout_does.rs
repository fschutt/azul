//! An edit in a `unicode-bidi: plaintext` paragraph lays it out as a fresh
//! layout of the edited text does.
//!
//! Before shaping, a paragraph's text is put in visual order (UAX 9) from
//! its base direction and its `unicode-bidi`: `plaintext` takes the base
//! direction from the first strong character, so Hebrew text runs right to
//! left in a left-to-right box. A keystroke lays the edited paragraph out
//! again, and its letters have to stand where a fresh window puts the
//! edited text.
//!
//! The page and the edit are those of
//! `an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does`.

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::{items_of, same_layout},
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{
        page, text_of, type_in_front, window,
    },
};

/// Asserts that typing `letter` in front of `text` in a paragraph styled
/// `css` lays it out as a fresh window lays out the edited text, and as no
/// paragraph without the `unicode-bidi` does.
fn assert_an_edit_lays_out_as_a_fresh_layout(text: &str, letter: &str, css: &str, plain: &str) {
    let edited = format!("{letter}{text}");
    let fresh = items_of(&window(page(&edited, css)));
    let without = items_of(&window(page(&edited, plain)));
    assert!(
        !same_layout(&fresh, &without),
        "harness: {css} lays the paragraph out apart from {plain}: {fresh:?}"
    );

    let mut lw = window(page(text, css));
    type_in_front(&mut lw, letter);
    assert_eq!(text_of(&lw), edited, "harness: the letter is typed in");
    let got = items_of(&lw);
    assert!(
        same_layout(&got, &fresh),
        "{css}: after an edit the paragraph's letters stand at {got:?}, a fresh layout of the \
         same text puts them at {fresh:?}: the edit put the text in visual order without its \
         paragraph's unicode-bidi"
    );
}

#[test]
fn an_edit_in_a_plaintext_paragraph_keeps_the_direction_of_its_first_strong_letter() {
    assert_an_edit_lays_out_as_a_fresh_layout(
        "\u{5E9}\u{5DC}\u{5D5}\u{5DD} abc",
        "\u{5D0}",
        "width: 200px; font-size: 20px; unicode-bidi: plaintext;",
        "width: 200px; font-size: 20px;",
    );
}
