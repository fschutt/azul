//! The EBANX stand-in's RSA against the test vectors in fixtures/rsa: PKCS#1 v1.5 is
//! deterministic, so the signature of the known message must be exactly the fixture's bytes, and
//! the fixture's JWS signature must verify. A token server checking the stand-in's notifications
//! uses the same vectors, so a port to another RSA crate cannot drift silently.

use crate::util;

const NOTIFY_MESSAGE: &str = include_str!("../fixtures/rsa/notify-message.txt");
const NOTIFY_SIGNATURE: &str = include_str!("../fixtures/rsa/notify-signature-sha1.b64");
const JWS_INPUT: &str = include_str!("../fixtures/rsa/jws-signing-input.txt");
const JWS_SIGNATURE: &str = include_str!("../fixtures/rsa/jws-signature-sha256.b64");

/// Standard base64 (padded) as the base64url the decoder reads.
fn std_b64_decode(s: &str) -> Vec<u8> {
    let url: String = s.trim().trim_end_matches('=').chars().map(|c| match c {
        '+' => '-',
        '/' => '_',
        c => c,
    }).collect();
    util::b64url_decode(&url).expect("the fixture is base64")
}

#[test]
fn the_notification_signature_is_the_fixtures() {
    let sig = util::rsa_sha1_sign(util::EBANX_NOTIFY_KEY, NOTIFY_MESSAGE.as_bytes()).expect("the test key signs");
    assert_eq!(sig, NOTIFY_SIGNATURE.trim());
}

#[test]
fn the_fixtures_jws_signature_verifies_and_a_changed_input_does_not() {
    let sig = std_b64_decode(JWS_SIGNATURE);
    assert!(util::rsa_sha256_ok(util::EBANX_JWS_PUB, JWS_INPUT.as_bytes(), &sig));
    let mut other = JWS_INPUT.as_bytes().to_vec();
    other[0] ^= 1;
    assert!(!util::rsa_sha256_ok(util::EBANX_JWS_PUB, &other, &sig));
}

