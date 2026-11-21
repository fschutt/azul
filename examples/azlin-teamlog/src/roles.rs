//! Roles and their order of privilege (planning/architecture.md section 7.2 "The model: RBAC on a
//! group hierarchy", section 5.3 `reg_member.role`; build unit S17, lane W1-S17c1).
//!
//! `Role` IS `azlin_seams::AccountRole` (COMMON-W1R R3b, P-S17c-1): the six words of the registry's
//! `role` column. Its declaration order is for maps and sorting only and is NOT a privilege order: the
//! privilege order is [`rank`] (owner > admin > group_admin > helpdesk > member > guest, the order of
//! the authority table v0 of the brief of lane S17c). Who may do what with a role is the authority
//! table's (`authority.rs`); this file holds only the facts about a role that no table can change.

pub use azlin_seams::{AccountRole as Role, MemberKind};

/// The place of a role in the order of privilege: owner 5, admin 4, group admin 3, helpdesk 2,
/// member 1, guest 0.
///
/// Implements planning/architecture.md section 7.2 (the four admin roles and the two below them,
/// "Roles by privilege: owner > admin > group_admin > helpdesk > member > guest" of the authority
/// table v0).
///
/// Caller: `outranks`, `at_most`; `authority.rs` and `admit.rs` (the rank rules of S17c2).
pub const fn rank(role: Role) -> u8 {
    match role {
        Role::Owner => 5,
        Role::Admin => 4,
        Role::GroupAdmin => 3,
        Role::Helpdesk => 2,
        Role::Member => 1,
        Role::Guest => 0,
    }
}

/// Whether `a` has a higher rank than `b`.
///
/// Implements planning/architecture.md section 7.2 (privilege order).
///
/// Caller: `admit.rs` (the roles a signer may touch or assign).
pub const fn outranks(a: Role, b: Role) -> bool {
    rank(a) > rank(b)
}

/// Whether `subject` has no higher rank than `signer`: the signer does not act on somebody above it.
///
/// Implements planning/architecture.md section 7.2 (privilege order; the v0 rule that a signer never
/// suspends, removes or changes the settings of a member above it: drift row of lane W1-S17c).
///
/// Caller: `admit.rs` (`member_suspend`, `member_remove`, `member_managed`, `device_remove`).
pub const fn at_most(subject: Role, signer: Role) -> bool {
    rank(subject) <= rank(signer)
}

/// Whether the role acts account-wide: owner and admin ("Owner and admin act on the whole account").
///
/// Implements planning/architecture.md section 7.2 (owner and admin are not scoped to a group).
///
/// Caller: `state.rs` (`active_owner_count`), `admit.rs` (scope and quorum eligibility).
pub const fn is_admin_class(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

/// Whether the role is scoped to a group subtree and so needs an `admin_scope` group: group admin and
/// helpdesk. The other four roles have no scope.
///
/// Implements planning/architecture.md section 5.3 (`reg_member.admin_scope`: "group subtree, for
/// group admins and helpdesk").
///
/// Caller: `admit.rs` (`member_role`, `member_invite`), `apply.rs` (clears the scope of another role).
pub const fn needs_scope(role: Role) -> bool {
    matches!(role, Role::GroupAdmin | Role::Helpdesk)
}
