//! Check 1 of the brief: the holds. "No key is released before its hold ends on any path"
//! (planning/architecture.md section 12 E2, section 7.5, section 5.10 `enroll_hold_s` and
//! `escrow_open_wait_s`, section 14 D42).
//!
//! Every test completes ALL the evidence of the path first, so that the hold is the only thing
//! between the request and the key.

mod common;

use azlin_enroll::{
    hold_length, life_after, release, Confirmer, Evidence, EvidenceKind, HoldKind, Opener, Path,
    Reason, Release, StampCheck, EXPIRES_AFTER_HOLD, OFFBOARDING_QUORUM_MIN, OWN_DEVICE_LIFE,
    PATHS,
};
use azlin_policy::Signatory;
use common::{add_evidence, at, hold_secs, open, Scene, DAY, HOLD_PATHS, HOUR};

/// The answer is `Withheld(HoldNotElapsed)` naming `hold_until`.
fn assert_hold_not_elapsed(answer: Release, hold_until: azlin_seams::Stamp, label: &str) {
    match answer {
        Release::Withheld(Reason::HoldNotElapsed { hold_until: got }) => {
            assert_eq!(got, hold_until, "{label}")
        }
        other => panic!("{label}: expected HoldNotElapsed, got {other:?}"),
    }
}

/// One test of check 1 for a hold path.
macro_rules! hold_withholds {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let path: Path = $path;
            let mut scene = Scene::full();
            let mut request = scene.open(path, at(0));
            scene.complete(&mut request, at(0));
            let hold = hold_secs(path);
            assert!(hold > 0, "{path} is a hold path");
            assert_eq!(request.hold_until(), at(hold), "{path}: hold_until");
            for offset in [0, 1, HOUR, hold / 2, hold - HOUR, hold - 1] {
                assert_hold_not_elapsed(
                    release(&request, at(offset), StampCheck::Agreed),
                    at(hold),
                    &format!("{path} at +{offset} s"),
                );
            }
            assert!(
                release(&request, at(hold), StampCheck::Agreed).is_granted(),
                "{path}: granted at hold_until"
            );
        }
    };
}

hold_withholds!(
    hold_withholds_release_before_hold_until_quorum_enrollment,
    Path::QuorumEnrollment
);
hold_withholds!(
    hold_withholds_release_before_hold_until_recovery_code,
    Path::RecoveryCode
);
hold_withholds!(
    hold_withholds_release_before_hold_until_contact_shares,
    Path::ContactShares
);
hold_withholds!(
    hold_withholds_release_before_hold_until_passkey_wrap,
    Path::PasskeyWrap
);
hold_withholds!(
    hold_withholds_release_before_hold_until_escrow_recovery,
    Path::EscrowRecovery
);
hold_withholds!(
    hold_withholds_release_before_hold_until_escrow_open,
    Path::EscrowOpen
);

#[test]
fn release_one_second_before_is_withheld_and_at_hold_until_is_granted() {
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let opened = 12_345;
        let mut request = scene.open(path, at(opened));
        scene.complete(&mut request, at(opened));
        let until = opened + hold_secs(path);
        assert_eq!(request.hold_until(), at(until));
        assert_hold_not_elapsed(
            release(&request, at(until - 1), StampCheck::Agreed),
            at(until),
            &format!("{path} one second before"),
        );
        assert!(
            release(&request, at(until), StampCheck::Agreed).is_granted(),
            "{path}: at hold_until"
        );
        assert!(
            release(&request, at(until + 1), StampCheck::Agreed).is_granted(),
            "{path}: one second after"
        );
        assert!(
            release(&request, at(until + DAY), StampCheck::Agreed).is_granted(),
            "{path}: a day after"
        );
    }
}

#[test]
fn own_device_path_has_no_hold() {
    // The table: the own-device path is the only one without a hold.
    for (path, rule) in PATHS {
        assert_eq!(
            rule.hold == HoldKind::None,
            path == Path::OwnDevice,
            "{path}: only the own-device path has no hold"
        );
    }
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(500));
    assert_eq!(request.hold_until(), request.opened_at());
    // Without the phrase there is nothing to release: the missing evidence is what withholds.
    match release(&request, at(500), StampCheck::Agreed) {
        Release::Withheld(Reason::EvidenceMissing(EvidenceKind::FingerprintPhrase)) => {}
        other => panic!("expected the missing phrase, got {other:?}"),
    }
    add_evidence(
        &mut request,
        Evidence::PhraseConfirmed(Confirmer::Device(common::device(1))),
        at(500),
    )
    .expect("the approving device confirms the phrase");
    // The same instant: no hold.
    assert!(release(&request, at(500), StampCheck::Agreed).is_granted());
}

