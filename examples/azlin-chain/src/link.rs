//! The link checks of a chain: what makes an entry the next one, an entry already held, a fork, a
//! gap or a corrupt copy (planning/architecture.md section 5.9 "The team log and the space
//! chains": "the metadata group rejects an entry with a reason for a wrong `prev`", "two different
//! entries at the same position are a fork, refused and alarmed", "an entry that fails its hash or
//! signature check is fetched again from another voter"; section 6.10 V3 "a restore epoch the
//! metadata group signs"; build unit S17, lane W1-S17a).
//!
//! - [`Tip`] is the small state of a chain that the checks need: the chain id, the head, the stamp
//!   of the newest entry, and the restore epoch the next entry must have at least. It stores no
//!   entry. A state machine that keeps the chain head in its own state (S17c's `TeamState`) holds a
//!   `Tip`; it checks an entry with `Tip::check` without mutating anything, and moves with
//!   `Tip::advance`.
//! - [`Chain`] is a `Tip` that also keeps the entries it admitted (a bounded, verified, in-memory
//!   chain), so that it can tell an entry it already holds from a different entry at the same
//!   position, which is a fork.
//! - [`verify_from_genesis`] checks a whole run of entries from the start.
//!
//! The order of the checks is fixed: the chain id, then the integrity of the copy (a corrupt copy
//! is never read as a fork or a gap), then the position, then, for the next entry, its `prev`, its
//! stamp and its restore epoch. The refusals are one enum with one class each ([`RefusalClass`]):
//! `Refuse` (the entry is wrong: say why), `Alarm` (the history itself is contradicted: raise it),
//! `Refetch` (this copy is bad: fetch it again from another voter).
//!
//! THE RESTORE EPOCH of an entry (X-DELTA-W1S17A-3): an entry of an epoch below the tip's is refused,
//! `ChainRefusal::BelowRestoreEpoch`. The tip's epoch is the largest of the epoch of its newest
//! entry and the newest restore this device accepted ([`Tip::resume`], [`Tip::raise_floor`]): after a
//! device re-anchors a chain to the survivor of a restore, the entries of the lost timeline that
//! chain correctly from the survivor are still refused, because they carry the old epoch.

use core::cmp::Ordering;

use azlin_seams::{mutant, ChainSeq, Hash32, RestoreEpoch, Stamp};

use crate::entry::{Alarm, ChainEntry, ChainId, Head};

/// What to do about a refused entry.
///
/// Implements planning/architecture.md section 5.9 (refused; refused and alarmed; fetched again
/// from another voter).
///
/// Caller: `ChainRefusal::class`, `HeadRefusal::class`, `RestoreRefusal::class`; S17c
/// (`Refusal::Chain`) and the sync client choose their reaction by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefusalClass {
    /// The entry is wrong (a wrong `prev`, a gap, a stamp that runs backwards): refuse it and say
    /// why.
    Refuse,
    /// The history is contradicted (a fork, a second genesis, the lost timeline of a restore):
    /// refuse it and raise an alarm.
    Alarm,
    /// This copy is bad (its hash or signature failed): fetch the entry again from another voter.
    Refetch,
}

