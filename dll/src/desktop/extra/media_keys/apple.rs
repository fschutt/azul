//! Apple media keys and media session via `MPRemoteCommandCenter` +
//! `MPNowPlayingInfoCenter`. ONE FILE FOR macOS AND iOS, because it is one
//! API: both classes, every selector used here, and the framework path are
//! identical, and `playbackState` is iOS 13 / macOS 10.12.
//!
//! # The iOS-only prerequisite this does NOT do
//!
//! On iOS the remote command centre only delivers to an app with an ACTIVE
//! AUDIO SESSION whose category allows playback, and the now-playing info only
//! reaches the lock screen for the app the system considers to be playing.
//! Neither the category nor the activation is set here, and that is deliberate
//! rather than missing: the audio session is the APP's own policy - it decides
//! whether the app ducks other audio, respects the silent switch, or records -
//! and a UI toolkit silently setting `.playback` and activating it would
//! interrupt whatever the user was listening to before the app ever played a
//! note. An app that plays audio activates its own session, at which point
//! everything here starts working. See 9h-i-a-i-d-i.
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

use azul_core::{
    media_session::{MediaPlaybackState, NowPlayingInfo},
    window::VirtualKeyCode,
};

/// `CGSize` / `NSSize`: two `CGFloat`s, which are `f64` on every 64-bit Apple
/// target. Declared here rather than pulled from a crate for the same reason
/// the gamepad backend declares `GcVector3` - the surface needed is one struct.
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct CgSize {
    width: f64,
    height: f64,
}

// SAFETY: matches `struct CGSize { CGFloat width, height; }` exactly, which is
// what the encoding has to describe for objc2 to pass and return it correctly.
unsafe impl objc2::encode::Encode for CgSize {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
        "CGSize",
        &[
            objc2::encode::Encoding::Double,
            objc2::encode::Encoding::Double,
        ],
    );
}

/// The image the current artwork's request handler hands back, retained.
///
/// ONE SLOT, replaced on each publish, because the handler block is copied by
/// `MPMediaItemArtwork` and outlives this call - so the image it returns has to
/// outlive it too. Releasing the PREVIOUS image when a new one is published
/// bounds this at one retained image rather than one per track, which is what a
/// plain leak would give a player that changes tracks all day.
static CURRENT_ARTWORK_IMAGE: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
use azul_core::media_session::{MediaControlKind, MediaControlRequest};
use azul_css::AzString;
use azul_layout::managers::media_keys::{push_media_key, push_media_control};

/// `MPRemoteCommandHandlerStatus.success`.
const HANDLER_STATUS_SUCCESS: isize = 0;
/// `MPNowPlayingPlaybackState`. The two azul does not use - `unknown` (0) and
/// `interrupted` (4) - are things the SYSTEM asserts about a player, not things
/// a player asserts about itself.
const PLAYBACK_STATE_PLAYING: isize = 1;
const PLAYBACK_STATE_PAUSED: isize = 2;
const PLAYBACK_STATE_STOPPED: isize = 3;

/// Load MediaPlayer.framework. `false` where it is absent.
///
/// dlopen'd rather than linked, like ScreenCaptureKit and IOKit beside it: an
/// app that never turns this on pays nothing, and a missing framework degrades
/// instead of failing to launch.
fn library() -> Option<&'static libloading::Library> {
    static LIB: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        unsafe {
            libloading::Library::new(
                "/System/Library/Frameworks/MediaPlayer.framework/MediaPlayer",
            )
        }
        .ok()
    })
    .as_ref()
}

fn ensure_loaded() -> bool {
    library().is_some()
}

