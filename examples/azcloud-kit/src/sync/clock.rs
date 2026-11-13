//! The clock an expiry is judged by (ARCH28 RF46 `TrustedClock`; AZ-S12).
//!
//! A pass expires two things by age: the trash folders of this device (30 days) and the
//! tombstones of the drive's index (90 days). Both used the wall clock of the computer that ran
//! the pass, and nothing else: a clock a year ahead - a dead battery, a virtual machine resumed,
//! a time zone typed wrong - found every trash folder "older than 30 days" and removed it at once,
//! the delete of a minute ago included, and its next commit dropped every tombstone of the shared
//! index (which stand between a stale copy and the files it would send again).
//!
//! No clock can be proved right from the computer alone, so an expiry is judged only on a clock
//! that has proved two things (the reading is [`TrustedClock::look`]):
//!
//! - **steady**: the pairing remembers the wall clock it last judged ([`ClockRecord`]); a clock
//!   that went back, or jumped more than [`MAX_GAP_SECS`] forward since then (an NTP step, a
//!   resumed machine, a long sleep), starts a new run, and only a clock that stayed steady for
//!   [`PROBATION_SECS`] since is believed. An expiry waits a day; nothing is lost by it. (A clock
//!   that is wrong from the start and stays wrong is believed after that day: nothing on this
//!   computer tells it from the right one - against a storage node the signed requests are
//!   refused long before, a signature carries the time.)
//! - **not behind the drive**: a drive that holds a commit dated after this clock's now has seen
//!   more time than this computer: the clock is wrong, and nothing expires.
//!
//! A reading that fails either is not a time: [`TrustedClock::purge_before`] answers `None`, and
//! the caller expires nothing this pass. The record is part of the pairing's base.
//!
//! **A second clock for what only the trash keeps** (X-DELTA-AZDRIVE-SYNC-8). A trash folder is
//! named by the wall clock of the pass that made it, and nothing proves that clock was right then:
//! after a reset to 2000-01-01 a "Take theirs" loser - a version that may exist nowhere else - lands
//! in `20000101-...`, and when the clock is fixed and has been steady for a day that name is "older
//! than 30 days". So the trash is purged only when two clocks agree: the date the folder is named by
//! AND the time this pairing has itself watched it lie there ([`TrustedClock::watched_secs`]: what
//! the wall clock advanced between looks, a step back or a jump counting for no more than
//! [`MAX_GAP_SECS`], summed per folder). A reset, a jump and a year ahead add days at most.

use serde::{Deserialize, Serialize};

/// How far a clock may lag behind a time that is on record (the drive's last commit, the last
/// look) before it counts as having gone back: the difference of two machines' clocks, an NTP
/// correction.
pub const SLACK_SECS: i64 = 3_600;
/// The longest step between two looks that is no jump: a laptop closed over a long weekend.
pub const MAX_GAP_SECS: i64 = 3 * 86_400;
/// How long a clock must have been steady before an expiry is judged by it.
pub const PROBATION_SECS: i64 = 86_400;

/// What a pairing remembers of the clock it judged by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockRecord {
    /// The wall clock at the last look, seconds since 1970 (0: never looked).
    #[serde(default)]
    pub last: i64,
    /// Where the steady run that last look belongs to began.
    #[serde(default)]
    pub steady_since: i64,
}

impl ClockRecord {
    /// Whether nothing was ever recorded (for the base's serialisation).
    #[must_use]
    pub fn is_unset(&self) -> bool {
        self.last == 0 && self.steady_since == 0
    }
}

/// One reading of the clock: a time an expiry may be judged by, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustedClock {
    now: Option<i64>,
    /// What the wall clock advanced since the look before, seconds: 0 at the first look and after a
    /// step back, at most [`MAX_GAP_SECS`] after a jump forward. The time a pairing has watched.
    watched: i64,
}

