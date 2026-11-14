//! Whose quota a write counts against, and whether the cap allows it (planning/architecture.md section 5.4
//! `reg_grant`: `cap_bytes` and `payer`; section 6.10 S14 "Quota exhaustion through shares"; section 5.12
//! "External members ... The share caps and payer rule apply"; build unit S16, lane W1-S16b).
//!
//! S14: "A participant's writes count against the participant's own quota, with a per-share cap set by
//! the owner (the grant's `payer` and `cap_bytes`, 5.4)." [`charge`] is the decision the write path makes
//! for one write: the payer the grant names and the cap left, or a refusal because the cap would be
//! exceeded.
//!
//! The cap is a ceiling on what the writer may put into THIS space in all, whoever pays: `spent_on_share`
//! is what the writer has already written (the registry's tally), `bytes` is this write. A writer that has
//! several grants that write (directly and through groups) is charged under the one that leaves it the
//! most room, and on a tie under the one the grantee pays (drift row X-DELTA-W1S16B-9).

use azlin_seams::{mutant, ByteCount};

use crate::access::{applicable_grants, Principal};
use crate::grant::{Grant, Payer};
use crate::role::role_caps;
use crate::state::SpaceState;

/// Why a write is refused by the charge.
///
/// Implements planning/architecture.md section 6.10 S14 (the per-share cap) and section 5.4.
///
/// Caller: `Charge::Refused`; the write path (S14's `quota_exceeded` status).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ChargeRefusal {
    /// The write would take the writer past the cap the owner set for the share.
    #[error("the write would exceed the cap the owner set for the share")]
    CapExceeded,
    /// No grant in force lets the principal write.
    #[error("no grant in force lets the principal write")]
    NoWriteGrant,
    /// The space is archived: nothing is written to it.
    #[error("the space is archived")]
    Archived,
}

/// Whose quota a write is counted against.
///
/// `Grantee { cap_left }` carries what the cap leaves AFTER this write (`None`: the owner set no cap).
///
/// Implements planning/architecture.md section 5.4 (`payer`: "the grantee's writes count against the
/// grantee's own quota, capped per share by the owner") and section 6.10 S14.
///
/// Caller: `charge`; the write path (S14) charges the quota it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charge {
    /// The owner's quota pays.
    Owner,
    /// The grantee's own quota pays, up to the cap.
    Grantee {
        /// What the cap leaves after this write; `None` when the share has no cap.
        cap_left: Option<ByteCount>,
    },
    /// The write is refused.
    Refused(ChargeRefusal),
}

/// What a grant leaves its grantee to write: the cap minus what it already spent (nothing left is zero);
/// `None` for no cap.
fn room(grant: &Grant, spent_on_share: ByteCount) -> Option<ByteCount> {
    grant
        .cap_bytes
        .map(|cap| cap.saturating_sub(spent_on_share))
}

/// Whether `a` is a better grant to charge a write under than `b`: more room (no cap is the most), and on
/// a tie the one the grantee pays.
fn better(a: &Grant, b: &Grant, spent_on_share: ByteCount) -> bool {
    let most = !mutant!("spaces_charge_picks_least_room");
    match (room(a, spent_on_share), room(b, spent_on_share)) {
        (None, Some(_)) => most,
        (Some(_), None) => !most,
        (Some(room_a), Some(room_b)) if room_a != room_b => (room_a > room_b) == most,
        _ => a.payer == Payer::Grantee && b.payer != Payer::Grantee,
    }
}

/// The charge of one write of `bytes` by `writer`, who has already written `spent_on_share` into the
/// space: the payer its grant names, with the cap left after the write, or a refusal when no grant lets
/// it write or the write would pass the cap.
///
/// Implements planning/architecture.md section 6.10 S14 ("A participant's writes count against the
/// participant's own quota, with a per-share cap set by the owner"), section 5.4 (`cap_bytes`, `payer`)
/// and section 5.12 (an external member pays under the same rule).
///
/// Caller: the write path of the node (S14, through SPACES-APPLY): on every `pack_commit` and
/// `shard_put` into a space the principal does not own.
pub fn charge(
    state: &SpaceState,
    writer: &Principal,
    bytes: ByteCount,
    spent_on_share: ByteCount,
) -> Charge {
    if state.archived.is_some() && !mutant!("spaces_charge_archived_unchecked") {
        return Charge::Refused(ChargeRefusal::Archived);
    }
    let mut best: Option<&Grant> = None;
    for grant in applicable_grants(state, writer) {
        if !role_caps(grant.role).can_write() {
            continue;
        }
        best = match best {
            Some(current) if !better(grant, current, spent_on_share) => Some(current),
            _ => Some(grant),
        };
    }
    let Some(grant) = best else {
        return Charge::Refused(ChargeRefusal::NoWriteGrant);
    };
    let Some(after) = spent_on_share.checked_add(bytes) else {
        return Charge::Refused(ChargeRefusal::CapExceeded);
    };
    let cap_left = match grant.cap_bytes {
        None => None,
        Some(cap) => match cap.checked_sub(after) {
            Some(left) => {
                // Writing exactly up to the cap is allowed.
                if left.is_zero() && mutant!("spaces_charge_cap_off_by_one") {
                    return Charge::Refused(ChargeRefusal::CapExceeded);
                }
                Some(left)
            }
            None => {
                if !mutant!("spaces_charge_cap_unchecked") {
                    return Charge::Refused(ChargeRefusal::CapExceeded);
                }
                Some(ByteCount::ZERO)
            }
        },
    };
    let payer = if mutant!("spaces_charge_payer_swapped") {
        match grant.payer {
            Payer::Owner => Payer::Grantee,
            Payer::Grantee => Payer::Owner,
        }
    } else {
        grant.payer
    };
    match payer {
        Payer::Owner => Charge::Owner,
        Payer::Grantee => Charge::Grantee { cap_left },
    }
}
