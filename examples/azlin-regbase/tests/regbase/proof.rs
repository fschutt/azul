//! S9A-27 `key_proofs` and S9A-28 `the_signing_input_is_pinned`.

use std::collections::BTreeSet;

use azlin_keys::sign::{ENVELOPE_V2, LOG_RECORD_V1, POINTER_V2};
use azlin_keys::{Secret, SignKeypair};
use azlin_regbase::sign::{APP_REFRESH_V1, ENROLL_FINISH_V1, PROOF_DOMAINS, REFRESH_V1};
use azlin_regbase::{
    proof_message, sign_key_proof, signing_input, verify_key_proof, AccountId, Challenge, DeviceId,
    Domain, KeyProof, ProofError, ProofPurpose, PublicKey32, Signature64,
};

use crate::common::{Counting, Failing};

fn keypair(n: u8) -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([n; 32]))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn account(n: u8) -> AccountId {
    AccountId::from_bytes([n; 16])
}

fn subject(n: u8) -> [u8; 16] {
    *DeviceId::from_bytes([n; 16]).as_bytes()
}

fn challenge(n: u8) -> Challenge {
    Challenge::from_bytes([n; 16])
}

/// The proof the tests start from: purpose, account 1, subject 2, challenge 3, key 7.
fn proof_for(purpose: ProofPurpose) -> KeyProof {
    sign_key_proof(
        purpose,
        &account(1),
        &subject(2),
        &challenge(3),
        &keypair(7),
    )
}

fn verify(purpose: ProofPurpose, proof: &KeyProof) -> Result<(), ProofError> {
    verify_key_proof(
        purpose,
        &account(1),
        &subject(2),
        &keypair(7).verify_key(),
        proof,
    )
}

#[test]
fn s9a_27_a_proof_for_exactly_this_purpose_account_subject_and_challenge_verifies() {
    for purpose in ProofPurpose::ALL {
        let proof = proof_for(purpose);
        assert_eq!(proof.challenge, challenge(3));
        assert_eq!(verify(purpose, &proof), Ok(()), "{purpose:?}");
    }
}

#[test]
fn s9a_27_a_proof_for_another_purpose_account_subject_or_challenge_is_refused() {
    let key = keypair(7).verify_key();
    for purpose in ProofPurpose::ALL {
        let proof = proof_for(purpose);
        // another purpose: every other one fails
        for other in ProofPurpose::ALL {
            if other != purpose {
                assert_eq!(
                    verify(other, &proof),
                    Err(ProofError::Bad),
                    "{purpose:?} as {other:?}"
                );
            }
        }
        // another account, subject, challenge
        assert_eq!(
            verify_key_proof(purpose, &account(9), &subject(2), &key, &proof),
            Err(ProofError::Bad),
            "account"
        );
        assert_eq!(
            verify_key_proof(purpose, &account(1), &subject(9), &key, &proof),
            Err(ProofError::Bad),
            "subject"
        );
        let swapped = KeyProof {
            challenge: challenge(9),
            signature: proof.signature,
        };
        assert_eq!(verify(purpose, &swapped), Err(ProofError::Bad), "challenge");
        // the account and the subject are not interchangeable (same 16 bytes, other slot)
        assert_eq!(
            verify_key_proof(
                purpose,
                &AccountId::from_bytes(subject(2)),
                account(1).as_bytes(),
                &key,
                &proof
            ),
            Err(ProofError::Bad),
            "swapped"
        );
    }
}