/// Why an entry is not accepted into a chain.
///
/// Implements planning/architecture.md section 5.9 (the reasons the group gives for a wrong
/// `prev`, a bad signature, a fork) and section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `Tip::check`, `Chain::admit`, `verify_from_checkpoint`; S17c `Refusal::Chain`, S16b.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ChainRefusal {
    /// The entry belongs to another chain.
    #[error("the entry belongs to another chain")]
    WrongChain,
    /// The chain is empty and the entry is not a genesis: position 0 with a zero `prev`.
    #[error("the first entry of a chain must be a genesis: position 0 with a zero `prev`")]
    NotGenesis,
    /// A genesis that differs from the one the chain holds. It is a fork at position 0.
    #[error("a second genesis was offered: this chain holds {have}, the offer is {got}")]
    GenesisAgain {
        /// The genesis hash the chain holds.
        have: Hash32,
        /// The hash of the second genesis.
        got: Hash32,
    },
    /// The entry is the next position but names another entry as its predecessor.
    #[error("wrong `prev`: expected {expected}, got {got}")]
    WrongPrev {
        /// The hash of the chain's head, which `prev` must be.
        expected: Hash32,
        /// The `prev` the entry carries.
        got: Hash32,
    },
    /// The entry is not at the next position: entries are missing before it (`got` above
    /// `expected`), or it is an old position this chain no longer holds (`got` below).
    #[error("wrong position: expected {expected}, got {got}")]
    Gap {
        /// The next position of the chain.
        expected: ChainSeq,
        /// The position the entry claims.
        got: ChainSeq,
    },
    /// A different entry at a position the chain holds.
    #[error("fork at position {at}: held {have}, offered {got}")]
    Fork {
        /// The position.
        at: ChainSeq,
        /// The hash the chain holds there.
        have: Hash32,
        /// The different hash offered.
        got: Hash32,
    },
    /// The entry's stamp is earlier than the stamp of the entry before it.
    #[error("the entry's stamp is earlier than the previous entry's")]
    StampBackwards,
    /// The entry was written in an older restore epoch than the chain stands in: an entry of the
    /// lost timeline of a restore (or of a restore this device already accepted).
    #[error("the entry is of restore epoch {got}, below the epoch {floor} the chain stands in")]
    BelowRestoreEpoch {
        /// The restore epoch the entry carries.
        got: RestoreEpoch,
        /// The restore epoch the chain stands in: the entry's must be at least this.
        floor: RestoreEpoch,
    },
    /// The copy's hash or signature does not hold: fetch it again from another voter.
    #[error("the entry's hash or signature does not hold: fetch it again from another voter")]
    Corrupt,
    /// The chain holds as many entries as it may (or has no next position).
    #[error("the chain has reached its limit")]
    Full,
}

impl ChainRefusal {
    /// What to do about the refusal.
    pub fn class(&self) -> RefusalClass {
        match self {
            ChainRefusal::GenesisAgain { .. }
            | ChainRefusal::Fork { .. }
            | ChainRefusal::BelowRestoreEpoch { .. } => RefusalClass::Alarm,
            ChainRefusal::Corrupt => RefusalClass::Refetch,
            ChainRefusal::WrongChain
            | ChainRefusal::NotGenesis
            | ChainRefusal::WrongPrev { .. }
            | ChainRefusal::Gap { .. }
            | ChainRefusal::StampBackwards
            | ChainRefusal::Full => RefusalClass::Refuse,
        }
    }

    /// The alarm to raise for this refusal on `chain`: a fork, a second genesis (a fork at position
    /// 0), or an entry of the lost timeline; `None` for the refusals that are no alarm.
    pub fn alarm(&self, chain: ChainId) -> Option<Alarm> {
        match *self {
            ChainRefusal::Fork { at, have, got } => Some(Alarm::Fork {
                chain,
                seq: at,
                have,
                got,
            }),
            ChainRefusal::GenesisAgain { have, got } => Some(Alarm::Fork {
                chain,
                seq: ChainSeq::MIN,
                have,
                got,
            }),
            ChainRefusal::BelowRestoreEpoch { got, floor } => {
                Some(Alarm::BelowRestoreEpoch { chain, got, floor })
            }
            _ => None,
        }
    }
}

/// Where an accepted entry stands.
///
/// Implements planning/architecture.md section 5.9 (hash-chained: each entry names its
/// predecessor).
///
/// Caller: `Tip::check`, `Chain::check`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Placement {
    /// The next entry of the chain: it extends the head.
    Next,
    /// An entry the chain already holds, identical: not a fork, and it changes nothing. `head` is
    /// the chain's current head.
    Held {
        /// The chain's head.
        head: Head,
    },
}

/// What admitting an entry did.
///
/// Implements planning/architecture.md section 5.9 (append-only: an entry is added once).
///
/// Caller: `Tip::advance`, `Chain::admit`; S17c (`apply`), S16b.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Admitted {
    /// The entry was appended; `head` is the new head.
    Appended {
        /// The chain's head after the entry.
        head: Head,
    },
    /// The entry was already held, identical: nothing changed; `head` is the unchanged head.
    AlreadyHeld {
        /// The chain's head.
        head: Head,
    },
}

impl Admitted {
    /// The chain's head after the call.
    pub fn head(&self) -> Head {
        match *self {
            Admitted::Appended { head } | Admitted::AlreadyHeld { head } => head,
        }
    }

