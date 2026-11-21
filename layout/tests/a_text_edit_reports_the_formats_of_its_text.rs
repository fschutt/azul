//! The text-edit report the app folds into its model carries the FORMATS of
//! the block's text, and the caret's pending format is readable.
//!
//! DEDUP_EDITORS D1 / A3.2: `DocumentTextEdit` was `{node, text, revision}`.
//! The engine keeps a typing style at a collapsed caret (Ctrl+B, then type:
//! the typed text is bold on screen) and pastes inline-formatted HTML into
//! the overlay with its bold - but the report the app syncs its model from
//! flattened all of it to plain text, so AzMail, AzNotes and AzWriter lost
//! the bold of both on the next rebuild (AzNotes mirrored the typing style
//! by hand and still lost a pasted bold).
//!
//! Now every edit reports `runs`: the byte spans of its `text` and the
//! formats they carry OVER the block element's own style (a heading's bold
//! is the heading's, not a format of its text), and
//! `typing_formats` names the formats the text typed next at the caret takes
//! (the run under the caret, with a toggled typing style on top) - what a
//! toolbar shows as pressed.

use azul_core::{
    dom::NodeId,
    events::{TextFormat, TextFormatSet},
    selection::{DocumentTextEdit, TextFormatSpan},
};

use crate::{
    a_rich_paste_inserts_formatting_and_blocks::clipboard,
    editing_harness::{dnid, Editor, HOST},
};

/// `body(0) > host(1) > p(2) > ...`
const P: usize = 2;

fn bold() -> TextFormatSet {
    TextFormatSet {
        bold: true,
        ..TextFormatSet::default()
    }
}

/// The paragraph's unsynced edit.
fn edit_of_p(editor: &Editor) -> DocumentTextEdit {
    editor
        .lw
        .unsynced_text_edits()
        .into_iter()
        .find(|e| e.node.node.into_crate_internal() == Some(NodeId::new(P)))
        .expect("the paragraph has an unsynced text edit")
}

fn span(start: u32, end: u32, formats: TextFormatSet) -> TextFormatSpan {
    TextFormatSpan {
        start,
        end,
        formats,
    }
}

#[test]
fn text_typed_after_ctrl_b_at_a_caret_is_reported_bold() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);
    editor.type_text("ab");

    let edit = edit_of_p(&editor);
    assert_eq!(edit.text.as_str(), "fooabbar");
    assert_eq!(edit.runs.as_ref(), &[span(3, 5, bold())]);
}

#[test]
fn typing_into_formatted_text_reports_the_formats_already_there() {
    let mut editor = Editor::new("<p>a<i>b</i>c<b>d[]</b>e</p>");
    editor.type_text("X");

    let edit = edit_of_p(&editor);
    assert_eq!(edit.text.as_str(), "abcdXe");
    let italic = TextFormatSet {
        italic: true,
        ..TextFormatSet::default()
    };
    assert_eq!(
        edit.runs.as_ref(),
        &[span(1, 2, italic), span(3, 5, bold())]
    );
}

#[test]
fn pasted_bold_text_is_reported_bold() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let _ = editor
        .lw
        .paste_clipboard_content(&clipboard("a <b>b</b> c", "a b c"));
    let _ = editor.lw.apply_text_changeset();

    let edit = edit_of_p(&editor);
    assert_eq!(edit.text.as_str(), "fooa b cbar");
    assert_eq!(edit.runs.as_ref(), &[span(5, 6, bold())]);
}

#[test]
fn plain_typing_reports_no_formats() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    editor.type_text("ab");

    let edit = edit_of_p(&editor);
    assert_eq!(edit.text.as_str(), "fooabbar");
    assert!(edit.runs.as_ref().is_empty(), "{:?}", edit.runs);
}

#[test]
fn the_pending_format_at_the_caret_is_readable_before_anything_is_typed() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    assert_eq!(
        editor.lw.typing_formats(dnid(HOST)),
        Some(TextFormatSet::default()),
        "plain text, nothing toggled"
    );
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);
    assert_eq!(
        editor.lw.typing_formats(dnid(HOST)),
        Some(bold()),
        "Ctrl+B at the caret: the next text is bold"
    );
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);
    assert_eq!(
        editor.lw.typing_formats(dnid(HOST)),
        Some(TextFormatSet::default()),
        "toggled back"
    );
}

#[test]
fn a_caret_in_bold_text_has_bold_as_its_pending_format() {
    let mut editor = Editor::new("<p><b>foo[]bar</b></p>");
    assert_eq!(editor.lw.typing_formats(dnid(HOST)), Some(bold()));
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);
    assert_eq!(
        editor.lw.typing_formats(dnid(HOST)),
        Some(TextFormatSet::default()),
        "Ctrl+B inside bold text: the next text is NOT bold"
    );
}
