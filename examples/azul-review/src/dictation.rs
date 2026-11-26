//! The dictation's listening loop: speech recognised into a note while the pen rests.
//!
//! The engine's recogniser (`Speech::listen_blocking`) answers ONE utterance per call - it listens until
//! the speaker has paused for a moment or the call's time is up - and cannot be interrupted. So a
//! dictation is a loop of calls on a worker thread: each call's words are handed to the window as they
//! come (the note grows by a sentence at a time, not by a letter), a stop is looked for between two
//! calls, and a stretch of quiet longer than the silence time ends it. The loop is pure: the listener
//! and the clock are traits, so a script and a clock the test moves stand in for the microphone and
//! the time (`voice.rs` has the real ones).
//!
//! A listener may also stream (`Listener::listen_stream`): while a call goes it tells the microphone's
//! state, when the microphone is open (`Ready`: the window says "listening" from then and not before),
//! the words of the utterance so far (`Partial`: they show while the user speaks) and each utterance
//! as it ends (`Final`: the note's text). A listener that cannot stream keeps the default, which is
//! ready as the call begins and the call's one answer as the one utterance.
//!
//! A listener whose stream lasts until it is told to end (`Listener::listen_stream_until`, the engine's stream) is asked
//! between its events whether the dictation is over (a stop was asked for, or the quiet has lasted the silence time), so a
//! stop or the quiet ends the call where it is and what was said so far comes as the last utterance: the loop does not wait
//! for a chunk of a few seconds to end.

use std::cell::{Cell, RefCell};

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

/// How ready the microphone is, in the speech session's own words. The longer the way to "listening", the
/// longer a dictation waits before its first word can be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mic {
    /// Closed, and the recogniser is not loaded: the first words wait for both.
    #[default]
    Cold,
    /// Closed, the recogniser is loaded: only the microphone's opening is waited for.
    Warm,
    /// Open and keeping the last moments: a dictation starts at once and has the words spoken just before it.
    Armed,
    /// Open and taking the words of a dictation.
    Listening,
}

impl Mic {
    /// Every state, from the longest way to listening to none.
    pub const ALL: [Self; 4] = [Self::Cold, Self::Warm, Self::Armed, Self::Listening];

    /// The word a script reads (`AZREVIEW_MIC <name>`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Warm => "warm",
            Self::Armed => "armed",
            Self::Listening => "listening",
        }
    }

    /// Whether the microphone is open: the system's microphone indicator is on.
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Armed | Self::Listening)
    }
}

/// What a streaming call tells while it goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The microphone's state changed.
    State(Mic),
    /// The microphone is open and taking words: from here the user can be heard.
    Ready,
    /// The words of the utterance so far; the recogniser may still change them.
    Partial(String),
    /// An utterance is over: its words (status, text, the words with their times, who answered).
    Final(Heard),
}

/// One word of what was heard: when it was said (ms from the start of the call) and how sure the recogniser was.
#[derive(Debug, Clone, PartialEq)]
pub struct HeardWord {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
    pub probability: f32,
    /// The word is one of the vocabulary the recogniser was told to favour: favouring pulls the probability of a RIGHT word
    /// down, so a low probability on a biased word is no doubt.
    pub biased: bool,
}

/// One utterance, as the window takes it: its text, the wall-clock moment of the call that heard it and its words with
/// their times (empty where the recogniser gives none).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Said {
    pub text: String,
    /// The wall-clock moment the call began, Unix ms: the moment of a word is this plus its `start_ms`.
    pub started_unix_ms: u64,
    pub words: Vec<HeardWord>,
}

impl Said {
    /// An utterance of `text` with no words and no time, as it was given.
    #[must_use]
    pub fn plain(text: &str) -> Self {
        Self { text: text.to_string(), started_unix_ms: 0, words: Vec::new() }
    }
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
    /// Listens for up to `timeout_ms` in `language` (empty: the system's) and answers the whole call.
    fn listen(&mut self, language: &str, timeout_ms: u32) -> Heard;

    /// The same call as a stream: `events` is told the microphone's state, when it is open, the words so far and each
    /// utterance as it ends, while the call goes. The answer is the call's: its status, and its words when no
    /// `Final` told them (a stop answers "what was said so far"; the words of an utterance a `Final` told are not
    /// taken again).
    ///
    /// The default is for a listener that cannot stream: it is ready as the call begins - the most it can say - and
    /// answers with the one utterance of the call.
    fn listen_stream(&mut self, language: &str, timeout_ms: u32, events: &mut dyn FnMut(Event)) -> Heard {
        events(Event::Ready);
        self.listen(language, timeout_ms)
    }

