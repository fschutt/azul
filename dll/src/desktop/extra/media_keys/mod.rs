//! Media keys that do not arrive through the keyboard.
//!
//! - **Linux**: MPRIS over D-Bus, for the (usual) case where the desktop has
//!   grabbed the media row. Opt-in via `AppConfig::expose_mpris_media_controls`.
//! - **Android**: `MediaSession`, which is BOTH halves at once - the same
//!   object receives the transport buttons and carries the metadata.
//! - **Windows**: `WM_APPCOMMAND` delivers the keys, and SMTC
//!   (`SystemMediaTransportControls`) is what the app publishes INTO. Both can
//!   report the same press; see `windows.rs` for why that is safe.
//! - **macOS and iOS**: `MPRemoteCommandCenter` plus `MPNowPlayingInfoCenter`,
//!   one file for both because the API is the same one - it needs NO
//!   permission (unlike a CGEventTap) but does require becoming the "now
//!   playing" app. Same opt-in flag as Linux.
//!
//! The transport is only half of it. The same platform object also PUBLISHES
//! what the app is playing - `publish_now_playing` below - because on Linux and
//! macOS alike an app becomes eligible to receive the keys by declaring a
//! session. See `azul_core::media_session`.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod apple;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "android")]
pub mod android;

/// Start any out-of-band media-key transport this platform has.
///
/// `window` is the app's own top-level window handle. Only Windows needs it -
/// SMTC attaches to a WINDOW rather than to a process - and it is passed in
/// rather than guessed with `GetForegroundWindow`, which would attach the
/// session to whatever the user happened to be looking at when the app
/// started.
pub fn ensure_started(window: azul_core::window::RawWindowHandle) {
    #[cfg(target_os = "linux")]
    linux::start(registry_id_of(window));
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    apple::start();
    #[cfg(target_os = "windows")]
    windows::start(hwnd_of(window));
    #[cfg(target_os = "android")]
    android::start();
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "android"
    )))]
    let _ = window;
}

/// The same id `PlatformWindow::registry_window_id` derives - what a raise
/// request names (9h-i-a-ii).
#[cfg(target_os = "linux")]
fn registry_id_of(window: azul_core::window::RawWindowHandle) -> u64 {
    use azul_core::window::RawWindowHandle;
    match window {
        RawWindowHandle::Xlib(h) => h.window,
        RawWindowHandle::Wayland(h) => h.surface as usize as u64,
        _ => 0,
    }
}

/// The `HWND` behind a raw window handle, or `0` where there is none.
#[cfg(target_os = "windows")]
fn hwnd_of(window: azul_core::window::RawWindowHandle) -> isize {
    match window {
        azul_core::window::RawWindowHandle::Windows(h) => h.hwnd as isize,
        _ => 0,
    }
}

/// Publish what the app is playing to the platform's media session.
///
/// The app-facing half of the same object: on every platform an app becomes
/// eligible to RECEIVE `Play`/`Next` by declaring what it is PLAYING, so this
/// and `ensure_started` drive one registration and not two.
///
/// Starting is idempotent and flag-gated, so calling it here costs nothing and
/// closes the ordering hole: an app that publishes before its first media key
/// would otherwise have no session to publish into.
///
/// Platforms with no session API (Windows, and the mobile shells) ignore this
/// rather than failing - see 9h-i-a-i-b/c in the ledger.
pub fn publish_now_playing(
    info: &azul_core::media_session::NowPlayingInfo,
    window: azul_core::window::RawWindowHandle,
) {
    if !azul_layout::window::expose_system_media_controls() {
        // Not an error, but silently doing nothing when an app asked for
        // something visible is exactly the kind of gap this whole backlog is
        // about, so say so - once, because a player republishes constantly.
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if WARNED.set(()).is_ok() {
            crate::plog_info!(
                "[media-session] set_now_playing ignored: \
                 AppConfig::expose_system_media_controls is off, so this app is \
                 not registered as a media player"
            );
        }
        return;
    }
    ensure_started(window);

    #[cfg(target_os = "linux")]
    linux::publish(info);
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    apple::publish(info);
    #[cfg(target_os = "windows")]
    windows::publish(info);
    #[cfg(target_os = "android")]
    android::publish(info);
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "android"
    )))]
    let _ = info;
}

// ---------------------------------------------------------------------------
// Android media keycodes.
//
// Here rather than in `android.rs` for the same reason `sensors/units.rs`
// exists: that file is cfg-gated to a target this machine never runs tests on,
// and a mapping table is exactly the thing that goes wrong silently. This
// module is compiled everywhere, so the tests below actually run.
// ---------------------------------------------------------------------------

use azul_core::window::VirtualKeyCode;

