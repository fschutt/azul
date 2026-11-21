//! Ctrl/Cmd+B, I or U with no selection, then typing: the typed text takes
//! the format. The "typing style" of every rich editor.
//!
//! The execCommand spec keeps it as the command's STATE OVERRIDE
//! (<https://w3c.github.io/editing/docs/execCommand/#overrides>): `bold` on a
//! collapsed selection toggles the override, the next insertion takes it, and
//! it is unset "whenever a boundary point of the range at a given index in
//! the selection changes". WPT pins the pairs in
//! `editing/data/multitest.js` (`[["bold",""],["inserttext","a"]]` on
//! `foo[]bar` gives `foo<b>a</b>bar`); the rows below are those, in the
//! fixture's markup (`tests/common/editing_harness.rs`).
//!
//! Before: nothing kept a format at a caret - the typed text went into the
//! styled run under the caret (`text3::edit::insert_text`) and Ctrl+B was
//! not a key the engine knew.

use azul_core::{
    events::{
        DefaultAction, SelectionDirection, SelectionMode, SelectionOp, SelectionStep, TextFormat,
    },
    window::{KeyboardState, VirtualKeyCode},
};

use crate::editing_harness::{dnid, Editor, HOST};

/// `body(0) > host(1) > p(2) > ...`
const P: usize = 2;

/// Rows: the paragraph with a caret, the toggles at the caret, the text
/// typed after them, the paragraph afterwards.
const ROWS: &[(&str, &[TextFormat], &str, &str)] = &[
    // multitest.js: bold, then inserttext.
    (
        "<p>foo[]bar</p>",
        &[TextFormat::Bold],
        "a",
        "foo<b>a</b>bar",
    ),
    // multitest.js: italic / underline, then inserttext.
    (
        "<p>foo[]bar</p>",
        &[TextFormat::Italic],
        "a",
        "foo<i>a</i>bar",
    ),
    (
        "<p>foo[]bar</p>",
        &[TextFormat::Underline],
        "a",
        "foo<u>a</u>bar",
    ),
    // Two overrides at once.
    (
        "<p>foo[]bar</p>",
        &[TextFormat::Bold, TextFormat::Italic],
        "a",
        "foo<b><i>a</i></b>bar",
    ),
    // Toggled twice: no override left.
    (
        "<p>foo[]bar</p>",
        &[TextFormat::Bold, TextFormat::Bold],
        "a",
        "fooabar",
    ),
    // Inside bold text the toggle turns bold OFF for what is typed.
    (
        "<p><b>foo[]bar</b></p>",
        &[TextFormat::Bold],
        "a",
        "<b>foo</b>a<b>bar</b>",
    ),
    // At the end and at the start of the text.
    ("<p>foo[]</p>", &[TextFormat::Bold], "a", "foo<b>a</b>"),
    ("<p>[]foo</p>", &[TextFormat::Italic], "a", "<i>a</i>foo"),
];

#[test]
fn a_format_toggle_at_a_caret_formats_the_text_typed_next() {
    for &(input, toggles, typed, expected) in ROWS {
        let mut editor = Editor::new(input);
        for format in toggles {
            let _ = editor.lw.toggle_text_format(dnid(HOST), *format);
        }
        editor.type_text(typed);
        assert_eq!(
            editor.markup_of(P),
            expected,
            "{input} + {toggles:?} + type {typed:?}"
        );
    }
}

#[test]
fn the_typed_text_keeps_the_format_as_typing_goes_on() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);

    editor.type_text("a");
    editor.type_text("b");

    assert_eq!(editor.markup_of(P), "foo<b>ab</b>bar");
}

/// The override is unset when the caret moves, even back to where it was:
/// Right, then Left, then typing types plain text.
#[test]
fn the_typing_style_is_dropped_when_the_caret_moves() {
    let mut editor = Editor::new("<p>foo[]bar</p>");
    let _ = editor.lw.toggle_text_format(dnid(HOST), TextFormat::Bold);

    for direction in [SelectionDirection::Forward, SelectionDirection::Backward] {
        let _ = editor.lw.apply_selection_op(
            dnid(HOST),
            &SelectionOp::new(direction, SelectionStep::Character, SelectionMode::Move),
        );
    }
    editor.type_text("a");

    assert_eq!(editor.markup_of(P), "fooabar");
}

/// Ctrl+B (Cmd+B on macOS) in a rich editing host is the bold toggle's key:
/// the default action the shells run after the key's callbacks, so an app
/// that handles the key itself vetoes it with `prevent_default`.
#[test]
fn the_primary_modifier_with_b_i_or_u_toggles_the_format() {
    let editor = Editor::new("<p>foo[]bar</p>");
    let primary = if azul_core::window::mac_shortcut_conventions() {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    };
    let focused = Some(dnid(HOST));
    let editing = editor
        .lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    for (key, format) in [
        (VirtualKeyCode::B, TextFormat::Bold),
        (VirtualKeyCode::I, TextFormat::Italic),
        (VirtualKeyCode::U, TextFormat::Underline),
    ] {
        let keys = KeyboardState {
            current_virtual_keycode: Some(key).into(),
            pressed_virtual_keycodes: vec![primary, key].into(),
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
        assert_eq!(
            action,
            DefaultAction::ToggleTextFormat {
                target: dnid(HOST),
                format,
            },
            "{key:?} with the primary modifier"
        );
    }
}