#[test]
fn hold_length_is_the_policys_enroll_hold() {
    for hours in [24u32, 48, 96] {
        for path in [
            Path::QuorumEnrollment,
            Path::RecoveryCode,
            Path::ContactShares,
            Path::PasskeyWrap,
            Path::EscrowRecovery,
        ] {
            let mut scene = Scene::full();
            scene.policy.enroll_hold = azlin_seams::Span::hours(hours);
            let mut request = scene.open(path, at(1_000));
            let hold = u64::from(hours) * HOUR;
            assert_eq!(
                request.hold_until(),
                at(1_000 + hold),
                "{path} with a hold of {hours} h"
            );
            assert_eq!(
                hold_length(HoldKind::EnrollHold, &scene.policy, &scene.facts),
                azlin_seams::Span::hours(hours)
            );
            scene.complete(&mut request, at(1_000));
            assert_hold_not_elapsed(
                release(&request, at(1_000 + hold - 1), StampCheck::Agreed),
                at(1_000 + hold),
                &format!("{path} {hours} h"),
            );
            assert!(release(&request, at(1_000 + hold), StampCheck::Agreed).is_granted());
        }
    }
    // The escrow opening waits its own wait, not the enrollment hold.
    let mut scene = Scene::full();
    scene.policy.enroll_hold = azlin_seams::Span::hours(24);
    scene.policy.escrow_open_wait = azlin_seams::Span::hours(96);
    let request = scene.open(Path::EscrowOpen, at(0));
    assert_eq!(request.hold_until(), at(96 * HOUR));
}

#[test]
fn hold_is_fixed_at_open_and_a_later_policy_change_does_not_shorten_it() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    scene.complete(&mut request, at(0));
    assert_eq!(request.hold_until(), at(48 * HOUR));
    // The admins (or a thief with one admin's session) shorten the hold afterwards.
    scene.policy.enroll_hold = azlin_seams::Span::hours(1);
    scene.policy.escrow_open_wait = azlin_seams::Span::ZERO;
    assert_eq!(
        request.hold_until(),
        at(48 * HOUR),
        "the request keeps its hold"
    );
    assert_hold_not_elapsed(
        release(&request, at(2 * HOUR), StampCheck::Agreed),
        at(48 * HOUR),
        "two hours after, under the shortened policy",
    );
    assert!(release(&request, at(48 * HOUR), StampCheck::Agreed).is_granted());
    // A request opened afterwards reads the policy of that moment.
    let later = scene.open(Path::RecoveryCode, at(DAY));
    assert_eq!(later.hold_until(), at(DAY + HOUR));
    // And a LONGER policy later does not lengthen the running one either.
    scene.policy.enroll_hold = azlin_seams::Span::hours(200);
    assert_eq!(request.hold_until(), at(48 * HOUR));
}

#[test]
fn escrow_open_waits_72h_for_an_active_subject() {
    let mut scene = Scene::full();
    assert_eq!(scene.facts.offboarded_by, 0);
    let mut request = scene.open(Path::EscrowOpen, at(0));
    scene.complete(&mut request, at(0));
    assert_eq!(request.hold_until(), at(72 * HOUR));
    assert_hold_not_elapsed(
        release(&request, at(72 * HOUR - 1), StampCheck::Agreed),
        at(72 * HOUR),
        "an active subject",
    );
    assert!(release(&request, at(72 * HOUR), StampCheck::Agreed).is_granted());
    // A subject who is offboarded by ONE admin (no quorum: one person is never a quorum, whatever
    // the policy's own number is) also waits the full wait.
    for policy_quorum in [1u8, 2] {
        let mut scene = Scene::full();
        scene.policy.admin_quorum = azlin_policy::AdminQuorum::new(policy_quorum).unwrap();
        scene.facts.active_devices.clear();
        scene.facts.offboarded_by = 1;
        let request = scene.open(Path::EscrowOpen, at(0));
        assert_eq!(
            request.hold_until(),
            at(72 * HOUR),
            "quorum {policy_quorum}"
        );
    }
}

