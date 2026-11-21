//! Check 3 of the brief (roles as capabilities): the one role table (manager = read, write, manage;
//! editor = read and write; viewer = read; uploader = write only), the union of several grants (direct and
//! through groups, with the nesting resolved by the caller), and the strict words of the vocabularies.
//!
//! Implements planning/architecture.md section 5.4 ("the one role vocabulary") and section 7.2 (resource
//! roles, mapped to grants).

mod common;

use std::str::FromStr;

use azlin_seams::AccountKind;
use azlin_spaces::{
    access, charge, role_caps, union, Caps, Charge, ChargeRefusal, GrantState, HistoryMode, Payer,
    Principal, RekeyReason, SpaceKind, SpaceOpName, SpaceRefusal, SpaceRole,
};
use common::*;

/// The table of the brief, typed here as (read, write, manage).
fn table(role: SpaceRole) -> (bool, bool, bool) {
    match role {
        SpaceRole::Manager => (true, true, true),
        SpaceRole::Editor => (true, true, false),
        SpaceRole::Viewer => (true, false, false),
        SpaceRole::Uploader => (false, true, false),
    }
}

fn flags(caps: Caps) -> (bool, bool, bool) {
    (caps.can_read(), caps.can_write(), caps.can_manage())
}

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

fn member_principal(n: u8, groups: &[u8]) -> Principal {
    Principal::Member {
        member: member(n),
        groups: groups.iter().map(|id| group(*id)).collect(),
    }
}

#[test]
fn role_caps_is_the_table_of_the_brief() {
    for role in SpaceRole::ALL {
        assert_eq!(flags(role_caps(*role)), table(*role), "{role:?}");
    }
    assert_eq!(SpaceRole::ALL.len(), 4);
}

#[test]
fn uploader_cannot_read() {
    assert!(!role_caps(SpaceRole::Uploader).can_read());
    assert!(role_caps(SpaceRole::Uploader).can_write());
    assert!(!role_caps(SpaceRole::Uploader).can_manage());

    // Through the access view of a space.
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Uploader));
    let caps = access(&w.state, &member_principal(5, &[]), at(2_000));
    assert!(!caps.can_read() && caps.can_write() && !caps.can_manage());
    // It may write, and is charged for it.
    assert_eq!(
        charge(&w.state, &member_principal(5, &[]), bytes(10), bytes(0)),
        Charge::Owner
    );
}

#[test]
fn viewer_cannot_write() {
    assert!(role_caps(SpaceRole::Viewer).can_read());
    assert!(!role_caps(SpaceRole::Viewer).can_write());
    assert!(!role_caps(SpaceRole::Viewer).can_manage());

    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    let caps = access(&w.state, &member_principal(5, &[]), at(2_000));
    assert!(caps.can_read() && !caps.can_write() && !caps.can_manage());
    assert_eq!(
        charge(&w.state, &member_principal(5, &[]), bytes(10), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
}

#[test]
fn union_of_viewer_and_uploader_reads_and_writes() {
    let both = union(role_caps(SpaceRole::Viewer), role_caps(SpaceRole::Uploader));
    assert_eq!(flags(both), (true, true, false));

    // Every pair of roles: the union is the capability-wise OR, typed here independently.
    for a in SpaceRole::ALL {
        for b in SpaceRole::ALL {
            let (ar, aw, am) = table(*a);
            let (br, bw, bm) = table(*b);
            assert_eq!(
                flags(union(role_caps(*a), role_caps(*b))),
                (ar || br, aw || bw, am || bm),
                "{a:?} + {b:?}"
            );
        }
    }
    // And through two grants of one member: a viewer directly, an uploader through a group.
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, grant_op(g(1), SpaceRole::Uploader));
    let caps = access(&w.state, &member_principal(5, &[1]), at(2_000));
    assert_eq!(flags(caps), (true, true, false));
    // Without the group only the viewer is left.
    let caps = access(&w.state, &member_principal(5, &[]), at(2_000));
    assert_eq!(flags(caps), (true, false, false));
}

#[test]
fn effective_caps_union_direct_and_group_grants_through_nesting() {
    // The groups nest: 1 inside 2 inside 3. The account lists, for a member, every group it is in with
    // the ancestors of its groups (nesting is resolved by the team log), so a grant to the OUTER group
    // reaches the members of the inner one.
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, grant_op(g(1), SpaceRole::Uploader));
    w.do_by(1, grant_op(g(3), SpaceRole::Editor));
    let everyone = member_principal(5, &[1, 2, 3]);
    assert_eq!(
        flags(access(&w.state, &everyone, at(2_000))),
        (true, true, false)
    );

    // A grant to the middle group changes nothing for those who are not in it, and gives manage to those
    // who are (through the chain of ancestors).
    w.do_by(1, grant_op(g(2), SpaceRole::Manager));
    assert_eq!(
        flags(access(
            &w.state,
            &member_principal(5, &[1, 2, 3]),
            at(2_000)
        )),
        (true, true, true)
    );
    assert_eq!(
        flags(access(&w.state, &member_principal(5, &[3]), at(2_000))),
        (true, true, false),
        "the outer group only: its editor grant, and the direct viewer grant"
    );
    assert_eq!(
        flags(access(&w.state, &member_principal(6, &[]), at(2_000))),
        (false, false, false),
        "a member with no grant and no group gets nothing"
    );
    assert!(access(&w.state, &member_principal(6, &[]), at(2_000)).is_empty());
    // A principal in the uploader group only writes, and does not read.
    assert!(access(&w.state, &member_principal(7, &[1]), at(2_000)).can_write());
    assert!(!access(&w.state, &member_principal(7, &[1]), at(2_000)).can_read());
}

