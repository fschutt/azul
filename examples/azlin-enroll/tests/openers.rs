//! Who may open a request and what `open` checks before it makes one (planning/architecture.md
//! section 7.5, section 7.6, section 5.10 `admin_quorum`, section 12 E1): the opener of each path,
//! the signers of an opening entry (active admins, people, the quorum of distinct people, never
//! the person themself), the facts that hold together, and the ledger that a refusal leaves alone.

mod common;

use azlin_enroll::{
    cancel, CancelRefusal, Canceller, Contribution, ContributionRefusal, FactsRefusal, OpenRefusal,
    OpenedBy, Opener, OpenerKind, Path, PathRule, ReleaseRefusal, MAX_SIGNERS, PATHS,
};
use azlin_policy::{AccountKind, Policy, QuorumShort, Signatory};
use common::{at, device, mark_released, member, open, Scene, HELPDESK, NEW_DEVICE, PERSON};

fn person(n: u8) -> Signatory {
    Signatory::person(member(n))
}

/// The opener kinds the table lists for a path.
fn openers_of(path: Path) -> &'static [OpenerKind] {
    path.rule().openers
}

#[test]
fn each_path_lists_who_may_open_it() {
    assert_eq!(openers_of(Path::OwnDevice), &[OpenerKind::PersonDevice]);
    assert_eq!(openers_of(Path::QuorumEnrollment), &[OpenerKind::Admins]);
    assert_eq!(openers_of(Path::RecoveryCode), &[OpenerKind::Person]);
    assert_eq!(openers_of(Path::ContactShares), &[OpenerKind::Person]);
    assert_eq!(openers_of(Path::PasskeyWrap), &[OpenerKind::Person]);
    assert_eq!(
        openers_of(Path::EscrowRecovery),
        &[OpenerKind::Person, OpenerKind::Helpdesk]
    );
    assert_eq!(openers_of(Path::EscrowOpen), &[OpenerKind::Admins]);
    // Every opener is tried on every path: it opens exactly where the table lists it.
    for (path, rule) in PATHS {
        let rule: PathRule = rule;
        for kind in OpenerKind::ALL {
            let opener = match kind {
                OpenerKind::PersonDevice => Opener::person_device(device(1), device(NEW_DEVICE)),
                OpenerKind::Person => Opener::person(device(NEW_DEVICE)),
                OpenerKind::Admins => {
                    Opener::admins(vec![person(11), person(12)], device(NEW_DEVICE))
                }
                OpenerKind::Helpdesk => Opener::helpdesk(member(HELPDESK), device(NEW_DEVICE)),
            };
            assert_eq!(opener.by.kind(), kind);
            let mut scene = Scene::full();
            let result = open(
                &scene.facts,
                &scene.policy,
                path,
                opener,
                at(0),
                &mut scene.limits,
            );
            if rule.openers.contains(&kind) {
                assert!(result.is_ok(), "{path}: {kind:?} may open it: {result:?}");
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    OpenRefusal::OpenerNotAllowed { path, opener: kind },
                    "{path}: {kind:?}"
                );
            }
        }
    }
}

#[test]
fn the_approving_device_must_be_an_active_device_of_the_person() {
    // A pending, revoked or foreign device cannot approve a new device.
    for approver in [device(NEW_DEVICE), device(3), device(77)] {
        let mut scene = Scene::full();
        let result = open(
            &scene.facts,
            &scene.policy,
            Path::OwnDevice,
            Opener::person_device(approver, device(NEW_DEVICE + 1)),
            at(0),
            &mut scene.limits,
        );
        assert_eq!(
            result.unwrap_err(),
            OpenRefusal::NotAnActiveDevice,
            "{approver:?}"
        );
    }
    // An active device approving itself: it is already enrolled.
    let mut scene = Scene::full();
    let result = open(
        &scene.facts,
        &scene.policy,
        Path::OwnDevice,
        Opener::person_device(device(1), device(1)),
        at(0),
        &mut scene.limits,
    );
    assert_eq!(result.unwrap_err(), OpenRefusal::DeviceAlreadyActive);
}

#[test]
fn signers_must_be_active_admins_who_are_people() {
    for path in [Path::QuorumEnrollment, Path::EscrowOpen] {
        let cases: Vec<(&str, Vec<Signatory>)> = vec![
            ("an ordinary member", vec![person(30), person(11)]),
            (
                "an app principal",
                vec![Signatory::app(member(10)), person(11)],
            ),
            ("a suspended admin", vec![person(10), person(13)]),
        ];
        for (what, signers) in cases {
            let mut scene = Scene::full();
            let result = open(
                &scene.facts,
                &scene.policy,
                path,
                Opener::admins(signers, device(NEW_DEVICE)),
                at(0),
                &mut scene.limits,
            );
            assert!(
                matches!(result, Err(OpenRefusal::NotAnAdmin { .. })),
                "{path}: {what}: {result:?}"
            );
        }
        // No signer at all, and too many.
        let mut scene = Scene::full();
        let result = open(
            &scene.facts,
            &scene.policy,
            path,
            Opener::admins(Vec::new(), device(NEW_DEVICE)),
            at(0),
            &mut scene.limits,
        );
        assert_eq!(result.unwrap_err(), OpenRefusal::NoSigners);
        let many: Vec<Signatory> = (0..=MAX_SIGNERS as u8).map(|n| person(40 + n)).collect();
        let result = open(
            &scene.facts,
            &scene.policy,
            path,
            Opener::admins(many, device(NEW_DEVICE)),
            at(0),
            &mut scene.limits,
        );
        assert_eq!(result.unwrap_err(), OpenRefusal::TooManySigners);
    }
}

