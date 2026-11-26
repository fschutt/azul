//! The recognizers a session listens with, and what they answer: plain types and the one trait
//! the system's recognizer, the built-in model and the scripted mock all implement, so the chain
//! ([`super::chain`]) and its tests do not care which is which. std only.

use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Which recognizer. The order of the chain is the order of this list for a platform:
/// native first, then the built-in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The platform's own: `SFSpeechRecognizer`, WinRT, Android's, none on Linux.
    Native,
    /// The small model the app provides (Whistle through the Cactus engine).
    Builtin,
    /// A script: a test's or a headless run's stand-in.
    Scripted,
}

impl Kind {
    /// What a message calls it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Native => "the system's recognizer",
            Self::Builtin => "the built-in recognizer",
            Self::Scripted => "the scripted recognizer",
        }
    }
}

/// One recognised word, with when it was said.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    /// Milliseconds from the start of the audio.
    pub start_ms: u32,
    pub end_ms: u32,
    /// How sure the recognizer is of this word, 0 to 1.
    pub probability: f32,
    /// The word is one of the vocabulary the recognizer was told to favour: favouring pulls a
    /// right word's probability down (a correct "enum" came out at 0.32), so a consumer must not
    /// read the probability of a biased word as doubt.
    pub biased: bool,
}

impl Word {
    #[must_use]
    pub fn new(text: &str, start_ms: u32, end_ms: u32, probability: f32) -> Self {
        Self {
            text: text.to_string(),
            start_ms,
            end_ms,
            probability,
            biased: false,
        }
    }
}

/// How one try ended: the same five as the public `HeardStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// Something was understood.
    Heard,
    /// The time was up and nothing was said.
    NothingHeard,
    /// The user (or an administrator) said no to the microphone or to speech recognition.
    Denied,
    /// There is no recognizer here, or it is not set up: the message says what is missing.
    Unavailable,
    /// The recognizer broke: the message says how.
    Failed,
}

/// What one recognizer answered to one call.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub outcome: Outcome,
    /// What was understood (empty unless `Heard`).
    pub text: String,
    /// Why nothing was heard, readable enough to show (empty for a plain `Heard`).
    pub message: String,
    /// How sure the recognizer is, 0 to 1 (0 where it does not say).
    pub confidence: f32,
    /// The words with their times, where the recognizer says them (empty where it does not).
    pub words: Vec<Word>,
    /// The language that was recognised (a BCP 47 tag; empty where it is not known).
    pub language: String,
    /// When the audio began, in milliseconds since the Unix epoch: with a word's time it gives
    /// the moment the word was said.
    pub started_unix_ms: u64,
}

impl Answer {
    /// An answer without text.
    #[must_use]
    pub fn none(outcome: Outcome, message: &str) -> Self {
        Self {
            outcome,
            text: String::new(),
            message: message.to_string(),
            confidence: 0.0,
            words: Vec::new(),
            language: String::new(),
            started_unix_ms: 0,
        }
    }

    /// What was understood, as words: the text is the words with single spaces, the confidence
    /// their mean probability.
    #[must_use]
    pub fn heard_words(words: Vec<Word>, language: &str, started_unix_ms: u64) -> Self {
        let text = words
            .iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let confidence = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| clamp01(w.probability)).sum::<f32>() / words.len() as f32
        };
        Self {
            outcome: Outcome::Heard,
            text,
            message: String::new(),
            confidence,
            words,
            language: language.to_string(),
            started_unix_ms,
        }
    }
}

/// A probability or confidence kept to 0..1 (not-a-number is 0).
#[must_use]
pub fn clamp01(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// What tells the time and waits: the wall clock of a real run, a clock the test moves by hand.
pub trait Clock: Send + Sync {
    /// Milliseconds since the clock began (only differences mean anything).
    fn now_ms(&self) -> u64;
    /// Milliseconds since the Unix epoch.
    fn unix_ms(&self) -> u64;
    /// Waits `ms`.
    fn sleep_ms(&self, ms: u64);
}

/// The wall clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    began: Instant,
}