/// Read one of MediaPlayer.framework's `NSString * const` dictionary keys.
///
/// These are EXPORTED SYMBOLS, not literals, and their string values are not
/// documented - `MPMediaItemPropertyTitle` is not guaranteed to be `"title"`,
/// so hardcoding a guess would silently produce a dictionary the framework
/// ignores. Since the framework is dlopen'd rather than linked, each one has to
/// be looked up by name: `dlsym` gives the ADDRESS OF THE VARIABLE, and the
/// variable holds the `NSString *` - which is what `Symbol<*mut AnyObject>`
/// dereferences to.
///
/// A missing key means that one field is dropped, not that publishing fails -
/// a future macOS renaming one constant should cost a subtitle, not the widget.
unsafe fn info_key(symbol: &[u8]) -> Option<*mut objc2::runtime::AnyObject> {
    let lib = library()?;
    // `Symbol<T>` derefs to the symbol's ADDRESS typed as `T` (libloading's
    // own example is `**awesome_variable = 42.0`), so for a variable holding
    // an `NSString *` that is `*mut *mut AnyObject`, and the string is one
    // more dereference away. This used to be `Symbol<*mut AnyObject>` with a
    // single `*`, which handed the framework the ADDRESS OF THE VARIABLE as
    // the dictionary key - a pointer into MediaPlayer's data segment posing
    // as an object. Found while writing the CoreHaptics constants lookup
    // (9g-i-d-a-i), which is the same shape.
    let sym: libloading::Symbol<'_, *mut *mut objc2::runtime::AnyObject> =
        unsafe { lib.get(symbol) }.ok()?;
    let slot: *mut *mut objc2::runtime::AnyObject = *sym;
    if slot.is_null() {
        return None;
    }
    let ptr: *mut objc2::runtime::AnyObject = unsafe { *slot };
    if ptr.is_null() {
        None
    } else {
        Some(ptr)
    }
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

        // THE SCRUBBER (9h-i-a-i-a-i). `changePlaybackPositionCommand` is the
        // one command whose event carries a value: an
        // `MPChangePlaybackPositionCommandEvent` with `positionTime`, an
        // `NSTimeInterval` in SECONDS (macOS 10.12.2+ / iOS 8+). Enabling it
        // is what turns the Now Playing progress bar draggable; a handler
        // that pushes a key could not carry the position, hence its own
        // block onto the seek queue.
        if let Ok(name) = std::ffi::CString::new("changePlaybackPositionCommand") {
            let sel = objc2::runtime::Sel::register(&name);
            let command: *mut AnyObject = msg_send![center, performSelector: sel];
            if !command.is_null() {
                let handler = RcBlock::new(move |event: *mut AnyObject| -> isize {
                    if !event.is_null() {
                        let seconds: f64 = msg_send![event, positionTime];
                        if seconds.is_finite() {
                            push_media_control(MediaControlRequest {
                                kind: MediaControlKind::SeekAbsolute,
                                position_us: (seconds.max(0.0) * 1_000_000.0) as i64,
                                uri: AzString::from_const_str(""),
                                track_id: AzString::from_const_str(""),
                                volume: 0.0,
                            });
                        }
                    }
                    HANDLER_STATUS_SUCCESS
                });
                let _: () = msg_send![command, setEnabled: true];
                let _: *mut AnyObject = msg_send![command, addTargetWithHandler: &*handler];
                core::mem::forget(handler);
            }
        }

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

/// Publish the app's session into `MPNowPlayingInfoCenter`.
///
/// # Why this is the same object as the command centre
///
/// macOS only delivers media keys to the app it considers "now playing", and
/// that is decided by this very centre. So publishing is not a cosmetic extra
/// on top of `start` - it is what keeps the app eligible for the keys it
/// registered for, and a player that goes `Stopped` correctly stops receiving
/// them.
///
/// # Artwork is dropped
///
/// `MPMediaItemPropertyArtwork` wants an `MPMediaItemArtwork`, which wraps a
/// decoded `NSImage`. `NowPlayingInfo::artwork_url` is a URI, because that is
/// what MPRIS takes, and turning it into pixels means fetching a URL and
/// decoding an image from inside a UI toolkit's event loop. That is a real
/// feature, not a line of glue - logged as 9h-i-a-i-d.
pub fn publish(info: &NowPlayingInfo) {
    if !ensure_loaded() {
        return;
    }

    unsafe {
        use objc2::{msg_send, runtime::AnyObject};

        let Some(info_cls) = std::ffi::CString::new("MPNowPlayingInfoCenter")
            .ok()
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        else {
            return;
        };
        let center: *mut AnyObject = msg_send![info_cls, defaultCenter];
        if center.is_null() {
            return;
        }

        // A DICTIONARY, replaced wholesale on every publish. Mutating the one
        // the centre already holds is not an option: `nowPlayingInfo` returns a
        // COPY, so writes to it would go nowhere.
        let Some(dict_cls) = std::ffi::CString::new("NSMutableDictionary")
            .ok()
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        else {
            return;
        };
        let dict: *mut AnyObject = msg_send![dict_cls, dictionary];
        if dict.is_null() {
            return;
        }

        let mut put = |symbol: &[u8], value: *mut AnyObject| {
            if value.is_null() {
                return;
            }
            if let Some(key) = info_key(symbol) {
                let _: () = msg_send![dict, setObject: value, forKey: key];
            }
        };

        if !info.title.as_str().is_empty() {
            put(b"MPMediaItemPropertyTitle\0", nsstring(info.title.as_str()));
        }
        if !info.artist.as_str().is_empty() {
            put(
                b"MPMediaItemPropertyArtist\0",
                nsstring(info.artist.as_str()),
            );
        }
        if !info.album.as_str().is_empty() {
            put(
                b"MPMediaItemPropertyAlbumTitle\0",
                nsstring(info.album.as_str()),
            );
        }
        if info.duration_ms != 0 {
            // SECONDS as a double here, where MPRIS wants microseconds as an
            // integer. Publishing milliseconds would make every track look
            // 1000x too long and the scrubber never move.
            put(
                b"MPMediaItemPropertyPlaybackDuration\0",
                nsnumber(info.duration_ms as f64 / 1000.0),
            );
        }
        put(
            b"MPNowPlayingInfoPropertyElapsedPlaybackTime\0",
            nsnumber(info.position_ms as f64 / 1000.0),
        );
        // WITHOUT A RATE THE TIME DOES NOT MOVE. Control Center advances the
        // elapsed time itself by extrapolating from the rate; a paused player
        // publishes 0.0 so the display holds, and a playing one publishes 1.0
        // so it ticks between publishes instead of freezing.
        let rate = if info.state == MediaPlaybackState::Playing {
            1.0
        } else {
            0.0
        };
        put(
            b"MPNowPlayingInfoPropertyPlaybackRate\0",
            nsnumber(rate),
        );
        if !info.artwork_url.as_str().is_empty() {
            put(
                b"MPMediaItemPropertyArtwork\0",
                artwork_for_url(info.artwork_url.as_str()),
            );
        }

        let _: () = msg_send![center, setNowPlayingInfo: dict];

        let state = match info.state {
            MediaPlaybackState::Playing => PLAYBACK_STATE_PLAYING,
            MediaPlaybackState::Paused => PLAYBACK_STATE_PAUSED,
            MediaPlaybackState::Stopped => PLAYBACK_STATE_STOPPED,
        };
        let _: () = msg_send![center, setPlaybackState: state];
    }
}

/// Bytes for an artwork URI, fetched through the shared resolver.
///
/// LOCAL URIs are read on this thread - a disk read on the event loop is
/// cheap and a player only changes tracks occasionally. A REMOTE one is
/// fetched on a thread of its own and cached; the publish that asked for it
/// goes out without a cover, and the NEXT one picks it up. A player publishes
/// its position continuously, so "the next one" is a frame away - which is why
/// this needs no re-publish machinery of its own.
///
/// One fetch per URL: an in-flight entry blocks a second attempt, or a player
/// publishing at 60 Hz would open sixty connections for one cover.
fn artwork_bytes(uri: &str) -> Option<Vec<u8>> {
    use azul_layout::fetch::{route_of, UriRoute};

    match route_of(uri) {
        UriRoute::LocalPath(_) => azul_layout::fetch::fetch_uri(uri).ok(),
        UriRoute::Remote(url) => {
            let mut cache = ARTWORK_CACHE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match cache.get(&url) {
                // `None` means a fetch is in flight or failed; either way, do
                // not start another.
                Some(entry) => entry.clone(),
                None => {
                    cache.insert(url.clone(), None);
                    drop(cache);
                    std::thread::Builder::new()
                        .name("azul-artwork".into())
                        .spawn(move || {
                            let bytes = azul_layout::fetch::fetch_uri(&url).ok();
                            let mut cache = ARTWORK_CACHE
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            cache.insert(url, bytes);
                        })
                        .ok();
                    None
                }
            }
        }
        UriRoute::Unsupported(scheme) => {
            static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
            if WARNED.set(()).is_ok() {
                crate::plog_info!(
                    "[media-session] artwork_url scheme `{}` cannot be fetched",
                    scheme
                );
            }
            None
        }
    }
}

/// Fetched remote artwork, by URL. `None` = in flight, or failed.
static ARTWORK_CACHE: std::sync::Mutex<
    std::collections::BTreeMap<String, Option<Vec<u8>>>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Build an `MPMediaItemArtwork` from an artwork URI, or null.
///
/// FROM BYTES, not from a path: `initWithData:` covers a downloaded cover and a
/// local file alike, where `initWithContentsOfFile:` covers only the second and
/// left every remote URL unhandled.
unsafe fn artwork_for_url(url: &str) -> *mut objc2::runtime::AnyObject {
    let null = core::ptr::null_mut();
    let Some(bytes) = artwork_bytes(url) else {
        return null;
    };
    if bytes.is_empty() {
        return null;
    }

    let Some(data_cls) = std::ffi::CString::new("NSData")
        .ok()
        .as_deref()
        .and_then(objc2::runtime::AnyClass::get)
    else {
        return null;
    };
    // COPIES the bytes, so the Vec may drop when this returns.
    let data: *mut objc2::runtime::AnyObject = objc2::msg_send![
        data_cls,
        dataWithBytes: bytes.as_ptr().cast::<core::ffi::c_void>(),
        length: bytes.len(),
    ];
    if data.is_null() {
        return null;
    }

    #[cfg(target_os = "macos")]
    let image: *mut objc2::runtime::AnyObject = {
        let Some(cls) = std::ffi::CString::new("NSImage")
            .ok()
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        else {
            return null;
        };
        let alloc: *mut objc2::runtime::AnyObject = objc2::msg_send![cls, alloc];
        objc2::msg_send![alloc, initWithData: data]
    };
    #[cfg(target_os = "ios")]
    let image: *mut objc2::runtime::AnyObject = {
        let Some(cls) = std::ffi::CString::new("UIImage")
            .ok()
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        else {
            return null;
        };
        objc2::msg_send![cls, imageWithData: data]
    };
    if image.is_null() {
        // Bytes the platform will not decode. Not an error: the track still
        // publishes without a cover.
        return null;
    }

    // RETAIN, because the handler block below outlives this function: the
    // artwork object copies the block and calls it later, off this stack.
    let retained: *mut objc2::runtime::AnyObject = objc2::msg_send![image, retain];
    let previous = {
        let mut slot = CURRENT_ARTWORK_IMAGE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        core::mem::replace(&mut *slot, retained as usize)
    };
    if previous != 0 {
        let old = previous as *mut objc2::runtime::AnyObject;
        let _: () = objc2::msg_send![old, release];
    }

    let size: CgSize = objc2::msg_send![retained, size];

    let Some(artwork_cls) = std::ffi::CString::new("MPMediaItemArtwork")
        .ok()
        .as_deref()
        .and_then(objc2::runtime::AnyClass::get)
    else {
        return null;
    };
    // `initWithBoundsSize:requestHandler:` is the ONLY initialiser on macOS -
    // `initWithImage:` is iPhone-only and deprecated - so there is no simpler
    // route to weigh up. The handler ignores the requested size and returns the
    // full image; the system scales.
    let handler = block2::RcBlock::new(
        move |_size: CgSize| -> *mut objc2::runtime::AnyObject { retained },
    );
    let alloc: *mut objc2::runtime::AnyObject = objc2::msg_send![artwork_cls, alloc];
    // The block is COPIED by the initialiser, so `handler` may drop here; the
    // image it returns is what needed the retain above.
    objc2::msg_send![
        alloc,
        initWithBoundsSize: size,
        requestHandler: block2::RcBlock::as_ptr(&handler),
    ]
}

/// An autoreleased `NSString` from a Rust string, or null if it contains an
/// interior NUL.
/// AVFAudio, dlopen'd like MediaPlayer above: `AVAudioSession` lives there
/// (iOS 14.5+, re-exported by AVFoundation), and the constants this needs
/// are exported NSString globals - read with the same double dereference as
/// [`info_key`], because `dlsym` hands back the address OF the global.
///
/// macOS TOO (9h-i-a-i-d-i-a): `AVAudioSession` exists on macOS since 11 and
/// the framework sits at the same path there. Nothing links it, so a Mac
/// without the framework, the class, a constant, or one of the selectors
/// (several `AVAudioSession` methods are `API_UNAVAILABLE(macos)` in the
/// headers and the runtime class may simply not respond) degrades to the
/// no-op the desktop always had - see [`AVF_ABSENT`].
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn avfaudio() -> Option<&'static libloading::Library> {
    static LIB: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        unsafe {
            libloading::Library::new("/System/Library/Frameworks/AVFAudio.framework/AVFAudio")
                .or_else(|_| {
                    libloading::Library::new(
                        "/System/Library/Frameworks/AVFoundation.framework/AVFoundation",
                    )
                })
        }
        .ok()
    })
    .as_ref()
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
unsafe fn avf_constant(symbol: &[u8]) -> Option<*mut objc2::runtime::AnyObject> {
    let lib = avfaudio()?;
    let sym: libloading::Symbol<'_, *mut *mut objc2::runtime::AnyObject> =
        unsafe { lib.get(symbol) }.ok()?;
    let slot: *mut *mut objc2::runtime::AnyObject = *sym;
    if slot.is_null() {
        return None;
    }
    let ptr: *mut objc2::runtime::AnyObject = unsafe { *slot };
    if ptr.is_null() {
        None
    } else {
        Some(ptr)
    }
}

