//! What the user's keyboard layout prints on a key.
//!
//! macOS reports a key by its place on the keyboard (`NSEvent::keyCode`): the
//! same number on a US, a German and a French keyboard. A shortcut names what
//! the key prints, so the shell asks the layout, the way AppKit matches menu
//! key equivalents: the active layout first, and where it prints no ASCII on
//! a key (Russian, Greek) the ASCII-capable layout the user has. The answer
//! goes through `common::event::macos_keycode_in_layout`.
//!
//! Text Input Sources and `UCKeyTranslate` live in HIToolbox, loaded at
//! runtime like the global-hotkey backend loads it. They are main-thread
//! APIs; off the main thread (or without HIToolbox) there is no answer and
//! every key keeps its place.

use core::ffi::{c_ulong, c_void};
use std::sync::OnceLock;

/// `kUCKeyActionDisplay`: the character the key shows, not one it types
/// after a dead key.
const KEY_ACTION_DISPLAY: u16 = 3;

/// `kUCKeyTranslateNoDeadKeysMask`: a dead key (´ on many layouts) answers
/// its own character instead of starting a composition.
const NO_DEAD_KEYS: u32 = 1;

/// The HIToolbox and CoreFoundation entry points, resolved once. The
/// libraries are leaked so the fn pointers stay valid (neither unloads).
struct Tis {
    copy_current_layout: unsafe extern "C" fn() -> *const c_void,
    copy_current_ascii_layout: unsafe extern "C" fn() -> *const c_void,
    get_property: unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void,
    /// `kTISPropertyUnicodeKeyLayoutData`, a `CFStringRef` constant (kept as
    /// an address so the table is `Send` and `Sync`).
    unicode_layout_key: usize,
    #[allow(clippy::type_complexity)] // the C signature of UCKeyTranslate
    key_translate: unsafe extern "C" fn(
        *const c_void,
        u16,
        u16,
        u32,
        u32,
        u32,
        *mut u32,
        c_ulong,
        *mut c_ulong,
        *mut u16,
    ) -> i32,
    keyboard_type: unsafe extern "C" fn() -> u8,
    data_bytes: unsafe extern "C" fn(*const c_void) -> *const u8,
    release: unsafe extern "C" fn(*const c_void),
}

fn tis() -> Option<&'static Tis> {
    static TIS: OnceLock<Option<Tis>> = OnceLock::new();
    TIS.get_or_init(|| unsafe {
        let hi = libloading::Library::new(
            "/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox",
        )
        .or_else(|_| libloading::Library::new("/System/Library/Frameworks/Carbon.framework/Carbon"))
        .ok()?;
        let cf = libloading::Library::new(
            "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation",
        )
        .ok()?;
        let unicode_layout_key = **hi
            .get::<*const *const c_void>(b"kTISPropertyUnicodeKeyLayoutData\0")
            .ok()?;
        let tis = Tis {
            copy_current_layout: *hi
                .get(b"TISCopyCurrentKeyboardLayoutInputSource\0")
                .ok()?,
            copy_current_ascii_layout: *hi
                .get(b"TISCopyCurrentASCIICapableKeyboardLayoutInputSource\0")
                .ok()?,
            get_property: *hi.get(b"TISGetInputSourceProperty\0").ok()?,
            unicode_layout_key: unicode_layout_key as usize,
            key_translate: *hi.get(b"UCKeyTranslate\0").ok()?,
            keyboard_type: *hi.get(b"LMGetKbdType\0").ok()?,
            data_bytes: *cf.get(b"CFDataGetBytePtr\0").ok()?,
            release: *cf.get(b"CFRelease\0").ok()?,
        };
        if tis.unicode_layout_key == 0 {
            return None;
        }
        core::mem::forget(hi);
        core::mem::forget(cf);
        Some(tis)
    })
    .as_ref()
}

