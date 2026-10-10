//! The computer's POWER STATE for background work: whether it runs from mains power, and how
//! long the user has not touched it. An app asks before it starts something heavy that can wait
//! (AzDrive's recompression pass of an encrypted drive: only idle and on mains).
//!
//! A synchronous READING, cheap enough to ask once a minute, from any thread. Where the
//! platform does not say, the answer is the cautious one ([`PowerState::UNKNOWN`]: on battery,
//! not idle), so background work waits rather than drains a battery or slows the user down.
//!
//! * macOS: IOKit's providing power source (`IOPSGetProvidingPowerSourceType`: "AC Power") and
//!   CoreGraphics' seconds since the last input event of the session
//!   (`CGEventSourceSecondsSinceLastEventType`, any input event; no permission needed).
//! * Windows: `GetSystemPowerStatus` (`ACLineStatus`; a computer without a system battery is on
//!   mains) and `GetLastInputInfo` against `GetTickCount` (the session's last input).
//! * Linux: `/sys/class/power_supply` (a `Mains` or `USB` supply online, or no system battery,
//!   or a battery that is not discharging) and, in an X11 session, the MIT-SCREEN-SAVER
//!   extension's idle time (`XScreenSaverQueryInfo`, libXss through dlopen). A Wayland session
//!   has no synchronous idle query (`ext-idle-notify-v1` speaks in events; X11's idle time
//!   under XWayland sees only X clients): idle stays 0 there for now.
//! * iOS, Android: not yet ([`PowerState::UNKNOWN`]).
//! * A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`): the fixed
//!   [`PowerState::HEADLESS`] - on mains, never idle - so no test depends on the machine it runs
//!   on and no background pass starts behind a test's back. On battery when the battery's
//!   switch file (`AZ_BATTERY_STATE_FILE`, see `super::battery`) names a battery off its
//!   charger: one file drains a test's battery and unplugs its power.
//!
//! The battery's own reading (its level, Low Power Mode, the thermal state) is
//! `super::battery`'s; on macOS and Windows it reads the power sources this module loads
//! (the platform module's `internal_battery` / `system_power_status`).

/// The computer's power state (see the module documentation).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PowerState {
    /// Whether the computer runs from mains power: a desktop, or a laptop on its charger.
    /// `false` on battery, and where the platform does not say.
    pub on_mains: bool,
    /// Seconds since the user's last input anywhere in the session (keyboard, mouse, touch);
    /// `0` where the platform does not say.
    pub idle_secs: u64,
}

impl PowerState {
    /// What a headless or E2E run reports: on mains, the user just acted.
    pub const HEADLESS: PowerState = PowerState {
        on_mains: true,
        idle_secs: 0,
    };

    /// What a platform without a reading reports: on battery, the user just acted.
    pub const UNKNOWN: PowerState = PowerState {
        on_mains: false,
        idle_secs: 0,
    };

    /// The power state now (see the module documentation).
    #[must_use]
    pub fn query() -> PowerState {
        if super::headless_run() {
            let path = std::env::var_os(super::battery::BATTERY_STATE_FILE_VAR);
            return power_of_battery_file(path.as_deref().map(std::path::Path::new));
        }
        let (on_mains, idle_secs) = platform::read();
        PowerState {
            on_mains: on_mains.unwrap_or(false),
            idle_secs: idle_secs.unwrap_or(0),
        }
    }

    /// [`PowerState::HEADLESS`].
    #[must_use]
    pub fn headless() -> PowerState {
        PowerState::HEADLESS
    }

    /// Whether heavy background work may run: on mains power and idle for at least
    /// `min_idle_secs` seconds.
    #[must_use]
    pub fn is_idle_on_mains(&self, min_idle_secs: u64) -> bool {
        self.on_mains && self.idle_secs >= min_idle_secs
    }
}

/// A headless run's power: on mains ([`PowerState::HEADLESS`]) - on battery when the battery
/// switch file at `path` (`AZ_BATTERY_STATE_FILE`, [`super::battery::BatteryState::from_words`])
/// names a battery off its charger, so one file drains a test's battery and unplugs its power.
fn power_of_battery_file(path: Option<&std::path::Path>) -> PowerState {
    let battery = super::battery::reading_of_file(path);
    if battery.present && !battery.charging {
        PowerState {
            on_mains: false,
            ..PowerState::HEADLESS
        }
    } else {
        PowerState::HEADLESS
    }
}

