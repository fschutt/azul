//! macOS and iOS: `AVSpeechSynthesizer` says the text; `SFSpeechRecognizer` hears what the
//! microphone picks up, fed by a tap on `AVAudioEngine`'s input. AVFoundation and Speech are
//! dlopen'd, nothing links them: a system without one answers `Unavailable`.
//!
//! Hearing needs two permissions, speech recognition and the microphone. What the system says
//! about them is read FIRST, without asking ([`authorization::decide`]): a bare binary such as
//! `target/release/<app>` has no Info.plist, but a terminal that holds both permissions makes it
//! `Authorized` (measured on macOS 15.5), and then it listens. Only a permission nobody decided
//! yet needs the usage texts (`NSSpeechRecognitionUsageDescription`,
//! `NSMicrophoneUsageDescription`) in the app's Info.plist - asking without them stops the
//! process - so without them it is not asked: an `Unavailable` that says what to do.

#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::{
    ffi::{c_char, CStr, CString},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex, OnceLock, PoisonError,
    },
    time::{Duration, Instant},
};

use block2::RcBlock;
use objc2::{
    msg_send,
    runtime::{AnyClass, AnyObject, Bool},
};

use super::{
    authorization::{self, Access, Decision, Gate, Gates, Refusal},
    HeardStatus, SpeechHeard, SpeechStatus,
};

/// The synthesizer, retained for the life of the process; `0` until the first utterance. A
/// pointer is kept as a number: raw pointers are not `Sync`.
static SYNTHESIZER: AtomicUsize = AtomicUsize::new(0);

/// `AVSpeechBoundaryImmediate`: stop mid-word.
const BOUNDARY_IMMEDIATE: isize = 0;

/// `SFSpeechRecognizerAuthorizationStatus`.
const AUTH_NOT_DETERMINED: isize = 0;
const AUTH_AUTHORIZED: isize = 3;

/// How long the user may pause before what they said counts as finished.
const SILENCE: Duration = Duration::from_millis(1_500);

/// How long the recognizer may take for its final answer once the audio ended.
const FINAL_WAIT: Duration = Duration::from_secs(2);

/// A framework at `path`, loaded once.
fn framework(slot: &'static OnceLock<Option<libloading::Library>>, path: &str) -> Option<&'static libloading::Library> {
    slot.get_or_init(|| unsafe { libloading::Library::new(path) }.ok())
        .as_ref()
}

fn avfoundation() -> Option<&'static libloading::Library> {
    static LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();
    framework(&LIB, "/System/Library/Frameworks/AVFoundation.framework/AVFoundation")
}

fn speech_framework() -> Option<&'static libloading::Library> {
    static LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();
    framework(&LIB, "/System/Library/Frameworks/Speech.framework/Speech")
}

fn class(name: &str) -> Option<&'static AnyClass> {
    let c = CString::new(name).ok()?;
    AnyClass::get(&c)
}

/// An autoreleased `NSString` (interior NULs dropped: `CString` refuses them).
unsafe fn nsstring(s: &str) -> *mut AnyObject {
    let Ok(c) = CString::new(s.replace('\0', "")) else {
        return core::ptr::null_mut();
    };
    let Some(cls) = class("NSString") else {
        return core::ptr::null_mut();
    };
    unsafe { msg_send![cls, stringWithUTF8String: c.as_ptr()] }
}

/// An `NSString *` as a Rust string; `None` for nil.
unsafe fn rust_string(ns: *mut AnyObject) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let p: *const c_char = unsafe { msg_send![ns, UTF8String] };
    if p.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// `-[NSError localizedDescription]`.
unsafe fn describe(error: *mut AnyObject) -> String {
    if error.is_null() {
        return String::new();
    }
    let text: *mut AnyObject = unsafe { msg_send![error, localizedDescription] };
    unsafe { rust_string(text) }.unwrap_or_default()
}

