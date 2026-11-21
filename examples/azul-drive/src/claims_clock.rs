//! When the window's timer starts the next run of the background claims (X-DELTA-MONEY5-4,
//! X-DELTA-MONEY6-3, -4, -5): a small value the window holds and a test drives, with no
//! `DriveState` and no window.
//!
//! A run of [`crate::jobs::claim_run`] ends with a word for what it left with a pace of its own - a
//! receipt awaited, a payment taken back: `again_in`, the seconds after which the next run is due
//! (`None`: nothing is left to ask later). The period timer asks [`ClaimsClock::due`] at each tick.
//!
//! Two clocks, each compared with its own stamp and never with the other (two clocks never meet in
//! one comparison): the run's word is `again_in` seconds on the wall clock AND `again_in` seconds on
//! a clock that never steps ([`mono_secs`]), and the run is due when EITHER has passed. A laptop
//! that sleeps for three days is due at its first tick after the wake (the wall clock; a clock that
//! stops while the machine sleeps would count none of it), and one whose wall clock was set a year
//! ahead at its start and stepped back by the network time is due after `again_in` seconds all the
//! same (the other clock; an absolute wall time a year ahead would have postponed the next ask by a
//! year, X-DELTA-MONEY6-5).

use std::{sync::OnceLock, time::Instant};

use azul_appkit::l10n::Text;

/// Seconds on a clock that never steps back or forward: since this process first asked.
#[must_use]
pub(crate) fn mono_secs() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs()
}

/// What a run that ended asked for: the next run `in_secs` after `wall` on the wall clock (seconds
/// since 1970) and `in_secs` after `mono` on [`mono_secs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Armed {
    in_secs: u64,
    wall: u64,
    mono: u64,
}

/// The window's side of the background claims: whether a run goes, when the last one asked for the
/// next, and whether its problem was said already.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClaimsClock {
    claiming: bool,
    armed: Option<Armed>,
    /// The last run that ended had a problem the window said: a streak of failing runs says it
    /// once (the timer tries again every ten minutes while a keychain stays locked).
    warned: bool,
}

impl ClaimsClock {
    /// A run goes.
    #[must_use]
    pub(crate) fn running(&self) -> bool {
        self.claiming
    }

    /// A run starts now (the app's start, a purchase, the timer): `false` when one goes already
    /// and nothing starts. The word of the last run is spent: this run decides when the next is
    /// due.
    #[must_use]
    pub(crate) fn start(&mut self) -> bool {
        if self.claiming {
            return false;
        }
        self.claiming = true;
        self.armed = None;
        true
    }

    /// The run ended with `problem` and the word `again_in` (seconds; `None`: nothing is left to ask
    /// later), at `wall` seconds since 1970 and `mono` on [`mono_secs`]. The problem to say to the
    /// user, once per streak of runs that had one: a run that ended well makes the next say it
    /// again.
    #[must_use]
    pub(crate) fn done(
        &mut self,
        problem: Option<Text>,
        again_in: Option<u64>,
        wall: u64,
        mono: u64,
    ) -> Option<Text> {
        self.claiming = false;
        self.armed = again_in.map(|in_secs| Armed {
            in_secs,
            wall,
            mono,
        });
        // a run that ended without a problem ends the streak
        let had = problem.is_some();
        let say = problem.filter(|_| !self.warned);
        self.warned = had;
        say
    }

    /// The seconds the last run named for the next one, `None` once a run started or when it named
    /// none: what a test reads to see that a start spent the word.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn named(&self) -> Option<u64> {
        self.armed.map(|a| a.in_secs)
    }

    /// Whether the timer starts a run at `wall` and `mono`: none goes, the last one named a time,
    /// and that many seconds have passed on either clock since it ended.
    #[must_use]
    pub(crate) fn due(&self, wall: u64, mono: u64) -> bool {
        !self.claiming
            && self.armed.is_some_and(|a| {
                wall >= a.wall.saturating_add(a.in_secs) || mono >= a.mono.saturating_add(a.in_secs)
            })
    }
}
