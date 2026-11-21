//! S9E-10 `the_issuer_probes_before_it_issues` [M] (planning/architecture.md section 1.4 rule 9) for the
//! credential issuer, and the behaviour of the signer seam: a holder that refuses, a holder that
//! returns a signature that does not verify, the domain that must be well formed.

use azlin_creds::{
    certify, CertError, CredError, CredentialIssuer, IssueError, IssuerError, ProbeFailure,
    SeedSigner, SignError, Signer, MAX_CERT_SPAN,
};
use azlin_keys::sign::verify;
use azlin_keys::Secret;
use azlin_regbase::{AnchorSet, Domain, PublicKey32, Signature64};

use crate::support::*;

fn probe_error(
    signer: &dyn Signer,
    cert: azlin_creds::Certificate,
    anchors: &AnchorSet,
    secs: u64,
) -> Option<IssuerError> {
    CredentialIssuer::new(signer, cert, anchors, at(secs)).err()
}

fn failed(e: CredError) -> Option<IssuerError> {
    Some(IssuerError::ProbeFailed(ProbeFailure::Credential(e)))
}

#[test]
fn s9e_10_an_issuer_exists_only_after_the_probe_verified() {
    let f = fixture();
    let issuer =
        CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED)).expect("the probe verifies");
    assert_eq!(issuer.key_epoch(), EPOCH);
    assert_eq!(issuer.certificate(), &f.cert);
}

#[test]
fn s9e_10_a_certificate_that_does_not_verify_fails_the_probe() {
    let f = fixture();
    // Signed by an identity key the anchors do not know.
    let foreign = signer(9);
    let cert = signed_cert(
        &foreign,
        GROUP,
        EPOCH,
        f.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    assert_eq!(
        probe_error(&f.cred, cert, &f.anchors, ISSUED),
        failed(CredError::UnknownIdentity)
    );
    // A certificate whose signature was tampered with.
    let mut tampered = f.cert;
    let mut bytes = *tampered.identity_sig.as_bytes();
    bytes[7] ^= 0x80;
    tampered.identity_sig = Signature64::from_bytes(bytes);
    assert_eq!(
        probe_error(&f.cred, tampered, &f.anchors, ISSUED),
        failed(CredError::BadCert)
    );
}

#[test]
fn s9e_10_a_window_that_does_not_cover_the_moment_fails_the_probe() {
    let f = fixture();
    assert_eq!(
        probe_error(&f.cred, f.cert, &f.anchors, CERT_FROM - 1),
        failed(CredError::CertWindow),
        "before the window"
    );
    assert_eq!(
        probe_error(&f.cred, f.cert, &f.anchors, CERT_FROM + CERT_SPAN),
        failed(CredError::CertWindow),
        "at the window's end"
    );
    // The last second that can hold a one-second probe is the second before the end.
    assert!(
        CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(CERT_FROM + CERT_SPAN - 1)).is_ok()
    );
    assert!(CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(CERT_FROM)).is_ok());
}

#[test]
fn s9e_10_a_signer_that_is_not_the_certified_key_fails_the_probe() {
    let f = fixture();
    let impostor = signer(77);
    assert_eq!(
        probe_error(&impostor, f.cert, &f.anchors, ISSUED),
        failed(CredError::BadSignature)
    );
}

#[test]
fn s9e_10_anchors_that_do_not_know_the_identity_key_fail_the_probe() {
    let f = fixture();
    assert_eq!(
        probe_error(&f.cred, f.cert, &AnchorSet::empty(), ISSUED),
        failed(CredError::UnknownIdentity)
    );
    let other_group = fixture_anchors(azlin_regbase::MetaGroupId::new(2), 1, &[(0, None)]);
    assert_eq!(
        probe_error(&f.cred, f.cert, &other_group, ISSUED),
        failed(CredError::UnknownIdentity)
    );
    let retired = fixture_anchors(GROUP, 1, &[(0, Some(ISSUED))]);
    assert_eq!(
        probe_error(&f.cred, f.cert, &retired, ISSUED),
        failed(CredError::IdentityWindow)
    );
}

/// A holder that refuses.
struct Refusing;

impl Signer for Refusing {
    fn public(&self) -> PublicKey32 {
        signer(2).public()
    }

    fn sign(&self, _: &Domain, _: &[u8]) -> Result<Signature64, SignError> {
        Err(SignError::Refused)
    }
}

/// A holder whose module returns a signature of something else.
struct Garbage(SeedSigner);

impl Signer for Garbage {
    fn public(&self) -> PublicKey32 {
        self.0.public()
    }

    fn sign(&self, domain: &Domain, _: &[u8]) -> Result<Signature64, SignError> {
        self.0.sign(domain, b"not what was asked")
    }
}

