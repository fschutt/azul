//! Linux: speech-dispatcher, the speech service of the Linux desktops, through its client
//! library `libspeechd.so.2`, dlopen'd: a system without it answers `Unavailable`. It says what
//! it is given in its own order of priorities; whether it is still talking is not something the
//! library tells, so it is estimated from the text's length. Hearing has no native service on
//! Linux yet.

use std::{
    ffi::{c_char, c_int, c_void, CString},
    sync::{Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use super::{HeardStatus, SpeechHeard, SpeechStatus};

type SpdOpen = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, c_int) -> *mut c_void;
type SpdSay = unsafe extern "C" fn(*mut c_void, c_int, *const c_char) -> c_int;
type SpdSetLanguage = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type SpdCancel = unsafe extern "C" fn(*mut c_void) -> c_int;

/// `SPD_MODE_SINGLE`: one connection, used under the mutex.
const MODE_SINGLE: c_int = 0;
/// `SPD_MESSAGE`: said before waiting text, and not cut by it.
const PRIORITY_MESSAGE: c_int = 2;
/// `SPD_TEXT`: said in order.
const PRIORITY_TEXT: c_int = 3;

/// The open connection and the library's calls.
struct Dispatcher {
    /// Kept loaded while the calls below are used.
    _library: libloading::Library,
    connection: *mut c_void,
    say: SpdSay,
    set_language: SpdSetLanguage,
    cancel: SpdCancel,
    /// Until when the last text is probably being said.
    talking_until: Option<Instant>,
}

// SAFETY: the connection is only used under the mutex (single-threaded mode).
unsafe impl Send for Dispatcher {}

static DISPATCHER: OnceLock<Option<Mutex<Dispatcher>>> = OnceLock::new();

/// The connection to speech-dispatcher, opened on first use.
fn dispatcher() -> Option<&'static Mutex<Dispatcher>> {
    DISPATCHER
        .get_or_init(|| unsafe {
            let library = libloading::Library::new("libspeechd.so.2").ok()?;
            let open: SpdOpen = *library.get::<SpdOpen>(b"spd_open\0").ok()?;
            let say: SpdSay = *library.get::<SpdSay>(b"spd_say\0").ok()?;
            let set_language: SpdSetLanguage = *library.get::<SpdSetLanguage>(b"spd_set_language\0").ok()?;
            let cancel: SpdCancel = *library.get::<SpdCancel>(b"spd_cancel\0").ok()?;
            let name = CString::new("azul").ok()?;
            let connection = open(name.as_ptr(), name.as_ptr(), std::ptr::null(), MODE_SINGLE);
            if connection.is_null() {
                return None;
            }
            Some(Mutex::new(Dispatcher {
                _library: library,
                connection,
                say,
                set_language,
                cancel,
                talking_until: None,
            }))
        })
        .as_ref()
}

/// Hands `text` to speech-dispatcher.
pub fn say(text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let (Some(dispatcher), Ok(text_c)) = (dispatcher(), CString::new(text.replace('\0', ""))) else {
        return SpeechStatus::Unavailable;
    };
    let mut d = dispatcher.lock().unwrap_or_else(PoisonError::into_inner);
    unsafe {
        if interrupt {
            (d.cancel)(d.connection);
        }
        // speech-dispatcher names languages by their primary subtag.
        let primary = language.split('-').next().unwrap_or("");
        if let (false, Ok(lang)) = (primary.is_empty(), CString::new(primary)) {
            (d.set_language)(d.connection, lang.as_ptr());
        }
        let priority = if interrupt { PRIORITY_MESSAGE } else { PRIORITY_TEXT };
        if (d.say)(d.connection, priority, text_c.as_ptr()) < 0 {
            return SpeechStatus::Unavailable;
        }
    }
    // About 75 ms a character at a normal rate.
    let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX / 100);
    let lasts = Duration::from_millis(300 + chars.saturating_mul(75));
    let from = match d.talking_until {
        Some(until) if !interrupt && until > Instant::now() => until,
        _ => Instant::now(),
    };
    d.talking_until = Some(from + lasts);
    SpeechStatus::Queued
}

/// Cancels what is said and what waits.
pub fn stop() {
    if let Some(Some(dispatcher)) = DISPATCHER.get() {
        let mut d = dispatcher.lock().unwrap_or_else(PoisonError::into_inner);
        unsafe {
            (d.cancel)(d.connection);
        }
        d.talking_until = None;
    }
}

/// Whether the last text is probably still being said.
pub fn is_speaking() -> bool {
    match DISPATCHER.get() {
        Some(Some(dispatcher)) => dispatcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .talking_until
            .is_some_and(|until| until > Instant::now()),
        _ => false,
    }
}

/// Whether speech-dispatcher is there and answers.
pub fn is_available() -> bool {
    dispatcher().is_some()
}

/// No native speech recognition on Linux yet.
pub fn listen(_language: &str, _timeout_ms: u32) -> SpeechHeard {
    SpeechHeard::none(
        HeardStatus::Unavailable,
        "there is no speech recognition on Linux yet",
    )
}
