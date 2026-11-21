//! The miss budget: "**Bots guessing link ids**: misses at `link_resolve` are budgeted per credential
//! and, at the bridge, per client address in RAM only (R2); a client with many misses gets Retry and
//! slows down" (planning/architecture.md section 5.12; the rules R1 and R2 of section 5.15).
//!
//! A miss is a `link_resolve` for an id the node does not know (or one it answers `NotFound`): a
//! guess. Each client key has a budget of misses that comes back with time (a token bucket in whole
//! seconds, no floating point: two replicas, or a replay, give the same answers):
//!
//! - the budget holds `capacity` misses, and one miss comes back every `refill_every`;
//! - the node asks `MissTable::admit` BEFORE it looks the id up, and answers `retry_after` (class
//!   After, section 2.5) without a lookup while the verdict is `Retry`, so a client that has used its
//!   budget cannot test one more id;
//! - when the lookup fails it calls `MissTable::record_miss`.
//!
//! **No address is held here.** A `BudgetKey` is 16 opaque bytes: the node keys a client that has a
//! credential by the credential, and the bridge keys the rest by `HMAC(daily key, address)` with the
//! key in memory only (R1: "no client IP address, prefix or unkeyed hash of one is written ... in any
//! layer"). The crate has no address type, and the table lives in RAM only: it has no `Clone` into a
//! file and no encoder (R2: "in memory an address serves only connection state, XDP and nftables
//! budgets (LRU maps with TTLs of seconds to minutes) and rate limits. None is exported").
//!
//! **The table is bounded** (section 1.4 rule 4): at `max_keys` keys the oldest key (the one whose
//! last miss lies furthest back) is forgotten to make room for a new one. That is a stated limit: a
//! flood of fresh keys can make the table forget a slow guesser, and what it costs the flooder is a
//! key per forgotten key (one HMAC'd address each).
//!
//! Implements planning/architecture.md section 5.12 ("Bots guessing link ids"), section 5.15 (R1 and
//! R2) and section 1.4 (rule 4, limits).

use core::fmt;
use std::collections::BTreeMap;

use azlin_seams::{Span, Stamp};

/// The misses a client key may make in a burst before it is slowed. An estimate: the plan gives no
/// number ("a client with many misses gets Retry"; drift register of lane W1-S16c2).
///
/// Implements planning/architecture.md section 5.12 ("Bots guessing link ids") and section 1.4 (rule
/// 4, limits).
///
/// Seam: LINKS-NODE (it builds its `MissBudget` from it).
pub const MISS_CAPACITY: u32 = 20;

/// The time in which one miss comes back to a slowed client key: 20 misses an hour, for a link id of
/// about 2^60 values. An estimate (drift register of lane W1-S16c2).
///
/// Implements planning/architecture.md section 5.12 ("Bots guessing link ids") and section 1.4 (rule
/// 4, limits).
///
/// Seam: LINKS-NODE (it builds its `MissBudget` from it).
pub const MISS_REFILL: Span = Span::minutes(3);

/// The most client keys a table remembers. An estimate (drift register of lane W1-S16c2): 65,536
/// keys of a few dozen bytes each are a few MiB of RAM.
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.15 (R2).
///
/// Seam: LINKS-NODE (it builds its `MissTable` from it).
pub const MISS_TABLE_KEYS: usize = 65_536;

/// The key of one client in a `MissTable`: 16 opaque bytes, made by the node or the bridge (the hash
/// of a credential, or `HMAC(daily key, address)`). It prints as nothing, and the crate can neither
/// make one from an address nor turn one back into anything.
///
/// Implements planning/architecture.md section 5.12 ("per credential and, at the bridge, per client
/// address in RAM only (R2)") and section 5.15 (R1, R2).
///
/// Caller: `MissTable`; Seam: LINKS-NODE and the bridge of `drive.azlin.io` (they derive the key).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BudgetKey([u8; 16]);

impl BudgetKey {
    /// The key with these bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> BudgetKey {
        BudgetKey(bytes)
    }

    /// The bytes of the key.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for BudgetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BudgetKey(..)")
    }
}

