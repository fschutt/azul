//! Check 2 of the brief (who may sign): a table with one case per op x account role x relation to the
//! space (288 cells), whose expected side is typed from the brief and not read from `admit`, and the named
//! checks around it: a manager grants, editor/viewer/uploader do not, an account admin who is not a manager
//! does not (the override needs the admin quorum and an orphaned space), the grantee may leave, the last
//! manager stays, a suspended member and an inactive device sign nothing, an app principal that manages may
//! rekey and nothing else.
//!
//! Implements planning/architecture.md section 5.9 ("signed by its managers"), section 7.2 (account roles,
//! space roles), section 7.6 (the key agent rotates forward through a grant) and section 12 E1 (no entry is
//! accepted unless its signer may sign it; a quorum counts distinct people).

mod common;

use azlin_seams::{AccountKind, AccountRole, MemberKind};
use azlin_spaces::{
    admit, Authority, HistoryMode, SpaceKind, SpaceOp, SpaceOpName, SpaceRefusal, SpaceRole,
};
use common::*;

// ---------------------------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------------------------

/// How the signer stands to the space.
#[derive(Clone, Copy, Debug)]
enum Relation {
    /// A direct manager grant.
    ManagerDirect,
    /// A manager grant to a group the signer is in.
    ManagerViaGroup,
    /// A direct editor grant.
    Editor,
    /// A direct viewer grant.
    Viewer,
    /// A direct uploader grant.
    Uploader,
    /// A member of the account with no grant at all.
    NoGrant,
    /// A member of another account with a manager grant.
    ExternalManager,
    /// A member of another account with a viewer grant.
    ExternalViewer,
}

const RELATIONS: [Relation; 8] = [
    Relation::ManagerDirect,
    Relation::ManagerViaGroup,
    Relation::Editor,
    Relation::Viewer,
    Relation::Uploader,
    Relation::NoGrant,
    Relation::ExternalManager,
    Relation::ExternalViewer,
];

const ROLES: [AccountRole; 6] = [
    AccountRole::Owner,
    AccountRole::Admin,
    AccountRole::GroupAdmin,
    AccountRole::Helpdesk,
    AccountRole::Member,
    AccountRole::Guest,
];

/// What the signer tries.
#[derive(Clone, Copy, Debug)]
enum Try {
    GrantViewer,
    GrantManager,
    RevokeOther,
    RevokeSelf,
    Rekey,
    History,
}

const TRIES: [Try; 6] = [
    Try::GrantViewer,
    Try::GrantManager,
    Try::RevokeOther,
    Try::RevokeSelf,
    Try::Rekey,
    Try::History,
];

/// The expected side, typed from the brief (section 9 item 1: only managers sign grants; an account
/// admin in scope may revoke; the grantee may leave; rekeys and history are the managers').
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Allowed,
    NotAManager,
    NoSuchGrant,
}

fn expected(attempt: Try, role: AccountRole, relation: Relation) -> Expect {
    let manager = matches!(
        relation,
        Relation::ManagerDirect | Relation::ManagerViaGroup | Relation::ExternalManager
    );
    let internal = !matches!(
        relation,
        Relation::ExternalManager | Relation::ExternalViewer
    );
    let admin = matches!(role, AccountRole::Owner | AccountRole::Admin);
    let own_grant_exists = matches!(
        relation,
        Relation::ManagerDirect
            | Relation::Editor
            | Relation::Viewer
            | Relation::Uploader
            | Relation::ExternalManager
            | Relation::ExternalViewer
    );
    match attempt {
        Try::GrantViewer | Try::GrantManager | Try::Rekey | Try::History => {
            if manager {
                Expect::Allowed
            } else {
                Expect::NotAManager
            }
        }
        Try::RevokeOther => {
            if manager || (internal && admin) {
                Expect::Allowed
            } else {
                Expect::NotAManager
            }
        }
        Try::RevokeSelf => {
            if own_grant_exists {
                Expect::Allowed
            } else {
                Expect::NoSuchGrant
            }
        }
    }
}

