//! Check 5 of the brief: the clock. "A clock that jumps (refused by the Roughtime check)" never
//! releases a key (planning/architecture.md section 12 E2, section 2.4 "time is the leader's
//! stamp, checked").
//!
//! The crate reads no clock: every call takes the leader's stamp, and `release` a `StampCheck`
//! from S40. A disputed stamp never releases; a stamp that goes back neither extends the hold,
//! nor cancels or expires the request, nor makes an old attestation look young.

mod common;

use azlin_enroll::{
    cancel, expire, release, Canceller, Contribution, DeviceEvidenceKind, EvidenceRefusal,
    OpenLimits, OpenRefusal, Path, Reason, Release, RequestState, StampCheck, OWN_DEVICE_LIFE,
};
use common::{
    add_evidence, all_checks_pass, at, contribute, hold_secs, member, proof, Scene, AGENT, DAY,
    HOLD_PATHS, HOUR, MINUTE, PERSON,
};

const YEAR: u64 = 365 * DAY;

#[test]
fn disputed_stamp_never_releases() {
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        scene.complete(&mut request, at(0));
        let hold = hold_secs(path);
        // The hold is over and the evidence complete: only the dispute is left.
        for offset in [hold, hold + 1, hold + HOUR, hold + DAY, hold + 6 * DAY] {
            match release(&request, at(offset), StampCheck::Disputed) {
                Release::Withheld(Reason::StampDisputed) => {}
                other => panic!("{path} at +{offset}: {other:?}"),
            }
        }
        // The same instants with an agreed stamp release.
        assert!(release(&request, at(hold), StampCheck::Agreed).is_granted());
        assert!(release(&request, at(hold + HOUR), StampCheck::Agreed).is_granted());
    }
    // The own-device path as well: no hold, but a disputed stamp is no release.
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(0));
    scene.complete(&mut request, at(0));
    assert!(release(&request, at(0), StampCheck::Agreed).is_granted());
    match release(&request, at(0), StampCheck::Disputed) {
        Release::Withheld(Reason::StampDisputed) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_stamp_a_year_ahead_without_agreement_never_releases() {
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        scene.complete(&mut request, at(0));
        // The clock jumped a year ahead and the Roughtime check does not agree.
        match release(&request, at(YEAR), StampCheck::Disputed) {
            Release::Withheld(Reason::StampDisputed) => {}
            other => panic!("{path}: {other:?}"),
        }
        // The jump expires nothing either: the request is as it was.
        assert!(
            !expire(&mut request, at(YEAR), StampCheck::Disputed),
            "{path}"
        );
        assert_eq!(request.state(), RequestState::Open, "{path}");
        // When the time is really right (the check agrees), the hold has long ended, and so has
        // the life of an unfinished request: nobody can use the stale request, a new one is
        // opened.
        match release(&request, at(YEAR), StampCheck::Agreed) {
            Release::Withheld(Reason::Expired) => {}
            other => panic!("{path}: {other:?}"),
        }
        assert!(expire(&mut request, at(YEAR), StampCheck::Agreed), "{path}");
        assert_eq!(request.state(), RequestState::Expired);
        match release(&request, at(YEAR), StampCheck::Agreed) {
            Release::Withheld(Reason::Expired) => {}
            other => panic!("{path}: {other:?}"),
        }
    }
    // A jump that is inside the life of the request and agreed is just a later time: the leader's
    // stamp is the clock.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    assert!(release(&request, at(5 * DAY), StampCheck::Agreed).is_granted());
}

