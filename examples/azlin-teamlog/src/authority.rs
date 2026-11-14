//! The authority table v0 as data (planning/architecture.md section 5.9 "Who may sign", section 7.2
//! "Admin roles, scoped to a group subtree", section 7.3 "Its authority is a `delegate` entry", section
//! 7.5, section 5.10 `admin_quorum`; build unit S17, lane W1-S17c2).
//!
//! The plan gives "who may sign" per op in a few words; the table of the brief of lane S17c ("AUTHORITY
//! TABLE v0", planner P4's reading) fills in what the plan leaves open, and the owner ratifies it
//! (ARCH29 9.13, X-D-54). THIS FILE IS THAT TABLE AS DATA: one [`Row`] for each op (who may sign it and
//! how far, whether the signer may also be a delegate key, which quorum it needs and when) and the role
//! assignment table. `admit.rs` reads it and adds the conditions that no row can say (the subject's
//! state, the last owner, a cycle in the tree); a changed table is a change of this file and of the
//! model of lane S47a, nothing else. `row` is total: a new `OpName` does not compile until it has a row.
//!
//! The roles: owner > admin > group admin > helpdesk > member > guest (`roles::rank`). Owner and admin
//! act on the whole account ([`Reach::Account`]); a group admin and helpdesk act in the subtree of their
//! `admin_scope` group ([`Reach::InScope`]).
//!
//! v0 readings where the plan is silent (each is a row of the drift register of lane W1-S17c):
//! - a signer never suspends, removes or changes the settings of a member ABOVE its own rank, and only an
//!   owner does so to an owner (the table's "never an owner unless by another owner with quorum" made
//!   general: a lower role must not be able to suspend a higher one with the help of a quorum);
//! - a group admin takes into its group nobody who belongs to another department (a member in no group,
//!   or already in the subtree, only);
//! - an admin's quorum may enrol a device for a member of no higher rank than its own;
//! - a `group_move` that would loosen the effective policy of a group of the moved subtree is for an owner
//!   or an admin with the quorum of a policy change, never for a group admin (plan 7.2: "subgroups inherit
//!   and may tighten, never loosen": a move changes what a group inherits without a `policy_set`;
//!   X-DELTA-W1S47A-2).

use azlin_policy::QuorumClass;

use crate::ops::OpName;
use crate::roles::Role;

/// How far a role acts.
///
/// Implements planning/architecture.md section 7.2 ("Owner and admin act on the whole account;
/// group_admin and helpdesk act in the subtree of their `admin_scope` group").
///
/// Caller: `Grant`, `admit.rs` (`reach_member`, `reach_group`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// The whole account.
    Account,
    /// The subtree of the signer's `admin_scope` group: the subject member belongs, directly or through
    /// nesting, to a group of it; a group as subject lies in it.
    InScope,
}

/// A role that may sign an op, and how far it reaches.
///
/// Implements planning/architecture.md section 5.9 (the table "Who may sign").
///
/// Caller: `Row::signers`, `admit.rs` (`gate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The role.
    pub role: Role,
    /// How far it reaches.
    pub reach: Reach,
}

/// When an op needs a quorum, and of which class (`azlin_policy::quorum_met`: the policy's
/// `admin_quorum` different people, the signer counting, devices of one person counting once).
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`: "admin changes, policy,
/// escrow, escrow_open, quorum enrollments") and section 5.9 ("with the policy's quorum").
///
/// Caller: `Row::quorum`, `admit.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuorumRule {
    /// No quorum: one person, the signer.
    Never,
    /// Always, of this class.
    Always(QuorumClass),
    /// When the subject member is an admin or an owner.
    SubjectIsAdminClass(QuorumClass),
    /// When the old or the new role is admin or owner.
    RoleChangeIsAdminClass(QuorumClass),
    /// Unless the member's own device signs (the quorum path only).
    NotOwnDevice(QuorumClass),
    /// For a managed member made private only.
    ManagedToPrivate(QuorumClass),
    /// When the effective policy of a group of the moved subtree would get looser (a move is the one
    /// op that changes what a group inherits without a `policy_set`: X-DELTA-W1S47A-2).
    MoveLoosensPolicy(QuorumClass),
}