#[test]
fn s9a_27_a_changed_signature_a_different_key_and_a_truncated_signature_are_refused() {
    for purpose in ProofPurpose::ALL {
        let proof = proof_for(purpose);
        // every single bit of the signature
        for byte in 0..64 {
            for bit in 0..8 {
                let mut bytes = *proof.signature.as_bytes();
                bytes[byte] ^= 1 << bit;
                let flipped = KeyProof {
                    challenge: proof.challenge,
                    signature: Signature64::from_bytes(bytes),
                };
                assert_eq!(
                    verify(purpose, &flipped),
                    Err(ProofError::Bad),
                    "byte {byte} bit {bit}"
                );
            }
        }
        // a different key
        assert_eq!(
            verify_key_proof(
                purpose,
                &account(1),
                &subject(2),
                &keypair(8).verify_key(),
                &proof
            ),
            Err(ProofError::Bad)
        );
        // an all-zero key is no key (small order / not a point): refused like any bad proof
        assert_eq!(
            verify_key_proof(
                purpose,
                &account(1),
                &subject(2),
                &PublicKey32::from_bytes([0; 32]),
                &proof
            ),
            Err(ProofError::Bad)
        );
        // an all-zero signature
        let zero = KeyProof {
            challenge: proof.challenge,
            signature: Signature64::from_bytes([0; 64]),
        };
        assert_eq!(verify(purpose, &zero), Err(ProofError::Bad));
    }
    // a truncated or padded signature never becomes a proof
    let proof = proof_for(ProofPurpose::Refresh);
    let bytes = proof.signature.as_bytes();
    for len in [0, 1, 32, 63, 65, 128] {
        let mut cut = bytes.to_vec();
        cut.resize(len, 0);
        assert_eq!(
            KeyProof::from_parts(proof.challenge, &cut),
            Err(ProofError::SignatureLength),
            "{len} bytes"
        );
    }
    // the exact 64 bytes are the proof
    assert_eq!(KeyProof::from_parts(proof.challenge, bytes), Ok(proof));
}

#[test]
fn s9a_27_the_three_domains_are_distinct_and_listed_literally() {
    assert_eq!(ProofPurpose::Refresh.domain().as_str(), "azlin-refresh-v1");
    assert_eq!(
        ProofPurpose::EnrollFinish.domain().as_str(),
        "azlin-enroll-finish-v1"
    );
    assert_eq!(
        ProofPurpose::AppRefresh.domain().as_str(),
        "azlin-app-refresh-v1"
    );
    assert_eq!(REFRESH_V1.as_str(), "azlin-refresh-v1");
    assert_eq!(ENROLL_FINISH_V1.as_str(), "azlin-enroll-finish-v1");
    assert_eq!(APP_REFRESH_V1.as_str(), "azlin-app-refresh-v1");
    let domains: BTreeSet<&str> = PROOF_DOMAINS.iter().map(Domain::as_str).collect();
    assert_eq!(domains.len(), 3);
    assert_eq!(PROOF_DOMAINS.len(), ProofPurpose::ALL.len());
    for domain in PROOF_DOMAINS {
        assert!(domain.is_wellformed(), "{domain:?}");
    }
    // and none is one of the domains azlin-keys defines
    for other in [ENVELOPE_V2, POINTER_V2, LOG_RECORD_V1] {
        assert!(!domains.contains(other.as_str()));
    }
}

#[test]
fn s9a_27_the_message_of_one_purpose_never_verifies_as_another() {
    let key = keypair(7);
    let public = key.verify_key();
    for a in ProofPurpose::ALL {
        for b in ProofPurpose::ALL {
            let ma = proof_message(a, &account(1), &subject(2), &challenge(3));
            let mb = proof_message(b, &account(1), &subject(2), &challenge(3));
            assert_eq!(ma == mb, a == b, "{a:?} {b:?}");
            // a signature over a's message is no signature over b's
            let proof = sign_key_proof(a, &account(1), &subject(2), &challenge(3), &key);
            assert_eq!(
                verify_key_proof(b, &account(1), &subject(2), &public, &proof).is_ok(),
                a == b,
                "{a:?} presented as {b:?}"
            );
        }
    }
}

/// The message is `signing_input(domain, account || subject || challenge)`, byte for byte.
#[test]
fn s9a_27_the_message_is_the_domain_framed_account_subject_challenge() {
    let message = proof_message(
        ProofPurpose::Refresh,
        &account(1),
        &subject(2),
        &challenge(3),
    );
    let mut expected = b"azlin-refresh-v1".to_vec();
    expected.push(0);
    expected.extend([1u8; 16]);
    expected.extend([2u8; 16]);
    expected.extend([3u8; 16]);
    assert_eq!(message, expected);
    assert_eq!(
        hex(&message),
        format!(
            "617a6c696e2d726566726573682d7631{}{}{}{}",
            "00",
            "01".repeat(16),
            "02".repeat(16),
            "03".repeat(16)
        )
    );
    let message = proof_message(
        ProofPurpose::AppRefresh,
        &account(1),
        &subject(2),
        &challenge(3),
    );
    assert!(message.starts_with(b"azlin-app-refresh-v1\0"));
    assert_eq!(message.len(), "azlin-app-refresh-v1".len() + 1 + 48);
}