#[test]
fn the_person_cannot_sign_the_opening_of_their_own_request() {
    // A person who is an admin themself does not approve their own new device, and no admin opens
    // their own data through escrow: they recover.
    for path in [Path::QuorumEnrollment, Path::EscrowOpen] {
        let mut scene = Scene::full();
        scene.facts.admins.push(member(PERSON));
        scene.facts.admin_count = 4;
        let result = open(
            &scene.facts,
            &scene.policy,
            path,
            Opener::admins(vec![person(10), person(PERSON)], device(NEW_DEVICE)),
            at(0),
            &mut scene.limits,
        );
        assert_eq!(result.unwrap_err(), OpenRefusal::SubjectIsASigner, "{path}");
    }
}

#[test]
fn the_quorum_counts_distinct_people_never_devices() {
    // A business asks two different people for a quorum enrollment and for an escrow opening.
    for path in [Path::QuorumEnrollment, Path::EscrowOpen] {
        let mut scene = Scene::full();
        // One admin, however many of their devices signed.
        for signers in [vec![person(11)], vec![person(11), person(11), person(11)]] {
            let result = open(
                &scene.facts,
                &scene.policy,
                path,
                Opener::admins(signers, device(NEW_DEVICE)),
                at(0),
                &mut scene.limits,
            );
            assert_eq!(
                result.unwrap_err(),
                OpenRefusal::QuorumShort(QuorumShort { have: 1, need: 2 }),
                "{path}"
            );
        }
        // Two different admins.
        let opened = open(
            &scene.facts,
            &scene.policy,
            path,
            Opener::admins(vec![person(11), person(12)], device(NEW_DEVICE)),
            at(0),
            &mut scene.limits,
        );
        let (request, _) = opened.unwrap();
        assert_eq!(request.asker(), member(11), "the first signer asks");
        assert_eq!(
            request.opened_by(),
            &OpenedBy::Admins(vec![person(11), person(12)])
        );
    }
    // A team asks one (the signer alone), also an escrow opening: the second person is the holder.
    let mut scene = Scene::full();
    scene.policy = Policy::defaults(AccountKind::Team);
    scene.policy.passkey_recovery = true;
    let opened = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        Opener::admins(vec![person(11)], device(NEW_DEVICE)),
        at(0),
        &mut scene.limits,
    );
    assert!(opened.is_ok(), "{opened:?}");
}

#[test]
fn facts_that_cannot_be_true_are_refused() {
    let mut scene = Scene::full();
    scene.facts.admin_count = 2; // below the three admins listed
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::Facts(FactsRefusal::AdminCountBelowList)
    );
    let mut scene = Scene::full();
    scene.facts.admins.push(member(10));
    scene.facts.admin_count = 4;
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::Facts(FactsRefusal::Duplicate)
    );
    let mut scene = Scene::full();
    scene.facts.active_devices = vec![device(1), device(2), device(1)];
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::Facts(FactsRefusal::Duplicate)
    );
    let mut scene = Scene::full();
    scene.facts.admins = (0..=255u8).map(member).take(256).collect();
    scene.facts.admin_count = 255;
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::Facts(FactsRefusal::TooLong)
    );
    // A refusal counted nothing.
    assert_eq!(scene.limits.tracked_ledgers(), 0);
}

#[test]
fn a_hold_that_does_not_fit_the_stamp_is_refused() {
    let mut scene = Scene::full();
    // The largest stamp, and a hold of 48 hours: the instant does not exist.
    let opener = scene.opener(Path::RecoveryCode);
    let result = open(
        &scene.facts,
        &scene.policy,
        Path::RecoveryCode,
        opener,
        azlin_seams::Stamp::from_secs(u64::MAX - 100),
        &mut scene.limits,
    );
    assert_eq!(result.unwrap_err(), OpenRefusal::TimeOverflow);
    // A stamp so late that only the expiry does not fit is refused as well, and no opening was
    // counted.
    let opener = scene.opener(Path::OwnDevice);
    let result = open(
        &scene.facts,
        &scene.policy,
        Path::OwnDevice,
        opener,
        azlin_seams::Stamp::from_secs(u64::MAX - 100),
        &mut scene.limits,
    );
    assert_eq!(result.unwrap_err(), OpenRefusal::TimeOverflow);
    assert_eq!(scene.limits.tracked_ledgers(), 0);
}