/// A world whose space was created by the owner (member 1, the first manager), with the member 30 (account
/// role `role`) or the external member 1 standing to it as `relation`. The member 8 holds a viewer grant
/// (the target of "revoke somebody else's grant").
fn cell(role: AccountRole, relation: Relation) -> (World, azlin_seams::DeviceId) {
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.account.add_person(30, role, MemberKind::Person);
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    let signer = match relation {
        Relation::ManagerDirect => {
            w.do_by(1, grant_op(m(30), SpaceRole::Manager));
            device(30)
        }
        Relation::ManagerViaGroup => {
            w.account.put_in_groups(30, &[3]);
            w.do_by(1, grant_op(g(3), SpaceRole::Manager));
            device(30)
        }
        Relation::Editor => {
            w.do_by(1, grant_op(m(30), SpaceRole::Editor));
            device(30)
        }
        Relation::Viewer => {
            w.do_by(1, grant_op(m(30), SpaceRole::Viewer));
            device(30)
        }
        Relation::Uploader => {
            w.do_by(1, grant_op(m(30), SpaceRole::Uploader));
            device(30)
        }
        Relation::NoGrant => device(30),
        Relation::ExternalManager => {
            w.do_by(1, grant_op(x(1), SpaceRole::Manager));
            ext_device(1)
        }
        Relation::ExternalViewer => {
            w.do_by(1, grant_op(x(1), SpaceRole::Viewer));
            ext_device(1)
        }
    };
    (w, signer)
}

fn op_of(attempt: Try, relation: Relation) -> SpaceOp {
    let own = match relation {
        Relation::ExternalManager | Relation::ExternalViewer => x(1),
        _ => m(30),
    };
    match attempt {
        Try::GrantViewer => grant_op(m(7), SpaceRole::Viewer),
        Try::GrantManager => grant_op(m(7), SpaceRole::Manager),
        Try::RevokeOther => revoke_op(m(8)),
        Try::RevokeSelf => revoke_op(own),
        Try::Rekey => rekey_op(2, 2),
        Try::History => history_op(HistoryMode::Forward),
    }
}

#[test]
fn who_may_sign_matrix_covers_op_by_role_by_relation() {
    let mut cells = 0usize;
    for role in ROLES {
        for relation in RELATIONS {
            for attempt in TRIES {
                let (w, signer) = cell(role, relation);
                let result = w.try_admit_as(signer, op_of(attempt, relation), &[]);
                let got = match &result {
                    Ok(_) => Expect::Allowed,
                    Err(SpaceRefusal::NotAManager { .. }) => Expect::NotAManager,
                    Err(SpaceRefusal::NoSuchGrant) => Expect::NoSuchGrant,
                    Err(other) => panic!("{role:?} / {relation:?} / {attempt:?}: {other:?}"),
                };
                assert_eq!(
                    got,
                    expected(attempt, role, relation),
                    "{role:?} / {relation:?} / {attempt:?}"
                );
                cells += 1;
            }
        }
    }
    assert_eq!(cells, 6 * 8 * 6);
    assert!(cells >= 120, "the brief asks for at least 120 cells");
}

// ---------------------------------------------------------------------------------------------
// The named checks
// ---------------------------------------------------------------------------------------------

#[test]
fn grant_by_a_manager_ok() {
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    // Every role, to a member, a group, an external member and a capability.
    for (n, role) in [
        (7, SpaceRole::Manager),
        (8, SpaceRole::Editor),
        (9, SpaceRole::Viewer),
        (10, SpaceRole::Uploader),
    ] {
        let admitted = w
            .try_admit_as(device(1), grant_op(m(n), role), &[])
            .expect("a manager grants");
        assert_eq!(admitted.authority(), Authority::Manager);
        w.do_by(1, grant_op(m(n), role));
    }
    w.do_by(1, grant_op(g(1), SpaceRole::Editor));
    w.do_by(1, grant_op(x(1), SpaceRole::Viewer));
    w.do_by(1, grant_op(cap(1), SpaceRole::Viewer));
    // A manager made by a grant grants in turn (the member 7 became a manager above).
    w.do_by(7, grant_op(m(11), SpaceRole::Viewer));
    assert!(w.state.grant(&m(11)).is_some_and(|grant| grant.is_active()));
}

