//! An edit that replaces a letter lays its paragraph out as a fresh layout
//! of the edited text does.
//!
//! Replacing a selected letter with another keeps the paragraph's number of
//! items, and its lines have to be the lines a fresh window lays the edited
//! text out in: a centred or right-aligned line is aligned again, a line
//! that runs right to left or top to bottom grows along its own direction,
//! and a space that takes a letter's place is a break the lines use.
//!
//! Each case first types a letter in front of the text, so the paragraph's
//! last layout is one an edit made, then replaces one letter. The page is
//! that of `an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does`.

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};
use azul_layout::window::LayoutWindow;

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::{items_of, same_layout},
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{
        page, text_of, type_in_front, window,
    },
};

/// The caret in front of byte `byte` of the paragraph's text.
const fn at(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

/// Selects bytes `start..end` of the paragraph's text and types `letter`
/// over them.
fn replace(lw: &mut LayoutWindow, start: u32, end: u32, letter: &str) {
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("the typing opened an editing session")
        .set_single_range(SelectionRange {
            start: at(start),
            end: at(end),
        });
    let _ = lw.record_text_input(letter);
    let _ = lw.apply_text_changeset();
}

/// Asserts that typing `first` in front of `text` and then `letter` over
/// bytes `start..end` of the result, in a paragraph styled `css`, gives the
/// paragraph `edited` laid out as a fresh window lays it out.
fn assert_a_replacement_lays_out_as_a_fresh_layout(
    text: &str,
    first: &str,
    (start, end): (u32, u32),
    letter: &str,
    edited: &str,
    css: &str,
) {
    let mut lw = window(page(text, css));
    type_in_front(&mut lw, first);
    replace(&mut lw, start, end, letter);
    assert_eq!(text_of(&lw), edited, "harness: the letter is replaced");
    let fresh = items_of(&window(page(edited, css)));
    let got = items_of(&lw);
    assert!(
        same_layout(&got, &fresh),
        "{css}: after replacing a letter the paragraph's letters stand at {got:?}, a fresh \
         layout of the same text puts them at {fresh:?}"
    );
}

#[test]
fn a_letter_replaced_in_a_centred_line_centres_the_line_again() {
    assert_a_replacement_lays_out_as_a_fresh_layout(
        "nin",
        "n",
        (2, 3),
        "m",
        "nnmn",
        "width: 300px; font-size: 20px; text-align: center;",
    );
}

#[test]
fn a_letter_replaced_in_a_right_aligned_line_aligns_the_line_again() {
    assert_a_replacement_lays_out_as_a_fresh_layout(
        "nin",
        "n",
        (2, 3),
        "m",
        "nnmn",
        "width: 300px; font-size: 20px; text-align: right;",
    );
}

#[test]
fn a_letter_replaced_in_a_right_to_left_line_grows_it_to_the_left() {
    // Alef in front of shin, vav, final mem; the narrow vav becomes a shin.
    assert_a_replacement_lays_out_as_a_fresh_layout(
        "\u{5E9}\u{5D5}\u{5DD}",
        "\u{5D0}",
        (4, 6),
        "\u{5E9}",
        "\u{5D0}\u{5E9}\u{5E9}\u{5DD}",
        "width: 300px; font-size: 20px; direction: rtl;",
    );
}

#[test]
fn a_letter_replaced_in_a_vertical_line_grows_it_downwards() {
    assert_a_replacement_lays_out_as_a_fresh_layout(
        "nin",
        "n",
        (2, 3),
        "m",
        "nnmn",
        "width: 100px; height: 300px; font-size: 20px; writing-mode: vertical-rl;",
    );
}

#[test]
fn a_letter_replaced_by_a_space_is_a_break_the_lines_use() {
    // Five monospace letters fit a line, six do not: "aa-" breaks after its
    // hyphen, "bbbbb" has no break. A space in it lets "bb" join "aa-".
    let css = "width: 75px; font-size: 20px; font-family: monospace;";
    let before = items_of(&window(page("aa-bbbbb", css)));
    assert!(
        !before.iter().any(|item| item.0 == 0 && item.1 == "b"),
        "harness: \"bbbbb\" does not fit behind \"aa-\": {before:?}"
    );
    let after = items_of(&window(page("aa-bb bb", css)));
    assert!(
        after.iter().any(|item| item.0 == 0 && item.1 == "b"),
        "harness: \"bb\" fits behind \"aa-\" once a space breaks the word: {after:?}"
    );
    assert_a_replacement_lays_out_as_a_fresh_layout("a-bbbbb", "a", (5, 6), " ", "aa-bb bb", css);
}
