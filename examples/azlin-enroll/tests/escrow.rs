//! Check 4 of the brief: escrow. "Every opening needs two different people: the one who asks and
//! an admin holder who contributes, never the same person. The key agent's share never stands in
//! for a person" (planning/architecture.md section 7.6); the single-admin workspace of section 7.5
//! ("the owner plus the offline recovery kit, with the waiting period"); private members have no
//! escrow path (section 7.6).

mod common;

use azlin_enroll::{
    release, shortfall, Contribution, ContributionKind, ContributionRefusal, EvidenceKind,
    KeyAgentCheck, KeyAgentChecks, OpenRefusal, Opener, Path, Reason, Release, Request, Shortfall,
    StampCheck, ESCROW_MIN_SHARES, ESCROW_PEOPLE, MAX_CONTRIBUTIONS,
};
use azlin_policy::{distinct_people, Signatory};
use common::{
    add_evidence, all_checks_pass, at, contribute, device, hold_secs, member, nd, open, Scene,
    AGENT, HELPDESK, HOUR, NEW_DEVICE, PERSON,
};

const AGENT_2: u8 = 201;

fn agent(n: u8) -> Contribution {
    Contribution::key_agent(member(n), all_checks_pass())
}

fn admin(n: u8) -> Contribution {
    Contribution::admin(member(n))
}

/// `contribute` with the facts of this scene (the shim of the common module uses the standard ones).
fn give(
    scene: &Scene,
    request: &mut Request,
    c: Contribution,
    now: azlin_seams::Stamp,
) -> Result<(), ContributionRefusal> {
    azlin_enroll::contribute(request, &scene.facts, c, now)
}

/// An escrow request with the contributions given and, for a recovery, the SSO attestation.
fn escrow_request(path: Path, contributions: &[Contribution]) -> (Scene, Request) {
    let mut scene = Scene::full();
    let mut request = scene.open(path, at(0));
    if path == Path::EscrowRecovery {
        add_evidence(&mut request, common::sso_of(PERSON, at(0)), at(0))
            .expect("the attestation of the person");
    }
    for c in contributions {
        contribute(&mut request, *c, at(0)).unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }
    (scene, request)
}

/// Whether the request releases when its hold ends.
fn opens(request: &Request) -> bool {
    release(request, at(hold_secs(request.path())), StampCheck::Agreed).is_granted()
}

#[test]
fn opening_needs_two_different_people() {
    assert_eq!(ESCROW_PEOPLE, 2);
    assert_eq!(ESCROW_MIN_SHARES, 2);
    // An escrow_open: the one who asks (admin 10) and nobody else.
    let (_, request) = escrow_request(Path::EscrowOpen, &[]);
    assert_eq!(request.asker(), member(10));
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::People { have: 1, need: 2 })
    );
    assert!(!opens(&request));
    // One admin holder: two different people, but one share.
    let (_, request) = escrow_request(Path::EscrowOpen, &[admin(11)]);
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::Shares { have: 1, need: 2 })
    );
    assert!(!opens(&request));
    // The same holder twice is one share.
    let (_, request) = escrow_request(Path::EscrowOpen, &[admin(11), admin(11)]);
    assert_eq!(request.contributions().len(), 1);
    assert!(!opens(&request));
    // Two admin holders besides the one who asks: opens.
    let (_, request) = escrow_request(Path::EscrowOpen, &[admin(11), admin(12)]);
    assert_eq!(shortfall(&request), None);
    assert!(opens(&request));
    // The same in a recovery: the person asks, two admin holders open.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[admin(11)]);
    assert_eq!(request.asker(), member(PERSON));
    assert!(!opens(&request));
    let (_, request) = escrow_request(Path::EscrowRecovery, &[admin(11), admin(12)]);
    assert!(opens(&request));
    // The release names the missing threshold.
    let (_, request) = escrow_request(Path::EscrowOpen, &[admin(11)]);
    match release(&request, at(72 * HOUR), StampCheck::Agreed) {
        Release::Withheld(Reason::EvidenceMissing(EvidenceKind::EscrowThreshold)) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn asker_cannot_contribute_to_own_opening() {
    // An escrow_open: admin 10 asked and cannot contribute; the cosigner 11 and admin 12 can.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    assert_eq!(
        contribute(&mut request, admin(10), at(1)).unwrap_err(),
        ContributionRefusal::AskerCannotContribute
    );
    assert!(
        request.contributions().is_empty(),
        "a refusal changes nothing"
    );
    // A person who is no admin holds no share of anybody's opening.
    assert_eq!(
        contribute(&mut request, admin(PERSON), at(1)).unwrap_err(),
        ContributionRefusal::NotAnAdmin {
            member: member(PERSON)
        }
    );
    // The person whose data is opened is no holder of its opening either, even when the person is
    // an admin of the account themself.
    scene.facts.admins.push(member(PERSON));
    scene.facts.admin_count = 4;
    assert_eq!(
        give(&scene, &mut request, admin(PERSON), at(1)).unwrap_err(),
        ContributionRefusal::SubjectCannotContribute
    );
    contribute(&mut request, admin(11), at(1)).expect("the cosigner is another person");
    contribute(&mut request, admin(12), at(1)).expect("another admin holder");
    // In a recovery the person asks: the person's own share does not count.
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    assert_eq!(
        give(&scene, &mut request, admin(PERSON), at(1)).unwrap_err(),
        ContributionRefusal::AskerCannotContribute
    );
    // Even with everything else in order, the asker's share cannot be what completes it.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[agent(AGENT)]);
    assert!(!opens(&request));
}

