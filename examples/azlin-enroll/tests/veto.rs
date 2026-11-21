//! Check 2 of the brief: the veto. "An existing device sees every request and can veto it"
//! (planning/architecture.md section 12 E2): any admin or any ACTIVE device of the person can
//! cancel (section 7.5); helpdesk, a pending device and a suspended admin cannot.
//!
//! Every test completes ALL the evidence first and judges after the hold, so that a cancel is the
//! only thing that can keep the request from releasing.

mod common;

use azlin_enroll::{
    cancel, release, CancelRefusal, Canceller, Contribution, ContributionRefusal, Evidence,
    EvidenceRefusal, OpenRefusal, Path, Reason, Release, ReleaseRefusal, Request, RequestState,
    Served, StampCheck,
};
use common::{
    add_evidence, all_checks_pass, at, contribute, device, hold_secs, mark_released, member, open,
    Scene, AGENT, HELPDESK, HOLD_PATHS, NEW_DEVICE,
};

/// A request opened at 0 with all its evidence, and the scene it lives in.
fn ready(path: Path) -> (Scene, Request) {
    let mut scene = Scene::full();
    let mut request = scene.open(path, at(0));
    scene.complete(&mut request, at(0));
    (scene, request)
}

/// Whether the request releases after its hold, with an agreed stamp.
fn releases(request: &Request) -> bool {
    let after = hold_secs(request.path()) + 1;
    release(request, at(after), StampCheck::Agreed).is_granted()
}

/// The answer after the hold must be `Withheld(Cancelled)`.
fn assert_cancelled(request: &Request) {
    let after = hold_secs(request.path()) + 1;
    match release(request, at(after), StampCheck::Agreed) {
        Release::Withheld(Reason::Cancelled) => {}
        other => panic!("{}: expected Cancelled, got {other:?}", request.path()),
    }
}

#[test]
fn any_admin_can_cancel() {
    for path in HOLD_PATHS {
        for admin in [10u8, 11, 12] {
            let (scene, mut request) = ready(path);
            assert!(releases(&request), "{path}: it would release");
            let notices = cancel(
                &mut request,
                &scene.facts,
                Canceller::Admin(member(admin)),
                at(10),
            )
            .unwrap_or_else(|e| panic!("{path}: admin {admin} must be able to cancel: {e}"));
            assert_eq!(notices.len(), 1);
            assert_eq!(
                request.state(),
                RequestState::Cancelled {
                    by: Canceller::Admin(member(admin)),
                    at: at(10)
                }
            );
            assert_cancelled(&request);
        }
    }
    // The own-device path has no hold, but a request that is still open can be cancelled too.
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(0));
    cancel(
        &mut request,
        &scene.facts,
        Canceller::Admin(member(10)),
        at(1),
    )
    .expect("an admin cancels an own-device request that waits for its phrase");
    assert!(!request.is_open());
}

#[test]
fn any_active_device_of_the_person_can_cancel() {
    for path in HOLD_PATHS {
        for active in [1u8, 2] {
            let (scene, mut request) = ready(path);
            cancel(
                &mut request,
                &scene.facts,
                Canceller::PersonDevice(device(active)),
                at(10),
            )
            .unwrap_or_else(|e| panic!("{path}: active device {active} must cancel: {e}"));
            assert_eq!(
                request.state(),
                RequestState::Cancelled {
                    by: Canceller::PersonDevice(device(active)),
                    at: at(10)
                }
            );
            assert_cancelled(&request);
        }
    }
}

#[test]
fn a_device_of_another_person_cannot_cancel() {
    // "Any of the PERSON's devices": an active device of somebody else is no veto.
    let (scene, mut request) = ready(Path::RecoveryCode);
    let result = cancel(
        &mut request,
        &scene.facts,
        Canceller::PersonDevice(device(77)),
        at(10),
    );
    assert_eq!(
        result.unwrap_err(),
        CancelRefusal::NotAnActiveDevice { device: device(77) }
    );
    assert!(releases(&request));
    // And the facts of another person are refused whole.
    let mut other = scene.facts.clone();
    other.person = member(2);
    other.active_devices.push(device(77));
    let result = cancel(
        &mut request,
        &other,
        Canceller::PersonDevice(device(77)),
        at(10),
    );
    assert!(matches!(result, Err(CancelRefusal::Facts(_))), "{result:?}");
    assert!(releases(&request));
}