#[test]
fn grant_by_editor_viewer_uploader_refused() {
    for role in [SpaceRole::Editor, SpaceRole::Viewer, SpaceRole::Uploader] {
        let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
        w.do_by(1, grant_op(m(5), role));
        for target in [
            grant_op(m(7), SpaceRole::Viewer),
            grant_op(m(7), SpaceRole::Manager),
            grant_op(g(1), SpaceRole::Viewer),
        ] {
            assert_eq!(
                w.refused_by(5, target),
                SpaceRefusal::NotAManager {
                    op: SpaceOpName::Grant
                },
                "{role:?}"
            );
        }
        // Nor does it rekey or switch the history: those are the managers' too.
        assert_eq!(
            w.refused_by(5, rekey_op(2, 2)),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::Rekey
            }
        );
        assert_eq!(
            w.refused_by(5, history_op(HistoryMode::Forward)),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::History
            }
        );
    }
}

#[test]
fn grant_by_an_account_admin_who_is_not_a_manager_refused_without_override() {
    // The space has its manager (the member 1); the admin (member 2) holds no grant.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    for target in [
        grant_op(m(7), SpaceRole::Viewer),
        grant_op(m(7), SpaceRole::Editor),
        grant_op(m(7), SpaceRole::Manager),
        grant_op(g(1), SpaceRole::Manager),
    ] {
        assert_eq!(
            w.refused_by(2, target.clone()),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::Grant
            },
            "{target:?}"
        );
    }
    // Not with a cosignature either: the space is not orphaned, so there is no override at all.
    let cosigned = w
        .try_admit_as(device(2), grant_op(m(7), SpaceRole::Manager), &[device(1)])
        .expect_err("a quorum does not make a manager of an admin in a space that has managers");
    assert_eq!(
        cosigned,
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
    // The same for an owner who is not a manager of this space (the space was made for the member 7).
    let mut w = World::new(AccountKind::Team);
    w.do_by(1, create_op(SpaceKind::Team, HistoryMode::Full, 7, 1));
    assert_eq!(
        w.refused_by(1, grant_op(m(8), SpaceRole::Viewer)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
}

/// A business account (the admin quorum is 2) whose space was made for the member 7 and then orphaned:
/// the member 7 is suspended, so no manager is really there.
fn orphaned_business() -> World {
    let mut w = World::new(AccountKind::Business);
    w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 7, 1));
    w.account.suspend(7);
    w
}

#[test]
fn admin_override_needs_the_admin_quorum() {
    let manager_for_8 = grant_op(m(8), SpaceRole::Manager);

    // Alone, an admin of a business is one person short.
    let mut w = orphaned_business();
    assert_eq!(
        w.refused_by(2, manager_for_8.clone()),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );

    // With the owner's cosignature the override holds: two different people.
    let admitted = w
        .try_admit_as(device(2), manager_for_8.clone(), &[device(1)])
        .expect("an admin with the owner's cosignature");
    assert_eq!(admitted.authority(), Authority::AdminOverride);
    assert_eq!(admitted.cosigners(), &[member(1)]);
    w.submit_as(device(2), manager_for_8.clone(), &[device(1)])
        .expect("applied");
    assert!(w.state.grant(&m(8)).is_some_and(|grant| grant.is_active()));
    // The new manager is a manager from then on: it signs, and the space is no longer orphaned.
    w.do_by(8, grant_op(m(9), SpaceRole::Viewer));
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), grant_op(m(10), SpaceRole::Manager), &[device(1)])),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );

    // A cosigner who is no admin does not count.
    let w = orphaned_business();
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(5)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    // Neither does a second device of the same person: a quorum counts distinct people.
    let mut w = orphaned_business();
    w.account.add_device(31, 2);
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(31)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    // Neither does a cosigner whose device is not active, or whose member is suspended.
    let mut w = orphaned_business();
    w.account.deactivate_device(1);
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(1)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    let mut w = orphaned_business();
    w.account.suspend(1);
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(1)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    // Neither does an app that holds an admin role: an app principal never counts.
    let mut w = orphaned_business();
    w.account
        .add_person(20, AccountRole::Admin, MemberKind::App);
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(20)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    // Neither does a cosignature that does not verify over this entry's preimage.
    let w = orphaned_business();
    let mut entry = w.build(device(2), manager_for_8.clone(), &[device(1)]);
    entry.cosigs[0].sig = azlin_seams::FakeScheme::sign(&device_key(1), b"another entry");
    assert_eq!(
        refusal_of(w.admit_entry(&entry)),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );
    // And a cosignature of a device the directory does not know counts for nothing.
    let mut w = orphaned_business();
    w.dir.keys.remove(&device(1));
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), manager_for_8.clone(), &[device(1)])),
        SpaceRefusal::QuorumShort { have: 1, need: 2 }
    );

    // In an account whose admin quorum is 1 (a team) the admin alone is enough.
    let mut w = World::new(AccountKind::Team);
    w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 7, 1));
    w.account.suspend(7);
    let admitted = w
        .try_admit_as(device(2), manager_for_8, &[])
        .expect("a team's quorum is one person");
    assert_eq!(admitted.authority(), Authority::AdminOverride);
    assert!(admitted.cosigners().is_empty());
}

