//! S9E-03 `the_chain_is_checked_link_by_link` [M] (planning/architecture.md section 2.4) and S9E-16
//! `anchors_windows`: the certificate must be signed by an identity key OF ITS GROUP, whose window
//! covers it; the credential must be signed by the key the certificate names; the certificate's window
//! must cover the credential. An empty anchor set refuses everything.

use azlin_creds::{
    verify_cert, verify_credential, CertError, CredError, Signer, VerifyRules, MAX_CERT_SPAN,
};
use azlin_regbase::{AnchorSet, KeyWindow, MetaGroupId};

use crate::support::*;

const DAY: u64 = 86_400;

fn verdict(
    anchors: &AnchorSet,
    credential: &azlin_creds::Credential,
    secs: u64,
) -> Result<(), CredError> {
    verify_credential(credential, anchors, now(secs), &VerifyRules::default()).map(|_| ())
}

fn claims_at(issued: u64, lifetime: u64) -> azlin_creds::CredentialClaims {
    let mut claims = claims_of(grant());
    claims.issued_at = at(issued);
    claims.expires_at = at(issued + lifetime);
    claims
}

#[test]
fn s9e_03_a_certificate_signed_by_no_identity_key_of_the_group_is_refused() {
    let f = fixture();
    let foreign = signer(9);
    let cert = signed_cert(
        &foreign,
        GROUP,
        EPOCH,
        f.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    let credential = signed_credential(&f.cred, cert, claims_of(grant()));
    assert_eq!(
        verdict(&f.anchors, &credential, ISSUED),
        Err(CredError::UnknownIdentity)
    );
    assert_eq!(
        verify_cert(&cert, &f.anchors),
        Err(CertError::UnknownIdentity)
    );
}

#[test]
fn s9e_03_an_identity_key_of_another_group_is_no_identity_key_of_this_one() {
    // Group 1 has key K1; group 2 has key K2. A certificate for group 1 signed by K2 must be refused:
    // a voter of group 2 holds a perfectly good identity key, and it is not group 1's.
    let f = fixture();
    let group2 = MetaGroupId::new(2);
    let k2 = signer(3);
    let mut anchors = fixture_anchors(GROUP, 1, &[(0, None)]);
    anchors.identity.push((
        group2,
        KeyWindow {
            public: azlin_creds::Signer::public(&k2),
            not_before: at(0),
            not_after: None,
            revoked_from: None,
        },
    ));
    let cert = signed_cert(
        &k2,
        GROUP,
        EPOCH,
        f.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    let credential = signed_credential(&f.cred, cert, claims_of(grant()));
    assert_eq!(
        verdict(&anchors, &credential, ISSUED),
        Err(CredError::UnknownIdentity)
    );
    assert_eq!(
        verify_cert(&cert, &anchors),
        Err(CertError::UnknownIdentity)
    );
    // The same certificate under the group it names is fine.
    let cert2 = signed_cert(
        &k2,
        group2,
        EPOCH,
        f.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    let credential2 = signed_credential(&f.cred, cert2, claims_of(grant()));
    assert_eq!(verdict(&anchors, &credential2, ISSUED), Ok(()));
}

#[test]
fn s9e_03_a_certificate_for_group_1_presented_with_the_identity_key_of_group_2_only() {
    let f = fixture();
    let anchors = fixture_anchors(MetaGroupId::new(2), 1, &[(0, None)]);
    let credential = signed_credential(&f.cred, f.cert, claims_of(grant()));
    assert_eq!(
        verdict(&anchors, &credential, ISSUED),
        Err(CredError::UnknownIdentity)
    );
}

#[test]
fn s9e_03_a_certificate_for_another_credential_key_does_not_vouch_for_this_signature() {
    let f = fixture();
    let other = signer(5);
    let cert_for_other = azlin_creds::certify(
        &f.identity,
        GROUP,
        EPOCH,
        azlin_creds::Signer::public(&other),
        at(CERT_FROM),
        at(CERT_FROM + CERT_SPAN),
    )
    .expect("a certificate");
    // The claims are signed by `f.cred`, the certificate names `other`.
    let credential = signed_credential(&f.cred, cert_for_other, claims_of(grant()));
    assert_eq!(
        verdict(&f.anchors, &credential, ISSUED),
        Err(CredError::BadSignature)
    );
}

#[test]
fn s9e_03_the_certificates_window_must_cover_the_issue_and_the_moment() {
    let f = fixture();
    let (from, until) = (1_000u64, 5_000u64);
    let cert = azlin_creds::certify(
        &f.identity,
        GROUP,
        EPOCH,
        azlin_creds::Signer::public(&f.cred),
        at(from),
        at(until),
    )
    .expect("a certificate");
    // issued_at at each boundary second; the node's now is the issue second.
    for (issued, expected) in [
        (from - 1, Err(CredError::CertWindow)),
        (from, Ok(())),
        (from + 1, Ok(())),
        (until - 1, Ok(())),
        (until, Err(CredError::CertWindow)),
        (until + 1, Err(CredError::CertWindow)),
    ] {
        let credential = signed_credential(&f.cred, cert, claims_at(issued, 60));
        assert_eq!(
            verdict(&f.anchors, &credential, issued),
            expected,
            "issued_at {issued}"
        );
    }
    // A credential issued inside the window is refused once the node's now reaches the window's end,
    // even while the credential itself has not expired.
    let credential = signed_credential(&f.cred, cert, claims_at(until - 10, 60));
    assert_eq!(verdict(&f.anchors, &credential, until - 1), Ok(()));
    assert_eq!(
        verdict(&f.anchors, &credential, until),
        Err(CredError::CertWindow)
    );
    assert_eq!(
        verdict(&f.anchors, &credential, until + 20),
        Err(CredError::CertWindow)
    );
}

#[test]
fn s9e_03_the_identity_keys_window_must_cover_the_certificate() {
    // The brief's rule, as `verify_cert` states it: the identity key valid at the certificate's
    // `not_before`. Boundary seconds each way.
    let f = fixture();
    let anchors = fixture_anchors(GROUP, 1, &[(1_000, Some(3_000))]);
    for (not_before, expected) in [
        (999u64, Err(CertError::IdentityWindow)),
        (1_000, Ok(())),
        (1_001, Ok(())),
        (2_999, Ok(())),
        (3_000, Err(CertError::IdentityWindow)),
        (3_001, Err(CertError::IdentityWindow)),
    ] {
        let cert = signed_cert(
            &f.identity,
            GROUP,
            EPOCH,
            f.cred.public(),
            not_before,
            not_before + 10_000,
        );
        assert_eq!(
            verify_cert(&cert, &anchors),
            expected,
            "not_before {not_before}"
        );
    }
}

#[test]
fn s9e_03_a_retired_identity_key_stops_vouching_at_once() {
    // DRIFT X-DELTA-W1S9E-4: a credential's chain is checked at the node's now too, so a certificate that
    // a retired (or stolen) identity key made, back-dated into its window, does not verify after the
    // key's window closed.
    let f = fixture();
    let anchors = fixture_anchors(GROUP, 1, &[(0, Some(ISSUED + 100))]);
    let credential = signed_credential(&f.cred, f.cert, claims_of(grant()));
    assert_eq!(verdict(&anchors, &credential, ISSUED), Ok(()));
    assert_eq!(verdict(&anchors, &credential, ISSUED + 99), Ok(()));
    assert_eq!(
        verdict(&anchors, &credential, ISSUED + 100),
        Err(CredError::IdentityWindow)
    );
}

#[test]
fn s9e_03_a_window_that_is_not_sane_is_malformed() {
    let f = fixture();
    let nb = 10_000u64;
    let max = MAX_CERT_SPAN.as_secs();
    assert_eq!(max, 45 * DAY, "45 days");
    for (not_after, sane) in [
        (nb + max, true),
        (nb + max + 1, false),
        (nb + 1, true),
        (nb, false),
        (nb - 1, false),
    ] {
        let cert = signed_cert(&f.identity, GROUP, EPOCH, f.cred.public(), nb, not_after);
        let expected = if sane {
            Ok(())
        } else {
            Err(CertError::Malformed)
        };
        assert_eq!(
            verify_cert(&cert, &f.anchors),
            expected,
            "not_after {not_after}"
        );
        // `certify` refuses to make it, as the verifier refuses to accept it.
        let made = azlin_creds::certify(
            &f.identity,
            GROUP,
            EPOCH,
            azlin_creds::Signer::public(&f.cred),
            at(nb),
            at(not_after),
        );
        assert_eq!(made.is_ok(), sane, "certify, not_after {not_after}");
        if !sane {
            assert_eq!(made.err(), Some(CertError::Malformed));
            let credential = signed_credential(&f.cred, cert, claims_at(nb, 60));
            assert_eq!(
                verdict(&f.anchors, &credential, nb),
                Err(CredError::CertMalformed)
            );
        }
    }
}

#[test]
fn s9e_16_identity_windows_overlap_during_a_rotation() {
    // K1 until 200, K2 from 100: both certify inside the overlap, one outside.
    let f = fixture();
    let k1 = signer(1);
    let k2 = signer(3);
    let mut anchors = fixture_anchors(GROUP, 1, &[(0, Some(200))]);
    anchors.identity.push((
        GROUP,
        KeyWindow {
            public: azlin_creds::Signer::public(&k2),
            not_before: at(100),
            not_after: None,
            revoked_from: None,
        },
    ));
    let cert_by = |who: &azlin_creds::SeedSigner, not_before: u64| {
        signed_cert(
            who,
            GROUP,
            EPOCH,
            f.cred.public(),
            not_before,
            not_before + 1_000,
        )
    };
    for (not_before, k1_ok, k2_ok) in [
        (50u64, true, false),
        (99, true, false),
        (100, true, true),
        (199, true, true),
        (200, false, true),
        (250, false, true),
    ] {
        let by_k1 = verify_cert(&cert_by(&k1, not_before), &anchors).is_ok();
        let by_k2 = verify_cert(&cert_by(&k2, not_before), &anchors).is_ok();
        assert_eq!((by_k1, by_k2), (k1_ok, k2_ok), "not_before {not_before}");
    }
}

#[test]
fn s9e_16_an_empty_anchor_set_refuses_everything() {
    let (f, credential) = issue_fixture();
    let empty = AnchorSet::empty();
    assert_eq!(
        verify_credential(&credential, &empty, now(ISSUED), &f.rules).err(),
        Some(CredError::UnknownIdentity)
    );
    assert_eq!(
        verify_cert(&f.cert, &empty),
        Err(CertError::UnknownIdentity)
    );
    // An anchor set of other lists only is as good as empty for identity keys.
    let mut payments_only = AnchorSet::empty();
    payments_only.payments.push(KeyWindow {
        public: azlin_creds::Signer::public(&f.identity),
        not_before: at(0),
        not_after: None,
        revoked_from: None,
    });
    assert_eq!(
        verify_cert(&f.cert, &payments_only),
        Err(CertError::UnknownIdentity),
        "a payments key is no identity key"
    );
}
