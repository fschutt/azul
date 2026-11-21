//! Check 8 of the brief (creation by members): who may create a space of which kind, as a table of kind x
//! account role under four account configurations (168 cells, the expected side typed from the brief), and
//! the named checks: a member creates a conversation when the account's view allows it and not otherwise,
//! a private member space exists only in a family account, the root space only at account creation.
//!
//! Implements planning/architecture.md section 5.4 (`reg_space.kind`), section 5.9 ("Members may create
//! conversation spaces where policy allows") and section 6.5 ("Members create conversations by policy
//! through the space chain").

mod common;

use std::collections::BTreeSet;

use azlin_policy::AuditLevel;
use azlin_seams::{AccountKind, AccountRole, MemberKind};
use azlin_spaces::{Authority, HistoryMode, SpaceKind, SpaceRefusal, SpaceRole};
use common::*;

const KINDS: [SpaceKind; 7] = [
    SpaceKind::Root,
    SpaceKind::Shared,
    SpaceKind::Team,
    SpaceKind::PrivateMember,
    SpaceKind::Inbox,
    SpaceKind::Conversation,
    SpaceKind::SystemAudit,
];

/// The member number of an account role in the test account (see `common::role_of`).
fn member_of(role: AccountRole) -> u8 {
    match role {
        AccountRole::Owner => 1,
        AccountRole::Admin => 2,
        AccountRole::GroupAdmin => 3,
        AccountRole::Helpdesk => 4,
        AccountRole::Member => 5,
        AccountRole::Guest => 6,
    }
}

/// One configuration of the account around the creation.
#[derive(Clone)]
struct Setup {
    kind: AccountKind,
    creators: Vec<SpaceKind>,
    audit_none: bool,
    root_exists: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Ok,
    NotAllowed,
    NeedsFamily,
    AuditNone,
    RootExists,
}

/// The expected side, typed from the brief: the root space is the owner's and only while the account has
/// none; the audit space an admin's and only when `audit_level` is not `none`; a private member space only
/// in a family account; every other kind by an admin, or by a member (never a guest) where the account's
/// policy lets members create that kind.
fn want(kind: SpaceKind, role: AccountRole, setup: &Setup) -> Want {
    let admin = matches!(role, AccountRole::Owner | AccountRole::Admin);
    let members_may = role != AccountRole::Guest && setup.creators.contains(&kind);
    match kind {
        SpaceKind::Root => {
            if role != AccountRole::Owner {
                Want::NotAllowed
            } else if setup.root_exists {
                Want::RootExists
            } else {
                Want::Ok
            }
        }
        SpaceKind::SystemAudit => {
            if !admin {
                Want::NotAllowed
            } else if setup.audit_none {
                Want::AuditNone
            } else {
                Want::Ok
            }
        }
        SpaceKind::PrivateMember => {
            if !(admin || members_may) {
                Want::NotAllowed
            } else if setup.kind != AccountKind::Family {
                Want::NeedsFamily
            } else {
                Want::Ok
            }
        }
        SpaceKind::Shared | SpaceKind::Team | SpaceKind::Inbox | SpaceKind::Conversation => {
            if admin || members_may {
                Want::Ok
            } else {
                Want::NotAllowed
            }
        }
    }
}

fn world_of(setup: &Setup) -> World {
    let mut w = World::new(setup.kind);
    w.account.creators = setup.creators.iter().copied().collect::<BTreeSet<_>>();
    w.account.policy.audit_level = if setup.audit_none {
        AuditLevel::None
    } else {
        AuditLevel::Admin
    };
    w.account.root_space = setup.root_exists;
    w
}

