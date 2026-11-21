//! C7 (domain-separated signatures) and C8 (the device-directory seam).
//!
//! The known answers are independent of this crate and of ed25519-dalek: RFC 8032 section 7.1
//! tests 1 and 2 through the raw layer, and signatures over `domain || 0x00 || msg` computed with
//! a pure-Python RFC 8032 reference. The refusals (non-canonical `S`, small-order keys, bytes
//! that are no point) are checked against ed25519-dalek's own non-strict verification, so each
//! vector is shown to be a real attack before it is shown to be refused.
//!
//! Implements planning/architecture.md section 2.5 (domain strings), section 5.5 (envelopes carry
//! the signer), ARCH29 9.13 decision 2 (one signing rule `domain || 0x00 || bytes`).

#![cfg_attr(not(feature = "test-support"), allow(unused_imports, dead_code))]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use azlin_keys::sign::{
    self, sign_preimage, signing_input, verify_preimage, ALL_DOMAINS, ENVELOPE_V2, LOG_RECORD_V1,
    POINTER_V2,
};
use azlin_keys::DeviceDirectory;
use azlin_keys::{
    ChainSeq, DeviceId, DirectoryError, Domain, KeysError, Secret, SignError, SignKeypair,
    Signature, UnixTime, VerifyAt, VerifyKey,
};
#[cfg(feature = "test-support")]
use azlin_keys::{
    DeviceState, DirectoryEntry, EntropyError, FailingEntropy, MapDirectory, SeededEntropy,
};

use common::{from_hex, from_hex_array, to_hex, SplitMix64};

trait AmbiguousIfClone<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfClone<()> for T {}
impl<T: Clone> AmbiguousIfClone<u8> for T {}

fn keypair(fill: u8) -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([fill; 32]))
}

/// The group order L of the Ed25519 base point, little-endian.
const L_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

// ---------------------------------------------------------------------------------------------
// The preimage and the domains
// ---------------------------------------------------------------------------------------------

#[test]
fn c7_the_preimage_is_domain_nul_message() {
    // The vector of the brief: domain `azlin-pointer-v2` gives `b"azlin-pointer-v2\0" || msg`.
    let msg = b"the pointer";
    let mut want = b"azlin-pointer-v2\0".to_vec();
    want.extend_from_slice(msg);
    assert_eq!(signing_input(&POINTER_V2, msg), want);
    assert_eq!(
        to_hex(&signing_input(&POINTER_V2, msg)),
        "617a6c696e2d706f696e7465722d76320074686520706f696e746572"
    );
    assert_eq!(
        signing_input(&ENVELOPE_V2, b""),
        b"azlin-envelope-v2\0".to_vec()
    );
    assert_eq!(
        signing_input(&LOG_RECORD_V1, b"\0"),
        b"azlin-log-record-v1\0\0".to_vec()
    );
    // NOT the old draft `u8 len(domain) || domain || msg`.
    assert_ne!(
        signing_input(&POINTER_V2, msg)[0],
        POINTER_V2.as_str().len() as u8
    );
    // The input is the framing plus the message, whatever the message.
    for len in [0usize, 1, 31, 32, 33, 1000, 70_000] {
        let m = vec![0xA5u8; len];
        let input = signing_input(&POINTER_V2, &m);
        assert_eq!(input.len(), POINTER_V2.as_str().len() + 1 + len);
        assert!(input.starts_with(b"azlin-pointer-v2\0"));
        assert!(input.ends_with(&m));
    }
}

#[test]
fn c7_two_different_domains_never_give_equal_inputs() {
    let mut rng = SplitMix64::new(0xC7);
    let mut domains: Vec<Domain> = ALL_DOMAINS.to_vec();
    // Domains that are prefixes of each other, and that differ in one byte.
    domains.push(Domain::new("azlin-pointer-v2x"));
    domains.push(Domain::new("azlin-pointer-v"));
    domains.push(Domain::new("a"));
    domains.push(Domain::new("ab"));
    for d in &domains {
        assert!(d.is_wellformed(), "{}", d.as_str());
    }
    let mut messages: Vec<Vec<u8>> =
        vec![vec![], vec![0], vec![0, 0], b"x".to_vec(), b"b\0c".to_vec()];
    for _ in 0..40 {
        let n = rng.below(70) as usize;
        messages.push(rng.bytes(n));
    }
    // Messages built from the other domain's text, to try to make the two framings meet.
    for d in &domains {
        messages.push(d.as_str().as_bytes().to_vec());
        let mut with_nul = d.as_str().as_bytes().to_vec();
        with_nul.push(0);
        messages.push(with_nul);
    }
    for (i, d1) in domains.iter().enumerate() {
        for (j, d2) in domains.iter().enumerate() {
            for m1 in &messages {
                for m2 in &messages {
                    let (a, b) = (signing_input(d1, m1), signing_input(d2, m2));
                    if i != j {
                        assert_ne!(
                            a,
                            b,
                            "{} / {} with {m1:?} / {m2:?}",
                            d1.as_str(),
                            d2.as_str()
                        );
                    } else {
                        assert_eq!(
                            a == b,
                            m1 == m2,
                            "one domain: equal inputs iff equal messages"
                        );
                    }
                }
            }
        }
    }

    // The negative control that shows why a domain must be well formed: with a NUL inside, two
    // different (domain, message) pairs give one input. `is_wellformed` refuses such a domain.
    let evil = Domain::new("a\0b");
    assert!(!evil.is_wellformed());
    assert_eq!(
        signing_input(&evil, b"c"),
        signing_input(&Domain::new("a"), b"b\0c")
    );
}

