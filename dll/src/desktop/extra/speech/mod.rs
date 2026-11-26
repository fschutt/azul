//! Speech on the platform's own engines: saying text aloud ([`Speech::speak`]) and hearing what
//! the user says ([`Speech::listen_blocking`], [`SpeechSession::listen_blocking`]).
//!
//! | Platform | Saying | Hearing |
//! |----------|--------|---------|
//! | macOS / iOS | `AVSpeechSynthesizer` (AVFoundation, dlopen'd) | `SFSpeechRecognizer` on the microphone through `AVAudioEngine` (Speech, dlopen'd); the permissions are read first (`authorization`): a bare binary whose terminal holds them listens, a refused one answers `Denied` with where to change it, and one nobody decided yet is asked only when the app's Info.plist carries `NSSpeechRecognitionUsageDescription` and `NSMicrophoneUsageDescription` (else `Unavailable` that says so, instead of the process being stopped by the OS) |
//! | Windows | SAPI 5 `ISpVoice` | WinRT `Windows.Media.SpeechRecognition.SpeechRecognizer` |
//! | Linux | speech-dispatcher (`libspeechd.so.2`, dlopen'd) | not yet: `Unavailable` |
//! | Android | `TextToSpeech` through the Java helper `AzulSpeech` | `SpeechRecognizer` through the same helper, on the main thread; the app declares `RECORD_AUDIO` (`AZ_ANDROID_PERMISSIONS`) and asks for it through the permission manager, else hearing answers `Denied` |
//!
//! Saying never blocks: the text is handed to the engine, which speaks it after what it is
//! saying ([`Speech::speak`]) or instead of it ([`Speech::speak_now`], for a navigation prompt
//! that must not wait). Hearing blocks until the user stopped talking or the time is up, so it
//! is for a worker thread.
//!
//! # The session: a chain of recognizers
//!
//! [`Speech::listen_blocking`] asks the platform's recognizer and nothing else. A
//! [`SpeechSession`] holds an ORDERED CHAIN of recognizers - the platform's first, then the
//! built-in model ([`builtin`]) the app registered - and a call goes to the first one that is
//! not marked refused. A refusal (`Denied`, `Unavailable`, or two `Failed` in a row) marks that
//! recognizer refused for the rest of the session and the next one answers IN THE SAME CALL, once;
//! a quiet room (`NothingHeard`) is an answer and hands nothing over. The answer says which
//! recognizer gave it ([`SpeechHeard::recognizer`]), the words with their times
//! ([`SpeechHeard::words`], milliseconds from [`SpeechHeard::started_unix_ms`]) and, once, what
//! happened to the ones before it. [`SpeechSession::recognizers`] lists the chain and each one's
//! state for a status line; [`SpeechSession::reset`] forgets the refusals.
//!
//! The same chain code runs with scripted recognizers ([`mock`]): the unit tests drive it on a
//! clock they move, and a test run reads the script `AZ_SPEECH_SCRIPT` names ([`script`]).
//!
//! A headless or E2E TEST run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`; a build with the cargo
//! feature `test-hooks` only, `azul_core::run_mode`) neither speaks nor listens: every utterance
//! is printed as `AZ_SPEECH <language> <text>` (and appended to the file `AZ_SPEECH_LOG`
//! names), hearing answers what `AZ_SPEECH_HEARD` holds, or nothing, and a session's chain is the
//! one `AZ_SPEECH_SCRIPT` scripts. A shipped build always speaks and listens, whatever its
//! environment holds.

use core::ffi::c_void;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, PoisonError,
    },
};

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, AzString, StringVec};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
// What the permissions allow (pure); the Apple recognizers are its only users, the tests run everywhere.
#[cfg_attr(not(any(target_os = "ios", target_os = "macos")), allow(dead_code))]
mod authorization;
pub mod builtin;
mod capture;
pub mod chain;
mod headless;
mod json;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod mock;
pub mod recognizer;
pub mod script;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;

#[cfg(target_os = "android")]
use self::android as platform;
#[cfg(any(target_os = "ios", target_os = "macos"))]
use self::apple as platform;
#[cfg(target_os = "linux")]
use self::linux as platform;
#[cfg(target_os = "windows")]
use self::windows as platform;

use self::{
    builtin::{Builtin, BuiltinStatus, Tuning},
    chain::{Chain, Health},
    mock::{words_of, MockSpeech},
    recognizer::{Answer, Kind, Limits, Listen, Outcome, Readiness, Recognizer, SharedClock, SystemClock, Word},
};

/// The platforms without a speech backend yet (the BSDs): nothing to say with.
#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
)))]
mod platform {
    use super::{HeardStatus, SpeechHeard, SpeechStatus};