/// Releases an object this module created with `new` / `alloc`.
unsafe fn release(object: *mut AnyObject) {
    if !object.is_null() {
        let _: () = unsafe { msg_send![object, release] };
    }
}

/// The process' synthesizer, made on first use.
fn synthesizer() -> Option<*mut AnyObject> {
    let existing = SYNTHESIZER.load(Ordering::Acquire);
    if existing != 0 {
        return Some(existing as *mut AnyObject);
    }
    avfoundation()?;
    let cls = class("AVSpeechSynthesizer")?;
    let made: *mut AnyObject = unsafe { msg_send![cls, new] };
    if made.is_null() {
        return None;
    }
    match SYNTHESIZER.compare_exchange(0, made as usize, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Some(made),
        Err(other) => {
            // Another thread made one first: keep that one.
            unsafe { release(made) };
            Some(other as *mut AnyObject)
        }
    }
}

/// Hands `text` to the synthesizer, in the voice of `language` when the system has one.
pub fn say(text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    objc2::rc::autoreleasepool(|_| unsafe {
        let Some(synth) = synthesizer() else {
            return SpeechStatus::Unavailable;
        };
        let Some(utterance_cls) = class("AVSpeechUtterance") else {
            return SpeechStatus::Unavailable;
        };
        let utterance: *mut AnyObject = msg_send![utterance_cls, speechUtteranceWithString: nsstring(text)];
        if utterance.is_null() {
            return SpeechStatus::Unavailable;
        }
        if !language.is_empty() {
            if let Some(voice_cls) = class("AVSpeechSynthesisVoice") {
                let voice: *mut AnyObject = msg_send![voice_cls, voiceWithLanguage: nsstring(language)];
                if !voice.is_null() {
                    let _: () = msg_send![utterance, setVoice: voice];
                }
            }
        }
        if interrupt {
            let _: bool = msg_send![synth, stopSpeakingAtBoundary: BOUNDARY_IMMEDIATE];
        }
        let _: () = msg_send![synth, speakUtterance: utterance];
        SpeechStatus::Queued
    })
}

/// Stops the synthesizer (when there is one) and drops what waits.
pub fn stop() {
    let existing = SYNTHESIZER.load(Ordering::Acquire);
    if existing != 0 {
        let synth = existing as *mut AnyObject;
        let _: bool = unsafe { msg_send![synth, stopSpeakingAtBoundary: BOUNDARY_IMMEDIATE] };
    }
}

/// Whether the synthesizer is talking.
pub fn is_speaking() -> bool {
    let existing = SYNTHESIZER.load(Ordering::Acquire);
    if existing == 0 {
        return false;
    }
    let synth = existing as *mut AnyObject;
    unsafe { msg_send![synth, isSpeaking] }
}

/// Whether AVFoundation's synthesizer is there.
pub fn is_available() -> bool {
    avfoundation().is_some() && class("AVSpeechSynthesizer").is_some()
}

/// Whether the app's Info.plist has `key`.
fn has_usage_key(key: &str) -> bool {
    objc2::rc::autoreleasepool(|_| unsafe {
        let Some(bundle_cls) = class("NSBundle") else {
            return false;
        };
        let bundle: *mut AnyObject = msg_send![bundle_cls, mainBundle];
        if bundle.is_null() {
            return false;
        }
        let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: nsstring(key)];
        !value.is_null()
    })
}

/// The answer to "may this app recognize speech": asked once with the system's prompt, then
/// read.
unsafe fn authorization(recognizer_cls: &AnyClass) -> isize {
    let status: isize = unsafe { msg_send![recognizer_cls, authorizationStatus] };
    if status != AUTH_NOT_DETERMINED {
        return status;
    }
    let answer: Arc<(Mutex<Option<isize>>, Condvar)> = Arc::new((Mutex::new(None), Condvar::new()));
    let reply = Arc::clone(&answer);
    let handler = RcBlock::new(move |status: isize| {
        *reply.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(status);
        reply.1.notify_all();
    });
    let _: () = unsafe { msg_send![recognizer_cls, requestAuthorization: &*handler] };
    let (lock, ready) = &*answer;
    let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    // The user may take a while with the prompt.
    let (guard, _) = ready
        .wait_timeout_while(guard, Duration::from_secs(120), |status| status.is_none())
        .unwrap_or_else(PoisonError::into_inner);
    let status: Option<isize> = *guard;
    status.unwrap_or(AUTH_NOT_DETERMINED)
}