/// `AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation`: on release,
/// tell the apps we interrupted that they may resume.
#[cfg(any(target_os = "ios", target_os = "macos"))]
const SET_ACTIVE_NOTIFY_OTHERS: usize = 1;
/// `AVAudioSessionInterruptionTypeBegan`; `Ended` is 0.
#[cfg(any(target_os = "ios", target_os = "macos"))]
const INTERRUPTION_BEGAN: usize = 1;
/// `AVAudioSessionInterruptionOptionShouldResume`.
#[cfg(any(target_os = "ios", target_os = "macos"))]
const INTERRUPTION_SHOULD_RESUME: usize = 1;

/// What the takeover answers when there is no session to take
/// (9h-i-a-i-d-i-a): on iOS "refused", because a phone's remote command
/// centre delivers nothing without an active session; on macOS "owned",
/// because the desktop mixer shares and the command centre works without
/// one - the answer `media_keys::set_system_audio_takeover` gave before
/// macOS took this path at all. Every guard below returns this, so a Mac
/// missing any piece of AVFAudio behaves exactly as it did.
#[cfg(any(target_os = "ios", target_os = "macos"))]
const AVF_ABSENT: Option<bool> = Some(cfg!(target_os = "macos"));

/// Activate (or deactivate) the shared `AVAudioSession` with the playback
/// category (9h-i-a-i-d-i). Activation is what makes the remote command
/// centre deliver anything - and what interrupts other apps' audio, which is
/// why it is a runtime call around playback rather than a config flag.
///
/// On macOS (9h-i-a-i-d-i-a, USER RULING 2026-09-04: implement blindly) the
/// same dlopen path runs; every step that a Mac may lack is guarded with a
/// debug log and falls back to [`AVF_ABSENT`]. `respondsToSelector:` guards
/// the three session methods because they are declared for iOS in the
/// headers and a macOS class that exists may still not implement them.
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub fn set_system_audio_takeover(active: bool) -> Option<bool> {
    use objc2::{msg_send, runtime::AnyObject};

    if avfaudio().is_none() {
        crate::plog_debug!("[media-session] AVFAudio unavailable; no session to take");
        return AVF_ABSENT;
    }
    unsafe {
        let Ok(name) = std::ffi::CString::new("AVAudioSession") else {
            return AVF_ABSENT;
        };
        let Some(cls) = objc2::runtime::AnyClass::get(&name) else {
            crate::plog_debug!("[media-session] AVAudioSession class absent; no session to take");
            return AVF_ABSENT;
        };
        let cls_responds: bool = msg_send![cls, respondsToSelector: objc2::sel!(sharedInstance)];
        if !cls_responds {
            crate::plog_debug!("[media-session] AVAudioSession has no sharedInstance here");
            return AVF_ABSENT;
        }
        let session: *mut AnyObject = msg_send![cls, sharedInstance];
        if session.is_null() {
            return AVF_ABSENT;
        }
        let responds = |sel: objc2::runtime::Sel| -> bool {
            msg_send![session, respondsToSelector: sel]
        };
        if active {
            if !responds(objc2::sel!(setCategory:error:))
                || !responds(objc2::sel!(setActive:error:))
            {
                crate::plog_debug!(
                    "[media-session] AVAudioSession cannot be activated on this platform"
                );
                return AVF_ABSENT;
            }
            let Some(category) = avf_constant(b"AVAudioSessionCategoryPlayback\0") else {
                crate::plog_debug!("[media-session] AVAudioSessionCategoryPlayback missing");
                return AVF_ABSENT;
            };
            // Explicit out-pointers rather than objc2's `error: _` shorthand:
            // that one wants a typed `NSError` class, and this file works on
            // untyped objects from a dlopen'd framework. The error object is
            // autoreleased; only the BOOL is read.
            let mut err: *mut AnyObject = core::ptr::null_mut();
            let set: bool = msg_send![session, setCategory: category, error: &mut err];
            if !set {
                crate::plog_info!("[media-session] AVAudioSession setCategory failed");
                return Some(false);
            }
            let mut err: *mut AnyObject = core::ptr::null_mut();
            let activated: bool = msg_send![session, setActive: true, error: &mut err];
            if !activated {
                crate::plog_info!("[media-session] AVAudioSession setActive failed");
                return Some(false);
            }
            install_interruption_observer();
            Some(true)
        } else {
            if !responds(objc2::sel!(setActive:withOptions:error:)) {
                crate::plog_debug!(
                    "[media-session] AVAudioSession cannot be released on this platform"
                );
                return AVF_ABSENT;
            }
            let mut err: *mut AnyObject = core::ptr::null_mut();
            let released: bool = msg_send![
                session,
                setActive: false,
                withOptions: SET_ACTIVE_NOTIFY_OTHERS,
                error: &mut err
            ];
            Some(released)
        }
    }
}

