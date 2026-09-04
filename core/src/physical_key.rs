//! Scancode -> [`PhysicalKey`] tables, one per platform convention.
//!
//! `ScanCode` already carried the physical key as a raw `u32` on every
//! backend, so nothing new has to be plumbed to fill
//! `KeyboardState.current_physical_key` — what was missing is the tables that
//! turn a platform-specific number into a name an application can match on.
//!
//! Three conventions cover every backend the engine has:
//!
//! * **evdev** (`from_evdev`) — Wayland's `wl_keyboard.key` IS an evdev code,
//!   X11 keycodes are `evdev + 8`, and Android's `KeyEvent` scan code is evdev
//!   too. One table serves all three.
//! * **PS/2 set 1** (`from_windows_scancode`) — what `WM_KEYDOWN`'s `lParam`
//!   carries, where the `E0` prefix (bit 24 of `lParam`) is what separates
//!   NumpadEnter from Enter, ControlRight from ControlLeft, and the arrow
//!   cluster from the numpad.
//! * **Carbon virtual keycodes** (`from_macos_keycode`) — `NSEvent.keyCode`.
//!   These are POSITIONAL despite the name; `kVK_ANSI_A` is the position, not
//!   the letter.
//!
//! Every table returns [`PhysicalKey::Unidentified`] for a code it does not
//! name, which is a value the enum carries precisely so that an unknown key is
//! reported honestly rather than guessed at or dropped.

use crate::window::PhysicalKey;

impl PhysicalKey {
    /// Linux evdev keycode -> position.
    ///
    /// Used directly by Wayland and Android. X11 callers must pass
    /// `keycode - 8`; see [`Self::from_x11_keycode`].
    #[must_use]
    // A transcription of the evdev code table, in code order. Distinct codes do
    // map to one key (95 and 121 are both NumpadComma); reordering them into
    // or-patterns would stop this reading like the table it mirrors.
    #[allow(clippy::match_same_arms)]
    pub const fn from_evdev(code: u32) -> Self {
        use PhysicalKey::{Escape, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Digit0, Minus, Equal, Backspace, Tab, KeyQ, KeyW, KeyE, KeyR, KeyT, KeyY, KeyU, KeyI, KeyO, KeyP, BracketLeft, BracketRight, Enter, ControlLeft, KeyA, KeyS, KeyD, KeyF, KeyG, KeyH, KeyJ, KeyK, KeyL, Semicolon, Quote, Backquote, ShiftLeft, Backslash, KeyZ, KeyX, KeyC, KeyV, KeyB, KeyN, KeyM, Comma, Period, Slash, ShiftRight, NumpadMultiply, AltLeft, Space, CapsLock, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, NumLock, ScrollLock, Numpad7, Numpad8, Numpad9, NumpadSubtract, Numpad4, Numpad5, Numpad6, NumpadAdd, Numpad1, Numpad2, Numpad3, Numpad0, NumpadDecimal, IntlBackslash, F11, F12, IntlRo, Convert, KanaMode, NonConvert, NumpadComma, NumpadEnter, ControlRight, NumpadDivide, PrintScreen, AltRight, Home, ArrowUp, PageUp, ArrowLeft, ArrowRight, End, ArrowDown, PageDown, Insert, Delete, NumpadEqual, Pause, Lang1, Lang2, IntlYen, MetaLeft, MetaRight, ContextMenu, F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24, Unidentified};
        match code {
            1 => Escape,
            2 => Digit1, 3 => Digit2, 4 => Digit3, 5 => Digit4, 6 => Digit5,
            7 => Digit6, 8 => Digit7, 9 => Digit8, 10 => Digit9, 11 => Digit0,
            12 => Minus, 13 => Equal, 14 => Backspace, 15 => Tab,
            16 => KeyQ, 17 => KeyW, 18 => KeyE, 19 => KeyR, 20 => KeyT,
            21 => KeyY, 22 => KeyU, 23 => KeyI, 24 => KeyO, 25 => KeyP,
            26 => BracketLeft, 27 => BracketRight, 28 => Enter, 29 => ControlLeft,
            30 => KeyA, 31 => KeyS, 32 => KeyD, 33 => KeyF, 34 => KeyG,
            35 => KeyH, 36 => KeyJ, 37 => KeyK, 38 => KeyL,
            39 => Semicolon, 40 => Quote, 41 => Backquote, 42 => ShiftLeft, 43 => Backslash,
            44 => KeyZ, 45 => KeyX, 46 => KeyC, 47 => KeyV, 48 => KeyB,
            49 => KeyN, 50 => KeyM, 51 => Comma, 52 => Period, 53 => Slash,
            54 => ShiftRight, 55 => NumpadMultiply, 56 => AltLeft, 57 => Space, 58 => CapsLock,
            59 => F1, 60 => F2, 61 => F3, 62 => F4, 63 => F5,
            64 => F6, 65 => F7, 66 => F8, 67 => F9, 68 => F10,
            69 => NumLock, 70 => ScrollLock,
            71 => Numpad7, 72 => Numpad8, 73 => Numpad9, 74 => NumpadSubtract,
            75 => Numpad4, 76 => Numpad5, 77 => Numpad6, 78 => NumpadAdd,
            79 => Numpad1, 80 => Numpad2, 81 => Numpad3, 82 => Numpad0, 83 => NumpadDecimal,
            // 86 is KEY_102ND, the extra key ISO boards have next to left
            // shift that ANSI boards do not.
            86 => IntlBackslash, 87 => F11, 88 => F12,
            89 => IntlRo,
            92 => Convert, 93 => KanaMode, 94 => NonConvert, 95 => NumpadComma,
            96 => NumpadEnter, 97 => ControlRight, 98 => NumpadDivide,
            99 => PrintScreen, 100 => AltRight,
            102 => Home, 103 => ArrowUp, 104 => PageUp, 105 => ArrowLeft,
            106 => ArrowRight, 107 => End, 108 => ArrowDown, 109 => PageDown,
            110 => Insert, 111 => Delete,
            117 => NumpadEqual, 119 => Pause,
            121 => NumpadComma, 122 => Lang1, 123 => Lang2, 124 => IntlYen,
            125 => MetaLeft, 126 => MetaRight, 127 => ContextMenu,
            183 => F13, 184 => F14, 185 => F15, 186 => F16, 187 => F17, 188 => F18,
            189 => F19, 190 => F20, 191 => F21, 192 => F22, 193 => F23, 194 => F24,
            _ => Unidentified,
        }
    }

