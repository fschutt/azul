//! The head a device pins and never lets go backwards (planning/architecture.md section 5.9 "each
//! device remembers the newest head it has seen and refuses an older one, so history cannot be
//! rolled back"; section 5.13 "A head never goes backwards. A device refuses a space head older than
//! the newest it has seen. The only way back is a restore"; section 6.10 S15 and V3 "a restore epoch
//! the metadata group signs"; build unit S17, lane W1-S17a).
//!
//! [`HeadPin`] is a device's memory of the chains it follows: per chain, the newest validly signed
//! head it was offered, and the newest restore epoch it accepted (the FLOOR). [`HeadPin::offer`] is
//! the one door for a head: it checks the signature through the [`HeadVerifier`], then the head's
//! restore epoch against the floor, then the position.
//!
//! | what is offered | verdict | the pin |
//! |---|---|---|
//! | a head whose signature does not verify | `BadSignature` | unchanged |
//! | a head signed in a restore epoch below the floor | `Refused(BelowRestoreEpoch)`, an alarm | unchanged |
//! | a head of a chain not pinned yet | `Advanced` | pins it |
//! | a head above the pinned position | `Advanced` | moves up |
//! | the pinned head again | `Unchanged` | unchanged |
//! | another hash at the pinned position | `Fork(alarm)` | KEEPS the old head |
//! | a head below the pinned position | `RefusedOlder` | unchanged |
//!
//! The only way down is a restore announcement: [`crate::reanchor`] sets the pins its signed record
//! lists (the spec is `azul-storage/src/restore.rs`, a drive's counter against the restore epoch) and
//! raises the floor to the record's epoch. THE FLOOR is what makes the way down one way: after a
//! restore, the heads of the lost timeline (signed in an older epoch, validly, before the restore)
//! can no longer advance a re-anchored pin (X-DELTA-W1S17A-3). A head of a HIGHER epoch than the
//! floor is an ordinary head (the device has not been announced that restore yet): it is judged by
//! its position, and the floor does not move until the record arrives. A device born after a
//! restore starts at the current epoch ([`HeadPin::starting_at`]) so that an old announcement
//! never excuses a later unannounced rollback.

use std::collections::BTreeMap;

use azlin_seams::{mutant, ChainSeq, RestoreEpoch};

use crate::entry::{Alarm, ChainId, Head, HeadVerifier, SignedHead};
use crate::link::RefusalClass;

/// The most chains one pin follows unless it is given another bound (a device follows its
/// account's team log and its spaces: the limit of watched spaces is 20,000).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front).
///
/// Caller: `HeadPin::new`.
pub const MAX_PINNED_CHAINS: usize = 100_000;

/// What the pin holds of one chain.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pinned {
    /// The newest validly signed head offered.
    Signed(SignedHead),
    /// A position a signed restore record set: no signed head of it is held.
    Anchored(Head),
}

impl Pinned {
    fn head(&self) -> Head {
        match self {
            Pinned::Signed(signed) => signed.head,
            Pinned::Anchored(head) => *head,
        }
    }
}

/// Why a head is refused on grounds other than its signature and its position.
///
/// Implements planning/architecture.md section 6.10 V3 (a restore epoch the metadata group signs)
/// and section 5.13 (the only way back is a restore).
///
/// Caller: `PinVerdict::Refused`; the sync client reacts to its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum HeadRefusal {
    /// The head was signed in a restore epoch below the newest restore this device accepted: a head
    /// of the lost timeline, however validly it is signed.
    #[error("the head is of restore epoch {head}, below the epoch {floor} this device accepted")]
    BelowRestoreEpoch {
        /// The restore epoch the head was signed in.
        head: RestoreEpoch,
        /// The newest restore epoch this device accepted.
        floor: RestoreEpoch,
    },
}

