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
use azul_layout::managers::media_keys::push_media_key;

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
    let sym: libloading::Symbol<'_, *mut objc2::runtime::AnyObject> =
        unsafe { lib.get(symbol) }.ok()?;
    let ptr: *mut objc2::runtime::AnyObject = *sym;
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
