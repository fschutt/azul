//! Authorized key epochs and the view of the space chains that the key rules read (planning/architecture.md
//! section 5.5 "Epochs must be authorized", section 5.9 "Space chains", section 5.4 `history`; build unit
//! S16, lane W1-S16b).
//!
//! THE RULE: "an epoch counts only if a signed `space_rekey` (or `space_create`) chain entry from an
//! authorized device commits to its key id. Envelopes carry the signer. Clients reject every other
//! epoch. Otherwise a removed member, who knows epoch n, plus any writer could plant an epoch n+1 they
//! also know." [`epoch_authorized`] is that rule over a verified [`SpaceState`]: the epoch must be one a
//! chain entry committed, and the key id must be the one that entry committed. Older epochs stay
//! authorized (they are what the members read the past with); an epoch no entry committed, or a key id
//! that is not the committed one, is not.
//!
//! [`SpaceChains`] is the device's collection of verified space chains and implements the seam that the
//! key code (`azlin-keys`, S15c2) asks: `commitment(space, epoch)` and `is_authorized_signer(space,
//! device, at)`. Its answers keep three things apart that must never be confused: a chain that holds no
//! commitment (`Ok(None)`), a signer that is not authorized (`Ok(false)`) and a chain that cannot be read
//! (`Err`): an unreadable chain is Unknown, never "not authorized" and never "no commitment".
//!
//! WHO COULD SIGN AT A POSITION (`is_authorized_signer`): the device signed the admitted entry at that
//! position (it was authorized by construction: `admit` decided it), or it belongs to a member who held
//! the manager role through a DIRECT grant at that position. A grant takes effect from the position AFTER
//! its entry and ends with the entry that revokes it: a manager made at position 5 and removed at
//! position 9 is authorized at 6 to 9 (it may sign its own removal) and not at 10. A manager through a
//! GROUP is recognized only at the positions it signed (the group's members are the team log's state,
//! which the chain does not hold; drift row X-DELTA-W1S16B-12).

use std::collections::BTreeMap;

use azlin_keys::{ChainError, CommitKind, Commitment, SpaceChainView};
use azlin_seams::{mutant, AccountId, ChainSeq, DeviceId, Epoch, KeyId, MemberId, SpaceId};

use crate::kind::HistoryMode;
use crate::state::{Actor, SpaceState};

/// What a signed chain entry committed for one epoch: the key id, the position and the device of the
/// entry (`Create` for epoch 1, `Rekey` after), and whether a chain wrap (the previous epoch under this
/// one) was made with it.
///
/// Implements planning/architecture.md section 5.5 ("an epoch counts only if a signed `space_rekey` (or
/// `space_create`) chain entry from an authorized device commits to its key id"; "plus the chain wrap
/// (epoch n under epoch n+1)").
///
/// Caller: `SpaceState::epoch_record`, `SpaceState::commitment`, `epoch_authorized`,
/// `chain_visible_to`, `apply` (writes one for every `space_create` and `space_rekey`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpochRecord {
    /// The key id the entry committed.
    pub key_id: KeyId,
    /// The position of the committing entry.
    pub seq: ChainSeq,
    /// Whether the committing entry created the space or rekeyed it.
    pub kind: CommitKind,
    /// The device that signed the committing entry.
    pub signer: DeviceId,
    /// Whether the previous epoch's key was wrapped under this one (a rotation in `full` history mode).
    /// Epoch 1 has none.
    pub chained: bool,
}

impl SpaceState {
    /// What the chain committed for `epoch`; `None` for an epoch no entry committed.
    pub fn epoch_record(&self, epoch: Epoch) -> Option<&EpochRecord> {
        self.epochs.get(&epoch)
    }

