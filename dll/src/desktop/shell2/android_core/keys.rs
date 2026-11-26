//! The text a key types.
//!
//! A soft keyboard talks to an `InputConnection` and never sends a `KeyEvent` for a letter; a HARDWARE
//! keyboard (a tablet's dock, a Chromebook, the emulator with the host's keyboard) and the keys
//! `adb shell input text` injects send `KeyEvent`s the IME did not consume, and the shell mapped a few
//! navigation keys and dropped every other: the letters never became text ("unicode_char mapping
//! (KeyCharacterMap) still TODO").
//!
//! Those events reach the activity's `dispatchKeyEvent` after the IME and the native input queue passed
//! them by; the Java side asks `KeyEvent.getUnicodeChar()` (the character the event's own device layout
//! types, a virtual keyboard's included) and hands it here with the meta state. The native queue cannot
//! answer: `AKeyEvent` has no character.

/// `KeyEvent.META_ALT_ON`.
const META_ALT_ON: i32 = 0x0000_0002;
/// `KeyEvent.META_CTRL_ON`.
const META_CTRL_ON: i32 = 0x0000_1000;
/// `KeyEvent.META_META_ON`.
const META_META_ON: i32 = 0x0001_0000;
/// `KeyCharacterMap.COMBINING_ACCENT`: the character is a dead key's accent, not text (yet).
const COMBINING_ACCENT: i32 = i32::MIN;

/// The text a key-down types, from `KeyEvent.getUnicodeChar()` and `getMetaState()`.
///
/// - Only a key going DOWN types (a repeat is another character; the release is nothing).
/// - Not with Ctrl, Alt or Meta held: that is a shortcut (Ctrl+C, Alt+Tab), not text.
/// - Not a control character (Enter, Tab, Escape, Backspace have their own key paths), not `0` (a key
///   with no character) and not a dead key's accent (a combining accent waits for the next key).
#[must_use]
pub fn typed_text(down: bool, unicode: i32, meta_state: i32) -> Option<String> {
    if !down || unicode == 0 || unicode & COMBINING_ACCENT != 0 {
        return None;
    }
    if meta_state & (META_ALT_ON | META_CTRL_ON | META_META_ON) != 0 {
        return None;
    }
    let c = u32::try_from(unicode).ok().and_then(char::from_u32)?;
    (!c.is_control()).then(|| c.to_string())
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod tests;
