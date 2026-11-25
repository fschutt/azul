//! The dwell: the pen at rest on a page of code. A pure state machine on an injected clock (the
//! milliseconds the caller passes in): the app feeds it the pointer's samples and a tick, and starts
//! or stops a dictation on what it says. No window, no timer, no azul here.
//!
//! - A **rest** is the pointer staying within `jitter_px` of where it was when the rest began. Any
//!   move past that begins a new rest at the new place; a lift or a set-down in place does not.
//! - After `rest_ms` of one rest, [`Dwell::tick`] reports it once, as an [`Anchor`]: a hovering pen
//!   (hover dwell), or a pen still touching after a stroke (contact dwell).
//! - While a dictation runs ([`Dwell::session_started`]) the pointer moving beyond `away_px` (or off
//!   the page, or another pointer taking over) is [`Reaction::MovedAway`], reported once; a pen drawing
//!   (tip down) is judged by the small radius, since a stroke is a few pixels of travel. A fresh rest
//!   reported while the dictation runs is the *second dwell*, which the app takes as "stop".

/// Which pointers may start a dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allow {
    /// None: no dwell, no dictation.
    Off,
    /// A pen (in range of the tablet, or touching).
    Pen,
    /// A pen or the mouse.
    Any,
}

impl Allow {
    /// Every choice, in the order the settings page lists them.
    pub const ALL: [Self; 3] = [Self::Off, Self::Pen, Self::Any];

    /// The name in the settings file and on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Pen => "pen",
            Self::Any => "any",
        }
    }

    /// A choice by name, any case, blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(name))
    }

    /// Whether a pointer of this kind may rest into a dictation.
    #[must_use]
    pub const fn allows(self, pen: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Pen => pen,
            Self::Any => true,
        }
    }
}

/// The shortest rest a hand can mean (a shorter one is a pause in a stroke).
pub const MIN_REST_MS: u64 = 300;
/// The longest rest worth waiting for.
pub const MAX_REST_MS: u64 = 10_000;
/// The rest time when nothing says otherwise.
pub const DEFAULT_REST_MS: u64 = 1_200;
/// How far the pointer may wander and still be at rest, px.
pub const JITTER_PX: f32 = 8.0;
/// How far from the anchor a hovering pen may go while a dictation runs, px.
pub const AWAY_PX: f32 = 48.0;

/// A rest time inside what a hand can mean, and the word about it when it was changed (an input
/// is capped with a diagnostic, never refused).
#[must_use]
pub fn cap_rest_ms(ms: u64) -> (u64, Option<String>) {
    if ms < MIN_REST_MS {
        (
            MIN_REST_MS,
            Some(format!(
                "the rest time of {ms} ms is shorter than a hand can mean: using {MIN_REST_MS} ms"
            )),
        )
    } else if ms > MAX_REST_MS {
        (
            MAX_REST_MS,
            Some(format!(
                "the rest time of {ms} ms is longer than worth waiting for: using {MAX_REST_MS} ms"
            )),
        )
    } else {
        (ms, None)
    }
}

/// What the dwell is told about the user.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Which pointers may rest into a dictation.
    pub allow: Allow,
    /// How long one rest lasts before it is a dwell, ms.
    pub rest_ms: u64,
    /// How far the pointer may wander and still be at rest, px.
    pub jitter_px: f32,
    /// How far from the anchor the pen may go while a dictation runs, px.
    pub away_px: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow: Allow::Pen,
            rest_ms: DEFAULT_REST_MS,
            jitter_px: JITTER_PX,
            away_px: AWAY_PX,
        }
    }
}

/// One sample of the pointer over a page, in that page's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The page the pointer is over.
    pub page: usize,
    /// Where on the page, px.
    pub x: f32,
    pub y: f32,
    /// A pen (in range of the tablet or touching); `false`: a mouse.
    pub pen: bool,
    /// The pen's tip (or the mouse button) is down.
    pub contact: bool,
}