#[test]
fn a_stamp_going_backwards_neither_extends_nor_cancels() {
    let opened = 10_000;
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(opened));
        scene.complete(&mut request, at(opened));
        let hold_until = at(opened + hold_secs(path));
        let before = request.clone();
        // The stamp went back before the request was opened, even a day.
        for stamp in [at(0), at(opened - 1), at(opened), at(opened + 1)] {
            match release(&request, stamp, StampCheck::Agreed) {
                Release::Withheld(Reason::HoldNotElapsed { hold_until: until }) => {
                    assert_eq!(until, hold_until, "{path}: the hold is not extended")
                }
                other => panic!("{path}: {other:?}"),
            }
        }
        // Nothing was cancelled or expired by it, and nothing changed.
        assert!(!expire(&mut request, at(0), StampCheck::Agreed), "{path}");
        assert!(!expire(
            &mut request,
            at(opened - DAY.min(opened)),
            StampCheck::Agreed
        ));
        assert_eq!(request, before, "{path}: the request is as it was");
        assert_eq!(request.hold_until(), hold_until);
        // A call with a stamp that went back does no harm: the evidence that is already in stays
        // in, and the hold stays where it was.
        let _ = add_evidence(&mut request, proof(), at(0));
        assert_eq!(request.hold_until(), hold_until);
        assert!(request.is_open());
        // And when the stamp is right again, it releases at the very instant of the hold.
        assert!(
            release(&request, hold_until_stamp(path, opened), StampCheck::Agreed).is_granted(),
            "{path}"
        );
    }
}

fn hold_until_stamp(path: Path, opened: u64) -> azlin_seams::Stamp {
    at(opened + hold_secs(path))
}

#[test]
fn a_stamp_that_went_back_makes_no_attestation_younger() {
    // The request opened at 10,000. An attestation issued 1,000 s before that is 16 minutes old
    // at the opening: stale. A call whose stamp went back to 9,100 would see it as 100 s old if
    // the request believed the stamp; it judges at its own opening instead.
    let opened = 10_000;
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(opened));
    let stale = common::sso_of(PERSON, at(opened - 1_000));
    assert_eq!(
        add_evidence(&mut request, stale, at(opened - 900)).unwrap_err(),
        EvidenceRefusal::SsoStale
    );
    assert_eq!(
        add_evidence(&mut request, stale, at(0)).unwrap_err(),
        EvidenceRefusal::SsoStale
    );
    // An attestation issued at the opening is fresh whatever the call's stamp says.
    let young = common::sso_of(PERSON, at(opened));
    add_evidence(&mut request, young, at(opened - 900)).expect("issued at the opening");
    // The same for the shares: a stamp that went back does not stop an honest contribution.
    contribute(
        &mut request,
        Contribution::key_agent(member(AGENT), all_checks_pass()),
        at(opened - 900),
    )
    .unwrap();
    // A stamp forward of the expiry stops evidence and contributions (an expired request is
    // closed), even before the sweep ran.
    let late = at(opened + 48 * HOUR + 7 * DAY + 1);
    assert_eq!(
        contribute(&mut request, Contribution::admin(member(11)), late).unwrap_err(),
        azlin_enroll::ContributionRefusal::Expired
    );
    assert_eq!(
        add_evidence(
            &mut request,
            common::device_evidence(DeviceEvidenceKind::MdmCertificate),
            late
        )
        .unwrap_err(),
        EvidenceRefusal::Expired
    );
}

#[test]
fn a_cancel_is_never_refused_for_a_clock() {
    // Cancelling is the safe direction: whatever the stamp says, an admin's veto stands.
    for stamp in [at(0), at(5_000), at(5_000 + HOUR), at(YEAR)] {
        let mut scene = Scene::full();
        let mut request = scene.open(Path::PasskeyWrap, at(5_000));
        let notices = cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            stamp,
        )
        .expect("a cancel is never refused for its stamp");
        assert_eq!(notices.len(), 1);
        assert_eq!(
            request.state(),
            RequestState::Cancelled {
                by: Canceller::Admin(member(10)),
                at: stamp
            }
        );
    }
}

#[test]
fn the_life_of_a_request_is_seven_days_after_the_hold() {
    let mut scene = Scene::full();
    let request = scene.open(Path::RecoveryCode, at(1_000));
    assert_eq!(request.hold_until(), at(1_000 + 48 * HOUR));
    assert_eq!(request.expires_at(), at(1_000 + 48 * HOUR + 7 * DAY));
    // Still pending on the last second, gone one second later.
    assert!(request.is_pending_at(request.expires_at()));
    assert!(!request.is_pending_at(at(1_000 + 48 * HOUR + 7 * DAY + 1)));
    // The own-device path has no hold: a life of fifteen minutes after the opening (the QR flow).
    let request = scene.open(Path::OwnDevice, at(2_000));
    assert_eq!(OWN_DEVICE_LIFE.as_secs(), 15 * MINUTE);
    assert_eq!(request.expires_at(), at(2_000 + 15 * MINUTE));
}