/// What the recognition task reported so far.
#[derive(Debug, Default, Clone)]
struct Hearing {
    /// The best transcription so far.
    text: String,
    /// When the transcription last changed: a long pause ends the listening.
    changed_at: Option<Instant>,
    /// The task gave its final answer (or failed).
    done: bool,
    error: Option<String>,
}

/// `AVMediaTypeAudio`: an `NSString` holding the four-character code of the media type.
const MEDIA_TYPE_AUDIO: &str = "soun";

/// What the system says about speech recognition and the microphone, WITHOUT asking, and which
/// usage texts this program's Info.plist has. `None` when the classes are missing.
pub(super) fn read_gates() -> Option<Gates> {
    // The classes exist once their frameworks are loaded.
    avfoundation()?;
    speech_framework()?;
    objc2::rc::autoreleasepool(|_| unsafe {
        let recognizer_cls = class("SFSpeechRecognizer")?;
        let capture_cls = class("AVCaptureDevice")?;
        let speech: isize = msg_send![recognizer_cls, authorizationStatus];
        let microphone: isize =
            msg_send![capture_cls, authorizationStatusForMediaType: nsstring(MEDIA_TYPE_AUDIO)];
        Some(Gates {
            speech: Access::from_speech(speech),
            microphone: Access::from_capture(microphone),
            speech_key: has_usage_key(authorization::SPEECH_KEY),
            microphone_key: has_usage_key(authorization::MICROPHONE_KEY),
            starter: None,
        })
    })
}

#[cfg(target_os = "macos")]
extern "C" {
    fn getppid() -> i32;
    fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
    fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut c_void, buffersize: i32) -> i32;
}

/// `PROC_PIDT_SHORTBSDINFO`: the flavor of `proc_pidinfo` that returns a `proc_bsdshortinfo`. The
/// full `proc_bsdinfo` is refused for a process of another user (`login`, which a terminal starts
/// the shell through, belongs to root); the short one is not.
#[cfg(target_os = "macos")]
const PROC_PIDT_SHORTBSDINFO: i32 = 13;
/// `sizeof(struct proc_bsdshortinfo)`; its `pbsi_ppid` is the second `u32`.
#[cfg(target_os = "macos")]
const BSDINFO_SIZE: usize = 64;
#[cfg(target_os = "macos")]
const BSDINFO_PPID_AT: usize = 4;
/// `PROC_PIDPATHINFO_MAXSIZE`.
#[cfg(target_os = "macos")]
const PATH_MAX_SIZE: usize = 4096;

/// The app this program was started from: the nearest ancestor process whose executable lives in
/// an app bundle (Terminal, iTerm, Xcode, an editor). The system holds that app responsible for
/// a bare binary's permission requests. Best effort, for a message only: `None` when the chain
/// cannot be read.
#[cfg(target_os = "macos")]
pub(super) fn starter_app() -> Option<String> {
    let mut pid = unsafe { getppid() };
    // A process tree is shallow; the bound only guards against a loop.
    for _ in 0..32 {
        if pid <= 1 {
            return None;
        }
        let mut path = vec![0u8; PATH_MAX_SIZE];
        let length = unsafe { proc_pidpath(pid, path.as_mut_ptr().cast(), PATH_MAX_SIZE as u32) };
        if let Ok(length) = usize::try_from(length) {
            path.truncate(length);
            if let Some(name) = authorization::app_name_from_path(&String::from_utf8_lossy(&path)) {
                return Some(name);
            }
        }
        let mut info = [0u8; BSDINFO_SIZE];
        let got = unsafe {
            proc_pidinfo(pid, PROC_PIDT_SHORTBSDINFO, 0, info.as_mut_ptr().cast(), BSDINFO_SIZE as i32)
        };
        if usize::try_from(got).ok() != Some(BSDINFO_SIZE) {
            return None;
        }
        let parent = i32::from_ne_bytes(info[BSDINFO_PPID_AT..BSDINFO_PPID_AT + 4].try_into().ok()?);
        if parent == pid {
            return None;
        }
        pid = parent;
    }
    None
}

