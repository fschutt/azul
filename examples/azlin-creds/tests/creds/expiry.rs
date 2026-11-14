//! S9E-04 `expiry_and_skew`, S9E-05 `an_untrusted_clock_decides_nothing` and S9E-21
//! `the_key_epoch_binds_credential_and_certificate`.

use azlin_creds::{
    verify_credential, verify_credential_bytes, CredError, CredentialClaims, IssueError,
    VerifierNow, VerifyRules,
};
use azlin_regbase::KeyEpoch;

use crate::support::*;

const I: u64 = ISSUED;
const E: u64 = ISSUED + LIFETIME;

fn claims(issued: u64, expires: u64) -> CredentialClaims {
    let mut c = claims_of(grant());
    c.issued_at = at(issued);
    c.expires_at = at(expires);
    c
}

fn check(c: CredentialClaims, secs: u64) -> Result<(), CredError> {
    let f = fixture();
    let credential = signed_credential(&f.cred, f.cert, c);
    verify_credential(&credential, &f.anchors, now(secs), &f.rules).map(|_| ())
}

#[test]
fn s9e_04_the_skew_applies_before_the_issue_and_never_after_the_expiry() {
    // skew 30 s.
    assert_eq!(check(claims(I, E), I - 31), Err(CredError::NotYetValid));
    assert_eq!(
        check(claims(I, E), I - 30),
        Ok(()),
        "30 s before is inside the skew"
    );
    assert_eq!(check(claims(I, E), I - 1), Ok(()));
    assert_eq!(check(claims(I, E), I), Ok(()));
    assert_eq!(check(claims(I, E), E - 1), Ok(()), "the last valid second");
    assert_eq!(
        check(claims(I, E), E),
        Err(CredError::Expired),
        "the expiry second belongs to the expired side"
    );
    assert_eq!(check(claims(I, E), E + 1), Err(CredError::Expired));
    assert_eq!(check(claims(I, E), E + 86_400), Err(CredError::Expired));
}

#[test]
fn s9e_04_a_credential_that_expires_before_it_is_issued_is_malformed() {
    assert_eq!(check(claims(I, I), I), Err(CredError::Malformed));
    assert_eq!(check(claims(I, I - 1), I), Err(CredError::Malformed));
    assert_eq!(
        check(claims(I, I + 1), I),
        Ok(()),
        "one second is a lifetime"
    );
}

#[test]
fn s9e_04_the_lifetime_is_capped_at_twelve_hours() {
    let twelve_hours = 12 * 3_600;
    assert_eq!(check(claims(I, I + twelve_hours), I), Ok(()));
    assert_eq!(
        check(claims(I, I + twelve_hours + 1), I),
        Err(CredError::LifetimeTooLong)
    );
    // The cap is the rules', not a constant of the verifier.
    let f = fixture();
    let credential = signed_credential(&f.cred, f.cert, claims(I, I + 7_200));
    let strict = VerifyRules {
        max_credential_lifetime: span(7_199),
        ..VerifyRules::default()
    };
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(I), &strict).err(),
        Some(CredError::LifetimeTooLong)
    );
    let exact = VerifyRules {
        max_credential_lifetime: span(7_200),
        ..VerifyRules::default()
    };
    assert!(verify_credential(&credential, &f.anchors, now(I), &exact).is_ok());
}

#[test]
fn s9e_04_the_skew_is_the_rules() {
    let f = fixture();
    let credential = signed_credential(&f.cred, f.cert, claims(I, E));
    let wide = VerifyRules {
        skew: span(120),
        ..VerifyRules::default()
    };
    assert!(verify_credential(&credential, &f.anchors, now(I - 120), &wide).is_ok());
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(I - 121), &wide).err(),
        Some(CredError::NotYetValid)
    );
    let none = VerifyRules {
        skew: span(0),
        ..VerifyRules::default()
    };
    assert!(verify_credential(&credential, &f.anchors, now(I), &none).is_ok());
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(I - 1), &none).err(),
        Some(CredError::NotYetValid)
    );
}

#[test]
fn s9e_04_the_issuer_refuses_what_the_verifier_would() {
    let f = fixture();
    let issuer =
        azlin_creds::CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(I)).expect("an issuer");
    let mut g = grant();
    g.expires_at = at(I);
    assert_eq!(
        issuer.issue(g.clone()).err(),
        Some(IssueError::Claims(CredError::Malformed))
    );
    g.expires_at = at(I + 12 * 3_600 + 1);
    assert_eq!(
        issuer.issue(g).err(),
        Some(IssueError::Claims(CredError::LifetimeTooLong))
    );
    // A credential that would outlive its certificate, or start before it, is refused too.
    let mut g = grant();
    g.expires_at = at(CERT_FROM + CERT_SPAN + 1);
    g.issued_at = at(CERT_FROM + CERT_SPAN - 60);
    assert_eq!(issuer.issue(g).err(), Some(IssueError::OutsideCertificate));
    let mut g = grant();
    g.issued_at = at(CERT_FROM - 1);
    g.expires_at = at(CERT_FROM + 60);
    assert_eq!(issuer.issue(g).err(), Some(IssueError::OutsideCertificate));
    // And one that fits exactly is issued.
    let mut g = grant();
    g.issued_at = at(CERT_FROM + CERT_SPAN - 60);
    g.expires_at = at(CERT_FROM + CERT_SPAN);
    assert!(issuer.issue(g).is_ok());
}

#[test]
fn s9e_05_an_untrusted_clock_decides_nothing() {
    let (f, credential) = issue_fixture();
    let bytes = credential.encode().expect("encodes");
    for secs in [0, I - 1_000, I, E - 1, E, E + 1_000, u64::MAX] {
        let untrusted = VerifierNow::untrusted(node(secs));
        let refused = verify_credential(&credential, &f.anchors, untrusted, &f.rules).err();
        assert_eq!(
            refused,
            Some(CredError::ClockUntrusted),
            "node reads {secs}"
        );
        assert_ne!(refused, Some(CredError::Expired));
        assert_ne!(refused, Some(CredError::NotYetValid));
        assert_eq!(
            verify_credential_bytes(&bytes, &f.anchors, untrusted, &f.rules).err(),
            Some(CredError::ClockUntrusted)
        );
    }
    // Even bytes that do not parse are refused for the clock first: a node with a bad clock says so.
    assert_eq!(
        verify_credential_bytes(
            b"\xff\xff",
            &f.anchors,
            VerifierNow::untrusted(node(I)),
            &f.rules
        )
        .err(),
        Some(CredError::ClockUntrusted)
    );
    // The same credential with a trusted clock is fine at the same instants.
    assert!(verify_credential(&credential, &f.anchors, now(I), &f.rules).is_ok());
}

#[test]
fn s9e_21_the_key_epoch_binds_the_credential_to_its_certificate() {
    let f = fixture();
    for delta in [-1i64, 1, 5] {
        let mut c = claims(I, E);
        let epoch = u32::try_from(i64::from(EPOCH.get()) + delta).expect("an epoch");
        c.key_epoch = KeyEpoch::new(epoch);
        assert_eq!(check(c, I), Err(CredError::CertMismatch), "delta {delta}");
    }
    assert_eq!(check(claims(I, E), I), Ok(()));
    // The issuer stamps the epoch from its certificate: the grant has no field to pass another.
    let (_, credential) = issue_fixture();
    assert_eq!(credential.claims().key_epoch, f.cert.epoch);
    assert_eq!(credential.certificate().epoch, f.cert.epoch);
}