    /// The same call as a stream that lasts until it is told to end: `should_stop` is asked between the events (a few
    /// times a second) and answers whether the dictation is to end now (a stop was asked for, or the quiet is long enough);
    /// the listener then ends its stream the way that keeps what was said so far (the last words come as the last `Final`)
    /// and answers. `timeout_ms` is only what a call that does not run on would take.
    ///
    /// The default is for a listener that does not run on: it is one [`Self::listen_stream`] call.
    fn listen_stream_until(
        &mut self,
        language: &str,
        timeout_ms: u32,
        events: &mut dyn FnMut(Event),
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Heard {
        let _ = should_stop;
        self.listen_stream(language, timeout_ms, events)
    }
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
    /// The microphone's state changed.
    State(Mic),
    /// The microphone is open: the dictation listens (told once).
    Ready,
    /// The words of the utterance so far (empty: none any more).
    Partial(String),
    /// Words that were understood: an utterance, with its words and times.
    Text(Said),
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

/// The shortest pause that ends an utterance worth asking the engine for, ms: the engine keeps its end-point to 200 to 3000.
pub const MIN_ENDPOINT_MS: u64 = 200;
/// The longest.
pub const MAX_ENDPOINT_MS: u64 = 3_000;

/// How long a pause ends an utterance, ms, inside what the engine takes, and the word about it when it was changed. 0 asks for
/// the engine's own (600 ms, the shortest that cost English nothing; long German prose is better at 800 to 1000).
#[must_use]
pub fn cap_endpoint_ms(ms: u64) -> (u64, Option<String>) {
    if ms == 0 {
        (0, None)
    } else if ms < MIN_ENDPOINT_MS {
        (MIN_ENDPOINT_MS, Some(format!("the end-point of {ms} ms is shorter than the engine takes: using {MIN_ENDPOINT_MS} ms")))
    } else if ms > MAX_ENDPOINT_MS {
        (MAX_ENDPOINT_MS, Some(format!("the end-point of {ms} ms is longer than the engine takes: using {MAX_ENDPOINT_MS} ms")))
    } else {
        (ms, None)
    }
}

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
///
/// Words being spoken (the words so far) count as words for the quiet: the dictation does not end under
/// someone who is talking. What a streaming call told as `Final` is handed over when the utterance ends, not when
/// the call does, and is not taken again from the call's answer.
pub fn run(
    plan: &Plan,
    listener: &mut dyn Listener,
    clock: &mut dyn Clock,
    stop: &mut dyn FnMut() -> bool,
    out: &mut dyn FnMut(Out),
) {
    // the clock is asked by the call's events and by its question whether to end: one at a time, never inside the other
    let clock = RefCell::new(clock);
    let now_ms = || clock.borrow_mut().now_ms();
    let last_words = Cell::new(now_ms());
    let mut ready_told = false;
    let mut shown = String::new();
    loop {
        if stop() {
            out(Out::Ended(End::Stopped));
            return;
        }
        let began = now_ms();
        let mut told_final = false;
        let heard = listener.listen_stream_until(
            &plan.language,
            plan.chunk_ms,
            &mut |event| match event {
                Event::State(mic) => out(Out::State(mic)),
                Event::Ready => {
                    if !ready_told {
                        ready_told = true;
                        out(Out::Ready);
                    }
                }
                Event::Partial(text) => {
                    let text = text.trim().to_string();
                    if text != shown {
                        if !text.is_empty() {
                            last_words.set(now_ms());
                        }
                        shown.clone_from(&text);
                        out(Out::Partial(text));
                    }
                }
                Event::Final(said) => {
                    let text = said.text.trim();
                    if said.status == Status::Heard && !text.is_empty() {
                        told_final = true;
                        last_words.set(now_ms());
                        shown.clear();
                        out(Out::Text(Said { text: text.to_string(), started_unix_ms: said.started_unix_ms, words: said.words.clone() }));
                    }
                }
            },
            // a stream that runs on ends when a stop was asked for or the quiet has lasted the silence time
            &mut || stop() || now_ms().saturating_sub(last_words.get()) >= plan.silence_ms,
        );
        let now = now_ms();
        match heard.status {
            Status::Heard if !told_final && !heard.text.trim().is_empty() => {
                shown.clear();
                out(Out::Text(Said {
                    text: heard.text.trim().to_string(),
                    started_unix_ms: heard.started_unix_ms,
                    words: heard.words.clone(),
                }));
                last_words.set(now);
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
        // Words so far that no utterance followed are gone with the call.
        if !shown.is_empty() {
            shown.clear();
            out(Out::Partial(String::new()));
        }
        if stop() {
            out(Out::Ended(End::Stopped));
            return;
        }
        if now.saturating_sub(last_words.get()) >= plan.silence_ms {
            out(Out::Ended(End::Silence));
            return;
        }
        let took = now.saturating_sub(began);
        if took < plan.min_gap_ms {
            clock.borrow_mut().sleep_ms(plan.min_gap_ms - took);
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
