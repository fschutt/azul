//! The chain: the recognizers a session listens with, in order, and what a call does with them.
//!
//! The first one that is not marked refused listens. A refusal (`Denied`, `Unavailable`) marks
//! that recognizer refused for the rest of the session and the next one listens in the SAME call,
//! with the whole time: the fallback happens once, not on every call. A `Failed` is passed over
//! within the call the same way but only marks the recognizer refused after two in a row (a
//! hiccup is not a pattern). `NothingHeard` is an answer: a quiet room does not hand the call
//! over. The answer says which recognizer gave it ([`Heard::by`]) and, once, what happened to the
//! ones before it. std only.

use super::recognizer::{
    Answer, Kind, Limits, Listen, Outcome, Readiness, Recognizer, SharedClock,
};

/// How many failures in a row make a recognizer refused for the session.
pub const FAILURES_IN_A_ROW: u8 = 2;

/// What a session knows about one recognizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// It listens (it may still have to ask the user for a permission).
    Ready,
    /// It will not be asked again in this session, and why.
    Refused { outcome: Outcome, message: String },
}

/// One line of the status the chain shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkState {
    pub kind: Kind,
    pub label: String,
    pub health: Health,
}

/// What a chain answered: the answer, and which recognizer gave it (`None`: nobody could listen).
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub answer: Answer,
    pub by: Option<Kind>,
}

struct Link {
    recognizer: Box<dyn Recognizer>,
    health: Health,
    /// `Failed` answers in a row.
    failures: u8,
}

/// The recognizers of a session, in the order they are tried.
pub struct Chain {
    links: Vec<Link>,
    clock: SharedClock,
    limits: Limits,
}

/// What one refused try left behind, for the message.
struct Refusal {
    kind: Kind,
    outcome: Outcome,
    message: String,
    /// It happened in this call (the others were remembered from an earlier one).
    fresh: bool,
}

impl Chain {
    /// A chain with no recognizer; `clock` dates the audio and times the waits.
    #[must_use]
    pub fn new(clock: SharedClock) -> Self {
        Self {
            links: Vec::new(),
            clock,
            limits: Limits::default(),
        }
    }

    /// The recognizer after the ones the chain has.
    #[must_use]
    pub fn with(mut self, recognizer: Box<dyn Recognizer>) -> Self {
        self.links.push(Self::link(recognizer));
        self
    }

    fn link(recognizer: Box<dyn Recognizer>) -> Link {
        Link {
            recognizer,
            health: Health::Ready,
            failures: 0,
        }
    }

    /// Puts `recognizer` where the one of its kind is (the end when there is none); what the
    /// session learned about the old one is forgotten.
    pub fn replace(&mut self, recognizer: Box<dyn Recognizer>) {
        let kind = recognizer.kind();
        match self.links.iter_mut().find(|l| l.recognizer.kind() == kind) {
            Some(link) => *link = Self::link(recognizer),
            None => self.links.push(Self::link(recognizer)),
        }
    }

    /// Puts the recognizer of `kind` first: the others keep their order, and what the session
    /// learned about each (a refusal) stays with it. Does nothing when the chain has none of
    /// that kind.
    pub fn set_first(&mut self, kind: Kind) {
        if let Some(at) = self.links.iter().position(|link| link.recognizer.kind() == kind) {
            if at > 0 {
                let link = self.links.remove(at);
                self.links.insert(0, link);
            }
        }
    }

    /// Whether the recognizer of `kind` is in the chain, not marked refused, and says it can
    /// listen.
    pub fn is_ready(&mut self, kind: Kind) -> bool {
        self.links
            .iter_mut()
            .find(|link| link.recognizer.kind() == kind)
            .is_some_and(|link| link.health == Health::Ready && matches!(link.recognizer.readiness(), Readiness::Ready))
    }

    /// The limits a call is kept to.
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// Hands the vocabulary to every recognizer that takes one; the words about what was cut.
    pub fn set_vocabulary(&mut self, words: &[String]) -> Option<String> {
        let notes: Vec<String> = self
            .links
            .iter_mut()
            .filter_map(|link| link.recognizer.set_vocabulary(words))
            .collect();
        (!notes.is_empty()).then(|| notes.join("; "))
    }

    /// Forgets every refusal: the next call asks every recognizer again (the user changed a
    /// permission, the app registered a model).
    pub fn reset(&mut self) {
        for link in &mut self.links {
            link.health = Health::Ready;
            link.failures = 0;
        }
    }

    /// Where each recognizer stands: refused in this session, or what it says about itself.
    pub fn states(&mut self) -> Vec<LinkState> {
        self.links
            .iter_mut()
            .map(|link| {
                let health = match &link.health {
                    Health::Refused { .. } => link.health.clone(),
                    Health::Ready => match link.recognizer.readiness() {
                        Readiness::Ready => Health::Ready,
                        Readiness::Refused { outcome, message } => Health::Refused { outcome, message },
                    },
                };
                LinkState {
                    kind: link.recognizer.kind(),
                    label: link.recognizer.label(),
                    health,
                }
            })
            .collect()
    }

