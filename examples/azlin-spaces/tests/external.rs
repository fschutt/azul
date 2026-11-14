//! Check 4 of the brief (external members and policy, and S14): an external grant is refused when the
//! policy is "same workspace only" and allowed otherwise, an external grantee pays its own quota up to the
//! cap the owner set, an owner-paid grant charges the owner, an external member signs only as a manager;
//! and the checks on the arguments of a grant (the grantee, the cap, the limits).
//!
//! Implements planning/architecture.md section 5.9 ("external members only if `external_sharing`
//! allows"), section 5.10 (`external_sharing`), section 5.12 ("External members ... The share caps and
//! payer rule apply") and section 6.10 S14 ("A participant's writes count against the participant's own
//! quota, with a per-share cap set by the owner").

mod common;

use azlin_policy::ExternalSharing;
use azlin_seams::{AccountKind, Hash32, MemberId};
use azlin_spaces::{
    access, charge, Charge, ChargeRefusal, Grantee, HistoryMode, Payer, Principal, Source,
    SpaceDelta, SpaceKind, SpaceOpName, SpaceRefusal, SpaceRole, WrapNeed, MAX_CAPABILITIES,
    MAX_GRANTS,
};
use common::*;

fn team() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

fn ext_principal(n: u8) -> Principal {
    Principal::External {
        account: account(2),
        member: ext_member(n),
    }
}

fn member_principal(n: u8) -> Principal {
    Principal::Member {
        member: member(n),
        groups: Vec::new(),
    }
}

#[test]
fn external_grant_refused_when_policy_is_same_workspace_only() {
    // A business: "same workspace only" is the default of its column, and a workspace is an account plus
    // its domain: another account is never the same workspace, so EVERY external grant is refused.
    let mut w = World::created(AccountKind::Business, SpaceKind::Shared, HistoryMode::Full);
    assert_eq!(
        w.account.policy.external_sharing,
        ExternalSharing::SameWorkspaceOnly
    );
    for role in SpaceRole::ALL {
        for payer in [Payer::Owner, Payer::Grantee] {
            let cap_bytes = if matches!(role, SpaceRole::Viewer) {
                None
            } else {
                Some(1_000)
            };
            assert_eq!(
                w.refused_by(1, grant_full(x(1), *role, cap_bytes, payer)),
                SpaceRefusal::ExternalSharingForbidden,
                "{role:?} / {payer:?}"
            );
        }
    }
    // A team that tightened its policy refuses them too, and one that loosened it again allows them.
    let mut w = team();
    w.account.policy.external_sharing = ExternalSharing::SameWorkspaceOnly;
    assert_eq!(
        w.refused_by(1, grant_op(x(1), SpaceRole::Viewer)),
        SpaceRefusal::ExternalSharingForbidden
    );
    w.account.policy.external_sharing = ExternalSharing::Allowed;
    w.do_by(1, grant_op(x(1), SpaceRole::Viewer));
    // The members of the account are not external: the policy does not touch them.
    let mut w = World::created(AccountKind::Business, SpaceKind::Shared, HistoryMode::Full);
    w.do_by(1, grant_op(m(5), SpaceRole::Editor));
}

#[test]
fn external_grant_allowed_when_policy_allows() {
    for kind in [
        AccountKind::Personal,
        AccountKind::Family,
        AccountKind::Team,
    ] {
        let mut w = World::created(kind, SpaceKind::Shared, HistoryMode::Full);
        let effects = w.do_by(
            1,
            grant_full(x(1), SpaceRole::Editor, Some(1_000), Payer::Grantee),
        );
        let grant = w.state.grant(&x(1)).expect("the external grant is a row");
        assert!(grant.is_active());
        assert_eq!(grant.role, SpaceRole::Editor);
        assert_eq!(grant.cap_bytes, Some(bytes(1_000)));
        assert_eq!(grant.payer, Payer::Grantee);
        assert_eq!(grant.source, Source::Chain(seq(1)));
        // The external reader needs a wrap of the current epoch.
        assert_eq!(
            effects.wraps,
            vec![WrapNeed {
                to: x(1),
                epoch: epoch(1)
            }]
        );
        assert!(effects
            .rows
            .iter()
            .any(|row| matches!(row, SpaceDelta::Grant(g) if g.grantee == x(1))));
        // It reads and writes through the access view, as the member of ANOTHER account it is.
        let caps = access(&w.state, &ext_principal(1), at(5_000));
        assert!(caps.can_read() && caps.can_write() && !caps.can_manage());
    }
}

