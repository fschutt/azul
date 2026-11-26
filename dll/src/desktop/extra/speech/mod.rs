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
//! # A session that stands ready, and the stream
//!
//! Opening the microphone takes about 270 ms on a Mac and what is said meanwhile is lost, so a session
//! can keep it open: [`SpeechSession::arm`] opens it when the user is about to speak (the pen comes
//! in range, the button is hovered) and keeps the last 2 s in memory (overwritten, never saved), so
//! the next call or stream starts at once (measured: sound reaches the recognizer 2-13 ms after the
//! call for the built-in model, 14-49 ms for the system's, 270 ms cold) and hears what came in
//! before it. [`SpeechSession::state`] says [`SpeechState::Cold`], `Warm` (a recognizer is
//! initialised), `Armed` (microphone open) or `Listening`; [`SpeechStandby`] says what stays open
//! between calls (`Off`, `OnDemand` the default, `WhileArmed` with a safety time, `Always`).
//!
//! [`SpeechSession::start_stream`] listens as a stream: utterance after utterance on one microphone,
//! with [`SpeechEventKind::Ready`] when sound flows, a `Partial` while an utterance goes on (the
//! built-in model re-decodes it about every 500 ms: 71-235 ms after a word was said; the system's
//! gives what it reports: 440-860 ms after), a `Final` per utterance (the end of speech to the final:
//! 0.95 s built-in, 1.3 s system at the default 600 ms end-point), `Quiet` and `Ended`.
//! [`SpeechStream::wait_events`] blocks until there is an event (it wakes in under a millisecond),
//! [`SpeechStream::poll_events`] never blocks; `stop` gives the final of what was said so far,
//! `cancel` drops it. Times: a word's `start_ms` is from [`SpeechHeard::started_unix_ms`], the moment
//! the audio of its utterance began, which for the first utterance may be before the stream began
//! (the microphone's last 2 s); never negative.
//!
//! ## Measured on an Apple M2 (8 GB, release build, real microphone, a TTS sentence through the
//! loudspeakers, word times from the recognizer's own, 2026-11-26)
//!
//! | what | built-in model (Whistle, one thread) | system's recognizer |
//! |------|--------------------------------------|---------------------|
//! | `arm()` (the microphone opens) | 142-187 ms, its first sound about 270 ms after the call | the same microphone |
//! | start of a call to sound flowing: opened for the call / kept open | 197-211 ms / 2-13 ms | 206-268 ms / 14-24 ms |
//! | `start_stream` to `Ready`: cold / armed | 274 ms / 2-12 ms | 265 ms / 16-49 ms |
//! | a word said to the `Partial` that shows it | 71-235 ms (a partial every ~500 ms of audio) | 438-860 ms (median ~600) |
//! | end of speech to the `Final`, end-point 800 / 600 ms | 946 ms / about 750 ms | 2046-2241 ms at 1500 ms, 1240 ms at 500, 1319 ms at 700 |
//! | CPU while an utterance goes on | 9-15 % of one core (one partial decode of 1-3 s of speech about 80 ms) | 1 % here (the work is the system's speech service's) |
//! | `wait_events`: event produced to returned | at most 1 ms | at most 1 ms |
//! | a sentence that begins 1100 ms before the call, microphone not kept / kept | "Session is closed twice." / complete | "Is closed twice" / complete |
//!
//! The pause that ends an utterance ([`SpeechStreamOptions::endpoint_ms`]), built-in model, 12 English
//! clips of 4 s and 12 German read paragraphs of 18 s, the finals joined, word error rate against the
//! reference (a whole clip in one call: English 5.8 %, German 12.6 %):
//!
//! | end-point | 300 ms | 400 | 500 | 600 | 800 | 1000 |
//! |-----------|--------|-----|-----|-----|-----|------|
//! | English word error rate | 8.3 % | 8.3 % | 6.7 % | **5.8 %** | 5.8 % | 5.8 % |
//! | English clips cut into more than one utterance | 4 of 12 | 3 | 3 | 2 | 1 | 1 |
//! | German word error rate (paragraphs cut at their sentence ends) | 20.4 % | 19.7 % | 18.1 % | 17.2 % | 15.5 % | 13.3 % |
//! | German utterances per paragraph | 6.2 | 5.6 | 5.1 | 4.6 | 3.7 | 2.4 |
//!
//! So a stream's end-point is 600 ms unless the app asks (an app that dictates long German prose asks
//! for 800 to 1000 ms), and a call keeps its measured 800 ms (built-in model) and 1500 ms (system).
//! An armed microphone hears the room: a stream is handed the last second of what it heard
//! ([`SpeechStreamOptions::pre_roll_ms`]), and in a room where people talk an utterance can start
//! with their words or run on with them, so start streams when the user means it and end them with
//! `stop`.
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
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex, PoisonError,
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
pub mod keeper;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod mock;
pub mod order;
pub mod recognizer;
pub mod script;
pub mod vocabulary;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
#[cfg(test)]
#[path = "stream_test.rs"]
mod stream_test;

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
    keeper::{MicKeeper, DEFAULT_ARMED_IDLE_MS, DEFAULT_KEEP_MS},
    mock::{words_of, MockSpeech},
    order::Preference,
    recognizer::{
        Answer, Event, EventKind, Kind, Limits, Listen, Outcome, Readiness, Recognizer, SharedClock, StreamOptions,
        SystemClock, Word, STREAM_PRE_ROLL_MS,
    },
    vocabulary::Vocabulary,
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

