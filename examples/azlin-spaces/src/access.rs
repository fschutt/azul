//! The access view: what a node checks on every connection (planning/architecture.md section 2.4 "the
//! space grant backed by a signed entry", section 5.4 `reg_grant`, section 5.15 M6 "capability grants"
//! "revoked like a link", section 5.6 "Wraps go to groups"; build unit S16, lane W1-S16b).
//!
//! [`access`] answers one question: what may this principal do in this space? A principal is a member of
//! the space's account (with every group it is in), a member of another account, or the holder of a
//! bearer capability. The answer is the UNION of the capabilities of every grant in force that names the
//! principal (directly or through one of its groups): a revoked grant gives nothing, from the entry that
//! revoked it on (a revocation is immediate: there is no cache in this crate to go stale).
//!
//! A CAPABILITY is a bearer secret: the node hashes the presented secret ([`capability_hash`]) and
//! compares the hash with every capability grant's stored hash in constant time (`subtle`), the whole
//! list every time, so the time of a refusal says nothing of how close a guess was. The secret never
//! leaves the `Principal` (whose `Debug` shows `Secret<32>(***)`): nothing in the state, the effects or
//! an error holds it.
//!
//! The GROUPS of a member principal are the caller's: the node resolves them from the team log's group
//! tree with nesting (a grant to a group reaches the members of its subgroups) and lists every group the
//! member is in. A member principal is built only from a credential of the space's own account; a member
//! of another account is a `Principal::External` and matches only the external grant that names both its
//! account and its member id.

use azlin_seams::{mutant, AccountId, GroupId, Hash32, MemberId, Secret, Stamp};
use subtle::ConstantTimeEq;

use crate::grant::{capability_hash, Grant, Grantee};
use crate::role::{role_caps, union, Caps};
use crate::state::SpaceState;

/// Who asks: a member of the space's account, a member of another account, or the holder of a bearer
/// capability.
///
/// Implements planning/architecture.md section 2.4 (what a node checks per connection), section 5.12
/// (external members) and section 5.15 M6 (the server sees a capability being used, not account B).
///
/// Caller: `access`, `charge`; the node (S9g's `check_connection` through SPACES-APPLY) builds it from the
/// verified credential and the presented capability.
#[derive(Debug)]
pub enum Principal {
    /// A member of the space's own account, with every group it is in (ancestors of its groups
    /// included).
    Member {
        /// The member.
        member: MemberId,
        /// Every group the member is in.
        groups: Vec<GroupId>,
    },
    /// A member of ANOTHER account.
    External {
        /// The member's own account.
        account: AccountId,
        /// The member in that account.
        member: MemberId,
    },
    /// The holder of a bearer capability: the secret it presents.
    Capability(Secret<32>),
}

/// Pushes the grant of `grantee` when it is in force.
fn push_active<'a>(state: &'a SpaceState, grantee: &Grantee, out: &mut Vec<&'a Grant>) {
    if let Some(grant) = state.grants.get(grantee) {
        if grant.is_active() || mutant!("spaces_access_ignores_revoked") {
            out.push(grant);
        }
    }
}

/// The capability grants the presented secret opens: its hash is compared with every stored hash in
/// constant time, none skipped.
fn capability_grants<'a>(state: &'a SpaceState, secret: &Secret<32>, out: &mut Vec<&'a Grant>) {
    let presented = capability_hash(secret);
    // The capabilities are the last variant of `Grantee`, so they are one contiguous run of the map.
    for (grantee, grant) in state.grants.range(Grantee::Capability(Hash32::ZERO)..) {
        let Grantee::Capability(stored) = grantee else {
            continue;
        };
        let equal = stored
            .as_bytes()
            .as_slice()
            .ct_eq(presented.as_bytes().as_slice());
        let opens = bool::from(equal) || mutant!("spaces_capability_any_secret_accepted");
        if opens && (grant.is_active() || mutant!("spaces_access_ignores_revoked")) {
            out.push(grant);
        }
    }
}

/// The grants in force that name the principal.
pub(crate) fn applicable_grants<'a>(
    state: &'a SpaceState,
    principal: &Principal,
) -> Vec<&'a Grant> {
    let mut grants: Vec<&Grant> = Vec::new();
    match principal {
        Principal::Member { member, groups } => {
            push_active(state, &Grantee::Member(*member), &mut grants);
            if !mutant!("spaces_access_groups_ignored") {
                for group in groups {
                    push_active(state, &Grantee::Group(*group), &mut grants);
                }
            }
        }
        Principal::External { account, member } => {
            if mutant!("spaces_access_external_ignores_account") {
                for (grantee, grant) in &state.grants {
                    if matches!(grantee, Grantee::External { member: other, .. } if other == member)
                        && grant.is_active()
                    {
                        grants.push(grant);
                    }
                }
            } else {
                push_active(
                    state,
                    &Grantee::External {
                        account: *account,
                        member: *member,
                    },
                    &mut grants,
                );
            }
        }
        Principal::Capability(secret) => capability_grants(state, secret, &mut grants),
    }
    grants
}

/// What the principal may do in the space now: the union of the capabilities of every grant in force
/// that names it. An archived space gives at most read access. A principal that no grant names gets
/// nothing, and so does a wrong secret.
///
/// Implements planning/architecture.md section 2.4 ("the space grant backed by a signed entry"), section
/// 5.4 (`reg_grant`), section 5.6 (grants to groups) and section 5.15 M6 (capability grants).
///
/// Caller: the node, on every connection that names a space (S9g's `check_connection`, through the
/// SPACES-APPLY glue, which maps `Caps` to the registry's `GrantCaps`, COMMON-W1R R14).
pub fn access(state: &SpaceState, principal: &Principal, now: Stamp) -> Caps {
    let mut caps = Caps::NONE;
    for grant in applicable_grants(state, principal) {
        caps = union(caps, role_caps(grant.role));
    }
    if let Some(archived_at) = state.archived {
        if archived_at <= now && !mutant!("spaces_access_archived_writable") {
            caps = caps.intersect(Caps::READ);
        }
    }
    caps
}