#[test]
fn admin_override_adds_managers_only_and_only_to_an_orphaned_space() {
    // An orphaned space: the override adds a MANAGER, nothing else.
    let w = orphaned_business();
    for role in [SpaceRole::Editor, SpaceRole::Viewer, SpaceRole::Uploader] {
        assert_eq!(
            refusal_of(w.try_admit_as(device(2), grant_op(m(8), role), &[device(1)])),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::Grant
            },
            "{role:?}"
        );
    }
    // A space that has a manager that is really there is no orphan, however many admins cosign.
    let mut w = World::new(AccountKind::Business);
    w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 7, 1));
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), grant_op(m(8), SpaceRole::Manager), &[device(1)])),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
    // The manager the override adds may be a group; a group that exists is a manager that is really there,
    // so the space is no orphan after it.
    let mut w = orphaned_business();
    let admitted = w
        .try_admit_as(device(2), grant_op(g(1), SpaceRole::Manager), &[device(1)])
        .expect("a group may be the manager");
    assert_eq!(admitted.authority(), Authority::AdminOverride);
    w.submit_as(device(2), grant_op(g(1), SpaceRole::Manager), &[device(1)])
        .expect("applied");
    assert_eq!(
        refusal_of(w.try_admit_as(device(2), grant_op(m(9), SpaceRole::Manager), &[device(1)])),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
}

#[test]
fn an_external_member_never_gets_the_admin_override() {
    // A team space made for the member 7, who shares it with the external member 1 and is then suspended:
    // the space is orphaned (an external viewer is no manager).
    let mut w = World::new(AccountKind::Team);
    w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 7, 1));
    w.do_by(7, grant_op(x(1), SpaceRole::Viewer));
    w.account.suspend(7);
    // The external member's role in ITS OWN account is owner (see `World::new`): that gives it nothing
    // here, and no quorum changes it.
    assert_eq!(
        refusal_of(w.try_admit_as(
            ext_device(1),
            grant_op(x(1), SpaceRole::Manager),
            &[device(1)]
        )),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
    // An internal admin can still rescue the space.
    assert!(w
        .try_admit_as(device(2), grant_op(m(8), SpaceRole::Manager), &[])
        .is_ok());
    // And the external member does not revoke as an admin does: its own grant is the only one it may end.
    assert_eq!(
        refusal_of(w.try_admit_as(ext_device(1), revoke_op(m(7)), &[])),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Revoke
        }
    );
}

