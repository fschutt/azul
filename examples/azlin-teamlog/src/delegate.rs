//! Delegations: what a delegate key may sign (planning/architecture.md section 5.9 "`delegate` | admin:
//! names a key and the exact ops it may sign (e.g. the identity service: `member_suspend`, pending
//! members; the key agent: its approvals)", section 7.3 "Its authority is a `delegate` entry in the team
//! log, signed by admins: its key and the exact ops it may sign. Allowed: `member_suspend` and
//! pending-member creation. Never a grant, a role or a key wrap"; build unit S17, lane W1-S17c2).
//!
//! The set of ops a delegation may name is the table's `delegable` column (`authority::Row`): in v0
//! `member_suspend` and `member_invite`. The brief's third delegable item, "cosign of device_add", is not
//! built: a cosignature is a device's (`RawEntry::cosigs`), and a delegate key is no device (drift row of
//! lane W1-S17c). A delegation is live until its `expires` stamp (the leader's stamp, section 2.4: an
//! `expires` equal to `now` is over), and an empty op set revokes it at once.

use azlin_seams::{mutant, KeyId, Stamp};

use crate::admit::Refusal;
use crate::authority::row;
use crate::ops::OpName;
use crate::state::{Delegation, TeamState};

/// Whether a delegation may name this op: the op's row of the authority table says `delegable`.
///
/// Implements planning/architecture.md section 7.3 ("Allowed: `member_suspend` and pending-member
/// creation. Never a grant, a role or a key wrap").
///
/// Caller: `admit.rs` (`delegate`: the ops a new delegation names), `live_delegation`.
pub fn is_delegable(op: OpName) -> bool {
    row(op).delegable
}

/// The ops a delegation may name, in the order of the table.
///
/// Implements planning/architecture.md section 7.3.
///
/// Caller: the tests (`identity_service_delegation_cannot_name_role_grant_or_wrap`), lane S47a.
pub fn delegable_ops() -> Vec<OpName> {
    OpName::ALL
        .iter()
        .copied()
        .filter(|op| is_delegable(*op))
        .collect()
}

/// The delegation that lets `key` sign `op` at the leader's stamp `now`: the key has a delegation, the
/// delegation is not over, it names the op, and the op is one a delegation may name.
///
/// A key without a delegation covers nothing; an expired delegation is `DelegationExpired`; an op the
/// delegation does not name (or that no delegation may name, such as `delegate` itself) is
/// `DelegationDoesNotCover`.
///
/// Implements planning/architecture.md section 5.9 (`delegate`: "the exact ops it may sign") and
/// section 2.4 (time is the leader's stamp).
///
/// Caller: `admit.rs` (`resolve_signer`).
pub(crate) fn live_delegation<'a>(
    state: &'a TeamState,
    key: &KeyId,
    op: OpName,
    now: Stamp,
) -> Result<&'a Delegation, Refusal> {
    let delegation = state
        .delegation(key)
        .ok_or(Refusal::DelegationDoesNotCover { op })?;
    if !delegation.is_live_at(now) {
        return Err(Refusal::DelegationExpired);
    }
    if mutant!("teamlog_delegate_any_op") {
        return Ok(delegation);
    }
    if delegation.ops.contains(&op) && is_delegable(op) {
        Ok(delegation)
    } else {
        Err(Refusal::DelegationDoesNotCover { op })
    }
}
