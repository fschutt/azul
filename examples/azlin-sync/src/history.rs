//! The history of a head register: `head_history` (a bounded, resumable page, oldest first) and the
//! retention prune (planning/architecture.md section 5.13 `head_history`, section 5.8 "Retention per
//! space comes from the entitlement: 14, 60, 90 or 365 days ... The CAS register keeps every version
//! under a lock", section 2.16 `heads`; build unit S16, lane W1-S16a).
//!
//! A page holds at most [`HISTORY_PAGE_MAX`] records and at most [`HISTORY_PAGE_BYTES`] bytes
//! (DRIFT X-DELTA-W1S16A-5: the plan gives no paging; 256 records or 1 MiB are the brief's estimate),
//! and always at least one record when there is one, so a reader that follows `next` always makes
//! progress. A prune drops what is older than the retention and keeps the current head, every version
//! a lock still holds, and everything while the space is under a legal hold.

use azlin_seams::{mutant, ByteCount, Span, Stamp};

use crate::cas::HeadRegister;
use crate::head::HeadRecord;
use crate::version::Version;

/// The most records of one history page.
///
/// Implements planning/architecture.md section 1.4 rule 4 (a reply has a limit) and section 5.13
/// (`head_history`); an estimate (DRIFT X-DELTA-W1S16A-5).
///
/// Caller: `HeadRegister::history`.
pub const HISTORY_PAGE_MAX: u16 = 256;

/// The most bytes of one history page (the records' sizes, `HeadRecord::size`).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.13 (`head_history`); an
/// estimate (DRIFT X-DELTA-W1S16A-5).
///
/// Caller: `HeadRegister::history`.
pub const HISTORY_PAGE_BYTES: ByteCount = ByteCount::mib(1);

/// One page of a register's history: records oldest first, with the version to continue from and
/// the extent of what the register holds, so a reader can tell "no such version" from "pruned".
///
/// Implements planning/architecture.md section 5.13 (`head_history`) and section 2.5 (the client
/// first reads whether a `head_cas` landed).
///
/// Caller: `HeadRegister::history`; `resolve_lost_cas` (the client's decision after a lost reply);
/// SYNC-CLIENT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPage {
    /// The records of the page, oldest first (versions strictly ascending; a gap is a pruned
    /// version).
    pub records: Vec<HeadRecord>,
    /// The version to ask for next: the first record after this page; `None` when the page reaches
    /// the head.
    pub next: Option<Version>,
    /// The oldest version the register still holds; `None` for an empty register.
    pub oldest: Option<Version>,
    /// The current head's version; `None` for an empty register.
    pub newest: Option<Version>,
}

/// What a prune dropped.
///
/// Implements planning/architecture.md section 5.8 (retention; a version under a lock is kept) and
/// section 2.16 (`heads`: the rows to delete).
///
/// Caller: `HeadRegister::prune`; SYNC-APPLY deletes the rows of `removed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pruned {
    /// The versions dropped, ascending.
    pub removed: Vec<Version>,
    /// What they cost (`HeadRecord::size`).
    pub freed: ByteCount,
    /// How many versions were past their retention but kept because a lock still holds them.
    pub kept_locked: usize,
    /// Whether the space is under a legal hold, which dropped nothing.
    pub held: bool,
}

impl HeadRegister {
    /// A page of history from `from` (inclusive): the records with a version at or above it, oldest
    /// first, at most `limit` of them (and never more than [`HISTORY_PAGE_MAX`] records or
    /// [`HISTORY_PAGE_BYTES`] bytes; a `limit` of 0 counts as 1, so that a reader always advances).
    /// A `from` below the oldest version the register holds starts at the oldest.
    pub fn history(&self, from: Version, limit: u16) -> HistoryPage {
        let limit = if mutant!("sync_history_page_unbounded") {
            usize::from(limit)
        } else {
            usize::from(limit.clamp(1, HISTORY_PAGE_MAX))
        };
        let mut records: Vec<HeadRecord> = Vec::new();
        let mut bytes = ByteCount::ZERO;
        let mut next = None;
        for (version, record) in self.records.range(from..) {
            let size = record.size();
            let over_bytes = !records.is_empty()
                && bytes.saturating_add(size) > HISTORY_PAGE_BYTES
                && !mutant!("sync_history_bytes_unbounded");
            if records.len() >= limit || over_bytes {
                next = Some(*version);
                break;
            }
            bytes = bytes.saturating_add(size);
            records.push(record.clone());
        }
        HistoryPage {
            records,
            next,
            oldest: self.records.keys().next().copied(),
            newest: self.records.keys().next_back().copied(),
        }
    }

    /// Drops the versions older than `retention` at `now`, never the current head, never a version a
    /// lock still holds (its `retain_until` is after `now`) and nothing at all while the space is
    /// under a legal hold. A version is older than the retention when its age is strictly above it
    /// (at exactly the retention it is kept), and a version stamped after `now` has no age and is
    /// kept. Deterministic: the same register, `now` and `retention` give the same result on every
    /// replica.
    pub fn prune(&mut self, now: Stamp, retention: Span) -> Pruned {
        let mut pruned = Pruned {
            removed: Vec::new(),
            freed: ByteCount::ZERO,
            kept_locked: 0,
            held: self.legal_hold,
        };
        if self.legal_hold && !mutant!("sync_prune_ignores_hold") {
            return pruned;
        }
        let head = self.records.keys().next_back().copied();
        let mut doomed: Vec<Version> = Vec::new();
        for (version, record) in &self.records {
            if Some(*version) == head && !mutant!("sync_prune_drops_head") {
                continue;
            }
            let expired = match now.elapsed_since(record.at) {
                Some(age) => {
                    age > retention
                        || (age == retention && mutant!("sync_prune_boundary_inclusive"))
                }
                None => false,
            };
            if !expired {
                continue;
            }
            if record.is_locked(now) && !mutant!("sync_prune_ignores_lock") {
                pruned.kept_locked += 1;
                continue;
            }
            doomed.push(*version);
        }
        for version in doomed {
            if let Some(record) = self.records.remove(&version) {
                let size = record.size();
                self.bytes = self.bytes.saturating_sub(size);
                pruned.freed = pruned.freed.saturating_add(size);
                pruned.removed.push(version);
            }
        }
        pruned
    }
}
