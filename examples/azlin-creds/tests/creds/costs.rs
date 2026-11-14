//! S9E-22 `costs` (planning/architecture.md section 1.4 rule 4): a node can check every connection, so
//! verifying one credential costs two signature verifications and nothing else that grows; S9E-13
//! `verification_does_not_consult_revocations`; S9E-12 `secrets_never_print` (QUALITY rule 26).

use std::cell::RefCell;

use azlin_creds::{
    verify_credential, verify_credential_with, AccountGroups, AssertError, CanonicalError,
    CertError, CredError, Credential, CredentialClaims, EntitlementError, IssueError, IssuerError,
    ProbeFailure, SignError, VerifiedCredential, VerifierNow, VerifyRules, WireError,
};
use azlin_regbase::AnchorSet;
use azlin_seams::{Ed25519Verifier, SigAlg, SignatureVerifier, VerifyError};

use crate::support::*;

/// A verifier that counts, and remembers the first bytes of each preimage.
struct Counting {
    preimages: RefCell<Vec<Vec<u8>>>,
}

impl SignatureVerifier for Counting {
    fn verify(
        &self,
        alg: SigAlg,
        public_key: &[u8],
        preimage: &[u8],
        sig: &[u8],
    ) -> Result<(), VerifyError> {
        self.preimages
            .borrow_mut()
            .push(preimage.iter().copied().take(40).collect());
        Ed25519Verifier.verify(alg, public_key, preimage, sig)
    }
}

#[test]
fn s9e_22_verifying_one_credential_costs_two_signature_checks() {
    let (f, credential) = issue_fixture();
    let counting = Counting {
        preimages: RefCell::new(Vec::new()),
    };
    let verified = verify_credential_with(
        &counting,
        &credential,
        &f.anchors,
        &placement(),
        now(ISSUED),
        &f.rules,
    )
    .expect("verifies");
    assert_eq!(verified.account(), grant().account);
    let seen = counting.preimages.borrow();
    assert_eq!(
        seen.len(),
        2,
        "the certificate and the credential, nothing more"
    );
    assert!(
        seen[0].starts_with(b"azlin-credential-cert-v1\0"),
        "the certificate first, under its own domain"
    );
    assert!(
        seen[1].starts_with(b"azlin-credential-v2\0"),
        "then the credential, under its own domain"
    );
}

#[test]
fn s9e_22_a_refusal_before_the_chain_costs_no_signature_check() {
    // Every refusal that needs no key is made before any signature is verified: a flood of expired or
    // malformed credentials costs a node no more than reading them.
    let (f, credential) = issue_fixture();
    let counting = Counting {
        preimages: RefCell::new(Vec::new()),
    };
    for (secs, trusted) in [
        (ISSUED + 100_000, true),
        (ISSUED, false),
        (ISSUED - 1_000, true),
    ] {
        let now = if trusted {
            VerifierNow::trusted(node(secs))
        } else {
            VerifierNow::untrusted(node(secs))
        };
        assert!(verify_credential_with(
            &counting,
            &credential,
            &f.anchors,
            &placement(),
            now,
            &f.rules
        )
        .is_err());
    }
    // An empty anchor set is refused without a check too: no key, no verification.
    assert!(verify_credential_with(
        &counting,
        &credential,
        &AnchorSet::empty(),
        &placement(),
        now(ISSUED),
        &f.rules
    )
    .is_err());
    assert_eq!(counting.preimages.borrow().len(), 0);
}

#[test]
fn s9e_13_verification_does_not_consult_revocations() {
    // The signature of the function has no revocation parameter: this does not compile if one is added.
    // The only thing it is given about the registry is where the account lives (`AccountGroups`,
    // D-W1S9E3), and that is a placement, not a revocation list.
    type VerifyCredential = fn(
        &Credential,
        &AnchorSet,
        &dyn AccountGroups,
        VerifierNow,
        &VerifyRules,
    ) -> Result<VerifiedCredential, CredError>;
    let _: VerifyCredential = verify_credential;
    // A credential whose nonce, family, device and member the registry has revoked still verifies here:
    // there is nothing in this crate to ask.
    let (f, credential) = issue_fixture();
    assert!(
        verify_credential(&credential, &f.anchors, &placement(), now(ISSUED), &f.rules).is_ok()
    );
    // And the documentation says so in its first line.
    let source = include_str!("../../src/credential.rs");
    let marker = "pub fn verify_credential(";
    let at = source
        .find(marker)
        .expect("the function is in credential.rs");
    let doc_lines: Vec<&str> = source[..at]
        .lines()
        .rev()
        .take_while(|l| l.trim_start().starts_with("///") || l.trim().is_empty())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let first = doc_lines.last().expect("a doc comment");
    assert!(
        first.contains("Does NOT consult revocations"),
        "the first line of the documentation: {first}"
    );
}