    pub fn say(_text: &str, _language: &str, _interrupt: bool) -> SpeechStatus {
        SpeechStatus::Unavailable
    }

    pub fn stop() {}

    pub fn is_speaking() -> bool {
        false
    }

    pub fn is_available() -> bool {
        false
    }

    pub fn listen(_language: &str, _timeout_ms: u32) -> SpeechHeard {
        SpeechHeard::none(
            HeardStatus::Unavailable,
            "there is no speech recognition on this platform yet",
        )
    }
}

/// The variables naming the file a headless run appends its utterances to, and holding what it
/// hears. The installed `azul_core::run_mode::TestHooks` hold their values; the names are
/// `extra_settings`'.
pub use crate::desktop::extra::extra_settings::{SPEECH_HEARD_VAR, SPEECH_LOG_VAR, SPEECH_SCRIPT_VAR};

/// What became of a text to say.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechStatus {
    /// The engine has it: it is said (or, in a headless run, logged).
    Queued,
    /// There is no speech engine here (no backend for the platform, or the engine is missing).
    Unavailable,
}

/// What became of listening.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HeardStatus {
    /// Something was understood: the text.
    Heard,
    /// The time was up and nothing was said.
    NothingHeard,
    /// The user refused the microphone or speech recognition.
    Denied,
    /// There is no recognizer here, or the app is not set up for one: see the message.
    Unavailable,
    /// The recognizer failed: see the message.
    Failed,
}

/// Which recognizer gave an answer.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechRecognizer {
    /// The platform's own: `SFSpeechRecognizer`, WinRT, Android's.
    Native,
    /// The small model the app registered, run in this process.
    Builtin,
    /// A script: a test's or a headless run's stand-in.
    Scripted,
    /// None: nobody could listen (the answer says why).
    None,
}

impl From<Kind> for SpeechRecognizer {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Native => Self::Native,
            Kind::Builtin => Self::Builtin,
            Kind::Scripted => Self::Scripted,
        }
    }
}

/// One recognised word and when it was said.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechWord {
    /// The word as the recognizer wrote it (punctuation attached, as the engine gives it).
    pub text: AzString,
    /// Milliseconds from [`SpeechHeard::started_unix_ms`] to the start of the word.
    pub start_ms: u32,
    /// And to its end.
    pub end_ms: u32,
    /// How sure the recognizer is of this word, 0 to 1.
    pub probability: f32,
}

impl_option!(SpeechWord, OptionSpeechWord, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    SpeechWord,
    SpeechWordVec,
    SpeechWordVecDestructor,
    SpeechWordVecDestructorType,
    SpeechWordVecSlice,
    OptionSpeechWord
);
impl_vec_debug!(SpeechWord, SpeechWordVec);
impl_vec_clone!(SpeechWord, SpeechWordVec, SpeechWordVecDestructor);
impl_vec_partialeq!(SpeechWord, SpeechWordVec);

impl From<&Word> for SpeechWord {
    fn from(word: &Word) -> Self {
        Self {
            text: AzString::from(word.text.clone()),
            start_ms: word.start_ms,
            end_ms: word.end_ms,
            probability: word.probability,
        }
    }
}

/// What listening heard.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechHeard {
    /// What was understood (empty unless `Heard`).
    pub text: AzString,
    /// Why nothing was heard, readable enough to show (empty for `Heard`, unless a recognizer
    /// before the one that answered refused: then it says so, once).
    pub message: AzString,
    /// The language that was recognised, a BCP 47 tag or the model's two letters (empty where
    /// the recognizer does not say).
    pub language: AzString,
    /// The words with their times, where the recognizer says them (empty where it does not).
    pub words: SpeechWordVec,
    /// When the audio began, milliseconds since the Unix epoch: with a word's time it gives the
    /// moment the word was said (0 where nothing was recorded).
    pub started_unix_ms: u64,
    /// How sure the recognizer is, 0 to 1 (0 where it does not say).
    pub confidence: f32,
    pub status: HeardStatus,
    /// Which recognizer answered.
    pub recognizer: SpeechRecognizer,
}

impl SpeechHeard {
    /// An answer without text.
    #[must_use]
    pub fn none(status: HeardStatus, message: &str) -> Self {
        Self {
            status,
            text: AzString::from(String::new()),
            confidence: 0.0,
            message: AzString::from(message.to_string()),
            language: AzString::from(String::new()),
            words: SpeechWordVec::from_vec(Vec::new()),
            started_unix_ms: 0,
            recognizer: SpeechRecognizer::None,
        }
    }

