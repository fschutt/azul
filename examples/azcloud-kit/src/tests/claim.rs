//! The claim of a paid drive: the sign-up the token server seals to a checkout's claim key opens
//! with that key, for that checkout, as it was sealed - and with nothing else.

use base64::Engine;

use crate::claim::{seal, seal_with, ClaimError, ClaimKey};

/// What a token server seals: the sign-up JSON's bytes.
const SIGNUP: &[u8] = br#"{"drive": {"id": "d_1"}, "drive_token": "dt_f.0.sesame"}"#;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap()
}

#[test]
fn a_sealed_signup_opens_with_its_claim_key_for_its_checkout() {
    let key = ClaimKey::generate().unwrap();
    let sealed = seal(&key.public_base64(), "ck_1", SIGNUP);
    assert_eq!(key.open("ck_1", &sealed).unwrap().as_bytes(), SIGNUP);
    // eph_pk (32) || nonce (12) || the sign-up and its tag (16).
    assert_eq!(unb64(&sealed).len(), 32 + 12 + SIGNUP.len() + 16);
    // Each seal is a fresh ephemeral key and nonce: the same sign-up seals differently.
    assert_ne!(sealed, seal(&key.public_base64(), "ck_1", SIGNUP));
}

#[test]
fn a_sealed_signup_does_not_open_with_another_claim_key() {
    let key = ClaimKey::generate().unwrap();
    let other = ClaimKey::generate().unwrap();
    let sealed = seal(&key.public_base64(), "ck_1", SIGNUP);
    assert_eq!(other.open("ck_1", &sealed), Err(ClaimError::Open));
}

#[test]
fn a_sealed_signup_does_not_open_for_another_checkout() {
    let key = ClaimKey::generate().unwrap();
    let sealed = seal(&key.public_base64(), "ck_1", SIGNUP);
    for other in ["ck_2", "ck_1 ", "CK_1", ""] {
        assert_eq!(key.open(other, &sealed), Err(ClaimError::Open), "{other:?}");
    }
}

#[test]
fn a_sealed_signup_with_a_flipped_byte_does_not_open() {
    let key = ClaimKey::generate().unwrap();
    let sealed = unb64(&seal(&key.public_base64(), "ck_1", SIGNUP));
    // The ephemeral key, the nonce, the ciphertext, the tag: every part is bound.
    for at in [0, 31, 32, 43, 44, sealed.len() / 2, sealed.len() - 17, sealed.len() - 1] {
        let mut changed = sealed.clone();
        changed[at] ^= 0x01;
        assert_eq!(
            key.open("ck_1", &b64(&changed)),
            Err(ClaimError::Open),
            "byte {at} flipped"
        );
    }
    let mut cut = sealed.clone();
    cut.pop();
    assert_eq!(key.open("ck_1", &b64(&cut)), Err(ClaimError::Open), "a byte short");
}

#[test]
fn what_is_no_sealed_signup_is_refused_before_it_is_opened() {
    let key = ClaimKey::generate().unwrap();
    // One byte short of an ephemeral key, a nonce and a tag.
    let too_short = b64(&[0u8; 59]);
    for bad in ["", "not base64!", "AAAA", too_short.as_str()] {
        assert!(
            matches!(key.open("ck_1", bad), Err(ClaimError::Malformed(_))),
            "{bad:?}"
        );
    }
    // A sealed sign-up whose ephemeral key is of a small order (all zero) gives no shared
    // secret: it never opens.
    let mut zero = vec![0u8; 32 + 12 + 16 + 4];
    zero[40] = 1;
    assert_eq!(key.open("ck_1", &b64(&zero)), Err(ClaimError::Open));
}

/// What scripts/azlin_claim.py (the mock token server's seal, stdlib Python) answers for
/// `--vector`: the claim secret is the bytes 1..=32, the ephemeral secret 101..=132, the nonce
/// 201..=212.
#[test]
fn a_signup_the_mock_token_server_sealed_opens() {
    let key = ClaimKey::from_base64("AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=").unwrap();
    assert_eq!(
        key.public_base64(),
        "B6N8vBQgk8i3VdwbEOhstCY3StFqqFPtC9/AsrhtHHw="
    );
    let sealed = "VxR2nRFr92Q2rnS8eT0sMK0ZA8WaxSc4BcfiaYtBDDbJysvMzc7P0NHS09Q5pJMPjcOPL6JRCNRLDWdeMTai\
                  u2DPk9ijRPiLqjWgxw7Y2XSnqDEhXIemDIEF3e6FkAn4yqIuEeuR3TtQIx8jGbsC4ensAW9nTDVTSJYy0hc=";
    assert_eq!(
        key.open("ck_vector", sealed).unwrap(),
        r#"{"drive_token":"dt_f_vector.0.claimed","drive":{"id":"d_vector"}}"#
    );
    // ... and the Rust seal of the same inputs is the same bytes.
    let eph: [u8; 32] = std::array::from_fn(|i| 101 + i as u8);
    let nonce: [u8; 12] = std::array::from_fn(|i| 201 + i as u8);
    assert_eq!(
        seal_with(
            &key.public_base64(),
            "ck_vector",
            br#"{"drive_token":"dt_f_vector.0.claimed","drive":{"id":"d_vector"}}"#,
            eph,
            nonce
        ),
        sealed
    );
}

#[test]
fn a_claim_key_keeps_its_secret_through_the_keyring_and_debug_shows_none() {
    let key = ClaimKey::generate().unwrap();
    let kept = key.to_base64();
    assert_eq!(unb64(&kept).len(), 32);
    let back = ClaimKey::from_base64(&kept).unwrap();
    assert_eq!(back.public_base64(), key.public_base64());
    assert_eq!(back.to_base64(), kept);
    // The checkout's claim_key: standard padded base64 of the 32-byte public key.
    let public = key.public_base64();
    assert_eq!(public.len(), 44);
    assert!(public.ends_with('='));
    assert_eq!(unb64(&public).len(), 32);
    assert_ne!(public, kept, "the checkout names the public key, never the secret");
    let shown = format!("{key:?}");
    assert!(!shown.contains(kept.as_str()), "{shown}");
    let (short, long) = (b64(&[7u8; 31]), b64(&[7u8; 33]));
    for bad in ["", "AAAA", short.as_str(), long.as_str(), "B6N8vBQgk8i3VdwbEOhs"] {
        assert!(
            matches!(ClaimKey::from_base64(bad), Err(ClaimError::Malformed(_))),
            "{bad:?}"
        );
    }
}