#[test]
fn key_agent_share_never_stands_in_for_a_person() {
    // The agent is an app principal: the policy's count of people gives it none.
    assert_eq!(distinct_people(&[Signatory::app(member(AGENT))]), 0);
    assert_eq!(
        distinct_people(&[Signatory::person(member(10)), Signatory::app(member(AGENT))]),
        1
    );
    // Two agent shares make two shares and no second person: it does not open.
    let (_, request) = escrow_request(Path::EscrowOpen, &[agent(AGENT), agent(AGENT_2)]);
    assert_eq!(request.contributions().len(), 2);
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::People { have: 1, need: 2 }),
        "the agents are no people"
    );
    assert!(!opens(&request));
    let (_, request) = escrow_request(Path::EscrowRecovery, &[agent(AGENT), agent(AGENT_2)]);
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::People { have: 1, need: 2 })
    );
    assert!(!opens(&request));
    // One admin holder beside them is the second person.
    let (_, request) = escrow_request(Path::EscrowOpen, &[agent(AGENT), agent(AGENT_2), admin(11)]);
    assert_eq!(shortfall(&request), None);
    assert!(opens(&request));
}

#[test]
fn key_agent_alone_never_opens() {
    for path in [Path::EscrowRecovery, Path::EscrowOpen] {
        let (_, request) = escrow_request(path, &[agent(AGENT)]);
        assert_eq!(
            shortfall(&request),
            Some(Shortfall::People { have: 1, need: 2 }),
            "{path}"
        );
        assert!(!opens(&request), "{path}");
        // Not after any time either.
        for offset in [hold_secs(path) + 1, 3 * common::DAY, 6 * common::DAY] {
            assert!(
                !release(&request, at(offset), StampCheck::Agreed).is_granted(),
                "{path} at +{offset}"
            );
        }
    }
}

#[test]
fn lost_laptop_needs_the_agent_plus_one_admin_holder_other_than_the_asker() {
    // The person lost every device: the SSO attestation is in, the asker is the person.
    // The agent alone: no.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[agent(AGENT)]);
    assert!(!opens(&request));
    // One admin holder alone: two people, one share: no.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[admin(11)]);
    assert!(!opens(&request));
    // The agent and the person's own share (refused: the asker): no.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    add_evidence(&mut request, common::sso_of(PERSON, at(0)), at(0)).unwrap();
    contribute(&mut request, agent(AGENT), at(0)).unwrap();
    assert!(contribute(&mut request, admin(PERSON), at(0)).is_err());
    assert!(!opens(&request));
    // The agent and one admin holder other than the asker, plus the hold: yes.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[agent(AGENT), admin(11)]);
    assert_eq!(shortfall(&request), None);
    assert!(!release(&request, at(48 * HOUR - 1), StampCheck::Agreed).is_granted());
    assert!(opens(&request));
    // Two admin holders and no agent: yes as well.
    let (_, request) = escrow_request(Path::EscrowRecovery, &[admin(11), admin(12)]);
    assert!(opens(&request));
}