/// Which recognizer is asked first. The chain falls through on a refusal whatever the order, so the
/// order only decides who answers when both could.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechPreference {
    /// By the language and the vocabulary, as measured on 24 clips of real read speech and on
    /// 12 sentences of TTS voices: English goes to the built-in model (5.8 % word errors against
    /// 10.0 % for the system's), German and every other language to the system's recognizer
    /// (German 7.8 % against 12.6 %), and so does any call while the vocabulary holds an
    /// identifier (`CssError`, `snake_case`): the system's recognizer writes it as spelt (3 of 3
    /// voices), the built-in model does not (0 of 3). The built-in model is never first when it
    /// is not set up. [`SpeechSession::recognizers`] says which order the last call had and why.
    Automatic,
    /// The system's recognizer first; the built-in model when it refuses.
    NativeFirst,
    /// The built-in model first; the system's recognizer when it refuses.
    BuiltinFirst,
}

impl Default for SpeechPreference {
    fn default() -> Self {
        Self::Automatic
    }
}

impl From<SpeechPreference> for Preference {
    fn from(preference: SpeechPreference) -> Self {
        match preference {
            SpeechPreference::Automatic => Self::Automatic,
            SpeechPreference::NativeFirst => Self::NativeFirst,
            SpeechPreference::BuiltinFirst => Self::BuiltinFirst,
        }
    }
}

/// What the session is ready for: how much a call would still have to do first.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechState {
    /// Nothing is initialised and the microphone is closed.
    Cold,
    /// A recognizer is initialised (the model is loaded, the system's recognizer was used) and the
    /// microphone is closed: a call opens it (about 270 ms on a Mac).
    Warm,
    /// The microphone is open and fills a buffer of the last 2 s (nothing is recorded or saved:
    /// the buffer is memory that is overwritten): a call starts at once and hears what came in.
    Armed,
    /// A call or a stream is listening.
    Listening,
}

/// How long the session keeps the microphone open between calls. The operating system's microphone
/// indicator is on for as long as it is open.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechStandby {
    /// Closed at the end of every call. `arm` is refused.
    Off,
    /// Kept `mic_keep_ms` after a call or an `arm` (the default).
    OnDemand,
    /// Opened by `arm` and kept until `disarm`, but given up when `armed_idle_ms` pass without an
    /// `arm` or a call (so a forgotten `disarm` cannot leave it on); after a call it is still armed.
    WhileArmed,
    /// Opened by the first `arm` or call and kept until `disarm`, `reset` or the end of the
    /// session, whatever happens in between: an explicit opt-in.
    Always,
}

impl Default for SpeechStandby {
    fn default() -> Self {
        Self::OnDemand
    }
}

/// One recognised word and when it was said.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechWord {
    /// The word as the recognizer wrote it (punctuation attached, as the engine gives it).
    pub text: AzString,
    /// Milliseconds from [`SpeechHeard::started_unix_ms`] to the start of the word: never
    /// negative. `started_unix_ms` is the moment the audio of the utterance began, which for the
    /// first utterance of a stream on an armed microphone can be before the stream was started (the
    /// pre-roll); the moment the word was said is `started_unix_ms + start_ms`. The first word of a
    /// built-in utterance typically starts 300 ms in (the end-pointer keeps 300 ms of room before
    /// the voice).
    pub start_ms: u32,
    /// And to its end.
    pub end_ms: u32,
    /// How sure the recognizer is of this word, 0 to 1.
    pub probability: f32,
    /// The word is one of the vocabulary the built-in recognizer was told to favour
    /// ([`SpeechSession::set_vocabulary`]). Favouring pulls the probability of a RIGHT word down
    /// (a correct "enum" came out at 0.32), so do not read the probability of a biased word as
    /// doubt.
    pub biased: bool,
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
            biased: word.biased,
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

/// What a stream reports.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechEventKind {
    /// Sound reaches the recognizer: say "speak now". The first event of a stream that could listen.
    Ready,
    /// What the utterance that goes on has come to so far (`heard.text`, `heard.words`).
    Partial,
    /// An utterance is over: its text and its words with their times, who gave them.
    Final,
    /// A stretch of the room without speech.
    Quiet,
    /// The stream is over; `heard.status` says how (`Heard` something was heard, `NothingHeard`,
    /// `Denied`, `Unavailable`, `Failed`) and `heard.message` why.
    Ended,
}

