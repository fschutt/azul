//! Typed and pasted text is read by the input rule.
//!
//! Text that comes in from a keyboard, an input method or the clipboard holds its characters
//! as the one input rule has them (`azul_css::input_chars`), as the text a document brings
//! does: NUL, the C0 controls but tab and line feed, U+FFFE and U+FFFF become U+FFFD, so none
//! of them reaches the text model, the shaper or the accessibility tree.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::dom::NodeId;
use azul_layout::managers::selection::ClipboardContent;

use crate::{a_rich_paste_inserts_formatting_and_blocks::clipboard, editing_harness::Editor};

/// `body(0) > host(1) > p(2) > ...`
const P: usize = 2;

/// The paragraph's text, as the edit left it.
fn text_of_p(editor: &Editor) -> String {
    editor
        .lw
        .unsynced_text_edits()
        .into_iter()
        .find(|e| e.node.node.into_crate_internal() == Some(NodeId::new(P)))
        .expect("the paragraph has an unsynced text edit")
        .text
        .as_str()
        .to_string()
}

#[test]
fn typed_text_holds_its_characters_as_the_input_rule_has_them() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    editor.type_text("x\u{0}y\u{FFFE}z\u{1}");
    assert_eq!(text_of_p(&editor), "foox\u{FFFD}y\u{FFFD}z\u{FFFD}bar");
}

#[test]
fn pasted_plain_text_holds_its_characters_as_the_input_rule_has_them() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let content = ClipboardContent {
        plain_text: "x\u{0}y\u{FFFF}z".into(),
        ..Default::default()
    };
    let _ = editor.lw.paste_clipboard_content(&content);
    let _ = editor.lw.apply_text_changeset();
    assert_eq!(text_of_p(&editor), "foox\u{FFFD}y\u{FFFD}zbar");
}

#[test]
fn pasted_html_holds_its_characters_as_the_input_rule_has_them() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let _ = editor
        .lw
        .paste_clipboard_content(&clipboard("x\u{1}<b>y&#xFFFE;</b>z", "x y z"));
    let _ = editor.lw.apply_text_changeset();
    assert_eq!(text_of_p(&editor), "foox\u{FFFD}y\u{FFFD}zbar");
}