/// The facts of one case that decide whether a [`QuorumRule`] asks for a quorum.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`).
///
/// Caller: `QuorumRule::class`, `admit.rs` (`require`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuorumFacts {
    /// The subject member is an admin or an owner.
    pub subject_is_admin_class: bool,
    /// The old or the new role of a `member_role` is admin or owner.
    pub role_change_is_admin_class: bool,
    /// The member's own device signs a `device_add` or `device_remove`.
    pub own_device: bool,
    /// A managed member is made private.
    pub managed_to_private: bool,
    /// A `group_move` would make the effective policy of a group of the moved subtree looser.
    pub move_loosens_policy: bool,
}

impl QuorumRule {
    /// The class of the quorum this rule asks for in a case, or `None` when it asks for none.
    ///
    /// Implements planning/architecture.md section 5.10 (row `admin_quorum`: "admin changes, policy,
    /// escrow, escrow_open, quorum enrollments") and section 5.9 ("with the policy's quorum").
    ///
    /// Caller: `admit.rs` (`require`), the matrix tests.
    pub fn class(self, facts: QuorumFacts) -> Option<QuorumClass> {
        match self {
            QuorumRule::Never => None,
            QuorumRule::Always(class) => Some(class),
            QuorumRule::SubjectIsAdminClass(class) => facts.subject_is_admin_class.then_some(class),
            QuorumRule::RoleChangeIsAdminClass(class) => {
                facts.role_change_is_admin_class.then_some(class)
            }
            QuorumRule::NotOwnDevice(class) => (!facts.own_device).then_some(class),
            QuorumRule::ManagedToPrivate(class) => facts.managed_to_private.then_some(class),
            QuorumRule::MoveLoosensPolicy(class) => facts.move_loosens_policy.then_some(class),
        }
    }
}

/// One row of the authority table.
///
/// Implements planning/architecture.md section 5.9 (the table of ops and who may sign).
///
/// Caller: `row`, `TABLE`, `admit.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The op.
    pub op: OpName,
    /// The roles that may sign it, with their reach. Empty for an op no role signs (`genesis`,
    /// `member_join`: the new device; `entitlement`: the payments key).
    pub signers: &'static [Grant],
    /// Whether an Active device of the SUBJECT member may sign it too (`device_add`, `device_remove`:
    /// "the member's own device").
    pub own_member: bool,
    /// Whether a delegate key may sign it, when its delegation names it (planning/architecture.md
    /// section 7.3: "Allowed: `member_suspend` and pending-member creation. Never a grant, a role or a
    /// key wrap").
    pub delegable: bool,
    /// The quorum.
    pub quorum: QuorumRule,
}

const fn account(role: Role) -> Grant {
    Grant {
        role,
        reach: Reach::Account,
    }
}

const fn scoped(role: Role) -> Grant {
    Grant {
        role,
        reach: Reach::InScope,
    }
}

/// Owner and admin.
const ADMINS: &[Grant] = &[account(Role::Owner), account(Role::Admin)];

/// Owner, admin, and a group admin in its scope.
const ADMINS_AND_GROUP_ADMINS: &[Grant] = &[
    account(Role::Owner),
    account(Role::Admin),
    scoped(Role::GroupAdmin),
];

/// Owner, admin, a group admin and helpdesk in their scope.
const ADMINS_GROUP_ADMINS_AND_HELPDESK: &[Grant] = &[
    account(Role::Owner),
    account(Role::Admin),
    scoped(Role::GroupAdmin),
    scoped(Role::Helpdesk),
];

/// Every member but a guest.
const EVERY_MEMBER: &[Grant] = &[
    account(Role::Owner),
    account(Role::Admin),
    account(Role::GroupAdmin),
    account(Role::Helpdesk),
    account(Role::Member),
];

/// No role.
const NOBODY: &[Grant] = &[];

