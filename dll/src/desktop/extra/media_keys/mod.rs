//! Media keys that do not arrive through the keyboard.
//!
//! - **Linux**: MPRIS over D-Bus, for the (usual) case where the desktop has
//!   grabbed the media row. Opt-in via `AppConfig::expose_mpris_media_controls`.
//! - **Windows**: `WM_APPCOMMAND` delivers the keys, and SMTC
//!   (`SystemMediaTransportControls`) is what the app publishes INTO. Both can
//!   report the same press; see `windows.rs` for why that is safe.
//! - **macOS**: `MPRemoteCommandCenter`, which needs NO permission (unlike a
//!   CGEventTap) but does require becoming the "now playing" app. Same opt-in
//!   flag as Linux.
//!
//! The transport is only half of it. The same platform object also PUBLISHES
//! what the app is playing - `publish_now_playing` below - because on Linux and
//! macOS alike an app becomes eligible to receive the keys by declaring a
//! session. See `azul_core::media_session`.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Start any out-of-band media-key transport this platform has.
///
/// `window` is the app's own top-level window handle. Only Windows needs it -
/// SMTC attaches to a WINDOW rather than to a process - and it is passed in
/// rather than guessed with `GetForegroundWindow`, which would attach the
/// session to whatever the user happened to be looking at when the app
/// started.
pub fn ensure_started(window: azul_core::window::RawWindowHandle) {
    #[cfg(target_os = "linux")]
    linux::start();
    #[cfg(target_os = "macos")]
    macos::start();
    #[cfg(target_os = "windows")]
    windows::start(hwnd_of(window));
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let _ = window;
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
    #[cfg(target_os = "macos")]
    macos::publish(info);
    #[cfg(target_os = "windows")]
    windows::publish(info);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let _ = info;
}