#[test]
fn key_agent_contributes_only_when_idp_token_mdm_and_policy_checks_all_pass() {
    let cases = [
        (false, true, true, KeyAgentCheck::IdpToken),
        (true, false, true, KeyAgentCheck::MdmCertificate),
        (true, true, false, KeyAgentCheck::Policy),
        (false, false, true, KeyAgentCheck::IdpToken),
        (false, true, false, KeyAgentCheck::IdpToken),
        (true, false, false, KeyAgentCheck::MdmCertificate),
        (false, false, false, KeyAgentCheck::IdpToken),
    ];
    for (idp_token, mdm_certificate, policy, first) in cases {
        let checks = KeyAgentChecks {
            person: member(PERSON),
            device: nd(),
            idp_token,
            mdm_certificate,
            policy,
        };
        assert_eq!(checks.first_failure(), Some(first));
        let mut scene = Scene::full();
        let mut request = scene.open(Path::EscrowRecovery, at(0));
        assert_eq!(
            contribute(
                &mut request,
                Contribution::key_agent(member(AGENT), checks),
                at(0)
            )
            .unwrap_err(),
            ContributionRefusal::KeyAgentCheckFailed(first),
            "{checks:?}"
        );
        assert!(
            request.contributions().is_empty(),
            "a failed check contributes nothing"
        );
    }
    assert_eq!(all_checks_pass().first_failure(), None);
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    contribute(&mut request, agent(AGENT), at(0)).expect("all three checks passed");
    assert_eq!(request.contributions().len(), 1);
}

/// A workspace with exactly one admin (member 5), who is not the subject (member 1).
fn single_admin_scene() -> Scene {
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 1;
    // Even a subject "offboarded by a quorum" waits: a single admin is never a quorum.
    scene.facts.offboarded_by = 2;
    scene
}

fn sole_admin_opener() -> Opener {
    Opener::admins(vec![Signatory::person(member(5))], device(NEW_DEVICE))
}

#[test]
fn single_admin_workspace_owner_kit_path_has_the_full_wait_and_notices() {
    let mut scene = single_admin_scene();
    let (mut request, notices) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        sole_admin_opener(),
        at(0),
        &mut scene.limits,
    )
    .expect("the owner of a single-admin workspace opens an escrow opening");
    // The full wait, though the subject was "offboarded by a quorum".
    assert_eq!(request.hold_until(), at(72 * HOUR));
    // The notices: the admin and the subject's devices.
    assert_eq!(notices.len(), 1);
    assert_eq!(notices.admins(), vec![member(5)]);
    assert_eq!(notices.devices(), scene.facts.active_devices);
    // Without the offline kit it never opens, whatever else is in.
    assert_eq!(shortfall(&request), Some(Shortfall::Kit));
    give(&scene, &mut request, agent(AGENT), at(1)).unwrap();
    assert_eq!(shortfall(&request), Some(Shortfall::Kit));
    // The owner's own share does not replace the kit.
    give(&scene, &mut request, admin(5), at(1)).expect("the sole admin may hold a share");
    assert_eq!(shortfall(&request), Some(Shortfall::Kit));
    assert!(!opens(&request));
    // The kit completes it: one person, the kit and a share.
    give(
        &scene,
        &mut request,
        Contribution::owner_kit(member(5)),
        at(1),
    )
    .unwrap();
    assert_eq!(shortfall(&request), None);
    assert!(!release(&request, at(72 * HOUR - 1), StampCheck::Agreed).is_granted());
    assert!(release(&request, at(72 * HOUR), StampCheck::Agreed).is_granted());
    // The kit alone is one share: not enough.
    let mut scene = single_admin_scene();
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        sole_admin_opener(),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    give(
        &scene,
        &mut request,
        Contribution::owner_kit(member(5)),
        at(1),
    )
    .unwrap();
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::Shares { have: 1, need: 2 })
    );
    // The kit and the key agent: enough.
    give(&scene, &mut request, agent(AGENT), at(1)).unwrap();
    assert_eq!(shortfall(&request), None);
}