    /// An answer with what was understood.
    #[must_use]
    pub fn heard(text: &str, confidence: f32) -> Self {
        Self {
            status: HeardStatus::Heard,
            text: AzString::from(text.to_string()),
            confidence,
            message: AzString::from(String::new()),
            language: AzString::from(String::new()),
            words: SpeechWordVec::from_vec(Vec::new()),
            started_unix_ms: 0,
            recognizer: SpeechRecognizer::None,
        }
    }
}

impl HeardStatus {
    const fn of(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Heard => Self::Heard,
            Outcome::NothingHeard => Self::NothingHeard,
            Outcome::Denied => Self::Denied,
            Outcome::Unavailable => Self::Unavailable,
            Outcome::Failed => Self::Failed,
        }
    }

    const fn outcome(self) -> Outcome {
        match self {
            Self::Heard => Outcome::Heard,
            Self::NothingHeard => Outcome::NothingHeard,
            Self::Denied => Outcome::Denied,
            Self::Unavailable => Outcome::Unavailable,
            Self::Failed => Outcome::Failed,
        }
    }
}

/// An answer of a recognizer as the public type, given by `by` (`None`: nobody).
fn answer_to_heard(answer: Answer, by: Option<Kind>) -> SpeechHeard {
    SpeechHeard {
        text: AzString::from(answer.text),
        message: AzString::from(answer.message),
        language: AzString::from(answer.language),
        words: SpeechWordVec::from_vec(answer.words.iter().map(SpeechWord::from).collect()),
        started_unix_ms: answer.started_unix_ms,
        confidence: answer.confidence,
        status: HeardStatus::of(answer.outcome),
        recognizer: by.map_or(SpeechRecognizer::None, SpeechRecognizer::from),
    }
}

/// An answer of the old public type as a recognizer's.
fn heard_to_answer(heard: &SpeechHeard) -> Answer {
    Answer {
        outcome: heard.status.outcome(),
        text: heard.text.as_str().to_string(),
        message: heard.message.as_str().to_string(),
        confidence: heard.confidence,
        words: Vec::new(),
        language: heard.language.as_str().to_string(),
        started_unix_ms: heard.started_unix_ms,
    }
}

/// The system's speech, as an app uses it: every call answers at once except
/// [`Speech::listen_blocking`].
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder
pub struct Speech {
    pub _reserved: u8,
}

impl Default for Speech {
    fn default() -> Self {
        Self::new()
    }
}

impl Speech {
    /// A zero-initialised namespace handle: the calls are static.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Says `text` in `language` (a BCP 47 tag such as `de-DE`, or just `de`; empty for the
    /// system's voice) after what is being said.
    #[must_use]
    pub fn speak(text: AzString, language: AzString) -> SpeechStatus {
        say_with(backend_of_run(), text.as_str(), language.as_str(), false)
    }

    /// Says `text` in `language` at once, cutting off what is being said and dropping what
    /// waits: for a prompt that is wrong a second later.
    #[must_use]
    pub fn speak_now(text: AzString, language: AzString) -> SpeechStatus {
        say_with(backend_of_run(), text.as_str(), language.as_str(), true)
    }

    /// Stops saying anything and drops what waits.
    pub fn stop() {
        stop_with(backend_of_run());
    }

    /// Whether something is being said.
    #[must_use]
    pub fn is_speaking() -> bool {
        speaking_with(backend_of_run())
    }

    /// Whether this platform can say anything at all.
    #[must_use]
    pub fn is_available() -> bool {
        available_with(backend_of_run())
    }

    /// Listens on the microphone for up to `timeout_ms` and answers what was said, in
    /// `language` (empty for the system's). Blocks: call it from a worker thread. The first
    /// call may show the system's permission prompt.
    #[must_use]
    pub fn listen_blocking(language: AzString, timeout_ms: u32) -> SpeechHeard {
        listen_with(backend_of_run(), language.as_str(), timeout_ms)
    }
}

/// What the app registered for the built-in recognizer did.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinSpeechStatus {
    /// The engine and the model are loaded: the built-in recognizer listens when the system's
    /// does not.
    Ready,
    /// There is no engine library at the path, or in the places looked in (next to the
    /// executable, in a `speech` folder there, in a bundle's `Resources`).
    EngineMissing,
    /// The library is there but is not a Cactus engine this program can use.
    EngineIncompatible,
    /// There is no model file at the path or in the places looked in.
    ModelMissing,
    /// The engine does not take the model: damaged, not a `.cact`, or larger than the limit.
    ModelRejected,
}

impl From<BuiltinStatus> for BuiltinSpeechStatus {
    fn from(status: BuiltinStatus) -> Self {
        match status {
            BuiltinStatus::Ready => Self::Ready,
            BuiltinStatus::EngineMissing => Self::EngineMissing,
            BuiltinStatus::EngineIncompatible => Self::EngineIncompatible,
            BuiltinStatus::ModelMissing => Self::ModelMissing,
            BuiltinStatus::ModelRejected => Self::ModelRejected,
        }
    }
}

