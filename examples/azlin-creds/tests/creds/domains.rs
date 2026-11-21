//! S9E-08 `domain_separation` [M] (planning/architecture.md section 2.5: "each with its own domain
//! string"), the credential half, and the pinning of the signing preimage (S9E-14, the preimage half).

use azlin_creds::{
    signing_preimage, AssertionClaims, Canonical, Certificate, CredentialClaims, Signer,
    ALL_DOMAINS, CREDENTIAL_CERT_V1, CREDENTIAL_V2, ENTITLEMENT_V2, MEMBER_ASSERT_V1,
};
use azlin_keys::sign::{signing_input, verify};
use azlin_regbase::Domain;

use crate::support::*;

#[test]
fn s9e_08_the_four_domains_are_distinct_well_formed_and_listed_literally() {
    let literal = [
        "azlin-credential-v2",
        "azlin-member-assert-v1",
        "azlin-entitlement-v2",
        "azlin-credential-cert-v1",
    ];
    let listed: Vec<&str> = ALL_DOMAINS.iter().map(|d| d.as_str()).collect();
    assert_eq!(listed, literal, "the list, in order");
    for domain in ALL_DOMAINS {
        assert!(domain.is_wellformed(), "{domain:?}");
    }
    for (i, a) in ALL_DOMAINS.iter().enumerate() {
        for b in &ALL_DOMAINS[i + 1..] {
            assert_ne!(a, b, "two structures share a domain");
        }
    }
    assert_eq!(CREDENTIAL_V2.as_str(), "azlin-credential-v2");
    assert_eq!(MEMBER_ASSERT_V1.as_str(), "azlin-member-assert-v1");
    assert_eq!(ENTITLEMENT_V2.as_str(), "azlin-entitlement-v2");
    assert_eq!(CREDENTIAL_CERT_V1.as_str(), "azlin-credential-cert-v1");
}

#[test]
fn s9e_08_each_structure_is_signed_under_its_own_domain_and_no_other() {
    let (_f, credential) = issue_fixture();
    let cert: &Certificate = credential.certificate();
    let body = cert.canonical_bytes().expect("the body");
    let payload = credential.payload();
    for domain in ALL_DOMAINS {
        // The certificate's signature verifies under the certificate's domain only.
        let cert_ok = verify(domain, &body, &cert.identity_pub, &cert.identity_sig).is_ok();
        assert_eq!(
            cert_ok,
            *domain == CREDENTIAL_CERT_V1,
            "certificate under {domain:?}"
        );
        // The credential's signature under the credential's domain only.
        let cred_ok = verify(domain, payload, &cert.cred_pub, credential.signature()).is_ok();
        assert_eq!(
            cred_ok,
            *domain == CREDENTIAL_V2,
            "credential under {domain:?}"
        );
    }
    // Re-labelling: the credential's signed bytes offered as a certificate body, an assertion or an
    // entitlement under the credential's own signature: nothing verifies.
    for other in [CREDENTIAL_CERT_V1, MEMBER_ASSERT_V1, ENTITLEMENT_V2] {
        assert!(verify(&other, payload, &cert.cred_pub, credential.signature()).is_err());
    }
    // The associated constants name the same domains.
    assert_eq!(<CredentialClaims as Canonical>::DOMAIN, CREDENTIAL_V2);
    assert_eq!(<AssertionClaims as Canonical>::DOMAIN, MEMBER_ASSERT_V1);
    assert_eq!(<Certificate as Canonical>::DOMAIN, CREDENTIAL_CERT_V1);
}

#[test]
fn s9e_08_one_signature_does_not_serve_two_domains_whatever_the_bytes() {
    // The preimages of two different domains are never equal, whatever the messages (the NUL ends the
    // domain, and a domain holds no NUL).
    let messages: [&[u8]; 4] = [b"", b"\0", b"abc", b"-v2\0x"];
    for a in ALL_DOMAINS {
        for b in ALL_DOMAINS {
            if a == b {
                continue;
            }
            for m1 in messages {
                for m2 in messages {
                    assert_ne!(signing_input(a, m1), signing_input(b, m2));
                }
            }
        }
    }
    // A key signs under a domain it has no business with, and the other domains refuse it.
    let s = signer(6);
    let weird: Domain = Domain::new("azlin-credential-v2x");
    let sig = s.sign(&weird, b"payload").expect("signs");
    for d in ALL_DOMAINS {
        assert!(verify(d, b"payload", &s.public(), &sig).is_err());
    }
}

#[test]
fn s9e_14_the_preimage_is_the_domain_a_nul_and_the_canonical_bytes() {
    let claims = claims_of(grant());
    let payload = claims.canonical_bytes().expect("canonical bytes");
    let preimage = signing_preimage(&claims).expect("a preimage");
    assert!(preimage.starts_with(b"azlin-credential-v2\0"), "the prefix");
    // "azlin-credential-v2" is 19 bytes, then the NUL: no length byte, no other prefix.
    assert_eq!(preimage.len(), 19 + 1 + payload.len());
    assert_eq!(&preimage[20..], payload.as_slice());
    assert_eq!(preimage, signing_input(&CREDENTIAL_V2, &payload));

    let f = fixture();
    let body = f.cert.canonical_bytes().expect("body");
    let cert_preimage = signing_preimage(&f.cert).expect("a preimage");
    assert!(cert_preimage.starts_with(b"azlin-credential-cert-v1\0"));
    assert_eq!(&cert_preimage[25..], body.as_slice());

    // The canonical bytes are a CBOR map: the encoder is deterministic (the same claims, the same bytes).
    assert_eq!(claims.canonical_bytes().expect("again"), payload);
    assert_eq!(
        payload.first(),
        Some(&0xa9),
        "a map of nine entries: keys 1 to 5 and 7 to 10"
    );
}
