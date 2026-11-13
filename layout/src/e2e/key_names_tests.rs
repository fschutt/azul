//! The key names `key_down` / `key_up` accept.
//!
//! A scenario types a calculator's operators, a path's separators or a
//! keypad digit by NAME (`{"op": "key_down", "key": "plus"}`). The name table
//! knew letters, digits, the editing keys, the arrows, the function keys and
//! the modifiers, so `plus`, `slash` or `numpad_multiply` parsed to nothing
//! and the step was a silent no-op: the app never saw a key. Every
//! punctuation key and every keypad key of `VirtualKeyCode` has a name now,
//! spelled as a word and (for a printable key) as its character.

use azul_core::window::VirtualKeyCode;

use super::full::parse_virtual_keycode;

#[test]
fn every_punctuation_key_has_a_word_and_a_character_name() {
    let table = [
        ("plus", "+", VirtualKeyCode::Plus),
        ("minus", "-", VirtualKeyCode::Minus),
        ("equals", "=", VirtualKeyCode::Equals),
        ("asterisk", "*", VirtualKeyCode::Asterisk),
        ("slash", "/", VirtualKeyCode::Slash),
        ("backslash", "\\", VirtualKeyCode::Backslash),
        ("period", ".", VirtualKeyCode::Period),
        ("comma", ",", VirtualKeyCode::Comma),
        ("semicolon", ";", VirtualKeyCode::Semicolon),
        ("colon", ":", VirtualKeyCode::Colon),
        ("apostrophe", "'", VirtualKeyCode::Apostrophe),
        ("grave", "`", VirtualKeyCode::Grave),
        ("lbracket", "[", VirtualKeyCode::LBracket),
        ("rbracket", "]", VirtualKeyCode::RBracket),
        ("caret", "^", VirtualKeyCode::Caret),
        ("at", "@", VirtualKeyCode::At),
        ("underline", "_", VirtualKeyCode::Underline),
    ];
    for (word, character, key) in table {
        assert_eq!(parse_virtual_keycode(word), Some(key), "the name {word:?}");
        assert_eq!(
            parse_virtual_keycode(character),
            Some(key),
            "the character {character:?}"
        );
    }
}

#[test]
fn every_keypad_key_has_a_name_with_and_without_the_underscore() {
    let digits = [
        VirtualKeyCode::Numpad0,
        VirtualKeyCode::Numpad1,
        VirtualKeyCode::Numpad2,
        VirtualKeyCode::Numpad3,
        VirtualKeyCode::Numpad4,
        VirtualKeyCode::Numpad5,
        VirtualKeyCode::Numpad6,
        VirtualKeyCode::Numpad7,
        VirtualKeyCode::Numpad8,
        VirtualKeyCode::Numpad9,
    ];
    for (n, key) in digits.into_iter().enumerate() {
        assert_eq!(parse_virtual_keycode(&format!("numpad{n}")), Some(key));
        assert_eq!(parse_virtual_keycode(&format!("numpad_{n}")), Some(key));
    }
    let operators = [
        ("add", VirtualKeyCode::NumpadAdd),
        ("subtract", VirtualKeyCode::NumpadSubtract),
        ("multiply", VirtualKeyCode::NumpadMultiply),
        ("divide", VirtualKeyCode::NumpadDivide),
        ("decimal", VirtualKeyCode::NumpadDecimal),
        ("comma", VirtualKeyCode::NumpadComma),
        ("enter", VirtualKeyCode::NumpadEnter),
        ("equals", VirtualKeyCode::NumpadEquals),
    ];
    for (name, key) in operators {
        assert_eq!(
            parse_virtual_keycode(&format!("numpad{name}")),
            Some(key),
            "numpad{name}"
        );
        assert_eq!(
            parse_virtual_keycode(&format!("numpad_{name}")),
            Some(key),
            "numpad_{name}"
        );
    }
}

#[test]
fn the_names_are_case_insensitive_like_the_letters() {
    assert_eq!(parse_virtual_keycode("Plus"), Some(VirtualKeyCode::Plus));
    assert_eq!(
        parse_virtual_keycode("NumpadMultiply"),
        Some(VirtualKeyCode::NumpadMultiply)
    );
    assert_eq!(
        parse_virtual_keycode("NUMPAD_ENTER"),
        Some(VirtualKeyCode::NumpadEnter)
    );
}

#[test]
fn the_names_that_already_worked_keep_their_meaning() {
    assert_eq!(parse_virtual_keycode("a"), Some(VirtualKeyCode::A));
    assert_eq!(parse_virtual_keycode("7"), Some(VirtualKeyCode::Key7));
    assert_eq!(parse_virtual_keycode("enter"), Some(VirtualKeyCode::Return));
    assert_eq!(parse_virtual_keycode(" "), Some(VirtualKeyCode::Space));
    assert_eq!(
        parse_virtual_keycode("backspace"),
        Some(VirtualKeyCode::Back)
    );
    assert_eq!(parse_virtual_keycode("f6"), Some(VirtualKeyCode::F6));
    assert_eq!(parse_virtual_keycode("no such key"), None);
}