impl HeadRefusal {
    /// What to do about the refusal: a head of the lost timeline offered after the device accepted
    /// the restore is a replay of what the restore removed: an `Alarm`.
    pub fn class(&self) -> RefusalClass {
        // The only refusal there is: a head of the lost timeline. A new variant decides its own.
        RefusalClass::Alarm
    }

    /// The alarm to raise for this refusal on `chain`.
    pub fn alarm(&self, chain: ChainId) -> Alarm {
        let HeadRefusal::BelowRestoreEpoch { head, floor } = *self;
        Alarm::BelowRestoreEpoch {
            chain,
            got: head,
            floor,
        }
    }
}

/// What the pin decided about an offered head.
///
/// Implements planning/architecture.md section 5.9 and section 5.13 (a head never goes backwards;
/// a fork is refused and alarmed) and section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `HeadPin::offer`; S16a's head guard and the sync client react to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinVerdict {
    /// The head is newer than the pinned one (or the first of its chain): it is pinned now.
    Advanced,
    /// The pinned head again: the position is unchanged.
    Unchanged,
    /// The head is older than the pinned one: refused, the pin stays. `newest` is the pinned
    /// position.
    RefusedOlder {
        /// The pinned position.
        newest: ChainSeq,
    },
    /// Another hash at the pinned position: refused and alarmed, the pin keeps the old head.
    Fork(Alarm),
    /// The head's signature does not verify (or the verifier cannot check it): refused, ignored.
    BadSignature,
    /// The head is refused for its restore epoch (the lost timeline): refused and alarmed, the pin
    /// stays.
    Refused(HeadRefusal),
    /// The pin follows as many chains as it may and this head is of another one.
    Full,
}

impl PinVerdict {
    /// The alarm to raise for the head of `chain`, if the verdict is one: a fork, or a head of the
    /// lost timeline.
    pub fn alarm(&self, chain: &ChainId) -> Option<Alarm> {
        match self {
            PinVerdict::Fork(alarm) => Some(*alarm),
            PinVerdict::Refused(refusal) => Some(refusal.alarm(*chain)),
            _ => None,
        }
    }
}

/// A device's memory of the heads it has seen. It never goes backwards: the pinned position of a
/// chain is the largest position of the validly signed heads offered (of the epochs it accepts),
/// until a signed restore record re-anchors it; and it never goes back into the lost timeline of a
/// restore: the floor is the newest restore epoch accepted.
///
/// Implements planning/architecture.md section 5.9 ("each device remembers the newest head it has
/// seen and refuses an older one"), section 5.13 (a head never goes backwards) and section 6.10 V3
/// (a restore epoch the metadata group signs).
///
/// Caller: the device's sync and team-log clients (CHAIN-GLUE), `compare_peer`, `reanchor`.
#[derive(Clone, Debug)]
pub struct HeadPin {
    pinned: BTreeMap<ChainId, Pinned>,
    restore_epoch: RestoreEpoch,
    max_chains: usize,
}

impl Default for HeadPin {
    fn default() -> HeadPin {
        HeadPin::new()
    }
}

impl HeadPin {
    /// An empty memory with the default bound ([`MAX_PINNED_CHAINS`]), at the restore epoch 0.
    pub fn new() -> HeadPin {
        HeadPin::with_limit(MAX_PINNED_CHAINS)
    }

    /// An empty memory that follows at most `max_chains` chains.
    pub fn with_limit(max_chains: usize) -> HeadPin {
        HeadPin {
            pinned: BTreeMap::new(),
            restore_epoch: RestoreEpoch::ZERO,
            max_chains,
        }
    }

    /// An empty memory of a device born (or reset) after the restore announced with `restore_epoch`:
    /// it starts at that epoch, so an older announcement never excuses a later unannounced
    /// rollback, and a head of the lost timeline is refused from the first.
    pub fn starting_at(restore_epoch: RestoreEpoch) -> HeadPin {
        HeadPin {
            restore_epoch,
            ..HeadPin::new()
        }
    }

