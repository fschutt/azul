//! A scripted recognizer: the unit tests and a headless or E2E run drive the same chain code
//! ([`super::chain::Chain`]) with it that a real run drives with the system's recognizer and the
//! built-in model. A script is a list of steps, one per listen call: words said after a delay (with
//! their times and probabilities), nothing said, a refusal, a failure part-way. Time is a
//! [`Clock`]: a [`ManualClock`] makes a delay of ten seconds cost nothing. std only.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
};

use super::recognizer::{Answer, Clock, Kind, Listen, Outcome, Readiness, Recognizer, Word};

/// A clock that stands still until somebody waits on it: `sleep_ms` moves it by that much and
/// returns at once. `unix_ms` is a fixed origin plus the same time.
#[derive(Debug)]
pub struct ManualClock {
    now: AtomicU64,
    origin_unix_ms: u64,
}

impl ManualClock {
    /// A clock at 0 whose Unix time is `origin_unix_ms` there.
    #[must_use]
    pub fn new(origin_unix_ms: u64) -> Self {
        Self {
            now: AtomicU64::new(0),
            origin_unix_ms,
        }
    }

    /// The same, shared.
    #[must_use]
    pub fn shared(origin_unix_ms: u64) -> Arc<Self> {
        Arc::new(Self::new(origin_unix_ms))
    }

    /// Moves the clock on without anybody waiting.
    pub fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn unix_ms(&self) -> u64 {
        self.origin_unix_ms + self.now_ms()
    }

    fn sleep_ms(&self, ms: u64) {
        self.advance(ms);
    }
}

/// One listen call of a script.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The words are said `delay_ms` into the call; their own times count from the start of the
    /// audio.
    Say {
        delay_ms: u32,
        words: Vec<Word>,
        language: String,
    },
    /// Nothing is said for the whole time of the call.
    Silence,
    /// The recognizer refuses at once: `Denied` or `Unavailable`, with the message.
    Refuse { outcome: Outcome, message: String },
    /// The recognizer breaks `after_ms` into the call, with the words it had understood.
    Fail {
        after_ms: u32,
        message: String,
        partial: Vec<Word>,
    },
}

/// What a script does when its steps are used up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// Nothing is heard any more.
    Silence,
    /// The last step again, as often as asked.
    RepeatLast,
}

/// A recognizer that follows a script.
pub struct MockSpeech {
    kind: Kind,
    label: String,
    steps: VecDeque<Step>,
    last: Option<Step>,
    then: Then,
    calls: Arc<AtomicU32>,
}

impl MockSpeech {
    /// A mock that stands in for the recognizer of `kind` and follows `steps`.
    #[must_use]
    pub fn new(kind: Kind, steps: Vec<Step>) -> Self {
        Self {
            kind,
            label: format!("{} (scripted)", kind.describe()),
            steps: steps.into(),
            last: None,
            then: Then::Silence,
            calls: Arc::new(AtomicU32::new(0)),
        }
    }

    /// What the mock does once its steps are used up (silence by default).
    #[must_use]
    pub fn then(mut self, then: Then) -> Self {
        self.then = then;
        self
    }

    /// The name a status line shows.
    #[must_use]
    pub fn labelled(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// How many times `listen` was called: a handle that stays valid after the mock went into a
    /// chain.
    #[must_use]
    pub fn calls(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.calls)
    }

    /// The step of the next call. A step that was cut short (the time was up first, the call was
    /// cancelled) is put back by [`Self::keep`], so what was about to be said is said in the next
    /// call.
    fn next_step(&mut self) -> Step {
        match self.steps.pop_front() {
            Some(step) => step,
            None => match (self.then, self.last.clone()) {
                (Then::RepeatLast, Some(step)) => step,
                _ => Step::Silence,
            },
        }
    }

    fn keep(&mut self, step: Step) {
        self.steps.push_front(step);
    }
}

impl Recognizer for MockSpeech {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn label(&self) -> String {
        self.label.clone()
    }

    fn readiness(&mut self) -> Readiness {
        // What the next step would say, without using it up.
        match self.steps.front() {
            Some(Step::Refuse { outcome, message }) => Readiness::Refused {
                outcome: *outcome,
                message: message.clone(),
            },
            _ => Readiness::Ready,
        }
    }

    fn listen(&mut self, call: &Listen<'_>) -> Answer {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let started_unix_ms = call.clock.unix_ms();
        let timeout = u64::from(call.timeout_ms);
        let step = self.next_step();
        self.last = Some(step.clone());
        match step {
            Step::Refuse { outcome, message } => Answer::none(outcome, &message),
            Step::Silence => {
                call.wait(timeout);
                Answer::none(Outcome::NothingHeard, "nothing was said")
            }
            Step::Say {
                delay_ms,
                words,
                language,
            } => {
                let delay = u64::from(delay_ms);
                if delay > timeout {
                    // The speaker is still thinking when the time is up: the words come in the
                    // next call, that much sooner.
                    let waited = call.wait(timeout);
                    self.keep(Step::Say {
                        delay_ms: u32::try_from(delay - waited).unwrap_or(u32::MAX),
                        words,
                        language,
                    });
                    return Answer::none(Outcome::NothingHeard, "nothing was said");
                }
                let waited = call.wait(delay);
                if waited < delay {
                    self.keep(Step::Say {
                        delay_ms: u32::try_from(delay - waited).unwrap_or(u32::MAX),
                        words,
                        language,
                    });
                    return Answer::none(Outcome::NothingHeard, "listening was cancelled");
                }
                Answer::heard_words(words, &language, started_unix_ms)
            }
            Step::Fail {
                after_ms,
                message,
                partial,
            } => {
                let after = u64::from(after_ms);
                if after > timeout {
                    let waited = call.wait(timeout);
                    self.keep(Step::Fail {
                        after_ms: u32::try_from(after - waited).unwrap_or(u32::MAX),
                        message,
                        partial,
                    });
                    return Answer::none(Outcome::NothingHeard, "nothing was said");
                }
                call.wait(after);
                let mut answer = Answer::none(Outcome::Failed, &message);
                answer.text = partial
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                answer.words = partial;
                answer.started_unix_ms = started_unix_ms;
                answer
            }
        }
    }
}

/// Words one after the other, each `per_word_ms` long, starting `first_ms` into the audio, every
/// one at probability `probability`: the shape a script of plain text gets.
#[must_use]
pub fn words_of(text: &str, first_ms: u32, per_word_ms: u32, probability: f32) -> Vec<Word> {
    let mut at = first_ms;
    text.split_whitespace()
        .map(|word| {
            let start = at;
            at = at.saturating_add(per_word_ms);
            Word::new(word, start, at, probability)
        })
        .collect()
}

#[cfg(test)]
#[path = "mock_test.rs"]
mod mock_test;