/// Why a budget or a table cannot be made.
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits).
///
/// Caller: `MissBudget::new`, `MissTable::new`; Seam: LINKS-NODE (a configuration error at start).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MissError {
    /// A budget of no misses would slow every client from its first request.
    #[error("a miss budget holds at least one miss")]
    NoCapacity,
    /// A miss that comes back after no time at all is no budget.
    #[error("a miss comes back after at least one second")]
    NoRefill,
    /// The capacity times the time of one miss does not fit a count of seconds.
    #[error("the miss budget is too large to count")]
    TooLarge,
    /// A table that holds no key remembers nothing.
    #[error("a table holds at least one key")]
    NoKeys,
}

/// How many misses a client key may make and how fast they come back.
///
/// Implements planning/architecture.md section 5.12 ("Bots guessing link ids").
///
/// Caller: `MissTable`; Seam: LINKS-NODE (it chooses the numbers: `MISS_CAPACITY` and
/// `MISS_REFILL` are the estimates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissBudget {
    capacity: u32,
    refill_secs: u64,
    full_secs: u64,
}

impl MissBudget {
    /// A budget of `capacity` misses of which one comes back every `refill_every`; an error for no
    /// capacity, for no time, and for numbers whose product does not fit.
    pub fn new(capacity: u32, refill_every: Span) -> Result<MissBudget, MissError> {
        if capacity == 0 {
            return Err(MissError::NoCapacity);
        }
        if refill_every == Span::ZERO {
            return Err(MissError::NoRefill);
        }
        match refill_every.checked_mul(capacity) {
            Some(full) => Ok(MissBudget {
                capacity,
                refill_secs: refill_every.as_secs(),
                full_secs: full.as_secs(),
            }),
            None => Err(MissError::TooLarge),
        }
    }

    /// The misses a key may make in a burst.
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// The time in which one miss comes back.
    pub const fn refill_every(&self) -> Span {
        Span::secs(self.refill_secs)
    }
}

/// The answer of the table: the client may go on, or it is slowed and told when to come back.
///
/// Implements planning/architecture.md section 5.12 ("a client with many misses gets Retry and slows
/// down") and section 2.5 (the status `retry_after`, class After).
///
/// Caller: `MissTable::admit`, `MissTable::record_miss`; Seam: LINKS-NODE (`Retry` becomes the status
/// `retry_after` with the delay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissVerdict {
    /// The client has a miss to spare.
    Proceed,
    /// The client has used its budget: answer `retry_after` with this delay, without a lookup. The
    /// delay is at least one second.
    Retry {
        /// How long until one miss has come back.
        after: Span,
    },
}

/// What the table keeps of one client key.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// The seconds of credit the key had at `at`: a miss costs `refill_secs`, at most `full_secs`.
    credit: u64,
    /// The stamp of the last miss (never moves back).
    at: Stamp,
    /// The position of the last miss in `MissTable::order`.
    tick: u64,
}

/// The misses of many clients, in RAM, bounded: at most `max_keys` keys, the oldest forgotten first.
///
/// Implements planning/architecture.md section 5.12 ("Bots guessing link ids"), section 5.15 (R2) and
/// section 1.4 (rule 4).
///
/// Seam: LINKS-NODE (one table keyed by credentials), the bridge of `drive.azlin.io` (one
/// keyed by HMAC'd client addresses), S47d (the escalation tests drive it).
#[derive(Clone, Debug)]
pub struct MissTable {
    budget: MissBudget,
    max_keys: usize,
    entries: BTreeMap<BudgetKey, Entry>,
    /// The keys by the position of their last miss: the first one is the oldest.
    order: BTreeMap<u64, BudgetKey>,
    tick: u64,
}

/// The later of two stamps.
fn later(a: Stamp, b: Stamp) -> Stamp {
    a.max(b)
}