/// Observe `AVAudioSessionInterruptionNotification` once, for the lifetime
/// of the process, and turn each one into a [`SystemAudioChange`]: began =
/// `Interrupted`; ended = `Resumed` with the should-resume hint, `Ended`
/// without it (Apple: do not resume on your own then).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn install_interruption_observer() {
    use azul_core::media_session::SystemAudioChange;
    use azul_layout::managers::media_keys::push_system_audio_change;
    use block2::RcBlock;
    use objc2::{msg_send, runtime::AnyObject};

    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    unsafe {
        let (Some(note_name), Some(type_key), Some(option_key)) = (
            avf_constant(b"AVAudioSessionInterruptionNotification\0"),
            avf_constant(b"AVAudioSessionInterruptionTypeKey\0"),
            avf_constant(b"AVAudioSessionInterruptionOptionKey\0"),
        ) else {
            crate::plog_debug!("[media-session] interruption constants missing; not observed");
            return;
        };
        let Ok(center_name) = std::ffi::CString::new("NSNotificationCenter") else {
            return;
        };
        let Some(center_cls) = objc2::runtime::AnyClass::get(&center_name) else {
            return;
        };
        let center: *mut AnyObject = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            return;
        }
        // The keys are process-lifetime constants, so the raw pointers the
        // block captures never dangle.
        let type_key = type_key as usize;
        let option_key = option_key as usize;
        let handler = RcBlock::new(move |note: *mut AnyObject| {
            if note.is_null() {
                return;
            }
            let info: *mut AnyObject = msg_send![note, userInfo];
            if info.is_null() {
                return;
            }
            let type_key = type_key as *mut AnyObject;
            let option_key = option_key as *mut AnyObject;
            let ty: *mut AnyObject = msg_send![info, objectForKey: type_key];
            let ty: usize = if ty.is_null() {
                0
            } else {
                msg_send![ty, unsignedIntegerValue]
            };
            let change = if ty == INTERRUPTION_BEGAN {
                SystemAudioChange::Interrupted
            } else {
                let opts: *mut AnyObject = msg_send![info, objectForKey: option_key];
                let opts: usize = if opts.is_null() {
                    0
                } else {
                    msg_send![opts, unsignedIntegerValue]
                };
                if opts & INTERRUPTION_SHOULD_RESUME != 0 {
                    SystemAudioChange::Resumed
                } else {
                    SystemAudioChange::Ended
                }
            };
            push_system_audio_change(change);
        });
        let nil: *mut AnyObject = core::ptr::null_mut();
        let _: *mut AnyObject = msg_send![
            center,
            addObserverForName: note_name,
            object: nil,
            queue: nil,
            usingBlock: &*handler
        ];
        core::mem::forget(handler);
    }
}

unsafe fn nsstring(s: &str) -> *mut objc2::runtime::AnyObject {
    use objc2::{msg_send, runtime::AnyObject};

    let Ok(c) = std::ffi::CString::new(s) else {
        return core::ptr::null_mut();
    };
    let Some(cls) = std::ffi::CString::new("NSString")
        .ok()
        .as_deref()
        .and_then(objc2::runtime::AnyClass::get)
    else {
        return core::ptr::null_mut();
    };
    unsafe { msg_send![cls, stringWithUTF8String: c.as_ptr()] }
}

/// An autoreleased `NSNumber` holding a double.
unsafe fn nsnumber(v: f64) -> *mut objc2::runtime::AnyObject {
    use objc2::{msg_send, runtime::AnyObject};

    let Some(cls) = std::ffi::CString::new("NSNumber")
        .ok()
        .as_deref()
        .and_then(objc2::runtime::AnyClass::get)
    else {
        return core::ptr::null_mut();
    };
    unsafe { msg_send![cls, numberWithDouble: v] }
}