/// What a caller may ask of the speech recognizers: every value outside is kept to it and the
/// answer says so (never a crash).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SpeechLimits {
    /// The shortest time one listen call takes, ms (500).
    pub min_listen_ms: u32,
    /// The longest, ms (60000).
    pub max_listen_ms: u32,
    /// The most words of vocabulary handed to the built-in model (200).
    pub max_keywords: u32,
    /// The largest model file taken, megabytes (128).
    pub max_model_mb: u32,
}

impl Default for SpeechLimits {
    fn default() -> Self {
        let chain = Limits::default();
        let tuning = Tuning::default();
        Self {
            min_listen_ms: chain.min_listen_ms,
            max_listen_ms: chain.max_listen_ms,
            max_keywords: u32::try_from(tuning.max_keywords).unwrap_or(u32::MAX),
            max_model_mb: u32::try_from(tuning.max_model_bytes / (1024 * 1024)).unwrap_or(u32::MAX),
        }
    }
}

/// What a session shares between its handles.
struct SessionInner {
    /// The chain; held for the whole of a listen call.
    chain: Mutex<Chain>,
    /// Set by `cancel`, cleared at the start of a call.
    cancel: AtomicBool,
    /// The status lines of the chain, refreshed after every call and change: read without the
    /// chain's lock, so a status line never waits for a listening call.
    lines: Mutex<Vec<String>>,
    /// What the session was asked to keep to.
    limits: Mutex<SpeechLimits>,
    /// The vocabulary handed to the built-in model, kept for when it is registered.
    vocabulary: Mutex<Vec<String>>,
    /// Where the executable lives: the files are looked for there.
    exe_dir: Option<PathBuf>,
    /// A scripted run: nothing real is loaded, a registration is answered from the script.
    scripted: bool,
}

/// A session of speech recognition: the chain of recognizers and what it learned (which one
/// refused, which answered). Made once by the app and shared: a clone is another handle on the
/// same session, so a worker thread can listen while the window asks `cancel` or `recognizers`.
///
/// ```text
/// let session = SpeechSession::new();
/// session.set_builtin_files(engine_path, model_path);   // the fallback, once, at the start
/// let heard = session.listen_blocking(language, 5000);  // on a worker thread
/// // heard.recognizer says who answered, heard.words when each word was said
/// ```
#[repr(C)]
pub struct SpeechSession {
    /// Opaque pointer to the shared session (null on a default handle).
    pub ptr: *mut c_void,
    /// Whether this handle owns one reference of the session (every `clone` does).
    pub run_destructor: bool,
}

// SAFETY: the session behind the pointer is `Sync` (its parts are mutexes and an atomic).
unsafe impl Send for SpeechSession {}
unsafe impl Sync for SpeechSession {}

impl Default for SpeechSession {
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Clone for SpeechSession {
    fn clone(&self) -> Self {
        if self.ptr.is_null() {
            return Self::default();
        }
        // SAFETY: `ptr` came from `Arc::into_raw` of a `SessionInner` and is alive while a handle
        // that owns a reference exists; a clone takes one reference of its own.
        unsafe { Arc::increment_strong_count(self.ptr.cast::<SessionInner>()) };
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Drop for SpeechSession {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            // SAFETY: this handle owns one reference.
            drop(unsafe { Arc::from_raw(self.ptr.cast::<SessionInner>()) });
            self.ptr = core::ptr::null_mut();
            self.run_destructor = false;
        }
    }
}

impl SpeechSession {
    /// A session whose chain is the platform's recognizer, then the built-in one (not set up
    /// until `set_builtin_files` or `set_builtin_bytes`, unless its files are next to the
    /// executable). In a headless or E2E test run it is the chain `AZ_SPEECH_SCRIPT` scripts.
    #[must_use]
    pub fn new() -> Self {
        Self::with_hooks(azul_core::run_mode::test_hooks())
    }

    fn with_hooks(hooks: &azul_core::run_mode::TestHooks) -> Self {
        let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        let clock: SharedClock = Arc::new(SystemClock::new());
        let scripted = backend_for(hooks) == Backend::Headless;
        let chain = if scripted {
            scripted_chain(hooks, clock)
        } else {
            platform_chain(clock, exe_dir.as_deref())
        };
        Self::from_chain(chain, exe_dir, scripted)
    }

