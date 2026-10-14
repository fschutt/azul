//! macOS and iOS: `AVSpeechSynthesizer` says the text; `SFSpeechRecognizer` hears what the
//! microphone picks up, fed by a tap on `AVAudioEngine`'s input. AVFoundation and Speech are
//! dlopen'd, nothing links them: a system without one answers `Unavailable`.
//!
//! Hearing needs `NSSpeechRecognitionUsageDescription` and `NSMicrophoneUsageDescription` in the
//! app's Info.plist. Without them the OS stops the process at the first request - an unbundled
//! binary such as `target/release/<app>` has neither - so they are checked before anything is
//! asked, and their absence is an `Unavailable` that says so.

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
    runtime::{AnyClass, AnyObject},
};

use super::{HeardStatus, SpeechHeard, SpeechStatus};

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

/// Listens for up to `timeout_ms` in `language` (a BCP 47 tag; empty for the system's).
pub fn listen(language: &str, timeout_ms: u32) -> SpeechHeard {
    for key in ["NSSpeechRecognitionUsageDescription", "NSMicrophoneUsageDescription"] {
        if !has_usage_key(key) {
            return SpeechHeard::none(
                HeardStatus::Unavailable,
                &format!(
                    "the app's Info.plist has no {key}: the system would stop the app at the \
                     first request. Run it from an app bundle that declares it."
                ),
            );
        }
    }
    if avfoundation().is_none() || speech_framework().is_none() {
        return SpeechHeard::none(HeardStatus::Unavailable, "the Speech framework is not on this system");
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
    match unsafe { authorization(recognizer_cls) } {
        AUTH_AUTHORIZED => {}
        AUTH_NOT_DETERMINED => {
            return SpeechHeard::none(HeardStatus::Denied, "speech recognition was not allowed (no answer to the prompt)");
        }
        _ => return SpeechHeard::none(HeardStatus::Denied, "speech recognition is not allowed for this app"),
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
