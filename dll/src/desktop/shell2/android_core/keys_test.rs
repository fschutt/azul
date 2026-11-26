//! The text of a key. No device.

use super::*;

const SHIFT: i32 = 0x1;

/// A letter on a keyboard types: the character was never delivered, so a hardware keyboard (and
/// `adb shell input text`) could not type at all.
///
/// Mutation: no mapping (the letter is dropped).
#[test]
fn a_letter_types() {
    assert_eq!(typed_text(true, 'a' as i32, 0).as_deref(), Some("a"));
    assert_eq!(typed_text(true, 'A' as i32, SHIFT).as_deref(), Some("A"), "shift is not a shortcut");
    assert_eq!(typed_text(true, 'Ä' as i32, 0).as_deref(), Some("Ä"));
    assert_eq!(typed_text(true, '€' as i32, 0).as_deref(), Some("€"));
    assert_eq!(typed_text(true, ' ' as i32, 0).as_deref(), Some(" "));
    assert_eq!(typed_text(true, '😀' as i32, 0).as_deref(), Some("😀"), "a code point beyond the BMP");
}

/// A release, a shortcut, a control character, a key with no character and a dead key type nothing.
///
/// Mutation: Ctrl+C typing a `c`; a key-up typing again.
#[test]
fn a_release_a_shortcut_a_control_character_and_a_dead_key_type_nothing() {
    assert_eq!(typed_text(false, 'a' as i32, 0), None, "the release");
    assert_eq!(typed_text(true, 'c' as i32, 0x1000), None, "Ctrl+C");
    assert_eq!(typed_text(true, 'x' as i32, 0x2), None, "Alt+x");
    assert_eq!(typed_text(true, 'v' as i32, 0x10000), None, "Meta+v");
    assert_eq!(typed_text(true, '\n' as i32, 0), None, "Enter has its own path");
    assert_eq!(typed_text(true, '\t' as i32, 0), None);
    assert_eq!(typed_text(true, 0x08, 0), None, "backspace");
    assert_eq!(typed_text(true, 0, 0), None, "a key with no character");
    assert_eq!(typed_text(true, i32::MIN | 0x60, 0), None, "a dead key's accent waits for the next key");
    assert_eq!(typed_text(true, 0xD800, 0), None, "a lone surrogate is no character");
}