    /// Every committed epoch, oldest first.
    pub fn epochs(&self) -> impl Iterator<Item = (Epoch, &EpochRecord)> + '_ {
        self.epochs.iter().map(|(epoch, record)| (*epoch, record))
    }

    /// The commitment of `epoch` in the form the key rules read it (`azlin_keys::Commitment`); `None`
    /// when no entry committed that epoch.
    ///
    /// Implements planning/architecture.md section 5.5 (a signed entry commits to the epoch's key id).
    ///
    /// Caller: `SpaceChains` (`SpaceChainView::commitment`).
    pub fn commitment(&self, epoch: Epoch) -> Option<Commitment> {
        let record = self.epochs.get(&epoch)?;
        Some(Commitment {
            space: self.id,
            epoch,
            key_id: record.key_id,
            signer: record.signer,
            seq: record.seq,
            kind: record.kind,
        })
    }

    /// The history mode in force at chain position `seq`: the mode of the last `space_create` or
    /// `space_history` entry at or before it; `None` before the space was created.
    pub fn history_at(&self, seq: ChainSeq) -> Option<HistoryMode> {
        self.history_switches
            .iter()
            .rev()
            .find(|(at, _)| *at <= seq)
            .map(|(_, mode)| *mode)
    }

    /// The epoch in force at chain position `seq`: the newest epoch committed at or before it; `None`
    /// before the space was created.
    pub fn epoch_at(&self, seq: ChainSeq) -> Option<Epoch> {
        self.epochs
            .iter()
            .rev()
            .find(|(_, record)| record.seq <= seq)
            .map(|(epoch, _)| *epoch)
    }

    /// Whether `device` could sign the entry at position `at` (see the module): `Err` for a position
    /// beyond the one after the head, which the chain says nothing about.
    pub(crate) fn authorized_at(
        &self,
        device: &DeviceId,
        at: ChainSeq,
        roster: &BTreeMap<DeviceId, Actor>,
    ) -> Result<bool, ChainError> {
        let next = match self.chain.head() {
            Some(head) => head.seq.next(),
            None => Some(ChainSeq::MIN),
        };
        let known = match next {
            Some(next) => at <= next,
            None => true,
        };
        if !known && !mutant!("spaces_view_future_position_answered") {
            return Err(ChainError::Unreadable);
        }
        // The device signed the admitted entry at this very position: `admit` decided it.
        if let Some(entry) = self.chain.get(at) {
            if entry.signer == *device {
                return Ok(true);
            }
        }
        let actor = match self.signers.get(device).or_else(|| roster.get(device)) {
            Some(actor) => *actor,
            None => return Ok(false),
        };
        let grantee = actor.grantee(self.account);
        let held = match self.windows.get(&grantee) {
            Some(windows) => windows.iter().any(|window| {
                if window.from > at {
                    return false;
                }
                match window.until {
                    // Still held.
                    None => true,
                    Some(until) => {
                        if mutant!("spaces_signer_window_open_ended") {
                            true
                        } else if mutant!("spaces_signer_window_off_by_one") {
                            at <= until
                        } else {
                            at < until
                        }
                    }
                }
            }),
            None => false,
        };
        Ok(held)
    }
}

/// Whether `key_id` is the key id that a signed chain entry committed for `epoch`: the rule "an epoch
/// counts only if a signed `space_rekey` (or `space_create`) chain entry from an authorized device
/// commits to its key id". An epoch no entry committed (the one a removed member plants), an epoch above
/// the current one, and a key id other than the committed one are all `false`; every committed epoch,
/// old ones included, is `true` for its own key id (old epochs stay authorized for reading).
///
/// Implements planning/architecture.md section 5.5 ("Epochs must be authorized"; "Clients reject every
/// other epoch").
///
/// Caller: S15's `open_epoch` and `authorize_epoch` (through the glue), the node's check of a `reg_wrap`
/// row.
pub fn epoch_authorized(state: &SpaceState, epoch: Epoch, key_id: &KeyId) -> bool {
    let Some(core) = state.core.as_ref() else {
        return false;
    };
    if mutant!("spaces_epoch_current_only") && epoch != core.key_epoch {
        return false;
    }
    match state.epochs.get(&epoch) {
        Some(record) => mutant!("spaces_epoch_any_key") || record.key_id == *key_id,
        None => mutant!("spaces_epoch_planted_authorized") && epoch > core.key_epoch,
    }
}