#[test]
fn domains_are_unique_and_ascii_without_nul() {
    assert_eq!(
        ALL_DOMAINS.len(),
        3,
        "the crate defines exactly three domains"
    );
    for (i, d) in ALL_DOMAINS.iter().enumerate() {
        let text = d.as_str();
        assert!(d.is_wellformed(), "{text}");
        assert!(
            text.is_ascii() && !text.contains('\0') && !text.is_empty(),
            "{text}"
        );
        assert!(text.starts_with("azlin-"), "{text}");
        let (_, version) = text.rsplit_once("-v").expect("a -v<N> suffix");
        assert!(
            !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()),
            "{text}"
        );
        for other in &ALL_DOMAINS[i + 1..] {
            assert_ne!(text, other.as_str());
        }
    }
    let texts: Vec<&str> = ALL_DOMAINS.iter().map(|d| d.as_str()).collect();
    assert_eq!(
        texts,
        [
            "azlin-envelope-v2",
            "azlin-pointer-v2",
            "azlin-log-record-v1"
        ]
    );
    assert_eq!(ENVELOPE_V2.as_str(), "azlin-envelope-v2");
    assert_eq!(POINTER_V2.as_str(), "azlin-pointer-v2");
    assert_eq!(LOG_RECORD_V1.as_str(), "azlin-log-record-v1");

    // The same three strings are in the contract's registry (S5's `signed.rs`), where the units
    // propose them (DRIFT W1S15A-2). Skipped when that file is not next to this crate.
    let signed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../azlin-contract/src/signed.rs");
    match fs::read_to_string(&signed) {
        Ok(text) => {
            for d in ALL_DOMAINS {
                assert!(
                    text.contains(&format!("\"{}\"", d.as_str())),
                    "azlin-contract's signed.rs does not list {}",
                    d.as_str()
                );
            }
        }
        Err(_) => eprintln!("SKIPPED: azlin-contract/src/signed.rs is not next to this crate"),
    }
}

#[test]
fn c7_domain_wellformedness() {
    assert!(Domain::new("azlin-x-v1").is_wellformed());
    assert!(!Domain::new("").is_wellformed(), "empty");
    assert!(!Domain::new("with space").is_wellformed());
    assert!(!Domain::new("tab\t").is_wellformed());
    assert!(!Domain::new("nul\0").is_wellformed());
    assert!(!Domain::new("\0").is_wellformed());
    assert!(!Domain::new("caf\u{e9}").is_wellformed(), "non-ASCII");
    assert!(!Domain::new("new\nline").is_wellformed());
    assert!(
        Domain::new("Upper-Case-v1").is_wellformed(),
        "printable ASCII is enough"
    );
    assert_eq!(Domain::MAX_LEN, 64);
    const SIXTY_FOUR: &str = "0123456789012345678901234567890123456789012345678901234567890123";
    const SIXTY_FIVE: &str = "01234567890123456789012345678901234567890123456789012345678901234";
    assert_eq!(SIXTY_FOUR.len(), 64);
    assert!(Domain::new(SIXTY_FOUR).is_wellformed());
    assert!(!Domain::new(SIXTY_FIVE).is_wellformed(), "65 bytes");
}

// ---------------------------------------------------------------------------------------------
// Known answers
// ---------------------------------------------------------------------------------------------

