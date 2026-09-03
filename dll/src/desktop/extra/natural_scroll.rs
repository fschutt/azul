//! The platform's NATURAL-SCROLLING preference (9b-ii-b-i-a), read once at
//! startup for `AppConfig::natural_scroll = System`.
//!
//! Every desktop platform applies the user's preference to the deltas it
//! hands the app, so this is a READING, never a second flip - see the field's
//! docs. Blind on Windows and macOS (no session to test on here; the user's
//! ruling was to implement and cross-compile), `None` where a platform has no
//! such setting or the read fails.
//!
//! * macOS: `NSUserDefaults` `com.apple.swipescrolldirection` - the "Natural
//!   scrolling" checkbox; the key is absent on a fresh account, and then the
//!   system default is ON.
//! * Windows: the precision touchpad's `ScrollDirection` under
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad`:
//!   `0` = "downwards motion scrolls down" (natural), `0x100` = reversed.
//!   Absent = the default, natural.
//! * Wayland: not here - the compositor says so per pointer through
//!   `wl_pointer.axis_relative_direction`, and the Wayland backend publishes
//!   that as it arrives.
//! * X11, Android, iOS: nothing to read.

/// The platform's preference: `Some(true)` natural, `Some(false)` classic,
/// `None` unknown.
#[must_use]
pub fn read_system_preference() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        return macos();
    }
    #[cfg(target_os = "windows")]
    {
        return windows();
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "macos")]
fn macos() -> Option<bool> {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    unsafe {
        let defaults: *mut Object = msg_send![class!(NSUserDefaults), standardUserDefaults];
        if defaults.is_null() {
            return None;
        }
        let key: *mut Object = msg_send![
            class!(NSString),
            stringWithUTF8String: b"com.apple.swipescrolldirection\0".as_ptr()
        ];
        if key.is_null() {
            return None;
        }
        // Absent key = the system default, which is natural scrolling ON.
        let present: *mut Object = msg_send![defaults, objectForKey: key];
        if present.is_null() {
            return Some(true);
        }
        let natural: bool = msg_send![defaults, boolForKey: key];
        Some(natural)
    }
}

#[cfg(target_os = "windows")]
fn windows() -> Option<bool> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value: u32 = 0;
    let mut size: u32 = core::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad"),
            w!("ScrollDirection"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status.is_ok() {
        // 0 = downwards motion scrolls down (natural); 0x100 = reversed.
        Some(value == 0)
    } else {
        // No precision touchpad key: the default is natural.
        None
    }
}