impl TrustedClock {
    /// Looks at the clock. `wall` is this computer's wall clock now, `drive` the newest time the
    /// drive's own record shows (0: it shows none), `record` what the pairing remembers: it is
    /// updated by the look.
    pub fn look(record: &mut ClockRecord, wall: i64, drive: i64) -> TrustedClock {
        if wall <= 0 {
            // The system could not say what time it is.
            return TrustedClock {
                now: None,
                watched: 0,
            };
        }
        let first = record.last == 0;
        let went_back = !first && wall + SLACK_SECS < record.last;
        let jumped = !first && wall - record.last > MAX_GAP_SECS;
        // What this look adds to the time the pairing has watched: the clock's own progress, a step
        // back adding nothing and a jump no more than the longest gap that is no jump.
        let watched = if first || went_back {
            0
        } else {
            (wall - record.last).clamp(0, MAX_GAP_SECS)
        };
        let none = TrustedClock { now: None, watched };
        if first || went_back || jumped {
            *record = ClockRecord {
                last: wall,
                steady_since: wall,
            };
            return none;
        }
        record.last = record.last.max(wall);
        let steady = wall - record.steady_since >= PROBATION_SECS;
        let behind_the_drive = drive > 0 && wall + SLACK_SECS < drive;
        if steady && !behind_the_drive {
            TrustedClock {
                now: Some(wall),
                watched,
            }
        } else {
            none
        }
    }

    /// The seconds this look adds to what a pairing has watched lie where it lies (the second clock
    /// of the trash: see the module's doc).
    #[must_use]
    pub fn watched_secs(&self) -> i64 {
        self.watched
    }

    /// The time before which something dated `days` ago is expired; `None` when the clock is not
    /// believed: nothing expires.
    #[must_use]
    pub fn purge_before(&self, days: i64) -> Option<i64> {
        self.now
            .map(|now| now.saturating_sub(days.saturating_mul(86_400)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_790_000_000;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;

    fn look(record: &mut ClockRecord, wall: i64, drive: i64) -> Option<i64> {
        TrustedClock::look(record, wall, drive).purge_before(30)
    }

    #[test]
    fn a_clock_is_believed_after_a_steady_day_and_not_before() {
        let mut record = ClockRecord::default();
        assert_eq!(
            look(&mut record, T0, 0),
            None,
            "the first look judges nothing"
        );
        assert_eq!(
            look(&mut record, T0 + 20 * HOUR, 0),
            None,
            "twenty hours of steadiness"
        );
        assert_eq!(
            look(&mut record, T0 + 40 * HOUR, 0),
            Some(T0 + 40 * HOUR - 30 * DAY),
            "a day and more"
        );
    }

    #[test]
    fn a_jump_forward_starts_over() {
        let mut record = ClockRecord::default();
        look(&mut record, T0, 0);
        look(&mut record, T0 + 2 * DAY, 0);
        assert!(
            look(&mut record, T0 + 3 * DAY, 0).is_some(),
            "steady for three days"
        );
        // A year ahead.
        assert_eq!(look(&mut record, T0 + 365 * DAY, 0), None);
        assert_eq!(
            look(&mut record, T0 + 365 * DAY + 20 * HOUR, 0),
            None,
            "on probation"
        );
        assert!(look(&mut record, T0 + 365 * DAY + 40 * HOUR, 0).is_some());
    }

    #[test]
    fn a_clock_that_went_back_starts_over() {
        let mut record = ClockRecord::default();
        look(&mut record, T0, 0);
        assert!(look(&mut record, T0 + 2 * DAY, 0).is_some());
        // A correction of a second or a minute is no step back; an hour and more is.
        assert!(look(&mut record, T0 + 2 * DAY - 60, 0).is_some());
        assert_eq!(look(&mut record, T0 + DAY, 0), None);
    }

    #[test]
    fn a_clock_behind_the_drives_record_is_not_believed() {
        let mut record = ClockRecord::default();
        look(&mut record, T0, 0);
        look(&mut record, T0 + 2 * DAY, 0);
        let drive_saw = T0 + 2 * DAY + 5 * HOUR;
        assert_eq!(look(&mut record, T0 + 2 * DAY, drive_saw), None);
        assert!(look(&mut record, T0 + 2 * DAY, T0 + 2 * DAY - 10).is_some());
    }

    #[test]
    fn a_clock_that_cannot_say_the_time_judges_nothing() {
        let mut record = ClockRecord {
            last: T0,
            steady_since: T0 - 10 * DAY,
        };
        assert_eq!(look(&mut record, 0, 0), None);
        assert_eq!(record.last, T0, "and the record is left as it was");
    }
}
