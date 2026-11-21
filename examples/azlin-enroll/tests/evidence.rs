//! Check 3 of the brief: the evidence. What each path needs before a key may be released, and the
//! rule of each piece (planning/architecture.md section 7.5, section 5.5, section 5.10).
//!
//! The SSO attestation is "necessary, never sufficient"; its subject is checked against the person
//! (the Bitwarden flaw of 2026, CVE-2026-60104); device evidence counts only as an MDM certificate;
//! the contacts' shares need two of the three; the passkey wrap needs the policy and user
//! verification.

mod common;

use azlin_enroll::{
    release, Confirmer, ContactSlot, Contribution, DeviceEvidenceKind, Evidence, EvidenceKind,
    EvidenceRefusal, OpenRefusal, Path, Reason, Release, StampCheck, CONTACT_SHARES_NEEDED,
    CONTACT_SHARES_TOTAL, MAX_EVIDENCE, PATHS, SSO_FRESH,
};
use azlin_policy::{AccountKind, DeviceApproval, Field, Policy};
use azlin_seams::Span;
use common::{
    add_evidence, all_checks_pass, at, contribute, device, device_evidence, hold_secs, member,
    passkey, proof, share, slot, Scene, AGENT, HOUR, MINUTE, PERSON,
};

/// The answer after the hold.
fn after_the_hold(request: &azlin_enroll::Request) -> Release {
    release(request, at(hold_secs(request.path())), StampCheck::Agreed)
}

/// The evidence kind a release lacks, if it is withheld for that.
fn missing(answer: Release) -> Option<EvidenceKind> {
    match answer {
        Release::Withheld(Reason::EvidenceMissing(kind)) => Some(kind),
        _ => None,
    }
}

fn sso(subject: u8, issued_at: u64) -> Evidence {
    common::sso_of(subject, at(issued_at))
}

#[test]
fn recovery_escrow_needs_fresh_sso_but_sso_alone_never_releases() {
    let mut scene = Scene::full();
    // SSO alone: the escrow threshold is missing, for ever.
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    add_evidence(&mut request, sso(PERSON, 0), at(0)).expect("a fresh attestation of the person");
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::EscrowThreshold)
    );
    assert_eq!(
        missing(release(&request, at(7 * 86_400), StampCheck::Agreed)),
        Some(EvidenceKind::EscrowThreshold),
        "sso alone does not release, however long it waits"
    );
    // The escrow alone: the SSO attestation is missing.
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    for c in [
        Contribution::key_agent(member(AGENT), all_checks_pass()),
        Contribution::admin(member(11)),
    ] {
        contribute(&mut request, c, at(0)).expect("the holders contribute");
    }
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::SsoAttestation)
    );
    // Both: it releases when the hold ends.
    add_evidence(&mut request, sso(PERSON, 0), at(0)).expect("the attestation");
    assert!(after_the_hold(&request).is_granted());
}

#[test]
fn sso_subject_must_match_the_person() {
    // The class of CVE-2026-60104: an approval whose subject is not checked against its sender.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(0));
    for other in [2u8, 11, 100, 200] {
        assert_eq!(
            add_evidence(&mut request, sso(other, 0), at(0)).unwrap_err(),
            EvidenceRefusal::SsoSubjectMismatch,
            "an attestation of member {other}"
        );
    }
    assert!(
        request.evidence().is_empty(),
        "a refused attestation is not kept"
    );
    // With the escrow complete, the attestation of somebody else still does not release.
    let mut again = scene.open(Path::EscrowRecovery, at(0));
    for c in [
        Contribution::key_agent(member(AGENT), all_checks_pass()),
        Contribution::admin(member(11)),
    ] {
        contribute(&mut again, c, at(0)).unwrap();
    }
    assert!(add_evidence(&mut again, sso(2, 0), at(0)).is_err());
    assert_eq!(
        missing(after_the_hold(&again)),
        Some(EvidenceKind::SsoAttestation),
        "the attestation of another person never counts"
    );
}

