//! Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z and Ctrl/Cmd+Y redo - and an editor
//! that keeps its OWN history (the RichTextEditor, a document app) takes the
//! keys for it.
//!
//! The browser model (UI Events, "default actions"): the keydown is
//! dispatched to the page first; the built-in undo of an editing host runs
//! after it, unless a handler called `preventDefault()` - that is how
//! ProseMirror, Google Docs and every rich editor own undo. The engine's
//! text undo is therefore the key's DEFAULT ACTION, decided here next to
//! Ctrl/Cmd+B's format toggle and run by the shells after the callbacks.
//!
//! Before: core's interpreter claimed the keys before any callback
//! (`AddAndSkip` of `SystemChange::UndoTextEdit`), so an app's history was
//! unreachable from the keyboard (RTE report, section 6).

use azul_core::{
    events::DefaultAction,
    window::{KeyboardState, VirtualKeyCode},
};
use azul_layout::default_actions::{
    determine_keyboard_default_action_with_editing, EditingQueryState,
};

use crate::editing_harness::{dnid, Editor, HOST};

fn primary() -> VirtualKeyCode {
    if azul_core::window::mac_shortcut_conventions() {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    }
}

/// The keyboard with `key` pressed and the primary modifier (and Shift) held.
fn keys(key: VirtualKeyCode, shift: bool) -> KeyboardState {
    let mut pressed = vec![primary(), key];
    if shift {
        pressed.push(VirtualKeyCode::LShift);
    }
    KeyboardState {
        current_virtual_keycode: Some(key).into(),
        pressed_virtual_keycodes: pressed.into(),
        ..Default::default()
    }
}

#[test]
fn the_primary_modifier_with_z_or_y_in_an_editing_host_is_the_text_undo_or_redo() {
    let editor = Editor::new("<p>foo[]bar</p>");
    let focused = Some(dnid(HOST));
    let editing = editor
        .lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    for (key, shift, expected) in [
        (
            VirtualKeyCode::Z,
            false,
            DefaultAction::UndoTextEdit { target: dnid(HOST) },
        ),
        (
            VirtualKeyCode::Z,
            true,
            DefaultAction::RedoTextEdit { target: dnid(HOST) },
        ),
        (
            VirtualKeyCode::Y,
            false,
            DefaultAction::RedoTextEdit { target: dnid(HOST) },
        ),
    ] {
        let result = determine_keyboard_default_action_with_editing(
            &keys(key, shift),
            focused,
            &editor.lw.layout_results,
            false,
            Some(&editing),
        );
        assert_eq!(result.action, expected, "{key:?} shift={shift}");
    }
}

/// The editor that owns its history calls `prevent_default` in its key
/// handler: no text undo runs behind its back.
#[test]
fn a_prevented_undo_key_runs_no_text_undo() {
    let editor = Editor::new("<p>foo[]bar</p>");
    let focused = Some(dnid(HOST));
    let editing = editor
        .lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    for (key, shift) in [
        (VirtualKeyCode::Z, false),
        (VirtualKeyCode::Z, true),
        (VirtualKeyCode::Y, false),
    ] {
        let result = determine_keyboard_default_action_with_editing(
            &keys(key, shift),
            focused,
            &editor.lw.layout_results,
            true,
            Some(&editing),
        );
        assert!(
            !result.has_action(),
            "{key:?} shift={shift}: a vetoed key has no default action, got {:?}",
            result.action
        );
    }
}

/// Outside text editing the keys are the app's alone (a canvas app's
/// Ctrl+Z): there is no text to undo.
#[test]
fn the_undo_keys_outside_an_editing_host_have_no_default_action() {
    let editor = Editor::new("<p>foo[]bar</p>");
    let not_editing = EditingQueryState::default();
    for (key, shift) in [
        (VirtualKeyCode::Z, false),
        (VirtualKeyCode::Z, true),
        (VirtualKeyCode::Y, false),
    ] {
        for editing in [None, Some(&not_editing)] {
            let result = determine_keyboard_default_action_with_editing(
                &keys(key, shift),
                Some(dnid(0)),
                &editor.lw.layout_results,
                false,
                editing,
            );
            assert_eq!(
                result.action,
                DefaultAction::None,
                "{key:?} shift={shift} on a non-editable focus"
            );
        }
    }
}