#[test]
fn external_grantee_pays_its_own_quota_up_to_cap_bytes() {
    let mut w = team();
    w.do_by(
        1,
        grant_full(x(1), SpaceRole::Editor, Some(1_000), Payer::Grantee),
    );
    let writer = ext_principal(1);
    // 400 of 1,000: the grantee's quota pays, 600 are left.
    assert_eq!(
        charge(&w.state, &writer, bytes(400), bytes(0)),
        Charge::Grantee {
            cap_left: Some(bytes(600))
        }
    );
    // Another 600, on top of the 400 spent: exactly up to the cap is fine, nothing is left.
    assert_eq!(
        charge(&w.state, &writer, bytes(600), bytes(400)),
        Charge::Grantee {
            cap_left: Some(bytes(0))
        }
    );
    // The whole cap in one write.
    assert_eq!(
        charge(&w.state, &writer, bytes(1_000), bytes(0)),
        Charge::Grantee {
            cap_left: Some(bytes(0))
        }
    );
    // A grantee-paid grant without a cap has no ceiling but the grantee's own quota.
    w.do_by(1, grant_full(x(2), SpaceRole::Editor, None, Payer::Grantee));
    assert_eq!(
        charge(
            &w.state,
            &ext_principal(2),
            bytes(5_000_000),
            bytes(9_000_000)
        ),
        Charge::Grantee { cap_left: None }
    );
}

#[test]
fn charge_refuses_past_the_cap() {
    let mut w = team();
    w.do_by(
        1,
        grant_full(x(1), SpaceRole::Editor, Some(1_000), Payer::Grantee),
    );
    let writer = ext_principal(1);
    let past = Charge::Refused(ChargeRefusal::CapExceeded);
    // One byte past the cap.
    assert_eq!(charge(&w.state, &writer, bytes(601), bytes(400)), past);
    assert_eq!(charge(&w.state, &writer, bytes(1_001), bytes(0)), past);
    // A writer that is already past the cap (the cap was lowered, or the tally is stale) writes nothing.
    assert_eq!(charge(&w.state, &writer, bytes(1), bytes(1_000)), past);
    assert_eq!(charge(&w.state, &writer, bytes(0), bytes(1_001)), past);
    // A number that overflows is past the cap too, never a wrap-around to a small one.
    assert_eq!(charge(&w.state, &writer, bytes(u64::MAX), bytes(2)), past);
    assert_eq!(
        charge(&w.state, &writer, bytes(u64::MAX), bytes(u64::MAX)),
        past
    );
}

#[test]
fn owner_payer_charges_the_owner() {
    let mut w = team();
    w.do_by(1, grant_full(m(5), SpaceRole::Editor, None, Payer::Owner));
    w.do_by(
        1,
        grant_full(m(6), SpaceRole::Editor, Some(100), Payer::Owner),
    );
    // No cap: the owner pays for any write.
    assert_eq!(
        charge(
            &w.state,
            &member_principal(5),
            bytes(7_000_000_000),
            bytes(1)
        ),
        Charge::Owner
    );
    // A cap bounds what the writer may put into the share whoever pays: the owner chose 100.
    assert_eq!(
        charge(&w.state, &member_principal(6), bytes(100), bytes(0)),
        Charge::Owner
    );
    assert_eq!(
        charge(&w.state, &member_principal(6), bytes(101), bytes(0)),
        Charge::Refused(ChargeRefusal::CapExceeded)
    );
    assert_eq!(
        charge(&w.state, &member_principal(6), bytes(60), bytes(41)),
        Charge::Refused(ChargeRefusal::CapExceeded)
    );
}

#[test]
fn external_member_cannot_grant_unless_manager() {
    for role in [SpaceRole::Editor, SpaceRole::Viewer, SpaceRole::Uploader] {
        let mut w = team();
        w.do_by(1, grant_op(x(1), role));
        assert_eq!(
            refusal_of(w.try_admit_as(ext_device(1), grant_op(m(7), SpaceRole::Viewer), &[])),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::Grant
            },
            "{role:?}"
        );
        // Not even with the member's own account role of owner (the facts say so: see World::new).
        assert_eq!(w.facts(ext_device(1)).account_role.as_str(), "owner");
    }
    // A manager that is external signs like any manager.
    let mut w = team();
    w.do_by(1, grant_op(x(1), SpaceRole::Manager));
    let effects = w.do_by_ext(1, grant_op(m(7), SpaceRole::Viewer));
    assert!(effects.applied);
    assert!(w.state.grant(&m(7)).is_some_and(|grant| grant.is_active()));
}