/// macOS: the power sources (IOKit) and the session's idle time; the battery's reading shares
/// the power sources ([`platform::internal_battery`]).
#[cfg(target_os = "macos")]
pub(super) mod platform {
    use std::ffi::{c_char, c_void, CStr};

    use crate::desktop::extra::sensors::battery::{readings, BatteryState};

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *const c_void;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        /// A snapshot of the power sources (Copy rule: the caller releases it).
        fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
        /// "AC Power", "Battery Power" or "UPS Power" (Get rule: owned by the snapshot).
        fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
        /// The snapshot's power sources (Copy rule: the caller releases the array).
        fn IOPSCopyPowerSourcesList(snapshot: CFTypeRef) -> CFArrayRef;
        /// A power source's description (Get rule: owned by the snapshot).
        fn IOPSGetPowerSourceDescription(snapshot: CFTypeRef, source: CFTypeRef)
            -> CFDictionaryRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringGetCString(
            string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
        fn CFStringCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            is_external_representation: u8,
        ) -> CFStringRef;
        fn CFArrayGetCount(array: CFArrayRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> CFTypeRef;
        fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFNumberGetTypeID() -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value: *mut c_void) -> u8;
        fn CFBooleanGetValue(boolean: CFTypeRef) -> u8;
    }

    /// `kCFNumberSInt64Type`.
    const K_CF_NUMBER_SINT64_TYPE: isize = 4;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// `kCGEventSourceStateCombinedSessionState`: every input source of the login session.
    const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: i32 = 0;
    /// `kCGAnyInputEventType` (`~0`).
    const K_CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;

    /// `string` as UTF-8 (at most 63 bytes); `None` when it is no string.
    ///
    /// # Safety
    ///
    /// `string` is a live CF object or null.
    unsafe fn text_of(string: CFTypeRef) -> Option<String> {
        let mut buffer = [0 as c_char; 64];
        // SAFETY: the caller's; the buffer's size is passed.
        unsafe {
            if string.is_null() || CFGetTypeID(string) != CFStringGetTypeID() {
                return None;
            }
            if CFStringGetCString(
                string,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                K_CF_STRING_ENCODING_UTF8,
            ) == 0
            {
                return None;
            }
            Some(CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned())
        }
    }

    /// The value of `description`'s key `key` (a power source key: "Type", "Is Charging").
    ///
    /// # Safety
    ///
    /// `description` is a live CFDictionary; the value is owned by it.
    unsafe fn value_of(description: CFDictionaryRef, key: &str) -> CFTypeRef {
        // SAFETY: the key string is made and released here; the dictionary is the caller's.
        unsafe {
            let key = CFStringCreateWithBytes(
                std::ptr::null(),
                key.as_ptr(),
                key.len() as isize,
                K_CF_STRING_ENCODING_UTF8,
                0,
            );
            if key.is_null() {
                return std::ptr::null();
            }
            let value = CFDictionaryGetValue(description, key);
            CFRelease(key);
            value
        }
    }

    /// `description`'s number `key`.
    ///
    /// # Safety
    ///
    /// As [`value_of`].
    unsafe fn number_of(description: CFDictionaryRef, key: &str) -> Option<i64> {
        // SAFETY: the caller's; the value's type is checked before it is read.
        unsafe {
            let value = value_of(description, key);
            if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
                return None;
            }
            let mut number = 0_i64;
            (CFNumberGetValue(
                value,
                K_CF_NUMBER_SINT64_TYPE,
                std::ptr::addr_of_mut!(number).cast(),
            ) != 0)
                .then_some(number)
        }
    }

    /// `description`'s boolean `key`.
    ///
    /// # Safety
    ///
    /// As [`value_of`].
    unsafe fn flag_of(description: CFDictionaryRef, key: &str) -> Option<bool> {
        // SAFETY: the caller's; the value's type is checked before it is read.
        unsafe {
            let value = value_of(description, key);
            if value.is_null() || CFGetTypeID(value) != CFBooleanGetTypeID() {
                return None;
            }
            Some(CFBooleanGetValue(value) != 0)
        }
    }

    /// The internal battery among the power sources: `(present, charging, level_percent)`
    /// (IOKit's `Current Capacity` of its `Max Capacity`; on its charger: `Is Charging`, or its
    /// power source state "AC Power"). A Mac without one (a desktop) has no battery; `None`
    /// without a snapshot.
    pub(in crate::desktop::extra::sensors) fn internal_battery() -> Option<(bool, bool, u8)> {
        // SAFETY: the snapshot and the list are released after their descriptions were read;
        // each description is only read while the snapshot lives.
        unsafe {
            let snapshot = IOPSCopyPowerSourcesInfo();
            if snapshot.is_null() {
                return None;
            }
            let mut battery = (false, false, BatteryState::LEVEL_UNKNOWN);
            let list = IOPSCopyPowerSourcesList(snapshot);
            if !list.is_null() {
                for index in 0..CFArrayGetCount(list) {
                    let source = CFArrayGetValueAtIndex(list, index);
                    let description = IOPSGetPowerSourceDescription(snapshot, source);
                    if description.is_null()
                        || text_of(value_of(description, "Type")).as_deref()
                            != Some("InternalBattery")
                    {
                        continue;
                    }
                    if flag_of(description, "Is Present") == Some(false) {
                        continue;
                    }
                    let on_charger = flag_of(description, "Is Charging") == Some(true)
                        || text_of(value_of(description, "Power Source State")).as_deref()
                            == Some("AC Power");
                    let level = match (
                        number_of(description, "Current Capacity"),
                        number_of(description, "Max Capacity"),
                    ) {
                        (Some(current), Some(max)) => readings::level_of_capacity(current, max),
                        _ => BatteryState::LEVEL_UNKNOWN,
                    };
                    battery = (true, on_charger, level);
                    break;
                }
                CFRelease(list);
            }
            CFRelease(snapshot);
            Some(battery)
        }
    }

    fn on_mains() -> Option<bool> {
        // SAFETY: the snapshot is released after its string was copied out; the string is
        // only read while the snapshot lives.
        unsafe {
            let snapshot = IOPSCopyPowerSourcesInfo();
            if snapshot.is_null() {
                return None;
            }
            let kind = text_of(IOPSGetProvidingPowerSourceType(snapshot));
            CFRelease(snapshot);
            Some(kind? == "AC Power")
        }
    }

    fn idle_secs() -> Option<u64> {
        // SAFETY: a plain query without pointers.
        let secs = unsafe {
            CGEventSourceSecondsSinceLastEventType(
                K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE,
                K_CG_ANY_INPUT_EVENT_TYPE,
            )
        };
        (secs.is_finite() && secs >= 0.0).then(|| secs as u64)
    }

    pub(super) fn read() -> (Option<bool>, Option<u64>) {
        (on_mains(), idle_secs())
    }
}