    fn from_chain(chain: Chain, exe_dir: Option<PathBuf>, scripted: bool) -> Self {
        let inner = Arc::new(SessionInner {
            chain: Mutex::new(chain),
            cancel: AtomicBool::new(false),
            lines: Mutex::new(Vec::new()),
            limits: Mutex::new(SpeechLimits::default()),
            vocabulary: Mutex::new(Vec::new()),
            exe_dir,
            scripted,
        });
        inner.refresh_lines();
        Self {
            ptr: Arc::into_raw(inner).cast_mut().cast::<c_void>(),
            run_destructor: true,
        }
    }

    /// A session on `chain` (for the tests of the chain through the public face).
    #[cfg(test)]
    pub(crate) fn on_chain(chain: Chain) -> Self {
        Self::from_chain(chain, None, true)
    }

    fn inner(&self) -> Option<&SessionInner> {
        // SAFETY: a non-null `ptr` is alive while this handle is (see `Clone` and `Drop`).
        unsafe { self.ptr.cast::<SessionInner>().as_ref() }
    }

    /// Registers the built-in recognizer from files: `engine_path` is the engine's shared library
    /// (`libcactus.dylib`, `libcactus.so`, `libcactus.dll`), `model_path` the model
    /// (`whistle.cact`); an empty path is looked for next to the executable, in a `speech` folder
    /// there and in a bundle's `Resources`, and the engine may also be linked into the program.
    /// Loads the engine and the model at once (a few tens of milliseconds) and answers what came
    /// of it; `recognizers` says why when it is not `Ready`. Call it at the start, not during a
    /// listen call (it waits for one to end).
    pub fn set_builtin_files(&self, engine_path: AzString, model_path: AzString) -> BuiltinSpeechStatus {
        self.register(engine_path.as_str(), BuiltinSource::File(model_path.as_str().to_string()))
    }

    /// The same with the model's bytes in memory (`include_bytes!`); the engine copies them, the
    /// slice need not outlive the call.
    pub fn set_builtin_bytes(&self, engine_path: AzString, model: azul_core::gl::U8VecRef) -> BuiltinSpeechStatus {
        self.register(engine_path.as_str(), BuiltinSource::Bytes(model.as_slice().to_vec()))
    }

    fn register(&self, engine_path: &str, source: BuiltinSource) -> BuiltinSpeechStatus {
        let Some(inner) = self.inner() else {
            return BuiltinSpeechStatus::EngineMissing;
        };
        if inner.scripted {
            // A scripted run loads nothing real: the script says whether it has a built-in one.
            let has = inner
                .chain
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .states()
                .iter()
                .any(|state| state.kind == Kind::Builtin);
            return if has { BuiltinSpeechStatus::Ready } else { BuiltinSpeechStatus::EngineMissing };
        }
        let limits = *inner.limits.lock().unwrap_or_else(PoisonError::into_inner);
        let tuning = Tuning {
            max_keywords: limits.max_keywords as usize,
            max_model_bytes: u64::from(limits.max_model_mb) * 1024 * 1024,
            ..Tuning::default()
        };
        let microphone = capture::platform_source();
        let built = match source {
            BuiltinSource::File(model_path) => {
                Builtin::from_files(engine_path, &model_path, inner.exe_dir.as_deref(), microphone, tuning.clone())
            }
            BuiltinSource::Bytes(bytes) => {
                Builtin::from_engine_and_bytes(engine_path, &bytes, inner.exe_dir.as_deref(), microphone, tuning.clone())
            }
        };
        let (mut recognizer, status) = match built {
            Ok(builtin) => (builtin, BuiltinStatus::Ready),
            Err(failure) => (Builtin::refused(capture::platform_source(), failure.message), failure.status),
        };
        let vocabulary = inner.vocabulary.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let _ = recognizer.set_keywords(&vocabulary);
        let mut chain = inner.chain.lock().unwrap_or_else(PoisonError::into_inner);
        chain.replace(Box::new(recognizer));
        drop(chain);
        inner.refresh_lines();
        status.into()
    }

    /// The words the built-in model should favour (names, jargon, the app's own vocabulary):
    /// at most `max_keywords` of 64 characters each. Answers what was cut, empty when nothing was.
    /// The system's recognizer does not take it.
    pub fn set_vocabulary(&self, words: StringVec) -> AzString {
        let Some(inner) = self.inner() else {
            return AzString::from(String::new());
        };
        let words: Vec<String> = words.as_slice().iter().map(|w| w.as_str().to_string()).collect();
        let note = inner.chain.lock().unwrap_or_else(PoisonError::into_inner).set_vocabulary(&words);
        *inner.vocabulary.lock().unwrap_or_else(PoisonError::into_inner) = words;
        AzString::from(note.unwrap_or_default())
    }