#[test]
fn s9e_10_a_holder_that_refuses_or_lies_fails_the_probe() {
    let f = fixture();
    assert_eq!(
        probe_error(&Refusing, f.cert, &f.anchors, ISSUED),
        Some(IssuerError::ProbeFailed(ProbeFailure::Sign(
            SignError::Refused
        )))
    );
    assert_eq!(
        probe_error(&Garbage(signer(2)), f.cert, &f.anchors, ISSUED),
        Some(IssuerError::ProbeFailed(ProbeFailure::SignerFault))
    );
}

#[test]
fn s9e_10_certify_checks_the_holders_signature_against_its_own_key() {
    let f = fixture();
    let made = certify(
        &Garbage(signer(1)),
        GROUP,
        EPOCH,
        f.cred.public(),
        at(CERT_FROM),
        at(CERT_FROM + 1_000),
    );
    assert_eq!(made.err(), Some(CertError::SignerFault));
    let made = certify(
        &Refusing,
        GROUP,
        EPOCH,
        f.cred.public(),
        at(CERT_FROM),
        at(CERT_FROM + 1_000),
    );
    assert_eq!(made.err(), Some(CertError::Sign(SignError::Refused)));
    // The window of 45 days exactly is made; one second more is not.
    assert!(certify(
        &f.identity,
        GROUP,
        EPOCH,
        f.cred.public(),
        at(CERT_FROM),
        at(CERT_FROM + MAX_CERT_SPAN.as_secs())
    )
    .is_ok());
    assert_eq!(
        certify(
            &f.identity,
            GROUP,
            EPOCH,
            f.cred.public(),
            at(CERT_FROM),
            at(CERT_FROM + MAX_CERT_SPAN.as_secs() + 1)
        )
        .err(),
        Some(CertError::Malformed)
    );
}

#[test]
fn s9e_10_the_issuer_refuses_a_signer_that_goes_bad_after_the_probe() {
    // A holder that signs the probe and then lies: `issue` checks each signature against the holder's
    // own public key, so nothing that does not verify leaves.
    use std::cell::Cell;
    struct Flaky {
        inner: SeedSigner,
        calls: Cell<u32>,
    }
    impl Signer for Flaky {
        fn public(&self) -> PublicKey32 {
            self.inner.public()
        }
        fn sign(&self, domain: &Domain, message: &[u8]) -> Result<Signature64, SignError> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() == 1 {
                self.inner.sign(domain, message)
            } else {
                self.inner.sign(domain, b"garbage")
            }
        }
    }
    let f = fixture();
    let flaky = Flaky {
        inner: signer(2),
        calls: Cell::new(0),
    };
    let issuer = CredentialIssuer::new(&flaky, f.cert, &f.anchors, at(ISSUED)).expect("the probe");
    assert_eq!(issuer.issue(grant()).err(), Some(IssueError::SignerFault));
}

#[test]
fn s9e_10_a_seed_signer_signs_under_well_formed_domains_only() {
    let s = signer(4);
    let message = b"a message";
    // A real domain: the signature is Ed25519 over `domain || 0x00 || message`.
    let sig = s.sign(&azlin_creds::CREDENTIAL_V2, message).expect("signs");
    assert!(verify(&azlin_creds::CREDENTIAL_V2, message, &s.public(), &sig).is_ok());
    assert!(
        verify(&azlin_creds::MEMBER_ASSERT_V1, message, &s.public(), &sig).is_err(),
        "another domain does not verify"
    );
    // A domain with a NUL byte would make two domains give equal inputs: refused.
    let long = "x".repeat(65);
    for bad in [
        "",
        "has\0nul",
        "has space",
        "tab\t",
        "non-ascii-\u{e9}",
        long.as_str(),
    ] {
        let domain: Domain = Domain::new(Box::leak(bad.to_string().into_boxed_str()));
        assert_eq!(
            s.sign(&domain, message),
            Err(SignError::BadDomain),
            "{bad:?}"
        );
    }
}

#[test]
fn s9e_10_a_generated_signer_needs_entropy() {
    use azlin_keys::{Entropy, EntropyError};
    struct Counting(u8);
    impl Entropy for Counting {
        fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
            for byte in out.iter_mut() {
                self.0 = self.0.wrapping_add(1);
                *byte = self.0;
            }
            Ok(())
        }
    }
    struct Failing;
    impl Entropy for Failing {
        fn fill(&mut self, _: &mut [u8]) -> Result<(), EntropyError> {
            Err(EntropyError::Exhausted)
        }
    }
    let a = SeedSigner::generate(&mut Counting(0)).expect("a key");
    let b = SeedSigner::from_seed(&Secret::from_bytes(
        <[u8; 32]>::try_from((1..=32u8).collect::<Vec<u8>>().as_slice()).expect("32 bytes"),
    ));
    assert_eq!(a.public(), b.public(), "the seed is the 32 bytes drawn");
    assert!(
        SeedSigner::generate(&mut Failing).is_err(),
        "no key from no entropy"
    );
}