/// Which epochs a member who joined at some chain position may read the history of: the epochs
/// `first..=through`, opened through the chain wraps (the previous epoch's key under the next one's).
///
/// `Full` history: the joiner gets the epoch chain, so it reads from the oldest epoch that is chained
/// to the one in force when it joined. `Forward` history: no chain for joiners, so it reads only from
/// the epoch that the rotation after its grant created (none yet while that rotation is still due).
///
/// Implements planning/architecture.md section 5.4 (`history`: "`full` (joiners get the epoch chain) or
/// `forward` (rotate on add, no chain for joiners)").
///
/// Caller: `chain_visible_to` (makes it); the node's serving of chain wraps and the registry's wrap
/// rows (S15c2 through the SPACES-APPLY glue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpochChainView {
    mode: Option<HistoryMode>,
    first: Option<Epoch>,
    through: Option<Epoch>,
}

impl EpochChainView {
    /// The history mode that was in force when the member joined; `None` before the space existed.
    pub fn mode(&self) -> Option<HistoryMode> {
        self.mode
    }

    /// The oldest epoch the member may read; `None` when it may read none (yet).
    pub fn first(&self) -> Option<Epoch> {
        self.first
    }

    /// The newest epoch of the space now.
    pub fn through(&self) -> Option<Epoch> {
        self.through
    }

    /// Whether the member may read the history of `epoch`.
    pub fn contains(&self, epoch: Epoch) -> bool {
        match (self.first, self.through) {
            (Some(first), Some(through)) => first <= epoch && epoch <= through,
            _ => false,
        }
    }
}

/// The epoch before `epoch`, when `epoch`'s key was made with a chain wrap of it (a rotation in `full`
/// history mode); `None` for epoch 1, for an epoch no entry committed and for a rotation without a chain.
fn chained_before(state: &SpaceState, epoch: Epoch) -> Option<Epoch> {
    let record = state.epochs.get(&epoch)?;
    if record.chained {
        epoch.prev()
    } else {
        None
    }
}

/// The epochs a member that joined at chain position `joined_at` (the position of its `space_grant`
/// entry) may read the history of: `Full` gives the epoch chain, `Forward` gives only the epochs from the
/// rotation after the join. The mode is the one in force AT the join: a later `space_history` switch
/// applies to later joiners only.
///
/// Implements planning/architecture.md section 5.4 (`history`: full or forward) and section 5.5
/// (rotation, "plus the chain wrap").
///
/// Caller: the node's decision which chain wraps to serve to a member (SPACES-APPLY), S15c2's rekey
/// plans.
pub fn chain_visible_to(state: &SpaceState, joined_at: ChainSeq) -> EpochChainView {
    let mode = if mutant!("spaces_visible_uses_current_mode") {
        state.history()
    } else {
        state.history_at(joined_at)
    };
    let first = match mode {
        None => None,
        Some(HistoryMode::Full) => match state.epoch_at(joined_at) {
            None => None,
            Some(mut first) => {
                // The epoch chain: while an epoch is chained to the one before it, the one before it can
                // be opened with this one's key.
                while let Some(previous) = chained_before(state, first) {
                    first = previous;
                }
                Some(first)
            }
        },
        Some(HistoryMode::Forward) => state
            .epochs
            .iter()
            .find(|(_, record)| record.seq > joined_at)
            .map(|(epoch, _)| *epoch),
    };
    EpochChainView {
        mode,
        first,
        through: state.key_epoch(),
    }
}

/// What a collection knows about one space.
#[derive(Debug)]
enum Slot {
    /// A chain that was verified entry by entry (`replay`, or `apply` on `admit`ted entries).
    Verified(Box<SpaceState>),
    /// A chain that cannot be read: why.
    Unreadable(ChainError),
}