    /// Offers a head: its signature is checked by `v`, then its restore epoch against the floor,
    /// then its position against the pin (see the module).
    pub fn offer(&mut self, h: SignedHead, v: &dyn HeadVerifier) -> PinVerdict {
        if !mutant!("chain_pin_skips_signature_check") && v.verify_head(&h).is_err() {
            return PinVerdict::BadSignature;
        }
        // The lost timeline: a head signed before the restore this device accepted. It may be
        // newer than the position the restore re-anchored to; it is no way forward.
        if h.epoch < self.restore_epoch && !mutant!("chain_pin_epoch_floor_off") {
            return PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
                head: h.epoch,
                floor: self.restore_epoch,
            });
        }
        let chain = h.head.chain;
        let Some(current) = self.pinned.get(&chain) else {
            if self.pinned.len() >= self.max_chains {
                return PinVerdict::Full;
            }
            self.pinned.insert(chain, Pinned::Signed(h));
            return PinVerdict::Advanced;
        };
        let have = current.head();
        if h.head.seq > have.seq {
            self.pinned.insert(chain, Pinned::Signed(h));
            return PinVerdict::Advanced;
        }
        if h.head.seq < have.seq {
            if mutant!("chain_pin_accepts_older") {
                self.pinned.insert(chain, Pinned::Signed(h));
                return PinVerdict::Advanced;
            }
            return PinVerdict::RefusedOlder { newest: have.seq };
        }
        // The same position.
        if h.head.hash != have.hash && !mutant!("chain_pin_fork_unchecked") {
            return PinVerdict::Fork(Alarm::Fork {
                chain,
                seq: have.seq,
                have: have.hash,
                got: h.head.hash,
            });
        }
        // The same head again. A position a restore record set gains its signed copy; the pinned
        // position is unchanged either way.
        if matches!(current, Pinned::Anchored(_)) && h.head.hash == have.hash {
            self.pinned.insert(chain, Pinned::Signed(h));
        }
        PinVerdict::Unchanged
    }

    /// The pinned head of `chain`.
    pub fn head(&self, chain: &ChainId) -> Option<Head> {
        self.pinned.get(chain).map(Pinned::head)
    }

    /// The pinned position of `chain`.
    pub fn seq(&self, chain: &ChainId) -> Option<ChainSeq> {
        self.head(chain).map(|head| head.seq)
    }

    /// The newest signed head of `chain` the device holds, to hand to a peer; `None` when the
    /// chain is not pinned or its position was set by a restore record only.
    pub fn signed_head(&self, chain: &ChainId) -> Option<&SignedHead> {
        match self.pinned.get(chain) {
            Some(Pinned::Signed(signed)) => Some(signed),
            Some(Pinned::Anchored(_)) | None => None,
        }
    }

    /// The signed heads the device holds, one per chain, in chain order: what it sends a peer when
    /// devices of one account meet.
    pub fn signed_heads(&self) -> impl Iterator<Item = &SignedHead> {
        self.pinned.values().filter_map(|pinned| match pinned {
            Pinned::Signed(signed) => Some(signed),
            Pinned::Anchored(_) => None,
        })
    }

    /// The newest restore epoch this device accepted: the floor of the heads it takes.
    pub fn restore_epoch(&self) -> RestoreEpoch {
        self.restore_epoch
    }

    /// How many chains are pinned.
    pub fn len(&self) -> usize {
        self.pinned.len()
    }

    /// Whether no chain is pinned.
    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty()
    }

    /// The most chains this pin follows.
    pub fn limit(&self) -> usize {
        self.max_chains
    }

    /// Sets the position of a chain from a signed restore record (the one way a pin goes back).
    pub(crate) fn anchor(&mut self, head: Head) {
        self.pinned.insert(head.chain, Pinned::Anchored(head));
    }

    /// Raises the floor to the restore epoch a verified record announced.
    pub(crate) fn set_restore_epoch(&mut self, epoch: RestoreEpoch) {
        self.restore_epoch = epoch;
    }
}
