//! The dictation's listening loop (stub).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Heard,
    Nothing,
    Denied,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub status: Status,
    pub text: String,
    pub message: String,
}

pub trait Listener {
    fn listen(&mut self, language: &str, timeout_ms: u32) -> Heard;
}

pub trait Clock {
    fn now_ms(&mut self) -> u64;
    fn sleep_ms(&mut self, ms: u64);
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub language: String,
    pub chunk_ms: u32,
    pub silence_ms: u64,
    pub min_gap_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Text(String),
    Failed { status: Status, message: String },
    Ended(End),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Stopped,
    Silence,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Stopped,
    Silence,
    Failed,
}

impl Cause {
    pub const ALL: [Self; 3] = [Self::Stopped, Self::Silence, Self::Failed];

    #[must_use]
    pub const fn name(self) -> &'static str {
        ""
    }
}

impl From<End> for Cause {
    fn from(_: End) -> Self {
        Self::Failed
    }
}

pub const MIN_SILENCE_MS: u64 = 2_000;
pub const MAX_SILENCE_MS: u64 = 120_000;

#[must_use]
pub fn cap_silence_ms(ms: u64) -> (u64, Option<String>) {
    (ms, None)
}

pub fn run(
    _plan: &Plan,
    _listener: &mut dyn Listener,
    _clock: &mut dyn Clock,
    _stop: &mut dyn FnMut() -> bool,
    _out: &mut dyn FnMut(Out),
) {
}

#[must_use]
pub fn join_text(existing: &str, _heard: &str) -> String {
    existing.to_string()
}

#[cfg(test)]
#[path = "dictation_test.rs"]
mod dictation_test;
