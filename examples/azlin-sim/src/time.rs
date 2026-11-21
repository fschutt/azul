//! Simulated clocks and the leader's stamp (planning/architecture.md section 1.4 rule 2: clock
//! skew and jumps; section 2.4 "time is the leader's stamp", checked).
//!
//! [`SimClock`] holds the global simulated time and one offset per clock (a node, or one of the
//! two follower voters of the metadata group): `skew` sets an offset, `jump` changes it. A clock
//! that would read before 1970 or that is switched off is a [`ClockError`], never the time 0.
//! [`Stamper`] is the leader's stamp rule (glue rule G13, drift row X-DELTA-W1SIM-8, provisional:
//! there is no pure implementation of 2.4 yet): the stamp is the leader's clock but never below its
//! previous stamp and never above the median of the voters' clocks, and when the voters are behind
//! the previous stamp the leader refuses to stamp.

use azlin_seams::{Clock, ClockError, Stamp, UnixTime};

/// A clock offset in whole seconds, forward or back.
///
/// Implements planning/architecture.md section 1.4 rule 2 (clock skew and jumps).
///
/// Caller: `Step::Skew`, `Step::Jump`, `SimClock`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Shift(i32);

impl Shift {
    /// No offset.
    pub const ZERO: Shift = Shift(0);

    /// The offset of `secs` seconds (negative: back).
    pub const fn from_secs(secs: i32) -> Shift {
        Shift(secs)
    }

    /// The offset in seconds.
    pub const fn secs(self) -> i32 {
        self.0
    }
}

/// The simulated time at the start of every run (2027-01-15): a time far from 1970 so that
/// large jumps back stay readable.
///
/// Implements planning/architecture.md section 1.4 rule 1 (the clock is a parameter).
///
/// Caller: `SimClock::new`, `world::World::new`.
pub const SIM_START: u64 = 1_800_000_000;

/// The largest offset a clock keeps, in seconds either way (a jump past it is clamped).
const MAX_OFFSET: i64 = 1 << 40;

/// The simulated clocks of a world: global time and one offset per clock.
///
/// Implements planning/architecture.md section 1.4 rule 2 (clock skew and jumps).
///
/// Caller: `world::World`, tests/s1_03.
#[derive(Clone, Debug)]
pub struct SimClock {
    now: u64,
    offsets: Vec<i64>,
    broken: Vec<bool>,
}

impl SimClock {
    /// `clocks` clocks, all at [`SIM_START`] with no offset.
    pub fn new(clocks: usize) -> SimClock {
        SimClock {
            now: SIM_START,
            offsets: vec![0; clocks],
            broken: vec![false; clocks],
        }
    }

    /// How many clocks there are.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Whether there is no clock.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// The global simulated time in seconds (the steady clock: skew never touches it).
    pub fn global(&self) -> u64 {
        self.now
    }

    /// Advances the global time (saturating).
    pub fn advance(&mut self, secs: u64) {
        self.now = self.now.saturating_add(secs);
    }

    /// Sets the offset of one clock (nothing for a clock that does not exist).
    pub fn skew(&mut self, clock: usize, shift: Shift) {
        if let Some(offset) = self.offsets.get_mut(clock) {
            *offset = i64::from(shift.secs());
        }
    }

    /// Adds to the offset of one clock (clamped; nothing for a clock that does not exist).
    pub fn jump(&mut self, clock: usize, shift: Shift) {
        if let Some(offset) = self.offsets.get_mut(clock) {
            *offset = offset
                .saturating_add(i64::from(shift.secs()))
                .clamp(-MAX_OFFSET, MAX_OFFSET);
        }
    }

    /// The offset of a clock.
    pub fn offset(&self, clock: usize) -> Option<i64> {
        self.offsets.get(clock).copied()
    }

    /// Switches a clock off (it reads `Unreadable`) or on again.
    pub fn set_unreadable(&mut self, clock: usize, unreadable: bool) {
        if let Some(flag) = self.broken.get_mut(clock) {
            *flag = unreadable;
        }
    }

    /// What a clock reads: the global time plus its offset; `Unreadable` for a clock that does
    /// not exist or is off, `BeforeEpoch` when the sum is negative.
    pub fn read(&self, clock: usize) -> Result<Stamp, ClockError> {
        let (Some(offset), Some(broken)) = (self.offsets.get(clock), self.broken.get(clock)) else {
            return Err(ClockError::Unreadable);
        };
        if *broken {
            return Err(ClockError::Unreadable);
        }
        let sum = i128::from(self.now) + i128::from(*offset);
        if sum < 0 {
            return Err(ClockError::BeforeEpoch);
        }
        match u64::try_from(sum) {
            Ok(secs) => Ok(UnixTime::from_secs(secs)),
            Err(_) => Err(ClockError::Unreadable),
        }
    }