#[test]
fn helpdesk_cannot_cancel_or_approve_alone() {
    // It cannot cancel, on any path.
    for path in HOLD_PATHS {
        let (scene, mut request) = ready(path);
        let result = cancel(
            &mut request,
            &scene.facts,
            Canceller::Helpdesk(member(HELPDESK)),
            at(10),
        );
        assert_eq!(
            result.unwrap_err(),
            CancelRefusal::HelpdeskCannotCancel,
            "{path}"
        );
        assert!(request.is_open(), "{path}: the request is as it was");
        assert!(releases(&request), "{path}: nothing was cancelled");
    }
    // It may open a recovery request and nothing else.
    for path in Path::ALL {
        let mut scene = Scene::full();
        let opener = azlin_enroll::Opener::helpdesk(member(HELPDESK), device(NEW_DEVICE));
        let result = open(
            &scene.facts,
            &scene.policy,
            path,
            opener,
            at(0),
            &mut scene.limits,
        );
        if path == Path::EscrowRecovery {
            assert!(result.is_ok(), "helpdesk opens a recovery request");
        } else {
            assert!(
                matches!(result, Err(OpenRefusal::OpenerNotAllowed { .. })),
                "{path}: helpdesk may not open it: {result:?}"
            );
        }
    }
    // It cannot approve alone: whatever it does, the SSO attestation and the escrow are missing.
    let mut scene = Scene::full();
    let opener = azlin_enroll::Opener::helpdesk(member(HELPDESK), device(NEW_DEVICE));
    let (mut request, _) = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        opener,
        at(0),
        &mut scene.limits,
    )
    .expect("helpdesk opens a recovery request");
    assert_eq!(request.asker(), member(common::PERSON), "the person asks");
    // The phrase check "with the person out of band" is no evidence a release needs.
    let phrase = Evidence::PhraseConfirmed(azlin_enroll::Confirmer::Member(member(HELPDESK)));
    assert!(matches!(
        add_evidence(&mut request, phrase, at(1)),
        Err(EvidenceRefusal::NotForThisPath { .. })
    ));
    // It cannot stand in for the person's SSO attestation or for an admin holder.
    let sso_of_helpdesk = Evidence::Sso {
        subject: member(HELPDESK),
        device: device(NEW_DEVICE),
        issued_at: at(1),
    };
    assert_eq!(
        add_evidence(&mut request, sso_of_helpdesk, at(1)).unwrap_err(),
        EvidenceRefusal::SsoSubjectMismatch
    );
    assert_eq!(
        contribute(&mut request, Contribution::admin(member(HELPDESK)), at(1)).unwrap_err(),
        ContributionRefusal::OpenerCannotContribute
    );
    contribute(
        &mut request,
        Contribution::key_agent(member(AGENT), all_checks_pass()),
        at(1),
    )
    .expect("the key agent contributes");
    match release(&request, at(48 * common::HOUR), StampCheck::Agreed) {
        Release::Withheld(Reason::EvidenceMissing(_)) => {}
        other => panic!("helpdesk alone must not release: {other:?}"),
    }
}

#[test]
fn a_pending_device_cannot_cancel_the_request_it_belongs_to_or_anyone_elses() {
    for path in HOLD_PATHS {
        let (scene, mut request) = ready(path);
        // The device the request is for, as a pending device.
        let own = cancel(
            &mut request,
            &scene.facts,
            Canceller::PendingDevice(device(NEW_DEVICE)),
            at(10),
        );
        assert_eq!(
            own.unwrap_err(),
            CancelRefusal::PendingDeviceCannotCancel,
            "{path}"
        );
        // Another pending device (of anybody).
        let other = cancel(
            &mut request,
            &scene.facts,
            Canceller::PendingDevice(device(101)),
            at(10),
        );
        assert_eq!(
            other.unwrap_err(),
            CancelRefusal::PendingDeviceCannotCancel,
            "{path}"
        );
        // The same devices claiming to be active devices of the person: they are not in the list.
        for pending in [device(NEW_DEVICE), device(101)] {
            let claimed = cancel(
                &mut request,
                &scene.facts,
                Canceller::PersonDevice(pending),
                at(10),
            );
            assert_eq!(
                claimed.unwrap_err(),
                CancelRefusal::NotAnActiveDevice { device: pending },
                "{path}"
            );
        }
        assert!(request.is_open(), "{path}: nothing cancelled it");
        assert!(releases(&request), "{path}: the request still releases");
    }
}

