//! The table of paths as DATA (planning/architecture.md section 7.5, section 5.5, section 7.6):
//! `PATHS` is the one source of every decision (hold, evidence, openers, gate, wrap), so these
//! tests pin the table itself, row by row, from the plan: a row edited by mistake is red here
//! before a matrix of S47b sees it.

mod common;

use azlin_enroll::{EvidenceKind, HoldKind, OpenerKind, Path, PolicyGate, WrapKind, PATHS};

#[test]
fn paths_table_is_the_seven_paths_in_the_order_of_all() {
    assert_eq!(PATHS.len(), 7);
    assert_eq!(Path::ALL.len(), 7);
    for (index, (path, rule)) in PATHS.iter().enumerate() {
        assert_eq!(*path, Path::ALL[index], "the order of the rows");
        assert_eq!(
            path.rule(),
            rule,
            "{path}: `Path::rule` is the row of the table"
        );
    }
    // Words: one spelling each, strict.
    for path in Path::ALL {
        assert_eq!(path.as_str().parse::<Path>(), Ok(path));
        assert_eq!(path.to_string(), path.as_str());
        assert!(path
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'_'));
        assert!(path.as_str().to_uppercase().parse::<Path>().is_err());
        assert!(format!(" {}", path.as_str()).parse::<Path>().is_err());
    }
    assert!("".parse::<Path>().is_err());
    assert!("recovery".parse::<Path>().is_err());
}

#[test]
fn each_row_of_the_table_is_what_the_plan_says() {
    use EvidenceKind as E;
    // (path, hold, evidence, gate, wrap) written out: the plan's 7.5 and 5.5.
    let expected: [(Path, HoldKind, &[EvidenceKind], PolicyGate, WrapKind); 7] = [
        // "the person's own existing device approves (default) | fingerprint phrase; that device
        // wraps the member key; no hold needed"
        (
            Path::OwnDevice,
            HoldKind::None,
            &[E::FingerprintPhrase],
            PolicyGate::Always,
            WrapKind::SealedToDevice,
        ),
        // `device_approval` "self or quorum", `admin_quorum` "quorum enrollments"; "any enrollment
        // without the person's own device" waits the enrollment hold
        (
            Path::QuorumEnrollment,
            HoldKind::EnrollHold,
            &[E::FingerprintPhrase, E::AdminQuorum],
            PolicyGate::DeviceApprovalAllowsQuorum,
            WrapKind::SealedToDevice,
        ),
        // 5.5: the recovery code (alg 3); "the same hold applies to every recovery"
        (
            Path::RecoveryCode,
            HoldKind::EnrollHold,
            &[E::RecoveryCodeProof],
            PolicyGate::Always,
            WrapKind::RecoveryCode,
        ),
        // 5.5: the contacts' 2-of-3 shares (alg 5)
        (
            Path::ContactShares,
            HoldKind::EnrollHold,
            &[E::ContactShares],
            PolicyGate::Always,
            WrapKind::ContactShare,
        ),
        // 5.5: the passkey wrap (alg 4), `passkey_recovery` in the policy
        (
            Path::PasskeyWrap,
            HoldKind::EnrollHold,
            &[E::PasskeyAssertion],
            PolicyGate::PasskeyRecoveryOn,
            WrapKind::Passkey,
        ),
        // 7.5: "a fresh SSO attestation for the person (necessary, never sufficient), plus escrow
        // opened by the threshold (7.6), plus the hold"
        (
            Path::EscrowRecovery,
            HoldKind::EnrollHold,
            &[E::SsoAttestation, E::EscrowThreshold],
            PolicyGate::ManagedMemberWithEscrow,
            WrapKind::Escrow,
        ),
        // 7.5: "an `escrow_open` entry with a reason, signed by two different admins ... 72 h for
        // an active member"
        (
            Path::EscrowOpen,
            HoldKind::EscrowOpenWait,
            &[E::AdminQuorum, E::EscrowThreshold],
            PolicyGate::ManagedMemberWithEscrow,
            WrapKind::Escrow,
        ),
    ];
    for (index, (path, hold, evidence, gate, wrap)) in expected.into_iter().enumerate() {
        let (table_path, rule) = PATHS[index];
        assert_eq!(table_path, path);
        assert_eq!(rule.hold, hold, "{path}: hold");
        assert_eq!(rule.evidence, evidence, "{path}: evidence");
        assert_eq!(rule.policy_gate, gate, "{path}: gate");
        assert_eq!(rule.wrap, wrap, "{path}: wrap");
    }
}

#[test]
fn the_classes_of_paths_follow_the_plan() {
    // "The same hold applies to every recovery: the recovery code, the contacts' shares and the
    // passkey wrap", and the escrow recovery.
    let recoveries: Vec<Path> = Path::ALL.into_iter().filter(|p| p.is_recovery()).collect();
    assert_eq!(
        recoveries,
        vec![
            Path::RecoveryCode,
            Path::ContactShares,
            Path::PasskeyWrap,
            Path::EscrowRecovery
        ]
    );
    for path in &recoveries {
        assert_eq!(path.rule().hold, HoldKind::EnrollHold, "{path}");
    }
    // Escrow is opened on two paths only.
    let escrow: Vec<Path> = Path::ALL.into_iter().filter(|p| p.is_escrow()).collect();
    assert_eq!(escrow, vec![Path::EscrowRecovery, Path::EscrowOpen]);
    for path in Path::ALL {
        assert_eq!(
            path.rule()
                .evidence
                .contains(&EvidenceKind::EscrowThreshold),
            path.is_escrow(),
            "{path}"
        );
    }
    // Every path but the escrow opening enrolls a device for the person.
    for path in Path::ALL {
        assert_eq!(path.enrolls_a_device(), path != Path::EscrowOpen);
    }
    // Nobody opens a path that no row lists, and every path has at least one opener and at least
    // one required piece of evidence (a path with none would release on the hold alone).
    for (path, rule) in PATHS {
        assert!(!rule.openers.is_empty(), "{path}");
        assert!(!rule.evidence.is_empty(), "{path}");
        assert!(
            !rule.evidence.contains(&EvidenceKind::DeviceEvidence),
            "{path}: device evidence is optional, never required"
        );
        // Only the table's admins open an admin path.
        if rule.openers.contains(&OpenerKind::Admins) {
            assert_eq!(rule.openers, &[OpenerKind::Admins], "{path}");
        }
    }
    // No path lets helpdesk be the only way in: it opens one recovery request and nothing else.
    for (path, rule) in PATHS {
        assert_eq!(
            rule.openers.contains(&OpenerKind::Helpdesk),
            path == Path::EscrowRecovery,
            "{path}"
        );
        assert_ne!(rule.openers, &[OpenerKind::Helpdesk], "{path}");
    }
}
