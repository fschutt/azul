//! The authority matrix (check 4 of the brief): the authority table v0 transcribed as DATA in THIS file,
//! from the brief's table and not from `authority.rs`, run against the production check. Each cell is an
//! op, the role of the signer, the relation of the subject to the signer (in scope, out of scope, the
//! signer itself, a subject of a higher, an equal and a lower role) and whether the quorum is met; each
//! cell asserts the verdict AND the reason of the refusal.
//!
//! The expectation is `expected`: a second implementation of the table, written the way the brief
//! states it (role gate, scope, rank, quorum), with its own `rank`. A cell whose verdict differs names
//! its op, its signer's role, its relation and the quorum, so a changed table is one red cell.

use std::collections::BTreeSet;

use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::Span;

use super::kit::{device, group, key, member};
use super::signed::{actor_of, subject_of, universe, SignedKit, ROLES};
use crate::admit::{Invalid, Refusal};
use crate::ops::{EscrowReason, Op, OpName, PolicyScope};
use crate::proposal::{AssertionVerdict, Signer};
use crate::roles::Role;

/// How the subject relates to the signer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    InScope,
    OutOfScope,
    SelfSubject,
    Higher,
    Equal,
    Lower,
}

const RELS: [Rel; 6] = [
    Rel::InScope,
    Rel::OutOfScope,
    Rel::SelfSubject,
    Rel::Higher,
    Rel::Equal,
    Rel::Lower,
];

/// The ops of the matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Role,
    Suspend,
    Remove,
    Managed,
    DeviceAdd,
    DeviceRemove,
    EscrowOpen,
    GroupMember,
    GroupCreate,
    GroupMove,
    PolicyGroup,
    PolicyAccount,
    EscrowSet,
    AuditKey,
    AdminQuorum,
    Delegate,
    Invite,
}

/// What the table says about a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    Role,
    Scope,
    Quorum,
    Assertion,
    State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Admit,
    Refuse(Why),
}

/// The order of privilege, typed here from the brief ("owner > admin > group_admin > helpdesk > member >
/// guest").
fn rank(role: Role) -> u8 {
    match role {
        Role::Owner => 5,
        Role::Admin => 4,
        Role::GroupAdmin => 3,
        Role::Helpdesk => 2,
        Role::Member => 1,
        Role::Guest => 0,
    }
}