#[test]
fn a_manager_through_a_group_may_grant() {
    // The member 5 is a manager only through the group 2: the account says so, the chain says the group
    // is a manager, and `admit` unions the grants of the signer's groups.
    let mut w = created();
    w.account.put_in_groups(5, &[2]);
    w.do_by(1, grant_op(g(2), SpaceRole::Manager));
    w.do_by(5, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(5, rekey_op(2, 2));
    // Out of the group it is nobody.
    w.account.put_in_groups(5, &[]);
    assert_eq!(
        w.refused_by(5, grant_op(m(8), SpaceRole::Viewer)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Grant
        }
    );
}

#[test]
fn a_revoked_manager_signs_nothing() {
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Manager));
    w.do_by(5, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(1, revoke_op(m(5)));
    assert!(!w.state.grant(&m(5)).expect("the row").is_active());
    for op in [
        grant_op(m(8), SpaceRole::Viewer),
        revoke_op(m(7)),
        rekey_op(2, 2),
        history_op(HistoryMode::Forward),
    ] {
        assert!(
            matches!(w.refused_by(5, op), SpaceRefusal::NotAManager { .. }),
            "a revoked manager"
        );
    }
    // A revoked grant gives nothing in the access view either.
    assert!(access(&w.state, &member_principal(5, &[]), at(2_000)).is_empty());
    // A member that is re-granted holds the new grant, not the old one.
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    let caps = access(&w.state, &member_principal(5, &[]), at(2_000));
    assert_eq!(flags(caps), (true, false, false));
    assert_eq!(
        w.state.grant(&m(5)).expect("the row").state,
        GrantState::Active
    );
}

#[test]
fn caps_have_the_helpers_the_glue_maps_from() {
    assert!(Caps::NONE.is_empty());
    assert!(!Caps::READ.is_empty());
    let all = union(union(Caps::READ, Caps::WRITE), Caps::MANAGE);
    assert_eq!(all, role_caps(SpaceRole::Manager));
    assert!(all.contains(Caps::READ) && all.contains(Caps::WRITE) && all.contains(Caps::MANAGE));
    assert!(!Caps::READ.contains(Caps::WRITE));
    assert_eq!(all.intersect(Caps::READ), Caps::READ);
    assert_eq!(Caps::READ.intersect(Caps::WRITE), Caps::NONE);
    assert_eq!(format!("{:?}", Caps::NONE), "Caps(-)");
    assert_eq!(format!("{:?}", Caps::READ), "Caps(R)");
    assert_eq!(format!("{all:?}"), "Caps(R|W|M)");
    assert_eq!(format!("{:?}", role_caps(SpaceRole::Uploader)), "Caps(W)");
}

/// The words of a vocabulary: every value round-trips, and nothing else parses.
fn words_are_strict<T>(all: &[T])
where
    T: FromStr + std::fmt::Display + std::fmt::Debug + PartialEq + Copy,
    <T as FromStr>::Err: std::fmt::Debug,
{
    for value in all {
        let word = value.to_string();
        assert_eq!(T::from_str(&word).expect("a word parses"), *value);
        assert_eq!(word, word.to_lowercase(), "lower case: {word}");
        assert!(
            T::from_str(&word.to_uppercase()).is_err(),
            "{word} in capitals"
        );
        assert!(
            T::from_str(&format!(" {word}")).is_err(),
            "{word} with a space"
        );
        assert!(
            T::from_str(&format!("{word}s")).is_err(),
            "{word} with a suffix"
        );
    }
    assert!(T::from_str("").is_err());
    assert!(T::from_str("nonsense").is_err());
}

#[test]
fn the_vocabularies_have_strict_words() {
    words_are_strict(SpaceRole::ALL);
    words_are_strict(SpaceKind::ALL);
    words_are_strict(HistoryMode::ALL);
    words_are_strict(Payer::ALL);
    words_are_strict(GrantState::ALL);
    words_are_strict(RekeyReason::ALL);
    words_are_strict(SpaceOpName::ALL);
    assert_eq!(SpaceKind::ALL.len(), 7);
    assert_eq!(SpaceOpName::Rekey.as_str(), "space_rekey");
    assert_eq!(SpaceKind::PrivateMember.as_str(), "private_member");
    assert_eq!(SpaceKind::SystemAudit.as_str(), "system_audit");
    // The order of severity of a rekey: a removal is the worst.
    assert!(RekeyReason::Removal > RekeyReason::Joiner);
    assert!(RekeyReason::Joiner > RekeyReason::Manual);
}