#[test]
fn stale_sso_attestation_refused() {
    let opened = 100_000;
    let fresh = SSO_FRESH.as_secs();
    assert_eq!(fresh, 10 * MINUTE, "ten minutes (an estimate of the lane)");
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(opened));
    // Eleven minutes old: stale.
    assert_eq!(
        add_evidence(&mut request, sso(PERSON, opened - fresh - 1), at(opened)).unwrap_err(),
        EvidenceRefusal::SsoStale
    );
    // From the future: refused as well.
    assert_eq!(
        add_evidence(&mut request, sso(PERSON, opened + 1), at(opened)).unwrap_err(),
        EvidenceRefusal::SsoFromTheFuture
    );
    assert!(request.evidence().is_empty());
    // Exactly ten minutes old: fresh.
    add_evidence(&mut request, sso(PERSON, opened - fresh), at(opened))
        .expect("an attestation of exactly the maximum age is fresh");
    // A policy with a shorter maximum age makes it shorter.
    let mut scene = Scene::full();
    scene.policy.sso_max_age = Some(Span::minutes(5));
    let mut request = scene.open(Path::EscrowRecovery, at(opened));
    assert_eq!(
        add_evidence(&mut request, sso(PERSON, opened - 6 * MINUTE), at(opened)).unwrap_err(),
        EvidenceRefusal::SsoStale
    );
    add_evidence(&mut request, sso(PERSON, opened - 5 * MINUTE), at(opened)).unwrap();
    // A policy with a LONGER maximum age (kept for identity-service outages) does not make a
    // recovery's attestation older.
    let mut scene = Scene::full();
    scene.policy.sso_max_age = Some(Span::hours(8));
    let mut request = scene.open(Path::EscrowRecovery, at(opened));
    assert_eq!(
        add_evidence(&mut request, sso(PERSON, opened - HOUR), at(opened)).unwrap_err(),
        EvidenceRefusal::SsoStale
    );
}

#[test]
fn mdm_certificate_is_device_evidence_tpm_attestation_is_not() {
    assert!(DeviceEvidenceKind::MdmCertificate.counts_as_company_device());
    assert!(!DeviceEvidenceKind::TpmAttestation.counts_as_company_device());
    assert!(!DeviceEvidenceKind::SecureEnclaveAttestation.counts_as_company_device());
    for path in [
        Path::OwnDevice,
        Path::QuorumEnrollment,
        Path::RecoveryCode,
        Path::ContactShares,
        Path::PasskeyWrap,
        Path::EscrowRecovery,
    ] {
        let mut scene = Scene::full();
        // An MDM certificate tied to the inventory is device evidence.
        let mut request = scene.open(path, at(0));
        assert!(!request.company_device());
        add_evidence(
            &mut request,
            device_evidence(DeviceEvidenceKind::MdmCertificate),
            at(0),
        )
        .unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(request.company_device(), "{path}");
        // A TPM or Secure Enclave attestation proves genuine hardware, not a company device.
        for hardware in [
            DeviceEvidenceKind::TpmAttestation,
            DeviceEvidenceKind::SecureEnclaveAttestation,
        ] {
            let mut request = scene.open(path, at(0));
            assert_eq!(
                add_evidence(&mut request, device_evidence(hardware), at(0)).unwrap_err(),
                EvidenceRefusal::NotDeviceEvidence(hardware),
                "{path}"
            );
            assert!(!request.company_device(), "{path}: {hardware:?}");
            assert!(request.evidence().is_empty(), "{path}");
        }
    }
    // Device evidence is no substitute for what the path needs.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    add_evidence(
        &mut request,
        device_evidence(DeviceEvidenceKind::MdmCertificate),
        at(0),
    )
    .unwrap();
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::RecoveryCodeProof)
    );
}

#[test]
fn contact_shares_need_two_of_three() {
    assert_eq!(CONTACT_SHARES_NEEDED, 2);
    assert_eq!(CONTACT_SHARES_TOTAL, 3);
    for bad in [0u8, 4, 5, 255] {
        assert!(ContactSlot::new(bad).is_err(), "slot {bad}");
    }
    for good in [1u8, 2, 3] {
        assert_eq!(ContactSlot::new(good).unwrap().get(), good);
    }
    let mut scene = Scene::full();
    // One share: not enough, and the same contact twice is still one contact.
    let mut request = scene.open(Path::ContactShares, at(0));
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::ContactShares)
    );
    add_evidence(&mut request, share(1), at(0)).unwrap();
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::ContactShares),
        "one of three"
    );
    add_evidence(&mut request, share(1), at(1)).unwrap();
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::ContactShares),
        "the same contact twice is one"
    );
    assert_eq!(request.evidence().len(), 1);
    // Two different contacts: enough, whichever two.
    for pair in [(1u8, 2u8), (1, 3), (2, 3), (3, 1)] {
        let mut request = scene.open(Path::ContactShares, at(0));
        add_evidence(&mut request, share(pair.0), at(0)).unwrap();
        add_evidence(&mut request, share(pair.1), at(0)).unwrap();
        assert!(after_the_hold(&request).is_granted(), "contacts {pair:?}");
    }
    // All three.
    let mut request = scene.open(Path::ContactShares, at(0));
    for n in [1u8, 2, 3] {
        add_evidence(&mut request, share(n), at(0)).unwrap();
    }
    assert_eq!(request.evidence().len(), 3);
    assert!(after_the_hold(&request).is_granted());
}