    /// What the recognizers are asked to keep to; the defaults are [`SpeechLimits::default`].
    pub fn set_limits(&self, limits: SpeechLimits) {
        let Some(inner) = self.inner() else {
            return;
        };
        *inner.limits.lock().unwrap_or_else(PoisonError::into_inner) = limits;
        inner.chain.lock().unwrap_or_else(PoisonError::into_inner).set_limits(Limits {
            min_listen_ms: limits.min_listen_ms,
            max_listen_ms: limits.max_listen_ms.max(limits.min_listen_ms),
            ..Limits::default()
        });
    }

    /// Listens for up to `timeout_ms` in `language` (a BCP 47 tag such as `de-DE`, or just `de`;
    /// empty for the system's) and answers what was said, with which recognizer said it. Blocks:
    /// call it from a worker thread. The first call may show the system's permission prompt.
    #[must_use]
    pub fn listen_blocking(&self, language: AzString, timeout_ms: u32) -> SpeechHeard {
        let Some(inner) = self.inner() else {
            return SpeechHeard::none(HeardStatus::Unavailable, "this speech session is closed");
        };
        inner.cancel.store(false, Ordering::SeqCst);
        let heard = {
            let mut chain = inner.chain.lock().unwrap_or_else(PoisonError::into_inner);
            chain.listen(language.as_str(), timeout_ms, &|| inner.cancel.load(Ordering::SeqCst))
        };
        inner.refresh_lines();
        answer_to_heard(heard.answer, heard.by)
    }

    /// Ends the listen call that is running: it answers what was said until now. Does nothing
    /// when none is running.
    pub fn cancel(&self) {
        if let Some(inner) = self.inner() {
            inner.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// The chain, in the order it is tried, one line each, for a status line:
    /// `native | <name> | ready` or `built-in | <name> | unavailable: <why>`. A recognizer that
    /// refused (in this session or by what it says about itself) shows its outcome, `denied`,
    /// `unavailable` or `failed`, and the reason.
    #[must_use]
    pub fn recognizers(&self) -> StringVec {
        let lines = self
            .inner()
            .map(|inner| inner.lines.lock().unwrap_or_else(PoisonError::into_inner).clone())
            .unwrap_or_default();
        StringVec::from_vec(lines.into_iter().map(AzString::from).collect())
    }

    /// Forgets which recognizers refused: the next call asks every one again (the user changed
    /// a permission, the app registered a model).
    pub fn reset(&self) {
        if let Some(inner) = self.inner() {
            inner.chain.lock().unwrap_or_else(PoisonError::into_inner).reset();
            inner.refresh_lines();
        }
    }
}

enum BuiltinSource {
    File(String),
    Bytes(Vec<u8>),
}

impl SessionInner {
    /// The status lines of the chain as they stand now. Called with the chain free (after a
    /// call or a change).
    fn refresh_lines(&self) {
        let states = match self.chain.try_lock() {
            Ok(mut chain) => chain.states(),
            Err(_) => return,
        };
        let lines = states
            .iter()
            .map(|state| {
                let kind = match state.kind {
                    Kind::Native => "native",
                    Kind::Builtin => "built-in",
                    Kind::Scripted => "scripted",
                };
                match &state.health {
                    Health::Ready => format!("{kind} | {} | ready", state.label),
                    Health::Refused { outcome, message } => {
                        let word = match outcome {
                            Outcome::Denied => "denied",
                            Outcome::Unavailable => "unavailable",
                            Outcome::Failed => "failed",
                            Outcome::Heard | Outcome::NothingHeard => "refused",
                        };
                        format!("{kind} | {} | {word}: {message}", state.label)
                    }
                }
            })
            .collect();
        *self.lines.lock().unwrap_or_else(PoisonError::into_inner) = lines;
    }
}

/// The platform's chain: its own recognizer, then the built-in one (looked for next to the
/// executable until the app registers it).
fn platform_chain(clock: SharedClock, exe_dir: Option<&std::path::Path>) -> Chain {
    Chain::new(clock)
        .with(Box::new(NativeRecognizer))
        .with(Box::new(Builtin::discover(
            exe_dir.map(std::path::Path::to_path_buf),
            capture::platform_source(),
            Tuning::default(),
        )))
}

/// The chain of a headless or E2E test run: what `AZ_SPEECH_SCRIPT` scripts, else what
/// `AZ_SPEECH_HEARD` holds (heard on every call, at once) or nothing.
fn scripted_chain(hooks: &azul_core::run_mode::TestHooks, clock: SharedClock) -> Chain {
    let Some(path) = hooks.speech_script.as_deref().filter(|path| !path.is_empty()) else {
        return Chain::new(clock).with(Box::new(HeadlessRecognizer));
    };
    let read = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|text| script::parse(&text));
    match read {
        Ok(script) => {
            let mut chain = Chain::new(clock);
            for (kind, steps) in [(Kind::Native, script.native), (Kind::Builtin, script.builtin)] {
                if let Some(steps) = steps {
                    chain = chain.with(Box::new(MockSpeech::new(kind, steps).then(script.then)));
                }
            }
            chain
        }
        Err(why) => Chain::new(clock).with(Box::new(MockSpeech::new(
            Kind::Scripted,
            vec![mock::Step::Refuse {
                outcome: Outcome::Unavailable,
                message: format!("{SPEECH_SCRIPT_VAR} {}: {why}", path.to_string_lossy()),
            }],
        ))),
    }
}

/// The platform's own recognizer as a link of the chain.
struct NativeRecognizer;

impl Recognizer for NativeRecognizer {
    fn kind(&self) -> Kind {
        Kind::Native
    }

