//! The microphone's standby: when the window asks the speech session to open the microphone ahead of a dictation, and
//! when to close it again.
//!
//! A dictation starts when the pen has rested for 1.2 s; opening the microphone takes about a third of a second. Opened
//! as the pen comes near a page, the microphone is already open when the rest ends: the dictation starts at once and has
//! the words spoken just before it. The price is the system's microphone indicator, which is on while it is open - so
//! this is the user's choice (`Standby`) and the window closes the microphone as soon as the pen has gone.
//!
//! This is the policy, with no window, no session and no clock of its own: the pen's events and the time go in, "open it"
//! or "close it" comes out (`Action`), and `voice.rs` asks the session. Nothing here opens a microphone.

/// How the window keeps the microphone ready for a dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Standby {
    /// Never ahead of time: the microphone opens when the dictation starts, a third of a second late.
    Off,
    /// While the pen is near: it opens as the pen comes near a page and closes when the pen has been away for a while.
    #[default]
    Near,
    /// As long as the window is open: the system's microphone indicator stays on.
    Always,
}

impl Standby {
    /// Every choice, in the order the settings page lists them.
    pub const ALL: [Self; 3] = [Self::Off, Self::Near, Self::Always];

    /// The name in the settings file and on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Near => "near",
            Self::Always => "always",
        }
    }

    /// A choice by name, any case, blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name))
    }
}

/// The shortest idle time worth a setting: shorter than the pen's pauses between two strokes.
pub const MIN_IDLE_MS: u64 = 5_000;
/// The longest: an hour of a microphone that nobody uses is a forgotten one.
pub const MAX_IDLE_MS: u64 = 3_600_000;
/// The idle time when nothing says otherwise.
pub const DEFAULT_IDLE_MS: u64 = 30_000;
/// How long a pen that left the tablet's range may be away before the microphone is closed: long enough for a pen that
/// is lifted and put down again, short enough that the indicator does not outlast the pen.
pub const AWAY_GRACE_MS: u64 = 3_000;

/// An idle time inside what a person can mean, and the word about it when it was changed.
#[must_use]
pub fn cap_idle_ms(ms: u64) -> (u64, Option<String>) {
    if ms < MIN_IDLE_MS {
        (
            MIN_IDLE_MS,
            Some(format!(
                "the idle time of {ms} ms before the microphone's closing is shorter than the pauses of a pen: using {MIN_IDLE_MS} ms"
            )),
        )
    } else if ms > MAX_IDLE_MS {
        (
            MAX_IDLE_MS,
            Some(format!(
                "the idle time of {ms} ms before the microphone's closing is longer than a forgotten microphone: using {MAX_IDLE_MS} ms"
            )),
        )
    } else {
        (ms, None)
    }
}

/// What the window is to ask of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open the microphone and keep the last moments.
    Arm,
    /// Close it.
    Disarm,
}

impl Action {
    /// The word a script reads (`AZREVIEW_STANDBY <name>`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Arm => "armed",
            Self::Disarm => "disarmed",
        }
    }
}

/// What the policy is told about the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub standby: Standby,
    /// How long the pen may stay away, still or out of the window, before the microphone is closed, ms.
    pub idle_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            standby: Standby::default(),
            idle_ms: DEFAULT_IDLE_MS,
        }
    }
}

/// The policy: what was asked of the session last, and what has happened to the pen since. Every call returns what is to
/// be asked now (`None`: nothing), so the microphone is opened and closed once and not on every move of the pen.
#[derive(Debug, Clone)]
pub struct Arming {
    cfg: Config,
    /// The session was asked to open the microphone and not yet to close it.
    armed: bool,
    /// When the pen was near last (`None`: not since the microphone was closed).
    last_near: Option<u64>,
    /// When the pen left the tablet's range (`None`: it is here).
    away_since: Option<u64>,
    /// The window is no longer in front (until the pen is near again).
    forced: bool,
    /// A dictation uses the session: it is neither opened nor closed under it.
    busy: bool,
}

impl Arming {
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            armed: false,
            last_near: None,
            away_since: None,
            forced: false,
            busy: false,
        }
    }

    #[must_use]
    pub fn config(&self) -> Config {
        self.cfg
    }

    /// Whether the session was asked to keep the microphone open.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// The window opened: "always" opens the microphone now.
    pub fn start(&mut self, now: u64) -> Option<Action> {
        self.settle(now)
    }

    /// The pen is near a page: it came into range, or it moved over one. The idle time counts from here.
    pub fn near(&mut self, now: u64) -> Option<Action> {
        if self.cfg.standby == Standby::Near {
            self.last_near = Some(now);
        }
        self.away_since = None;
        self.forced = false;
        self.settle(now)
    }

    /// The pen left the tablet's range. The microphone closes after the grace (or the idle time, if that is sooner)
    /// unless the pen is back.
    pub fn out_of_range(&mut self, now: u64) {
        if self.away_since.is_none() {
            self.away_since = Some(now);
        }
    }

    /// The window is no longer in front: the microphone closes at once (the pen coming near again opens it).
    pub fn deactivated(&mut self, now: u64) -> Option<Action> {
        self.forced = true;
        self.settle(now)
    }

    /// A dictation begins or ends. While one runs the session is in use: nothing is opened or closed under it, and
    /// when it ends the idle time counts from there.
    pub fn busy(&mut self, busy: bool, now: u64) -> Option<Action> {
        if self.busy && !busy && self.armed {
            self.last_near = Some(now);
        }
        self.busy = busy;
        self.settle(now)
    }

    /// The time passed: a pen that has been away long enough closes the microphone.
    pub fn tick(&mut self, now: u64) -> Option<Action> {
        self.settle(now)
    }

    /// The user chose another standby or idle time: it applies at once.
    pub fn set_config(&mut self, cfg: Config, now: u64) -> Option<Action> {
        if cfg.standby == Standby::Near && self.cfg.standby != Standby::Near && self.armed {
            // Open already (it was "always"): it closes after the idle time, unless the pen moves.
            self.last_near = Some(now);
        }
        self.cfg = cfg;
        self.settle(now)
    }

    /// When "near" stops wanting the microphone open: the idle time after the pen was last near, or the grace after it
    /// left the tablet's range if that is sooner.
    fn deadline(&self, last_near: u64) -> u64 {
        let idle = last_near.saturating_add(self.cfg.idle_ms);
        match self.away_since {
            Some(away) => idle.min(away.saturating_add(AWAY_GRACE_MS)),
            None => idle,
        }
    }

    /// Whether the microphone should be open now.
    fn wanted(&self, now: u64) -> bool {
        match self.cfg.standby {
            Standby::Off => false,
            Standby::Always => true,
            Standby::Near => !self.forced && self.last_near.is_some_and(|near| now < self.deadline(near)),
        }
    }

    /// What is to be asked of the session now, for what the pen and the time have come to.
    fn settle(&mut self, now: u64) -> Option<Action> {
        if self.busy {
            return None;
        }
        match (self.armed, self.wanted(now)) {
            (false, true) => {
                self.armed = true;
                Some(Action::Arm)
            }
            (true, false) => {
                self.armed = false;
                self.last_near = None;
                self.away_since = None;
                Some(Action::Disarm)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "standby_test.rs"]
mod standby_test;
