//! Up, Down, Left and Right without Shift move the caret from block to
//! block inside one editing host - paragraph to paragraph, into and out of a
//! quote and a list item - and Up / Down keep the caret's column.
//!
//! What a browser's contenteditable does (the caret-movement part of the
//! Selection API's `modify("move", "forward" | "backward", "character" |
//! "line")`, <https://w3c.github.io/selection-api/#dom-selection-modify>,
//! which WPT exercises in `selection/modify*.html`): at the end of a block a
//! Right goes to the start of the next one, and Down from a block's last line
//! goes to the next block's first line at the same x. The x a run of
//! Up / Down aims at is kept while a short line cuts it off - the "goal
//! column" every text editor keeps.
//!
//! Before: a plain arrow only stepped inside the caret's block
//! (`LayoutWindow::apply_selection_op`'s Move arm); at a block edge it did
//! nothing, so a caret could not leave the paragraph it was put in.

use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};

use crate::editing_harness::{dnid, Editor, HOST};

/// `body(0) > host(1) > [p(2) > "abcdef"(3), blockquote(4) > p(5) >
/// "abcdef"(6), ul(7) > li(8) > "abcdef"(9), p(10) > "abcdef"(11)]`
const DOCUMENT: &str = "<p>abcdef</p><blockquote><p>abcdef</p></blockquote>\
                        <ul><li>abcdef</li></ul><p>abcdef</p>";

#[derive(Debug, Clone, Copy)]
enum Key {
    Left,
    Right,
    Up,
    Down,
}

fn press(editor: &mut Editor, key: Key) {
    let (direction, step) = match key {
        Key::Left => (SelectionDirection::Backward, SelectionStep::Character),
        Key::Right => (SelectionDirection::Forward, SelectionStep::Character),
        Key::Up => (SelectionDirection::Backward, SelectionStep::VisualLine),
        Key::Down => (SelectionDirection::Forward, SelectionStep::VisualLine),
    };
    let _ = editor.lw.apply_selection_op(
        dnid(HOST),
        &SelectionOp::new(direction, step, SelectionMode::Move),
    );
}

/// The caret as the app reads it: the block's element and the byte in the
/// block's text.
fn caret(editor: &Editor) -> (usize, u32) {
    let position = editor.lw.document_caret().expect("a session is open");
    let node = position
        .node
        .node
        .into_crate_internal()
        .expect("the caret's block has an element")
        .index();
    (node, position.text_byte)
}

/// Put the caret at byte `byte` of text node `text_node`.
fn caret_in(editor: &mut Editor, text_node: usize, byte: u32) {
    let (block, cursor) = editor.caret_at(text_node, byte);
    editor.lw.open_session(
        block,
        azul_core::selection::SelectionRange {
            start: cursor,
            end: cursor,
        },
    );
}

/// The caret (text node, byte), the key, the caret afterwards (block
/// element, byte in its text).
type Row = ((usize, u32), Key, (usize, u32));

/// Rows: the caret (text node, byte), the key, the caret afterwards
/// (block element, byte in its text).
const ROWS: &[Row] = &[
    // Right at the end of a paragraph: the start of the next block, in a quote.
    ((3, 6), Key::Right, (5, 0)),
    // Left at the start of the quoted paragraph: the end of the one before.
    ((6, 0), Key::Left, (2, 6)),
    // Down / Up keep the column, into and out of the quote and the list item.
    ((3, 2), Key::Down, (5, 2)),
    ((6, 2), Key::Down, (8, 2)),
    ((9, 2), Key::Up, (5, 2)),
    ((11, 4), Key::Up, (8, 4)),
    // Out of the list item at its end, into it at its start (after its marker).
    ((9, 6), Key::Right, (10, 0)),
    ((11, 0), Key::Left, (8, 6)),
    ((9, 0), Key::Left, (5, 6)),
    // The host's first block: Up has nowhere to go and leaves the caret.
    ((3, 2), Key::Up, (2, 2)),
];

#[test]
fn a_plain_arrow_at_a_block_edge_moves_the_caret_into_the_next_block() {
    for &((text_node, byte), key, expected) in ROWS {
        let mut editor = Editor::new(DOCUMENT);
        caret_in(&mut editor, text_node, byte);
        press(&mut editor, key);
        assert_eq!(
            caret(&editor),
            expected,
            "caret at byte {byte} of text node {text_node}, then {key:?}"
        );
    }
}

/// `abcdef|gh`, Down into the two-letter line below: the caret stops at its
/// end. Down again: back in the column it came from, not at the short line's
/// end.
#[test]
fn up_and_down_keep_the_column_across_a_short_block() {
    // `body(0) > host(1) > [p(2) > "abcdefgh"(3), p(4) > "ab"(5),
    // p(6) > "abcdefgh"(7)]`
    let mut editor = Editor::new("<p>abcdef[]gh</p><p>ab</p><p>abcdefgh</p>");

    press(&mut editor, Key::Down);
    assert_eq!(caret(&editor), (4, 2), "the short line's end");

    press(&mut editor, Key::Down);
    assert_eq!(caret(&editor), (6, 6), "the column Down started in");

    press(&mut editor, Key::Up);
    press(&mut editor, Key::Up);
    assert_eq!(caret(&editor), (2, 6), "and back up to where it started");
}