#[test]
fn passkey_path_refused_when_policy_passkey_recovery_is_off() {
    let mut scene = Scene::business();
    assert!(
        !scene.policy.passkey_recovery,
        "off by default for a business"
    );
    assert_eq!(
        scene.try_open(Path::PasskeyWrap, at(0)).unwrap_err(),
        OpenRefusal::PolicyForbids {
            field: Field::PasskeyRecovery
        }
    );
    // Nothing was counted against the person's limits by the refusal.
    assert_eq!(scene.limits.opened_today(member(PERSON), at(0)), 0);
    // A business admin may switch it on (5.5): then it opens.
    scene.policy.passkey_recovery = true;
    let (mut request, _) = scene
        .try_open(Path::PasskeyWrap, at(0))
        .expect("switched on");
    // And an assertion without user verification is no evidence.
    assert_eq!(
        add_evidence(&mut request, passkey(false), at(0)).unwrap_err(),
        EvidenceRefusal::UserVerificationMissing
    );
    assert_eq!(
        missing(after_the_hold(&request)),
        Some(EvidenceKind::PasskeyAssertion)
    );
    add_evidence(&mut request, passkey(true), at(0)).unwrap();
    assert!(after_the_hold(&request).is_granted());
}

#[test]
fn passkey_path_open_for_personal_and_family_by_default() {
    for kind in [AccountKind::Personal, AccountKind::Family] {
        let mut scene = Scene::personal();
        scene.policy = Policy::defaults(kind);
        let (mut request, _) = scene
            .try_open(Path::PasskeyWrap, at(0))
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        scene.complete(&mut request, at(0));
        // A new phone signed in to the same Apple or Google account recovers after the 48 h hold.
        assert!(!release(&request, at(48 * HOUR - 1), StampCheck::Agreed).is_granted());
        assert!(release(&request, at(48 * HOUR), StampCheck::Agreed).is_granted());
    }
    for kind in [AccountKind::Team, AccountKind::Business] {
        let mut scene = Scene::business();
        scene.policy = Policy::defaults(kind);
        assert_eq!(
            scene.try_open(Path::PasskeyWrap, at(0)).unwrap_err(),
            OpenRefusal::PolicyForbids {
                field: Field::PasskeyRecovery
            },
            "{kind:?}"
        );
    }
    assert!(Scene::family().try_open(Path::PasskeyWrap, at(0)).is_ok());
}

#[test]
fn quorum_enrollment_refused_when_device_approval_is_self_only() {
    for kind in [AccountKind::Personal, AccountKind::Family] {
        let mut scene = Scene::business();
        scene.policy = Policy::defaults(kind);
        assert_eq!(scene.policy.device_approval, DeviceApproval::SelfOnly);
        assert_eq!(
            scene.try_open(Path::QuorumEnrollment, at(0)).unwrap_err(),
            OpenRefusal::PolicyForbids {
                field: Field::DeviceApproval
            },
            "{kind:?}"
        );
    }
    for kind in [AccountKind::Team, AccountKind::Business] {
        let mut scene = Scene::business();
        scene.policy = Policy::defaults(kind);
        assert_eq!(scene.policy.device_approval, DeviceApproval::SelfOrQuorum);
        assert!(
            scene.try_open(Path::QuorumEnrollment, at(0)).is_ok(),
            "{kind:?}"
        );
    }
    // A policy that an admin set to "self" closes the path again.
    let mut scene = Scene::business();
    scene.policy.device_approval = DeviceApproval::SelfOnly;
    assert!(scene.try_open(Path::QuorumEnrollment, at(0)).is_err());
}