/// The character the active keyboard layout prints on `keycode` without
/// modifiers, or `None` when it cannot say (off the main thread, no
/// HIToolbox, a key that prints nothing).
///
/// A layout that prints no ASCII on the key is asked again through the
/// ASCII-capable layout, so Cmd+Я on a Russian keyboard is Cmd+Z as AppKit
/// has it. A character that stays outside ASCII (QWERTZ's ö) is answered as
/// it is; `macos_keycode_in_layout` then keeps the key's place.
#[must_use]
pub fn layout_char_for_keycode(keycode: u16) -> Option<char> {
    objc2::MainThreadMarker::new()?;
    let tis = tis()?;
    // SAFETY: main thread (checked above); the Copy functions return an
    // owned reference (released below) or null, which `char_in` refuses.
    unsafe {
        let current = (tis.copy_current_layout)();
        let printed = char_in(tis, current, keycode);
        release(tis, current);
        if printed.is_some_and(|c| c.is_ascii()) {
            return printed;
        }
        let ascii = (tis.copy_current_ascii_layout)();
        let fallback = char_in(tis, ascii, keycode);
        release(tis, ascii);
        fallback.filter(char::is_ascii).or(printed)
    }
}

/// The key `keycode` names in the active keyboard layout: what a key event,
/// a `flagsChanged:` and a global hot key registration all read, so the
/// three always agree. Only the keys a layout engraves are asked about.
#[must_use]
pub fn virtual_key_for_keycode(keycode: u16) -> Option<azul_core::window::VirtualKeyCode> {
    use crate::desktop::shell2::common::event::{
        macos_keycode_follows_layout, macos_keycode_in_layout,
    };
    let printed = if macos_keycode_follows_layout(keycode) {
        layout_char_for_keycode(keycode)
    } else {
        None
    };
    macos_keycode_in_layout(keycode, printed)
}

/// What the input source `source` prints on `keycode`.
///
/// # Safety
/// `source` is a live `TISInputSourceRef` or null; main thread.
unsafe fn char_in(tis: &Tis, source: *const c_void, keycode: u16) -> Option<char> {
    if source.is_null() {
        return None;
    }
    // A Get function: the data belongs to `source`, which outlives this call.
    let data = (tis.get_property)(source, tis.unicode_layout_key as *const c_void);
    if data.is_null() {
        return None;
    }
    let layout = (tis.data_bytes)(data);
    if layout.is_null() {
        return None;
    }
    let mut dead_key_state = 0u32;
    let mut length: c_ulong = 0;
    let mut units = [0u16; 4];
    let status = (tis.key_translate)(
        layout.cast(),
        keycode,
        KEY_ACTION_DISPLAY,
        0,
        u32::from((tis.keyboard_type)()),
        NO_DEAD_KEYS,
        &mut dead_key_state,
        units.len() as c_ulong,
        &mut length,
        units.as_mut_ptr(),
    );
    let length = usize::try_from(length).unwrap_or(0).min(units.len());
    if status != 0 || length == 0 {
        return None;
    }
    char::decode_utf16(units[..length].iter().copied())
        .next()?
        .ok()
        .filter(|c| !c.is_control())
}

/// # Safety
/// `source` is an owned CF reference or null.
unsafe fn release(tis: &Tis, source: *const c_void) {
    if !source.is_null() {
        (tis.release)(source);
    }
}

