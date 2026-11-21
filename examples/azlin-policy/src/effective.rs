//! The policy that really holds for a group, and the epoch a change of it counts under.
//!
//! A group's policy is a patch (`PolicyPatch`) on what the group inherits. [`effective`] walks the
//! tree from the account down: at each level it applies that level's patch to the policy it
//! inherits and CLAMPS the result to the inherited policy (the tighter of the two in every field,
//! `clamp`). So a child is never looser than any ancestor, even when an ancestor tightened after the
//! child's patch was made ("groups inherit and may only tighten"): the patch stays as it was set,
//! and the clamp is applied every time the policy is read.
//!
//! [`next_epoch`] is the epoch rule of section 5.11: "a policy change invalidates older
//! credentials", so the epoch moves exactly when the policy a credential was issued under changed.
//!
//! Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten"),
//! section 7.2 ("Policies (5.10) sit on the account or a group; subgroups inherit and may tighten,
//! never loosen") and section 5.11 (`policy_epoch`).

use crate::fields::Policy;
use crate::kind::PolicyEpoch;
use crate::patch::{apply, PolicyPatch};
use crate::tighten::clamp;

/// The policy of a group: `account` is the policy of the account, and `down_the_tree` are the
/// patches of the groups on the way from the account down to the group, the topmost first (a group
/// without a patch of its own contributes `PolicyPatch::none()` or nothing). The result is never
/// looser than the account's policy, nor than the policy at any level on the way: every level is
/// clamped to the level above it. The work is linear in the length of `down_the_tree`, which the
/// caller bounds (the depth of the group tree).
///
/// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten") and
/// section 7.2 (subgroups "may tighten, never loosen").
///
/// Seam: S17c1 (`azlin-teamlog` `AccountView::policy_of(group)`), S16c1, S16f, S16b (the policy
/// they check a link, a request, a grant against).
pub fn effective(account: &Policy, down_the_tree: &[&PolicyPatch]) -> Policy {
    let mut current = account.clone();
    for patch in down_the_tree {
        let child = apply(&current, patch);
        current = if azlin_seams::mutant!("policy_effective_skips_clamp") {
            child
        } else {
            clamp(&current, &child)
        };
    }
    current
}

/// The epoch counter cannot count another policy change.
///
/// Implements planning/architecture.md section 5.11 (`policy_epoch`).
///
/// Caller: `next_epoch`; Seam: S17c1 (`policy_set` refuses the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the policy epoch is at its largest number: no further policy change can be counted")]
pub struct EpochExhausted;

/// The epoch after a change of policy: `old` when the effective policy did not change (`before` and
/// `after` are equal: a patch that sets nothing, a patch that sets what is already there, or a
/// change under a group that a tighter ancestor already clamps), and the next epoch when it did. The
/// caller passes the effective policies it computed with [`effective`] (for the account, and for the
/// group the change was made under). The counter never wraps: at its largest number a change is an
/// error.
///
/// Implements planning/architecture.md section 5.11 (`policy_epoch`: "a policy change invalidates
/// older credentials").
///
/// Seam: S17c1 (`azlin-teamlog` bumps `reg_account.policy_epoch` when it applies a `policy_set`).
pub fn next_epoch(
    old: PolicyEpoch,
    before: &Policy,
    after: &Policy,
) -> Result<PolicyEpoch, EpochExhausted> {
    if before == after && !azlin_seams::mutant!("policy_epoch_always_bumps") {
        return Ok(old);
    }
    old.next().ok_or(EpochExhausted)
}
