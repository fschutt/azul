//! The rate limits of enrollment, per PERSON and kind of opener: one pending request at a time and
//! three openings a day (planning/architecture.md section 7.5 "Rate limits per person, and an entry
//! in the signed log", section 1.4 rule 4; COMMON-W1R R13; build unit S17, lane W1-S17e).
//!
//! The plan gives no numbers: one pending request and three openings in any 24 hours, for each
//! kind of opener, are the estimates of the lane's brief (drift row of the lane). The registry's
//! structural cap of pending devices per member (S9c, `MAX_PENDING_PER_MEMBER`, three) must stay
//! above the limit of the person (R13: a glue test asserts that the hard cap is at least the rate
//! limit): with a budget for each of the four kinds of opener the limit of a person is four
//! pending requests, so the cap must be at least four (drift row of the lane).
//!
//! The ledger is keyed by the person the request is about, never by the device: a second request
//! for the same person is a second request whatever its new device is, and the people of an
//! account do not share a budget. Inside a person it is keyed by the KIND OF OPENER
//! ([`OpenerKind`]): a lower kind of opener never uses up the budget that a higher kind needs.
//! Nobody has verified anything of a person who opens from a new device, and a helpdesk member can
//! open a recovery request for any member with no evidence at all; if their openings shared a
//! budget with the admins' signed openings and the person's own approving device, they would starve
//! the escrow opening that two admins signed, a quorum enrollment, and the QR enrollment of a
//! person who has a working laptop. So the kinds have budgets of their own (at most four requests
//! of a person are pending at once, each of them visible and open to a veto).
//!
//! The ledger is part of the group's state, passed as `&mut OpenLimits` to `open`. A request that
//! is no longer open (cancelled, released or expired) frees its slot when the group tells the
//! ledger with [`OpenLimits::closed`]; a slot the group forgot to free frees itself when the
//! request would have expired anyway, so a lost call never blocks a person for ever.

use std::collections::BTreeMap;

use azlin_seams::{MemberId, Span, Stamp};

use crate::path::OpenerKind;
use crate::request::{Request, RequestId, RequestState};

/// The openings a person may have pending at once, for each kind of opener: one (estimate, drift
/// row of the lane).
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person") and COMMON-W1R R13.
///
/// Caller: `OpenLimits::standard`.
pub const STANDARD_PENDING_MAX: u8 = 1;

/// The openings a person may make in any 24 hours, for each kind of opener: three (estimate, drift
/// row of the lane).
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person") and COMMON-W1R R13.
///
/// Caller: `OpenLimits::standard`.
pub const STANDARD_PER_DAY: u8 = 3;

/// The window of the daily limit: a rolling 24 hours of the leader's stamp (no midnight at which
/// the count starts again).
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person").
///
/// Caller: `OpenLimits` (the daily count).
pub const DAILY_WINDOW: Span = Span::days(1);

/// The most ledgers one `OpenLimits` keeps (section 1.4 rule 4): one for each person and kind of
/// opener with a pending request or an opening in the last 24 hours; a ledger with neither is
/// forgotten, so this bounds the people with activity.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up
/// front).
///
/// Caller: `OpenLimits::admit`.
pub const MAX_LEDGERS: usize = 100_000;

/// Why a request may not be opened now.
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person").
///
/// Caller: `open` (as `OpenRefusal::Limit`). Seam: S9c/ENROLL-WIRE (maps it to `retry_after`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LimitRefusal {
    /// The person has the most requests of this kind of opener pending that the limit allows.
    #[error("the person has {pending} requests pending: the limit is {max}")]
    PendingLimit {
        /// The requests pending now.
        pending: usize,
        /// The most that may be pending.
        max: u8,
    },
    /// The person had the most requests of this kind of opener opened in the last 24 hours that
    /// the limit allows.
    #[error("the person opened {opened} requests in the last 24 hours: the limit is {max}")]
    DailyLimit {
        /// The openings in the last 24 hours.
        opened: usize,
        /// The most that may be opened.
        max: u8,
    },
    /// The ledger keeps the most ledgers it may.
    #[error("the ledger of enrollment limits is full")]
    LedgerFull,
}

/// A limit of zero would refuse every opening: no value.
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person") and section 1.4 rule
/// 4.
///
/// Caller: `OpenLimits::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a limit of zero would refuse every request")]
pub struct LimitsInvalid;

/// What the ledger knows about one pending request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Live {
    id: RequestId,
    expires_at: Stamp,
}

/// What the ledger knows about one person and one kind of opener.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Ledger {
    /// The stamps of the openings in the last 24 hours (older ones are dropped when read).
    opened: Vec<Stamp>,
    /// The requests that are pending.
    live: Vec<Live>,
    /// How many requests this ledger admitted: it makes the id of the next request differ from
    /// every earlier one, so that a cancel followed by an opening in the same second of the same
    /// device is another request. A forgotten ledger starts again at zero (it was idle for 24
    /// hours, so no request of the same second exists).
    next_seq: u64,
}

impl Ledger {
    /// Drops what no longer counts at `now`: openings 24 hours old or older, and requests past
    /// their expiry. A stamp after `now` (a clock that went back) still counts.
    fn prune(&mut self, now: Stamp) {
        self.opened
            .retain(|opened| match now.elapsed_since(*opened) {
                None => true,
                Some(age) => age < DAILY_WINDOW,
            });
        self.live.retain(|live| live.expires_at >= now);
    }

    fn is_idle(&self) -> bool {
        self.opened.is_empty() && self.live.is_empty()
    }
}