    /// Listens for up to `timeout_ms` in `language` (kept to the limits), with the first
    /// recognizer that is not refused; `cancel` ends the call early.
    pub fn listen(&mut self, language: &str, timeout_ms: u32, cancel: &dyn Fn() -> bool) -> Heard {
        self.listen_with(language, timeout_ms, cancel, &|| {})
    }

    /// The same, and `flowing` is called once when sound reaches the recognizer that listens
    /// (see [`Listen::flowing`]).
    pub fn listen_with(
        &mut self,
        language: &str,
        timeout_ms: u32,
        cancel: &dyn Fn() -> bool,
        flowing: &dyn Fn(),
    ) -> Heard {
        let (timeout_ms, time_note) = self.limits.cap_listen_ms(timeout_ms);
        let (language, language_note) = self.limits.cap_language(language);
        let notes: Vec<String> = time_note.into_iter().chain(language_note).collect();

        // What refused, in the order of the chain: in this call, and remembered from before.
        let mut trail: Vec<Refusal> = Vec::new();
        let mut partial: Option<Answer> = None;
        let clock = self.clock.clone();
        for link in &mut self.links {
            let kind = link.recognizer.kind();
            if let Health::Refused { outcome, message } = &link.health {
                trail.push(Refusal {
                    kind,
                    outcome: *outcome,
                    message: message.clone(),
                    fresh: false,
                });
                continue;
            }
            let answer = link.recognizer.listen(&Listen {
                language: &language,
                timeout_ms,
                clock: &*clock,
                cancel,
                flowing,
            });
            match answer.outcome {
                Outcome::Heard | Outcome::NothingHeard => {
                    link.failures = 0;
                    return Self::answered(answer, kind, &notes, &trail);
                }
                Outcome::Denied | Outcome::Unavailable => {
                    link.health = Health::Refused {
                        outcome: answer.outcome,
                        message: answer.message.clone(),
                    };
                    trail.push(Refusal {
                        kind,
                        outcome: answer.outcome,
                        message: answer.message,
                        fresh: true,
                    });
                }
                Outcome::Failed => {
                    link.failures = link.failures.saturating_add(1);
                    if link.failures >= FAILURES_IN_A_ROW {
                        link.health = Health::Refused {
                            outcome: Outcome::Failed,
                            message: answer.message.clone(),
                        };
                    }
                    trail.push(Refusal {
                        kind,
                        outcome: Outcome::Failed,
                        message: answer.message.clone(),
                        fresh: true,
                    });
                    if !answer.text.is_empty() {
                        partial = Some(answer);
                    }
                }
            }
        }
        Self::nobody(&notes, &trail, partial)
    }

    /// The answer of the recognizer that listened, with the word about the ones before it that
    /// refused in this call (the older refusals were said when they happened).
    fn answered(mut answer: Answer, kind: Kind, notes: &[String], trail: &[Refusal]) -> Heard {
        let mut parts: Vec<String> = Vec::new();
        if !answer.message.is_empty() {
            parts.push(answer.message.clone());
        }
        parts.extend(notes.iter().cloned());
        let before: Vec<String> = trail
            .iter()
            .filter(|r| r.fresh)
            .map(|r| {
                let verb = if r.outcome == Outcome::Failed { "failed" } else { "refused" };
                if r.message.is_empty() {
                    format!("{} {verb}", r.kind.describe())
                } else {
                    format!("{} {verb} ({})", r.kind.describe(), r.message)
                }
            })
            .collect();
        if !before.is_empty() {
            let verb = if answer.outcome == Outcome::Heard { "answered" } else { "listened" };
            parts.push(format!("{}; {} {verb}", before.join("; "), kind.describe()));
        }
        answer.message = parts.join("; ");
        Heard {
            answer,
            by: Some(kind),
        }
    }

    /// Nobody could listen: the refusal that helps most (a "no" the user can change comes before
    /// a missing piece the app has to provide, before a failure), every message with its
    /// recognizer, and the words of a failure that had some.
    fn nobody(notes: &[String], trail: &[Refusal], partial: Option<Answer>) -> Heard {
        let outcome = if trail.iter().any(|r| r.outcome == Outcome::Denied) {
            Outcome::Denied
        } else if trail.is_empty() || trail.iter().any(|r| r.outcome == Outcome::Unavailable) {
            Outcome::Unavailable
        } else {
            Outcome::Failed
        };
        let mut parts: Vec<String> = notes.to_vec();
        if trail.is_empty() {
            parts.push("there is no speech recognizer in this session".to_string());
        } else {
            parts.extend(
                trail
                    .iter()
                    .map(|r| format!("{}: {}", r.kind.describe(), r.message)),
            );
        }
        let mut answer = partial.unwrap_or_else(|| Answer::none(outcome, ""));
        answer.outcome = outcome;
        answer.message = parts.join("; ");
        Heard { answer, by: None }
    }
}

#[cfg(test)]
#[path = "chain_test.rs"]
mod chain_test;