impl MissTable {
    /// An empty table for `budget` that holds at most `max_keys` keys; an error for no key.
    pub fn new(budget: MissBudget, max_keys: usize) -> Result<MissTable, MissError> {
        if max_keys == 0 {
            return Err(MissError::NoKeys);
        }
        Ok(MissTable {
            budget,
            max_keys,
            entries: BTreeMap::new(),
            order: BTreeMap::new(),
            tick: 0,
        })
    }

    /// The budget every key has.
    pub const fn budget(&self) -> MissBudget {
        self.budget
    }

    /// The most keys the table remembers.
    pub const fn max_keys(&self) -> usize {
        self.max_keys
    }

    /// The keys the table remembers now (never more than `max_keys`).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table remembers no key.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The credit the entry has at `now`: what it kept, plus the time that has passed, at most a full
    /// budget. A stamp before the entry's own earns nothing.
    fn credit_at(&self, entry: &Entry, now: Stamp) -> u64 {
        let gained = match now.elapsed_since(entry.at) {
            Some(span) => span.as_secs(),
            None => 0,
        };
        entry
            .credit
            .saturating_add(gained)
            .min(self.budget.full_secs)
    }

    /// The verdict for a key that has `credit` seconds of credit: it goes on when one miss is
    /// covered.
    fn verdict(&self, credit: u64) -> MissVerdict {
        if credit >= self.budget.refill_secs {
            MissVerdict::Proceed
        } else {
            MissVerdict::Retry {
                after: Span::secs(self.budget.refill_secs - credit),
            }
        }
    }

    /// Whether the client of `key` may make a request now: `Proceed` for a key the table does not
    /// know and for one with a miss to spare, `Retry` for one that has used its budget. It changes
    /// nothing: a request is charged only when it misses.
    ///
    /// Implements planning/architecture.md section 5.12 ("a client with many misses gets Retry and
    /// slows down").
    ///
    /// Seam: LINKS-NODE (before the lookup of every `link_resolve`).
    pub fn admit(&self, key: BudgetKey, now: Stamp) -> MissVerdict {
        match self.entries.get(&key) {
            Some(entry) => self.verdict(self.credit_at(entry, now)),
            None => MissVerdict::Proceed,
        }
    }

    /// Charges one miss to `key` at `now` and answers whether the client may go on. A key the table
    /// does not know starts with a full budget; at `max_keys` keys the oldest one is forgotten to make
    /// room.
    ///
    /// Implements planning/architecture.md section 5.12 ("Bots guessing link ids": "misses at
    /// `link_resolve` are budgeted") and section 1.4 (rule 4).
    ///
    /// Seam: LINKS-NODE (when the lookup of a `link_resolve` finds no link).
    pub fn record_miss(&mut self, key: BudgetKey, now: Stamp) -> MissVerdict {
        let (credit, at) = match self.entries.get(&key) {
            Some(entry) => (self.credit_at(entry, now), later(entry.at, now)),
            None => (self.budget.full_secs, now),
        };
        let left = if azlin_seams::mutant!("links_miss_budget_unbounded") {
            credit
        } else {
            credit.saturating_sub(self.budget.refill_secs)
        };
        self.store(key, left, at);
        self.verdict(left)
    }

    /// Puts the entry of `key` into the table as the newest one, forgetting the oldest key when a new
    /// key finds the table full.
    fn store(&mut self, key: BudgetKey, credit: u64, at: Stamp) {
        let old_tick = self.entries.get(&key).map(|entry| entry.tick);
        match old_tick {
            Some(tick) => {
                self.order.remove(&tick);
            }
            None => {
                if self.entries.len() >= self.max_keys
                    && !azlin_seams::mutant!("links_miss_table_unbounded")
                {
                    if let Some((_, oldest)) = self.order.pop_first() {
                        self.entries.remove(&oldest);
                    }
                }
            }
        }
        // 2^64 misses are out of reach of any process: the counter saturates instead of wrapping.
        self.tick = self.tick.saturating_add(1);
        self.entries.insert(
            key,
            Entry {
                credit,
                at,
                tick: self.tick,
            },
        );
        self.order.insert(self.tick, key);
    }
}