/// The ledger of openings, for each person and kind of opener, with its two limits.
///
/// Implements planning/architecture.md section 7.5 ("Rate limits per person") and COMMON-W1R R13
/// (this crate owns the user-facing numbers, the registry only its structural cap).
///
/// Caller: `open` (the one writer). Seam: S9c/ENROLL-WIRE (the group's state holds one, calls
/// `closed` after `cancel`, `mark_released` and `expire`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenLimits {
    pending_max: u8,
    per_day: u8,
    ledgers: BTreeMap<(MemberId, OpenerKind), Ledger>,
}

impl OpenLimits {
    /// A ledger with these limits; a limit of zero is refused.
    pub fn new(pending_max: u8, per_day: u8) -> Result<OpenLimits, LimitsInvalid> {
        if pending_max == 0 || per_day == 0 {
            return Err(LimitsInvalid);
        }
        Ok(OpenLimits {
            pending_max,
            per_day,
            ledgers: BTreeMap::new(),
        })
    }

    /// A ledger with the standard limits: one pending request and three openings a day.
    pub fn standard() -> OpenLimits {
        OpenLimits {
            pending_max: STANDARD_PENDING_MAX,
            per_day: STANDARD_PER_DAY,
            ledgers: BTreeMap::new(),
        }
    }

    /// The most requests of one person and kind of opener that may be pending.
    pub const fn pending_max(&self) -> u8 {
        self.pending_max
    }

    /// The most openings of one person and kind of opener in any 24 hours.
    pub const fn per_day(&self) -> u8 {
        self.per_day
    }

    /// The ledgers of a person, one for each kind of opener the person has one for.
    fn ledgers_of(&self, person: MemberId) -> impl Iterator<Item = &Ledger> + '_ {
        OpenerKind::ALL
            .into_iter()
            .filter_map(move |kind| self.ledgers.get(&(person, kind)))
    }

    /// How many requests of the person are pending at `now`, of every kind of opener.
    pub fn pending_of(&self, person: MemberId, now: Stamp) -> usize {
        self.ledgers_of(person)
            .map(|ledger| {
                ledger
                    .live
                    .iter()
                    .filter(|live| live.expires_at >= now)
                    .count()
            })
            .sum()
    }

    /// How many requests of the person that this kind of opener opened are pending at `now`.
    pub fn pending_of_kind(&self, person: MemberId, kind: OpenerKind, now: Stamp) -> usize {
        match self.ledgers.get(&(person, kind)) {
            Some(ledger) => ledger
                .live
                .iter()
                .filter(|live| live.expires_at >= now)
                .count(),
            None => 0,
        }
    }

    /// How many requests the person opened in the 24 hours up to `now`, of every kind of opener.
    pub fn opened_today(&self, person: MemberId, now: Stamp) -> usize {
        self.ledgers_of(person)
            .map(|ledger| {
                ledger
                    .opened
                    .iter()
                    .filter(|opened| match now.elapsed_since(**opened) {
                        None => true,
                        Some(age) => age < DAILY_WINDOW,
                    })
                    .count()
            })
            .sum()
    }

    /// How many ledgers the ledger keeps.
    pub fn tracked_ledgers(&self) -> usize {
        self.ledgers.len()
    }

    /// The number the next request of this person and kind of opener gets: the id of the request
    /// is made with it.
    pub fn peek_seq(&self, person: MemberId, kind: OpenerKind) -> u64 {
        match self.ledgers.get(&(person, kind)) {
            Some(ledger) => ledger.next_seq,
            None => 0,
        }
    }

    /// Admits the opening of request `id` for `person` by this kind of opener at `now` (the request
    /// expires at `expires_at`), or says which limit stops it. Nothing is written when it refuses.
    pub(crate) fn admit(
        &mut self,
        person: MemberId,
        kind: OpenerKind,
        id: RequestId,
        now: Stamp,
        expires_at: Stamp,
    ) -> Result<(), LimitRefusal> {
        let key = (person, kind);
        if !self.ledgers.contains_key(&key) {
            if self.ledgers.len() >= MAX_LEDGERS {
                self.forget_idle(now);
            }
            if self.ledgers.len() >= MAX_LEDGERS {
                return Err(LimitRefusal::LedgerFull);
            }
        }
        let pending_max = self.pending_max;
        let per_day = self.per_day;
        let ledger = self.ledgers.entry(key).or_default();
        ledger.prune(now);
        let verdict = if ledger.live.len() >= usize::from(pending_max) {
            Err(LimitRefusal::PendingLimit {
                pending: ledger.live.len(),
                max: pending_max,
            })
        } else if ledger.opened.len() >= usize::from(per_day) {
            Err(LimitRefusal::DailyLimit {
                opened: ledger.opened.len(),
                max: per_day,
            })
        } else {
            ledger.opened.push(now);
            ledger.live.push(Live { id, expires_at });
            ledger.next_seq = ledger.next_seq.saturating_add(1);
            Ok(())
        };
        if verdict.is_err() && ledger.is_idle() {
            self.ledgers.remove(&key);
        }
        verdict
    }

    /// Tells the ledger that `request` is no longer open (cancelled, released or expired): its
    /// slot is free. A request that is still open frees nothing, so the call cannot be used to
    /// get round the limit; calling it twice is harmless.
    pub fn closed(&mut self, request: &Request) {
        if request.state() == RequestState::Open {
            return;
        }
        let key = (request.person(), request.opened_by().kind());
        if let Some(ledger) = self.ledgers.get_mut(&key) {
            ledger.live.retain(|live| live.id != request.id());
        }
    }

    /// Forgets the ledgers with no pending request and no opening in the last 24 hours.
    pub fn forget_idle(&mut self, now: Stamp) {
        for ledger in self.ledgers.values_mut() {
            ledger.prune(now);
        }
        self.ledgers.retain(|_, ledger| !ledger.is_idle());
    }
}