/// iOS: an app is always a bundle, and the process list is not the app's to read.
#[cfg(not(target_os = "macos"))]
pub(super) fn starter_app() -> Option<String> {
    None
}

/// Asks the system for the microphone (with its prompt, when it shows one) and waits for the
/// answer: whether it was allowed, `None` when nobody answered in time.
unsafe fn request_microphone() -> Option<bool> {
    let capture_cls = class("AVCaptureDevice")?;
    let answer: Arc<(Mutex<Option<bool>>, Condvar)> = Arc::new((Mutex::new(None), Condvar::new()));
    let reply = Arc::clone(&answer);
    let handler = RcBlock::new(move |granted: Bool| {
        *reply.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(granted.as_bool());
        reply.1.notify_all();
    });
    let _: () = unsafe {
        msg_send![capture_cls, requestAccessForMediaType: nsstring(MEDIA_TYPE_AUDIO), completionHandler: &*handler]
    };
    let (lock, ready) = &*answer;
    let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    // The user may take a while with the prompt.
    let (guard, _) = ready
        .wait_timeout_while(guard, Duration::from_secs(120), |granted| granted.is_none())
        .unwrap_or_else(PoisonError::into_inner);
    let granted: Option<bool> = *guard;
    granted
}

/// A refusal as the engine's answer.
fn refused(why: Refusal, message: &str) -> SpeechHeard {
    SpeechHeard::none(
        match why {
            Refusal::Denied => HeardStatus::Denied,
            Refusal::Unavailable => HeardStatus::Unavailable,
        },
        message,
    )
}

/// Listens for up to `timeout_ms` in `language` (a BCP 47 tag; empty for the system's).
pub fn listen(language: &str, timeout_ms: u32) -> SpeechHeard {
    if avfoundation().is_none() || speech_framework().is_none() {
        return SpeechHeard::none(HeardStatus::Unavailable, "the Speech framework is not on this system");
    }
    let Some(mut gates) = read_gates() else {
        return SpeechHeard::none(HeardStatus::Unavailable, "the speech recognizer's classes are missing");
    };
    // The name of the app that is held responsible is only looked for when a message will say it.
    if gates.speech != Access::Authorized || gates.microphone != Access::Authorized {
        gates.starter = starter_app();
    }
    match authorization::decide(&gates) {
        Decision::Listen => {}
        Decision::Refuse { why, message } => return refused(why, &message),
        Decision::Ask { speech, microphone } => {
            if speech {
                let Some(recognizer_cls) = class("SFSpeechRecognizer") else {
                    return SpeechHeard::none(HeardStatus::Unavailable, "the speech recognizer's classes are missing");
                };
                match unsafe { authorization(recognizer_cls) } {
                    AUTH_AUTHORIZED => {}
                    AUTH_NOT_DETERMINED => {
                        return SpeechHeard::none(HeardStatus::Denied, &authorization::declined(Gate::Speech, false, &gates));
                    }
                    _ => {
                        return SpeechHeard::none(HeardStatus::Denied, &authorization::declined(Gate::Speech, true, &gates));
                    }
                }
            }
            if microphone {
                match unsafe { request_microphone() } {
                    Some(true) => {}
                    Some(false) => {
                        return SpeechHeard::none(HeardStatus::Denied, &authorization::declined(Gate::Microphone, true, &gates));
                    }
                    None => {
                        return SpeechHeard::none(HeardStatus::Denied, &authorization::declined(Gate::Microphone, false, &gates));
                    }
                }
            }
        }
    }
    objc2::rc::autoreleasepool(|_| unsafe {
        listen_with_recognizer(language, Duration::from_millis(u64::from(timeout_ms)))
    })
}