    /// X11 keycode -> position.
    ///
    /// XKB keycodes are `evdev + 8` by protocol, so this is the evdev table
    /// with the offset removed. A keycode below 8 cannot be an evdev key and
    /// is reported as unidentified rather than wrapping into a wrong one.
    #[must_use]
    pub const fn from_x11_keycode(keycode: u32) -> Self {
        if keycode < 8 {
            return Self::Unidentified;
        }
        Self::from_evdev(keycode - 8)
    }

    /// PS/2 set-1 scancode -> position, as `WM_KEYDOWN` reports it.
    ///
    /// `extended` is `lParam` bit 24 (the `E0` prefix). It is not optional
    /// detail: without it Enter and `NumpadEnter`, `ControlLeft` and `ControlRight`,
    /// and the whole arrow cluster versus the numpad are the SAME scancode.
    #[must_use]
    pub const fn from_windows_scancode(scancode: u32, extended: bool) -> Self {
        use PhysicalKey::{NumpadEnter, ControlRight, NumpadDivide, PrintScreen, AltRight, Pause, Home, ArrowUp, PageUp, ArrowLeft, ArrowRight, End, ArrowDown, PageDown, Insert, Delete, MetaLeft, MetaRight, ContextMenu, Unidentified, Escape, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Digit0, Minus, Equal, Backspace, Tab, KeyQ, KeyW, KeyE, KeyR, KeyT, KeyY, KeyU, KeyI, KeyO, KeyP, BracketLeft, BracketRight, Enter, ControlLeft, KeyA, KeyS, KeyD, KeyF, KeyG, KeyH, KeyJ, KeyK, KeyL, Semicolon, Quote, Backquote, ShiftLeft, Backslash, KeyZ, KeyX, KeyC, KeyV, KeyB, KeyN, KeyM, Comma, Period, Slash, ShiftRight, NumpadMultiply, AltLeft, Space, CapsLock, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, NumLock, ScrollLock, Numpad7, Numpad8, Numpad9, NumpadSubtract, Numpad4, Numpad5, Numpad6, NumpadAdd, Numpad1, Numpad2, Numpad3, Numpad0, NumpadDecimal, IntlBackslash, F11, F12, NumpadEqual, F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24, KanaMode, IntlRo, Convert, NonConvert, IntlYen};
        if extended {
            return match scancode {
                0x1C => NumpadEnter,
                0x1D => ControlRight,
                0x35 => NumpadDivide,
                0x37 => PrintScreen,
                0x38 => AltRight,
                0x45 => Pause,
                0x47 => Home,
                0x48 => ArrowUp,
                0x49 => PageUp,
                0x4B => ArrowLeft,
                0x4D => ArrowRight,
                0x4F => End,
                0x50 => ArrowDown,
                0x51 => PageDown,
                0x52 => Insert,
                0x53 => Delete,
                0x5B => MetaLeft,
                0x5C => MetaRight,
                0x5D => ContextMenu,
                _ => Unidentified,
            };
        }
        match scancode {
            0x01 => Escape,
            0x02 => Digit1, 0x03 => Digit2, 0x04 => Digit3, 0x05 => Digit4, 0x06 => Digit5,
            0x07 => Digit6, 0x08 => Digit7, 0x09 => Digit8, 0x0A => Digit9, 0x0B => Digit0,
            0x0C => Minus, 0x0D => Equal, 0x0E => Backspace, 0x0F => Tab,
            0x10 => KeyQ, 0x11 => KeyW, 0x12 => KeyE, 0x13 => KeyR, 0x14 => KeyT,
            0x15 => KeyY, 0x16 => KeyU, 0x17 => KeyI, 0x18 => KeyO, 0x19 => KeyP,
            0x1A => BracketLeft, 0x1B => BracketRight, 0x1C => Enter, 0x1D => ControlLeft,
            0x1E => KeyA, 0x1F => KeyS, 0x20 => KeyD, 0x21 => KeyF, 0x22 => KeyG,
            0x23 => KeyH, 0x24 => KeyJ, 0x25 => KeyK, 0x26 => KeyL,
            0x27 => Semicolon, 0x28 => Quote, 0x29 => Backquote, 0x2A => ShiftLeft,
            0x2B => Backslash,
            0x2C => KeyZ, 0x2D => KeyX, 0x2E => KeyC, 0x2F => KeyV, 0x30 => KeyB,
            0x31 => KeyN, 0x32 => KeyM, 0x33 => Comma, 0x34 => Period, 0x35 => Slash,
            0x36 => ShiftRight, 0x37 => NumpadMultiply, 0x38 => AltLeft,
            0x39 => Space, 0x3A => CapsLock,
            0x3B => F1, 0x3C => F2, 0x3D => F3, 0x3E => F4, 0x3F => F5,
            0x40 => F6, 0x41 => F7, 0x42 => F8, 0x43 => F9, 0x44 => F10,
            0x45 => NumLock, 0x46 => ScrollLock,
            0x47 => Numpad7, 0x48 => Numpad8, 0x49 => Numpad9, 0x4A => NumpadSubtract,
            0x4B => Numpad4, 0x4C => Numpad5, 0x4D => Numpad6, 0x4E => NumpadAdd,
            0x4F => Numpad1, 0x50 => Numpad2, 0x51 => Numpad3, 0x52 => Numpad0,
            0x53 => NumpadDecimal,
            0x56 => IntlBackslash, 0x57 => F11, 0x58 => F12, 0x59 => NumpadEqual,
            0x64 => F13, 0x65 => F14, 0x66 => F15, 0x67 => F16, 0x68 => F17,
            0x69 => F18, 0x6A => F19, 0x6B => F20, 0x6C => F21, 0x6D => F22,
            0x6E => F23, 0x76 => F24,
            0x70 => KanaMode, 0x73 => IntlRo, 0x79 => Convert, 0x7B => NonConvert,
            0x7D => IntlYen,
            _ => Unidentified,
        }
    }