#[test]
fn s9a_27_a_proof_signed_by_the_key_of_the_proof_domain_only_is_deterministic() {
    // ed25519 is deterministic: the same inputs give the same proof (so a test can pin one)
    assert_eq!(
        proof_for(ProofPurpose::Refresh),
        proof_for(ProofPurpose::Refresh)
    );
    assert_ne!(
        proof_for(ProofPurpose::Refresh),
        proof_for(ProofPurpose::EnrollFinish)
    );
}

#[test]
fn s9a_27_a_challenge_comes_from_the_entropy_seam_and_fails_with_it() {
    let mut entropy = Counting(0);
    let first = Challenge::generate(&mut entropy).expect("entropy");
    let second = Challenge::generate(&mut entropy).expect("entropy");
    assert_ne!(first, second);
    assert_eq!(first.as_bytes()[0], 1);
    assert!(Challenge::generate(&mut Failing).is_err());
}

/// S9A-28: the preimage of every signature is `domain || 0x00 || bytes`, built once by
/// azlin-keys and re-exported here; the value is pinned as a hex literal.
#[test]
fn s9a_28_the_signing_input_is_pinned() {
    let input = signing_input(&REFRESH_V1, &[0, 1, 2, 3]);
    assert_eq!(hex(&input), "617a6c696e2d726566726573682d76310000010203");
    // a second pin: another domain, another message
    let input = signing_input(&ENROLL_FINISH_V1, b"abc");
    assert_eq!(
        hex(&input),
        "617a6c696e2d656e726f6c6c2d66696e6973682d763100616263",
    );
    // and the shape in words: domain, one NUL, bytes
    assert_eq!(
        signing_input(&APP_REFRESH_V1, b"x"),
        b"azlin-app-refresh-v1\0x".to_vec()
    );
    // the empty message is the domain and its NUL
    assert_eq!(
        signing_input(&REFRESH_V1, &[]),
        b"azlin-refresh-v1\0".to_vec()
    );
}

/// A domain over 64 bytes, with a NUL, a space, a control byte or a byte above ASCII is not
/// well-formed (azlin-keys' `Domain::is_wellformed`, the check every domain constant of the engine
/// passes).
#[test]
fn s9a_28_a_bad_domain_is_refused() {
    // a domain is a 'static str: leak the built ones (a test)
    let long: &'static str = Box::leak("a".repeat(65).into_boxed_str());
    let exactly_64: &'static str = Box::leak("a".repeat(64).into_boxed_str());
    assert_eq!((long.len(), exactly_64.len()), (65, 64));
    assert!(!Domain::new(long).is_wellformed());
    assert!(Domain::new(exactly_64).is_wellformed());
    for bad in [
        "",
        "azlin\0v1",
        "azlin v1",
        "azlin\tv1",
        "azlin-caf\u{e9}-v1",
        "azlin\u{7f}v1",
    ] {
        assert!(!Domain::new(bad).is_wellformed(), "{bad:?}");
    }
    for good in ["azlin-refresh-v1", "a", "azlin-x-v2"] {
        assert!(Domain::new(good).is_wellformed(), "{good:?}");
    }
}

/// Two different domains never give the same input for the same bytes, whatever the bytes (the 0x00
/// ends the domain).
#[test]
fn s9a_28_two_domains_never_give_the_same_input() {
    let domains = [
        REFRESH_V1,
        ENROLL_FINISH_V1,
        APP_REFRESH_V1,
        ENVELOPE_V2,
        POINTER_V2,
        LOG_RECORD_V1,
        Domain::new("azlin-refresh-v10"),
        Domain::new("azlin-refresh"),
    ];
    let messages: [&[u8]; 6] = [b"", b"x", b"\0", b"-v1\0", b"\0-v1", b"refresh-v1\0xyz"];
    let mut inputs = BTreeSet::new();
    let mut count = 0;
    for domain in &domains {
        for message in messages {
            inputs.insert(signing_input(domain, message));
            count += 1;
        }
    }
    assert_eq!(
        inputs.len(),
        count,
        "a collision between two (domain, bytes) pairs"
    );
    // a prefix domain shifts bytes into the message but the NUL keeps the inputs apart
    assert_ne!(
        signing_input(&Domain::new("azlin-refresh"), b"-v1\0x"),
        signing_input(&REFRESH_V1, b"x")
    );
}