#[test]
fn an_external_member_with_an_internal_members_id_gets_no_internal_grant() {
    // The member 5 of THIS account is a manager. A member of another account that happens to carry the
    // same member id is somebody else: it holds the grants that name its own account, and nothing else.
    let mut w = team();
    w.do_by(1, grant_op(m(5), SpaceRole::Manager));
    let impostor_facts = azlin_spaces::SignerFacts {
        device: ext_device(1),
        member: member(5),
        account: account(2),
        member_kind: azlin_seams::MemberKind::Person,
        device_active: true,
        member_active: true,
        account_role: azlin_seams::AccountRole::Owner,
    };
    let entry = w.build(ext_device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    let proposal = w.verify(&entry).expect("the signatures hold");
    assert_eq!(
        refusal_of(azlin_spaces::admit(
            &w.state,
            &proposal,
            &impostor_facts,
            &w.account,
            w.now()
        )),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
    // The same in the access view: the principal of the other account gets nothing of the member 5's.
    let impostor = Principal::External {
        account: account(2),
        member: member(5),
    };
    assert!(access(&w.state, &impostor, at(5_000)).is_empty());
    assert!(access(&w.state, &member_principal(5), at(5_000)).can_manage());
    // And an external grant names the account AND the member: another account's member 1 is not ours.
    w.do_by(1, grant_op(x(1), SpaceRole::Editor));
    let other_account = Principal::External {
        account: account(3),
        member: ext_member(1),
    };
    assert!(access(&w.state, &other_account, at(5_000)).is_empty());
    assert!(access(&w.state, &ext_principal(1), at(5_000)).can_write());
}

#[test]
fn an_external_grant_cannot_name_this_account_or_one_of_its_members() {
    let mut w = team();
    for grantee in [
        // This very account.
        Grantee::External {
            account: account(1),
            member: ext_member(1),
        },
        // Another account's id, but a member of this one.
        Grantee::External {
            account: account(2),
            member: member(5),
        },
    ] {
        assert_eq!(
            w.refused_by(1, grant_op(grantee, SpaceRole::Viewer)),
            SpaceRefusal::ExternalIsInternal,
            "{grantee:?}"
        );
    }
}

#[test]
fn the_grantee_of_a_grant_must_exist() {
    let mut w = team();
    // A member who is not a member of the account, and one who is suspended.
    assert_eq!(
        w.refused_by(1, grant_op(m(99), SpaceRole::Viewer)),
        SpaceRefusal::GranteeNotAMember
    );
    w.account.suspend(9);
    assert_eq!(
        w.refused_by(1, grant_op(m(9), SpaceRole::Viewer)),
        SpaceRefusal::GranteeNotAMember
    );
    // A group that does not exist.
    assert_eq!(
        w.refused_by(1, grant_op(g(9), SpaceRole::Viewer)),
        SpaceRefusal::GranteeGroupUnknown
    );
    // A group that does.
    w.do_by(1, grant_op(g(1), SpaceRole::Viewer));
}

#[test]
fn a_cap_needs_a_role_that_writes_and_is_more_than_nothing() {
    let mut w = team();
    assert_eq!(
        w.refused_by(
            1,
            grant_full(m(5), SpaceRole::Viewer, Some(100), Payer::Owner)
        ),
        SpaceRefusal::CapNeedsWriteRole
    );
    for role in [SpaceRole::Editor, SpaceRole::Uploader, SpaceRole::Manager] {
        assert_eq!(
            w.refused_by(1, grant_full(m(5), role, Some(0), Payer::Owner)),
            SpaceRefusal::CapIsZero,
            "{role:?}"
        );
        assert!(w
            .admit_by(1, grant_full(m(5), role, Some(1), Payer::Owner))
            .is_ok());
    }
}

#[test]
fn several_write_grants_use_the_one_with_the_most_room() {
    // The member 5 may write through two grants: directly (cap 100, the owner pays) and through the group
    // 1 (cap 500, the grantee pays). It is charged under the one that leaves it the most room.
    let mut w = team();
    w.do_by(
        1,
        grant_full(m(5), SpaceRole::Editor, Some(100), Payer::Owner),
    );
    w.do_by(
        1,
        grant_full(g(1), SpaceRole::Editor, Some(500), Payer::Grantee),
    );
    let writer = Principal::Member {
        member: member(5),
        groups: vec![group(1)],
    };
    assert_eq!(
        charge(&w.state, &writer, bytes(50), bytes(0)),
        Charge::Grantee {
            cap_left: Some(bytes(450))
        }
    );
    // After 450 are spent the direct grant (room 0) and the group's (room 50) have changed places in
    // nothing: the group's still has more.
    assert_eq!(
        charge(&w.state, &writer, bytes(50), bytes(450)),
        Charge::Grantee {
            cap_left: Some(bytes(0))
        }
    );
    // No cap at all is the most room, whoever pays.
    let mut w = team();
    w.do_by(1, grant_full(m(5), SpaceRole::Editor, None, Payer::Owner));
    w.do_by(
        1,
        grant_full(g(1), SpaceRole::Editor, Some(500), Payer::Grantee),
    );
    assert_eq!(
        charge(&w.state, &writer, bytes(50), bytes(0)),
        Charge::Owner
    );
    // On a tie the grantee pays: the owner's money is the one to spare.
    let mut w = team();
    w.do_by(
        1,
        grant_full(m(5), SpaceRole::Editor, Some(500), Payer::Owner),
    );
    w.do_by(
        1,
        grant_full(g(1), SpaceRole::Editor, Some(500), Payer::Grantee),
    );
    assert_eq!(
        charge(&w.state, &writer, bytes(50), bytes(0)),
        Charge::Grantee {
            cap_left: Some(bytes(450))
        }
    );
    // A viewer grant is no write grant: it does not offer room.
    let mut w = team();
    w.do_by(
        1,
        grant_full(m(5), SpaceRole::Editor, Some(100), Payer::Owner),
    );
    w.do_by(1, grant_op(g(1), SpaceRole::Viewer));
    assert_eq!(
        charge(&w.state, &writer, bytes(50), bytes(0)),
        Charge::Owner
    );
}

#[test]
fn a_write_by_a_principal_without_a_write_grant_or_into_an_archived_space_is_refused() {
    let mut w = team();
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, grant_op(m(6), SpaceRole::Editor));
    let nobody = member_principal(9);
    assert_eq!(
        charge(&w.state, &nobody, bytes(1), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
    assert_eq!(
        charge(&w.state, &member_principal(5), bytes(1), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
    // The editor is charged to the owner (the default payer) ...
    assert_eq!(
        charge(&w.state, &member_principal(6), bytes(1), bytes(0)),
        Charge::Owner
    );
    // ... until the space is archived: nothing is written into it.
    w.state.archive(at(5_000));
    assert_eq!(
        charge(&w.state, &member_principal(6), bytes(1), bytes(0)),
        Charge::Refused(ChargeRefusal::Archived)
    );
}

#[test]
fn a_revoked_grant_charges_nobody() {
    let mut w = team();
    w.do_by(
        1,
        grant_full(x(1), SpaceRole::Editor, Some(100), Payer::Grantee),
    );
    assert!(matches!(
        charge(&w.state, &ext_principal(1), bytes(1), bytes(0)),
        Charge::Grantee { .. }
    ));
    w.do_by(1, revoke_op(x(1)));
    assert_eq!(
        charge(&w.state, &ext_principal(1), bytes(1), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
}

#[test]
fn a_space_holds_at_most_max_grants_rows_and_max_capabilities_capabilities() {
    let ext = |i: usize| Grantee::External {
        account: account(2),
        member: MemberId::from_bytes({
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&(i as u64).to_be_bytes());
            id[8] = 0xAB;
            id
        }),
    };
    let mut w = team();
    // The first manager is one row; fill the rest.
    for i in 0..(MAX_GRANTS - 1) {
        w.do_by(1, grant_op(ext(i), SpaceRole::Viewer));
    }
    assert_eq!(w.state.grants().count(), MAX_GRANTS);
    // A grantee that has no row is refused ...
    assert_eq!(
        w.refused_by(1, grant_op(ext(MAX_GRANTS), SpaceRole::Viewer)),
        SpaceRefusal::TooManyGrants
    );
    assert_eq!(
        w.refused_by(1, grant_op(m(5), SpaceRole::Viewer)),
        SpaceRefusal::TooManyGrants
    );
    // ... one that has one may be changed or ended.
    w.do_by(1, grant_op(ext(0), SpaceRole::Editor));
    w.do_by(1, revoke_op(ext(1)));
    assert_eq!(w.state.grants().count(), MAX_GRANTS);

    let capability = |i: usize| {
        let mut hash = [0u8; 32];
        hash[..8].copy_from_slice(&(i as u64).to_be_bytes());
        hash[8] = 0xCD;
        Grantee::Capability(Hash32::from_bytes(hash))
    };
    let mut w = team();
    for i in 0..MAX_CAPABILITIES {
        w.do_by(1, grant_op(capability(i), SpaceRole::Viewer));
    }
    assert_eq!(
        w.refused_by(1, grant_op(capability(MAX_CAPABILITIES), SpaceRole::Viewer)),
        SpaceRefusal::TooManyCapabilities
    );
    // Other grantees are not counted against the capabilities' bound.
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    // An existing capability may be replaced.
    w.do_by(1, grant_op(capability(0), SpaceRole::Editor));
}