#[test]
fn escrow_open_has_no_wait_for_a_subject_offboarded_by_quorum() {
    let mut scene = Scene::full();
    scene.facts.offboarded_by = 2;
    scene.facts.active_devices.clear();
    let mut request = scene.open(Path::EscrowOpen, at(5_000));
    assert_eq!(request.hold_until(), request.opened_at(), "no wait");
    // The two people and the shares are still needed: no wait is not no rules.
    match release(&request, at(5_000), StampCheck::Agreed) {
        Release::Withheld(Reason::EvidenceMissing(EvidenceKind::EscrowThreshold)) => {}
        other => panic!("expected the missing escrow threshold, got {other:?}"),
    }
    scene.complete(&mut request, at(5_000));
    assert!(release(&request, at(5_000), StampCheck::Agreed).is_granted());
    // A quorum offboarding does not shorten the ENROLLMENT hold of any other path.
    let mut scene = Scene::full();
    scene.facts.offboarded_by = 2;
    scene.facts.active_devices.clear();
    let request = scene.open(Path::EscrowRecovery, at(0));
    assert_eq!(request.hold_until(), at(48 * HOUR));
    assert_eq!(
        hold_length(HoldKind::EscrowOpenWait, &scene.policy, &scene.facts),
        azlin_seams::Span::ZERO
    );
}

#[test]
fn a_quorum_offboarding_is_two_admins_in_a_workspace_with_at_least_two() {
    assert_eq!(OFFBOARDING_QUORUM_MIN, 2);
    // (admins who signed the offboarding, admins of the workspace, the wait in hours)
    let cases: [(u8, u8, u64); 8] = [
        (0, 3, 72),
        (1, 3, 72),
        (2, 3, 0),
        (3, 3, 0),
        (2, 2, 0),
        (1, 2, 72),
        (2, 1, 72),
        (5, 1, 72),
    ];
    for (offboarded_by, admin_count, wait) in cases {
        let mut scene = Scene::full();
        // The offboarded member has no device left.
        scene.facts.active_devices.clear();
        scene.facts.offboarded_by = offboarded_by;
        scene.facts.admin_count = admin_count;
        scene.facts.admins = [10u8, 11, 12]
            .into_iter()
            .take(usize::from(admin_count))
            .map(common::member)
            .collect();
        // With one admin only, the one admin asks alone (the relaxation of section 7.5).
        let opener = if admin_count == 1 {
            Opener::admins(vec![Signatory::person(common::member(10))], common::nd())
        } else {
            scene.opener(Path::EscrowOpen)
        };
        let (request, _) = open(
            &scene.facts,
            &scene.policy,
            Path::EscrowOpen,
            opener,
            at(0),
            &mut scene.limits,
        )
        .unwrap_or_else(|e| panic!("{offboarded_by} of {admin_count}: {e}"));
        assert_eq!(
            request.hold_until(),
            at(wait * HOUR),
            "offboarded by {offboarded_by} of {admin_count} admins"
        );
    }
    // A member who is offboarded by a quorum but still has an active device is still an active
    // member: that device can object, so the wait is the full one.
    let mut scene = Scene::full();
    scene.facts.offboarded_by = 2;
    assert!(!scene.facts.active_devices.is_empty());
    let request = scene.open(Path::EscrowOpen, at(0));
    assert_eq!(request.hold_until(), at(72 * HOUR));
}

#[test]
fn a_request_lives_seven_days_after_its_hold_and_an_own_device_request_fifteen_minutes() {
    assert_eq!(EXPIRES_AFTER_HOLD.as_secs(), 7 * DAY);
    assert_eq!(OWN_DEVICE_LIFE.as_secs(), 15 * 60);
    assert_eq!(life_after(HoldKind::None), OWN_DEVICE_LIFE);
    assert_eq!(life_after(HoldKind::EnrollHold), EXPIRES_AFTER_HOLD);
    assert_eq!(life_after(HoldKind::EscrowOpenWait), EXPIRES_AFTER_HOLD);
    for path in HOLD_PATHS {
        let mut scene = Scene::full();
        let request = scene.open(path, at(1_000));
        assert_eq!(
            request.expires_at(),
            at(1_000 + hold_secs(path) + 7 * DAY),
            "{path}"
        );
    }
    let mut scene = Scene::full();
    let request = scene.open(Path::OwnDevice, at(1_000));
    assert_eq!(request.expires_at(), at(1_000 + 15 * 60));
}