#[test]
fn an_own_device_request_nobody_finished_expires_after_fifteen_minutes() {
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(0));
    scene.complete(&mut request, at(0));
    // Finished inside its life: released, on the last second too.
    assert!(release(&request, at(15 * MINUTE), StampCheck::Agreed).is_granted());
    // A second later the approver can no longer finish it: it must be opened again.
    match release(&request, at(15 * MINUTE + 1), StampCheck::Agreed) {
        Release::Withheld(Reason::Expired) => {}
        other => panic!("{other:?}"),
    }
    assert!(expire(
        &mut request,
        at(15 * MINUTE + 1),
        StampCheck::Agreed
    ));
    assert_eq!(request.state(), RequestState::Expired);
}

#[test]
fn a_disputed_stamp_opens_nothing_and_adds_nothing() {
    // The hold is fixed on the stamp of the opening: a clock that lags would shorten it in real
    // time, so no request is opened on a stamp that nobody vouches for, and nothing is counted.
    let mut scene = Scene::full();
    let mut limits = OpenLimits::standard();
    let refused = azlin_enroll::open(
        &scene.facts,
        &scene.policy,
        Path::RecoveryCode,
        scene.opener(Path::RecoveryCode),
        at(0),
        StampCheck::Disputed,
        &mut limits,
    );
    assert_eq!(refused.unwrap_err(), OpenRefusal::StampDisputed);
    assert_eq!(limits.tracked_ledgers(), 0);
    assert_eq!(limits.opened_today(member(PERSON), at(0)), 0);
    // The same call with an agreed stamp opens.
    scene.limits = limits;
    scene.open(Path::RecoveryCode, at(0));
    // Evidence on a disputed stamp is not added either: its age is a time-dependent fact.
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    assert_eq!(
        azlin_enroll::add_evidence(
            &mut request,
            common::sso_of(PERSON, at(0)),
            at(0),
            StampCheck::Disputed
        )
        .unwrap_err(),
        EvidenceRefusal::StampDisputed
    );
    assert!(request.evidence().is_empty());
}

#[test]
fn a_request_releases_on_its_last_second_and_expires_on_the_next() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(1_000));
    scene.complete(&mut request, at(1_000));
    let last = request.expires_at().as_secs() - common::BASE;
    // The last second of its life: it still releases and there is nothing to expire.
    assert!(release(&request, at(last), StampCheck::Agreed).is_granted());
    assert!(!expire(&mut request, at(last), StampCheck::Agreed));
    assert_eq!(request.state(), RequestState::Open);
    // The next second: expired by the stamp even before the sweep ran, and the sweep closes it.
    match release(&request, at(last + 1), StampCheck::Agreed) {
        Release::Withheld(Reason::Expired) => {}
        other => panic!("{other:?}"),
    }
    assert!(expire(&mut request, at(last + 1), StampCheck::Agreed));
    assert_eq!(request.state(), RequestState::Expired);
}

#[test]
fn a_disputed_stamp_takes_no_grant() {
    // The key goes out on a stamp that is agreed or not at all: a grant decided on an agreed stamp
    // is refused when it is taken on a disputed one, and the request is as it was.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    let hold = at(hold_secs(Path::RecoveryCode));
    let grant = match release(&request, hold, StampCheck::Agreed) {
        Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    };
    let before = request.clone();
    assert_eq!(
        azlin_enroll::mark_released(
            &mut request,
            &scene.facts,
            grant,
            hold,
            StampCheck::Disputed
        )
        .unwrap_err(),
        azlin_enroll::ReleaseRefusal::StampDisputed
    );
    assert_eq!(request, before);
    // The same grant on an agreed stamp is taken.
    let grant = match release(&request, hold, StampCheck::Agreed) {
        Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    };
    azlin_enroll::mark_released(&mut request, &scene.facts, grant, hold, StampCheck::Agreed)
        .expect("an agreed stamp");
}