#[test]
fn revoke_by_the_grantee_is_leave_and_allowed() {
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, grant_op(x(1), SpaceRole::Editor));
    w.do_by(1, grant_op(m(6), SpaceRole::Uploader));

    // A viewer leaves.
    let admitted = w
        .try_admit_as(device(5), revoke_op(m(5)), &[])
        .expect("a grantee leaves");
    assert_eq!(admitted.authority(), Authority::SelfLeave);
    let effects = w.do_by(5, revoke_op(m(5)));
    assert!(effects.applied);
    assert!(!w.state.grant(&m(5)).expect("the row stays").is_active());
    // An external member leaves.
    assert_eq!(
        w.try_admit_as(ext_device(1), revoke_op(x(1)), &[])
            .expect("an external grantee leaves")
            .authority(),
        Authority::SelfLeave
    );
    // An uploader leaves.
    assert!(w.try_admit_as(device(6), revoke_op(m(6)), &[]).is_ok());

    // Leaving is leaving ONE'S OWN grant: the member 6 cannot end the member 5's, whatever it claims.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, grant_op(m(6), SpaceRole::Viewer));
    assert_eq!(
        w.refused_by(6, revoke_op(m(5))),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Revoke
        }
    );
    // A grant to a group is the group's: one of its members cannot leave it alone.
    w.account.put_in_groups(7, &[1]);
    w.do_by(1, grant_op(g(1), SpaceRole::Viewer));
    assert_eq!(
        w.refused_by(7, revoke_op(g(1))),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Revoke
        }
    );
    // A bearer capability does not leave through the chain: it has no signing key.
    w.do_by(1, grant_op(cap(1), SpaceRole::Viewer));
    assert_eq!(
        w.refused_by(7, revoke_op(cap(1))),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Revoke
        }
    );
    // A grant that is not in force cannot be revoked again.
    w.do_by(5, revoke_op(m(5)));
    assert_eq!(w.refused_by(1, revoke_op(m(5))), SpaceRefusal::NoSuchGrant);
}

#[test]
fn last_manager_cannot_be_revoked() {
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    // The only manager cannot leave, be revoked by an admin, or be made a viewer.
    assert_eq!(w.refused_by(1, revoke_op(m(1))), SpaceRefusal::LastManager);
    assert_eq!(w.refused_by(2, revoke_op(m(1))), SpaceRefusal::LastManager);
    assert_eq!(
        w.refused_by(1, grant_op(m(1), SpaceRole::Viewer)),
        SpaceRefusal::LastManager
    );
    // With a second manager the first may leave; then the second is the last.
    w.do_by(1, grant_op(m(2), SpaceRole::Manager));
    w.do_by(1, revoke_op(m(1)));
    assert_eq!(w.refused_by(2, revoke_op(m(2))), SpaceRefusal::LastManager);
    assert_eq!(
        w.refused_by(2, grant_op(m(2), SpaceRole::Editor)),
        SpaceRefusal::LastManager
    );
    // A manager who is no longer a member does not count: the one that is really there is the last.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(m(7), SpaceRole::Manager));
    w.account.suspend(7);
    assert_eq!(w.refused_by(2, revoke_op(m(1))), SpaceRefusal::LastManager);
    // The dead grant itself may be cleaned up (by an admin: the member 7 cannot sign any more).
    w.do_by(2, revoke_op(m(7)));
    // A manager grant to a group that exists counts as a manager that is really there.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(g(1), SpaceRole::Manager));
    w.do_by(1, revoke_op(m(1)));
    assert_eq!(w.refused_by(2, revoke_op(g(1))), SpaceRefusal::LastManager);
    // An external manager is a manager too (its standing in its own account is not this account's to see).
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(x(1), SpaceRole::Manager));
    w.do_by(1, revoke_op(m(1)));
    assert_eq!(w.refused_by(2, revoke_op(x(1))), SpaceRefusal::LastManager);
}