    /// The clock of one node as an `azlin_keys::Clock`.
    pub fn view(&self, clock: usize) -> NodeClock<'_> {
        NodeClock {
            clocks: self,
            clock,
        }
    }
}

/// One clock of a [`SimClock`], as the `Clock` seam of the engine's crates.
///
/// Implements planning/architecture.md section 1.4 rule 1 (the clock is an interface).
///
/// Caller: tests/s1_03, `world::World`.
#[derive(Clone, Copy, Debug)]
pub struct NodeClock<'a> {
    clocks: &'a SimClock,
    clock: usize,
}

impl Clock for NodeClock<'_> {
    fn now(&self) -> Result<UnixTime, ClockError> {
        self.clocks.read(self.clock)
    }
}

/// Why the leader cannot stamp an entry now.
///
/// Implements planning/architecture.md section 2.4 (the stamp is checked against the voters).
///
/// Caller: `Stamper::stamp`, `leader`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StampRefusal {
    /// The leader's own clock cannot be read.
    ClockUnreadable(ClockError),
    /// Fewer than two of the three voters' clocks can be read: there is no median.
    VotersUnreadable,
    /// The median of the voters is behind the previous stamp: stamping would run backwards or
    /// ahead of the voters.
    VotersBehind,
}

error_text!(StampRefusal {
    StampRefusal::ClockUnreadable(error) => "the leader's clock cannot be read: {error}",
    StampRefusal::VotersUnreadable => "no majority of voter clocks can be read",
    StampRefusal::VotersBehind => "the voters are behind the previous stamp",
});

/// The median of the voters' clocks: the middle one of three, the smaller of two readable ones,
/// nothing when fewer than two can be read.
///
/// Implements planning/architecture.md section 2.4 ("nor past the median of the voters' clocks").
///
/// Caller: `Stamper::stamp`, tests/s1_03.
pub fn median(voters: &[Result<Stamp, ClockError>; 3]) -> Option<Stamp> {
    let mut readable: Vec<Stamp> = voters
        .iter()
        .filter_map(|v| v.as_ref().ok().copied())
        .collect();
    readable.sort();
    match readable.len() {
        3 => readable.get(1).copied(),
        2 => readable.first().copied(),
        _ => None,
    }
}

/// The leader's stamp rule (glue rule G13): `max(its clock, the previous stamp)`, at most the median
/// of the three voters' clocks; when the median is behind the previous stamp the leader refuses.
/// The output never decreases and never exceeds the median.
///
/// Implements planning/architecture.md section 2.4 (time is the leader's stamp, checked).
///
/// Caller: `leader::Leader`, tests/s1_03.
#[derive(Clone, Debug, Default)]
pub struct Stamper {
    last: Option<Stamp>,
}

impl Stamper {
    /// A stamper that has stamped nothing.
    pub const fn new() -> Stamper {
        Stamper { last: None }
    }

    /// A stamper that continues from a stamp it knows (rebuilt from the durable chain).
    pub const fn resume(last: Option<Stamp>) -> Stamper {
        Stamper { last }
    }

    /// The previous stamp.
    pub const fn last(&self) -> Option<Stamp> {
        self.last
    }

    /// Records a stamp that was made elsewhere (a restore record's stamp): never lowers it.
    pub fn observe(&mut self, at: Stamp) {
        self.last = Some(match self.last {
            Some(last) if last > at => last,
            Some(_) | None => at,
        });
    }

    /// The next stamp from the leader's clock and the three voters' clocks.
    pub fn stamp(
        &mut self,
        wall: Result<Stamp, ClockError>,
        voters: &[Result<Stamp, ClockError>; 3],
    ) -> Result<Stamp, StampRefusal> {
        let wall = wall.map_err(StampRefusal::ClockUnreadable)?;
        let median = median(voters).ok_or(StampRefusal::VotersUnreadable)?;
        let want = match self.last {
            Some(last) if last > wall => last,
            Some(_) | None => wall,
        };
        let stamp = if want <= median {
            want
        } else if self.last.is_none_or(|last| last <= median) {
            median
        } else {
            return Err(StampRefusal::VotersBehind);
        };
        self.last = Some(stamp);
        Ok(stamp)
    }

    /// The stamp of a BUGGY leader (`LeaderVariant::BuggyStamp`): its raw clock, with no rule at all.
    pub fn raw(&mut self, wall: Result<Stamp, ClockError>) -> Result<Stamp, StampRefusal> {
        let wall = wall.map_err(StampRefusal::ClockUnreadable)?;
        self.last = Some(wall);
        Ok(wall)
    }
}
