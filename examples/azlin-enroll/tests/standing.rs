//! The standing of the ones a request depends on, judged AGAIN when the key goes out
//! (planning/architecture.md section 7.5: "Any admin or any of the person's devices can cancel";
//! section 7.6: the holders of an opening are admins; section 12 E2).
//!
//! `release` judges a request by what the request itself holds; `mark_released`, which the group
//! calls before it serves the wrap, takes the facts of the moment and refuses when somebody the
//! request depends on does not stand any more: the device that approved was revoked, the admins that
//! signed were suspended, the admin who compared the phrase left, an escrow holder is no admin now,
//! the single-admin workspace has a second admin. The request then stays open and is judged again
//! with the facts of the next moment.

mod common;

use azlin_enroll::{
    release, shortfall, shortfall_for, Confirmer, Contribution, ContributionRefusal, Evidence,
    KeyAgentChecks, Opener, Path, Release, ReleaseGrant, ReleaseRefusal, Request, RequestState,
    Served, Shortfall, StampCheck, StandingLost,
};
use azlin_policy::Signatory;
use common::{
    add_evidence, all_checks_pass, at, device, hold_secs, mark_released, member, nd, open, Scene,
    AGENT, BASE, PERSON,
};

/// The grant of an open request at the end of its hold.
fn grant_of(request: &Request) -> ReleaseGrant {
    match release(request, at(hold_secs(request.path())), StampCheck::Agreed) {
        Release::Granted(grant) => grant,
        other => panic!("{} must release: {other:?}", request.path()),
    }
}

/// What the standing refusal says; any other answer is a failure of the test.
fn lost(answer: Result<Served, ReleaseRefusal>) -> StandingLost {
    match answer {
        Err(ReleaseRefusal::NoLongerStands(what)) => what,
        other => panic!("expected a lost standing, got {other:?}"),
    }
}

fn agent() -> Contribution {
    Contribution::key_agent(member(AGENT), all_checks_pass())
}

#[test]
fn a_revoked_approving_device_does_not_finish_an_own_device_request() {
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(0));
    scene.complete(&mut request, at(0));
    let grant = grant_of(&request);
    // The approving device (device 1) was revoked between the approval and the moment the key
    // goes out: a thief who held it for a minute does not get a key out of a request it opened.
    scene.facts.active_devices.retain(|d| *d != device(1));
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Approver
    );
    assert!(request.is_open(), "nothing was released");
    // The request is judged again with the facts of the next moment: with the device back it goes.
    scene.facts.active_devices.push(device(1));
    let grant = grant_of(&request);
    mark_released(&mut request, &scene.facts, grant).expect("the approver stands again");
    assert!(matches!(request.state(), RequestState::Released { .. }));
}

#[test]
fn admins_who_signed_and_were_suspended_do_not_finish_a_quorum_enrollment() {
    let mut scene = Scene::full();
    // Signed by admins 11 and 12: the quorum of a business is two.
    let mut request = scene.open(Path::QuorumEnrollment, at(0));
    scene.complete(&mut request, at(0));
    let grant = grant_of(&request);
    // Admin 12 was suspended: one admin is no quorum.
    scene.facts.admins.retain(|a| *a != member(12));
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Signers { have: 1, need: 2 }
    );
    assert!(request.is_open());
    // An admin who signed nothing (10) leaving changes nothing for this request.
    scene.facts.admins = vec![member(11), member(12)];
    let grant = grant_of(&request);
    mark_released(&mut request, &scene.facts, grant).expect("both signers stand");
}

#[test]
fn the_admin_who_confirmed_the_phrase_must_still_be_an_admin() {
    let mut scene = Scene::full();
    scene.facts.admins.push(member(13));
    scene.facts.admin_count = 4;
    let signers = vec![
        Signatory::person(member(11)),
        Signatory::person(member(12)),
        Signatory::person(member(13)),
    ];
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::QuorumEnrollment,
        Opener::admins(signers, nd()),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    add_evidence(
        &mut request,
        Evidence::PhraseConfirmed(Confirmer::Member(member(11))),
        at(0),
    )
    .unwrap();
    let grant = grant_of(&request);
    // Admin 11, who compared the phrase with the person, was suspended: 12 and 13 are still a
    // quorum of two, but nobody who still stands compared the phrase.
    scene.facts.admins.retain(|a| *a != member(11));
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Confirmer
    );
    assert!(request.is_open());
}

