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
    builtin::{Microphone, MicrophoneSource},
    keeper::{Lease, MicKeeper},
    recognizer::{Answer, EventKind, Outcome, Readiness, StreamOptions, Word, STREAM_ENDPOINT_MS},
    vocabulary::Vocabulary,
    SpeechStatus,
};

/// The synthesizer, retained for the life of the process; `0` until the first utterance. A
/// pointer is kept as a number: raw pointers are not `Sync`.
static SYNTHESIZER: AtomicUsize = AtomicUsize::new(0);

/// `AVSpeechBoundaryImmediate`: stop mid-word.
const BOUNDARY_IMMEDIATE: isize = 0;

/// `AVAudioPCMFormatFloat32` (`AVAudioCommonFormat`).
const AV_AUDIO_PCM_FORMAT_FLOAT32: usize = 1;

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

/// The words of a transcription (`SFTranscription`): each segment's text, when it began and how
/// long it lasted in seconds from the start of the audio, and how sure the recognizer is.
unsafe fn read_segments(transcription: *mut AnyObject) -> Vec<Segment> {
    let segments: *mut AnyObject = unsafe { msg_send![transcription, segments] };
    if segments.is_null() {
        return Vec::new();
    }
    let count: usize = unsafe { msg_send![segments, count] };
    let mut out = Vec::with_capacity(count.min(4_096));
    for index in 0..count.min(4_096) {
        let segment: *mut AnyObject = unsafe { msg_send![segments, objectAtIndex: index] };
        if segment.is_null() {
            continue;
        }
        let text: *mut AnyObject = unsafe { msg_send![segment, substring] };
        let timestamp: f64 = unsafe { msg_send![segment, timestamp] };
        let duration: f64 = unsafe { msg_send![segment, duration] };
        let confidence: f32 = unsafe { msg_send![segment, confidence] };
        if let Some(text) = unsafe { rust_string(text) } {
            out.push((text, timestamp, duration, confidence));
        }
    }
    out
}

/// The words of the segments: a segment that holds several words (the recognizer joins "is a")
/// is cut into them, each with an equal share of its time.
fn words_of_segments(segments: &[Segment]) -> Vec<Word> {
    let ms = |seconds: f64| -> u32 {
        if seconds.is_finite() && seconds > 0.0 {
            (seconds * 1000.0).round().min(f64::from(u32::MAX)) as u32
        } else {
            0
        }
    };
    let mut words = Vec::new();
    for (text, start, duration, confidence) in segments {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let each = duration / parts.len().max(1) as f64;
        for (index, part) in parts.iter().enumerate() {
            let from = start + each * index as f64;
            words.push(Word::new(part, ms(from), ms(from + each).max(ms(from)), *confidence));
        }
    }
    words
}

/// The words of a transcription as the recognizer gives them: text, seconds from the start of the
/// audio, seconds long, how sure.
type Segment = (String, f64, f64, f32);

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
    /// The words of the best transcription.
    segments: Vec<Segment>,
}