#[test]
fn suspended_or_removed_member_signs_nothing() {
    let attempts = |w: &World| {
        vec![
            (
                "grant",
                w.admit_by(1, grant_op(m(7), SpaceRole::Viewer)).map(|_| ()),
            ),
            ("revoke", w.admit_by(1, revoke_op(m(1))).map(|_| ())),
            ("rekey", w.admit_by(1, rekey_op(2, 2)).map(|_| ())),
            (
                "history",
                w.admit_by(1, history_op(HistoryMode::Forward)).map(|_| ()),
            ),
        ]
    };

    // The member is suspended or removed: none of its devices signs anything.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.account.suspend(1);
    for (what, result) in attempts(&w) {
        assert_eq!(result, Err(SpaceRefusal::MemberNotActive), "{what}");
    }

    // The device is not active (still in its enrollment hold, or revoked in the team log).
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.account.deactivate_device(1);
    for (what, result) in attempts(&w) {
        assert_eq!(result, Err(SpaceRefusal::SignerNotActive), "{what}");
    }

    // The directory's key of the device is revoked: the signature check says so, before the permission
    // check is asked.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    w.dir
        .errors
        .insert(device(1), azlin_keys::DirectoryError::Revoked);
    for (what, result) in attempts(&w) {
        assert_eq!(result, Err(SpaceRefusal::SignerNotActive), "{what}");
    }

    // The facts and the account's view must agree: facts that say "active" for a member the view does not
    // know as an active member are refused too.
    let mut w = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    let proposal = w.verify(&entry).expect("signatures hold");
    let mut who = w.facts(device(1));
    w.account.suspend(1);
    who.member_active = true;
    assert_eq!(
        refusal_of(admit(&w.state, &proposal, &who, &w.account, w.now())),
        SpaceRefusal::MemberNotActive
    );

    // A suspended member cannot create a space either.
    let mut w = World::new(AccountKind::Team);
    w.account.suspend(1);
    assert_eq!(
        w.refused_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1)),
        SpaceRefusal::MemberNotActive
    );
    let mut w = World::new(AccountKind::Team);
    w.account.deactivate_device(1);
    assert_eq!(
        w.refused_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1)),
        SpaceRefusal::SignerNotActive
    );
}

#[test]
fn app_principal_manager_may_rekey() {
    let mut w = World::created(
        AccountKind::Business,
        SpaceKind::Shared,
        HistoryMode::Forward,
    );
    w.account
        .add_person(20, AccountRole::Member, MemberKind::App);
    // The key agent holds the space key through a manager grant: it rotates forward.
    w.do_by(1, grant_op(m(20), SpaceRole::Manager));
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    let effects = w.do_by(20, rekey_op(2, 2));
    assert!(effects.applied);
    assert_eq!(w.state.key_epoch().map(|e| e.get()), Some(2));
    assert_eq!(
        w.state.epoch_record(epoch(2)).expect("committed").signer,
        device(20)
    );

    // It signs a rekey and its own leaving, nothing else.
    assert_eq!(
        w.refused_by(20, grant_op(m(9), SpaceRole::Viewer)),
        SpaceRefusal::AppMayOnlyRekey {
            op: SpaceOpName::Grant
        }
    );
    assert_eq!(
        w.refused_by(20, revoke_op(m(8))),
        SpaceRefusal::AppMayOnlyRekey {
            op: SpaceOpName::Revoke
        }
    );
    assert_eq!(
        w.refused_by(20, history_op(HistoryMode::Full)),
        SpaceRefusal::AppMayOnlyRekey {
            op: SpaceOpName::History
        }
    );
    assert!(w.try_admit_as(device(20), revoke_op(m(20)), &[]).is_ok());

    // An app that is only a viewer does not rekey: the manage capability is the grant's.
    let mut w = World::created(
        AccountKind::Business,
        SpaceKind::Shared,
        HistoryMode::Forward,
    );
    w.account
        .add_person(20, AccountRole::Member, MemberKind::App);
    w.do_by(1, grant_op(m(20), SpaceRole::Viewer));
    assert_eq!(
        w.refused_by(20, rekey_op(2, 2)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Rekey
        }
    );

    // An app does not create a space.
    let mut w = World::new(AccountKind::Business);
    w.account
        .add_person(20, AccountRole::Owner, MemberKind::App);
    assert_eq!(
        w.refused_by(20, create_op(SpaceKind::Shared, HistoryMode::Full, 20, 1)),
        SpaceRefusal::AppMayOnlyRekey {
            op: SpaceOpName::Create
        }
    );
}