impl From<EventKind> for SpeechEventKind {
    fn from(kind: EventKind) -> Self {
        match kind {
            EventKind::Ready => Self::Ready,
            EventKind::Partial => Self::Partial,
            EventKind::Final => Self::Final,
            EventKind::Quiet => Self::Quiet,
            EventKind::Ended => Self::Ended,
        }
    }
}

/// One thing a stream reports.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechEvent {
    pub kind: SpeechEventKind,
    /// Milliseconds since the stream began.
    pub at_ms: u32,
    /// What was heard: for a partial or a final the text, the words with their times and who gave
    /// them; for an end how it ended (the status) and why (the message).
    pub heard: SpeechHeard,
}

impl_option!(SpeechEvent, OptionSpeechEvent, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    SpeechEvent,
    SpeechEventVec,
    SpeechEventVecDestructor,
    SpeechEventVecDestructorType,
    SpeechEventVecSlice,
    OptionSpeechEvent
);
impl_vec_debug!(SpeechEvent, SpeechEventVec);
impl_vec_clone!(SpeechEvent, SpeechEventVec, SpeechEventVecDestructor);
impl_vec_partialeq!(SpeechEvent, SpeechEventVec);

/// What a stream is asked to do.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SpeechStreamOptions {
    /// Report what an utterance has come to while it goes on (the built-in recognizer decodes
    /// the growing utterance about every 500 ms; the system's gives its own).
    pub partials: bool,
    /// How long a pause ends an utterance, ms (kept to 200 .. 3000; 0: 600, the shortest that cost
    /// English nothing in the measurement on 24 read clips: English word error rate 5.8 % at 600,
    /// 800 and 1000 ms, 6.7 % at 500, 8.3 % at 400 and 300; German read paragraphs 17.2 % at 600,
    /// 15.5 % at 800, 13.3 % at 1000 ms, so an app that dictates long German prose asks for 800 to
    /// 1000). The system's recognizer ends an utterance when its text has not changed that long
    /// (and then takes about 0.7 s to give the final: end of speech to final 1.3 s at 500 and
    /// 700 ms, 2.1 s at the 1500 ms a call uses).
    pub endpoint_ms: u32,
    /// How long the stream runs at most, ms (0: until it is stopped; at most one hour).
    pub max_ms: u32,
    /// How many utterances end the stream (0: until it is stopped or its time is up).
    pub max_utterances: u32,
    /// How much of what an open (armed) microphone heard before the stream began is handed to it,
    /// ms (1000 by default, at most the 2000 the microphone keeps, 0: none). A second covers the
    /// words said just before the app started the stream (a pen that rests for 1.2 s before the
    /// dictation begins) and keeps most of the talk of the room out of it: an armed microphone hears
    /// the room, so start streams only when the user means it and end them with `stop` (the last
    /// words come as the last final), not with `cancel` (which drops them).
    pub pre_roll_ms: u32,
}

impl Default for SpeechStreamOptions {
    fn default() -> Self {
        Self {
            partials: true,
            endpoint_ms: 0,
            max_ms: 0,
            max_utterances: 0,
            pre_roll_ms: STREAM_PRE_ROLL_MS,
        }
    }
}