impl Hearing {
    /// What one report of the task changes: a changed text (with its words, and the moment it
    /// changed), the words of the FINAL report even when its text is the one already known (the
    /// times and the confidences come only with it: before, they are zero), the end of the task,
    /// a failure.
    fn update(&mut self, text: Option<String>, is_final: bool, segments: Vec<Segment>, failure: Option<String>, now: Instant) {
        let changed = text.as_ref().is_some_and(|text| *text != self.text);
        if let Some(text) = text.filter(|_| changed) {
            self.text = text;
            self.changed_at = Some(now);
            self.segments = segments;
        } else if is_final && !segments.is_empty() {
            self.segments = segments;
        }
        if is_final || failure.is_some() {
            self.done = true;
        }
        if failure.is_some() {
            self.error = failure;
        }
    }
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
fn refused(why: Refusal, message: &str) -> Answer {
    Answer::none(
        match why {
            Refusal::Denied => Outcome::Denied,
            Refusal::Unavailable => Outcome::Unavailable,
        },
        message,
    )
}

/// Listens for up to `timeout_ms` in `language` (a BCP 47 tag; empty for the system's), favouring
/// the words of `vocabulary` (`contextualStrings`: measured with TTS voices, a compound such as
/// `CssError` is then written as spelt, which the recognizer does not do unasked): one utterance,
/// the first final of a stream of one. A cancel answers what was said until then.
pub fn listen(
    language: &str,
    timeout_ms: u32,
    cancel: &dyn Fn() -> bool,
    vocabulary: &Vocabulary,
    flowing: &dyn Fn(),
    keeper: &MicKeeper,
) -> Answer {
    let first: std::cell::RefCell<Option<Answer>> = std::cell::RefCell::new(None);
    let never = || false;
    let ended = stream(
        language,
        timeout_ms,
        &never,
        &|| cancel(),
        &StreamOptions {
            partials: false,
            // A call keeps the end-point it was verified with (a stream's default is shorter).
            endpoint_ms: u32::try_from(SILENCE.as_millis()).unwrap_or(1_500),
            max_ms: 0,
            max_utterances: 1,
        },
        &|kind, answer| {
            if kind == EventKind::Final {
                *first.borrow_mut() = Some(answer);
            }
        },
        vocabulary,
        flowing,
        keeper,
        false,
    );
    match first.into_inner() {
        Some(answer) => answer,
        None if matches!(ended.outcome, Outcome::Heard | Outcome::NothingHeard) => {
            Answer::none(Outcome::NothingHeard, "nothing was said")
        }
        None => ended,
    }
}

/// Listens as a stream for up to `timeout_ms`: a recognition request per utterance, every one fed
/// from the same microphone with no gap between them (what is heard while a request waits for its
/// final goes to the next one first), a partial whenever the recognizer reports a new text, a final
/// when it does. `stop` ends the request that goes on (its final is what was said so far) and the
/// stream, `cancel` drops it. `keep_on_stop` is whether the microphone may be kept after a stop.
#[allow(clippy::too_many_arguments)]
pub fn stream(
    language: &str,
    timeout_ms: u32,
    cancel: &dyn Fn() -> bool,
    stop: &dyn Fn() -> bool,
    options: &StreamOptions,
    emit: &dyn Fn(EventKind, Answer),
    vocabulary: &Vocabulary,
    flowing: &dyn Fn(),
    keeper: &MicKeeper,
    keep_on_stop: bool,
) -> Answer {
    if avfoundation().is_none() || speech_framework().is_none() {
        return Answer::none(Outcome::Unavailable, "the Speech framework is not on this system");
    }
    let Some(mut gates) = read_gates() else {
        return Answer::none(Outcome::Unavailable, "the speech recognizer's classes are missing");
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
                    return Answer::none(Outcome::Unavailable, "the speech recognizer's classes are missing");
                };
                match unsafe { authorization(recognizer_cls) } {
                    AUTH_AUTHORIZED => {}
                    AUTH_NOT_DETERMINED => {
                        return Answer::none(Outcome::Denied, &authorization::declined(Gate::Speech, false, &gates));
                    }
                    _ => {
                        return Answer::none(Outcome::Denied, &authorization::declined(Gate::Speech, true, &gates));
                    }
                }
            }
            if microphone {
                match unsafe { request_microphone() } {
                    Some(true) => {}
                    Some(false) => {
                        return Answer::none(Outcome::Denied, &authorization::declined(Gate::Microphone, true, &gates));
                    }
                    None => {
                        return Answer::none(Outcome::Denied, &authorization::declined(Gate::Microphone, false, &gates));
                    }
                }
            }
        }
    }
    unsafe {
        stream_with_recognizer(
            language,
            Duration::from_millis(u64::from(timeout_ms)),
            cancel,
            stop,
            options,
            emit,
            vocabulary,
            flowing,
            keeper,
            keep_on_stop,
        )
    }
}

/// Whether listening can begin, asked without listening and without asking the user.
pub fn readiness() -> Readiness {
    if avfoundation().is_none() || speech_framework().is_none() {
        return Readiness::Refused {
            outcome: Outcome::Unavailable,
            message: "the Speech framework is not on this system".to_string(),
        };
    }
    let Some(mut gates) = read_gates() else {
        return Readiness::Refused {
            outcome: Outcome::Unavailable,
            message: "the speech recognizer's classes are missing".to_string(),
        };
    };
    if gates.speech != Access::Authorized || gates.microphone != Access::Authorized {
        gates.starter = starter_app();
    }
    match authorization::decide(&gates) {
        // Undecided with the usage texts: the system's prompt comes with the first listen.
        Decision::Listen | Decision::Ask { .. } => Readiness::Ready,
        Decision::Refuse { why, message } => Readiness::Refused {
            outcome: match why {
                Refusal::Denied => Outcome::Denied,
                Refusal::Unavailable => Outcome::Unavailable,
            },
            message,
        },
    }
}

