//! The five ops of a space's membership chain (planning/architecture.md section 5.9 "Space chains":
//! `space_create`, `space_grant`, `space_revoke`, `space_rekey`, `space_history`; section 5.5 "Epochs
//! must be authorized"; build unit S16, lane W1-S16b).
//!
//! An op is plain data: what the signed entry says. Whether the signer may sign it, and what it
//! changes, is `admit` and `apply`; the byte-exact encoding of an op is the glue's (azlin-cbor, S5's
//! contract types), never this crate's.

use azlin_seams::{ByteCount, Epoch, KeyId, MemberId};

use crate::grant::{Grantee, Payer};
use crate::kind::{HistoryMode, SpaceKind};
use crate::role::SpaceRole;

/// Why a space's key was rotated. The order is the order of severity (`Removal` is the most severe):
/// a rekey that is already due for a lesser reason becomes due for the greater one, and one that is due
/// for the greater is not lowered.
///
/// Implements planning/architecture.md section 5.5 ("Rotation: a new epoch key, wrapped only to the
/// remaining members and groups") and section 5.4 (`history = forward`: rotate on add).
///
/// Caller: `SpaceOp::Rekey`, `SpaceState::rekey_due`, `Obligation::RekeyDue` (S15c2's rekey plans read
/// it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RekeyReason {
    /// An explicit "re-key space" (also the occasion to rotate `K_chunk`, section 5.5).
    Manual,
    /// A member joined a forward space: the key rotates on add.
    Joiner,
    /// A grant that could read was ended: the removed grantee knows the key.
    Removal,
}

words!(
    RekeyReason,
    "rekey reason",
    [Manual => "manual", Joiner => "joiner", Removal => "removal"]
);

/// One op of a space chain, with its arguments.
///
/// - `Create`: the first entry (position 0): the kind, the history mode, the member who becomes the
///   first manager, and the key id of the first epoch (`SpaceKey::key_id` of S15c1: the entry COMMITS
///   the epoch's key id).
/// - `Grant`: gives a grantee a role, with the owner's per-share cap and the payer; a grant to a
///   grantee who already holds one replaces it.
/// - `Revoke`: ends a grantee's grant (a grantee revoking its own is leaving).
/// - `Rekey`: commits the next epoch's key id.
/// - `History`: switches the history mode for the members who join later.
///
/// Implements planning/architecture.md section 5.9 ("Space chains ... `space_create`, `space_grant`,
/// `space_revoke`, `space_rekey`, `space_history`, signed by its managers") and section 5.5 (an epoch
/// counts only if a signed `space_rekey` (or `space_create`) entry commits to its key id). The brief's
/// raw `u64` epoch and `Hash32` key id are the keys crate's `Epoch` and `KeyId` (drift row
/// X-DELTA-W1S16B-3).
///
/// Caller: `SpaceEntry::op`, `admit`, `apply`; the glue that decodes a wire entry (CHAIN-GLUE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceOp {
    /// The first entry of a space chain.
    Create {
        /// The kind of the space.
        kind: SpaceKind,
        /// How the history is kept for later joiners.
        history: HistoryMode,
        /// The member who becomes the first manager.
        first_manager: MemberId,
        /// The key id of epoch 1.
        key_id: KeyId,
    },
    /// A grant (or the replacement of the grantee's grant).
    Grant {
        /// Who the grant is for.
        grantee: Grantee,
        /// The role.
        role: SpaceRole,
        /// The owner's per-share cap on what the grantee may write.
        cap_bytes: Option<ByteCount>,
        /// Whose quota the grantee's writes count against.
        payer: Payer,
    },
    /// The end of a grant.
    Revoke {
        /// Whose grant ends.
        grantee: Grantee,
    },
    /// The next epoch's key.
    Rekey {
        /// The new epoch: the current one plus one.
        epoch: Epoch,
        /// The key id of the new epoch's key.
        key_id: KeyId,
        /// Why the key rotates.
        reason: RekeyReason,
    },
    /// A switch of the history mode.
    History {
        /// The new mode.
        mode: HistoryMode,
    },
}

/// The name of an op, as the plan spells it (`space_create` ...).
///
/// Implements planning/architecture.md section 5.9 (the five ops of a space chain).
///
/// Caller: `SpaceOp::name`, `SpaceRefusal` (which op was refused), `SpaceNotice::LogAppended`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceOpName {
    /// `space_create`.
    Create,
    /// `space_grant`.
    Grant,
    /// `space_revoke`.
    Revoke,
    /// `space_rekey`.
    Rekey,
    /// `space_history`.
    History,
}

words!(
    SpaceOpName,
    "space op",
    [
        Create => "space_create",
        Grant => "space_grant",
        Revoke => "space_revoke",
        Rekey => "space_rekey",
        History => "space_history",
    ]
);

impl SpaceOp {
    /// The name of the op.
    pub fn name(&self) -> SpaceOpName {
        match self {
            SpaceOp::Create { .. } => SpaceOpName::Create,
            SpaceOp::Grant { .. } => SpaceOpName::Grant,
            SpaceOp::Revoke { .. } => SpaceOpName::Revoke,
            SpaceOp::Rekey { .. } => SpaceOpName::Rekey,
            SpaceOp::History { .. } => SpaceOpName::History,
        }
    }
}