/// Windows: `GetSystemPowerStatus` and the session's last input; the battery's reading shares
/// the power status ([`platform::system_power_status`]).
#[cfg(target_os = "windows")]
pub(super) mod platform {
    use std::sync::OnceLock;

    use crate::desktop::shell2::{
        common::DynamicLibrary as DynamicLibraryTrait, windows::dlopen::DynamicLibrary,
    };

    /// `SYSTEM_POWER_STATUS` (winbase.h).
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    pub(in crate::desktop::extra::sensors) struct SystemPowerStatus {
        pub(in crate::desktop::extra::sensors) ac_line_status: u8,
        pub(in crate::desktop::extra::sensors) battery_flag: u8,
        pub(in crate::desktop::extra::sensors) battery_life_percent: u8,
        pub(in crate::desktop::extra::sensors) system_status_flag: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }

    /// `LASTINPUTINFO` (winuser.h).
    #[repr(C)]
    struct LastInputInfo {
        cb_size: u32,
        dw_time: u32,
    }

    type GetSystemPowerStatusFn = unsafe extern "system" fn(*mut SystemPowerStatus) -> i32;
    type GetLastInputInfoFn = unsafe extern "system" fn(*mut LastInputInfo) -> i32;
    type GetTickCountFn = unsafe extern "system" fn() -> u32;

    #[derive(Clone, Copy)]
    struct Functions {
        get_system_power_status: Option<GetSystemPowerStatusFn>,
        get_last_input_info: Option<GetLastInputInfoFn>,
        get_tick_count: Option<GetTickCountFn>,
    }