#[test]
fn who_may_create_which_kind_covers_kind_by_role_by_setup() {
    let setups = [
        // A team where members may create shared spaces and conversations.
        Setup {
            kind: AccountKind::Team,
            creators: vec![SpaceKind::Shared, SpaceKind::Conversation],
            audit_none: false,
            root_exists: false,
        },
        // A family where members may create their private spaces and shared ones.
        Setup {
            kind: AccountKind::Family,
            creators: vec![SpaceKind::PrivateMember, SpaceKind::Shared],
            audit_none: false,
            root_exists: false,
        },
        // A team whose audit level is none, and where members may create every kind.
        Setup {
            kind: AccountKind::Team,
            creators: KINDS.to_vec(),
            audit_none: true,
            root_exists: false,
        },
        // A business that already has its root space, and where members may create nothing.
        Setup {
            kind: AccountKind::Business,
            creators: Vec::new(),
            audit_none: false,
            root_exists: true,
        },
    ];
    let roles = [
        AccountRole::Owner,
        AccountRole::Admin,
        AccountRole::GroupAdmin,
        AccountRole::Helpdesk,
        AccountRole::Member,
        AccountRole::Guest,
    ];
    let mut cells = 0usize;
    for setup in &setups {
        for kind in KINDS {
            for role in roles {
                let n = member_of(role);
                let w = world_of(setup);
                let result = w.admit_by(n, create_op(kind, HistoryMode::Full, n, 1));
                let expected = want(kind, role, setup);
                let got = match &result {
                    Ok(admitted) => {
                        assert_eq!(admitted.authority(), Authority::Creator);
                        Want::Ok
                    }
                    Err(SpaceRefusal::CreateNotAllowed { kind: refused }) => {
                        assert_eq!(*refused, kind);
                        Want::NotAllowed
                    }
                    Err(SpaceRefusal::PrivateSpaceNeedsFamily) => Want::NeedsFamily,
                    Err(SpaceRefusal::AuditLevelNone) => Want::AuditNone,
                    Err(SpaceRefusal::RootExists) => Want::RootExists,
                    Err(other) => panic!("{kind:?} by {role:?}: {other:?}"),
                };
                assert_eq!(
                    got, expected,
                    "{kind:?} by {role:?} in a {:?} account with {:?} (audit none {}, root {})",
                    setup.kind, setup.creators, setup.audit_none, setup.root_exists
                );
                cells += 1;
            }
        }
    }
    assert_eq!(cells, 4 * 7 * 6);
}

#[test]
fn member_creates_a_conversation_when_the_account_view_allows() {
    let mut w = World::new(AccountKind::Team);
    assert!(azlin_spaces::AccountView::members_may_create(
        &w.account,
        SpaceKind::Conversation
    ));
    // A member (not an admin) makes a conversation and is its first manager.
    let effects = w.do_by(
        5,
        create_op(SpaceKind::Conversation, HistoryMode::Forward, 5, 1),
    );
    assert!(effects.applied);
    assert_eq!(w.state.kind(), Some(SpaceKind::Conversation));
    assert_eq!(w.state.history(), Some(HistoryMode::Forward));
    assert_eq!(w.state.created_by(), Some(member(5)));
    assert!(w
        .state
        .grant(&m(5))
        .is_some_and(|grant| grant.role == SpaceRole::Manager && grant.is_active()));
    // The members of a conversation are its manager's to grant: a direct message of two.
    w.do_by(5, grant_op(m(8), SpaceRole::Editor));
    // A helpdesk member and a group admin do the same where the view says members may.
    for n in [3u8, 4] {
        let mut w = World::new(AccountKind::Business);
        w.account.creators.insert(SpaceKind::Conversation);
        let effects = w.do_by(
            n,
            create_op(SpaceKind::Conversation, HistoryMode::Full, n, 1),
        );
        assert!(effects.applied, "member {n}");
    }
    // The first manager may be another member (a company channel made for a lead).
    let mut w = World::new(AccountKind::Team);
    w.do_by(
        5,
        create_op(SpaceKind::Conversation, HistoryMode::Full, 9, 1),
    );
    assert!(w.state.grant(&m(9)).is_some());
    assert!(w.state.grant(&m(5)).is_none());
}

