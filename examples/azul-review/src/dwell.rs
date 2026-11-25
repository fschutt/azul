//! The dwell: the pen at rest on a page of code. A pure state machine on an injected clock; the app
//! feeds it the pointer's samples and a tick, and starts or stops a dictation on what it says.

/// Which pointers may start a dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allow {
    Off,
    Pen,
    Any,
}

impl Allow {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Pen => "pen",
            Self::Any => "any",
        }
    }

    #[must_use]
    pub fn parse(_name: &str) -> Option<Self> {
        None
    }
}

/// The shortest rest a hand can mean.
pub const MIN_REST_MS: u64 = 300;
/// The longest rest worth waiting for.
pub const MAX_REST_MS: u64 = 10_000;

/// A rest time inside what a hand can mean, and the word about it when it was changed.
#[must_use]
pub fn cap_rest_ms(ms: u64) -> (u64, Option<String>) {
    (ms, None)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub allow: Allow,
    pub rest_ms: u64,
    pub jitter_px: f32,
    pub away_px: f32,
}

/// One sample of the pointer over a page, in the page's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub pen: bool,
    pub contact: bool,
}

/// Where a rest was reported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub contact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Stay,
    MovedAway,
}

#[derive(Debug, Clone)]
pub struct Dwell {
    config: Config,
}

impl Dwell {
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn observe(&mut self, _sample: Sample, _now: u64) -> Reaction {
        Reaction::Stay
    }

    pub fn tick(&mut self, _now: u64) -> Option<Anchor> {
        None
    }

    pub fn left(&mut self) -> Reaction {
        Reaction::Stay
    }

    pub fn session_started(&mut self, _anchor: Anchor) {}

    pub fn session_ended(&mut self) {}

    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "dwell_test.rs"]
mod dwell_test;