    fn label(&self) -> String {
        native_label().to_string()
    }

    fn readiness(&mut self) -> Readiness {
        native_readiness()
    }

    fn listen(&mut self, call: &Listen<'_>) -> Answer {
        let began = call.clock.unix_ms();
        let mut answer = native_listen(&language_tag(call.language), call.timeout_ms.max(1), call.cancel);
        if answer.started_unix_ms == 0 && answer.outcome == Outcome::Heard {
            answer.started_unix_ms = began;
        }
        answer
    }
}

/// What a headless run hears (`AZ_SPEECH_HEARD`, or the test's own), at once, with every word 300 ms
/// long: the old stand-in as a link of the chain.
struct HeadlessRecognizer;

impl Recognizer for HeadlessRecognizer {
    fn kind(&self) -> Kind {
        Kind::Scripted
    }

    fn label(&self) -> String {
        format!("scripted ({SPEECH_HEARD_VAR})")
    }

    fn readiness(&mut self) -> Readiness {
        Readiness::Ready
    }

    fn listen(&mut self, call: &Listen<'_>) -> Answer {
        with_timed_words(heard_to_answer(&headless::heard()), call.clock.unix_ms())
    }
}

/// What the old headless stand-in heard, with a word's time: 300 ms for each, one after the other
/// from the start, the audio dated `unix_ms`.
fn with_timed_words(mut answer: Answer, unix_ms: u64) -> Answer {
    if answer.outcome == Outcome::Heard {
        answer.words = words_of(&answer.text, 0, 300, 1.0);
        answer.started_unix_ms = unix_ms;
    }
    answer
}

/// A name for the platform's recognizer in a status line.
const fn native_label() -> &'static str {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        "Speech framework"
    } else if cfg!(target_os = "windows") {
        "Windows speech recognition"
    } else if cfg!(target_os = "android") {
        "Android speech recognizer"
    } else {
        "none on this platform"
    }
}

/// Whether the platform's recognizer can listen, without listening.
fn native_readiness() -> Readiness {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        apple::readiness()
    }
    #[cfg(target_os = "linux")]
    {
        Readiness::Refused {
            outcome: Outcome::Unavailable,
            message: "there is no speech recognition on Linux yet".to_string(),
        }
    }
    #[cfg(any(target_os = "android", target_os = "windows"))]
    {
        Readiness::Ready
    }
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    )))]
    {
        Readiness::Refused {
            outcome: Outcome::Unavailable,
            message: "there is no speech recognition on this platform yet".to_string(),
        }
    }
}

/// Listens on the platform's recognizer.
fn native_listen(language: &str, timeout_ms: u32, cancel: &dyn Fn() -> bool) -> Answer {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        platform::listen(language, timeout_ms, cancel)
    }
    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    {
        // The other platforms' recognizers answer at the end of their own call: a cancel waits for it.
        let _ = cancel;
        heard_to_answer(&platform::listen(language, timeout_ms))
    }
}

/// Where a run's speech goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    /// The platform's engines.
    Platform,
    /// Nothing is said or heard; utterances are logged.
    Headless,
}

/// The speech of this run ([`backend_for`] its hooks).
fn backend_of_run() -> Backend {
    backend_for(azul_core::run_mode::test_hooks())
}

/// The speech under the hooks `hooks`: the headless stand-in in a headless / E2E TEST run (a test
/// on a dev machine must not talk), else the platform's.
const fn backend_for(hooks: &azul_core::run_mode::TestHooks) -> Backend {
    if hooks.no_speech {
        Backend::Headless
    } else {
        Backend::Platform
    }
}