#[test]
fn an_escrow_holder_who_is_no_admin_any_more_gives_no_share() {
    // An opening that admins 10 and 11 signed, with the share of admin 12 and the key agent's.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    azlin_enroll::contribute(&mut request, &scene.facts, agent(), at(0)).unwrap();
    azlin_enroll::contribute(
        &mut request,
        &scene.facts,
        Contribution::admin(member(12)),
        at(0),
    )
    .unwrap();
    assert_eq!(shortfall(&request), None);
    let grant = grant_of(&request);
    scene.facts.admins.retain(|a| *a != member(12));
    // As the record has it the opening is complete; counting only the holders who still stand it
    // is not: the key agent is a share and no person.
    assert_eq!(shortfall(&request), None);
    assert_eq!(
        shortfall_for(&request, &scene.facts),
        Some(Shortfall::People { have: 1, need: 2 })
    );
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Escrow(Shortfall::People { have: 1, need: 2 })
    );
    assert!(request.is_open());
    // A recovery: the person asks, and admins 11 and 12 hold shares.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    add_evidence(&mut request, common::sso_of(PERSON, at(0)), at(0)).unwrap();
    for holder in [11u8, 12] {
        azlin_enroll::contribute(
            &mut request,
            &scene.facts,
            Contribution::admin(member(holder)),
            at(0),
        )
        .unwrap();
    }
    let grant = grant_of(&request);
    scene.facts.admins.retain(|a| *a != member(12));
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Escrow(Shortfall::Shares { have: 1, need: 2 })
    );
    // The key agent has no standing of the team's to lose: with the agent's share instead of 12's
    // the opening goes out whatever happens to the admins that did not hold a share.
    azlin_enroll::contribute(&mut request, &scene.facts, agent(), at(1)).unwrap();
    scene.facts.admins.retain(|a| *a != member(10));
    let grant = grant_of(&request);
    mark_released(&mut request, &scene.facts, grant).expect("11 and the agent");
}

#[test]
fn the_single_admin_exception_holds_only_while_the_workspace_has_one_admin() {
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 1;
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        Opener::admins(vec![Signatory::person(member(5))], nd()),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    for c in [agent(), Contribution::owner_kit(member(5))] {
        azlin_enroll::contribute(&mut request, &scene.facts, c, at(0)).unwrap();
    }
    assert_eq!(shortfall(&request), None);
    let grant = grant_of(&request);
    // A second admin joined: "the owner plus the kit" is no opening of this workspace any more, the
    // two people of the quorum are needed.
    scene.facts.admins.push(member(6));
    scene.facts.admin_count = 2;
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Escrow(Shortfall::People { have: 1, need: 2 })
    );
    // The second admin holds a share: two people, three shares.
    azlin_enroll::contribute(
        &mut request,
        &scene.facts,
        Contribution::admin(member(6)),
        at(1),
    )
    .unwrap();
    let grant = grant_of(&request);
    mark_released(&mut request, &scene.facts, grant).expect("two people and three shares");
    // The sole admin who is gone: nobody who signed the opening stands.
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 1;
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        Opener::admins(vec![Signatory::person(member(5))], nd()),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    for c in [agent(), Contribution::owner_kit(member(5))] {
        azlin_enroll::contribute(&mut request, &scene.facts, c, at(0)).unwrap();
    }
    let grant = grant_of(&request);
    scene.facts.admins.clear();
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Signers { have: 0, need: 1 }
    );
}