    /// Carbon virtual keycode (`NSEvent.keyCode`) -> position.
    ///
    /// Cross-checked entry by entry against
    /// `macos_keycode_to_virtual_key`, the table this codebase already
    /// trusts for the LOGICAL key; the two agree on every code both name.
    #[must_use]
    pub const fn from_macos_keycode(keycode: u16) -> Self {
        use PhysicalKey::{KeyA, KeyS, KeyD, KeyF, KeyH, KeyG, KeyZ, KeyX, KeyC, KeyV, IntlBackslash, KeyB, KeyQ, KeyW, KeyE, KeyR, KeyY, KeyT, Digit1, Digit2, Digit3, Digit4, Digit6, Digit5, Equal, Digit9, Digit7, Minus, Digit8, Digit0, BracketRight, KeyO, KeyU, BracketLeft, KeyI, KeyP, Enter, KeyL, KeyJ, Quote, KeyK, Semicolon, Backslash, Comma, Slash, KeyN, KeyM, Period, Tab, Space, Backquote, Backspace, Escape, MetaRight, MetaLeft, ShiftLeft, CapsLock, AltLeft, ControlLeft, ShiftRight, AltRight, ControlRight, F17, NumpadDecimal, NumpadMultiply, NumpadAdd, NumLock, NumpadDivide, NumpadEnter, NumpadSubtract, F18, F19, NumpadEqual, Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, F20, Numpad8, Numpad9, IntlYen, IntlRo, NumpadComma, F5, F6, F7, F3, F8, F9, Lang2, F11, Lang1, F13, F16, F14, F10, ContextMenu, F12, F15, Insert, Home, PageUp, Delete, F4, End, F2, PageDown, F1, ArrowLeft, ArrowRight, ArrowDown, ArrowUp, Unidentified};
        match keycode {
            0x00 => KeyA, 0x01 => KeyS, 0x02 => KeyD, 0x03 => KeyF, 0x04 => KeyH,
            0x05 => KeyG, 0x06 => KeyZ, 0x07 => KeyX, 0x08 => KeyC, 0x09 => KeyV,
            // kVK_ISO_Section — the extra ISO key, absent on ANSI boards.
            0x0A => IntlBackslash,
            0x0B => KeyB, 0x0C => KeyQ, 0x0D => KeyW, 0x0E => KeyE, 0x0F => KeyR,
            0x10 => KeyY, 0x11 => KeyT,
            0x12 => Digit1, 0x13 => Digit2, 0x14 => Digit3, 0x15 => Digit4,
            0x16 => Digit6, 0x17 => Digit5, 0x18 => Equal, 0x19 => Digit9,
            0x1A => Digit7, 0x1B => Minus, 0x1C => Digit8, 0x1D => Digit0,
            0x1E => BracketRight, 0x1F => KeyO, 0x20 => KeyU, 0x21 => BracketLeft,
            0x22 => KeyI, 0x23 => KeyP, 0x24 => Enter, 0x25 => KeyL, 0x26 => KeyJ,
            0x27 => Quote, 0x28 => KeyK, 0x29 => Semicolon, 0x2A => Backslash,
            0x2B => Comma, 0x2C => Slash, 0x2D => KeyN, 0x2E => KeyM, 0x2F => Period,
            0x30 => Tab, 0x31 => Space, 0x32 => Backquote, 0x33 => Backspace,
            0x35 => Escape,
            0x36 => MetaRight, 0x37 => MetaLeft, 0x38 => ShiftLeft, 0x39 => CapsLock,
            0x3A => AltLeft, 0x3B => ControlLeft, 0x3C => ShiftRight, 0x3D => AltRight,
            0x3E => ControlRight,
            // 0x3F is kVK_Function (the `fn` key), which has no W3C `code`.
            0x40 => F17, 0x41 => NumpadDecimal, 0x43 => NumpadMultiply, 0x45 => NumpadAdd,
            // kVK_ANSI_KeypadClear sits where NumLock does on a PC board.
            0x47 => NumLock,
            0x4B => NumpadDivide, 0x4C => NumpadEnter, 0x4E => NumpadSubtract,
            0x4F => F18, 0x50 => F19, 0x51 => NumpadEqual,
            0x52 => Numpad0, 0x53 => Numpad1, 0x54 => Numpad2, 0x55 => Numpad3,
            0x56 => Numpad4, 0x57 => Numpad5, 0x58 => Numpad6, 0x59 => Numpad7,
            0x5A => F20, 0x5B => Numpad8, 0x5C => Numpad9,
            0x5D => IntlYen, 0x5E => IntlRo, 0x5F => NumpadComma,
            0x60 => F5, 0x61 => F6, 0x62 => F7, 0x63 => F3, 0x64 => F8, 0x65 => F9,
            0x66 => Lang2, 0x67 => F11, 0x68 => Lang1, 0x69 => F13, 0x6A => F16,
            0x6B => F14, 0x6D => F10, 0x6E => ContextMenu, 0x6F => F12,
            0x71 => F15, 0x72 => Insert, 0x73 => Home, 0x74 => PageUp, 0x75 => Delete,
            0x76 => F4, 0x77 => End, 0x78 => F2, 0x79 => PageDown, 0x7A => F1,
            0x7B => ArrowLeft, 0x7C => ArrowRight, 0x7D => ArrowDown, 0x7E => ArrowUp,
            _ => Unidentified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same physical key must get the same name from every platform's
    /// table — that is the entire point of the enum, and a table typo would
    /// otherwise only show up as a wrong binding on one OS.
    #[test]
    fn the_same_position_gets_the_same_name_on_every_platform() {
        // (name, evdev, windows set-1, macOS Carbon)
        let cases: &[(PhysicalKey, u32, u32, u16)] = &[
            (PhysicalKey::KeyA, 30, 0x1E, 0x00),
            (PhysicalKey::KeyW, 17, 0x11, 0x0D),
            (PhysicalKey::KeyZ, 44, 0x2C, 0x06),
            (PhysicalKey::Digit1, 2, 0x02, 0x12),
            (PhysicalKey::Digit0, 11, 0x0B, 0x1D),
            (PhysicalKey::Space, 57, 0x39, 0x31),
            (PhysicalKey::Enter, 28, 0x1C, 0x24),
            (PhysicalKey::Tab, 15, 0x0F, 0x30),
            (PhysicalKey::Escape, 1, 0x01, 0x35),
            (PhysicalKey::Backspace, 14, 0x0E, 0x33),
            (PhysicalKey::ShiftLeft, 42, 0x2A, 0x38),
            (PhysicalKey::ShiftRight, 54, 0x36, 0x3C),
            (PhysicalKey::CapsLock, 58, 0x3A, 0x39),
            (PhysicalKey::F1, 59, 0x3B, 0x7A),
            (PhysicalKey::F12, 88, 0x58, 0x6F),
            (PhysicalKey::Numpad0, 82, 0x52, 0x52),
            (PhysicalKey::NumpadAdd, 78, 0x4E, 0x45),
            (PhysicalKey::Comma, 51, 0x33, 0x2B),
            (PhysicalKey::Slash, 53, 0x35, 0x2C),
            (PhysicalKey::Backquote, 41, 0x29, 0x32),
            (PhysicalKey::IntlBackslash, 86, 0x56, 0x0A),
        ];
        for (want, evdev, win, mac) in cases {
            assert_eq!(PhysicalKey::from_evdev(*evdev), *want, "evdev {evdev}");
            assert_eq!(
                PhysicalKey::from_windows_scancode(*win, false),
                *want,
                "windows {win:#04X}",
            );
            assert_eq!(PhysicalKey::from_macos_keycode(*mac), *want, "macos {mac:#04X}");
            // X11 is the evdev table shifted by the protocol's +8.
            assert_eq!(PhysicalKey::from_x11_keycode(evdev + 8), *want, "x11 {}", evdev + 8);
        }
    }

    /// Without the `E0` prefix these pairs are the SAME scancode, so dropping
    /// `extended` would silently merge them.
    #[test]
    fn the_windows_extended_bit_separates_the_duplicated_scancodes() {
        let pairs: &[(u32, PhysicalKey, PhysicalKey)] = &[
            (0x1C, PhysicalKey::Enter, PhysicalKey::NumpadEnter),
            (0x1D, PhysicalKey::ControlLeft, PhysicalKey::ControlRight),
            (0x35, PhysicalKey::Slash, PhysicalKey::NumpadDivide),
            (0x38, PhysicalKey::AltLeft, PhysicalKey::AltRight),
            (0x45, PhysicalKey::NumLock, PhysicalKey::Pause),
            (0x47, PhysicalKey::Numpad7, PhysicalKey::Home),
            (0x48, PhysicalKey::Numpad8, PhysicalKey::ArrowUp),
            (0x4B, PhysicalKey::Numpad4, PhysicalKey::ArrowLeft),
            (0x4D, PhysicalKey::Numpad6, PhysicalKey::ArrowRight),
            (0x50, PhysicalKey::Numpad2, PhysicalKey::ArrowDown),
            (0x52, PhysicalKey::Numpad0, PhysicalKey::Insert),
            (0x53, PhysicalKey::NumpadDecimal, PhysicalKey::Delete),
        ];
        for (sc, plain, ext) in pairs {
            assert_eq!(PhysicalKey::from_windows_scancode(*sc, false), *plain, "{sc:#04X}");
            assert_eq!(PhysicalKey::from_windows_scancode(*sc, true), *ext, "E0 {sc:#04X}");
            assert_ne!(plain, ext);
        }
    }

    /// An unknown code is reported as unidentified, never guessed at and never
    /// silently turned into a neighbouring key.
    #[test]
    fn an_unnamed_code_is_unidentified() {
        assert_eq!(PhysicalKey::from_evdev(0), PhysicalKey::Unidentified);
        assert_eq!(PhysicalKey::from_evdev(9999), PhysicalKey::Unidentified);
        assert_eq!(PhysicalKey::from_windows_scancode(0xFE, false), PhysicalKey::Unidentified);
        assert_eq!(PhysicalKey::from_windows_scancode(0x02, true), PhysicalKey::Unidentified);
        assert_eq!(PhysicalKey::from_macos_keycode(0xFF), PhysicalKey::Unidentified);
        // kVK_Function has no W3C `code` and must not be invented.
        assert_eq!(PhysicalKey::from_macos_keycode(0x3F), PhysicalKey::Unidentified);
    }

    /// X11 keycodes are `evdev + 8`; below that they cannot be evdev keys and
    /// must not wrap into a wrong one.
    #[test]
    fn an_x11_keycode_below_the_offset_cannot_wrap() {
        for kc in 0..8 {
            assert_eq!(PhysicalKey::from_x11_keycode(kc), PhysicalKey::Unidentified, "{kc}");
        }
        assert_eq!(PhysicalKey::from_x11_keycode(8), PhysicalKey::Unidentified); // evdev 0
        assert_eq!(PhysicalKey::from_x11_keycode(9), PhysicalKey::Escape); // evdev 1
    }

    /// Left and right modifiers are distinct positions — the reason
    /// `PhysicalKey` splits them where `KeyModifiers` deliberately does not.
    #[test]
    fn left_and_right_modifiers_are_distinct_positions() {
        assert_ne!(PhysicalKey::from_evdev(29), PhysicalKey::from_evdev(97));
        assert_eq!(PhysicalKey::from_evdev(29), PhysicalKey::ControlLeft);
        assert_eq!(PhysicalKey::from_evdev(97), PhysicalKey::ControlRight);
        assert_eq!(PhysicalKey::from_macos_keycode(0x37), PhysicalKey::MetaLeft);
        assert_eq!(PhysicalKey::from_macos_keycode(0x36), PhysicalKey::MetaRight);
    }
}
