//! The text of a hardware key. No device.

use super::*;

/// A letter on a physical keyboard types: the character was never delivered, so a hardware keyboard
/// could not type at all.
///
/// Mutation: no mapping (the letter is dropped).
#[test]
fn a_letter_on_a_physical_keyboard_types() {
    assert_eq!(typed_text(true, 5, false, false, false, Some('a')).as_deref(), Some("a"));
    assert_eq!(typed_text(true, 5, false, false, false, Some('Ä')).as_deref(), Some("Ä"));
    assert_eq!(typed_text(true, 5, false, false, false, Some('€')).as_deref(), Some("€"));
}

/// A release, a shortcut, a virtual device and a control character type nothing.
///
/// Mutation: Ctrl+C typing a `c`; a key-up typing again.
#[test]
fn a_release_a_shortcut_a_virtual_device_and_a_control_character_type_nothing() {
    assert_eq!(typed_text(false, 5, false, false, false, Some('a')), None, "the release");
    assert_eq!(typed_text(true, 5, true, false, false, Some('c')), None, "Ctrl+C");
    assert_eq!(typed_text(true, 5, false, true, false, Some('x')), None, "Alt+x");
    assert_eq!(typed_text(true, 5, false, false, true, Some('v')), None, "Meta+v");
    assert_eq!(typed_text(true, 0, false, false, false, Some('a')), None, "device 0: the soft keyboard");
    assert_eq!(typed_text(true, -1, false, false, false, Some('a')), None, "the virtual keyboard");
    assert_eq!(typed_text(true, 5, false, false, false, Some('\n')), None, "Enter has its own path");
    assert_eq!(typed_text(true, 5, false, false, false, Some('\t')), None);
    assert_eq!(typed_text(true, 5, false, false, false, None), None, "a key with no character");
}