#[test]
fn a_grant_is_stale_after_the_life_of_the_request() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    let first = grant_of(&request);
    let second = grant_of(&request);
    let late = at(request.expires_at().as_secs() - BASE + 1);
    // The grant was decided inside the life of the request; the call comes after it.
    assert_eq!(
        azlin_enroll::mark_released(&mut request, &scene.facts, first, late, StampCheck::Agreed)
            .unwrap_err(),
        ReleaseRefusal::Expired
    );
    assert!(request.is_open());
    // The sweep closes the request, and a grant is refused for good.
    assert!(azlin_enroll::expire(&mut request, late, StampCheck::Agreed));
    assert_eq!(
        azlin_enroll::mark_released(&mut request, &scene.facts, second, late, StampCheck::Agreed)
            .unwrap_err(),
        ReleaseRefusal::NotOpen
    );
}

#[test]
fn recoveries_by_the_person_depend_on_no_admin_and_no_device() {
    // The person who lost every device recovers with what they know or hold: the recovery code,
    // the contacts' shares, the passkey. Nobody else has to still stand for it.
    for path in [Path::RecoveryCode, Path::ContactShares, Path::PasskeyWrap] {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        scene.complete(&mut request, at(0));
        let grant = grant_of(&request);
        scene.facts.admins.clear();
        scene.facts.active_devices.clear();
        mark_released(&mut request, &scene.facts, grant).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(matches!(request.state(), RequestState::Released { .. }));
    }
}

#[test]
fn the_single_admin_exception_is_for_a_request_opened_under_it() {
    // The person is an admin too (member 5), and the workspace has two admins when the recovery is
    // opened: the normal rule applies, the person and another admin's share. Admin 6 is removed
    // afterwards and the person is the only admin left. That does not turn the request into an
    // opening under the exception of a single-admin workspace, and the kit cannot be given to it.
    let mut scene = Scene::full();
    scene.facts.person = member(5);
    scene.facts.admins = vec![member(5), member(6)];
    scene.facts.admin_count = 2;
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    add_evidence(&mut request, common::sso_of(5, at(0)), at(0)).unwrap();
    let agent_for_five = Contribution::key_agent(
        member(AGENT),
        KeyAgentChecks {
            person: member(5),
            ..all_checks_pass()
        },
    );
    azlin_enroll::contribute(&mut request, &scene.facts, agent_for_five, at(0)).unwrap();
    azlin_enroll::contribute(
        &mut request,
        &scene.facts,
        Contribution::admin(member(6)),
        at(0),
    )
    .unwrap();
    assert_eq!(shortfall(&request), None);
    let grant = grant_of(&request);
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 1;
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Escrow(Shortfall::People { have: 1, need: 2 })
    );
    assert_eq!(
        azlin_enroll::contribute(
            &mut request,
            &scene.facts,
            Contribution::owner_kit(member(5)),
            at(1)
        )
        .unwrap_err(),
        ContributionRefusal::KitOnlyForTheSoleAdmin
    );
}

#[test]
fn an_asker_who_is_no_admin_any_more_is_no_person_when_the_key_goes_out() {
    // A team asks one admin for an escrow opening (the quorum of one): admins 10 and 11 signed,
    // 10 asked. The holder is admin 12. Then 10 is suspended: 11 still signed, so the entry is
    // still a quorum of one, but the one who asked is no active admin any more, and the opening
    // has the person of one holder only.
    let mut scene = Scene::full();
    scene.policy.admin_quorum = azlin_policy::AdminQuorum::ONE;
    let mut request = scene.open(Path::EscrowOpen, at(0));
    assert_eq!(request.asker(), member(10));
    azlin_enroll::contribute(&mut request, &scene.facts, agent(), at(0)).unwrap();
    azlin_enroll::contribute(
        &mut request,
        &scene.facts,
        Contribution::admin(member(12)),
        at(0),
    )
    .unwrap();
    assert_eq!(shortfall(&request), None);
    let grant = grant_of(&request);
    scene.facts.admins.retain(|a| *a != member(10));
    assert_eq!(
        shortfall_for(&request, &scene.facts),
        Some(Shortfall::People { have: 1, need: 2 })
    );
    assert_eq!(
        lost(mark_released(&mut request, &scene.facts, grant)),
        StandingLost::Escrow(Shortfall::People { have: 1, need: 2 })
    );
    // With the asker back it goes out.
    scene.facts.admins.push(member(10));
    let grant = grant_of(&request);
    mark_released(&mut request, &scene.facts, grant).expect("the asker stands again");
}