    /// The functions, loaded once. kernel32 and user32 stay loaded for the process's life
    /// (every Win32 process has them), so their handles are leaked, not freed.
    fn functions() -> Functions {
        static FUNCTIONS: OnceLock<Functions> = OnceLock::new();
        *FUNCTIONS.get_or_init(|| {
            let kernel32 = DynamicLibrary::load("kernel32.dll").ok();
            let user32 = DynamicLibrary::load("user32.dll").ok();
            // SAFETY: the symbols have exactly these signatures (winbase.h, winuser.h).
            let functions = unsafe {
                Functions {
                    get_system_power_status: kernel32
                        .as_ref()
                        .and_then(|lib| lib.get_symbol("GetSystemPowerStatus").ok()),
                    get_tick_count: kernel32
                        .as_ref()
                        .and_then(|lib| lib.get_symbol("GetTickCount").ok()),
                    get_last_input_info: user32
                        .as_ref()
                        .and_then(|lib| lib.get_symbol("GetLastInputInfo").ok()),
                }
            };
            std::mem::forget(kernel32);
            std::mem::forget(user32);
            functions
        })
    }

    /// `GetSystemPowerStatus` now; `None` where kernel32 lacks it or the call fails.
    pub(in crate::desktop::extra::sensors) fn system_power_status() -> Option<SystemPowerStatus> {
        let get = functions().get_system_power_status?;
        let mut status = SystemPowerStatus::default();
        // SAFETY: `status` is a valid SYSTEM_POWER_STATUS.
        if unsafe { get(&mut status) } == 0 {
            return None;
        }
        Some(status)
    }

    fn on_mains() -> Option<bool> {
        let status = system_power_status()?;
        match status.ac_line_status {
            1 => Some(true),
            0 => Some(false),
            // 255: unknown. No system battery (flag 128): a desktop, on mains.
            _ => (status.battery_flag == 128).then_some(true),
        }
    }