#[test]
fn a_suspended_admin_cannot_cancel() {
    for path in HOLD_PATHS {
        let (mut scene, mut request) = ready(path);
        // Admin 12 is suspended: still counted as an admin of the account, no longer active.
        scene.facts.admins.retain(|admin| *admin != member(12));
        assert_eq!(scene.facts.admin_count, 3);
        let result = cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(12)),
            at(10),
        );
        assert_eq!(
            result.unwrap_err(),
            CancelRefusal::NotAnAdmin { member: member(12) },
            "{path}"
        );
        // An ordinary member who calls himself an admin is no admin either.
        let result = cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(30)),
            at(10),
        );
        assert_eq!(
            result.unwrap_err(),
            CancelRefusal::NotAnAdmin { member: member(30) },
            "{path}"
        );
        assert!(releases(&request), "{path}: still not cancelled");
        // The admins who are active still can.
        cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(10),
        )
        .expect("an active admin cancels");
        assert_cancelled(&request);
    }
}

#[test]
fn a_cancelled_request_never_releases() {
    for path in HOLD_PATHS {
        let (scene, mut request) = ready(path);
        // A grant that was made before the cancel (the cancel came second in the log).
        let grant = match release(&request, at(hold_secs(path)), StampCheck::Agreed) {
            Release::Granted(grant) => grant,
            other => panic!("{path}: it would release: {other:?}"),
        };
        cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(5),
        )
        .unwrap();
        // It never releases: not after the hold, not a year later, not with any evidence added.
        for offset in [hold_secs(path), hold_secs(path) + 1, 3 * common::DAY] {
            match release(&request, at(offset), StampCheck::Agreed) {
                Release::Withheld(Reason::Cancelled) => {}
                other => panic!("{path} at +{offset}: {other:?}"),
            }
        }
        // The old grant buys nothing: the cancel that came first wins and no wrap goes out.
        let late: Result<Served, ReleaseRefusal> = mark_released(&mut request, &scene.facts, grant);
        assert_eq!(late.unwrap_err(), ReleaseRefusal::NotOpen, "{path}");
        assert!(matches!(request.state(), RequestState::Cancelled { .. }));
        // Nothing can be added to it, and it cannot be cancelled twice.
        assert_eq!(
            contribute(&mut request, Contribution::admin(member(12)), at(6)).unwrap_err(),
            ContributionRefusal::NotOpen,
            "{path}: no contribution to a cancelled request"
        );
        assert_eq!(
            add_evidence(&mut request, common::proof(), at(6)).unwrap_err(),
            EvidenceRefusal::NotOpen,
            "{path}: no evidence for a cancelled request"
        );
        assert_eq!(
            cancel(
                &mut request,
                &scene.facts,
                Canceller::Admin(member(11)),
                at(7)
            )
            .unwrap_err(),
            CancelRefusal::AlreadyCancelled
        );
    }
}

#[test]
fn cancel_after_release_is_refused() {
    for path in HOLD_PATHS {
        let (scene, mut request) = ready(path);
        let grant = match release(&request, at(hold_secs(path)), StampCheck::Agreed) {
            Release::Granted(grant) => grant,
            other => panic!("{path}: {other:?}"),
        };
        let served = mark_released(&mut request, &scene.facts, grant).expect("the group serves it");
        assert_eq!(served.notices().len(), 1);
        assert_eq!(served.new_device(), device(NEW_DEVICE), "{path}");
        assert_eq!(served.path(), path);
        assert!(matches!(request.state(), RequestState::Released { .. }));
        for by in [
            Canceller::Admin(member(10)),
            Canceller::PersonDevice(device(1)),
        ] {
            assert_eq!(
                cancel(&mut request, &scene.facts, by, at(hold_secs(path) + 1)).unwrap_err(),
                CancelRefusal::AlreadyReleased,
                "{path}"
            );
        }
        // The state is as it was. A second question is answered with the record of the release,
        // never with a second grant: a new device that lost the answer is served from the group's
        // own record of the device, and a device that was removed since is never served again.
        assert!(matches!(request.state(), RequestState::Released { .. }));
        for stamp in [StampCheck::Agreed, StampCheck::Disputed] {
            match release(&request, at(hold_secs(path) + 2), stamp) {
                Release::AlreadyReleased { at: released_at } => {
                    assert_eq!(released_at, at(hold_secs(path)), "{path}")
                }
                other => panic!("{path}: {other:?}"),
            }
        }
    }
}

#[test]
fn a_grant_of_another_request_is_refused() {
    let (scene, mut first) = ready(Path::RecoveryCode);
    let mut other_scene = Scene::full();
    let mut second = other_scene.open(Path::RecoveryCode, at(7));
    other_scene.complete(&mut second, at(7));
    let grant = match release(&second, at(7 + 48 * common::HOUR), StampCheck::Agreed) {
        Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        mark_released(&mut first, &scene.facts, grant).unwrap_err(),
        ReleaseRefusal::WrongRequest
    );
    assert!(first.is_open());
}