#[test]
fn member_creation_refused_when_it_does_not() {
    // The account's view says members may not: only admins create.
    for kind in [
        SpaceKind::Shared,
        SpaceKind::Team,
        SpaceKind::Inbox,
        SpaceKind::Conversation,
    ] {
        for n in [3u8, 4, 5] {
            let mut w = World::new(AccountKind::Team);
            w.account.creators.clear();
            assert_eq!(
                w.refused_by(n, create_op(kind, HistoryMode::Full, n, 1)),
                SpaceRefusal::CreateNotAllowed { kind },
                "member {n} / {kind:?}"
            );
        }
        // An admin does.
        let mut w = World::new(AccountKind::Team);
        w.account.creators.clear();
        w.do_by(2, create_op(kind, HistoryMode::Full, 2, 1));
    }
    // A guest never creates, even where the view says members may.
    let mut w = World::new(AccountKind::Team);
    w.account.creators = KINDS.iter().copied().collect();
    assert_eq!(
        w.refused_by(
            6,
            create_op(SpaceKind::Conversation, HistoryMode::Full, 6, 1)
        ),
        SpaceRefusal::CreateNotAllowed {
            kind: SpaceKind::Conversation
        }
    );
    // A member of another account never creates a space of this one.
    let mut w = World::new(AccountKind::Team);
    w.account.creators = KINDS.iter().copied().collect();
    assert_eq!(
        refusal_of(w.try_admit_as(
            ext_device(1),
            create_op(SpaceKind::Conversation, HistoryMode::Full, 5, 1),
            &[]
        )),
        SpaceRefusal::CreateNotAllowed {
            kind: SpaceKind::Conversation
        }
    );
}

#[test]
fn private_member_space_only_for_family_accounts() {
    // A family member creates its own private space.
    let mut w = World::new(AccountKind::Family);
    w.account.creators.insert(SpaceKind::PrivateMember);
    let effects = w.do_by(
        5,
        create_op(SpaceKind::PrivateMember, HistoryMode::Full, 5, 1),
    );
    assert!(effects.applied);
    assert_eq!(w.state.kind(), Some(SpaceKind::PrivateMember));

    // Nobody else's: a private space is its member's own, an owner included.
    let mut w = World::new(AccountKind::Family);
    w.account.creators.insert(SpaceKind::PrivateMember);
    assert_eq!(
        w.refused_by(
            1,
            create_op(SpaceKind::PrivateMember, HistoryMode::Full, 5, 1)
        ),
        SpaceRefusal::CreateNotAllowed {
            kind: SpaceKind::PrivateMember
        }
    );

    // In every other kind of account it does not exist, whoever asks.
    for kind in [
        AccountKind::Personal,
        AccountKind::Team,
        AccountKind::Business,
    ] {
        let mut w = World::new(kind);
        w.account.creators.insert(SpaceKind::PrivateMember);
        for n in [1u8, 2, 5] {
            assert_eq!(
                w.refused_by(
                    n,
                    create_op(SpaceKind::PrivateMember, HistoryMode::Full, n, 1)
                ),
                SpaceRefusal::PrivateSpaceNeedsFamily,
                "{kind:?} / member {n}"
            );
        }
    }
}

#[test]
fn the_first_manager_must_be_an_active_member_of_the_account() {
    let mut w = World::new(AccountKind::Team);
    // Somebody who is no member of the account, or whose membership is suspended.
    assert_eq!(
        w.refused_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 99, 1)),
        SpaceRefusal::FirstManagerUnknown
    );
    w.account.suspend(9);
    assert_eq!(
        w.refused_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 9, 1)),
        SpaceRefusal::FirstManagerUnknown
    );
    // An app may be the first manager (the key agent of a space made for it).
    w.account
        .add_person(20, AccountRole::Member, MemberKind::App);
    w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 20, 1));
    assert!(w.state.grant(&m(20)).is_some());
}

#[test]
fn the_audit_space_needs_an_admin_and_an_audit_level() {
    // audit_level is `admin` for a team, `none` for personal and family.
    let mut w = World::new(AccountKind::Team);
    w.do_by(
        2,
        create_op(SpaceKind::SystemAudit, HistoryMode::Full, 2, 1),
    );
    let mut w = World::new(AccountKind::Family);
    assert_eq!(w.account.policy.audit_level, AuditLevel::None);
    assert_eq!(
        w.refused_by(
            1,
            create_op(SpaceKind::SystemAudit, HistoryMode::Full, 1, 1)
        ),
        SpaceRefusal::AuditLevelNone
    );
    // A member does not create it, whatever the view says about members.
    let mut w = World::new(AccountKind::Team);
    w.account.creators.insert(SpaceKind::SystemAudit);
    assert_eq!(
        w.refused_by(
            5,
            create_op(SpaceKind::SystemAudit, HistoryMode::Full, 5, 1)
        ),
        SpaceRefusal::CreateNotAllowed {
            kind: SpaceKind::SystemAudit
        }
    );
}
