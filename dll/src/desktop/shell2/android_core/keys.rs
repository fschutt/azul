//! The text a hardware key types.
//!
//! A soft keyboard talks to an `InputConnection` and never sends a `KeyEvent` for a letter; a HARDWARE
//! keyboard (a tablet's dock, a Chromebook, the emulator with the host's keyboard) sends `KeyEvent`s the
//! IME did not consume, and the shell mapped a few navigation keys and dropped every other: the letters
//! never became text ("unicode_char mapping (KeyCharacterMap) still TODO"). `KeyCharacterMap.get(keyCode,
//! metaState)` turns such an event into the character the layout types.

/// The text a key-down types, from the character the device's `KeyCharacterMap` gives for it.
///
/// - Only a key going DOWN types (a repeat is another character; the release is nothing).
/// - Not from a virtual device: `0` and the framework's `VIRTUAL_KEYBOARD` (`-1`) are the soft keyboard
///   and injected events, which have no map (asking for one raises a Java exception) and whose text
///   comes through the `InputConnection`.
/// - Not with Ctrl, Alt or Meta held: that is a shortcut (Ctrl+C, Alt+Tab), not text.
/// - Not a control character (Enter, Tab, Escape, Backspace have their own key paths).
#[must_use]
pub fn typed_text(
    down: bool,
    device_id: i32,
    ctrl: bool,
    alt: bool,
    meta: bool,
    mapped: Option<char>,
) -> Option<String> {
    if !down || device_id <= 0 || ctrl || alt || meta {
        return None;
    }
    let c = mapped?;
    (!c.is_control()).then(|| c.to_string())
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod tests;
