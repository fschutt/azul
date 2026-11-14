//! The kinds the policy is keyed on. `AccountKind` (the four columns of the table of
//! planning/architecture.md section 5.10), `MemberKind` (a person or an app: a quorum counts people
//! only) and `PolicyEpoch` (the counter a policy change bumps) are the shared enums of
//! `azlin_seams::vocab` (COMMON-W1R R3b, conflict C-04: one definition for the registry, the policy,
//! the team log and the spaces). This module re-exports them, so that a lane that depends on this
//! crate names one path, and holds the one helper that reads a column of the table.
//!
//! Implements planning/architecture.md section 5.10 ("personal / family / team / business default")
//! and section 5.3 (`reg_account.kind`, `reg_member.kind`, `policy_epoch`).

pub use azlin_seams::{AccountKind, MemberKind, PolicyEpoch};

/// The value of a row of the table of planning/architecture.md section 5.10 for `kind`: the four
/// cells of the row in the plan's order (personal, family, team, business). The cells are given
/// out as they stand, so a row of `defaults.rs` reads like the row of the plan.
///
/// Caller: `Policy::defaults` (defaults.rs).
pub(crate) fn by_kind<T>(kind: AccountKind, personal: T, family: T, team: T, business: T) -> T {
    match kind {
        AccountKind::Personal => personal,
        AccountKind::Family => family,
        AccountKind::Team => team,
        AccountKind::Business => business,
    }
}