    fn idle_secs(f: &Functions) -> Option<u64> {
        let (get_last_input, get_ticks) = (f.get_last_input_info?, f.get_tick_count?);
        let mut info = LastInputInfo {
            cb_size: core::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        // SAFETY: `info` is a valid LASTINPUTINFO with its size set.
        if unsafe { get_last_input(&mut info) } == 0 {
            return None;
        }
        // Both are GetTickCount milliseconds: the difference is right across the 49-day wrap.
        let now = unsafe { get_ticks() };
        Some(u64::from(now.wrapping_sub(info.dw_time)) / 1000)
    }

    pub(super) fn read() -> (Option<bool>, Option<u64>) {
        (on_mains(), idle_secs(&functions()))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{
        ffi::{c_char, c_int, c_ulong, c_void},
        sync::OnceLock,
    };

    use crate::desktop::{
        extra::sensors::battery::readings::read_trimmed,
        shell2::{
            common::{dlopen::load_first_available, DynamicLibrary as DynamicLibraryTrait},
            linux::x11::dlopen::Library,
        },
    };

    /// From `/sys/class/power_supply`: a mains or USB supply online is mains; without one, a
    /// system battery that discharges is battery power, and no system battery at all (a
    /// desktop, a virtual machine) is mains. Batteries of devices (a mouse: `scope` `Device`)
    /// do not count.
    fn on_mains() -> Option<bool> {
        let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
        let (mut mains_online, mut battery, mut discharging) = (false, false, false);
        for entry in entries.flatten() {
            let supply = entry.path();
            match read_trimmed(&supply.join("type")).as_str() {
                "Mains" | "USB" => {
                    if read_trimmed(&supply.join("online")) == "1" {
                        mains_online = true;
                    }
                }
                "Battery" => {
                    if read_trimmed(&supply.join("scope")) == "Device" {
                        continue;
                    }
                    battery = true;
                    if read_trimmed(&supply.join("status")) == "Discharging" {
                        discharging = true;
                    }
                }
                _ => {}
            }
        }
        Some(mains_online || !battery || !discharging)
    }

    /// `XScreenSaverInfo` (X11/extensions/scrnsaver.h).
    #[repr(C)]
    #[allow(dead_code)]
    struct XScreenSaverInfo {
        window: c_ulong,
        state: c_int,
        kind: c_int,
        til_or_since: c_ulong,
        /// Milliseconds since the last input.
        idle: c_ulong,
        event_mask: c_ulong,
    }

    type XOpenDisplayFn = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type XCloseDisplayFn = unsafe extern "C" fn(*mut c_void) -> c_int;
    type XDefaultRootWindowFn = unsafe extern "C" fn(*mut c_void) -> c_ulong;
    type XFreeFn = unsafe extern "C" fn(*mut c_void) -> c_int;
    type XScreenSaverAllocInfoFn = unsafe extern "C" fn() -> *mut XScreenSaverInfo;
    type XScreenSaverQueryInfoFn =
        unsafe extern "C" fn(*mut c_void, c_ulong, *mut XScreenSaverInfo) -> c_int;

    #[derive(Clone, Copy)]
    struct Xss {
        open_display: XOpenDisplayFn,
        close_display: XCloseDisplayFn,
        default_root_window: XDefaultRootWindowFn,
        free: XFreeFn,
        alloc_info: XScreenSaverAllocInfoFn,
        query_info: XScreenSaverQueryInfoFn,
    }

    /// libX11 and libXss, loaded once (and kept loaded); `None` where either is missing.
    fn xss() -> Option<Xss> {
        static XSS: OnceLock<Option<Xss>> = OnceLock::new();
        *XSS.get_or_init(|| {
            let x11: Library = load_first_available(&["libX11.so.6", "libX11.so"]).ok()?;
            let xss: Library = load_first_available(&["libXss.so.1", "libXss.so"]).ok()?;
            // SAFETY: the symbols have exactly these signatures (Xlib.h, scrnsaver.h).
            let functions = unsafe {
                Xss {
                    open_display: x11.get_symbol("XOpenDisplay").ok()?,
                    close_display: x11.get_symbol("XCloseDisplay").ok()?,
                    default_root_window: x11.get_symbol("XDefaultRootWindow").ok()?,
                    free: x11.get_symbol("XFree").ok()?,
                    alloc_info: xss.get_symbol("XScreenSaverAllocInfo").ok()?,
                    query_info: xss.get_symbol("XScreenSaverQueryInfo").ok()?,
                }
            };
            std::mem::forget(x11);
            std::mem::forget(xss);
            Some(functions)
        })
    }

    /// The X11 session's idle time; `None` in a Wayland session (see the module docs) and
    /// without an X display or the extension.
    fn idle_secs() -> Option<u64> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
            || std::env::var_os("DISPLAY").is_none()
        {
            return None;
        }
        let x = xss()?;
        // SAFETY: a display of our own, opened and closed here; the info is allocated by the
        // extension and freed with XFree.
        unsafe {
            let display = (x.open_display)(std::ptr::null());
            if display.is_null() {
                return None;
            }
            let info = (x.alloc_info)();
            let mut idle = None;
            if !info.is_null() {
                if (x.query_info)(display, (x.default_root_window)(display), info) != 0 {
                    idle = Some(u64::from((*info).idle) / 1000);
                }
                (x.free)(info.cast());
            }
            (x.close_display)(display);
            idle
        }
    }

    pub(super) fn read() -> (Option<bool>, Option<u64>) {
        (on_mains(), idle_secs())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    pub(super) fn read() -> (Option<bool>, Option<u64>) {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::{power_of_battery_file, PowerState};

    #[test]
    fn a_headless_runs_power_follows_the_battery_switch_file() {
        let path =
            std::env::temp_dir().join(format!("azul-power-test-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(power_of_battery_file(None), PowerState::HEADLESS);
        assert_eq!(power_of_battery_file(Some(&path)), PowerState::HEADLESS, "no file yet");
        std::fs::write(&path, "battery 23 discharging\n").unwrap();
        assert_eq!(
            power_of_battery_file(Some(&path)),
            PowerState {
                on_mains: false,
                idle_secs: 0,
            },
            "a battery off its charger"
        );
        std::fs::write(&path, "charging 23").unwrap();
        assert_eq!(power_of_battery_file(Some(&path)), PowerState::HEADLESS, "on its charger");
        std::fs::write(&path, "serious").unwrap();
        assert_eq!(power_of_battery_file(Some(&path)), PowerState::HEADLESS, "no battery");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn background_work_waits_unless_the_computer_is_idle_on_mains() {
        let idle_on_mains = PowerState {
            on_mains: true,
            idle_secs: 600,
        };
        assert!(idle_on_mains.is_idle_on_mains(300));
        assert!(!idle_on_mains.is_idle_on_mains(601));
        assert!(!PowerState {
            on_mains: false,
            idle_secs: 600
        }
        .is_idle_on_mains(300));
        assert!(!PowerState::UNKNOWN.is_idle_on_mains(0), "unknown is on battery");
        assert!(PowerState::HEADLESS.is_idle_on_mains(0));
        assert!(!PowerState::HEADLESS.is_idle_on_mains(1), "headless is never idle");
    }
}