/// What the installed layout `id` (`com.apple.keylayout.French`) prints on
/// `keycode`, for the tests: a French keyboard need not be the active one.
/// Looking a source up by id works off the main thread, where the test
/// harness runs.
#[cfg(test)]
pub(crate) fn char_in_layout(id: &str, keycode: u16) -> Option<char> {
    type CreateList = unsafe extern "C" fn(*const c_void, bool) -> *const c_void;
    type CreateString = unsafe extern "C" fn(*const c_void, *const u8, u32) -> *const c_void;
    type CreateDictionary = unsafe extern "C" fn(
        *const c_void,
        *const *const c_void,
        *const *const c_void,
        isize,
        *const c_void,
        *const c_void,
    ) -> *const c_void;
    type ArrayCount = unsafe extern "C" fn(*const c_void) -> isize;
    type ArrayAt = unsafe extern "C" fn(*const c_void, isize) -> *const c_void;
    /// `kCFStringEncodingUTF8`.
    const UTF8: u32 = 0x0800_0100;

    let tis = tis()?;
    let name = std::ffi::CString::new(id).ok()?;
    // SAFETY: the libraries are loaded for the duration of the call; every
    // CF object created here is released before it returns.
    unsafe {
        let hi = libloading::Library::new(
            "/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox",
        )
        .ok()?;
        let cf = libloading::Library::new(
            "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation",
        )
        .ok()?;
        let create_list: CreateList = *hi.get(b"TISCreateInputSourceList\0").ok()?;
        let id_key = **hi.get::<*const *const c_void>(b"kTISPropertyInputSourceID\0").ok()?;
        let create_string: CreateString = *cf.get(b"CFStringCreateWithCString\0").ok()?;
        let create_dictionary: CreateDictionary = *cf.get(b"CFDictionaryCreate\0").ok()?;
        let key_callbacks = *cf
            .get::<*const c_void>(b"kCFTypeDictionaryKeyCallBacks\0")
            .ok()?;
        let value_callbacks = *cf
            .get::<*const c_void>(b"kCFTypeDictionaryValueCallBacks\0")
            .ok()?;
        let array_count: ArrayCount = *cf.get(b"CFArrayGetCount\0").ok()?;
        let array_at: ArrayAt = *cf.get(b"CFArrayGetValueAtIndex\0").ok()?;

        let value = create_string(core::ptr::null(), name.as_ptr().cast(), UTF8);
        if value.is_null() {
            return None;
        }
        let keys = [id_key];
        let values = [value];
        let filter = create_dictionary(
            core::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            key_callbacks,
            value_callbacks,
        );
        release(tis, value);
        if filter.is_null() {
            return None;
        }
        let list = create_list(filter, true);
        release(tis, filter);
        if list.is_null() {
            return None;
        }
        let found = if array_count(list) > 0 {
            char_in(tis, array_at(list, 0), keycode)
        } else {
            None
        };
        release(tis, list);
        found
    }
}

#[cfg(test)]
mod tests {
    use azul_core::window::VirtualKeyCode;

    use super::char_in_layout;
    use crate::desktop::shell2::common::event::macos_keycode_in_layout;

    /// The key `keycode` names on the installed layout `id`.
    fn key_on(id: &str, keycode: u16) -> Option<VirtualKeyCode> {
        macos_keycode_in_layout(keycode, char_in_layout(id, keycode))
    }

    #[test]
    fn the_layouts_answer_what_they_print() {
        assert_eq!(char_in_layout("com.apple.keylayout.US", 0x06), Some('z'));
        assert_eq!(char_in_layout("com.apple.keylayout.French", 0x0D), Some('z'));
        assert_eq!(char_in_layout("com.apple.keylayout.German", 0x10), Some('z'));
        assert_eq!(char_in_layout("com.apple.keylayout.Russian", 0x06), Some('я'));
    }

    /// Cmd+Z on a French keyboard is the key at the US W place, on a German
    /// one the key at the US Y place: both arrive as Z, and the US engraving
    /// of those places (W, Y) arrives where those layouts print it.
    #[test]
    fn the_key_that_prints_z_arrives_as_z_on_french_and_german_keyboards() {
        assert_eq!(key_on("com.apple.keylayout.French", 0x0D), Some(VirtualKeyCode::Z));
        assert_eq!(key_on("com.apple.keylayout.French", 0x06), Some(VirtualKeyCode::W));
        assert_eq!(key_on("com.apple.keylayout.French", 0x0C), Some(VirtualKeyCode::A));
        assert_eq!(key_on("com.apple.keylayout.French", 0x29), Some(VirtualKeyCode::M));
        assert_eq!(key_on("com.apple.keylayout.French", 0x12), Some(VirtualKeyCode::Key1));
        assert_eq!(key_on("com.apple.keylayout.German", 0x10), Some(VirtualKeyCode::Z));
        assert_eq!(key_on("com.apple.keylayout.German", 0x06), Some(VirtualKeyCode::Y));
        assert_eq!(key_on("com.apple.keylayout.US", 0x06), Some(VirtualKeyCode::Z));
        // A layout that prints no Latin letter keeps the place.
        assert_eq!(key_on("com.apple.keylayout.Russian", 0x06), Some(VirtualKeyCode::Z));
    }
}