    /// Whether the call appended an entry.
    pub fn is_new(&self) -> bool {
        matches!(self, Admitted::Appended { .. })
    }
}

/// The whole rule set of a link check, over the state of a chain: its id, its head, the stamp of
/// its newest entry, the restore epoch the next entry must have at least, and a way to read the
/// hash it holds at an older position.
fn place<E: ChainEntry>(
    chain: ChainId,
    head: Option<Head>,
    last_at: Option<Stamp>,
    floor: RestoreEpoch,
    held: impl Fn(ChainSeq) -> Option<Hash32>,
    e: &E,
) -> Result<Placement, ChainRefusal> {
    if e.chain() != chain && !mutant!("chain_checks_chain_id") {
        return Err(ChainRefusal::WrongChain);
    }
    // A corrupt copy is told before anything about its position: it is no evidence of a fork.
    if !e.is_intact() && !mutant!("chain_checks_intact") {
        return Err(ChainRefusal::Corrupt);
    }
    let seq = e.seq();
    let Some(head) = head else {
        // An empty chain starts with a genesis: position 0 and a zero `prev`.
        if seq != ChainSeq::MIN {
            return Err(ChainRefusal::NotGenesis);
        }
        if !e.prev().is_zero() && !mutant!("chain_genesis_prev_zero") {
            return Err(ChainRefusal::NotGenesis);
        }
        if e.epoch() < floor && !mutant!("chain_checks_epoch") {
            return Err(ChainRefusal::BelowRestoreEpoch {
                got: e.epoch(),
                floor,
            });
        }
        return Ok(Placement::Next);
    };
    let Some(next) = head.seq.next() else {
        return Err(ChainRefusal::Full);
    };
    match seq.cmp(&next) {
        Ordering::Greater => {
            if mutant!("chain_checks_seq") {
                return Ok(Placement::Next);
            }
            Err(ChainRefusal::Gap {
                expected: next,
                got: seq,
            })
        }
        Ordering::Equal => {
            if e.prev() != head.hash && !mutant!("chain_checks_prev") {
                return Err(ChainRefusal::WrongPrev {
                    expected: head.hash,
                    got: e.prev(),
                });
            }
            if let Some(last) = last_at {
                if e.at() < last && !mutant!("chain_checks_stamp") {
                    return Err(ChainRefusal::StampBackwards);
                }
            }
            if e.epoch() < floor && !mutant!("chain_checks_epoch") {
                return Err(ChainRefusal::BelowRestoreEpoch {
                    got: e.epoch(),
                    floor,
                });
            }
            Ok(Placement::Next)
        }
        Ordering::Less => match held(seq) {
            Some(have) if have == e.hash() => Ok(Placement::Held { head }),
            Some(have) => {
                if mutant!("chain_fork_check_off") {
                    return Ok(Placement::Held { head });
                }
                if seq == ChainSeq::MIN && !mutant!("chain_second_genesis_check_off") {
                    Err(ChainRefusal::GenesisAgain {
                        have,
                        got: e.hash(),
                    })
                } else {
                    Err(ChainRefusal::Fork {
                        at: seq,
                        have,
                        got: e.hash(),
                    })
                }
            }
            // An old position this state does not hold: a replay it cannot judge.
            None => Err(ChainRefusal::Gap {
                expected: next,
                got: seq,
            }),
        },
    }
}

/// The state of a chain that the link checks need: the chain, its head, the stamp of its newest
/// entry, and the restore epoch the next entry must have at least. It stores no entry, so it can only
/// tell a fork at the head's own position (it holds one hash); `Chain` keeps entries and tells a fork
/// at any position it holds.
///
/// Implements planning/architecture.md section 5.9 (link checks; "the head a device pins") and
/// section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `Chain`, `verify_from_genesis`, `verify_from_checkpoint`; S17c (`TeamState`'s chain
/// head, a seam of its `admit` and `apply`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    chain: ChainId,
    head: Option<Head>,
    last_at: Option<Stamp>,
    epoch: RestoreEpoch,
}

impl Tip {
    /// The state of an empty chain: the next entry must be a genesis, of any restore epoch (raise
    /// the floor with `raise_floor` on a device that accepted a restore).
    pub const fn new(chain: ChainId) -> Tip {
        Tip {
            chain,
            head: None,
            last_at: None,
            epoch: RestoreEpoch::ZERO,
        }
    }

