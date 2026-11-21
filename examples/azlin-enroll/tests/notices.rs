//! Check 6 of the brief: notices and visibility. "Every enrollment emits `log_appended` from the
//! metadata group to every admin and to the person's devices" (planning/architecture.md section
//! 7.5, section 5.9); "an existing device sees every request and can veto it" (section 12 E2), also
//! when a notice is lost.

mod common;

use azlin_enroll::{
    cancel, expire, pending, pending_for_person, release, Canceller, NoticeKind, Notices, Path,
    Release, ReleaseGrant, ReleaseRefusal, Request, StampCheck, WrapKind, PATHS,
};
use common::{at, device, hold_secs, mark_released, member, Scene, DAY, HOLD_PATHS, PERSON};

/// The grant of an open request at `now`.
fn grant_of(request: &Request, now: azlin_seams::Stamp) -> ReleaseGrant {
    match release(request, now, StampCheck::Agreed) {
        Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    }
}

/// The admins and the devices of the facts, as sets for comparison.
fn audience_of(scene: &Scene) -> (Vec<azlin_seams::MemberId>, Vec<azlin_seams::DeviceId>) {
    let mut admins = scene.facts.admins.clone();
    let mut devices = scene.facts.active_devices.clone();
    admins.sort();
    devices.sort();
    (admins, devices)
}

fn sorted(notices: &Notices) -> (Vec<azlin_seams::MemberId>, Vec<azlin_seams::DeviceId>) {
    let mut admins = notices.admins();
    let mut devices = notices.devices();
    admins.sort();
    devices.sort();
    (admins, devices)
}

#[test]
fn open_notices_every_admin_and_every_device_of_the_person() {
    for (path, _) in PATHS {
        let mut scene = Scene::full();
        let (request, notices) = scene.try_open(path, at(0)).unwrap();
        assert_eq!(notices.len(), 1, "{path}: one log_appended");
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.kind, NoticeKind::Opened { path });
        assert_eq!(notice.request, request.id());
        assert_eq!(notice.person, member(PERSON));
        // Every admin and every active device of the person: exactly them.
        assert_eq!(sorted(&notices), audience_of(&scene), "{path}");
        assert_eq!(notice.audience.admins, scene.facts.admins);
        assert_eq!(notice.audience.devices, scene.facts.active_devices);
    }
    // More admins and more devices: all of them.
    let mut scene = Scene::full();
    scene.facts.admins = (10..20).map(member).collect();
    scene.facts.admin_count = 10;
    scene.facts.active_devices = (1..=6).map(device).collect();
    let (_, notices) = scene.try_open(Path::RecoveryCode, at(0)).unwrap();
    assert_eq!(notices.admins().len(), 10);
    assert_eq!(notices.devices().len(), 6);
    assert_eq!(sorted(&notices), audience_of(&scene));
    // An escrow opening of a person with no active device: the admins are told, the person has no
    // device to tell ("notice to the person (if active) and every admin").
    let mut scene = Scene::full();
    scene.facts.active_devices.clear();
    scene.facts.offboarded_by = 2;
    let (_, notices) = scene.try_open(Path::EscrowOpen, at(0)).unwrap();
    assert_eq!(notices.admins(), scene.facts.admins);
    assert!(notices.devices().is_empty());
    // A refused opening tells nobody: there is no request.
    let mut scene = Scene::business();
    assert!(scene.try_open(Path::PasskeyWrap, at(0)).is_err());
}

#[test]
fn a_lost_notice_changes_no_state_and_the_request_is_listed_by_pending_for_person() {
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let (request, notices) = scene.try_open(path, at(0)).unwrap();
        // The notice is lost: whoever should have received it never did.
        drop(notices);
        let other = {
            let mut elsewhere = Scene::full();
            elsewhere.facts.person = member(2);
            elsewhere.try_open(Path::RecoveryCode, at(1)).unwrap().0
        };
        let requests = vec![request.clone(), other];
        // A device of the person that missed the notice lists the request when it syncs ...
        let mine = pending_for_person(&requests, member(PERSON));
        assert_eq!(mine.len(), 1, "{path}");
        assert_eq!(mine[0].id(), request.id());
        assert_eq!(*mine[0], request, "{path}: the request is as it was");
        // ... and the person with another id does not see it.
        let theirs = pending_for_person(&requests, member(2));
        assert_eq!(theirs.len(), 1);
        assert_ne!(theirs[0].id(), request.id());
        assert!(pending_for_person(&requests, member(3)).is_empty());
        // An admin lists every pending request.
        assert_eq!(pending(&requests).len(), 2);
        // The lost notice changed nothing: the hold, the evidence and the release are as if it
        // had arrived.
        let mut kept = request.clone();
        scene.complete(&mut kept, at(0));
        assert!(release(&kept, at(hold_secs(path)), StampCheck::Agreed).is_granted());
        // The device that finds the request can cancel it.
        let mut found = request.clone();
        cancel(
            &mut found,
            &scene.facts,
            Canceller::PersonDevice(device(1)),
            at(61),
        )
        .unwrap();
        assert!(pending_for_person(&[found], member(PERSON)).is_empty());
    }
    // A request is listed until it is closed. The list asks for no stamp, so that a stamp that
    // jumped cannot hide a request from the devices of the person; the sweep (`expire`) closes the
    // requests whose life is over.
    let mut scene = Scene::full();
    let (request, _) = scene.try_open(Path::RecoveryCode, at(0)).unwrap();
    let life = request.expires_at();
    let after_life = at(8 * DAY + 48 * 3_600);
    let mut requests = [request];
    assert_eq!(pending_for_person(&requests, member(PERSON)).len(), 1);
    assert!(requests[0].is_pending_at(life));
    assert!(!requests[0].is_pending_at(after_life));
    assert_eq!(
        pending_for_person(&requests, member(PERSON)).len(),
        1,
        "past its life and not swept yet: still listed"
    );
    assert!(expire(&mut requests[0], after_life, StampCheck::Agreed));
    assert!(pending_for_person(&requests, member(PERSON)).is_empty());
    assert!(pending(&requests).is_empty());
}