#[test]
fn max_devices_per_member_refuses_the_extra_device() {
    // The default is ten (5.10).
    let mut scene = Scene::full();
    assert_eq!(scene.policy.max_devices_per_member, 10);
    scene.facts.active_devices = (1..=10).map(device).collect();
    for path in [
        Path::OwnDevice,
        Path::QuorumEnrollment,
        Path::RecoveryCode,
        Path::ContactShares,
        Path::PasskeyWrap,
        Path::EscrowRecovery,
    ] {
        scene.limits = azlin_enroll::OpenLimits::standard();
        assert_eq!(
            scene.try_open(path, at(0)).unwrap_err(),
            OpenRefusal::DevicesFull { have: 10, max: 10 },
            "{path}"
        );
    }
    // Nine devices: the tenth may come.
    scene.facts.active_devices.pop();
    scene.limits = azlin_enroll::OpenLimits::standard();
    assert!(scene.try_open(Path::RecoveryCode, at(0)).is_ok());
    // A policy with a smaller number.
    let mut scene = Scene::full();
    scene.policy.max_devices_per_member = 2;
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::DevicesFull { have: 2, max: 2 }
    );
    // An escrow opening enrolls no device of the person, so the limit does not apply to it.
    let mut scene = Scene::full();
    scene.facts.active_devices = (1..=10).map(device).collect();
    assert!(scene.try_open(Path::EscrowOpen, at(0)).is_ok());
    // And a device that is already an active device cannot be enrolled again.
    let mut scene = Scene::full();
    scene.facts.active_devices.push(device(common::NEW_DEVICE));
    assert_eq!(
        scene.try_open(Path::RecoveryCode, at(0)).unwrap_err(),
        OpenRefusal::DeviceAlreadyActive
    );
}

#[test]
fn a_piece_of_evidence_the_path_does_not_ask_for_is_refused() {
    // Every kind of evidence, tried on every path: it is accepted exactly where the table lists
    // it (and device evidence, which is optional, everywhere).
    let samples: [(EvidenceKind, Evidence); 6] = [
        (
            EvidenceKind::FingerprintPhrase,
            Evidence::PhraseConfirmed(Confirmer::Device(device(1))),
        ),
        (EvidenceKind::SsoAttestation, sso(PERSON, 0)),
        (EvidenceKind::RecoveryCodeProof, proof()),
        (EvidenceKind::ContactShares, share(1)),
        (EvidenceKind::PasskeyAssertion, passkey(true)),
        (
            EvidenceKind::DeviceEvidence,
            device_evidence(DeviceEvidenceKind::MdmCertificate),
        ),
    ];
    for (path, rule) in PATHS {
        for (kind, piece) in samples {
            let mut scene = Scene::full();
            let mut request = scene.open(path, at(0));
            let listed = rule.evidence.contains(&kind) || kind == EvidenceKind::DeviceEvidence;
            let result = add_evidence(&mut request, piece, at(0));
            if listed {
                // The phrase of the quorum path comes from an admin, not the device: that is
                // `WrongConfirmer`, not "not for this path".
                if path != Path::QuorumEnrollment || kind != EvidenceKind::FingerprintPhrase {
                    assert!(result.is_ok(), "{path}: {kind:?}: {result:?}");
                }
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    EvidenceRefusal::NotForThisPath { path, kind },
                    "{path}: {kind:?}"
                );
            }
        }
    }
}

#[test]
fn the_phrase_is_confirmed_only_by_the_one_who_opened_the_request() {
    let mut scene = Scene::full();
    // The own-device path: the approving device, not another active device of the person.
    let mut request = scene.open(Path::OwnDevice, at(0));
    for wrong in [
        Confirmer::Device(device(2)),
        Confirmer::Device(device(common::NEW_DEVICE)),
        Confirmer::Member(member(PERSON)),
        Confirmer::Member(member(10)),
    ] {
        assert_eq!(
            add_evidence(&mut request, Evidence::PhraseConfirmed(wrong), at(0)).unwrap_err(),
            EvidenceRefusal::WrongConfirmer,
            "{wrong:?}"
        );
    }
    add_evidence(
        &mut request,
        Evidence::PhraseConfirmed(Confirmer::Device(device(1))),
        at(0),
    )
    .unwrap();
    // The quorum path: one of the admins who signed, not an admin who did not, not a device.
    let mut request = scene.open(Path::QuorumEnrollment, at(0));
    for wrong in [
        Confirmer::Member(member(10)),
        Confirmer::Member(member(PERSON)),
        Confirmer::Device(device(1)),
    ] {
        assert_eq!(
            add_evidence(&mut request, Evidence::PhraseConfirmed(wrong), at(0)).unwrap_err(),
            EvidenceRefusal::WrongConfirmer,
            "{wrong:?}"
        );
    }
    for right in [member(11), member(12)] {
        let mut again = scene.open(Path::QuorumEnrollment, at(0));
        add_evidence(
            &mut again,
            Evidence::PhraseConfirmed(Confirmer::Member(right)),
            at(0),
        )
        .unwrap();
    }
}