#[test]
fn the_kit_is_only_the_sole_admins_in_a_workspace_with_one_admin() {
    // Another member's kit is refused even in a single-admin workspace.
    let mut scene = single_admin_scene();
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        sole_admin_opener(),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    // (There is no other admin in this workspace: a kit of somebody else is no admin's at all.)
    assert_eq!(
        give(
            &scene,
            &mut request,
            Contribution::owner_kit(member(6)),
            at(1)
        )
        .unwrap_err(),
        ContributionRefusal::NotAnAdmin { member: member(6) }
    );
    // In a workspace with several admins no kit opens anything alone.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    for holder in [10u8, 11, 12] {
        assert_eq!(
            contribute(&mut request, Contribution::owner_kit(member(holder)), at(1)).unwrap_err(),
            ContributionRefusal::KitOnlyForTheSoleAdmin,
            "admin {holder}"
        );
    }
    // A workspace whose second admin is only SUSPENDED still has two admins: no kit, and the
    // quorum of the policy is not relaxed.
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 2;
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        sole_admin_opener(),
        at(0),
        &mut scene.limits,
    );
    assert!(
        matches!(refused, Err(OpenRefusal::QuorumShort(_))),
        "{refused:?}"
    );
    // The relaxation is for the escrow opening only: a quorum enrollment still needs the quorum.
    let mut scene = single_admin_scene();
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::QuorumEnrollment,
        sole_admin_opener(),
        at(0),
        &mut scene.limits,
    );
    assert!(
        matches!(refused, Err(OpenRefusal::QuorumShort(_))),
        "{refused:?}"
    );
    // The sole admin who is the asker of a recovery of their OWN keys needs the kit as well.
    let mut scene = Scene::full();
    scene.facts.person = member(5);
    scene.facts.admins = vec![member(5)];
    scene.facts.admin_count = 1;
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    assert_eq!(request.asker(), member(5));
    add_evidence(&mut request, common::sso_of(5, at(0)), at(0)).unwrap();
    // The key agent's checks were made for this person (member 5).
    let agent_for_five = Contribution::key_agent(
        member(AGENT),
        KeyAgentChecks {
            person: member(5),
            ..all_checks_pass()
        },
    );
    give(&scene, &mut request, agent_for_five, at(0)).unwrap();
    assert_eq!(shortfall(&request), Some(Shortfall::Kit));
    give(
        &scene,
        &mut request,
        Contribution::owner_kit(member(5)),
        at(0),
    )
    .unwrap();
    assert_eq!(shortfall(&request), None);
    assert!(!release(&request, at(48 * HOUR - 1), StampCheck::Agreed).is_granted());
    assert!(release(&request, at(48 * HOUR), StampCheck::Agreed).is_granted());
}

#[test]
fn private_member_has_no_escrow_path() {
    // A private member (never escrowed: 7.6): neither a recovery nor an opening.
    let mut scene = Scene::full();
    scene.facts.managed = false;
    for path in [Path::EscrowRecovery, Path::EscrowOpen] {
        assert_eq!(
            scene.try_open(path, at(0)).unwrap_err(),
            OpenRefusal::NotManaged,
            "{path}"
        );
    }
    // A personal and a family account: no member is managed.
    for mut scene in [Scene::personal(), Scene::family()] {
        assert_eq!(
            scene.try_open(Path::EscrowRecovery, at(0)).unwrap_err(),
            OpenRefusal::NotManaged
        );
        // The other recoveries are there for them.
        assert!(scene.try_open(Path::RecoveryCode, at(0)).is_ok());
    }
    // A managed member of an account without an escrow key.
    let mut scene = Scene::full();
    scene.facts.escrow_configured = false;
    for path in [Path::EscrowRecovery, Path::EscrowOpen] {
        assert_eq!(
            scene.try_open(path, at(0)).unwrap_err(),
            OpenRefusal::EscrowNotConfigured,
            "{path}"
        );
    }
    // Nothing was counted against the limits.
    assert_eq!(scene.limits.opened_today(member(PERSON), at(0)), 0);
}

#[test]
fn contributions_belong_to_escrow_paths_and_are_bounded() {
    // The five other paths take no contribution.
    for path in [
        Path::OwnDevice,
        Path::QuorumEnrollment,
        Path::RecoveryCode,
        Path::ContactShares,
        Path::PasskeyWrap,
    ] {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        assert_eq!(
            contribute(&mut request, admin(11), at(0)).unwrap_err(),
            ContributionRefusal::NotAnEscrowPath(path)
        );
        assert_eq!(shortfall(&request), None, "{path} asks no escrow");
    }
    // At most MAX_CONTRIBUTIONS: an account with many admins (50 to 90) besides 10, 11 and 12.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    scene.facts.admins.extend((50..=90u8).map(member));
    scene.facts.admin_count = 44;
    for holder in 0..MAX_CONTRIBUTIONS {
        let holder = 50 + u8::try_from(holder).unwrap();
        give(&scene, &mut request, admin(holder), at(0)).unwrap();
    }
    assert_eq!(request.contributions().len(), MAX_CONTRIBUTIONS);
    assert_eq!(
        give(&scene, &mut request, admin(90), at(0)).unwrap_err(),
        ContributionRefusal::TooMany
    );
    // A repeated contribution is still accepted when the list is full: it changes nothing.
    give(&scene, &mut request, admin(50), at(0)).unwrap();
    assert_eq!(request.contributions().len(), MAX_CONTRIBUTIONS);
    let kinds: Vec<ContributionKind> = request.contributions().iter().map(|c| c.kind).collect();
    assert!(kinds.iter().all(|k| matches!(k, ContributionKind::Admin)));
}

