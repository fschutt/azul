//! The desktop players of the system-sound routes that need no state of their own: the macOS alert
//! sound and beep, and Windows' `MessageBeep`.
//!
//! `azul_core::sound::route` names the route and `common::event::play_sound_native` hands an
//! admitted one to [`play`]. Both players call a function the OS already has, through `libloading`,
//! the way the Opus decoder reaches AudioToolbox: no import library and no framework to link, and
//! a library or a symbol that is not there is a `Failed` outcome, not a crash. Linux has no player
//! here: its route (libcanberra, the freedesktop sound theme) needs a connection to the sound
//! server, which belongs to the window.

use azul_core::sound::{SoundOutcome, SoundRoute};

/// `kSystemSoundID_UserPreferredAlert`: the alert sound the user chose in System Settings.
pub const USER_PREFERRED_ALERT: u32 = 0x1000;

/// The `uType` of Win32 `MessageBeep` for the name of the constant a route carries
/// (`azul_core::sound`'s Windows table): the sound the user's scheme has for that kind of message.
#[must_use]
pub fn message_beep_kind(name: &str) -> Option<u32> {
    match name {
        "MB_OK" => Some(0x0000_0000),
        "MB_ICONHAND" => Some(0x0000_0010),
        "MB_ICONQUESTION" => Some(0x0000_0020),
        "MB_ICONEXCLAMATION" => Some(0x0000_0030),
        "MB_ICONASTERISK" => Some(0x0000_0040),
        _ => None,
    }
}

/// Play one admitted route of a desktop and say how that went.
#[must_use]
pub fn play(route: &SoundRoute) -> SoundOutcome {
    match route {
        SoundRoute::Effect { name, .. } => play_effect(name),
        SoundRoute::Silent(_)
        | SoundRoute::Ringtone { .. }
        | SoundRoute::MediaAction { .. }
        | SoundRoute::Haptic(_) => SoundOutcome::Failed(format!(
            "the desktop player has no sound for the route {}",
            route.label()
        )),
    }
}

/// The player has no function of this name on this platform.
fn unsupported(name: &str) -> SoundOutcome {
    SoundOutcome::Failed(format!("this platform has no desktop player for {name}"))
}

#[cfg(target_os = "macos")]
fn play_effect(name: &str) -> SoundOutcome {
    match name {
        // `void AudioServicesPlayAlertSound(SystemSoundID)`: the user's alert sound, on the alert
        // device at the alert volume, or a flash of the screen when the user asked for that.
        "AudioServicesPlayAlertSound" => call_with_id(
            "/System/Library/Frameworks/AudioToolbox.framework/AudioToolbox",
            b"AudioServicesPlayAlertSound\0",
            USER_PREFERRED_ALERT,
        ),
        // `void NSBeep(void)`: the system beep.
        "NSBeep" => call_without_arguments(
            "/System/Library/Frameworks/AppKit.framework/AppKit",
            b"NSBeep\0",
        ),
        other => unsupported(other),
    }
}

/// Open `library`, call the C function `symbol` of it with one `UInt32`, and close it again (the
/// system frameworks stay loaded: other code of the process holds them).
#[cfg(target_os = "macos")]
fn call_with_id(library: &str, symbol: &[u8], argument: u32) -> SoundOutcome {
    // SAFETY: the symbols named by the two callers are functions of system frameworks with the
    // C signature `void f(UInt32)`; the library outlives the call.
    unsafe {
        let lib = match libloading::Library::new(library) {
            Ok(lib) => lib,
            Err(e) => return SoundOutcome::Failed(format!("cannot open {library}: {e}")),
        };
        let function: libloading::Symbol<'_, unsafe extern "C" fn(u32)> = match lib.get(symbol) {
            Ok(function) => function,
            Err(e) => {
                return SoundOutcome::Failed(format!(
                    "{library} has no {}: {e}",
                    String::from_utf8_lossy(symbol)
                ))
            }
        };
        function(argument);
    }
    SoundOutcome::Played
}

/// As [`call_with_id`], for `void f(void)`.
#[cfg(target_os = "macos")]
fn call_without_arguments(library: &str, symbol: &[u8]) -> SoundOutcome {
    // SAFETY: the symbol named by the caller is a function of a system framework with the C
    // signature `void f(void)`; the library outlives the call.
    unsafe {
        let lib = match libloading::Library::new(library) {
            Ok(lib) => lib,
            Err(e) => return SoundOutcome::Failed(format!("cannot open {library}: {e}")),
        };
        let function: libloading::Symbol<'_, unsafe extern "C" fn()> = match lib.get(symbol) {
            Ok(function) => function,
            Err(e) => {
                return SoundOutcome::Failed(format!(
                    "{library} has no {}: {e}",
                    String::from_utf8_lossy(symbol)
                ))
            }
        };
        function();
    }
    SoundOutcome::Played
}

#[cfg(target_os = "windows")]
fn play_effect(name: &str) -> SoundOutcome {
    let Some(kind) = message_beep_kind(name) else {
        return unsupported(name);
    };
    // SAFETY: `MessageBeep` of user32.dll is `BOOL MessageBeep(UINT)`; the library outlives the call.
    unsafe {
        let lib = match libloading::Library::new("user32.dll") {
            Ok(lib) => lib,
            Err(e) => return SoundOutcome::Failed(format!("cannot open user32.dll: {e}")),
        };
        let beep: libloading::Symbol<'_, unsafe extern "system" fn(u32) -> i32> = match lib
            .get(b"MessageBeep\0")
        {
            Ok(beep) => beep,
            Err(e) => return SoundOutcome::Failed(format!("user32.dll has no MessageBeep: {e}")),
        };
        if beep(kind) != 0 {
            SoundOutcome::Played
        } else {
            SoundOutcome::Failed(String::from(
                "MessageBeep answered FALSE: the sound service is off, or the machine has no \
                 audio device",
            ))
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn play_effect(name: &str) -> SoundOutcome {
    unsupported(name)
}

#[cfg(test)]
#[path = "system_sounds_test.rs"]
mod system_sounds_test;
