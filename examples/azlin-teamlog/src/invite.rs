//! Invitations: the hash of the invite secret and the check of a `member_join` (planning/
//! architecture.md section 5.11 "Invites: a `member_invite` entry carries the hash of a secret; the link
//! `https://azlin.io/i/<id>#<secret>` travels on any channel. No separate invite table. A mailed invite
//! is a bearer secret anyone who reads the mailbox could use first, so the joiner's device shows its
//! fingerprint phrase and the inviter's device confirms it ... before the `member_join` entry takes
//! effect"; build unit S17, lane W1-S17c2).
//!
//! The confirmation is a COSIGNATURE of the inviter's device on the `member_join` entry (the authority
//! table v0): the join is signed by the joiner's new device, which holds the secret, and cosigned by an
//! Active device of the member who invited, who has seen the fingerprint phrase. An invitation made by a
//! delegate key (the identity service) has no inviter: an Active device of an owner or an admin confirms
//! it. The invitation is used up by the join (the member is Active, no longer Invited), so the same
//! secret opens nothing twice, and a secret opens only the member whose invitation carries its hash.

use azlin_seams::{hash32, DeviceId, Hash32, MemberId, Stamp};

use crate::admit::{Invalid, Refusal};
use crate::devices::PublicKeys;
use crate::members::MemberState;
use crate::ops::InviteSecret;
use crate::roles::is_admin_class;
use crate::state::{TeamState, MAX_DEVICES};

/// The domain of the invite hash: a `derive_key` context no other hash of the engine uses. A hash
/// domain, not a signature domain (drift row of lane W1-S17c).
///
/// Implements planning/architecture.md section 5.11 and section 2.5 (each structure has its own domain
/// string).
///
/// Caller: `invite_hash`.
pub const INVITE_HASH_DOMAIN: &str = "azlin-teamlog-invite-v1";

/// The hash of an invite secret: what the `member_invite` entry carries.
///
/// Implements planning/architecture.md section 5.11 ("a `member_invite` entry carries the hash of a
/// secret").
///
/// Caller: `check_join`, the joiner's and the inviter's devices (they make the hash of a fresh secret).
pub fn invite_hash(secret: &InviteSecret) -> Hash32 {
    hash32(INVITE_HASH_DOMAIN, &[&secret.0])
}

/// The arguments of a `member_join`, borrowed from the op.
pub(crate) struct JoinArgs<'a> {
    pub(crate) member: &'a MemberId,
    pub(crate) device: &'a DeviceId,
    pub(crate) member_keys: &'a PublicKeys,
    pub(crate) device_keys: &'a PublicKeys,
    pub(crate) secret: &'a InviteSecret,
}

/// Checks a `member_join` against the state: the member has an open invitation, the secret is its
/// preimage, an Active device of the inviter (or, for a delegate's invitation, of an owner or an admin)
/// cosigned, and the device id is free. The member whose device confirmed, for the entry's record.
///
/// Every problem of the invitation itself (no such member, not Invited, a wrong secret, no
/// confirmation) is `InviteInvalid`; a device id that is taken is `Invalid(IdTaken)`.
///
/// Implements planning/architecture.md section 5.11 (invites; "the inviter's device confirms it").
///
/// Caller: `admit.rs` (`member_join`).
pub(crate) fn check_join(
    state: &TeamState,
    args: &JoinArgs<'_>,
    cosigners: &[DeviceId],
    now: Stamp,
) -> Result<Vec<MemberId>, Refusal> {
    let member = state.member(args.member).ok_or(Refusal::InviteInvalid)?;
    if member.state != MemberState::Invited {
        return Err(Refusal::InviteInvalid);
    }
    let invite = member.invite.as_ref().ok_or(Refusal::InviteInvalid)?;
    if invite_hash(args.secret) != invite.hash {
        return Err(Refusal::InviteInvalid);
    }
    let confirmer = cosigners.iter().find_map(|id| {
        let device = state.device(id)?;
        if !device.state.is_active_at(now) {
            return None;
        }
        let candidate = state.member(&device.member)?;
        let right_one = match invite.inviter {
            Some(inviter) => candidate.id == inviter,
            None => is_admin_class(candidate.role),
        };
        if right_one && candidate.state == MemberState::Active {
            Some(candidate.id)
        } else {
            None
        }
    });
    let Some(confirmer) = confirmer else {
        return Err(Refusal::InviteInvalid);
    };
    if state.device(args.device).is_some() {
        return Err(Refusal::Invalid(Invalid::IdTaken));
    }
    if state.devices().count() >= MAX_DEVICES {
        return Err(Refusal::Invalid(Invalid::Limit("devices")));
    }
    for keys in [args.member_keys, args.device_keys] {
        if keys
            .pq
            .as_ref()
            .is_some_and(|pq| pq.len() > PublicKeys::PQ_MAX_LEN)
        {
            return Err(Refusal::Invalid(Invalid::Args(
                "a post-quantum key is too long",
            )));
        }
    }
    Ok(vec![confirmer])
}