fn admin_class(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

/// One cell of the matrix.
#[derive(Clone, Copy, Debug)]
struct Cell {
    kind: Kind,
    actor: Role,
    rel: Rel,
    subject_role: Role,
    in_scope: bool,
    is_self: bool,
    met: bool,
}

/// The table v0, as the brief states it. The order of the checks is the order of the sentences of the
/// brief's table and of `admit.rs`'s documentation: the role gate, the scope, the rank, the quorum.
fn expected(c: &Cell) -> Expect {
    use Expect::{Admit, Refuse};
    let a = c.actor;
    let s = c.subject_role;
    // Owner and admin act on the whole account; group_admin and helpdesk act in their subtree.
    let scoped = matches!(a, Role::GroupAdmin | Role::Helpdesk);
    let scope_ok = !scoped || c.in_scope;
    let can = |roles: &[Role]| roles.contains(&a);
    match c.kind {
        // member_role | owner: any role; admin: below admin; group_admin: member, guest in scope |
        // never the signer's own role | AdminChange when the old or new role is admin or owner.
        Kind::Role => {
            if !can(&[Role::Owner, Role::Admin, Role::GroupAdmin]) {
                return Refuse(Why::Role);
            }
            if c.is_self {
                return Refuse(Why::Role);
            }
            if !scope_ok {
                return Refuse(Why::Scope);
            }
            let touches = match a {
                Role::Owner => true,
                Role::Admin => rank(s) < rank(Role::Admin),
                Role::GroupAdmin => rank(s) < rank(Role::GroupAdmin),
                _ => false,
            };
            if !touches {
                return Refuse(Why::Role);
            }
            if admin_class(s) && !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // member_suspend | admin, group_admin, helpdesk in scope | never an owner unless by another
        // owner (a signer acts on nobody above it) | AdminChange for an admin or owner subject.
        Kind::Suspend => {
            if !can(&[Role::Owner, Role::Admin, Role::GroupAdmin, Role::Helpdesk]) {
                return Refuse(Why::Role);
            }
            if !scope_ok {
                return Refuse(Why::Scope);
            }
            if rank(s) > rank(a) || (s == Role::Owner && c.is_self) {
                return Refuse(Why::Role);
            }
            if admin_class(s) && !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // member_remove | admin, group_admin in scope (helpdesk never) | AdminChange for an admin or
        // owner subject.
        Kind::Remove => {
            if !can(&[Role::Owner, Role::Admin, Role::GroupAdmin]) {
                return Refuse(Why::Role);
            }
            if !scope_ok {
                return Refuse(Why::Scope);
            }
            if rank(s) > rank(a) {
                return Refuse(Why::Role);
            }
            if admin_class(s) && !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // member_managed | admin in scope | managed to private needs the quorum (EscrowChange).
        Kind::Managed => {
            if !can(&[Role::Owner, Role::Admin]) {
                return Refuse(Why::Role);
            }
            if rank(s) > rank(a) {
                return Refuse(Why::Role);
            }
            if !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // device_add | an Active device of the same member; or admins by quorum (a member of no higher
        // rank than the admins').
        Kind::DeviceAdd => {
            if c.is_self {
                return Admit;
            }
            if !can(&[Role::Owner, Role::Admin]) {
                return Refuse(Why::Role);
            }
            if rank(s) > rank(a) {
                return Refuse(Why::Role);
            }
            if !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // device_remove | an Active device of the same member; or an admin in scope | removing a device
        // of an admin or owner by someone else: AdminChange.
        Kind::DeviceRemove => {
            if c.is_self {
                return Admit;
            }
            if !can(&[Role::Owner, Role::Admin]) {
                return Refuse(Why::Role);
            }
            if rank(s) > rank(a) {
                return Refuse(Why::Role);
            }
            if admin_class(s) && !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // escrow_open | quorum of admins, each with a passkey assertion with user verification.
        Kind::EscrowOpen => {
            if !can(&[Role::Owner, Role::Admin]) {
                return Refuse(Why::Role);
            }
            if !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // group_create, group_member, group_move | admin; group_admin in scope.
        Kind::GroupMember | Kind::GroupCreate | Kind::GroupMove | Kind::Invite => {
            if !can(&[Role::Owner, Role::Admin, Role::GroupAdmin]) {
                return Refuse(Why::Role);
            }
            if a == Role::GroupAdmin && !c.in_scope {
                return Refuse(Why::Scope);
            }
            Admit
        }
        // policy_set of a group | admin; group_admin in scope | PolicyChange.
        Kind::PolicyGroup => {
            if !can(&[Role::Owner, Role::Admin, Role::GroupAdmin]) {
                return Refuse(Why::Role);
            }
            if a == Role::GroupAdmin && !c.in_scope {
                return Refuse(Why::Scope);
            }
            if !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
        // policy_set of the account, escrow_set, audit_key_set, admin_quorum_set, delegate | admin |
        // PolicyChange, EscrowChange, AdminChange.
        Kind::PolicyAccount
        | Kind::EscrowSet
        | Kind::AuditKey
        | Kind::AdminQuorum
        | Kind::Delegate => {
            if !can(&[Role::Owner, Role::Admin]) {
                return Refuse(Why::Role);
            }
            if !c.met {
                return Refuse(Why::Quorum);
            }
            Admit
        }
    }
}

/// What the production check said, in the words of the table.
fn actual(result: Result<(), Refusal>) -> Result<(), Why> {
    match result {
        Ok(()) => Ok(()),
        Err(Refusal::RoleMaySignNo { .. }) => Err(Why::Role),
        Err(Refusal::OutOfScope { .. }) => Err(Why::Scope),
        Err(Refusal::QuorumShort { .. }) => Err(Why::Quorum),
        Err(Refusal::AssertionMissing) => Err(Why::Assertion),
        Err(Refusal::Invalid(Invalid::WrongMemberState(_))) => Err(Why::State),
        Err(other) => panic!("a refusal the matrix has no word for: {other:?}"),
    }
}

/// The subject of a cell: the member number, its role, and whether it is in the scope of the group
/// admin and helpdesk.
fn subject_for(actor: Role, rel: Rel) -> Option<(u8, Role, bool)> {
    let index = ROLES.iter().position(|r| *r == actor).expect("a role");
    let at_rank = |role: Role| (subject_of(role, true), role, true);
    match rel {
        Rel::InScope => Some((subject_of(Role::Member, true), Role::Member, true)),
        Rel::OutOfScope => Some((subject_of(Role::Member, false), Role::Member, false)),
        // The signer itself: owner, admin, member and guest reach everything; the group admin and
        // helpdesk members belong to group 2 (their own scope).
        Rel::SelfSubject => Some((actor_of(actor), actor, true)),
        Rel::Higher => index
            .checked_sub(1)
            .and_then(|i| ROLES.get(i))
            .map(|r| at_rank(*r)),
        Rel::Equal => Some(at_rank(actor)),
        Rel::Lower => ROLES.get(index + 1).map(|r| at_rank(*r)),
    }
}

/// The entry of a cell, signed by the actor (with the cosigner when the quorum is met).
fn entry_of(kit: &SignedKit, c: &Cell, subject: u8) -> crate::proposal::RawEntry {
    let device_n = actor_of(c.actor);
    let cosigners: &[u8] = if c.met { &[4] } else { &[] };
    let new_role = if c.subject_role == Role::Guest {
        Role::Member
    } else {
        Role::Guest
    };
    let op = match c.kind {
        Kind::Role => Op::MemberRole {
            member: member(subject),
            role: new_role,
            admin_scope: None,
        },
        Kind::Suspend => Op::MemberSuspend {
            member: member(subject),
        },
        Kind::Remove => Op::MemberRemove {
            member: member(subject),
        },
        Kind::Managed => Op::MemberManaged {
            member: member(subject),
            managed: false,
        },
        Kind::DeviceAdd => Op::DeviceAdd {
            member: member(subject),
            device: device(100),
            keys: super::signed::dkeys(100),
        },
        Kind::DeviceRemove => Op::DeviceRemove {
            device: device(subject),
        },
        Kind::EscrowOpen => Op::EscrowOpen {
            member: member(subject),
            reason: EscrowReason::LegalRequest,
        },
        Kind::GroupMember => Op::GroupMember {
            group: group(if c.in_scope { 3 } else { 4 }),
            member: member(40),
            present: true,
        },
        Kind::GroupCreate => Op::GroupCreate {
            group: group(60),
            parent: Some(group(if c.in_scope { 3 } else { 4 })),
        },
        Kind::GroupMove => {
            if c.in_scope {
                Op::GroupMove {
                    group: group(3),
                    new_parent: group(2),
                }
            } else {
                Op::GroupMove {
                    group: group(4),
                    new_parent: group(1),
                }
            }
        }
        Kind::PolicyGroup => Op::PolicySet {
            scope: PolicyScope::Group(group(if c.in_scope { 3 } else { 4 })),
            patch: PolicyPatch {
                credential_ttl: Some(Span::hours(6)),
                ..PolicyPatch::none()
            },
        },
        Kind::PolicyAccount => Op::PolicySet {
            scope: PolicyScope::Account,
            patch: PolicyPatch {
                credential_ttl: Some(Span::hours(6)),
                ..PolicyPatch::none()
            },
        },
        Kind::EscrowSet => Op::EscrowSet {
            escrow_pub: [9; 32],
            holders: [member(1), member(2)].into_iter().collect(),
        },
        Kind::AuditKey => Op::AuditKeySet { audit_pub: [8; 32] },
        Kind::AdminQuorum => Op::AdminQuorumSet {
            quorum: AdminQuorum::TWO,
        },
        Kind::Delegate => Op::Delegate {
            key: key(40),
            ops: [OpName::MemberSuspend].into_iter().collect::<BTreeSet<_>>(),
            expires: None,
        },
        Kind::Invite => Op::MemberInvite {
            member: member(70),
            kind: azlin_seams::MemberKind::Person,
            role: Role::Member,
            invite_hash: crate::invite::invite_hash(&crate::ops::InviteSecret([70; 32])),
            groups: [group(if c.in_scope { 3 } else { 4 })]
                .into_iter()
                .collect(),
            app: None,
        },
    };
    let assertions = if matches!(c.kind, Kind::EscrowOpen) {
        let mut all = vec![AssertionVerdict {
            key: device(device_n),
            user_verified: true,
            challenge_matches: true,
            rp_ok: true,
        }];
        if c.met {
            all.push(AssertionVerdict {
                key: device(4),
                user_verified: true,
                challenge_matches: true,
                rp_ok: true,
            });
        }
        all
    } else {
        Vec::new()
    };
    kit.entry(
        Signer::Device(device(device_n)),
        op,
        cosigners,
        assertions,
        None,
    )
}

/// The kinds that ask for a subject member, with the relations that make sense for them.
const SUBJECT_KINDS: [Kind; 7] = [
    Kind::Role,
    Kind::Suspend,
    Kind::Remove,
    Kind::Managed,
    Kind::DeviceAdd,
    Kind::DeviceRemove,
    Kind::EscrowOpen,
];

/// The kinds that act on a group (in scope or out of scope of the group admin).
const GROUP_KINDS: [Kind; 5] = [
    Kind::GroupMember,
    Kind::GroupCreate,
    Kind::GroupMove,
    Kind::PolicyGroup,
    Kind::Invite,
];

/// The kinds that act on the account.
const ACCOUNT_KINDS: [Kind; 5] = [
    Kind::PolicyAccount,
    Kind::EscrowSet,
    Kind::AuditKey,
    Kind::AdminQuorum,
    Kind::Delegate,
];

/// Whether a kind asks for a quorum when the cell says it is met or short: only then are both columns
/// distinct cells.
fn has_quorum(kind: Kind) -> bool {
    !matches!(
        kind,
        Kind::GroupMember | Kind::GroupCreate | Kind::GroupMove | Kind::Invite
    )
}

/// Every cell of the matrix.
fn cells() -> Vec<(Cell, u8)> {
    let mut all = Vec::new();
    for actor in ROLES {
        for kind in SUBJECT_KINDS {
            for rel in RELS {
                let Some((subject, subject_role, in_scope)) = subject_for(actor, rel) else {
                    continue;
                };
                for met in [false, true] {
                    all.push((
                        Cell {
                            kind,
                            actor,
                            rel,
                            subject_role,
                            in_scope,
                            is_self: rel == Rel::SelfSubject,
                            met,
                        },
                        subject,
                    ));
                }
            }
        }
        for kind in GROUP_KINDS {
            for in_scope in [true, false] {
                let rel = if in_scope {
                    Rel::InScope
                } else {
                    Rel::OutOfScope
                };
                for met in [false, true] {
                    if !has_quorum(kind) && met {
                        continue;
                    }
                    all.push((
                        Cell {
                            kind,
                            actor,
                            rel,
                            subject_role: Role::Member,
                            in_scope,
                            is_self: false,
                            met,
                        },
                        subject_of(Role::Member, in_scope),
                    ));
                }
            }
        }
        for kind in ACCOUNT_KINDS {
            for met in [false, true] {
                all.push((
                    Cell {
                        kind,
                        actor,
                        rel: Rel::InScope,
                        subject_role: Role::Member,
                        in_scope: true,
                        is_self: false,
                        met,
                    },
                    subject_of(Role::Member, true),
                ));
            }
        }
    }
    all
}

#[test]
fn the_matrix_has_at_least_four_hundred_cells_and_covers_every_op_role_and_relation() {
    let all = cells();
    assert!(all.len() >= 400, "{} cells", all.len());
    let mut kinds = BTreeSet::new();
    let mut roles = BTreeSet::new();
    let mut rels = BTreeSet::new();
    let mut verdicts = BTreeSet::new();
    for (cell, _) in &all {
        kinds.insert(format!("{:?}", cell.kind));
        roles.insert(format!("{:?}", cell.actor));
        rels.insert(format!("{:?}", cell.rel));
        verdicts.insert(format!("{:?}", expected(cell)));
    }
    assert_eq!(kinds.len(), 17);
    assert_eq!(roles.len(), 6);
    assert_eq!(rels.len(), 6);
    // Every refusal word and the admit appear in the expectations: the matrix is not all one verdict.
    for word in ["Admit", "Refuse(Role)", "Refuse(Scope)", "Refuse(Quorum)"] {
        assert!(verdicts.contains(word), "{word} is expected somewhere");
    }
}

#[test]
fn matrix_every_cell_matches_the_table() {
    let kit = universe();
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (cell, subject) in cells() {
        let entry = entry_of(&kit, &cell, subject);
        let got = actual(kit.verdict(&entry));
        let want = expected(&cell);
        let agrees = match (got, want) {
            (Ok(()), Expect::Admit) => true,
            (Err(why), Expect::Refuse(expected_why)) => why == expected_why,
            _ => false,
        };
        if !agrees {
            failures.push(format!(
                "matrix_{:?}_{:?}_{:?}_{}: expected {:?}, got {:?}",
                cell.kind,
                cell.actor,
                cell.rel,
                if cell.met {
                    "quorum_met"
                } else {
                    "quorum_short"
                },
                want,
                got
            ));
        }
        checked += 1;
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} cells differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked >= 400);
}

/// A cell in particular, by name, so that a red matrix names its cell and a reader can run it alone.
fn cell_by_name(name: &str) -> Option<(Cell, u8)> {
    cells().into_iter().find(|(cell, _)| {
        format!(
            "matrix_{:?}_{:?}_{:?}_{}",
            cell.kind,
            cell.actor,
            cell.rel,
            if cell.met {
                "quorum_met"
            } else {
                "quorum_short"
            }
        ) == name
    })
}

#[test]
fn matrix_admin_member_role_in_scope_lower_quorum_short() {
    // An admin changes the role of a lower subject (a group admin becomes a guest): the subject is no
    // admin and neither is the new role, so no quorum is asked for and the cell is admitted whether the
    // quorum is "met" or "short" (a one-person change).
    let kit = universe();
    let (cell, subject) =
        cell_by_name("matrix_Role_Admin_Lower_quorum_short").expect("the cell exists");
    assert_eq!(subject, subject_of(Role::GroupAdmin, true));
    assert_eq!(expected(&cell), Expect::Admit);
    assert_eq!(actual(kit.verdict(&entry_of(&kit, &cell, subject))), Ok(()));
}

#[test]
fn matrix_group_admin_cannot_touch_an_admin_even_with_a_quorum() {
    let kit = universe();
    let (cell, subject) =
        cell_by_name("matrix_Suspend_GroupAdmin_Higher_quorum_met").expect("the cell exists");
    assert_eq!(cell.subject_role, Role::Admin);
    assert_eq!(expected(&cell), Expect::Refuse(Why::Role));
    assert_eq!(
        actual(kit.verdict(&entry_of(&kit, &cell, subject))),
        Err(Why::Role)
    );
}

#[test]
fn the_signer_of_a_cell_is_the_actor_and_nobody_else() {
    // Sanity of the universe: each actor exists with the role of its number, and each subject is in the
    // group the matrix says.
    let kit = universe();
    for role in ROLES {
        let actor = kit.state.member(&member(actor_of(role))).expect("an actor");
        assert_eq!(actor.role, role);
        assert!(kit.state.device(&device(actor_of(role))).is_some());
        for in_scope in [true, false] {
            let subject = kit
                .state
                .member(&member(subject_of(role, in_scope)))
                .expect("a subject");
            assert_eq!(subject.role, role);
            let groups = kit
                .state
                .groups()
                .groups_of(&subject.id)
                .expect("in a group");
            assert!(groups.contains(&group(if in_scope { 3 } else { 4 })));
        }
    }
    assert_eq!(kit.state.policy().admin_quorum, AdminQuorum::TWO);
    assert!(kit.state.members().all(|m| m.managed));
    assert_eq!(kit.state.active_owner_count(None), 4);
}

// ---- one named test for each (op, signer role, relation) of the matrix (check 4 of the brief) ----------
//
// `matrix_<op>_<signer role>_<relation>`: each test runs the cells of its triple (quorum met and short)
// against the production check and asserts the verdict and the class of the refusal. The list below is
// written out by a small generator from the same loops as `cells()`; `every_cell_has_its_named_test`
// compares the two, so a cell without a test, or a test without a cell, is a red test.

/// The word of an op of the matrix in a test name.
fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Role => "role",
        Kind::Suspend => "suspend",
        Kind::Remove => "remove",
        Kind::Managed => "managed",
        Kind::DeviceAdd => "device_add",
        Kind::DeviceRemove => "device_remove",
        Kind::EscrowOpen => "escrow_open",
        Kind::GroupMember => "group_member",
        Kind::GroupCreate => "group_create",
        Kind::GroupMove => "group_move",
        Kind::PolicyGroup => "policy_group",
        Kind::PolicyAccount => "policy_account",
        Kind::EscrowSet => "escrow_set",
        Kind::AuditKey => "audit_key",
        Kind::AdminQuorum => "admin_quorum",
        Kind::Delegate => "delegate",
        Kind::Invite => "invite",
    }
}

/// The word of a relation in a test name. An op on the account has no subject: its one relation is
/// "account".
fn rel_word(kind: Kind, rel: Rel) -> &'static str {
    if ACCOUNT_KINDS.contains(&kind) {
        return "account";
    }
    match rel {
        Rel::InScope => "in_scope",
        Rel::OutOfScope => "out_of_scope",
        Rel::SelfSubject => "self",
        Rel::Higher => "higher",
        Rel::Equal => "equal",
        Rel::Lower => "lower",
    }
}

/// Runs the cells of one triple (the quorum met and short) and asserts each one.
fn check_triple(kind: Kind, actor: Role, rel: Rel) {
    let kit = universe();
    let mut seen = 0usize;
    for (cell, subject) in cells() {
        if cell.kind != kind || cell.actor != actor || cell.rel != rel {
            continue;
        }
        seen += 1;
        let got = actual(kit.verdict(&entry_of(&kit, &cell, subject)));
        let want = expected(&cell);
        let agrees = match (got, want) {
            (Ok(()), Expect::Admit) => true,
            (Err(why), Expect::Refuse(expected_why)) => why == expected_why,
            _ => false,
        };
        assert!(
            agrees,
            "matrix_{}_{}_{} (quorum {}): expected {want:?}, got {got:?}",
            kind_word(kind),
            actor.as_str(),
            rel_word(kind, rel),
            if cell.met { "met" } else { "short" },
        );
    }
    assert!(seen >= 1, "the triple has no cell");
}

macro_rules! named_cells {
    ($($name:ident: $kind:ident $actor:ident $rel:ident;)*) => {
        $(
            #[test]
            fn $name() {
                check_triple(Kind::$kind, Role::$actor, Rel::$rel);
            }
        )*

        /// The names of the generated tests.
        const NAMED: &[&str] = &[$(stringify!($name)),*];
    };
}

named_cells! {
    matrix_role_owner_in_scope: Role Owner InScope;
    matrix_role_owner_out_of_scope: Role Owner OutOfScope;
    matrix_role_owner_self: Role Owner SelfSubject;
    matrix_role_owner_equal: Role Owner Equal;
    matrix_role_owner_lower: Role Owner Lower;
    matrix_suspend_owner_in_scope: Suspend Owner InScope;
    matrix_suspend_owner_out_of_scope: Suspend Owner OutOfScope;
    matrix_suspend_owner_self: Suspend Owner SelfSubject;
    matrix_suspend_owner_equal: Suspend Owner Equal;
    matrix_suspend_owner_lower: Suspend Owner Lower;
    matrix_remove_owner_in_scope: Remove Owner InScope;
    matrix_remove_owner_out_of_scope: Remove Owner OutOfScope;
    matrix_remove_owner_self: Remove Owner SelfSubject;
    matrix_remove_owner_equal: Remove Owner Equal;
    matrix_remove_owner_lower: Remove Owner Lower;
    matrix_managed_owner_in_scope: Managed Owner InScope;
    matrix_managed_owner_out_of_scope: Managed Owner OutOfScope;
    matrix_managed_owner_self: Managed Owner SelfSubject;
    matrix_managed_owner_equal: Managed Owner Equal;
    matrix_managed_owner_lower: Managed Owner Lower;
    matrix_device_add_owner_in_scope: DeviceAdd Owner InScope;
    matrix_device_add_owner_out_of_scope: DeviceAdd Owner OutOfScope;
    matrix_device_add_owner_self: DeviceAdd Owner SelfSubject;
    matrix_device_add_owner_equal: DeviceAdd Owner Equal;
    matrix_device_add_owner_lower: DeviceAdd Owner Lower;
    matrix_device_remove_owner_in_scope: DeviceRemove Owner InScope;
    matrix_device_remove_owner_out_of_scope: DeviceRemove Owner OutOfScope;
    matrix_device_remove_owner_self: DeviceRemove Owner SelfSubject;
    matrix_device_remove_owner_equal: DeviceRemove Owner Equal;
    matrix_device_remove_owner_lower: DeviceRemove Owner Lower;
    matrix_escrow_open_owner_in_scope: EscrowOpen Owner InScope;
    matrix_escrow_open_owner_out_of_scope: EscrowOpen Owner OutOfScope;
    matrix_escrow_open_owner_self: EscrowOpen Owner SelfSubject;
    matrix_escrow_open_owner_equal: EscrowOpen Owner Equal;
    matrix_escrow_open_owner_lower: EscrowOpen Owner Lower;
    matrix_group_member_owner_in_scope: GroupMember Owner InScope;
    matrix_group_member_owner_out_of_scope: GroupMember Owner OutOfScope;
    matrix_group_create_owner_in_scope: GroupCreate Owner InScope;
    matrix_group_create_owner_out_of_scope: GroupCreate Owner OutOfScope;
    matrix_group_move_owner_in_scope: GroupMove Owner InScope;
    matrix_group_move_owner_out_of_scope: GroupMove Owner OutOfScope;
    matrix_policy_group_owner_in_scope: PolicyGroup Owner InScope;
    matrix_policy_group_owner_out_of_scope: PolicyGroup Owner OutOfScope;
    matrix_invite_owner_in_scope: Invite Owner InScope;
    matrix_invite_owner_out_of_scope: Invite Owner OutOfScope;
    matrix_policy_account_owner_account: PolicyAccount Owner InScope;
    matrix_escrow_set_owner_account: EscrowSet Owner InScope;
    matrix_audit_key_owner_account: AuditKey Owner InScope;
    matrix_admin_quorum_owner_account: AdminQuorum Owner InScope;
    matrix_delegate_owner_account: Delegate Owner InScope;
    matrix_role_admin_in_scope: Role Admin InScope;
    matrix_role_admin_out_of_scope: Role Admin OutOfScope;
    matrix_role_admin_self: Role Admin SelfSubject;
    matrix_role_admin_higher: Role Admin Higher;
    matrix_role_admin_equal: Role Admin Equal;
    matrix_role_admin_lower: Role Admin Lower;
    matrix_suspend_admin_in_scope: Suspend Admin InScope;
    matrix_suspend_admin_out_of_scope: Suspend Admin OutOfScope;
    matrix_suspend_admin_self: Suspend Admin SelfSubject;
    matrix_suspend_admin_higher: Suspend Admin Higher;
    matrix_suspend_admin_equal: Suspend Admin Equal;
    matrix_suspend_admin_lower: Suspend Admin Lower;
    matrix_remove_admin_in_scope: Remove Admin InScope;
    matrix_remove_admin_out_of_scope: Remove Admin OutOfScope;
    matrix_remove_admin_self: Remove Admin SelfSubject;
    matrix_remove_admin_higher: Remove Admin Higher;
    matrix_remove_admin_equal: Remove Admin Equal;
    matrix_remove_admin_lower: Remove Admin Lower;
    matrix_managed_admin_in_scope: Managed Admin InScope;
    matrix_managed_admin_out_of_scope: Managed Admin OutOfScope;
    matrix_managed_admin_self: Managed Admin SelfSubject;
    matrix_managed_admin_higher: Managed Admin Higher;
    matrix_managed_admin_equal: Managed Admin Equal;
    matrix_managed_admin_lower: Managed Admin Lower;
    matrix_device_add_admin_in_scope: DeviceAdd Admin InScope;
    matrix_device_add_admin_out_of_scope: DeviceAdd Admin OutOfScope;
    matrix_device_add_admin_self: DeviceAdd Admin SelfSubject;
    matrix_device_add_admin_higher: DeviceAdd Admin Higher;
    matrix_device_add_admin_equal: DeviceAdd Admin Equal;
    matrix_device_add_admin_lower: DeviceAdd Admin Lower;
    matrix_device_remove_admin_in_scope: DeviceRemove Admin InScope;
    matrix_device_remove_admin_out_of_scope: DeviceRemove Admin OutOfScope;
    matrix_device_remove_admin_self: DeviceRemove Admin SelfSubject;
    matrix_device_remove_admin_higher: DeviceRemove Admin Higher;
    matrix_device_remove_admin_equal: DeviceRemove Admin Equal;
    matrix_device_remove_admin_lower: DeviceRemove Admin Lower;
    matrix_escrow_open_admin_in_scope: EscrowOpen Admin InScope;
    matrix_escrow_open_admin_out_of_scope: EscrowOpen Admin OutOfScope;
    matrix_escrow_open_admin_self: EscrowOpen Admin SelfSubject;
    matrix_escrow_open_admin_higher: EscrowOpen Admin Higher;
    matrix_escrow_open_admin_equal: EscrowOpen Admin Equal;
    matrix_escrow_open_admin_lower: EscrowOpen Admin Lower;
    matrix_group_member_admin_in_scope: GroupMember Admin InScope;
    matrix_group_member_admin_out_of_scope: GroupMember Admin OutOfScope;
    matrix_group_create_admin_in_scope: GroupCreate Admin InScope;
    matrix_group_create_admin_out_of_scope: GroupCreate Admin OutOfScope;
    matrix_group_move_admin_in_scope: GroupMove Admin InScope;
    matrix_group_move_admin_out_of_scope: GroupMove Admin OutOfScope;
    matrix_policy_group_admin_in_scope: PolicyGroup Admin InScope;
    matrix_policy_group_admin_out_of_scope: PolicyGroup Admin OutOfScope;
    matrix_invite_admin_in_scope: Invite Admin InScope;
    matrix_invite_admin_out_of_scope: Invite Admin OutOfScope;
    matrix_policy_account_admin_account: PolicyAccount Admin InScope;
    matrix_escrow_set_admin_account: EscrowSet Admin InScope;
    matrix_audit_key_admin_account: AuditKey Admin InScope;
    matrix_admin_quorum_admin_account: AdminQuorum Admin InScope;
    matrix_delegate_admin_account: Delegate Admin InScope;
    matrix_role_group_admin_in_scope: Role GroupAdmin InScope;
    matrix_role_group_admin_out_of_scope: Role GroupAdmin OutOfScope;
    matrix_role_group_admin_self: Role GroupAdmin SelfSubject;
    matrix_role_group_admin_higher: Role GroupAdmin Higher;
    matrix_role_group_admin_equal: Role GroupAdmin Equal;
    matrix_role_group_admin_lower: Role GroupAdmin Lower;
    matrix_suspend_group_admin_in_scope: Suspend GroupAdmin InScope;
    matrix_suspend_group_admin_out_of_scope: Suspend GroupAdmin OutOfScope;
    matrix_suspend_group_admin_self: Suspend GroupAdmin SelfSubject;
    matrix_suspend_group_admin_higher: Suspend GroupAdmin Higher;
    matrix_suspend_group_admin_equal: Suspend GroupAdmin Equal;
    matrix_suspend_group_admin_lower: Suspend GroupAdmin Lower;
    matrix_remove_group_admin_in_scope: Remove GroupAdmin InScope;
    matrix_remove_group_admin_out_of_scope: Remove GroupAdmin OutOfScope;
    matrix_remove_group_admin_self: Remove GroupAdmin SelfSubject;
    matrix_remove_group_admin_higher: Remove GroupAdmin Higher;
    matrix_remove_group_admin_equal: Remove GroupAdmin Equal;
    matrix_remove_group_admin_lower: Remove GroupAdmin Lower;
    matrix_managed_group_admin_in_scope: Managed GroupAdmin InScope;
    matrix_managed_group_admin_out_of_scope: Managed GroupAdmin OutOfScope;
    matrix_managed_group_admin_self: Managed GroupAdmin SelfSubject;
    matrix_managed_group_admin_higher: Managed GroupAdmin Higher;
    matrix_managed_group_admin_equal: Managed GroupAdmin Equal;
    matrix_managed_group_admin_lower: Managed GroupAdmin Lower;
    matrix_device_add_group_admin_in_scope: DeviceAdd GroupAdmin InScope;
    matrix_device_add_group_admin_out_of_scope: DeviceAdd GroupAdmin OutOfScope;
    matrix_device_add_group_admin_self: DeviceAdd GroupAdmin SelfSubject;
    matrix_device_add_group_admin_higher: DeviceAdd GroupAdmin Higher;
    matrix_device_add_group_admin_equal: DeviceAdd GroupAdmin Equal;
    matrix_device_add_group_admin_lower: DeviceAdd GroupAdmin Lower;
    matrix_device_remove_group_admin_in_scope: DeviceRemove GroupAdmin InScope;
    matrix_device_remove_group_admin_out_of_scope: DeviceRemove GroupAdmin OutOfScope;
    matrix_device_remove_group_admin_self: DeviceRemove GroupAdmin SelfSubject;
    matrix_device_remove_group_admin_higher: DeviceRemove GroupAdmin Higher;
    matrix_device_remove_group_admin_equal: DeviceRemove GroupAdmin Equal;
    matrix_device_remove_group_admin_lower: DeviceRemove GroupAdmin Lower;
    matrix_escrow_open_group_admin_in_scope: EscrowOpen GroupAdmin InScope;
    matrix_escrow_open_group_admin_out_of_scope: EscrowOpen GroupAdmin OutOfScope;
    matrix_escrow_open_group_admin_self: EscrowOpen GroupAdmin SelfSubject;
    matrix_escrow_open_group_admin_higher: EscrowOpen GroupAdmin Higher;
    matrix_escrow_open_group_admin_equal: EscrowOpen GroupAdmin Equal;
    matrix_escrow_open_group_admin_lower: EscrowOpen GroupAdmin Lower;
    matrix_group_member_group_admin_in_scope: GroupMember GroupAdmin InScope;
    matrix_group_member_group_admin_out_of_scope: GroupMember GroupAdmin OutOfScope;
    matrix_group_create_group_admin_in_scope: GroupCreate GroupAdmin InScope;
    matrix_group_create_group_admin_out_of_scope: GroupCreate GroupAdmin OutOfScope;
    matrix_group_move_group_admin_in_scope: GroupMove GroupAdmin InScope;
    matrix_group_move_group_admin_out_of_scope: GroupMove GroupAdmin OutOfScope;
    matrix_policy_group_group_admin_in_scope: PolicyGroup GroupAdmin InScope;
    matrix_policy_group_group_admin_out_of_scope: PolicyGroup GroupAdmin OutOfScope;
    matrix_invite_group_admin_in_scope: Invite GroupAdmin InScope;
    matrix_invite_group_admin_out_of_scope: Invite GroupAdmin OutOfScope;
    matrix_policy_account_group_admin_account: PolicyAccount GroupAdmin InScope;
    matrix_escrow_set_group_admin_account: EscrowSet GroupAdmin InScope;
    matrix_audit_key_group_admin_account: AuditKey GroupAdmin InScope;
    matrix_admin_quorum_group_admin_account: AdminQuorum GroupAdmin InScope;
    matrix_delegate_group_admin_account: Delegate GroupAdmin InScope;
    matrix_role_helpdesk_in_scope: Role Helpdesk InScope;
    matrix_role_helpdesk_out_of_scope: Role Helpdesk OutOfScope;
    matrix_role_helpdesk_self: Role Helpdesk SelfSubject;
    matrix_role_helpdesk_higher: Role Helpdesk Higher;
    matrix_role_helpdesk_equal: Role Helpdesk Equal;
    matrix_role_helpdesk_lower: Role Helpdesk Lower;
    matrix_suspend_helpdesk_in_scope: Suspend Helpdesk InScope;
    matrix_suspend_helpdesk_out_of_scope: Suspend Helpdesk OutOfScope;
    matrix_suspend_helpdesk_self: Suspend Helpdesk SelfSubject;
    matrix_suspend_helpdesk_higher: Suspend Helpdesk Higher;
    matrix_suspend_helpdesk_equal: Suspend Helpdesk Equal;
    matrix_suspend_helpdesk_lower: Suspend Helpdesk Lower;
    matrix_remove_helpdesk_in_scope: Remove Helpdesk InScope;
    matrix_remove_helpdesk_out_of_scope: Remove Helpdesk OutOfScope;
    matrix_remove_helpdesk_self: Remove Helpdesk SelfSubject;
    matrix_remove_helpdesk_higher: Remove Helpdesk Higher;
    matrix_remove_helpdesk_equal: Remove Helpdesk Equal;
    matrix_remove_helpdesk_lower: Remove Helpdesk Lower;
    matrix_managed_helpdesk_in_scope: Managed Helpdesk InScope;
    matrix_managed_helpdesk_out_of_scope: Managed Helpdesk OutOfScope;
    matrix_managed_helpdesk_self: Managed Helpdesk SelfSubject;
    matrix_managed_helpdesk_higher: Managed Helpdesk Higher;
    matrix_managed_helpdesk_equal: Managed Helpdesk Equal;
    matrix_managed_helpdesk_lower: Managed Helpdesk Lower;
    matrix_device_add_helpdesk_in_scope: DeviceAdd Helpdesk InScope;
    matrix_device_add_helpdesk_out_of_scope: DeviceAdd Helpdesk OutOfScope;
    matrix_device_add_helpdesk_self: DeviceAdd Helpdesk SelfSubject;
    matrix_device_add_helpdesk_higher: DeviceAdd Helpdesk Higher;
    matrix_device_add_helpdesk_equal: DeviceAdd Helpdesk Equal;
    matrix_device_add_helpdesk_lower: DeviceAdd Helpdesk Lower;
    matrix_device_remove_helpdesk_in_scope: DeviceRemove Helpdesk InScope;
    matrix_device_remove_helpdesk_out_of_scope: DeviceRemove Helpdesk OutOfScope;
    matrix_device_remove_helpdesk_self: DeviceRemove Helpdesk SelfSubject;
    matrix_device_remove_helpdesk_higher: DeviceRemove Helpdesk Higher;
    matrix_device_remove_helpdesk_equal: DeviceRemove Helpdesk Equal;
    matrix_device_remove_helpdesk_lower: DeviceRemove Helpdesk Lower;
    matrix_escrow_open_helpdesk_in_scope: EscrowOpen Helpdesk InScope;
    matrix_escrow_open_helpdesk_out_of_scope: EscrowOpen Helpdesk OutOfScope;
    matrix_escrow_open_helpdesk_self: EscrowOpen Helpdesk SelfSubject;
    matrix_escrow_open_helpdesk_higher: EscrowOpen Helpdesk Higher;
    matrix_escrow_open_helpdesk_equal: EscrowOpen Helpdesk Equal;
    matrix_escrow_open_helpdesk_lower: EscrowOpen Helpdesk Lower;
    matrix_group_member_helpdesk_in_scope: GroupMember Helpdesk InScope;
    matrix_group_member_helpdesk_out_of_scope: GroupMember Helpdesk OutOfScope;
    matrix_group_create_helpdesk_in_scope: GroupCreate Helpdesk InScope;
    matrix_group_create_helpdesk_out_of_scope: GroupCreate Helpdesk OutOfScope;
    matrix_group_move_helpdesk_in_scope: GroupMove Helpdesk InScope;
    matrix_group_move_helpdesk_out_of_scope: GroupMove Helpdesk OutOfScope;
    matrix_policy_group_helpdesk_in_scope: PolicyGroup Helpdesk InScope;
    matrix_policy_group_helpdesk_out_of_scope: PolicyGroup Helpdesk OutOfScope;
    matrix_invite_helpdesk_in_scope: Invite Helpdesk InScope;
    matrix_invite_helpdesk_out_of_scope: Invite Helpdesk OutOfScope;
    matrix_policy_account_helpdesk_account: PolicyAccount Helpdesk InScope;
    matrix_escrow_set_helpdesk_account: EscrowSet Helpdesk InScope;
    matrix_audit_key_helpdesk_account: AuditKey Helpdesk InScope;
    matrix_admin_quorum_helpdesk_account: AdminQuorum Helpdesk InScope;
    matrix_delegate_helpdesk_account: Delegate Helpdesk InScope;
    matrix_role_member_in_scope: Role Member InScope;
    matrix_role_member_out_of_scope: Role Member OutOfScope;
    matrix_role_member_self: Role Member SelfSubject;
    matrix_role_member_higher: Role Member Higher;
    matrix_role_member_equal: Role Member Equal;
    matrix_role_member_lower: Role Member Lower;
    matrix_suspend_member_in_scope: Suspend Member InScope;
    matrix_suspend_member_out_of_scope: Suspend Member OutOfScope;
    matrix_suspend_member_self: Suspend Member SelfSubject;
    matrix_suspend_member_higher: Suspend Member Higher;
    matrix_suspend_member_equal: Suspend Member Equal;
    matrix_suspend_member_lower: Suspend Member Lower;
    matrix_remove_member_in_scope: Remove Member InScope;
    matrix_remove_member_out_of_scope: Remove Member OutOfScope;
    matrix_remove_member_self: Remove Member SelfSubject;
    matrix_remove_member_higher: Remove Member Higher;
    matrix_remove_member_equal: Remove Member Equal;
    matrix_remove_member_lower: Remove Member Lower;
    matrix_managed_member_in_scope: Managed Member InScope;
    matrix_managed_member_out_of_scope: Managed Member OutOfScope;
    matrix_managed_member_self: Managed Member SelfSubject;
    matrix_managed_member_higher: Managed Member Higher;
    matrix_managed_member_equal: Managed Member Equal;
    matrix_managed_member_lower: Managed Member Lower;
    matrix_device_add_member_in_scope: DeviceAdd Member InScope;
    matrix_device_add_member_out_of_scope: DeviceAdd Member OutOfScope;
    matrix_device_add_member_self: DeviceAdd Member SelfSubject;
    matrix_device_add_member_higher: DeviceAdd Member Higher;
    matrix_device_add_member_equal: DeviceAdd Member Equal;
    matrix_device_add_member_lower: DeviceAdd Member Lower;
    matrix_device_remove_member_in_scope: DeviceRemove Member InScope;
    matrix_device_remove_member_out_of_scope: DeviceRemove Member OutOfScope;
    matrix_device_remove_member_self: DeviceRemove Member SelfSubject;
    matrix_device_remove_member_higher: DeviceRemove Member Higher;
    matrix_device_remove_member_equal: DeviceRemove Member Equal;
    matrix_device_remove_member_lower: DeviceRemove Member Lower;
    matrix_escrow_open_member_in_scope: EscrowOpen Member InScope;
    matrix_escrow_open_member_out_of_scope: EscrowOpen Member OutOfScope;
    matrix_escrow_open_member_self: EscrowOpen Member SelfSubject;
    matrix_escrow_open_member_higher: EscrowOpen Member Higher;
    matrix_escrow_open_member_equal: EscrowOpen Member Equal;
    matrix_escrow_open_member_lower: EscrowOpen Member Lower;
    matrix_group_member_member_in_scope: GroupMember Member InScope;
    matrix_group_member_member_out_of_scope: GroupMember Member OutOfScope;
    matrix_group_create_member_in_scope: GroupCreate Member InScope;
    matrix_group_create_member_out_of_scope: GroupCreate Member OutOfScope;
    matrix_group_move_member_in_scope: GroupMove Member InScope;
    matrix_group_move_member_out_of_scope: GroupMove Member OutOfScope;
    matrix_policy_group_member_in_scope: PolicyGroup Member InScope;
    matrix_policy_group_member_out_of_scope: PolicyGroup Member OutOfScope;
    matrix_invite_member_in_scope: Invite Member InScope;
    matrix_invite_member_out_of_scope: Invite Member OutOfScope;
    matrix_policy_account_member_account: PolicyAccount Member InScope;
    matrix_escrow_set_member_account: EscrowSet Member InScope;
    matrix_audit_key_member_account: AuditKey Member InScope;
    matrix_admin_quorum_member_account: AdminQuorum Member InScope;
    matrix_delegate_member_account: Delegate Member InScope;
    matrix_role_guest_in_scope: Role Guest InScope;
    matrix_role_guest_out_of_scope: Role Guest OutOfScope;
    matrix_role_guest_self: Role Guest SelfSubject;
    matrix_role_guest_higher: Role Guest Higher;
    matrix_role_guest_equal: Role Guest Equal;
    matrix_suspend_guest_in_scope: Suspend Guest InScope;
    matrix_suspend_guest_out_of_scope: Suspend Guest OutOfScope;
    matrix_suspend_guest_self: Suspend Guest SelfSubject;
    matrix_suspend_guest_higher: Suspend Guest Higher;
    matrix_suspend_guest_equal: Suspend Guest Equal;
    matrix_remove_guest_in_scope: Remove Guest InScope;
    matrix_remove_guest_out_of_scope: Remove Guest OutOfScope;
    matrix_remove_guest_self: Remove Guest SelfSubject;
    matrix_remove_guest_higher: Remove Guest Higher;
    matrix_remove_guest_equal: Remove Guest Equal;
    matrix_managed_guest_in_scope: Managed Guest InScope;
    matrix_managed_guest_out_of_scope: Managed Guest OutOfScope;
    matrix_managed_guest_self: Managed Guest SelfSubject;
    matrix_managed_guest_higher: Managed Guest Higher;
    matrix_managed_guest_equal: Managed Guest Equal;
    matrix_device_add_guest_in_scope: DeviceAdd Guest InScope;
    matrix_device_add_guest_out_of_scope: DeviceAdd Guest OutOfScope;
    matrix_device_add_guest_self: DeviceAdd Guest SelfSubject;
    matrix_device_add_guest_higher: DeviceAdd Guest Higher;
    matrix_device_add_guest_equal: DeviceAdd Guest Equal;
    matrix_device_remove_guest_in_scope: DeviceRemove Guest InScope;
    matrix_device_remove_guest_out_of_scope: DeviceRemove Guest OutOfScope;
    matrix_device_remove_guest_self: DeviceRemove Guest SelfSubject;
    matrix_device_remove_guest_higher: DeviceRemove Guest Higher;
    matrix_device_remove_guest_equal: DeviceRemove Guest Equal;
    matrix_escrow_open_guest_in_scope: EscrowOpen Guest InScope;
    matrix_escrow_open_guest_out_of_scope: EscrowOpen Guest OutOfScope;
    matrix_escrow_open_guest_self: EscrowOpen Guest SelfSubject;
    matrix_escrow_open_guest_higher: EscrowOpen Guest Higher;
    matrix_escrow_open_guest_equal: EscrowOpen Guest Equal;
    matrix_group_member_guest_in_scope: GroupMember Guest InScope;
    matrix_group_member_guest_out_of_scope: GroupMember Guest OutOfScope;
    matrix_group_create_guest_in_scope: GroupCreate Guest InScope;
    matrix_group_create_guest_out_of_scope: GroupCreate Guest OutOfScope;
    matrix_group_move_guest_in_scope: GroupMove Guest InScope;
    matrix_group_move_guest_out_of_scope: GroupMove Guest OutOfScope;
    matrix_policy_group_guest_in_scope: PolicyGroup Guest InScope;
    matrix_policy_group_guest_out_of_scope: PolicyGroup Guest OutOfScope;
    matrix_invite_guest_in_scope: Invite Guest InScope;
    matrix_invite_guest_out_of_scope: Invite Guest OutOfScope;
    matrix_policy_account_guest_account: PolicyAccount Guest InScope;
    matrix_escrow_set_guest_account: EscrowSet Guest InScope;
    matrix_audit_key_guest_account: AuditKey Guest InScope;
    matrix_admin_quorum_guest_account: AdminQuorum Guest InScope;
    matrix_delegate_guest_account: Delegate Guest InScope;
}

#[test]
fn every_cell_has_its_named_test() {
    let mut wanted = BTreeSet::new();
    for (cell, _) in cells() {
        wanted.insert(format!(
            "matrix_{}_{}_{}",
            kind_word(cell.kind),
            cell.actor.as_str(),
            rel_word(cell.kind, cell.rel)
        ));
    }
    let named: BTreeSet<String> = NAMED.iter().map(|name| (*name).to_string()).collect();
    assert_eq!(named.len(), NAMED.len(), "no name twice");
    assert_eq!(named, wanted, "the named tests are the cells of the matrix");
}