#[test]
fn c7_known_answers_made_with_an_independent_reference() {
    // seed 07..07, computed with a pure-Python RFC 8032 implementation over `domain || 0 || msg`.
    let key = keypair(7);
    assert_eq!(
        to_hex(key.verify_key().as_bytes()),
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
    );
    let cases: [(&Domain, Vec<u8>, &str, &str); 3] = [
        (
            &POINTER_V2,
            b"the pointer".to_vec(),
            "617a6c696e2d706f696e7465722d76320074686520706f696e746572",
            "3ba82c8c6b62ec47055bbaef096c340ad0a1272ae98385b35e9fb7582a2b82630acfe8869c973988d73958ab63278ada371600e5136c90d2c235e5e5229d6b03",
        ),
        (
            &ENVELOPE_V2,
            vec![],
            "617a6c696e2d656e76656c6f70652d763200",
            "b5ac0ee6bcd7e51d35eb3aed5b14430b57f42ccf6bdeeb9b0cb0edd74985369ec83b59366d42986338d50ee01fdb5bc600c1c53d373cd6ba90161c4c76b05801",
        ),
        (
            &LOG_RECORD_V1,
            (0u8..40).collect(),
            "617a6c696e2d6c6f672d7265636f72642d763100000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627",
            "f51c79303d7591dfe4e071cd359feb89609525c0f94b8294955b60a29c315dc4c6e084bc3c649298058633cbbe18682f2ab587831b834ce46f2c686051f5db0f",
        ),
    ];
    for (domain, msg, preimage_hex, sig_hex) in cases {
        assert_eq!(
            to_hex(&signing_input(domain, &msg)),
            preimage_hex,
            "{}",
            domain.as_str()
        );
        let sig = sign::sign(domain, &msg, &key);
        assert_eq!(to_hex(sig.as_bytes()), sig_hex, "{}", domain.as_str());
        assert_eq!(sign::verify(domain, &msg, &key.verify_key(), &sig), Ok(()));
        // The framed bytes through the raw layer give the same signature.
        assert_eq!(sign_preimage(&from_hex(preimage_hex), &key), sig);
    }
}