const GENESIS: Row = Row {
    op: OpName::Genesis,
    signers: NOBODY,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

const DEVICE_ADD: Row = Row {
    op: OpName::DeviceAdd,
    signers: ADMINS,
    own_member: true,
    delegable: false,
    quorum: QuorumRule::NotOwnDevice(QuorumClass::QuorumEnrollment),
};

const DEVICE_REMOVE: Row = Row {
    op: OpName::DeviceRemove,
    signers: ADMINS,
    own_member: true,
    delegable: false,
    quorum: QuorumRule::SubjectIsAdminClass(QuorumClass::AdminChange),
};

const MEMBER_INVITE: Row = Row {
    op: OpName::MemberInvite,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: true,
    quorum: QuorumRule::Never,
};

const MEMBER_JOIN: Row = Row {
    op: OpName::MemberJoin,
    signers: NOBODY,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

const MEMBER_ROLE: Row = Row {
    op: OpName::MemberRole,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::RoleChangeIsAdminClass(QuorumClass::AdminChange),
};

const MEMBER_SUSPEND: Row = Row {
    op: OpName::MemberSuspend,
    signers: ADMINS_GROUP_ADMINS_AND_HELPDESK,
    own_member: false,
    delegable: true,
    quorum: QuorumRule::SubjectIsAdminClass(QuorumClass::AdminChange),
};

const MEMBER_REMOVE: Row = Row {
    op: OpName::MemberRemove,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::SubjectIsAdminClass(QuorumClass::AdminChange),
};

const MEMBER_MANAGED: Row = Row {
    op: OpName::MemberManaged,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::ManagedToPrivate(QuorumClass::EscrowChange),
};

const GROUP_CREATE: Row = Row {
    op: OpName::GroupCreate,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

const GROUP_MEMBER: Row = Row {
    op: OpName::GroupMember,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

const GROUP_MOVE: Row = Row {
    op: OpName::GroupMove,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::MoveLoosensPolicy(QuorumClass::PolicyChange),
};

const POLICY_SET: Row = Row {
    op: OpName::PolicySet,
    signers: ADMINS_AND_GROUP_ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::PolicyChange),
};

const ESCROW_SET: Row = Row {
    op: OpName::EscrowSet,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::EscrowChange),
};

const AUDIT_KEY_SET: Row = Row {
    op: OpName::AuditKeySet,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::PolicyChange),
};

const ADMIN_QUORUM_SET: Row = Row {
    op: OpName::AdminQuorumSet,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::PolicyChange),
};

const ESCROW_OPEN: Row = Row {
    op: OpName::EscrowOpen,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::EscrowOpen),
};

const DELEGATE: Row = Row {
    op: OpName::Delegate,
    signers: ADMINS,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Always(QuorumClass::AdminChange),
};

const ENTITLEMENT: Row = Row {
    op: OpName::Entitlement,
    signers: NOBODY,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

const APP: Row = Row {
    op: OpName::App,
    signers: EVERY_MEMBER,
    own_member: false,
    delegable: false,
    quorum: QuorumRule::Never,
};

/// The authority table v0, one row for each op, in the order of `OpName::ALL`.
///
/// Implements planning/architecture.md section 5.9 (the table "Who may sign"), section 7.2, section 7.3
/// and section 7.5, as read by the brief of lane S17c ("AUTHORITY TABLE v0").
///
/// Caller: `row`, the tests (`the_table_has_one_row_for_each_op`), lane S47a's model data.
pub const TABLE: [Row; 20] = [
    GENESIS,
    DEVICE_ADD,
    DEVICE_REMOVE,
    MEMBER_INVITE,
    MEMBER_JOIN,
    MEMBER_ROLE,
    MEMBER_SUSPEND,
    MEMBER_REMOVE,
    MEMBER_MANAGED,
    GROUP_CREATE,
    GROUP_MEMBER,
    GROUP_MOVE,
    POLICY_SET,
    ESCROW_SET,
    AUDIT_KEY_SET,
    ADMIN_QUORUM_SET,
    ESCROW_OPEN,
    DELEGATE,
    ENTITLEMENT,
    APP,
];

/// The row of an op. Total: every `OpName` has one (the `match` names them all).
///
/// Implements planning/architecture.md section 5.9.
///
/// Caller: `admit.rs`, `delegate.rs` (`is_delegable`).
pub fn row(op: OpName) -> &'static Row {
    match op {
        OpName::Genesis => &GENESIS,
        OpName::DeviceAdd => &DEVICE_ADD,
        OpName::DeviceRemove => &DEVICE_REMOVE,
        OpName::MemberInvite => &MEMBER_INVITE,
        OpName::MemberJoin => &MEMBER_JOIN,
        OpName::MemberRole => &MEMBER_ROLE,
        OpName::MemberSuspend => &MEMBER_SUSPEND,
        OpName::MemberRemove => &MEMBER_REMOVE,
        OpName::MemberManaged => &MEMBER_MANAGED,
        OpName::GroupCreate => &GROUP_CREATE,
        OpName::GroupMember => &GROUP_MEMBER,
        OpName::GroupMove => &GROUP_MOVE,
        OpName::PolicySet => &POLICY_SET,
        OpName::EscrowSet => &ESCROW_SET,
        OpName::AuditKeySet => &AUDIT_KEY_SET,
        OpName::AdminQuorumSet => &ADMIN_QUORUM_SET,
        OpName::EscrowOpen => &ESCROW_OPEN,
        OpName::Delegate => &DELEGATE,
        OpName::Entitlement => &ENTITLEMENT,
        OpName::App => &APP,
    }
}

