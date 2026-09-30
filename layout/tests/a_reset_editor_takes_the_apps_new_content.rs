//! An app replaces a live editor's content from code - a mail app quoting
//! the original into the reply, inserting a signature, switching between
//! plain and rich, clearing the body after Send - and the editor shows
//! the app's new content, not the typing that came before, with the caret
//! and the undo history handled sanely.
//!
//! HTML's `innerHTML = ..` / `value = ..` on an editing host: the new
//! content is the truth, the selection collapses into it and the undo
//! history of the old content is gone (the HTML Standard's "undo manager"
//! is per editing host and its transactions are cleared when the host's
//! content is replaced from script, as every browser does).
//!
//! Before: the app's new DOM was painted over by the content overlay's
//! uncommitted typing until the DOM happened to equal it
//! (`scripts/W1_INPUT_TYPES_2026_09_29.md`, "ENGINE GAP"), the caret stayed
//! in a block the app had replaced, and Ctrl+Z restored text of a document
//! that no longer existed.

use azul_core::{
    dom::NodeId,
    events::DefaultAction,
    window::{KeyboardState, VirtualKeyCode},
};

use crate::editing_harness::{dnid, host_dom, lay_out_new_generation, Editor, HOST};

/// The new content: `body(0) > host(1) > [p(2) > "Re: subject"(3),
/// blockquote(4) > p(5) > "original"(6)]`.
const REPLY: &str = "<p>Re: subject</p><blockquote><p>original</p></blockquote>";
const REPLY_P: usize = 2;
const QUOTED_P: usize = 5;

/// `<p>hello[]</p>`, then " world" typed - uncommitted typing the app never
/// folded into its model.
fn typed_editor() -> Editor {
    let mut editor = Editor::new("<p>hello[]</p>");
    editor.type_text(" world");
    assert_eq!(
        editor.markup_of(2),
        "hello world",
        "premise: the typing is in"
    );
    assert!(
        !editor.lw.unsynced_text_edits().is_empty(),
        "premise: the app has not synced it"
    );
    editor
}

/// The app replaces the content and re-renders.
fn reset_and_render(editor: &mut Editor, caret_at_end: bool) {
    assert!(
        editor.lw.reset_editor_content(dnid(HOST), caret_at_end),
        "the host is a live editor"
    );
    let (dom, ..) = host_dom(REPLY);
    lay_out_new_generation(&mut editor.lw, dom);
}

#[test]
fn the_apps_new_content_replaces_the_typing_that_came_before() {
    let mut editor = typed_editor();

    reset_and_render(&mut editor, true);

    assert_eq!(editor.markup_of(REPLY_P), "Re: subject");
    assert_eq!(editor.markup_of(QUOTED_P), "original");
    assert!(
        editor.lw.unsynced_text_edits().is_empty(),
        "nothing of the old typing is left for the app to sync"
    );
}

#[test]
fn the_caret_lands_at_the_end_of_the_new_content() {
    let mut editor = typed_editor();

    reset_and_render(&mut editor, true);

    let caret = editor.lw.document_caret().expect("the editor has a caret");
    assert_eq!(caret.node, dnid(QUOTED_P), "in the last block");
    assert_eq!(caret.text_byte, "original".len() as u32, "at its end");
}

#[test]
fn the_caret_lands_at_the_start_of_the_new_content_when_asked() {
    let mut editor = typed_editor();

    reset_and_render(&mut editor, false);

    let caret = editor.lw.document_caret().expect("the editor has a caret");
    assert_eq!(caret.node, dnid(REPLY_P), "in the first block");
    assert_eq!(caret.text_byte, 0, "at its start");
}

#[test]
fn typing_after_the_reset_goes_into_the_new_content() {
    let mut editor = typed_editor();
    reset_and_render(&mut editor, true);

    editor.type_text("!");

    assert_eq!(editor.markup_of(QUOTED_P), "original!");
    assert_eq!(editor.markup_of(REPLY_P), "Re: subject");
}

#[test]
fn the_undo_history_of_the_old_content_is_gone() {
    let mut editor = typed_editor();
    assert!(
        editor.lw.undo_redo_manager.can_undo(NodeId::new(2)),
        "premise: the typing is undoable"
    );

    reset_and_render(&mut editor, true);

    assert!(
        !editor.lw.undo_redo_manager.can_undo(NodeId::new(REPLY_P)),
        "Ctrl+Z after the reset has nothing of the old content to restore"
    );
}

/// A structural edit the app never answered (Enter, recorded and left
/// pending) does not outlive the content it was recorded against.
#[test]
fn a_pending_structural_edit_of_the_old_content_is_dropped() {
    let mut editor = typed_editor();
    let focused = Some(dnid(HOST));
    let editing = editor
        .lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    let keys = KeyboardState {
        current_virtual_keycode: Some(VirtualKeyCode::Return).into(),
        pressed_virtual_keycodes: vec![VirtualKeyCode::Return].into(),
        ..Default::default()
    };
    let action = azul_layout::default_actions::determine_keyboard_default_action_with_editing(
        &keys,
        focused,
        &editor.lw.layout_results,
        false,
        Some(&editing),
    )
    .action;
    assert!(
        matches!(action, DefaultAction::SplitBlockAtCursor { .. }),
        "premise: Enter splits the paragraph"
    );
    assert!(
        editor
            .lw
            .record_structural_default_action(&action)
            .is_some(),
        "premise: the split is recorded"
    );

    reset_and_render(&mut editor, true);

    assert!(
        editor.lw.get_pending_document_edit().is_none(),
        "the app's new content supersedes the pending split"
    );
}