#[test]
fn request_ids_are_deterministic_and_tell_requests_apart() {
    // The group is a replicated state machine: every replica computes the same id from the same
    // facts, with no entropy.
    let mut a = Scene::full();
    let mut b = Scene::full();
    let first = a.open(Path::RecoveryCode, at(7));
    let same = b.open(Path::RecoveryCode, at(7));
    assert_eq!(first.id(), same.id());
    assert_eq!(first, same);
    assert_eq!(first.id().to_string().len(), 64);
    // Another stamp, path, device or person: another id.
    let mut ids = vec![first.id()];
    ids.push(a.open(Path::RecoveryCode, at(8)).id());
    ids.push(a.open(Path::ContactShares, at(7)).id());
    let mut scene = Scene::full();
    let opener = Opener::person(device(NEW_DEVICE + 1));
    let (other_device, _) = open(
        &scene.facts,
        &scene.policy,
        Path::RecoveryCode,
        opener,
        at(7),
        &mut scene.limits,
    )
    .unwrap();
    ids.push(other_device.id());
    let mut scene = Scene::full();
    scene.facts.person = member(2);
    ids.push(scene.open(Path::RecoveryCode, at(7)).id());
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 5, "five different requests, five different ids");
}

#[test]
fn two_kinds_of_opener_make_two_requests() {
    // The same person, device, path and second: the person's own request and a helpdesk member's
    // for them are two requests, and so are their ids.
    let mut scene = Scene::full();
    let (by_person, _) = scene.try_open(Path::EscrowRecovery, at(7)).unwrap();
    let (by_helpdesk, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        Opener::helpdesk(member(HELPDESK), device(NEW_DEVICE)),
        at(7),
        &mut scene.limits,
    )
    .unwrap();
    assert_eq!(by_person.asker(), by_helpdesk.asker());
    assert_ne!(by_person.id(), by_helpdesk.id());
}

#[test]
fn a_sole_admin_asks_alone_for_an_escrow_opening_and_for_nothing_else() {
    // Section 7.5, "Single-admin workspaces": the owner plus the offline recovery kit, with the
    // waiting period. A quorum of two people cannot exist there, so an opening needs the one
    // admin; no other path is relaxed.
    let alone = || Opener::admins(vec![person(10)], device(NEW_DEVICE));
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(10)];
    scene.facts.admin_count = 1;
    let opened = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        alone(),
        at(0),
        &mut scene.limits,
    );
    assert!(opened.is_ok(), "{opened:?}");
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(10)];
    scene.facts.admin_count = 1;
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::QuorumEnrollment,
        alone(),
        at(0),
        &mut scene.limits,
    );
    assert_eq!(
        refused.unwrap_err(),
        OpenRefusal::QuorumShort(QuorumShort { have: 1, need: 2 })
    );
    // A second admin who is only suspended (not listed, but counted) does not make the workspace
    // a one-admin workspace: the count is the account's.
    let mut scene = Scene::full();
    scene.facts.admins = vec![member(10)];
    scene.facts.admin_count = 2;
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowOpen,
        alone(),
        at(0),
        &mut scene.limits,
    );
    assert_eq!(
        refused.unwrap_err(),
        OpenRefusal::QuorumShort(QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn the_functions_that_take_facts_refuse_the_facts_of_another_person() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowOpen, at(0));
    let mut other = scene.facts.clone();
    other.person = member(2);
    let another = FactsRefusal::AnotherPerson;
    assert_eq!(
        cancel(&mut request, &other, Canceller::Admin(member(10)), at(1)).unwrap_err(),
        CancelRefusal::Facts(another)
    );
    assert_eq!(
        azlin_enroll::contribute(&mut request, &other, Contribution::admin(member(12)), at(1))
            .unwrap_err(),
        ContributionRefusal::Facts(another)
    );
    assert!(request.is_open());
    assert!(request.contributions().is_empty());
    // And a grant is not taken against the facts of somebody else.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    let grant = match azlin_enroll::release(
        &request,
        at(common::hold_secs(Path::RecoveryCode)),
        azlin_enroll::StampCheck::Agreed,
    ) {
        azlin_enroll::Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        mark_released(&mut request, &other, grant).unwrap_err(),
        ReleaseRefusal::Facts(another)
    );
    assert!(request.is_open());
}

#[test]
fn a_veto_is_not_refused_for_a_mistake_in_the_lists_of_the_facts() {
    // `cancel` asks of the facts that they are the person's and nothing more: a list with a repeat
    // and a count below the list would refuse an `open`, never a veto.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.facts.admins.push(member(10));
    scene.facts.admin_count = 1;
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(1)).unwrap_err(),
        OpenRefusal::Facts(FactsRefusal::AdminCountBelowList)
    );
    cancel(
        &mut request,
        &scene.facts,
        Canceller::Admin(member(10)),
        at(1),
    )
    .expect("the veto of an admin stands");
    assert!(!request.is_open());
}
