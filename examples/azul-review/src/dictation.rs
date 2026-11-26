//! The dictation's listening loop: speech recognised into a note while the pen rests.
//!
//! The engine's recogniser (`Speech::listen_blocking`) answers ONE utterance per call - it listens until
//! the speaker has paused for a moment or the call's time is up - and cannot be interrupted. So a
//! dictation is a loop of calls on a worker thread: each call's words are handed to the window as they
//! come (the note grows by a sentence at a time, not by a letter), a stop is looked for between two
//! calls, and a stretch of quiet longer than the silence time ends it. The loop is pure: the listener
//! and the clock are traits, so a script and a clock the test moves stand in for the microphone and
//! the time (`voice.rs` has the real ones).

/// How one listen call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Words were understood.
    Heard,
    /// The time was up and nothing was said.
    Nothing,
    /// The user refused the microphone or speech recognition.
    Denied,
    /// There is no recogniser here, or the app is not set up for one.
    Unavailable,
    /// The recogniser failed.
    Failed,
}

/// Who answered a listen call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Recognizer {
    /// The system's recogniser.
    Native,
    /// The built-in one (the engine's own model).
    Builtin,
    /// A script (tests).
    Scripted,
    /// Nobody could listen.
    #[default]
    None,
}

/// One word of what was heard: when it was said (ms from the start of the call) and how sure the recogniser was.
#[derive(Debug, Clone, PartialEq)]
pub struct HeardWord {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
    pub probability: f32,
}

/// What one listen call heard.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub status: Status,
    pub text: String,
    /// Why nothing was heard, readable enough to show.
    pub message: String,
    /// The words with their times (empty where the recogniser gives none).
    pub words: Vec<HeardWord>,
    /// The wall-clock moment the call began, Unix ms: the moment of a word is this plus its `start_ms`.
    pub started_unix_ms: u64,
    /// Who answered.
    pub recognizer: Recognizer,
}

impl Heard {
    /// What a call that heard `text` (or failed with `message`) comes to, with no words and no time.
    #[must_use]
    pub fn new(status: Status, text: &str, message: &str) -> Self {
        Self {
            status,
            text: text.to_string(),
            message: message.to_string(),
            words: Vec::new(),
            started_unix_ms: 0,
            recognizer: Recognizer::None,
        }
    }
}

/// What listens: the engine's recogniser, or a script.
pub trait Listener {
    /// Listens for up to `timeout_ms` in `language` (empty: the system's).
    fn listen(&mut self, language: &str, timeout_ms: u32) -> Heard;
}

/// What tells the time and waits.
pub trait Clock {
    fn now_ms(&mut self) -> u64;
    fn sleep_ms(&mut self, ms: u64);
}

/// What a dictation is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The language to recognise (a BCP 47 tag; empty: the system's).
    pub language: String,
    /// The time of one listen call, ms.
    pub chunk_ms: u32,
    /// How long without words ends the dictation, ms.
    pub silence_ms: u64,
    /// The least time between the starts of two calls, ms: a listener that answers at once (a
    /// stand-in) must not be asked in a tight loop.
    pub min_gap_ms: u64,
}

/// What the loop hands to the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Words that were understood.
    Text(String),
    /// The listener refused: the status and its words, for the user.
    Failed { status: Status, message: String },
    /// The loop is over.
    Ended(End),
}

/// Why the loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A stop was asked for.
    Stopped,
    /// No words for the silence time.
    Silence,
    /// The listener refused.
    Failed,
}

/// Why a dictation ended, as the app tells it (the stop's cause or the loop's end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The hovering pen went away from the anchor.
    Moved,
    /// The pen began a stroke.
    Drew,
    /// The pen rested again: the second dwell.
    SecondDwell,
    /// The stop button.
    Button,
    /// Escape.
    Key,
    /// Another file was opened.
    File,
    /// The window is closing.
    Closed,
    /// A stop asked for by the loop's owner without a cause of its own.
    Stopped,
    /// No words for the silence time.
    Silence,
    /// The listener refused.
    Failed,
}

impl Cause {
    pub const ALL: [Self; 10] = [
        Self::Moved,
        Self::Drew,
        Self::SecondDwell,
        Self::Button,
        Self::Key,
        Self::File,
        Self::Closed,
        Self::Stopped,
        Self::Silence,
        Self::Failed,
    ];