/// Where a rest was reported: on which page, where, and whether the tip was down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub contact: bool,
}

/// What a sample or a leave did to a running dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Nothing to end.
    Stay,
    /// The pen moved away from the anchor (or drew, or left): the dictation ends.
    MovedAway,
}

/// A rest being timed.
#[derive(Debug, Clone, Copy)]
struct Rest {
    /// Where it began.
    at: Sample,
    /// When it began, ms.
    since: u64,
    /// Already reported (a rest is one dwell).
    reported: bool,
}

/// The dwell of one window's pointer.
#[derive(Debug, Clone)]
pub struct Dwell {
    config: Config,
    rest: Option<Rest>,
    /// Where the running dictation was anchored.
    session: Option<Anchor>,
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

impl Dwell {
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            rest: None,
            session: None,
        }
    }

    /// New settings, in effect at once (a rest in progress keeps its start).
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        if !config.allow.allows(true) {
            self.rest = None;
        }
    }

    /// The settings now.
    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }

    /// The pointer is at `sample` at `now`. A move past the jitter radius begins a new rest there;
    /// the answer says whether a running dictation is left behind.
    pub fn observe(&mut self, sample: Sample, now: u64) -> Reaction {
        if !self.config.allow.allows(sample.pen) {
            // Another pointer than the one that may dwell: nothing rests, and a dictation the
            // pen started is left behind.
            self.rest = None;
            return self.leave_session();
        }
        let reaction = match self.session {
            Some(anchor) => {
                // A stroke (tip down) is a few pixels of travel; a hovering pen may drift.
                let radius = if sample.contact {
                    self.config.jitter_px
                } else {
                    self.config.away_px
                };
                if anchor.page != sample.page
                    || distance((anchor.x, anchor.y), (sample.x, sample.y)) > radius
                {
                    self.leave_session()
                } else {
                    Reaction::Stay
                }
            }
            None => Reaction::Stay,
        };
        let moved = self.rest.as_ref().is_none_or(|r| {
            r.at.page != sample.page
                || distance((r.at.x, r.at.y), (sample.x, sample.y)) > self.config.jitter_px
        });
        if moved {
            self.rest = Some(Rest {
                at: sample,
                since: now,
                reported: false,
            });
        }
        reaction
    }

    /// The clock is at `now`: the rest that has lasted `rest_ms`, once.
    pub fn tick(&mut self, now: u64) -> Option<Anchor> {
        let rest = self.rest.as_mut()?;
        if rest.reported || now < rest.since || now - rest.since < self.config.rest_ms {
            return None;
        }
        rest.reported = true;
        Some(Anchor {
            page: rest.at.page,
            x: rest.at.x,
            y: rest.at.y,
            contact: rest.at.contact,
        })
    }

    /// The pointer left the pages (off the window, off the page, out of the tablet's range).
    pub fn left(&mut self) -> Reaction {
        self.rest = None;
        self.leave_session()
    }

    /// A dictation started at `anchor`: from now on the pointer is judged against it.
    pub const fn session_started(&mut self, anchor: Anchor) {
        self.session = Some(anchor);
    }

    /// The dictation is over, whatever ended it.
    pub const fn session_ended(&mut self) {
        self.session = None;
    }

    /// Whether a rest is being timed (the app keeps its tick timer while this holds).
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.rest.as_ref().is_some_and(|r| !r.reported)
    }

    /// Whether the pointer still rests where `anchor` was reported (it has not moved on, nor left).
    #[must_use]
    pub fn still_at(&self, anchor: &Anchor) -> bool {
        self.rest.as_ref().is_some_and(|r| {
            r.at.page == anchor.page
                && distance((r.at.x, r.at.y), (anchor.x, anchor.y)) <= self.config.jitter_px
        })
    }

    fn leave_session(&mut self) -> Reaction {
        if self.session.take().is_some() {
            Reaction::MovedAway
        } else {
            Reaction::Stay
        }
    }
}

#[cfg(test)]
#[path = "dwell_test.rs"]
mod dwell_test;
