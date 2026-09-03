//! macOS media keys via `MPRemoteCommandCenter`.
//!
//! # Why not a CGEventTap
//!
//! The 9h-i note offered two routes: `NSEventTypeSystemDefined` through a
//! `CGEventTap`, or the MediaPlayer framework's remote-command centre. The tap
//! is the wrong one. It needs the ACCESSIBILITY permission - the same TCC gate
//! that lets an app read every keystroke system-wide - which is a heavy thing
//! to ask for a play button, and it intercepts the keys from every other app.
//! `MPRemoteCommandCenter` needs no permission at all and is the sanctioned
//! API; Firefox uses it for exactly this.
//!
//! # It is the same bargain as MPRIS, under a different name
//!
//! macOS only delivers media keys to the app it considers "now playing", and
//! an app becomes that by setting `MPNowPlayingInfoCenter.playbackState`. So
//! registering here puts the app in Control Center and the Now Playing widget,
//! exactly as registering on the session bus puts it in GNOME's media applet.
//! Both are therefore behind the one `AppConfig::expose_system_media_controls`
//! flag, off by default: right for a music app, wrong for a text editor.
//!
//! # Handlers are blocks
//!
//! `addTargetWithHandler:` takes an Objective-C block returning an
//! `MPRemoteCommandHandlerStatus`. `RcBlock` is what the camera-authorisation
//! and audio-sink backends already use for the same purpose.

use azul_core::window::VirtualKeyCode;
use azul_layout::managers::media_keys::push_media_key;

/// `MPRemoteCommandHandlerStatus.success`.
const HANDLER_STATUS_SUCCESS: isize = 0;
/// `MPNowPlayingPlaybackState.playing`.
const PLAYBACK_STATE_PLAYING: isize = 1;

/// Load MediaPlayer.framework. `false` where it is absent.
///
/// dlopen'd rather than linked, like ScreenCaptureKit and IOKit beside it: an
/// app that never turns this on pays nothing, and a missing framework degrades
/// instead of failing to launch.
fn ensure_loaded() -> bool {
    static LIB: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        unsafe {
            libloading::Library::new(
                "/System/Library/Frameworks/MediaPlayer.framework/MediaPlayer",
            )
        }
        .ok()
    })
    .is_some()
}

/// Register for the transport commands.
///
/// Idempotent and quiet: no MediaPlayer framework, or a system that refuses
/// the registration, simply means no media keys - the app still runs.
pub fn start() {
    if !azul_layout::window::expose_system_media_controls() {
        return;
    }
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    if !ensure_loaded() {
        crate::plog_info!("[media-keys] MediaPlayer.framework unavailable");
        return;
    }

    unsafe {
        use block2::RcBlock;
        use objc2::{msg_send, runtime::AnyObject};

        let Ok(center_name) = std::ffi::CString::new("MPRemoteCommandCenter") else {
            return;
        };
        let Some(center_cls) = objc2::runtime::AnyClass::get(&center_name) else {
            return;
        };
        let center: *mut AnyObject = msg_send![center_cls, sharedCommandCenter];
        if center.is_null() {
            return;
        }

        // Each command is a separate object with its own handler. Registering
        // them individually rather than one handler for all is not a choice -
        // `MPRemoteCommand` has no "which command was this" on the event.
        let mut register = |selector: &str, key: VirtualKeyCode| {
            let Ok(name) = std::ffi::CString::new(selector) else {
                return;
            };
            // The commands are PROPERTIES on the shared centre, so each is
            // fetched by its own getter selector rather than by a common call.
            let sel = objc2::runtime::Sel::register(&name);
            let command: *mut AnyObject = msg_send![center, performSelector: sel];
            if command.is_null() {
                return;
            }
            let handler = RcBlock::new(move |_event: *mut AnyObject| -> isize {
                // Parked, not handled here: this runs on whatever thread the
                // media daemon calls on, and the engine's key pass belongs to
                // the main thread. Same channel the MPRIS backend uses.
                push_media_key(key);
                HANDLER_STATUS_SUCCESS
            });
            let _: () = msg_send![command, setEnabled: true];
            let _: *mut AnyObject = msg_send![command, addTargetWithHandler: &*handler];
            // The block must OUTLIVE this scope - the command centre holds it
            // and calls it later - so it is leaked deliberately. There is one
            // per command for the life of the process, and unregistering is
            // not something this backend ever does.
            core::mem::forget(handler);
        };

        // `togglePlayPause` is the one a keyboard's play button actually
        // sends; play/pause are what Control Center's separate buttons send.
        register("togglePlayPauseCommand", VirtualKeyCode::PlayPause);
        register("playCommand", VirtualKeyCode::PlayPause);
        register("pauseCommand", VirtualKeyCode::PlayPause);
        register("stopCommand", VirtualKeyCode::MediaStop);
        register("nextTrackCommand", VirtualKeyCode::NextTrack);
        register("previousTrackCommand", VirtualKeyCode::PrevTrack);

        // WITHOUT THIS THE COMMANDS NEVER FIRE. macOS delivers media keys only
        // to the app it considers "now playing", and an app becomes that by
        // declaring a playback state - it cannot be inferred from an audio
        // session the way it is on iOS. Firefox does the same thing for the
        // same reason.
        let info_name = std::ffi::CString::new("MPNowPlayingInfoCenter").ok();
        if let Some(info_cls) = info_name
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        {
            let info: *mut AnyObject = msg_send![info_cls, defaultCenter];
            if !info.is_null() {
                let _: () = msg_send![info, setPlaybackState: PLAYBACK_STATE_PLAYING];
            }
        }
        crate::plog_info!("[media-keys] MPRemoteCommandCenter registered");
    }
}