impl From<SpeechStreamOptions> for StreamOptions {
    fn from(options: SpeechStreamOptions) -> Self {
        Self {
            partials: options.partials,
            endpoint_ms: options.endpoint_ms,
            max_ms: options.max_ms,
            max_utterances: options.max_utterances,
            pre_roll_ms: options.pre_roll_ms,
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
    /// How long the microphone stays open after a call, ms (5000, at most 60000; 0 closes it at the
    /// end of every call): the next call then starts at once, without the 430 ms a Mac takes to open
    /// one, and what was said in between is not lost (the last 2 s are kept for it). The system
    /// shows its microphone indicator for as long. A cancel, a `reset` and the end of the session
    /// close it at once.
    pub mic_keep_ms: u32,
    /// How long an armed microphone waits without an `arm` or a call before the session gives it up,
    /// ms (60000, at most 600000): the safety of [`SpeechStandby::WhileArmed`].
    pub armed_idle_ms: u32,
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
            mic_keep_ms: DEFAULT_KEEP_MS,
            armed_idle_ms: DEFAULT_ARMED_IDLE_MS,
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
    /// The microphone: opened for a call, kept for the next one for `mic_keep_ms`.
    keeper: MicKeeper,
    /// What the app chose to keep open.
    standby: Mutex<SpeechStandby>,
    /// How many calls and streams are listening.
    active: AtomicU32,
    /// Sound is reaching the recognizer of the call that runs.
    flowing: AtomicBool,
    /// What the app asked for: who is asked first.
    preference: Mutex<Preference>,
    /// Whether the vocabulary holds an identifier the system's recognizer takes (so it goes first).
    identifiers: AtomicBool,
    /// Whether the system's recognizer takes a vocabulary at all (macOS and iOS; a scripted run
    /// plays the platform where it does).
    native_takes_vocabulary: bool,
    /// The language of the last call: the status lines say who goes first for it.
    last_language: Mutex<String>,
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
        if scripted {
            let (chain, preference) = scripted_chain(hooks, clock);
            return Self::from_chain(chain, exe_dir, true, preference, true, MicKeeper::without_microphone());
        }
        let keeper = MicKeeper::new(capture::platform_source());
        keeper.set_keep_ms(DEFAULT_KEEP_MS);
        let chain = platform_chain(clock, exe_dir.as_deref(), &keeper);
        Self::from_chain(chain, exe_dir, false, Preference::Automatic, NATIVE_TAKES_VOCABULARY, keeper)
    }

    fn from_chain(
        chain: Chain,
        exe_dir: Option<PathBuf>,
        scripted: bool,
        preference: Preference,
        native_takes_vocabulary: bool,
        keeper: MicKeeper,
    ) -> Self {
        let inner = Arc::new(SessionInner {
            chain: Mutex::new(chain),
            cancel: AtomicBool::new(false),
            lines: Mutex::new(Vec::new()),
            limits: Mutex::new(SpeechLimits::default()),
            vocabulary: Mutex::new(Vec::new()),
            keeper,
            standby: Mutex::new(SpeechStandby::OnDemand),
            active: AtomicU32::new(0),
            flowing: AtomicBool::new(false),
            preference: Mutex::new(preference),
            identifiers: AtomicBool::new(false),
            native_takes_vocabulary,
            last_language: Mutex::new(String::new()),
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
        Self::from_chain(chain, None, true, Preference::Automatic, true, MicKeeper::without_microphone())
    }

    /// The same with the keeper whose microphone a test watches.
    #[cfg(test)]
    pub(crate) fn on_chain_with_keeper(chain: Chain, keeper: MicKeeper) -> Self {
        Self::from_chain(chain, None, true, Preference::Automatic, true, keeper)
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
        let microphone = inner.keeper.clone();
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
            Err(failure) => (Builtin::refused(inner.keeper.clone(), failure.message), failure.status),
        };
        let vocabulary = inner.vocabulary.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let _ = recognizer.set_keywords(&vocabulary);
        let mut chain = inner.chain.lock().unwrap_or_else(PoisonError::into_inner);
        chain.replace(Box::new(recognizer));
        drop(chain);
        inner.refresh_lines();
        status.into()
    }

    /// The words the recognizers should favour (names, jargon, identifiers of the app): at most
    /// `max_keywords` of 64 characters each (the system's recognizer takes 100, Apple's guidance).
    /// Answers what was cut, empty when nothing was. Pass the identifiers as they are written:
    /// the session shapes them for each recognizer. The system's recognizer gets the entries as
    /// given (`CssError` is then written as spelt: 15 of 33 identifiers in the experiment, 0
    /// without), the built-in model gets them with the spoken parts of every identifier
    /// (`css error`, `unwrap or else`; it cannot write a compound but writes the parts right: its
    /// "something else" fell from 18 to 11 of 33), and the spoken parts count against the limit. A
    /// word of an answer that is one of the vocabulary's has `biased` set (favouring pulls the
    /// probability of a right word down: do not read it as doubt). Ordinary speech gained no
    /// code word in 36 runs of the experiment. The numbers are in [`vocabulary`]'s module docs.
    pub fn set_vocabulary(&self, words: StringVec) -> AzString {
        let Some(inner) = self.inner() else {
            return AzString::from(String::new());
        };
        let words: Vec<String> = words.as_slice().iter().map(|w| w.as_str().to_string()).collect();
        let note = inner.chain.lock().unwrap_or_else(PoisonError::into_inner).set_vocabulary(&words);
        // An identifier among the phrases the system's recognizer takes puts it first.
        let (taken, _) = Vocabulary::from_words(&words, NATIVE_PHRASES, NATIVE_PHRASE_CHARS);
        inner
            .identifiers
            .store(inner.native_takes_vocabulary && taken.has_identifier(), Ordering::SeqCst);
        *inner.vocabulary.lock().unwrap_or_else(PoisonError::into_inner) = words;
        inner.refresh_lines();
        AzString::from(note.unwrap_or_default())
    }

    /// Listens as a stream: utterance after utterance on one microphone, with what each has
    /// come to as it goes, until [`SpeechStream::stop`], [`SpeechStream::cancel`], the options'
    /// time or utterance count. Returns at once; the stream listens on a thread of its own, through
    /// the same chain as [`Self::listen_blocking`] (the same order, the same hand-over to the next
    /// recognizer on a refusal before the first event). What was said before the call and is
    /// still in the microphone's buffer (an armed session keeps the last 2 s) is part of the first
    /// utterance. A call or another stream that runs waits for this one to end.
    #[must_use]
    pub fn start_stream(&self, language: AzString, options: SpeechStreamOptions) -> SpeechStream {
        let shared = Arc::new(StreamShared::new());
        let handle = SpeechStream::from_shared(Arc::clone(&shared));
        let ended = |status: HeardStatus, message: &str| {
            shared.push(SpeechEvent {
                kind: SpeechEventKind::Ended,
                at_ms: 0,
                heard: SpeechHeard::none(status, message),
            });
            shared.finish();
        };
        if self.inner().is_none() {
            ended(HeardStatus::Unavailable, "this speech session is closed");
            return handle;
        }
        let session = self.clone();
        let weak = Arc::downgrade(&shared);
        let language = language.as_str().to_string();
        let spawned = std::thread::Builder::new()
            .name("azul-speech-stream".to_string())
            .spawn(move || run_stream(&session, &weak, &language, options.into()));
        if spawned.is_err() {
            ended(HeardStatus::Failed, "no thread could be started for the stream");
        }
        handle
    }

    /// What the session is ready for ([`SpeechState`]).
    #[must_use]
    pub fn state(&self) -> SpeechState {
        let Some(inner) = self.inner() else {
            return SpeechState::Cold;
        };
        if inner.active.load(Ordering::SeqCst) > 0 || inner.keeper.is_leased() {
            SpeechState::Listening
        } else if inner.keeper.is_held() {
            SpeechState::Armed
        } else if inner.chain.try_lock().map_or(true, |chain| chain.is_warm()) {
            SpeechState::Warm
        } else {
            SpeechState::Cold
        }
    }

    /// Opens the microphone now and keeps it open (see [`SpeechStandby`]), so that the next call
    /// starts at once and hears what came in meanwhile (the last 2 s): arm when the user is about
    /// to speak (the pen comes in range, the button is hovered), not when they have begun. Opening
    /// costs about 270 ms once. Answers why it could not, empty when the session is armed (the
    /// permission rules of the microphone apply: a refusal is a sentence the user can act on).
    /// Arming an armed session starts the waiting time again. It takes no language and needs none:
    /// the microphone does not depend on one, the built-in model takes the language per call, and
    /// the system's recognizer is made for the language of each call (14-49 ms with an armed
    /// microphone). Safe to call while a call or a stream listens (they hand the microphone back
    /// armed) and at any time again; [`Self::disarm`] is as safe.
    pub fn arm(&self) -> AzString {
        let Some(inner) = self.inner() else {
            return AzString::from("this speech session is closed".to_string());
        };
        match inner.keeper.arm() {
            Ok(()) => AzString::from(String::new()),
            Err((_outcome, message)) => AzString::from(message),
        }
    }

    /// Closes the microphone that waits for a call: the system's indicator goes out. A call that
    /// is running goes on.
    pub fn disarm(&self) {
        if let Some(inner) = self.inner() {
            inner.keeper.disarm();
        }
    }

    /// What stays open between calls: [`SpeechStandby::OnDemand`] by default.
    pub fn set_standby(&self, standby: SpeechStandby) {
        let Some(inner) = self.inner() else {
            return;
        };
        *inner.standby.lock().unwrap_or_else(PoisonError::into_inner) = standby;
        inner.apply_standby();
    }

    /// Whether sound is reaching the recognizer in the call that runs: false until the
    /// microphone delivers its first sound (it takes a moment to start: about 270 ms on a Mac),
    /// true from then until the call ends. A window shows "speak now" from the moment it turns
    /// true: what is said before is lost. Poll it from another thread while `listen_blocking`
    /// runs on a worker thread. A recognizer that cannot tell (Windows, Android) makes it true
    /// when it begins to listen.
    #[must_use]
    pub fn audio_flowing(&self) -> bool {
        self.inner().is_some_and(|inner| inner.flowing.load(Ordering::SeqCst))
    }

    /// Who is asked first: [`SpeechPreference::Automatic`] by default.
    pub fn set_preference(&self, preference: SpeechPreference) {
        let Some(inner) = self.inner() else {
            return;
        };
        *inner.preference.lock().unwrap_or_else(PoisonError::into_inner) = preference.into();
        inner.refresh_lines();
    }

    /// What the recognizers are asked to keep to; the defaults are [`SpeechLimits::default`].
    pub fn set_limits(&self, limits: SpeechLimits) {
        let Some(inner) = self.inner() else {
            return;
        };
        *inner.limits.lock().unwrap_or_else(PoisonError::into_inner) = limits;
        inner.apply_standby();
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
        inner.flowing.store(false, Ordering::SeqCst);
        let _listening = ActiveGuard::new(&inner.active);
        let heard = {
            let mut chain = inner.chain.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = inner.arrange(&mut chain, language.as_str());
            chain.listen_with(
                language.as_str(),
                timeout_ms,
                &|| inner.cancel.load(Ordering::SeqCst),
                &|| inner.flowing.store(true, Ordering::SeqCst),
            )
        };
        inner.flowing.store(false, Ordering::SeqCst);
        inner.refresh_lines();
        answer_to_heard(heard.answer, heard.by)
    }

    /// Ends the listen call that is running: it answers what was said until now. Does nothing
    /// when none is running.
    pub fn cancel(&self) {
        if let Some(inner) = self.inner() {
            inner.cancel.store(true, Ordering::SeqCst);
            // A microphone that waits for the next call is let go too: the user is done (a call that
            // is running has the microphone, and gives it up at its end) - unless the app wants it
            // always open.
            if *inner.standby.lock().unwrap_or_else(PoisonError::into_inner) != SpeechStandby::Always {
                inner.keeper.release();
            }
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
            inner.keeper.release();
            inner.chain.lock().unwrap_or_else(PoisonError::into_inner).reset();
            inner.refresh_lines();
        }
    }
}

/// The most events a stream keeps for an app that does not collect them.
const STREAM_QUEUE: usize = 1_000;

/// What a stream shares between its handles and its thread.
struct StreamShared {
    events: Mutex<VecDeque<SpeechEvent>>,
    arrived: Condvar,
    stop: AtomicBool,
    cancel: AtomicBool,
    running: AtomicBool,
}

impl StreamShared {
    fn new() -> Self {
        Self {
            events: Mutex::new(VecDeque::new()),
            arrived: Condvar::new(),
            stop: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(true),
        }
    }

    fn push(&self, event: SpeechEvent) {
        let mut queue = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.len() >= STREAM_QUEUE {
            // Nobody collects: the oldest partial goes first, then the oldest.
            let at = queue.iter().position(|e| e.kind == SpeechEventKind::Partial).unwrap_or(0);
            queue.remove(at);
        }
        queue.push_back(event);
        drop(queue);
        self.arrived.notify_all();
    }

    fn finish(&self) {
        let queue = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        self.running.store(false, Ordering::SeqCst);
        drop(queue);
        self.arrived.notify_all();
    }
}

fn public_event(event: Event) -> SpeechEvent {
    SpeechEvent {
        kind: event.kind.into(),
        at_ms: u32::try_from(event.at_ms).unwrap_or(u32::MAX),
        heard: answer_to_heard(event.answer, event.by),
    }
}

/// A listen that goes on: utterance after utterance, with what each has come to as it goes
/// ([`SpeechSession::start_stream`]). The events are collected with `wait_events` (blocks until
/// there is one) or `poll_events` (never blocks).
///
/// ```text
/// let stream = session.start_stream(language, SpeechStreamOptions { partials: true, ..Default::default() });
/// loop {
///     for event in stream.wait_events(100).as_slice() { /* Ready, Partial, Final, Quiet, Ended */ }
///     if !stream.is_running() { break }
/// }
/// ```
#[repr(C)]
pub struct SpeechStream {
    /// Opaque pointer to the shared stream (null on a default handle).
    pub ptr: *mut c_void,
    /// Whether this handle owns one reference of the stream (every `clone` does).
    pub run_destructor: bool,
}

// SAFETY: the stream behind the pointer is `Sync` (its parts are mutexes and atomics).
unsafe impl Send for SpeechStream {}
unsafe impl Sync for SpeechStream {}

impl Default for SpeechStream {
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Clone for SpeechStream {
    fn clone(&self) -> Self {
        if self.ptr.is_null() {
            return Self::default();
        }
        // SAFETY: `ptr` came from `Arc::into_raw` of a `StreamShared` and is alive while a handle
        // that owns a reference exists; a clone takes one reference of its own.
        unsafe { Arc::increment_strong_count(self.ptr.cast::<StreamShared>()) };
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Drop for SpeechStream {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            // SAFETY: this handle owns one reference.
            let shared = unsafe { Arc::from_raw(self.ptr.cast::<StreamShared>()) };
            // The last handle going ends the stream: nobody is left to hear it.
            if Arc::strong_count(&shared) == 1 {
                shared.cancel.store(true, Ordering::SeqCst);
            }
            drop(shared);
            self.ptr = core::ptr::null_mut();
            self.run_destructor = false;
        }
    }
}

impl SpeechStream {
    fn from_shared(shared: Arc<StreamShared>) -> Self {
        Self {
            ptr: Arc::into_raw(shared).cast_mut().cast::<c_void>(),
            run_destructor: true,
        }
    }

    fn shared(&self) -> Option<&StreamShared> {
        // SAFETY: a non-null `ptr` is alive while this handle is (see `Clone` and `Drop`).
        unsafe { self.ptr.cast::<StreamShared>().as_ref() }
    }

    /// The events that came since the last call, oldest first; never waits.
    #[must_use]
    pub fn poll_events(&self) -> SpeechEventVec {
        let Some(shared) = self.shared() else {
            return SpeechEventVec::from_vec(Vec::new());
        };
        let mut queue = shared.events.lock().unwrap_or_else(PoisonError::into_inner);
        SpeechEventVec::from_vec(queue.drain(..).collect())
    }

    /// The events that came since the last call, waiting up to `timeout_ms` for the first one:
    /// returns as soon as there is one (a worker thread that waits here neither spins nor adds a
    /// polling period to the latency), at once when the stream is over, empty when the time ran
    /// out first.
    #[must_use]
    pub fn wait_events(&self, timeout_ms: u32) -> SpeechEventVec {
        let Some(shared) = self.shared() else {
            return SpeechEventVec::from_vec(Vec::new());
        };
        let mut queue = shared.events.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.is_empty() && shared.running.load(Ordering::SeqCst) {
            queue = shared
                .arrived
                .wait_timeout_while(queue, std::time::Duration::from_millis(u64::from(timeout_ms)), |queue| {
                    queue.is_empty() && shared.running.load(Ordering::SeqCst)
                })
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        SpeechEventVec::from_vec(queue.drain(..).collect())
    }

    /// Ends the stream after the utterance that goes on (an `Ended` event follows).
    pub fn stop(&self) {
        if let Some(shared) = self.shared() {
            shared.stop.store(true, Ordering::SeqCst);
        }
    }

    /// Ends the stream now: the utterance that goes on is dropped without a final (an `Ended`
    /// event follows).
    pub fn cancel(&self) {
        if let Some(shared) = self.shared() {
            shared.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Whether the stream is still listening (events may still be waiting to be collected).
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared().is_some_and(|shared| shared.running.load(Ordering::SeqCst))
    }
}

/// The thread of a stream: listens through the chain and hands every event to the handles.
fn run_stream(session: &SpeechSession, shared: &std::sync::Weak<StreamShared>, language: &str, options: StreamOptions) {
    let Some(inner) = session.inner() else {
        return;
    };
    let push = |event: Event| {
        if let Some(shared) = shared.upgrade() {
            shared.push(public_event(event));
        }
    };
    let stopped = || shared.upgrade().map_or(true, |s| s.stop.load(Ordering::SeqCst));
    let cancelled =
        || inner.cancel.load(Ordering::SeqCst) || shared.upgrade().map_or(true, |s| s.cancel.load(Ordering::SeqCst));
    inner.cancel.store(false, Ordering::SeqCst);
    inner.flowing.store(false, Ordering::SeqCst);
    let _listening = ActiveGuard::new(&inner.active);
    {
        let mut chain = inner.chain.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = inner.arrange(&mut chain, language);
        let _ = chain.stream_with(
            language,
            &options,
            &cancelled,
            &stopped,
            &|| inner.flowing.store(true, Ordering::SeqCst),
            &push,
        );
    }
    inner.flowing.store(false, Ordering::SeqCst);
    if let Some(shared) = shared.upgrade() {
        shared.finish();
    }
    inner.refresh_lines();
}

enum BuiltinSource {
    File(String),
    Bytes(Vec<u8>),
}

/// Counts a listening call for as long as it lives.
struct ActiveGuard<'a>(&'a AtomicU32);

impl<'a> ActiveGuard<'a> {
    fn new(counter: &'a AtomicU32) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl SessionInner {
    /// What the keeper is told from the standby and the limits.
    fn apply_standby(&self) {
        let limits = *self.limits.lock().unwrap_or_else(PoisonError::into_inner);
        let standby = *self.standby.lock().unwrap_or_else(PoisonError::into_inner);
        let (keep_ms, armed_idle_ms, always) = match standby {
            SpeechStandby::Off => (0, 0, false),
            SpeechStandby::OnDemand => (limits.mic_keep_ms, 0, false),
            SpeechStandby::WhileArmed => (limits.mic_keep_ms, limits.armed_idle_ms, false),
            SpeechStandby::Always => (limits.mic_keep_ms, 0, true),
        };
        self.keeper.set_always(always);
        self.keeper.set_armed_idle_ms(armed_idle_ms);
        self.keeper.set_keep_ms(keep_ms);
    }

    /// Puts who goes first for a call in `language` first in `chain` (the rule is
    /// [`order::choose`]) and remembers the language for the status lines.
    fn arrange(&self, chain: &mut Chain, language: &str) -> order::Order {
        let language: String = language.chars().take(64).collect();
        let preference = *self.preference.lock().unwrap_or_else(PoisonError::into_inner);
        let identifiers = self.identifiers.load(Ordering::SeqCst);
        let order = order::choose(preference, &language, identifiers, chain.is_ready(Kind::Builtin));
        chain.set_first(order.first);
        *self.last_language.lock().unwrap_or_else(PoisonError::into_inner) = language;
        order
    }

    /// The status lines of the chain as they stand now, in the order the next call would try
    /// them (for the language of the last one), the first with why. Called with the chain free
    /// (after a call or a change).
    fn refresh_lines(&self) {
        let language = self.last_language.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let (states, order) = match self.chain.try_lock() {
            Ok(mut chain) => {
                let order = self.arrange(&mut chain, &language);
                (chain.states(), order)
            }
            Err(_) => return,
        };
        // Who goes first only needs saying when there is a choice: two recognizers that can listen.
        let choice = states.iter().filter(|state| state.health == Health::Ready).count() >= 2;
        let lines = states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                let kind = match state.kind {
                    Kind::Native => "native",
                    Kind::Builtin => "built-in",
                    Kind::Scripted => "scripted",
                };
                match &state.health {
                    Health::Ready if choice && index == 0 && state.kind == order.first => {
                        format!("{kind} | {} | ready; tried first: {}", state.label, order.why)
                    }
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
fn platform_chain(clock: SharedClock, exe_dir: Option<&std::path::Path>, keeper: &MicKeeper) -> Chain {
    Chain::new(clock)
        .with(Box::new(NativeRecognizer::new(keeper.clone())))
        .with(Box::new(Builtin::discover(
            exe_dir.map(std::path::Path::to_path_buf),
            keeper.clone(),
            Tuning::default(),
        )))
}

/// The chain of a headless or E2E test run: what `AZ_SPEECH_SCRIPT` scripts, else what
/// `AZ_SPEECH_HEARD` holds (heard on every call, at once) or nothing.
fn scripted_chain(hooks: &azul_core::run_mode::TestHooks, clock: SharedClock) -> (Chain, Preference) {
    let Some(path) = hooks.speech_script.as_deref().filter(|path| !path.is_empty()) else {
        return (Chain::new(clock).with(Box::new(HeadlessRecognizer)), Preference::Automatic);
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
            (chain, script.preference)
        }
        Err(why) => (
            Chain::new(clock).with(Box::new(MockSpeech::new(
                Kind::Scripted,
                vec![mock::Step::Refuse {
                    outcome: Outcome::Unavailable,
                    message: format!("{SPEECH_SCRIPT_VAR} {}: {why}", path.to_string_lossy()),
                }],
            ))),
            Preference::Automatic,
        ),
    }
}

/// Whether the system's recognizer of this platform takes a vocabulary.
const NATIVE_TAKES_VOCABULARY: bool = cfg!(any(target_os = "ios", target_os = "macos"));

/// The most phrases the system's recognizer is given (Apple's guidance for `contextualStrings`:
/// 100 or fewer, short ones), each at most [`NATIVE_PHRASE_CHARS`] characters.
const NATIVE_PHRASES: usize = 100;
const NATIVE_PHRASE_CHARS: usize = 64;

/// The platform's own recognizer as a link of the chain.
struct NativeRecognizer {
    /// What it is asked to favour: on macOS and iOS (`contextualStrings`), nowhere else yet.
    vocabulary: Vocabulary,
    /// The session's microphone (macOS and iOS: the recognizer is fed from it).
    keeper: MicKeeper,
    /// A call has listened: the frameworks are loaded and the permissions known.
    used: bool,
}

impl NativeRecognizer {
    fn new(keeper: MicKeeper) -> Self {
        Self {
            vocabulary: Vocabulary::default(),
            keeper,
            used: false,
        }
    }
}

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

    fn warm(&self) -> bool {
        self.used
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn listen_stream(
        &mut self,
        call: &Listen<'_>,
        options: &StreamOptions,
        stop: &dyn Fn() -> bool,
        emit: &dyn Fn(EventKind, Answer),
    ) -> Answer {
        let answer = platform::stream(
            &language_tag(call.language),
            call.timeout_ms.max(1),
            call.cancel,
            stop,
            options,
            emit,
            &self.vocabulary,
            call.flowing,
            &self.keeper,
            true,
        );
        self.used |= matches!(answer.outcome, Outcome::Heard | Outcome::NothingHeard);
        answer
    }

    fn set_vocabulary(&mut self, words: &[String]) -> Option<String> {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            let (vocabulary, note) = Vocabulary::from_words(words, NATIVE_PHRASES, NATIVE_PHRASE_CHARS);
            self.vocabulary = vocabulary;
            note.map(|note| format!("{}: {note}", Kind::Native.describe()))
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos")))]
        {
            let _ = words;
            None
        }
    }

    fn listen(&mut self, call: &Listen<'_>) -> Answer {
        let began = call.clock.unix_ms();
        let mut answer = native_listen(
            &language_tag(call.language),
            call.timeout_ms.max(1),
            call.cancel,
            &self.vocabulary,
            call.flowing,
            &self.keeper,
        );
        if answer.started_unix_ms == 0 && answer.outcome == Outcome::Heard {
            answer.started_unix_ms = began;
        }
        self.used |= matches!(answer.outcome, Outcome::Heard | Outcome::NothingHeard);
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
fn native_listen(
    language: &str,
    timeout_ms: u32,
    cancel: &dyn Fn() -> bool,
    vocabulary: &Vocabulary,
    flowing: &dyn Fn(),
    keeper: &MicKeeper,
) -> Answer {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        platform::listen(language, timeout_ms, cancel, vocabulary, flowing, keeper)
    }
    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    {
        // The other platforms' recognizers answer at the end of their own call (a cancel waits for
        // it), take no vocabulary yet, and cannot say when sound begins to flow: from the start.
        let _ = (cancel, vocabulary, keeper);
        flowing();
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
            native_listen(
                &language_tag(language),
                timeout_ms.max(1),
                &|| false,
                &Vocabulary::default(),
                &|| {},
                &MicKeeper::new(capture::platform_source()),
            ),
            Some(Kind::Native),
        ),
    }
}