#[test]
fn cancel_notices_the_same_audience() {
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let (mut request, opened) = scene.try_open(path, at(0)).unwrap();
        let cancelled = cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(5),
        )
        .unwrap();
        assert_eq!(cancelled.len(), 1);
        let notice = cancelled.iter().next().unwrap();
        assert_eq!(notice.kind, NoticeKind::Cancelled);
        assert_eq!(notice.request, request.id());
        assert_eq!(notice.person, member(PERSON));
        // The audience of the opening: every admin and every active device of the person.
        assert_eq!(sorted(&cancelled), sorted(&opened), "{path}");
        assert_eq!(sorted(&cancelled), audience_of(&scene), "{path}");
        // A refused cancel makes no notice (it returns the refusal).
        assert!(cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(6)
        )
        .is_err());
    }
    // The audience is read from the facts of the moment of the cancel: a device that was added
    // since the opening is told of the cancel as well.
    let mut scene = Scene::full();
    let (mut request, opened) = scene.try_open(Path::RecoveryCode, at(0)).unwrap();
    scene.facts.active_devices.push(device(3));
    let cancelled = cancel(
        &mut request,
        &scene.facts,
        Canceller::Admin(member(10)),
        at(5),
    )
    .unwrap();
    assert_eq!(opened.devices(), vec![device(1), device(2)]);
    assert_eq!(cancelled.devices(), vec![device(1), device(2), device(3)]);
}

#[test]
fn the_release_notices_the_same_audience_and_a_second_call_none() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    let hold = at(hold_secs(Path::RecoveryCode));
    let grant = grant_of(&request, hold);
    // A second grant of the same open request, decided before the first was taken.
    let second = grant_of(&request, hold);
    assert_eq!(grant.request(), request.id());
    assert_eq!(grant.at(), hold);
    let served = mark_released(&mut request, &scene.facts, grant).unwrap();
    // The permission to serve says which wrap goes to which device, and nothing else does.
    assert_eq!(served.new_device(), device(common::NEW_DEVICE));
    assert_eq!(served.person(), member(PERSON));
    assert_eq!(served.path(), Path::RecoveryCode);
    assert_eq!(served.wrap(), WrapKind::RecoveryCode);
    assert_eq!(served.request(), request.id());
    assert_eq!(served.at(), hold);
    let released = served.into_notices();
    assert_eq!(released.len(), 1);
    assert_eq!(released.iter().next().unwrap().kind, NoticeKind::Released);
    assert_eq!(sorted(&released), audience_of(&scene));
    // The second grant of the same request: refused, no second permission to serve and nothing
    // new to tell (the group serves a device that lost the answer from its own record).
    assert_eq!(
        mark_released(&mut request, &scene.facts, second).unwrap_err(),
        ReleaseRefusal::AlreadyReleased { at: hold }
    );
    // Once the key is out, `release` makes no grant any more: it says when.
    match release(&request, hold, StampCheck::Agreed) {
        Release::AlreadyReleased { at: when } => assert_eq!(when, hold),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_release_notice_goes_to_the_audience_of_the_moment() {
    // A device that was added since the opening is told that the key went out.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    let grant = grant_of(&request, at(hold_secs(Path::RecoveryCode)));
    scene.facts.active_devices.push(device(3));
    let released = mark_released(&mut request, &scene.facts, grant)
        .unwrap()
        .into_notices();
    assert_eq!(released.devices(), vec![device(1), device(2), device(3)]);
    assert_eq!(released.admins(), scene.facts.admins);
}