/// A language as the engines want it: `de` is `de-DE`, `en` is `en-US`, `pt_BR` is `pt-BR`;
/// empty stays empty (the system's voice).
#[must_use]
pub fn language_tag(language: &str) -> String {
    let tag = language.trim().replace('_', "-");
    let mut parts = tag.split('-').filter(|p| !p.is_empty());
    let Some(primary) = parts.next().map(str::to_ascii_lowercase) else {
        return String::new();
    };
    if let Some(region) = parts.next() {
        return format!("{primary}-{}", region.to_ascii_uppercase());
    }
    // The region whose voice a bare language most likely means.
    let region = match primary.as_str() {
        "en" => "US",
        "de" => "DE",
        "fr" => "FR",
        "es" => "ES",
        "it" => "IT",
        "nl" => "NL",
        "pt" => "PT",
        "pl" => "PL",
        "cs" => "CZ",
        "da" => "DK",
        "sv" => "SE",
        "nb" | "no" => "NO",
        "fi" => "FI",
        "ja" => "JP",
        "zh" => "CN",
        "ko" => "KR",
        "ru" => "RU",
        "tr" => "TR",
        "el" => "GR",
        "uk" => "UA",
        _ => return primary,
    };
    format!("{primary}-{region}")
}

/// SAPI's id of a language (a Windows LCID), for the ones it ships voices for.
fn sapi_language_id(tag: &str) -> Option<u16> {
    Some(match tag {
        "en-US" => 0x409,
        "en-GB" => 0x809,
        "de-DE" => 0x407,
        "de-AT" => 0xC07,
        "de-CH" => 0x807,
        "fr-FR" => 0x40C,
        "es-ES" => 0xC0A,
        "it-IT" => 0x410,
        "nl-NL" => 0x413,
        "pt-PT" => 0x816,
        "pt-BR" => 0x416,
        "pl-PL" => 0x415,
        "ja-JP" => 0x411,
        "zh-CN" => 0x804,
        _ => return None,
    })
}

/// What SAPI is given for `text` in `language`: the text in a `<lang>` element naming the
/// language's id, escaped, and that it is markup; the text as it is when the language is empty
/// or one SAPI has no id for.
#[must_use]
pub fn sapi_text(text: &str, language: &str) -> (String, bool) {
    let Some(id) = sapi_language_id(&language_tag(language)) else {
        return (text.to_string(), false);
    };
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    (format!("<lang langid=\"{id:X}\">{escaped}</lang>"), true)
}

/// What Android's recognizer answered, as the Java helper reports it (`AzulSpeech`'s
/// `nativeOnHeard`): its status code (this enum's order: heard, nothing heard, denied,
/// unavailable, failed), the text, why not, and how sure. Blank text is nothing heard, an
/// unknown code a failure, the confidence kept to 0..1.
#[must_use]
pub(crate) fn heard_from_android(status: i32, text: &str, message: &str, confidence: f32) -> SpeechHeard {
    let status = match status {
        0 if text.trim().is_empty() => HeardStatus::NothingHeard,
        0 => HeardStatus::Heard,
        1 => HeardStatus::NothingHeard,
        2 => HeardStatus::Denied,
        3 => HeardStatus::Unavailable,
        _ => HeardStatus::Failed,
    };
    if status == HeardStatus::Heard {
        let confidence = if confidence.is_finite() {
            confidence.clamp(0.0, 1.0)
        } else {
            0.0
        };
        return SpeechHeard::heard(text.trim(), confidence);
    }
    let message = if message.trim().is_empty() && status == HeardStatus::Failed {
        "the speech recognizer failed"
    } else {
        message
    };
    SpeechHeard::none(status, message)
}

/// Says `text` on `backend`.
pub(crate) fn say_with(backend: Backend, text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let text = text.trim();
    if text.is_empty() {
        return SpeechStatus::Queued;
    }
    let language = language_tag(language);
    match backend {
        Backend::Headless => {
            headless::say(text, &language, interrupt);
            SpeechStatus::Queued
        }
        Backend::Platform => platform::say(text, &language, interrupt),
    }
}

fn stop_with(backend: Backend) {
    if backend == Backend::Platform {
        platform::stop();
    }
}

fn speaking_with(backend: Backend) -> bool {
    backend == Backend::Platform && platform::is_speaking()
}

fn available_with(backend: Backend) -> bool {
    match backend {
        Backend::Headless => true,
        Backend::Platform => platform::is_available(),
    }
}

/// Listens on `backend`.
pub(crate) fn listen_with(backend: Backend, language: &str, timeout_ms: u32) -> SpeechHeard {
    match backend {
        Backend::Headless => {
            let mut heard = headless::heard();
            heard.recognizer = SpeechRecognizer::Scripted;
            heard
        }
        Backend::Platform => answer_to_heard(
            native_listen(&language_tag(language), timeout_ms.max(1), &|| false),
            Some(Kind::Native),
        ),
    }
}