/// `KeyEvent.KEYCODE_MEDIA_*`, Android's own values.
const KEYCODE_MEDIA_PLAY_PAUSE: i32 = 85;
const KEYCODE_MEDIA_STOP: i32 = 86;
const KEYCODE_MEDIA_NEXT: i32 = 87;
const KEYCODE_MEDIA_PREVIOUS: i32 = 88;
const KEYCODE_MEDIA_PLAY: i32 = 126;
const KEYCODE_MEDIA_PAUSE: i32 = 127;

/// Map an Android media keycode onto the ordinary key every other media-key
/// producer delivers.
///
/// `PLAY` and `PAUSE` both become `PlayPause`, matching the keysym table, the
/// `WM_APPCOMMAND` arm and SMTC: a keyboard's play button is a toggle, and an
/// app that bound the toggle must not miss a headset's separate buttons.
/// Anything else maps to nothing rather than to a wrong key - a headset sends
/// plenty of codes this app never asked for.
pub(crate) fn media_keycode_to_key(keycode: i32) -> Option<VirtualKeyCode> {
    Some(match keycode {
        KEYCODE_MEDIA_PLAY_PAUSE | KEYCODE_MEDIA_PLAY | KEYCODE_MEDIA_PAUSE => {
            VirtualKeyCode::PlayPause
        }
        KEYCODE_MEDIA_STOP => VirtualKeyCode::MediaStop,
        KEYCODE_MEDIA_NEXT => VirtualKeyCode::NextTrack,
        KEYCODE_MEDIA_PREVIOUS => VirtualKeyCode::PrevTrack,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PLAY and PAUSE are separate buttons on a headset and both mean the
    /// toggle here, matching every other backend. A code this app never asked
    /// for maps to nothing rather than to a wrong key.
    #[test]
    fn the_media_keycodes_map_the_way_every_other_backend_does() {
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_PLAY_PAUSE),
            Some(VirtualKeyCode::PlayPause)
        );
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_PLAY),
            Some(VirtualKeyCode::PlayPause)
        );
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_PAUSE),
            Some(VirtualKeyCode::PlayPause)
        );
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_STOP),
            Some(VirtualKeyCode::MediaStop)
        );
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_NEXT),
            Some(VirtualKeyCode::NextTrack)
        );
        assert_eq!(
            media_keycode_to_key(KEYCODE_MEDIA_PREVIOUS),
            Some(VirtualKeyCode::PrevTrack)
        );

        // KEYCODE_A, KEYCODE_HEADSETHOOK and KEYCODE_MEDIA_RECORD: real
        // codes a headset or keyboard sends that this must NOT answer.
        for other in [29, 79, 130, 0, -1] {
            assert_eq!(
                media_keycode_to_key(other),
                None,
                "keycode {other} must not map to a media key"
            );
        }
    }

    /// The constants are ANDROID's, so they are checked against the platform's
    /// documented values rather than against themselves.
    #[test]
    fn the_constants_are_the_android_platform_values() {
        assert_eq!(KEYCODE_MEDIA_PLAY_PAUSE, 85);
        assert_eq!(KEYCODE_MEDIA_STOP, 86);
        assert_eq!(KEYCODE_MEDIA_NEXT, 87);
        assert_eq!(KEYCODE_MEDIA_PREVIOUS, 88);
        assert_eq!(KEYCODE_MEDIA_PLAY, 126);
        assert_eq!(KEYCODE_MEDIA_PAUSE, 127);
    }
}

/// The app reported a position that is not the continuation of the last one
/// (9h-i-a-i-a): tell the platform so its scrubber re-syncs. MPRIS has the
/// `Seeked` signal for exactly this; SMTC and the Apple command centre read
/// the position back from the published session, so they need nothing here.
#[allow(unused_variables)]
pub fn announce_seeked(position_us: i64) {
    #[cfg(target_os = "linux")]
    linux::announce_seeked(position_us);
}

/// Take over (or release) the system audio (9h-i-a-i-d-i).
///
/// `Some(true)` = the app owns it now, `Some(false)` = refused, `None` =
/// the platform will answer later through `push_system_audio_change`
/// (Android's delayed focus grant). Desktop mixers share, so there is
/// nothing to take and the answer is `Some(true)`; the SMTC and MPRIS
/// controls work without any of this. macOS (9h-i-a-i-d-i-a) runs the same
/// `AVAudioSession` path as iOS and answers `Some(true)` wherever a piece of
/// AVFAudio is missing, so a Mac without it is the desktop case above.
#[allow(unused_variables)]
pub fn set_system_audio_takeover(active: bool) -> Option<bool> {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        return apple::set_system_audio_takeover(active);
    }
    #[cfg(target_os = "android")]
    {
        return android::set_system_audio_takeover(active);
    }
    #[allow(unreachable_code)]
    Some(true)
}
