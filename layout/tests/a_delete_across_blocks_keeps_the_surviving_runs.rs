//! Delete, Backspace or typing over a selection that spans blocks joins the
//! first and the last block, and what survives of the two keeps its
//! formatting: the `<b>` before the selection and the `<i>` after it are
//! still there in the joined paragraph.
//!
//! WPT's `editing/data/delete.js`, `forwarddelete.js` and `inserttext.js`
//! pin the join (`<p>foo[bar</p><p>baz]quz</p>` -> `<p>foo{}quz</p>`, the
//! inline elements on both sides kept); typed text takes the style of the
//! selection's start. The rows below are those shapes with formatting on
//! both sides, in the fixture's markup (`tests/common/editing_harness.rs`),
//! read from the structural edit the engine records for the app
//! (`ReplaceChildren`, `LayoutWindow::replace_cross_block_selection`).
//!
//! Before: the joined block got ONE text run - the two blocks' kept text
//! flattened ("v1"), every `<b>`, `<i>` and `<br>` of both gone.

use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};
use azul_layout::managers::changeset::DocumentOperation;

use crate::editing_harness::{
    dnid, host_dom, lay_out_new_generation, markup_of_fragment, Editor, HOST,
};

#[derive(Debug, Clone, Copy)]
enum Command {
    Delete,
    ForwardDelete,
    InsertText(&'static str),
}

fn run(editor: &mut Editor, command: Command) {
    match command {
        Command::Delete | Command::ForwardDelete => {
            let direction = if matches!(command, Command::Delete) {
                SelectionDirection::Backward
            } else {
                SelectionDirection::Forward
            };
            let _ = editor.lw.apply_selection_op(
                dnid(HOST),
                &SelectionOp::new(direction, SelectionStep::Character, SelectionMode::Delete),
            );
        }
        Command::InsertText(text) => editor.type_text(text),
    }
}

/// The blocks the recorded edit puts where the selected ones were.
fn replacement(editor: &Editor) -> String {
    let edit = editor
        .lw
        .get_pending_document_edit()
        .expect("the join is recorded for the app");
    match &edit.operation {
        DocumentOperation::ReplaceChildren(r) => markup_of_fragment(&r.content),
        other => panic!("the join is a ReplaceChildren, not {other:?}"),
    }
}

/// Rows: the blocks with a selection across them, the command, the blocks
/// that replace them.
const ROWS: &[(&str, Command, &str)] = &[
    // delete.js / forwarddelete.js: the join, formatting on both sides.
    (
        "<p>ab<b>c[d</b>ef</p><p><i>g]h</i>ij</p>",
        Command::Delete,
        "<p>ab<b>c</b><i>h</i>ij</p>",
    ),
    (
        "<p>ab<b>c[d</b>ef</p><p><i>g]h</i>ij</p>",
        Command::ForwardDelete,
        "<p>ab<b>c</b><i>h</i>ij</p>",
    ),
    // inserttext.js: the typed text takes the style of the selection's start.
    (
        "<p>ab<b>c[d</b>ef</p><p><i>g]h</i>ij</p>",
        Command::InsertText("X"),
        "<p>ab<b>cX</b><i>h</i>ij</p>",
    ),
    // A block between the two ends goes with the selection.
    (
        "<p>a[b</p><p><u>cd</u></p><p>e]<s>f</s></p>",
        Command::Delete,
        "<p>a<s>f</s></p>",
    ),
    // A line break before the selection stays.
    (
        "<p>a<br>b[c</p><p>d]e</p>",
        Command::Delete,
        "<p>a<br>be</p>",
    ),
    // Out of a quote: what the quote held after the selection joins the
    // paragraph, and the emptied quote goes.
    (
        "<p>a[b</p><blockquote><p>c]<b>d</b></p></blockquote>",
        Command::Delete,
        "<p>a<b>d</b></p>",
    ),
];

#[test]
fn a_delete_across_blocks_keeps_the_formatting_of_what_survives() {
    for &(input, command, expected) in ROWS {
        let mut editor = Editor::new(input);
        run(&mut editor, command);
        assert_eq!(replacement(&editor), expected, "{input} + {command:?}");
    }
}

/// The caret lands at the join - inside the `<b>` it was cut in - once the
/// app has applied the edit and rendered the joined paragraph.
#[test]
fn the_caret_lands_at_the_join_inside_the_formatting_it_was_cut_in() {
    let mut editor = Editor::new("<p>ab<b>c[d</b>ef</p><p><i>g]h</i>ij</p>");
    run(&mut editor, Command::Delete);
    let id = editor
        .lw
        .get_pending_document_edit()
        .expect("the join is recorded")
        .id;

    // The app applies it and renders `<p>ab<b>c</b><i>h</i>ij</p>`:
    // `p(2) > [text(3) "ab", b(4) > text(5) "c", i(6) > text(7) "h",
    // text(8) "ij"]`.
    assert!(editor.lw.mark_document_edit_applied(id));
    let (dom, ..) = host_dom("<p>ab<b>c</b><i>h</i>ij</p>");
    lay_out_new_generation(&mut editor.lw, dom);

    let caret = editor.lw.document_caret().expect("the caret is restored");
    assert_eq!(caret.node, dnid(2), "in the joined paragraph");
    assert_eq!(
        caret.text_byte, 3,
        "after \"abc\", where the selection began"
    );
}