unsafe fn listen_with_recognizer(language: &str, timeout: Duration) -> SpeechHeard {
    let (Some(recognizer_cls), Some(request_cls), Some(engine_cls), Some(queue_cls)) = (
        class("SFSpeechRecognizer"),
        class("SFSpeechAudioBufferRecognitionRequest"),
        class("AVAudioEngine"),
        class("NSOperationQueue"),
    ) else {
        return SpeechHeard::none(HeardStatus::Unavailable, "the speech recognizer's classes are missing");
    };
    // `listen` decided and asked already: this only keeps a permission that was taken away in
    // the meantime from starting a recognizer that would fail.
    let status: isize = unsafe { msg_send![recognizer_cls, authorizationStatus] };
    if status != AUTH_AUTHORIZED {
        return SpeechHeard::none(HeardStatus::Denied, "speech recognition is not allowed any more");
    }

    // The recognizer of the language; its answers on a queue of their own, so a caller on the
    // main thread does not wait for itself.
    let recognizer: *mut AnyObject = if language.is_empty() {
        unsafe { msg_send![recognizer_cls, new] }
    } else {
        let locale: *mut AnyObject = match class("NSLocale") {
            Some(locale_cls) => unsafe { msg_send![locale_cls, localeWithLocaleIdentifier: nsstring(language)] },
            None => core::ptr::null_mut(),
        };
        let alloc: *mut AnyObject = unsafe { msg_send![recognizer_cls, alloc] };
        unsafe { msg_send![alloc, initWithLocale: locale] }
    };
    if recognizer.is_null() {
        return SpeechHeard::none(HeardStatus::Unavailable, &format!("no speech recognizer for {language}"));
    }
    let available: bool = unsafe { msg_send![recognizer, isAvailable] };
    if !available {
        unsafe { release(recognizer) };
        return SpeechHeard::none(HeardStatus::Unavailable, "the speech recognizer is not available now");
    }
    let queue: *mut AnyObject = unsafe { msg_send![queue_cls, new] };
    let _: () = unsafe { msg_send![recognizer, setQueue: queue] };
    let request: *mut AnyObject = unsafe { msg_send![request_cls, new] };
    let _: () = unsafe { msg_send![request, setShouldReportPartialResults: true] };

    // The microphone: every buffer of the input goes to the request.
    let engine: *mut AnyObject = unsafe { msg_send![engine_cls, new] };
    let input: *mut AnyObject = unsafe { msg_send![engine, inputNode] };
    let format: *mut AnyObject = unsafe { msg_send![input, outputFormatForBus: 0usize] };
    // A Mac without a microphone reports a format of zero Hz: a tap on it raises an exception.
    let rate: f64 = if format.is_null() { 0.0 } else { unsafe { msg_send![format, sampleRate] } };
    if !(rate > 0.0) {
        unsafe {
            release(engine);
            release(request);
            release(queue);
            release(recognizer);
        }
        return SpeechHeard::none(HeardStatus::Unavailable, "there is no microphone to listen with");
    }
    let request_ptr = request as usize;
    let tap = RcBlock::new(move |buffer: *mut AnyObject, _when: *mut AnyObject| {
        if !buffer.is_null() {
            let request = request_ptr as *mut AnyObject;
            let _: () = unsafe { msg_send![request, appendAudioPCMBuffer: buffer] };
        }
    });
    let _: () = unsafe {
        msg_send![input, installTapOnBus: 0usize, bufferSize: 1024u32, format: format, block: &*tap]
    };
    let _: () = unsafe { msg_send![engine, prepare] };
    let mut error: *mut AnyObject = core::ptr::null_mut();
    let started: bool = unsafe { msg_send![engine, startAndReturnError: &mut error as *mut *mut AnyObject] };
    let finish = |task: *mut AnyObject, audio_ended: bool| unsafe {
        let _: () = msg_send![engine, stop];
        let _: () = msg_send![input, removeTapOnBus: 0usize];
        if !audio_ended {
            let _: () = msg_send![request, endAudio];
        }
        if !task.is_null() {
            let _: () = msg_send![task, cancel];
        }
        release(engine);
        release(request);
        release(queue);
        release(recognizer);
    };
    if !started {
        let why = unsafe { describe(error) };
        finish(core::ptr::null_mut(), false);
        return SpeechHeard::none(HeardStatus::Failed, &format!("the microphone did not start: {why}"));
    }

    // The task reports partial transcriptions, then a final one (or an error).
    let shared: Arc<(Mutex<Hearing>, Condvar)> = Arc::new((Mutex::new(Hearing::default()), Condvar::new()));
    let reply = Arc::clone(&shared);
    let handler = RcBlock::new(move |result: *mut AnyObject, error: *mut AnyObject| {
        let (text, is_final) = if result.is_null() {
            (None, false)
        } else {
            let transcription: *mut AnyObject = unsafe { msg_send![result, bestTranscription] };
            let formatted: *mut AnyObject = if transcription.is_null() {
                core::ptr::null_mut()
            } else {
                unsafe { msg_send![transcription, formattedString] }
            };
            let is_final: bool = unsafe { msg_send![result, isFinal] };
            (unsafe { rust_string(formatted) }, is_final)
        };
        let failure = (!error.is_null()).then(|| unsafe { describe(error) });
        let mut hearing = reply.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(text) = text.filter(|t| *t != hearing.text) {
            hearing.text = text;
            hearing.changed_at = Some(Instant::now());
        }
        if is_final || failure.is_some() {
            hearing.done = true;
        }
        if failure.is_some() {
            hearing.error = failure;
        }
        reply.1.notify_all();
    });
    let task: *mut AnyObject =
        unsafe { msg_send![recognizer, recognitionTaskWithRequest: request, resultHandler: &*handler] };

    // Wait for the final answer: the time is up, or the user paused, ends the audio; then the
    // recognizer has a moment for its final transcription.
    let deadline = Instant::now() + timeout;
    let mut audio_ended_at: Option<Instant> = None;
    let heard = loop {
        let now = {
            let (lock, changed) = &*shared;
            let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
            let (guard, _) = changed
                .wait_timeout(guard, Duration::from_millis(100))
                .unwrap_or_else(PoisonError::into_inner);
            guard.clone()
        };
        if now.done {
            break now;
        }
        let paused = !now.text.is_empty() && now.changed_at.is_some_and(|at| at.elapsed() >= SILENCE);
        match audio_ended_at {
            None if paused || Instant::now() >= deadline => {
                let _: () = unsafe { msg_send![request, endAudio] };
                audio_ended_at = Some(Instant::now());
            }
            Some(at) if at.elapsed() >= FINAL_WAIT => break now,
            _ => {}
        }
    };
    finish(if heard.done { core::ptr::null_mut() } else { task }, audio_ended_at.is_some());
    // The blocks stay alive until here: the framework holds copies of its own.
    drop(tap);
    drop(handler);

    if !heard.text.trim().is_empty() {
        return SpeechHeard::heard(heard.text.trim(), if heard.done { 1.0 } else { 0.5 });
    }
    match heard.error {
        // "No speech detected" is the recognizer's way of saying nothing was said.
        Some(why) if !why.to_ascii_lowercase().contains("no speech") => {
            SpeechHeard::none(HeardStatus::Failed, &why)
        }
        _ => SpeechHeard::none(HeardStatus::NothingHeard, "nothing was said"),
    }
}
