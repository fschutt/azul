//! Where the window's speech session stands while a thread makes it, and what a dictation asked for in that time does.
//!
//! The session (the chain of recognisers, its vocabulary, the call a stop ends) is made on a thread once the window is
//! there, so that the first window does not wait for it (it was 23 to 94 ms of the way to the window). A dictation asked
//! before the session is ready WAITS for it: the rest is kept and starts when the session arrives, which is a state of
//! the window and not a race between two threads. The wait is bounded ([`MAKE_LIMIT_MS`]): a session that cannot be made,
//! or whose thread never answers, is told ONCE and the dictation then listens through the engine's own call, which has no
//! vocabulary - a session that fails costs the biasing and never a dictation.
//!
//! Plain data and a clock that is passed in: no thread, no session and no microphone.

/// How long a dictation waits for the session to be made before the thread is given up on, ms.
pub const MAKE_LIMIT_MS: u64 = 5_000;

/// Where the session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Session {
    /// None was asked for (a test, a run that makes none): the engine's own call listens.
    #[default]
    Unasked,
    /// A thread is making it.
    Making,
    /// The window has it.
    Ready,
    /// It could not be made (or its thread never answered); the engine's own call listens.
    Failed,
}

impl Session {
    /// The name a script reads (`azreview_state`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unasked => "unasked",
            Self::Making => "making",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

/// What a dictation that is asked for now does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked<T> {
    /// It starts now, at `what`. `report` is a failure to tell the reviewer's log, the first time only.
    Start { what: T, report: Option<String> },
    /// It waits: the session is being made, and `what` is kept (the latest rest only).
    Wait,
}

/// What the arrival of the session, or of its failure, means for the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made<T> {
    /// The rest that waited, to start now (or not: the pen may have moved on).
    pub start: Option<T>,
    /// A failure to tell, the first time only.
    pub report: Option<String>,
}

/// The state of the session and the dictation that waits for it. `T` is what a dictation is asked at (the pen's rest).
#[derive(Debug, Clone)]
pub struct Gate<T> {
    state: Session,
    /// When the thread was begun, ms of the window's voice clock.
    since_ms: u64,
    waiting: Option<T>,
    why: Option<String>,
    told: bool,
}

impl<T> Default for Gate<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Gate<T> {
    /// A gate for a window that has asked for no session.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Session::Unasked,
            since_ms: 0,
            waiting: None,
            why: None,
            told: false,
        }
    }

    #[must_use]
    pub const fn state(&self) -> Session {
        self.state
    }

    /// Why the session could not be made (the first reason), while it has not been.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.why.as_deref()
    }

    /// A dictation waits for the session.
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// The thread that makes the session was begun at `now_ms`: what an earlier failure said is over.
    pub fn begin(&mut self, now_ms: u64) {
        self.state = Session::Making;
        self.since_ms = now_ms;
        self.why = None;
        self.told = false;
    }

    /// A dictation is asked at `what`.
    pub fn ask(&mut self, what: T, now_ms: u64) -> Asked<T> {
        // The limit is looked at here too: a rest after it starts on the engine's own call.
        let given_up = self.expire(now_ms);
        match self.state {
            Session::Making => {
                self.waiting = Some(what);
                Asked::Wait
            }
            _ => Asked::Start {
                what,
                report: given_up.and_then(|m| m.report),
            },
        }
    }

    /// The thread answered: the session is ready (`Ok`) or could not be made (`Err`, with why).
    pub fn made(&mut self, outcome: Result<(), String>) -> Made<T> {
        match outcome {
            Ok(()) => {
                self.state = Session::Ready;
                self.why = None;
                Made { start: self.waiting.take(), report: None }
            }
            Err(why) => {
                if self.state != Session::Failed {
                    self.state = Session::Failed;
                    self.why = Some(why.clone());
                }
                let report = self.tell(why);
                Made { start: self.waiting.take(), report }
            }
        }
    }

    /// The limit is over for a thread that never answered: the session is given up on, the dictation that waited starts on
    /// the engine's own call, and it is told. `None` while there is nothing to give up on.
    pub fn expire(&mut self, now_ms: u64) -> Option<Made<T>> {
        if self.state != Session::Making || now_ms.saturating_sub(self.since_ms) < MAKE_LIMIT_MS {
            return None;
        }
        let why = format!("the speech session was not made within {} s", MAKE_LIMIT_MS / 1_000);
        self.state = Session::Failed;
        self.why = Some(why.clone());
        let report = self.tell(why);
        Some(Made { start: self.waiting.take(), report })
    }

    /// The rest that waits is not wanted any more (the pen moved on): it is given back, and starts nothing.
    pub fn forget(&mut self) -> Option<T> {
        self.waiting.take()
    }

    /// `why`, the first time a failure is told since the thread was begun.
    fn tell(&mut self, why: String) -> Option<String> {
        if self.told {
            None
        } else {
            self.told = true;
            Some(why)
        }
    }
}

#[cfg(test)]
#[path = "speech_gate_test.rs"]
mod speech_gate_test;