/// RFC 8032 section 7.1 tests 1 and 2 pass through the raw layer (no domain framing).
#[test]
fn c7_rfc_8032_test_vectors_through_the_raw_layer() {
    let vectors = [
        (
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "",
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        ),
        (
            "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
            "72",
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
        ),
    ];
    for (secret, public, message, signature) in vectors {
        let key = SignKeypair::from_seed(&Secret::from_bytes(from_hex_array::<32>(secret)));
        let vk = key.verify_key();
        assert_eq!(to_hex(vk.as_bytes()), public);
        let msg = from_hex(message);
        let sig = sign_preimage(&msg, &key);
        assert_eq!(to_hex(sig.as_bytes()), signature);
        assert_eq!(verify_preimage(&msg, &vk, &sig), Ok(()));
        // The same message through the DOMAIN layer is another signature, and the RFC signature is
        // not a domain signature of the message.
        let framed = sign::sign(&POINTER_V2, &msg, &key);
        assert_ne!(framed, sig);
        assert_eq!(
            sign::verify(
                &POINTER_V2,
                &msg,
                &vk,
                &Signature::from_bytes(from_hex_array::<64>(signature))
            ),
            Err(SignError::Bad)
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Round trips, separation, tampering
// ---------------------------------------------------------------------------------------------

#[test]
fn c7_sign_verify_domain_separation() {
    let key = keypair(1);
    let vk = key.verify_key();
    let mut rng = SplitMix64::new(0x5167);
    let mut messages: Vec<Vec<u8>> =
        vec![vec![], vec![0], b"a message".to_vec(), vec![0xFF; 70_000]];
    for _ in 0..30 {
        let n = rng.below(300) as usize;
        messages.push(rng.bytes(n));
    }

    for msg in &messages {
        for domain in ALL_DOMAINS {
            let sig = sign::sign(domain, msg, &key);
            // Round trip, and determinism.
            assert_eq!(sign::verify(domain, msg, &vk, &sig), Ok(()));
            assert_eq!(
                sign::sign(domain, msg, &key),
                sig,
                "Ed25519 signatures are deterministic"
            );
            // The same message under any other domain does not verify.
            for other in ALL_DOMAINS {
                if other != domain {
                    assert_eq!(
                        sign::verify(other, msg, &vk, &sig),
                        Err(SignError::Bad),
                        "{} signature under {}",
                        domain.as_str(),
                        other.as_str()
                    );
                }
            }
        }
    }

    // A signature of another message, another key, or another domain's message is refused.
    let sig = sign::sign(&POINTER_V2, b"one", &key);
    assert_eq!(
        sign::verify(&POINTER_V2, b"two", &vk, &sig),
        Err(SignError::Bad)
    );
    assert_eq!(
        sign::verify(&POINTER_V2, b"one", &keypair(2).verify_key(), &sig),
        Err(SignError::Bad)
    );
    // A message that carries the domain text itself is no way across domains.
    let sig_env = sign::sign(&ENVELOPE_V2, b"azlin-pointer-v2\0one", &key);
    assert_eq!(
        sign::verify(&POINTER_V2, b"one", &vk, &sig_env),
        Err(SignError::Bad)
    );
    assert_eq!(
        sign::verify(&POINTER_V2, b"azlin-pointer-v2\0one", &vk, &sig_env),
        Err(SignError::Bad)
    );
}

#[test]
fn c7_every_tampered_bit_is_refused_and_nothing_panics() {
    let key = keypair(3);
    let vk = key.verify_key();
    let msg: Vec<u8> = (0..64u8).collect();
    let sig = sign::sign(&LOG_RECORD_V1, &msg, &key);
    assert_eq!(sign::verify(&LOG_RECORD_V1, &msg, &vk, &sig), Ok(()));

    // Every single bit of the message.
    for bit in 0..msg.len() * 8 {
        let mut m = msg.clone();
        m[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            sign::verify(&LOG_RECORD_V1, &m, &vk, &sig),
            Err(SignError::Bad),
            "message bit {bit}"
        );
    }
    // Truncated and extended messages.
    assert_eq!(
        sign::verify(&LOG_RECORD_V1, &msg[..63], &vk, &sig),
        Err(SignError::Bad)
    );
    let mut longer = msg.clone();
    longer.push(0);
    assert_eq!(
        sign::verify(&LOG_RECORD_V1, &longer, &vk, &sig),
        Err(SignError::Bad)
    );
    // Every single bit of the signature (512): the high bits of `S` make it non-canonical, the
    // others change `R` or `S`.
    for bit in 0..512 {
        let mut bytes = *sig.as_bytes();
        bytes[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            sign::verify(&LOG_RECORD_V1, &msg, &vk, &Signature::from_bytes(bytes)),
            Err(SignError::Bad),
            "signature bit {bit}"
        );
    }
    // Every single bit of the key (256): another point, or no point at all.
    for bit in 0..256 {
        let mut bytes = *vk.as_bytes();
        bytes[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            sign::verify(&LOG_RECORD_V1, &msg, &VerifyKey::from_bytes(bytes), &sig),
            Err(SignError::Bad),
            "key bit {bit}"
        );
    }

    // Garbage keys and signatures: always `Bad`, never a panic.
    let mut rng = SplitMix64::new(0xBAD);
    for _ in 0..2000 {
        let k = VerifyKey::from_bytes(<[u8; 32]>::try_from(rng.bytes(32)).expect("32 bytes"));
        let s = Signature::from_bytes(<[u8; 64]>::try_from(rng.bytes(64)).expect("64 bytes"));
        let n = rng.below(80) as usize;
        let m = rng.bytes(n);
        assert_eq!(sign::verify(&POINTER_V2, &m, &k, &s), Err(SignError::Bad));
    }
    for fill in [0x00u8, 0xFF, 0x01, 0x80] {
        let k = VerifyKey::from_bytes([fill; 32]);
        let s = Signature::from_bytes([fill; 64]);
        assert_eq!(
            sign::verify(&POINTER_V2, b"x", &k, &s),
            Err(SignError::Bad),
            "fill {fill:#x}"
        );
        assert_eq!(
            sign::verify(&POINTER_V2, b"x", &vk, &s),
            Err(SignError::Bad)
        );
        assert_eq!(
            sign::verify(&POINTER_V2, b"x", &k, &sig),
            Err(SignError::Bad)
        );
    }
}

/// `verify_strict` refuses a non-canonical `S` (`S + L`), the case that plain verification of
/// the old drafts accepted in some libraries.
#[test]
fn c7_a_non_canonical_signature_is_refused() {
    let key = keypair(4);
    let vk = key.verify_key();
    let msg = b"malleable?";
    let sig = sign::sign(&POINTER_V2, msg, &key);

    let mut malleated = *sig.as_bytes();
    // S' = S + L over 256 bits, little-endian (S < L, so S + L < 2^254: no carry out).
    let mut carry = 0u16;
    for (s_byte, l_byte) in malleated[32..].iter_mut().zip(L_LE) {
        let sum = u16::from(*s_byte) + u16::from(l_byte) + carry;
        *s_byte = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    assert_eq!(carry, 0);
    assert_ne!(&malleated, sig.as_bytes());
    // S' is S plus L: the same value modulo L, a second encoding of one signature.
    assert_eq!(
        sign::verify(&POINTER_V2, msg, &vk, &Signature::from_bytes(malleated)),
        Err(SignError::Bad),
        "S + L must not verify"
    );
    assert_eq!(
        sign::verify(&POINTER_V2, msg, &vk, &sig),
        Ok(()),
        "the canonical one does"
    );

    // The raw layer is strict too.
    let preimage = signing_input(&POINTER_V2, msg);
    assert_eq!(
        verify_preimage(&preimage, &vk, &Signature::from_bytes(malleated)),
        Err(SignError::Bad)
    );
    assert_eq!(verify_preimage(&preimage, &vk, &sig), Ok(()));
}

/// Keys of small order: with such a key and `R` the identity, `S = 0` is a signature of every
/// message under a permissive verifier (ed25519-dalek's `verify` accepts it: asserted below, so the
/// vector is shown to be a real attack); `verify_strict` refuses it.
#[test]
fn c7_small_order_keys_are_refused() {
    use ed25519_dalek::{Signature as DalekSignature, Verifier, VerifyingKey};

    let small_order = [
        "0100000000000000000000000000000000000000000000000000000000000000", // identity, order 1
        "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f", // order 2
        "0000000000000000000000000000000000000000000000000000000000000000", // order 4
        "0000000000000000000000000000000000000000000000000000000000000080", // order 4
        "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a", // order 8
        "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa", // order 8
        "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05", // order 8
        "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85", // order 8
    ];
    let msg = b"anything at all";
    let preimage = signing_input(&POINTER_V2, msg);
    for hex in small_order {
        let bytes = from_hex_array::<32>(hex);
        // The control: dalek parses the encoding and calls it weak.
        let dalek = VerifyingKey::from_bytes(&bytes).expect("a small-order encoding is a point");
        assert!(dalek.is_weak(), "{hex} must have small order");

        let key = VerifyKey::from_bytes(bytes);
        // Signatures an attacker can make without any secret: R = identity, S = 0, and R = the
        // key itself with S = 0.
        for r in [
            from_hex_array::<32>(
                "0100000000000000000000000000000000000000000000000000000000000000",
            ),
            bytes,
        ] {
            let mut sig_bytes = [0u8; 64];
            sig_bytes[..32].copy_from_slice(&r);
            let sig = Signature::from_bytes(sig_bytes);
            assert_eq!(
                verify_preimage(&preimage, &key, &sig),
                Err(SignError::Bad),
                "{hex}"
            );
            assert_eq!(
                sign::verify(&POINTER_V2, msg, &key, &sig),
                Err(SignError::Bad),
                "{hex}"
            );
        }
    }

    // The control that the identity key with `R = identity`, `S = 0` is a signature of EVERY
    // message for a non-strict verifier: the attack the strict check exists for.
    let identity = VerifyingKey::from_bytes(&from_hex_array::<32>(small_order[0])).unwrap();
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&from_hex_array::<32>(small_order[0]));
    let permissive = identity.verify(&preimage, &DalekSignature::from_bytes(&sig_bytes));
    assert!(
        permissive.is_ok(),
        "non-strict verification accepts the universal forgery"
    );
    assert!(identity
        .verify_strict(&preimage, &DalekSignature::from_bytes(&sig_bytes))
        .is_err());
    assert_eq!(
        verify_preimage(
            &preimage,
            &VerifyKey::from_bytes(from_hex_array::<32>(small_order[0])),
            &Signature::from_bytes(sig_bytes)
        ),
        Err(SignError::Bad)
    );

    // A small-order R under an honest key is refused too.
    let key = keypair(5);
    let honest = sign::sign(&POINTER_V2, msg, &key);
    let mut forged = *honest.as_bytes();
    forged[..32].copy_from_slice(&from_hex_array::<32>(small_order[1]));
    assert_eq!(
        sign::verify(
            &POINTER_V2,
            msg,
            &key.verify_key(),
            &Signature::from_bytes(forged)
        ),
        Err(SignError::Bad)
    );
}

#[test]
fn c7_bytes_that_are_no_curve_point_are_a_bad_key() {
    use ed25519_dalek::VerifyingKey;
    // y = 2 is no point of the curve (no square root): the control is dalek's own parser.
    let not_a_point =
        from_hex_array::<32>("0200000000000000000000000000000000000000000000000000000000000000");
    assert!(VerifyingKey::from_bytes(&not_a_point).is_err());
    let key = keypair(6);
    let sig = sign::sign(&POINTER_V2, b"m", &key);
    assert_eq!(
        sign::verify(&POINTER_V2, b"m", &VerifyKey::from_bytes(not_a_point), &sig),
        Err(SignError::Bad)
    );
}

// ---------------------------------------------------------------------------------------------
// The types
// ---------------------------------------------------------------------------------------------

#[test]
fn c7_verify_key_and_signature_are_plain_public_data() {
    // Copy, Eq, Ord, Hash, Debug: usable as BTreeMap keys (azlin-regbase aliases them).
    let a = keypair(1).verify_key();
    let b = keypair(2).verify_key();
    let copy = a;
    assert_eq!(a, copy);
    assert_ne!(a, b);
    let mut map: BTreeMap<VerifyKey, u32> = BTreeMap::new();
    map.insert(a, 1);
    map.insert(b, 2);
    map.insert(copy, 3);
    assert_eq!(map.len(), 2);
    assert_eq!(map[&a], 3);
    assert!(
        (a < b) == (a.as_bytes() < b.as_bytes()),
        "the order is the order of the bytes"
    );

    let s1 = sign::sign(&POINTER_V2, b"1", &keypair(1));
    let s2 = sign::sign(&POINTER_V2, b"2", &keypair(1));
    let mut sigs: BTreeMap<Signature, u32> = BTreeMap::new();
    sigs.insert(s1, 1);
    sigs.insert(s2, 2);
    assert_eq!(sigs.len(), 2);
    assert!((s1 < s2) == (s1.as_bytes() < s2.as_bytes()));

    // from_bytes / as_bytes round trip, lengths, text.
    assert_eq!(VerifyKey::from_bytes([9; 32]).as_bytes(), &[9u8; 32]);
    assert_eq!(Signature::from_bytes([9; 64]).as_bytes(), &[9u8; 64]);
    assert_eq!(VerifyKey::LEN, 32);
    assert_eq!(Signature::LEN, 64);
    assert_eq!(a.to_string().len(), 64);
    assert_eq!(s1.to_string().len(), 128);
    assert_eq!(a.to_string(), to_hex(a.as_bytes()));
    assert_eq!(format!("{a:?}"), format!("VerifyKey({a})"));
    assert_eq!(format!("{s1:?}"), format!("Signature({s1})"));
}

#[test]
fn c7_sign_keypair_hides_its_seed() {
    let key = keypair(7);
    // The same seed is the same key; another seed another key.
    assert_eq!(key.verify_key(), keypair(7).verify_key());
    assert_ne!(key.verify_key(), keypair(8).verify_key());
    // Debug shows the public key and never the seed.
    let text = format!("{key:?}");
    assert!(text.contains(&key.verify_key().to_string()));
    assert!(text.contains("seed ***"));
    assert!(!text.contains("0707"), "no seed byte is printed: {text}");
    // No Clone: a second holder comes from the seed.
    <SignKeypair as AmbiguousIfClone<_>>::check();
}

#[cfg(feature = "test-support")]
#[test]
fn c7_generate_draws_the_seed_from_the_entropy_seam() {
    let mut ent = SeededEntropy::from_u64(21);
    let a = SignKeypair::generate(&mut ent).unwrap();
    assert_eq!(ent.drawn(), 32, "exactly the 32 seed bytes");
    let b = SignKeypair::generate(&mut ent).unwrap();
    assert_ne!(a.verify_key(), b.verify_key());
    let mut replay = SeededEntropy::from_u64(21);
    assert_eq!(
        SignKeypair::generate(&mut replay).unwrap().verify_key(),
        a.verify_key()
    );
    // The key of a seed drawn from the seam is the key of that seed.
    let mut again = SeededEntropy::from_u64(21);
    let seed = Secret::<32>::generate(&mut again).unwrap();
    assert_eq!(SignKeypair::from_seed(&seed).verify_key(), a.verify_key());

    let mut failing = FailingEntropy::after(31);
    assert_eq!(
        SignKeypair::generate(&mut failing).err(),
        Some(EntropyError::Exhausted)
    );
}

// ---------------------------------------------------------------------------------------------
// C8: the device directory seam
// ---------------------------------------------------------------------------------------------

fn device(n: u8) -> DeviceId {
    DeviceId::from_bytes([n; 16])
}

/// A directory that answers one fixed thing and counts the questions: the seam is object safe and
/// `verify_with` asks it exactly once.
struct Fixed {
    answer: Result<VerifyKey, DirectoryError>,
    asked: std::cell::Cell<u32>,
    last: std::cell::Cell<Option<VerifyAt>>,
}

impl DeviceDirectory for Fixed {
    fn verify_key(&self, _device: &DeviceId, at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        self.asked.set(self.asked.get() + 1);
        self.last.set(Some(at));
        self.answer
    }
}

#[test]
fn c8_directory_errors_map_to_distinct_sign_errors_and_nothing_is_verified() {
    let signer = keypair(1);
    let msg = b"signed by the device";
    let sig = sign::sign(&ENVELOPE_V2, msg, &signer);

    let errors = [
        (DirectoryError::Unknown, SignError::UnknownSigner),
        (DirectoryError::NotYetActive, SignError::NotYetActive),
        (DirectoryError::Revoked, SignError::Revoked),
        (DirectoryError::Unavailable, SignError::DirectoryUnavailable),
    ];
    let mut seen = Vec::new();
    for (directory_error, sign_error) in errors {
        assert_eq!(SignError::from(directory_error), sign_error);
        let dir = Fixed {
            answer: Err(directory_error),
            asked: Default::default(),
            last: Default::default(),
        };
        // The signature is VALID for the right key; the directory's refusal still wins, and no key
        // is tried (there is none: no default key).
        let at = VerifyAt::Time(UnixTime::from_secs(77));
        assert_eq!(
            sign::verify_with(&dir, &device(1), at, &ENVELOPE_V2, msg, &sig),
            Err(sign_error)
        );
        assert_eq!(dir.asked.get(), 1, "the directory is asked once");
        assert_eq!(
            dir.last.get(),
            Some(at),
            "the point asked about is passed through"
        );
        assert!(
            !seen.contains(&sign_error),
            "each directory error has its own sign error"
        );
        seen.push(sign_error);
        assert_ne!(sign_error, SignError::Bad);
    }
    assert_eq!(seen.len(), 4);

    // With a key, the answer is the verification's own.
    let good = Fixed {
        answer: Ok(signer.verify_key()),
        asked: Default::default(),
        last: Default::default(),
    };
    assert_eq!(
        sign::verify_with(&good, &device(1), VerifyAt::Now, &ENVELOPE_V2, msg, &sig),
        Ok(())
    );
    assert_eq!(good.last.get(), Some(VerifyAt::Now));
    // Wrong domain, wrong message, wrong key, a garbage key: `Bad` (never a directory error).
    assert_eq!(
        sign::verify_with(&good, &device(1), VerifyAt::Now, &POINTER_V2, msg, &sig),
        Err(SignError::Bad)
    );
    assert_eq!(
        sign::verify_with(
            &good,
            &device(1),
            VerifyAt::Now,
            &ENVELOPE_V2,
            b"other",
            &sig
        ),
        Err(SignError::Bad)
    );
    let other = Fixed {
        answer: Ok(keypair(2).verify_key()),
        asked: Default::default(),
        last: Default::default(),
    };
    assert_eq!(
        sign::verify_with(&other, &device(1), VerifyAt::Now, &ENVELOPE_V2, msg, &sig),
        Err(SignError::Bad)
    );
    let garbage = Fixed {
        answer: Ok(VerifyKey::from_bytes([0xFF; 32])),
        asked: Default::default(),
        last: Default::default(),
    };
    assert_eq!(
        sign::verify_with(&garbage, &device(1), VerifyAt::Now, &ENVELOPE_V2, msg, &sig),
        Err(SignError::Bad)
    );

    // All five sign errors print distinct words.
    let words: Vec<String> = [
        SignError::Bad,
        SignError::UnknownSigner,
        SignError::NotYetActive,
        SignError::Revoked,
        SignError::DirectoryUnavailable,
    ]
    .iter()
    .map(|e| e.to_string())
    .collect();
    for (i, a) in words.iter().enumerate() {
        for b in &words[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[cfg(feature = "test-support")]
#[test]
fn c8_directory_seam() {
    let signer = keypair(1);
    let key = signer.verify_key();
    let msg = b"a wrap";
    let sig = sign::sign(&ENVELOPE_V2, msg, &signer);

    let t = UnixTime::from_secs;
    let s = ChainSeq::new;
    let mut dir = MapDirectory::new();
    // device 1: active from time 100 and position 10, revoked from time 200 and position 20.
    dir.insert(
        device(1),
        DirectoryEntry {
            key,
            now: DeviceState::Active,
            added_at: t(100),
            removed_at: Some(t(200)),
            added_seq: s(10),
            removed_seq: Some(s(20)),
        },
    );
    // device 2: pending (added, not active yet). device 3: revoked. device 4: always active.
    dir.insert(
        device(2),
        DirectoryEntry {
            now: DeviceState::Pending,
            ..DirectoryEntry::active(key)
        },
    );
    dir.insert(
        device(3),
        DirectoryEntry {
            now: DeviceState::Revoked,
            ..DirectoryEntry::active(key)
        },
    );
    dir.insert_active(device(4), key);

    // Object safe: the seam is used as a `&dyn DeviceDirectory`.
    let seam: &dyn DeviceDirectory = &dir;

    // Unknown device, at every point.
    for at in [
        VerifyAt::Now,
        VerifyAt::Time(t(150)),
        VerifyAt::ChainSeq(s(15)),
    ] {
        assert_eq!(
            seam.verify_key(&device(9), at),
            Err(DirectoryError::Unknown)
        );
    }

    // `Now` follows the device's state.
    assert_eq!(seam.verify_key(&device(1), VerifyAt::Now), Ok(key));
    assert_eq!(
        seam.verify_key(&device(2), VerifyAt::Now),
        Err(DirectoryError::NotYetActive)
    );
    assert_eq!(
        seam.verify_key(&device(3), VerifyAt::Now),
        Err(DirectoryError::Revoked)
    );
    assert_eq!(seam.verify_key(&device(4), VerifyAt::Now), Ok(key));

    // `Time`: added inclusive, removed from that time on.
    for (time, want) in [
        (0, Err(DirectoryError::NotYetActive)),
        (99, Err(DirectoryError::NotYetActive)),
        (100, Ok(key)),
        (199, Ok(key)),
        (200, Err(DirectoryError::Revoked)),
        (u64::MAX, Err(DirectoryError::Revoked)),
    ] {
        assert_eq!(
            seam.verify_key(&device(1), VerifyAt::Time(t(time))),
            want,
            "time {time}"
        );
    }
    // `ChainSeq`: the same windows in log positions ("valid at the position of the write").
    for (seq, want) in [
        (0, Err(DirectoryError::NotYetActive)),
        (9, Err(DirectoryError::NotYetActive)),
        (10, Ok(key)),
        (19, Ok(key)),
        (20, Err(DirectoryError::Revoked)),
        (u64::MAX, Err(DirectoryError::Revoked)),
    ] {
        assert_eq!(
            seam.verify_key(&device(1), VerifyAt::ChainSeq(s(seq))),
            want,
            "position {seq}"
        );
    }
    // An always-active device is valid at every point.
    for at in [
        VerifyAt::Time(t(0)),
        VerifyAt::Time(t(u64::MAX)),
        VerifyAt::ChainSeq(s(0)),
        VerifyAt::ChainSeq(s(u64::MAX)),
    ] {
        assert_eq!(seam.verify_key(&device(4), at), Ok(key));
    }

    // `verify_with` through the double: each refusal is its own error, the valid case verifies.
    let check = |device_n: u8, at: VerifyAt| {
        sign::verify_with(seam, &device(device_n), at, &ENVELOPE_V2, msg, &sig)
    };
    assert_eq!(check(1, VerifyAt::Time(t(150))), Ok(()));
    assert_eq!(check(1, VerifyAt::ChainSeq(s(15))), Ok(()));
    assert_eq!(
        check(1, VerifyAt::Time(t(50))),
        Err(SignError::NotYetActive)
    );
    assert_eq!(check(1, VerifyAt::Time(t(250))), Err(SignError::Revoked));
    assert_eq!(check(1, VerifyAt::ChainSeq(s(25))), Err(SignError::Revoked));
    assert_eq!(check(2, VerifyAt::Now), Err(SignError::NotYetActive));
    assert_eq!(check(3, VerifyAt::Now), Err(SignError::Revoked));
    assert_eq!(check(9, VerifyAt::Now), Err(SignError::UnknownSigner));
    assert_eq!(check(4, VerifyAt::Now), Ok(()));

    // A directory that is down answers `Unavailable` to everything, the unknown included.
    dir.set_unavailable(true);
    let down: &dyn DeviceDirectory = &dir;
    for n in [1u8, 2, 3, 4, 9] {
        for at in [
            VerifyAt::Now,
            VerifyAt::Time(t(150)),
            VerifyAt::ChainSeq(s(15)),
        ] {
            assert_eq!(
                down.verify_key(&device(n), at),
                Err(DirectoryError::Unavailable)
            );
            assert_eq!(
                sign::verify_with(down, &device(n), at, &ENVELOPE_V2, msg, &sig),
                Err(SignError::DirectoryUnavailable)
            );
        }
    }
    dir.set_unavailable(false);
    assert_eq!(dir.verify_key(&device(4), VerifyAt::Now), Ok(key));

    // Another device's key under the same id is `Bad`, not accepted: the directory names the key.
    let mut swapped = MapDirectory::new();
    swapped.insert_active(device(1), keypair(2).verify_key());
    assert_eq!(
        sign::verify_with(&swapped, &device(1), VerifyAt::Now, &ENVELOPE_V2, msg, &sig),
        Err(SignError::Bad)
    );
    // The DirectoryError words are distinct.
    let words: Vec<String> = [
        DirectoryError::Unknown,
        DirectoryError::NotYetActive,
        DirectoryError::Revoked,
        DirectoryError::Unavailable,
    ]
    .iter()
    .map(|e| e.to_string())
    .collect();
    for (i, a) in words.iter().enumerate() {
        for b in &words[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn c8_keys_error_wraps_each_family_and_keeps_its_source() {
    use std::error::Error;
    let wrapped: [KeysError; 6] = [
        azlin_keys::IdError::Length.into(),
        azlin_keys::EntropyError::Os.into(),
        azlin_keys::ClockError::Unreadable.into(),
        SignError::Revoked.into(),
        DirectoryError::Unavailable.into(),
        azlin_keys::ChainError::Corrupt.into(),
    ];
    for (i, e) in wrapped.iter().enumerate() {
        assert!(e.source().is_some(), "{e}");
        assert_eq!(e.to_string(), e.source().unwrap().to_string());
        for other in &wrapped[i + 1..] {
            assert_ne!(e, other);
        }
    }
}