#[test]
fn s9e_12_nothing_secret_prints() {
    let s = signer(4);
    let text = format!("{s:?}");
    assert_eq!(text, "SeedSigner(<redacted>)");
    let public = format!("{}", azlin_creds::Signer::public(&s));
    assert!(!text.contains(&public[..8]), "not even the public key");

    // A credential shows its ids as four bytes and `..`, its certificate without a signature.
    let (_, credential) = issue_fixture();
    let g = grant();
    let text = format!("{credential:?}");
    for id in [
        g.account.to_string(),
        g.member.to_string(),
        g.device.expect("a device").to_string(),
        g.family.expect("a family").to_string(),
        g.nonce.to_string(),
    ] {
        assert!(!text.contains(&id), "a whole id in {text}");
        assert!(
            text.contains(&id[..8]),
            "the first four bytes of {id} in {text}"
        );
    }
    assert!(
        !text.contains(&to_hex(credential.signature().as_bytes())[..16]),
        "no signature byte"
    );
    assert!(
        !text.contains(&to_hex(credential.certificate().identity_sig.as_bytes())[..16]),
        "no signature byte of the certificate"
    );
    // The claims alone, a grant and a verified credential do the same.
    let claims: &CredentialClaims = credential.claims();
    assert!(!format!("{claims:?}").contains(&g.account.to_string()));
    assert!(!format!("{g:?}").contains(&g.member.to_string()));
    let (f, c) = issue_fixture();
    let verified =
        verify_credential(&c, &f.anchors, &placement(), now(ISSUED), &f.rules).expect("verifies");
    assert!(!format!("{verified:?}").contains(&g.account.to_string()));
}

/// Whether a text holds a run of at least 16 hex digits: the shape of a key, a signature or an id.
fn has_hex_run(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if c.is_ascii_hexdigit() {
            run += 1;
            if run >= 16 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

#[test]
fn s9e_12_no_error_text_carries_a_byte_of_a_key_or_a_signature() {
    let mut texts: Vec<String> = Vec::new();
    let mut add = |t: String| texts.push(t);
    for e in [
        CredError::ClockUntrusted,
        CredError::Malformed,
        CredError::LifetimeTooLong,
        CredError::CertMalformed,
        CredError::CertMismatch,
        CredError::NotYetValid,
        CredError::Expired,
        CredError::CertWindow,
        CredError::UnknownIdentity,
        CredError::IdentityWindow,
        CredError::BadCert,
        CredError::BadSignature,
        CredError::UnknownAccount,
        CredError::GroupMismatch,
        CredError::UnknownScope,
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        AssertError::ClockUntrusted,
        AssertError::Malformed,
        AssertError::LifetimeTooLong,
        AssertError::WrongAccount,
        AssertError::CertMalformed,
        AssertError::NotYetValid,
        AssertError::Expired,
        AssertError::CertWindow,
        AssertError::UnknownIdentity,
        AssertError::IdentityWindow,
        AssertError::BadCert,
        AssertError::BadSignature,
        AssertError::UnknownAccount,
        AssertError::GroupMismatch,
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        CertError::Malformed,
        CertError::UnknownIdentity,
        CertError::IdentityWindow,
        CertError::BadSignature,
        CertError::Sign(SignError::Refused),
        CertError::SignerFault,
        CertError::Encode(CanonicalError::TooLarge),
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        EntitlementError::UnknownKey,
        EntitlementError::KeyWindow,
        EntitlementError::NotYetIssued,
        EntitlementError::BadSignature,
        EntitlementError::Revoked,
        EntitlementError::Malformed,
        EntitlementError::SignerFault,
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        IssueError::Claims(CredError::Malformed),
        IssueError::OutsideCertificate,
        IssueError::SignerFault,
        IssueError::Sign(SignError::Unavailable),
        IssueError::Encode(CanonicalError::NotWholeDays),
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        IssuerError::ProbeFailed(ProbeFailure::Credential(CredError::BadSignature)),
        IssuerError::ProbeFailed(ProbeFailure::SignerFault),
        IssuerError::ProbeFailed(ProbeFailure::Sign(SignError::BadDomain)),
    ] {
        add(format!("{e} / {e:?}"));
    }
    for e in [
        WireError::UnknownScope,
        WireError::UnknownWord,
        WireError::OutOfRange(5),
        WireError::NotCanonical,
        WireError::TooMany,
        WireError::UnknownField(7),
        WireError::Malformed,
    ] {
        add(format!("{e} / {e:?}"));
    }
    for text in &texts {
        assert!(
            !has_hex_run(text),
            "an error text that could hold a key: {text}"
        );
        assert!(!text.contains("0x"), "{text}");
    }
    assert!(texts.len() > 40);
}