    /// The state of a chain that stands at `head`, whose newest entry has the stamp `last_at`, in
    /// the restore epoch `epoch`: the next entry must be of that epoch or later. A verified
    /// checkpoint passes the epoch its head was signed in; a device that re-anchored the chain to
    /// the survivor of a restore passes the epoch of that restore (the entries of the lost timeline
    /// that chain correctly from the survivor carry an older one). The head a registry persisted
    /// passes the epoch it stored with it.
    pub const fn resume(head: Head, last_at: Stamp, epoch: RestoreEpoch) -> Tip {
        Tip {
            chain: head.chain,
            head: Some(head),
            last_at: Some(last_at),
            epoch,
        }
    }

    /// The chain.
    pub const fn chain(&self) -> ChainId {
        self.chain
    }

    /// The head; `None` for an empty chain.
    pub const fn head(&self) -> Option<Head> {
        self.head
    }

    /// The stamp of the newest entry; `None` for an empty chain.
    pub const fn last_at(&self) -> Option<Stamp> {
        self.last_at
    }

    /// The restore epoch the next entry must have at least.
    pub const fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }

    /// Raises the restore epoch the next entry must have at least (never lowers it): a device
    /// that accepted the restore of this epoch calls it on every chain it follows.
    pub fn raise_floor(&mut self, epoch: RestoreEpoch) {
        self.epoch = self.epoch.max(epoch);
    }

    /// The position the next entry must have; `None` when the head is at the largest position.
    pub fn next_seq(&self) -> Option<ChainSeq> {
        match self.head {
            None => Some(ChainSeq::MIN),
            Some(head) => head.seq.next(),
        }
    }

    /// Where `e` stands against this state, without changing it.
    pub fn check<E: ChainEntry>(&self, e: &E) -> Result<Placement, ChainRefusal> {
        let head = self.head;
        place(
            self.chain,
            head,
            self.last_at,
            self.epoch,
            |seq| head.filter(|h| h.seq == seq).map(|h| h.hash),
            e,
        )
    }

    /// Checks `e` and, when it is the next entry, moves the state to it.
    pub fn advance<E: ChainEntry>(&mut self, e: &E) -> Result<Admitted, ChainRefusal> {
        match self.check(e)? {
            Placement::Held { head } => Ok(Admitted::AlreadyHeld { head }),
            Placement::Next => {
                let head = Head {
                    chain: self.chain,
                    seq: e.seq(),
                    hash: e.hash(),
                };
                self.head = Some(head);
                self.last_at = Some(e.at());
                self.epoch = e.epoch();
                Ok(Admitted::Appended { head })
            }
        }
    }
}

/// A verified, in-memory chain: a `Tip` and the entries it admitted, bounded. It tells an entry it
/// already holds (identical: nothing changes) from a different entry at a held position (a fork).
///
/// Implements planning/architecture.md section 5.9 ("every device keeps the part of the chain it
/// verified"; limits stated up front, section 1.4 rule 4).
///
/// Caller: the device's local chain store (CHAIN-GLUE), S16b's space chains, the tests of S17c.
#[derive(Clone, Debug)]
pub struct Chain<E: ChainEntry> {
    tip: Tip,
    entries: Vec<E>,
    max_entries: usize,
}

impl<E: ChainEntry> Chain<E> {
    /// The most entries a chain keeps unless it is given another bound.
    pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

    /// An empty chain with the default bound.
    pub fn new(chain: ChainId) -> Chain<E> {
        Chain::with_limit(chain, Self::DEFAULT_MAX_ENTRIES)
    }

    /// An empty chain that keeps at most `max_entries` entries; the next entry after that is
    /// refused as `Full`.
    pub fn with_limit(chain: ChainId, max_entries: usize) -> Chain<E> {
        Chain {
            tip: Tip::new(chain),
            entries: Vec::new(),
            max_entries,
        }
    }

    /// A chain that stands at `head` (a verified checkpoint, a persisted head, the survivor of a
    /// restore) in the restore epoch `epoch` (see `Tip::resume`) and holds none of the entries before
    /// it: positions up to `head.seq` are old positions it cannot judge.
    pub fn resume(head: Head, last_at: Stamp, epoch: RestoreEpoch, max_entries: usize) -> Chain<E> {
        Chain {
            tip: Tip::resume(head, last_at, epoch),
            entries: Vec::new(),
            max_entries,
        }
    }