    /// The word a script reads (`AZREVIEW_DICTATION ended <id> <name>`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Moved => "moved",
            Self::Drew => "drew",
            Self::SecondDwell => "second-dwell",
            Self::Button => "button",
            Self::Key => "key",
            Self::File => "file",
            Self::Closed => "closed",
            Self::Stopped => "stopped",
            Self::Silence => "silence",
            Self::Failed => "failed",
        }
    }
}

impl From<End> for Cause {
    fn from(end: End) -> Self {
        match end {
            End::Stopped => Self::Stopped,
            End::Silence => Self::Silence,
            End::Failed => Self::Failed,
        }
    }
}

/// The shortest silence worth a setting: shorter than a breath between two sentences.
pub const MIN_SILENCE_MS: u64 = 2_000;
/// The longest: two minutes of quiet is a forgotten dictation.
pub const MAX_SILENCE_MS: u64 = 120_000;
/// The silence time when nothing says otherwise.
pub const DEFAULT_SILENCE_MS: u64 = 10_000;
/// The time of one listen call.
pub const CHUNK_MS: u32 = 5_000;
/// The least time between two calls.
pub const MIN_GAP_MS: u64 = 400;

/// A silence time inside what a person can mean, and the word about it when it was changed.
#[must_use]
pub fn cap_silence_ms(ms: u64) -> (u64, Option<String>) {
    if ms < MIN_SILENCE_MS {
        (
            MIN_SILENCE_MS,
            Some(format!(
                "the silence time of {ms} ms is shorter than a breath between sentences: using {MIN_SILENCE_MS} ms"
            )),
        )
    } else if ms > MAX_SILENCE_MS {
        (
            MAX_SILENCE_MS,
            Some(format!(
                "the silence time of {ms} ms is longer than a forgotten dictation: using {MAX_SILENCE_MS} ms"
            )),
        )
    } else {
        (ms, None)
    }
}

/// Listens, call after call, handing each call's words to `out` as they come, until `stop` says so
/// (asked between two calls and after one, so what was said while the stop was asked for is kept), the
/// listener refuses, or `plan.silence_ms` pass without words. The last thing handed over is
/// [`Out::Ended`].
pub fn run(
    plan: &Plan,
    listener: &mut dyn Listener,
    clock: &mut dyn Clock,
    stop: &mut dyn FnMut() -> bool,
    out: &mut dyn FnMut(Out),
) {
    let mut last_words = clock.now_ms();
    loop {
        if stop() {
            out(Out::Ended(End::Stopped));
            return;
        }
        let began = clock.now_ms();
        let heard = listener.listen(&plan.language, plan.chunk_ms);
        let now = clock.now_ms();
        match heard.status {
            Status::Heard if !heard.text.trim().is_empty() => {
                out(Out::Text(heard.text.trim().to_string()));
                last_words = now;
            }
            Status::Heard | Status::Nothing => {}
            Status::Denied | Status::Unavailable | Status::Failed => {
                out(Out::Failed {
                    status: heard.status,
                    message: heard.message,
                });
                out(Out::Ended(End::Failed));
                return;
            }
        }
        if stop() {
            out(Out::Ended(End::Stopped));
            return;
        }
        if now.saturating_sub(last_words) >= plan.silence_ms {
            out(Out::Ended(End::Silence));
            return;
        }
        let took = now.saturating_sub(began);
        if took < plan.min_gap_ms {
            clock.sleep_ms(plan.min_gap_ms - took);
        }
    }
}

/// `existing` with `heard` after it: the heard words in single spaces, one space between the two;
/// what the user typed in `existing` is kept as it is.
#[must_use]
pub fn join_text(existing: &str, heard: &str) -> String {
    let heard = heard.split_whitespace().collect::<Vec<_>>().join(" ");
    if heard.is_empty() {
        return existing.to_string();
    }
    let existing = existing.trim_end();
    if existing.is_empty() {
        heard
    } else {
        format!("{existing} {heard}")
    }
}

#[cfg(test)]
#[path = "dictation_test.rs"]
mod dictation_test;