impl SystemClock {
    #[must_use]
    pub fn new() -> Self {
        Self { began: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.began.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// How often a wait looks at the cancel flag, ms.
pub const WAIT_SLICE_MS: u64 = 20;

/// What a recognizer is asked to do: listen for up to `timeout_ms` in `language`.
#[derive(Clone, Copy)]
pub struct Listen<'a> {
    /// A BCP 47 tag, or empty for the system's language.
    pub language: &'a str,
    /// The time to listen, already kept to the limits.
    pub timeout_ms: u32,
    pub clock: &'a dyn Clock,
    /// Whether the caller wants the call to end now.
    pub cancel: &'a dyn Fn() -> bool,
    /// Called once, as soon as sound reaches the recognizer in this call: the microphone is open
    /// and delivering, so a UI may say "speak now". Never called when no sound can come (the
    /// microphone was refused). A recognizer that cannot tell calls it when it begins to listen.
    pub flowing: &'a dyn Fn(),
}

impl Listen<'_> {
    /// Waits `ms` on the clock, looking at the cancel flag between slices; returns the time
    /// waited, which is less than `ms` only when the call was cancelled.
    pub fn wait(&self, ms: u64) -> u64 {
        let mut waited = 0;
        while waited < ms {
            if (self.cancel)() {
                break;
            }
            let slice = WAIT_SLICE_MS.min(ms - waited);
            self.clock.sleep_ms(slice);
            waited += slice;
        }
        waited
    }
}

/// What a recognizer says about itself without listening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    /// It can listen (it may ask the user for a permission first).
    Ready,
    /// It cannot, and why.
    Refused { outcome: Outcome, message: String },
}

/// Something that listens: the system's recognizer, the built-in model, a script.
pub trait Recognizer: Send {
    fn kind(&self) -> Kind;

    /// A name for a status line ("Speech framework", "Whistle").
    fn label(&self) -> String;

    /// Whether it can listen, asked without listening and without asking the user.
    fn readiness(&mut self) -> Readiness;

    /// Listens for up to `call.timeout_ms` and answers what was said. It looks at `call.cancel`
    /// at least every few hundred milliseconds and answers `NothingHeard` when it is set.
    fn listen(&mut self, call: &Listen<'_>) -> Answer;

    /// The words to favour (names, jargon), for a recognizer that takes them; answers the word
    /// about what it cut, `None` when nothing was or it takes none.
    fn set_vocabulary(&mut self, _words: &[String]) -> Option<String> {
        None
    }
}

/// The limits on what a caller may ask for: every input has a realistic least and most, a value
/// outside is kept to it and the answer says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The shortest time to listen, ms: below it nothing is heard but a click.
    pub min_listen_ms: u32,
    /// The longest time to listen in one call, ms.
    pub max_listen_ms: u32,
    /// The longest language tag.
    pub max_language_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            min_listen_ms: 500,
            max_listen_ms: 60_000,
            max_language_chars: 35,
        }
    }
}

impl Limits {
    /// `ms` kept between the least and the most, and the word about it when it was changed.
    #[must_use]
    pub fn cap_listen_ms(&self, ms: u32) -> (u32, Option<String>) {
        if ms < self.min_listen_ms {
            (
                self.min_listen_ms,
                Some(format!(
                    "the listening time of {ms} ms is too short to hear anything: using {} ms",
                    self.min_listen_ms
                )),
            )
        } else if ms > self.max_listen_ms {
            (
                self.max_listen_ms,
                Some(format!(
                    "the listening time of {ms} ms is longer than one call may take: using {} ms",
                    self.max_listen_ms
                )),
            )
        } else {
            (ms, None)
        }
    }

    /// The language tag trimmed; one that is longer than any tag is not a tag, so the system's
    /// language is used, and the word about it.
    #[must_use]
    pub fn cap_language(&self, language: &str) -> (String, Option<String>) {
        let tag = language.trim();
        if tag.chars().count() > self.max_language_chars {
            (
                String::new(),
                Some(format!(
                    "the language tag is longer than {} characters: using the system's language",
                    self.max_language_chars
                )),
            )
        } else {
            (tag.to_string(), None)
        }
    }
}

/// A clock shared by a chain and its recognizers.
pub type SharedClock = Arc<dyn Clock>;