    /// The chain.
    pub fn id(&self) -> ChainId {
        self.tip.chain
    }

    /// The head; `None` for an empty chain.
    pub fn head(&self) -> Option<Head> {
        self.tip.head
    }

    /// The stamp of the newest entry; `None` for an empty chain.
    pub fn last_at(&self) -> Option<Stamp> {
        self.tip.last_at
    }

    /// The restore epoch the next entry must have at least.
    pub fn epoch(&self) -> RestoreEpoch {
        self.tip.epoch
    }

    /// Raises the restore epoch the next entry must have at least (see `Tip::raise_floor`).
    pub fn raise_floor(&mut self, epoch: RestoreEpoch) {
        self.tip.raise_floor(epoch);
    }

    /// The link state of the chain.
    pub fn tip(&self) -> &Tip {
        &self.tip
    }

    /// How many entries the chain holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the chain holds no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries held, oldest first.
    pub fn entries(&self) -> &[E] {
        &self.entries
    }

    /// The entry held at `seq`; `None` for a position the chain does not hold.
    pub fn get(&self, seq: ChainSeq) -> Option<&E> {
        let first = self.entries.first()?.seq();
        let offset = seq.get().checked_sub(first.get())?;
        match usize::try_from(offset) {
            Ok(index) => self.entries.get(index),
            Err(_) => None,
        }
    }

    /// Where `e` stands against this chain, without changing it.
    pub fn check(&self, e: &E) -> Result<Placement, ChainRefusal> {
        let placement = place(
            self.tip.chain,
            self.tip.head,
            self.tip.last_at,
            self.tip.epoch,
            |seq| self.get(seq).map(|held| held.hash()),
            e,
        )?;
        if placement == Placement::Next && self.entries.len() >= self.max_entries {
            return Err(ChainRefusal::Full);
        }
        Ok(placement)
    }

    /// Checks `e` and, when it is the next entry, appends it. An identical entry already held is
    /// `AlreadyHeld` and changes nothing; any refusal leaves the chain as it was.
    pub fn admit(&mut self, e: E) -> Result<Admitted, ChainRefusal> {
        match self.check(&e)? {
            Placement::Held { head } => Ok(Admitted::AlreadyHeld { head }),
            Placement::Next => {
                let head = Head {
                    chain: self.tip.chain,
                    seq: e.seq(),
                    hash: e.hash(),
                };
                self.tip.head = Some(head);
                self.tip.last_at = Some(e.at());
                self.tip.epoch = e.epoch();
                self.entries.push(e);
                Ok(Admitted::Appended { head })
            }
        }
    }
}

/// Moves `tip` through `entries`, which must be a strictly sequential run: every entry the next one.
/// An entry the tip already holds is out of place in a run (`Gap`), whatever it equals. The head
/// after the last entry, `None` when there are none.
pub(crate) fn run<E: ChainEntry>(
    tip: &mut Tip,
    entries: &[E],
) -> Result<Option<Head>, ChainRefusal> {
    let mut last = None;
    for e in entries {
        match tip.advance(e)? {
            Admitted::Appended { head } => last = Some(head),
            Admitted::AlreadyHeld { .. } => {
                let expected = tip.next_seq().ok_or(ChainRefusal::Full)?;
                return Err(ChainRefusal::Gap {
                    expected,
                    got: e.seq(),
                });
            }
        }
    }
    Ok(last)
}

/// Verifies a whole run of entries from the start of `chain`: a genesis, then every entry the next
/// one, none of an older restore epoch than the one before it. The head after the last entry; `None`
/// for no entries. The first refusal stops the run.
///
/// Implements planning/architecture.md section 5.9 (a new device verifies the chain; "because the
/// chain authenticates itself, any copy can restore it").
///
/// Caller: a device that has no checkpoint (CHAIN-GLUE); the property test
/// `verify_from_checkpoint_equals_verify_from_genesis`.
pub fn verify_from_genesis<E: ChainEntry>(
    chain: ChainId,
    entries: &[E],
) -> Result<Option<Head>, ChainRefusal> {
    run(&mut Tip::new(chain), entries)
}