#[test]
fn a_second_piece_of_a_kind_adds_nothing_and_the_evidence_is_bounded() {
    let mut scene = Scene::full();
    let mut request = scene.open(Path::EscrowRecovery, at(1_000));
    add_evidence(&mut request, sso(PERSON, 900), at(1_000)).unwrap();
    // A second attestation (younger) does not replace the first and does not fail.
    add_evidence(&mut request, sso(PERSON, 990), at(1_000)).unwrap();
    assert_eq!(request.evidence().len(), 1);
    assert_eq!(request.evidence().first(), Some(&sso(PERSON, 900)));
    // Device evidence is kept once.
    for _ in 0..3 {
        add_evidence(
            &mut request,
            device_evidence(DeviceEvidenceKind::MdmCertificate),
            at(1_000),
        )
        .unwrap();
    }
    assert_eq!(request.evidence().len(), 2);
    assert!(request.evidence().len() <= MAX_EVIDENCE);
}

#[test]
fn evidence_made_for_another_device_is_refused() {
    // Every fact names the device it was verified for (an attestation of the identity service
    // names one, section 5.11): a proof made for one request cannot be moved into another request
    // of the same person by a mix-up of the glue.
    let other = device(common::NEW_DEVICE + 1);
    let cases: [(Path, Evidence); 5] = [
        (
            Path::EscrowRecovery,
            Evidence::Sso {
                subject: member(PERSON),
                device: other,
                issued_at: at(0),
            },
        ),
        (
            Path::RecoveryCode,
            Evidence::RecoveryCodeProof { device: other },
        ),
        (
            Path::ContactShares,
            Evidence::ContactShare {
                slot: slot(1),
                device: other,
            },
        ),
        (
            Path::PasskeyWrap,
            Evidence::PasskeyAssertion {
                device: other,
                user_verified: true,
            },
        ),
        (
            Path::RecoveryCode,
            Evidence::Device {
                device: other,
                kind: DeviceEvidenceKind::MdmCertificate,
            },
        ),
    ];
    for (path, piece) in cases {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        assert_eq!(
            add_evidence(&mut request, piece, at(0)).unwrap_err(),
            EvidenceRefusal::WrongDevice,
            "{path}: {piece:?}"
        );
        assert!(
            request.evidence().is_empty(),
            "{path}: a refusal keeps nothing"
        );
    }
    // The same kinds for the device of the request are accepted.
    for (path, piece) in [
        (Path::EscrowRecovery, sso(PERSON, 0)),
        (Path::RecoveryCode, proof()),
        (Path::ContactShares, share(1)),
        (Path::PasskeyWrap, passkey(true)),
        (
            Path::RecoveryCode,
            device_evidence(DeviceEvidenceKind::MdmCertificate),
        ),
    ] {
        let mut scene = Scene::full();
        let mut request = scene.open(path, at(0));
        add_evidence(&mut request, piece, at(0)).unwrap_or_else(|e| panic!("{path}: {e}"));
    }
}

#[test]
fn a_closed_request_takes_no_evidence() {
    // Cancelled, released or expired: nothing is added any more, and the reason is named.
    let mut scene = Scene::full();
    let mut request = scene.open(Path::RecoveryCode, at(0));
    azlin_enroll::cancel(
        &mut request,
        &scene.facts,
        azlin_enroll::Canceller::Admin(member(10)),
        at(1),
    )
    .unwrap();
    assert_eq!(
        add_evidence(&mut request, proof(), at(2)).unwrap_err(),
        EvidenceRefusal::NotOpen
    );
    let mut request = scene.open(Path::RecoveryCode, at(0));
    let late = request.expires_at().as_secs() - common::BASE + 1;
    assert_eq!(
        add_evidence(&mut request, proof(), at(late)).unwrap_err(),
        EvidenceRefusal::Expired
    );
    assert!(request.evidence().is_empty());
}
