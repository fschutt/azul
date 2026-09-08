// azul VirtualKeyCode names → CDP Input.dispatchKeyEvent fields.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.4).
// Desktop scenarios name keys after `parse_virtual_keycode`
// (dll/src/desktop/shell2/common/debug_server/full.rs:4402+, case-insensitive):
// letters, digits, "tab", "return", "space", "escape", "back", "lshift",
// "lcontrol", arrows "left"/"right"/"up"/"down", ...
//
// CDP modifiers bitmask: Alt=1, Ctrl=2, Meta=4, Shift=8.

export const MOD_ALT = 1;
export const MOD_CTRL = 2;
export const MOD_META = 4;
export const MOD_SHIFT = 8;

// Non-printable / special keys. `text` is only attached for printable keys
// (and Enter/Space, which desktop treats as text-producing too).
const SPECIAL = {
    tab: { key: 'Tab', code: 'Tab', vk: 9 },
    return: { key: 'Enter', code: 'Enter', vk: 13, text: '\r' },
    enter: { key: 'Enter', code: 'Enter', vk: 13, text: '\r' },
    space: { key: ' ', code: 'Space', vk: 32, text: ' ' },
    escape: { key: 'Escape', code: 'Escape', vk: 27 },
    back: { key: 'Backspace', code: 'Backspace', vk: 8 },
    backspace: { key: 'Backspace', code: 'Backspace', vk: 8 },
    delete: { key: 'Delete', code: 'Delete', vk: 46 },
    home: { key: 'Home', code: 'Home', vk: 36 },
    end: { key: 'End', code: 'End', vk: 35 },
    pageup: { key: 'PageUp', code: 'PageUp', vk: 33 },
    pagedown: { key: 'PageDown', code: 'PageDown', vk: 34 },
    insert: { key: 'Insert', code: 'Insert', vk: 45 },
    left: { key: 'ArrowLeft', code: 'ArrowLeft', vk: 37 },
    up: { key: 'ArrowUp', code: 'ArrowUp', vk: 38 },
    right: { key: 'ArrowRight', code: 'ArrowRight', vk: 39 },
    down: { key: 'ArrowDown', code: 'ArrowDown', vk: 40 },
    arrowleft: { key: 'ArrowLeft', code: 'ArrowLeft', vk: 37 },
    arrowup: { key: 'ArrowUp', code: 'ArrowUp', vk: 38 },
    arrowright: { key: 'ArrowRight', code: 'ArrowRight', vk: 39 },
    arrowdown: { key: 'ArrowDown', code: 'ArrowDown', vk: 40 },
    // Modifier keys — the driver tracks these as held and ORs their bit into
    // every subsequent input event until the matching key_up (desktop
    // scenarios bracket Shift+Left as explicit key_down/key_up pairs, see
    // tests/e2e/contenteditable_overflow_test.json:135-141).
    lshift: { key: 'Shift', code: 'ShiftLeft', vk: 16, mod: MOD_SHIFT },
    rshift: { key: 'Shift', code: 'ShiftRight', vk: 16, mod: MOD_SHIFT },
    shift: { key: 'Shift', code: 'ShiftLeft', vk: 16, mod: MOD_SHIFT },
    lcontrol: { key: 'Control', code: 'ControlLeft', vk: 17, mod: MOD_CTRL },
    rcontrol: { key: 'Control', code: 'ControlRight', vk: 17, mod: MOD_CTRL },
    control: { key: 'Control', code: 'ControlLeft', vk: 17, mod: MOD_CTRL },
    ctrl: { key: 'Control', code: 'ControlLeft', vk: 17, mod: MOD_CTRL },
    lalt: { key: 'Alt', code: 'AltLeft', vk: 18, mod: MOD_ALT },
    ralt: { key: 'Alt', code: 'AltRight', vk: 18, mod: MOD_ALT },
    alt: { key: 'Alt', code: 'AltLeft', vk: 18, mod: MOD_ALT },
    lwin: { key: 'Meta', code: 'MetaLeft', vk: 91, mod: MOD_META },
    rwin: { key: 'Meta', code: 'MetaRight', vk: 92, mod: MOD_META },
    f1: { key: 'F1', code: 'F1', vk: 112 },
    f2: { key: 'F2', code: 'F2', vk: 113 },
    f3: { key: 'F3', code: 'F3', vk: 114 },
    f4: { key: 'F4', code: 'F4', vk: 115 },
    f5: { key: 'F5', code: 'F5', vk: 116 },
    f6: { key: 'F6', code: 'F6', vk: 117 },
    f7: { key: 'F7', code: 'F7', vk: 118 },
    f8: { key: 'F8', code: 'F8', vk: 119 },
    f9: { key: 'F9', code: 'F9', vk: 120 },
    f10: { key: 'F10', code: 'F10', vk: 121 },
    f11: { key: 'F11', code: 'F11', vk: 122 },
    f12: { key: 'F12', code: 'F12', vk: 123 },
};

/**
 * Look up an azul VirtualKeyCode name (case-insensitive).
 *
 * @param {string} name           scenario "key" value, e.g. "Tab", "LShift", "A", "Key5", "5"
 * @param {number} activeModifiers CDP modifier bitmask currently in effect (shift changes letter case/text)
 * @returns {{key,code,vk,text?,mod?}|null} CDP fields; `mod` set for modifier keys.
 */
export function lookupKey(name, activeModifiers = 0) {
    if (typeof name !== 'string' || name.length === 0) return null;
    const n = name.toLowerCase();

    if (SPECIAL[n]) return { ...SPECIAL[n] };

    // Letters "a".."z"
    if (/^[a-z]$/.test(n)) {
        const shift = (activeModifiers & MOD_SHIFT) !== 0;
        const ch = shift ? n.toUpperCase() : n;
        const out = { key: ch, code: `Key${n.toUpperCase()}`, vk: 65 + (n.charCodeAt(0) - 97) };
        // Only printable when no ctrl/alt chord is held (Ctrl+A must not type "a").
        if (!(activeModifiers & (MOD_CTRL | MOD_ALT))) out.text = ch;
        return out;
    }

    // Digits "0".."9" and desktop-style "key0".."key9"
    const dm = n.match(/^(?:key)?([0-9])$/);
    if (dm) {
        const d = dm[1];
        const out = { key: d, code: `Digit${d}`, vk: 48 + Number(d) };
        if (!(activeModifiers & (MOD_CTRL | MOD_ALT))) out.text = d;
        return out;
    }

    return null;
}

/**
 * Desktop `modifiers` param ({shift,ctrl,alt,meta} bools,
 * full.rs:2342-2350) → CDP bitmask.
 */
export function modifiersToBits(m) {
    if (!m || typeof m !== 'object') return 0;
    return (m.alt ? MOD_ALT : 0) | (m.ctrl ? MOD_CTRL : 0) |
        (m.meta ? MOD_META : 0) | (m.shift ? MOD_SHIFT : 0);
}