#[test]
fn the_key_agents_checks_are_for_this_person_and_this_device() {
    // The checks the agent made name the person and the device: checks made for somebody else, or
    // for another device, are no checks of this request.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    for checks in [
        KeyAgentChecks {
            person: member(2),
            ..all_checks_pass()
        },
        KeyAgentChecks {
            device: device(NEW_DEVICE + 1),
            ..all_checks_pass()
        },
    ] {
        assert_eq!(
            give(
                &scene,
                &mut request,
                Contribution::key_agent(member(AGENT), checks),
                at(0)
            )
            .unwrap_err(),
            ContributionRefusal::AgentCheckedAnother,
            "{checks:?}"
        );
    }
    assert!(request.contributions().is_empty());
}

#[test]
fn an_admin_or_the_person_is_no_key_agent() {
    // The key agent is an app principal; an active admin or the subject who presents itself as the
    // agent would be counted as a share and no person, which is not what they are.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    for holder in [10u8, 11, 12, PERSON] {
        assert_eq!(
            give(&scene, &mut request, agent(holder), at(0)).unwrap_err(),
            ContributionRefusal::AgentIsNoApp,
            "{holder}"
        );
    }
    assert!(request.contributions().is_empty());
}

#[test]
fn a_share_of_an_admin_needs_an_active_admin() {
    // An ordinary member, and an admin who is suspended in the facts of the moment, hold no share.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    assert_eq!(
        give(&scene, &mut request, admin(30), at(0)).unwrap_err(),
        ContributionRefusal::NotAnAdmin { member: member(30) }
    );
    scene.facts.admins.retain(|a| *a != member(12));
    assert_eq!(
        give(&scene, &mut request, admin(12), at(0)).unwrap_err(),
        ContributionRefusal::NotAnAdmin { member: member(12) }
    );
    give(&scene, &mut request, admin(11), at(0)).unwrap();
    assert_eq!(request.contributions().len(), 1);
}

#[test]
fn the_helpdesk_member_who_opened_a_recovery_holds_no_share_of_it() {
    // "Helpdesk may only open a recovery request ... and cannot approve alone": even when the glue
    // lists the helpdesk member as an admin of the account, the member who opened the request gives
    // no share of it.
    let mut scene = Scene::full();
    scene.facts.admins.push(member(HELPDESK));
    scene.facts.admin_count = 4;
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        Opener::helpdesk(member(HELPDESK), nd()),
        at(0),
        &mut scene.limits,
    )
    .unwrap();
    assert_eq!(
        request.asker(),
        member(PERSON),
        "the one who asks is the person"
    );
    assert_eq!(
        give(&scene, &mut request, admin(HELPDESK), at(0)).unwrap_err(),
        ContributionRefusal::OpenerCannotContribute
    );
    assert_eq!(
        give(
            &scene,
            &mut request,
            Contribution::owner_kit(member(HELPDESK)),
            at(0)
        )
        .unwrap_err(),
        ContributionRefusal::OpenerCannotContribute
    );
    // Nor the share of the key agent: the helpdesk member is no app principal, and the agent's
    // share is the one share that needs no standing of the team's.
    assert_eq!(
        give(&scene, &mut request, agent(HELPDESK), at(0)).unwrap_err(),
        ContributionRefusal::OpenerCannotContribute
    );
    // Another admin's share is a share.
    give(&scene, &mut request, admin(11), at(0)).unwrap();
    assert_eq!(request.contributions().len(), 1);
}

#[test]
fn a_member_listed_as_a_person_and_as_the_agent_counts_for_nobody() {
    // A member is a person or an app, never both: a registry that lists one id as both is no
    // consistent fact, and the count of the people (the policy's `distinct_people`) gives that
    // member to nobody, so an opening is never over-counted. Here the admin 11 holds a share and
    // is then suspended; the facts of the moment no longer list 11 as an admin, so 11 can be
    // presented as the key agent, and is counted as neither.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    contribute(&mut request, admin(11), at(0)).unwrap();
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::Shares { have: 1, need: 2 }),
        "two people, one share"
    );
    scene.facts.admins.retain(|a| *a != member(11));
    give(&scene, &mut request, agent(11), at(0)).unwrap();
    assert_eq!(request.contributions().len(), 2);
    assert_eq!(
        shortfall(&request),
        Some(Shortfall::People { have: 1, need: 2 }),
        "the asker alone: 11 is a person and an app"
    );
    assert!(!opens(&request));
}