/// Hands the vocabulary to a request as `contextualStrings`: phrases the recognizer should
/// favour, even when they are not in the system's vocabulary.
unsafe fn set_contextual_strings(request: *mut AnyObject, vocabulary: &Vocabulary) {
    if vocabulary.is_empty() {
        return;
    }
    let Some(array_cls) = class("NSMutableArray") else {
        return;
    };
    let array: *mut AnyObject = unsafe { msg_send![array_cls, array] };
    if array.is_null() {
        return;
    }
    for entry in vocabulary.entries() {
        let text = unsafe { nsstring(entry) };
        if !text.is_null() {
            let _: () = unsafe { msg_send![array, addObject: text] };
        }
    }
    let _: () = unsafe { msg_send![request, setContextualStrings: array] };
}

/// Hands `samples` (mono, in `format`) to the request as one buffer.
unsafe fn append_samples(request: *mut AnyObject, format: *mut AnyObject, samples: &[f32]) {
    let Some(buffer_cls) = class("AVAudioPCMBuffer") else {
        return;
    };
    let Ok(frames) = u32::try_from(samples.len()) else {
        return;
    };
    if frames == 0 {
        return;
    }
    let alloc: *mut AnyObject = unsafe { msg_send![buffer_cls, alloc] };
    let buffer: *mut AnyObject = unsafe { msg_send![alloc, initWithPCMFormat: format, frameCapacity: frames] };
    if buffer.is_null() {
        return;
    }
    let _: () = unsafe { msg_send![buffer, setFrameLength: frames] };
    let planes: *const *mut f32 = unsafe { msg_send![buffer, floatChannelData] };
    if !planes.is_null() {
        let plane = unsafe { *planes };
        if !plane.is_null() {
            unsafe { core::ptr::copy_nonoverlapping(samples.as_ptr(), plane, samples.len()) };
            let _: () = unsafe { msg_send![request, appendAudioPCMBuffer: buffer] };
        }
    }
    unsafe { release(buffer) };
}

/// How long one read of the microphone waits for sound while the request is fed, ms.
const FEED_WAIT_MS: u32 = 40;

/// The longest one recognition request is fed: the system takes about a minute of audio per request.
const MAX_REQUEST_MS: u64 = 55_000;

/// How much audio read while a request waits for its final is kept for the next one, ms.
const BACKLOG_MS: u64 = 3_000;

/// What one recognition request came to.
struct RequestHeard {
    hearing: Hearing,
    /// When the audio of the request began (its first sample), ms since the Unix epoch.
    started_unix_ms: u64,
    microphone_failed: Option<String>,
    cancelled: bool,
    /// How long the request took, a request that ends at once and says nothing is not looped on.
    took: Duration,
}

fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// One recognition request, fed from the microphone until the end-point, the stop, the time or
/// the length of a request. The audio read while it waits for its final goes to `pending`.
#[allow(clippy::too_many_arguments)]
unsafe fn run_request(
    recognizer: *mut AnyObject,
    request_cls: &AnyClass,
    language: &str,
    partials: bool,
    vocabulary: &Vocabulary,
    lease: &mut Lease,
    format: *mut AnyObject,
    pending: &mut Vec<f32>,
    deadline: Instant,
    endpoint: Duration,
    cancel: &dyn Fn() -> bool,
    stop: &dyn Fn() -> bool,
    signalled: &mut bool,
    flowing: &dyn Fn(),
    emit: &dyn Fn(EventKind, Answer),
) -> RequestHeard {
    let began = Instant::now();
    let rate = lease.rate().max(1);
    let request: *mut AnyObject = unsafe { msg_send![request_cls, new] };
    let _: () = unsafe { msg_send![request, setShouldReportPartialResults: true] };
    unsafe { set_contextual_strings(request, vocabulary) };

    // The task reports partial transcriptions, then a final one (or an error).
    let shared: Arc<(Mutex<Hearing>, Condvar)> = Arc::new((Mutex::new(Hearing::default()), Condvar::new()));
    let reply = Arc::clone(&shared);
    let handler = RcBlock::new(move |result: *mut AnyObject, error: *mut AnyObject| {
        let (text, is_final, segments) = if result.is_null() {
            (None, false, Vec::new())
        } else {
            let transcription: *mut AnyObject = unsafe { msg_send![result, bestTranscription] };
            let formatted: *mut AnyObject = if transcription.is_null() {
                core::ptr::null_mut()
            } else {
                unsafe { msg_send![transcription, formattedString] }
            };
            let is_final: bool = unsafe { msg_send![result, isFinal] };
            let segments = if transcription.is_null() { Vec::new() } else { unsafe { read_segments(transcription) } };
            (unsafe { rust_string(formatted) }, is_final, segments)
        };
        let failure = (!error.is_null()).then(|| unsafe { describe(error) });
        let mut hearing = reply.0.lock().unwrap_or_else(PoisonError::into_inner);
        hearing.update(text, is_final, segments, failure, Instant::now());
        reply.1.notify_all();
    });
    let task: *mut AnyObject =
        unsafe { msg_send![recognizer, recognitionTaskWithRequest: request, resultHandler: &*handler] };

    // What was read before this request began is the first of its audio.
    let backlog_ms = pending.len() as u64 * 1000 / u64::from(rate);
    let started_unix_ms = unix_ms_now().saturating_sub(backlog_ms);
    let mut audio_samples = pending.len() as u64;
    if !pending.is_empty() {
        unsafe { append_samples(request, format, pending) };
        pending.clear();
    }
    let mut audio_ended_at: Option<Instant> = None;
    let mut microphone_failed: Option<String> = None;
    let mut cancelled = false;
    let mut emitted = String::new();
    let mut raw: Vec<f32> = Vec::new();
    let heard = loop {
        raw.clear();
        if let Err(why) = lease.microphone().read(&mut raw, FEED_WAIT_MS) {
            microphone_failed.get_or_insert(why);
        }
        if !raw.is_empty() {
            if !*signalled {
                // The first sound of this call: a UI may say "speak now".
                *signalled = true;
                flowing();
            }
            if audio_ended_at.is_none() {
                unsafe { append_samples(request, format, &raw) };
                audio_samples += raw.len() as u64;
            } else {
                // The request waits for its final: what is heard meanwhile is the next request's.
                pending.extend_from_slice(&raw);
                let keep = (u64::from(rate) * BACKLOG_MS / 1000) as usize;
                if pending.len() > keep {
                    let surplus = pending.len() - keep;
                    pending.drain(..surplus);
                }
            }
        }
        let now = shared.0.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if partials && !now.done && !now.text.is_empty() && now.text != emitted {
            emitted.clone_from(&now.text);
            let mut partial = Answer::heard_words(words_of_segments(&now.segments), language, started_unix_ms);
            partial.text = now.text.trim().to_string();
            vocabulary.mark(&mut partial.words);
            emit(EventKind::Partial, partial);
        }
        if cancel() {
            cancelled = true;
            break now;
        }
        if now.done {
            break now;
        }
        let paused = !now.text.is_empty() && now.changed_at.is_some_and(|at| at.elapsed() >= endpoint);
        let too_long = audio_samples * 1000 / u64::from(rate) >= MAX_REQUEST_MS;
        match audio_ended_at {
            None if paused || Instant::now() >= deadline || stop() || microphone_failed.is_some() || too_long => {
                let _: () = unsafe { msg_send![request, endAudio] };
                audio_ended_at = Some(Instant::now());
            }
            Some(at) if at.elapsed() >= FINAL_WAIT => break now,
            _ => {}
        }
    };
    if audio_ended_at.is_none() {
        let _: () = unsafe { msg_send![request, endAudio] };
    }
    if !heard.done || cancelled {
        let _: () = unsafe { msg_send![task, cancel] };
    }
    unsafe { release(request) };
    // The block stays alive until here: the framework holds a copy of its own.
    drop(handler);
    RequestHeard {
        hearing: heard,
        started_unix_ms,
        microphone_failed,
        cancelled,
        took: began.elapsed(),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn stream_with_recognizer(
    language: &str,
    timeout: Duration,
    cancel: &dyn Fn() -> bool,
    stop: &dyn Fn() -> bool,
    options: &StreamOptions,
    emit: &dyn Fn(EventKind, Answer),
    vocabulary: &Vocabulary,
    flowing: &dyn Fn(),
    keeper: &MicKeeper,
    keep_on_stop: bool,
) -> Answer {
    let (Some(recognizer_cls), Some(request_cls), Some(queue_cls), Some(format_cls)) = (
        class("SFSpeechRecognizer"),
        class("SFSpeechAudioBufferRecognitionRequest"),
        class("NSOperationQueue"),
        class("AVAudioFormat"),
    ) else {
        return Answer::none(Outcome::Unavailable, "the speech recognizer's classes are missing");
    };
    // `stream` decided and asked already: this only keeps a permission that was taken away in
    // the meantime from starting a recognizer that would fail.
    let status: isize = unsafe { msg_send![recognizer_cls, authorizationStatus] };
    if status != AUTH_AUTHORIZED {
        return Answer::none(Outcome::Denied, "speech recognition is not allowed any more");
    }

    // The recognizer of the language, made once for the stream; its answers on a queue of their own,
    // so a caller on the main thread does not wait for itself.
    let recognizer: *mut AnyObject = objc2::rc::autoreleasepool(|_| unsafe {
        if language.is_empty() {
            msg_send![recognizer_cls, new]
        } else {
            let locale: *mut AnyObject = match class("NSLocale") {
                Some(locale_cls) => msg_send![locale_cls, localeWithLocaleIdentifier: nsstring(language)],
                None => core::ptr::null_mut(),
            };
            let alloc: *mut AnyObject = msg_send![recognizer_cls, alloc];
            msg_send![alloc, initWithLocale: locale]
        }
    });
    if recognizer.is_null() {
        return Answer::none(Outcome::Unavailable, &format!("no speech recognizer for {language}"));
    }
    let available: bool = unsafe { msg_send![recognizer, isAvailable] };
    if !available {
        unsafe { release(recognizer) };
        return Answer::none(Outcome::Unavailable, "the speech recognizer is not available now");
    }
    let queue: *mut AnyObject = unsafe { msg_send![queue_cls, new] };
    let _: () = unsafe { msg_send![recognizer, setQueue: queue] };

    // The microphone is the session's: open already when the last call kept it, else opened now
    // (the permission rules of the microphone apply there).
    let mut lease = match keeper.acquire() {
        Ok(lease) => lease,
        Err((outcome, message)) => {
            unsafe {
                release(queue);
                release(recognizer);
            }
            return Answer::none(outcome, &message);
        }
    };
    let rate = lease.rate();
    // What is handed to the requests: mono floats at the microphone's rate.
    let format: *mut AnyObject = unsafe {
        let alloc: *mut AnyObject = msg_send![format_cls, alloc];
        msg_send![alloc, initWithCommonFormat: AV_AUDIO_PCM_FORMAT_FLOAT32, sampleRate: f64::from(rate), channels: 1u32, interleaved: Bool::NO]
    };
    if format.is_null() {
        unsafe {
            release(queue);
            release(recognizer);
        }
        return Answer::none(Outcome::Failed, "the audio format of the microphone could not be made");
    }
    // What came in while nobody listened is the first of the first request's audio.
    let mut pending = std::mem::take(&mut lease.preroll);
    let mut signalled = false;
    if lease.warm {
        // Open before the call: its sound is flowing already.
        signalled = true;
        flowing();
    }
    let endpoint = Duration::from_millis(u64::from(
        if options.endpoint_ms > 0 { options.endpoint_ms } else { STREAM_ENDPOINT_MS }.clamp(200, 3_000),
    ));
    let deadline = Instant::now() + timeout;
    let mut finals = 0u32;
    let mut quick_misses = 0u32;
    let mut cancelled = false;
    let mut failure: Option<Answer> = None;
    loop {
        let heard = objc2::rc::autoreleasepool(|_| unsafe {
            run_request(
                recognizer,
                request_cls,
                language,
                options.partials,
                vocabulary,
                &mut lease,
                format,
                &mut pending,
                deadline,
                endpoint,
                cancel,
                stop,
                &mut signalled,
                flowing,
                emit,
            )
        });
        if heard.cancelled {
            cancelled = true;
            break;
        }
        let text = heard.hearing.text.trim().to_string();
        if !text.is_empty() {
            quick_misses = 0;
            let mut words = words_of_segments(&heard.hearing.segments);
            vocabulary.mark(&mut words);
            let mut answer = Answer::heard_words(words, language, heard.started_unix_ms);
            answer.text = text;
            // The recognizer's confidence per word is 0 until its final answer: the old rule stands
            // where it gives none.
            if answer.confidence <= 0.0 {
                answer.confidence = if heard.hearing.done { 1.0 } else { 0.5 };
            }
            finals += 1;
            emit(EventKind::Final, answer);
        } else {
            match heard.hearing.error {
                // "No speech detected" is the recognizer's way of saying nothing was said.
                Some(why) if !why.to_ascii_lowercase().contains("no speech") => {
                    failure = Some(Answer::none(Outcome::Failed, &why));
                    break;
                }
                _ => emit(EventKind::Quiet, Answer::none(Outcome::NothingHeard, "nothing was said")),
            }
            // A request that ends at once and says nothing is not asked again and again.
            if heard.took < Duration::from_millis(300) {
                quick_misses += 1;
                if quick_misses >= 5 {
                    failure = Some(Answer::none(Outcome::Failed, "the speech recognizer keeps ending without a result"));
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            } else {
                quick_misses = 0;
            }
        }
        if let Some(why) = heard.microphone_failed {
            failure = Some(Answer::none(Outcome::Failed, &format!("the microphone failed: {why}")));
            break;
        }
        if stop() || Instant::now() >= deadline || (options.max_utterances > 0 && finals >= options.max_utterances) {
            break;
        }
    }
    // The microphone goes back to be kept for the next call, unless this one was cancelled, the
    // microphone failed (the user is done, or the device is gone), or the one utterance of a call
    // was stopped.
    if failure.is_some() {
        drop(lease);
    } else {
        lease.finish(!cancelled && (keep_on_stop || !stop()));
    }
    unsafe {
        release(format);
        release(queue);
        release(recognizer);
    }
    failure.unwrap_or_else(|| Answer::none(if finals > 0 { Outcome::Heard } else { Outcome::NothingHeard }, ""))
}

// ---- The microphone of the built-in recognizer ----

/// The most sound the tap keeps for a reader that stalls: half a minute (the oldest goes first).
const TAP_KEEP_SECONDS: usize = 30;

/// The Mac's microphone for the built-in recognizer: the same `AVAudioEngine` input the system's
/// recognizer listens through, behind the same permission rules (for the microphone alone).
#[derive(Debug, Clone, Copy)]
pub struct AppleMicrophoneSource;

impl MicrophoneSource for AppleMicrophoneSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        let Some(mut gates) = read_gates() else {
            return Err((Outcome::Unavailable, "the audio classes are missing".to_string()));
        };
        if gates.microphone != Access::Authorized {
            gates.starter = starter_app();
        }
        match authorization::decide_microphone(&gates) {
            Decision::Listen => {}
            Decision::Refuse { why, message } => {
                return Err((
                    match why {
                        Refusal::Denied => Outcome::Denied,
                        Refusal::Unavailable => Outcome::Unavailable,
                    },
                    message,
                ));
            }
            Decision::Ask { microphone, .. } => {
                if microphone {
                    match unsafe { request_microphone() } {
                        Some(true) => {}
                        answered => {
                            return Err((
                                Outcome::Denied,
                                authorization::declined(Gate::Microphone, answered.is_some(), &gates),
                            ));
                        }
                    }
                }
            }
        }
        objc2::rc::autoreleasepool(|_| unsafe { AppleMicrophone::start() })
            .map(|microphone| Box::new(microphone) as Box<dyn Microphone>)
    }
}

/// An open `AVAudioEngine` whose input tap fills `shared` with mono samples.
struct AppleMicrophone {
    engine: *mut AnyObject,
    input: *mut AnyObject,
    shared: Arc<(Mutex<Vec<f32>>, Condvar)>,
    rate: u32,
    /// The tap's block, kept alive while the engine may call it.
    _tap: Box<dyn std::any::Any>,
}

// SAFETY: the engine and its input node are used from the one thread that listens; the tap writes
// only into `shared`, which is a mutex.
unsafe impl Send for AppleMicrophone {}

impl AppleMicrophone {
    unsafe fn start() -> Result<Self, (Outcome, String)> {
        let Some(engine_cls) = class("AVAudioEngine") else {
            return Err((Outcome::Unavailable, "the audio engine's class is missing".to_string()));
        };
        let engine: *mut AnyObject = unsafe { msg_send![engine_cls, new] };
        if engine.is_null() {
            return Err((Outcome::Unavailable, "the audio engine could not be made".to_string()));
        }
        let input: *mut AnyObject = unsafe { msg_send![engine, inputNode] };
        let format: *mut AnyObject = unsafe { msg_send![input, outputFormatForBus: 0usize] };
        // A Mac without a microphone reports a format of zero Hz: a tap on it raises an exception.
        let rate: f64 = if format.is_null() { 0.0 } else { unsafe { msg_send![format, sampleRate] } };
        if !(rate > 0.0) {
            unsafe { release(engine) };
            return Err((Outcome::Unavailable, "there is no microphone to listen with".to_string()));
        }
        let channels: u32 = unsafe { msg_send![format, channelCount] };
        let channels = channels.max(1) as usize;
        let keep = rate as usize * TAP_KEEP_SECONDS;
        let shared: Arc<(Mutex<Vec<f32>>, Condvar)> = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let tap_shared = Arc::clone(&shared);
        let tap = RcBlock::new(move |buffer: *mut AnyObject, _when: *mut AnyObject| {
            if buffer.is_null() {
                return;
            }
            let frames: u32 = unsafe { msg_send![buffer, frameLength] };
            let planes: *const *mut f32 = unsafe { msg_send![buffer, floatChannelData] };
            if planes.is_null() || frames == 0 {
                return;
            }
            let frames = frames as usize;
            let mut mono = vec![0.0f32; frames];
            for channel in 0..channels {
                let plane = unsafe { *planes.add(channel) };
                if plane.is_null() {
                    continue;
                }
                let plane = unsafe { core::slice::from_raw_parts(plane, frames) };
                for (sum, sample) in mono.iter_mut().zip(plane) {
                    *sum += sample / channels as f32;
                }
            }
            let (lock, ready) = &*tap_shared;
            let mut samples = lock.lock().unwrap_or_else(PoisonError::into_inner);
            samples.extend_from_slice(&mono);
            if samples.len() > keep {
                let drop = samples.len() - keep;
                samples.drain(..drop);
            }
            ready.notify_all();
        });
        let _: () = unsafe {
            msg_send![input, installTapOnBus: 0usize, bufferSize: 1024u32, format: format, block: &*tap]
        };
        let _: () = unsafe { msg_send![engine, prepare] };
        let mut error: *mut AnyObject = core::ptr::null_mut();
        let started: bool = unsafe { msg_send![engine, startAndReturnError: &mut error as *mut *mut AnyObject] };
        if !started {
            let why = unsafe { describe(error) };
            unsafe {
                let _: () = msg_send![input, removeTapOnBus: 0usize];
                release(engine);
            }
            return Err((Outcome::Failed, format!("the microphone did not start: {why}")));
        }
        Ok(Self {
            engine,
            input,
            shared,
            rate: rate as u32,
            _tap: Box::new(tap),
        })
    }
}

impl Microphone for AppleMicrophone {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn read(&mut self, out: &mut Vec<f32>, wait_ms: u32) -> Result<(), String> {
        let (lock, ready) = &*self.shared;
        let mut samples = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if samples.is_empty() {
            samples = ready
                .wait_timeout_while(samples, Duration::from_millis(u64::from(wait_ms)), |samples| samples.is_empty())
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        out.append(&mut samples);
        Ok(())
    }
}

impl Drop for AppleMicrophone {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.engine, stop];
            let _: () = msg_send![self.input, removeTapOnBus: 0usize];
            release(self.engine);
        }
    }
}

#[cfg(test)]
#[path = "apple_test.rs"]
mod apple_test;