/// Which subjects a role may change the role of.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 (`member_role`).
///
/// Caller: `Assignment::touches`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touch {
    /// Anyone, including another owner (with the quorum of an admin change).
    Anyone,
    /// Only a subject whose role is strictly below the signer's.
    StrictlyBelow,
}

/// What a role may do with `member_role`: the roles it may assign, and the subjects it may touch.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 (`member_role`: "owner:
/// any role incl. owner/admin; admin: group_admin, helpdesk, member, guest for subjects in scope whose
/// role is below admin; group_admin: member, guest in scope").
///
/// Caller: `ROLE_ASSIGNMENT`, `role_assignable`, `role_touchable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assignment {
    /// The signer's role.
    pub signer: Role,
    /// The roles it may give.
    pub assigns: &'static [Role],
    /// The subjects it may change.
    pub touches: Touch,
}

/// The role assignment table v0: only these three roles change a member's role.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 (`member_role`).
///
/// Caller: `role_assignable`, `role_touchable`, the tests.
pub const ROLE_ASSIGNMENT: [Assignment; 3] = [
    Assignment {
        signer: Role::Owner,
        assigns: &[
            Role::Owner,
            Role::Admin,
            Role::GroupAdmin,
            Role::Helpdesk,
            Role::Member,
            Role::Guest,
        ],
        touches: Touch::Anyone,
    },
    Assignment {
        signer: Role::Admin,
        assigns: &[Role::GroupAdmin, Role::Helpdesk, Role::Member, Role::Guest],
        touches: Touch::StrictlyBelow,
    },
    Assignment {
        signer: Role::GroupAdmin,
        assigns: &[Role::Member, Role::Guest],
        touches: Touch::StrictlyBelow,
    },
];

/// Whether a signer of this role may give `new_role` to a member.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 (`member_role`).
///
/// Caller: `admit.rs` (`member_role`, guarded by `teamlog_role_assign_unbounded`).
pub fn role_assignable(signer: Role, new_role: Role) -> bool {
    ROLE_ASSIGNMENT
        .iter()
        .find(|assignment| assignment.signer == signer)
        .is_some_and(|assignment| assignment.assigns.contains(&new_role))
}

/// Whether a signer of this role may change the role of a member who has `subject_role`.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 (`member_role`).
///
/// Caller: `admit.rs` (`member_role`, guarded by `teamlog_role_assign_unbounded`).
pub fn role_touchable(signer: Role, subject_role: Role) -> bool {
    ROLE_ASSIGNMENT
        .iter()
        .find(|assignment| assignment.signer == signer)
        .is_some_and(|assignment| match assignment.touches {
            Touch::Anyone => true,
            Touch::StrictlyBelow => crate::roles::outranks(signer, subject_role),
        })
}