/// A device's collection of verified space chains: the view the key rules read (`azlin_keys::
/// SpaceChainView`).
///
/// A chain is `insert`ed only as a [`SpaceState`], which only `apply` and `replay` can build, so every
/// commitment it answers comes from a signed entry that `admit` checked. `mark_unreadable` records that a
/// chain could not be read (a store failure, a copy that fails its own verification): every answer for it
/// is then an error, never "no commitment" and never "not authorized".
///
/// `learn_device` tells the collection which member a device belongs to, for the devices that have not
/// signed a chain entry (the team log knows the devices; the chain does not).
///
/// Implements planning/architecture.md section 5.5 ("Epochs must be authorized") and section 5.9 (each
/// space has its own signed chain); it is the seam that planner P3 asked for (W1-S15c2).
///
/// Caller: S15c2's `authorize_epoch` and `RekeyPlan::is_stale` (through CHAIN-GLUE), the device's key
/// flows (INT-S15-1).
#[derive(Debug, Default)]
pub struct SpaceChains {
    slots: BTreeMap<SpaceId, Slot>,
    roster: BTreeMap<DeviceId, Actor>,
}

impl SpaceChains {
    /// An empty collection: every space is unknown.
    pub fn new() -> SpaceChains {
        SpaceChains::default()
    }

    /// Holds a verified chain (replacing what was held for the space).
    pub fn insert(&mut self, state: SpaceState) {
        self.slots
            .insert(state.id(), Slot::Verified(Box::new(state)));
    }

    /// Records that the chain of `space` cannot be read, and why.
    pub fn mark_unreadable(&mut self, space: SpaceId, why: ChainError) {
        self.slots.insert(space, Slot::Unreadable(why));
    }

    /// The verified chain of `space`; `None` for an unknown or unreadable one.
    pub fn state(&self, space: &SpaceId) -> Option<&SpaceState> {
        match self.slots.get(space) {
            Some(Slot::Verified(state)) => Some(state),
            _ => None,
        }
    }

    /// The verified chain of `space`, to apply further entries to; `None` for an unknown or unreadable
    /// one.
    pub fn state_mut(&mut self, space: &SpaceId) -> Option<&mut SpaceState> {
        match self.slots.get_mut(space) {
            Some(Slot::Verified(state)) => Some(state),
            _ => None,
        }
    }

    /// Tells the collection that `device` belongs to `member` of `account` (a fact of the team log).
    pub fn learn_device(&mut self, device: DeviceId, account: AccountId, member: MemberId) {
        self.roster.insert(device, Actor { account, member });
    }

    /// How many spaces the collection holds a slot for.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the collection holds no space.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The chain of `space`, or why it cannot be read.
    fn readable(&self, space: &SpaceId) -> Result<&SpaceState, ChainError> {
        match self.slots.get(space) {
            None => Err(ChainError::UnknownSpace),
            Some(Slot::Unreadable(why)) => Err(*why),
            Some(Slot::Verified(state)) => Ok(state),
        }
    }
}

impl SpaceChainView for SpaceChains {
    fn commitment(&self, space: &SpaceId, epoch: Epoch) -> Result<Option<Commitment>, ChainError> {
        match self.readable(space) {
            Ok(state) => Ok(state.commitment(epoch)),
            // The confusion the view must never make: an unreadable chain is not "no commitment".
            Err(ChainError::Unreadable | ChainError::Corrupt)
                if mutant!("spaces_view_unreadable_as_none") =>
            {
                Ok(None)
            }
            Err(why) => Err(why),
        }
    }

    fn is_authorized_signer(
        &self,
        space: &SpaceId,
        device: &DeviceId,
        at: ChainSeq,
    ) -> Result<bool, ChainError> {
        match self.readable(space) {
            Ok(state) => state.authorized_at(device, at, &self.roster),
            // The confusion the view must never make: an unreadable chain is not "not authorized".
            Err(ChainError::Unreadable | ChainError::Corrupt)
                if mutant!("spaces_view_unreadable_as_unauthorized") =>
            {
                Ok(false)
            }
            Err(why) => Err(why),
        }
    }
}
