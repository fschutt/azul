//! The signature verifier seam: the strict Ed25519 verifier (known answers of RFC 8032 section
//! 7.1, malleability, wrong key, wrong message, truncated signature), what `azlin-keys` signs and
//! nothing else, the unsupported algorithms, and the fake scheme.
//!
//! The preimage the verifier takes is ALREADY framed (`domain || 0x00 || bytes`, COMMON-W1R R4):
//! the RFC vectors go through as raw messages because the verifier never frames.
//!
//! Implements planning/architecture.md section 2.5 (signed structures, one domain each) and
//! section 5.9 (every entry is signed).

use azlin_keys::sign::{sign, sign_preimage, ENVELOPE_V2, LOG_RECORD_V1, POINTER_V2};
use azlin_seams::sim::Rng;
use azlin_seams::{
    signing_input, Ed25519Verifier, FakeScheme, KeyId, Secret, SigAlg, SignKeypair,
    SignatureVerifier, VerifyError, VerifyKey,
};

mod common;
use common::{from_hex, from_hex_array, to_hex};

fn keypair(fill: u8) -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([fill; 32]))
}

/// The group order L of the Ed25519 base point, little-endian.
const L_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// RFC 8032 section 7.1, tests 1, 2 and 3: (secret key, public key, message, signature). Tests 1 and
/// 2 are the ones azlin-keys quotes; test 3 and all three were also recomputed with a pure-Python
/// RFC 8032 reference (scratch/W1-S17a/rfc3.py).
const RFC_8032: [(&str, &str, &str, &str); 3] = [
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
    (
        "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        "af82",
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    ),
];

#[test]
fn ed25519_verifier_rfc8032_vectors() {
    let verifier = Ed25519Verifier;
    for (index, (secret, public, message, signature)) in RFC_8032.iter().enumerate() {
        let key = SignKeypair::from_seed(&Secret::from_bytes(from_hex_array::<32>(secret)));
        assert_eq!(
            to_hex(key.verify_key().as_bytes()),
            *public,
            "TEST {}: the public key",
            index + 1
        );
        let public = from_hex(public);
        let message = from_hex(message);
        let signature = from_hex(signature);
        // The vector verifies through the seam, as a raw preimage (the seam never frames).
        assert_eq!(
            verifier.verify(SigAlg::Ed25519, &public, &message, &signature),
            Ok(()),
            "TEST {}",
            index + 1
        );
        // The same bytes are what the signing layer makes.
        assert_eq!(
            sign_preimage(&message, &key).as_bytes().as_slice(),
            signature.as_slice()
        );
        // The vector is no domain signature of the message: framing the message first refuses it.
        assert_eq!(
            verifier.verify(
                SigAlg::Ed25519,
                &public,
                &signing_input(&POINTER_V2, &message),
                &signature
            ),
            Err(VerifyError::BadSignature),
            "TEST {}: the framed message is another preimage",
            index + 1
        );
        // Every single flipped bit of the signature (512), of the message and of the key (256) is
        // refused, and nothing panics.
        for bit in 0..signature.len() * 8 {
            let mut bad = signature.clone();
            bad[bit / 8] ^= 1 << (bit % 8);
            assert!(
                verifier
                    .verify(SigAlg::Ed25519, &public, &message, &bad)
                    .is_err(),
                "TEST {}: signature bit {bit}",
                index + 1
            );
        }
        for bit in 0..message.len() * 8 {
            let mut bad = message.clone();
            bad[bit / 8] ^= 1 << (bit % 8);
            assert_eq!(
                verifier.verify(SigAlg::Ed25519, &public, &bad, &signature),
                Err(VerifyError::BadSignature),
                "TEST {}: message bit {bit}",
                index + 1
            );
        }
        for bit in 0..public.len() * 8 {
            let mut bad = public.clone();
            bad[bit / 8] ^= 1 << (bit % 8);
            assert!(
                verifier
                    .verify(SigAlg::Ed25519, &bad, &message, &signature)
                    .is_err(),
                "TEST {}: key bit {bit}",
                index + 1
            );
        }
    }
}

#[test]
fn ed25519_verifier_rejects_malleable_s_wrong_key_msg_and_truncated_sig() {
    let verifier = Ed25519Verifier;
    let key = keypair(4);
    let public = key.verify_key().as_bytes().to_vec();
    let preimage = signing_input(&POINTER_V2, b"malleable?");
    let signature = sign_preimage(&preimage, &key).as_bytes().to_vec();
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, &preimage, &signature),
        Ok(()),
        "the control: the genuine signature verifies"
    );

    // A malleable signature: S + L is a second encoding of the same signature. Strict verification
    // refuses it.
    let mut malleated = signature.clone();
    let mut carry = 0u16;
    for (s_byte, l_byte) in malleated[32..].iter_mut().zip(L_LE) {
        let sum = u16::from(*s_byte) + u16::from(l_byte) + carry;
        *s_byte = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    assert_eq!(carry, 0, "S < L, so S + L fits 256 bits");
    assert_ne!(malleated, signature);
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, &preimage, &malleated),
        Err(VerifyError::BadSignature),
        "S + L must not verify"
    );

    // Another key: a usable key, the wrong one.
    let other = keypair(5).verify_key().as_bytes().to_vec();
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &other, &preimage, &signature),
        Err(VerifyError::BadSignature)
    );
    // Another message, one byte longer and one byte shorter.
    let mut longer = preimage.clone();
    longer.push(0);
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, &longer, &signature),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        verifier.verify(
            SigAlg::Ed25519,
            &public,
            &preimage[..preimage.len() - 1],
            &signature
        ),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, b"", &signature),
        Err(VerifyError::BadSignature)
    );

    // Truncated, extended and empty signatures are malformed, whatever the key.
    for len in [0usize, 1, 32, 63] {
        assert_eq!(
            verifier.verify(SigAlg::Ed25519, &public, &preimage, &signature[..len]),
            Err(VerifyError::Malformed),
            "{len} bytes"
        );
    }
    let mut extended = signature.clone();
    extended.push(0);
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, &preimage, &extended),
        Err(VerifyError::Malformed),
        "65 bytes"
    );
    extended.extend_from_slice(&[0u8; 63]);
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public, &preimage, &extended),
        Err(VerifyError::Malformed),
        "128 bytes"
    );

    // A key of the wrong length is a bad key.
    for len in [0usize, 1, 31, 33, 64] {
        let wrong: Vec<u8> = public.iter().copied().cycle().take(len).collect();
        assert_eq!(
            verifier.verify(SigAlg::Ed25519, &wrong, &preimage, &signature),
            Err(VerifyError::BadKey),
            "{len}-byte key"
        );
    }
    // The key is checked before the signature's shape.
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &public[..31], &preimage, &signature[..10]),
        Err(VerifyError::BadKey)
    );

    // A 32-byte string that is no curve point is a bad key: the smallest `y` that has no `x`.
    let no_point = (2u8..=255)
        .map(|y| {
            let mut bytes = [0u8; 32];
            bytes[0] = y;
            bytes
        })
        .find(|bytes| ed25519_dalek::VerifyingKey::from_bytes(bytes).is_err())
        .expect("some small y is no point of the curve");
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &no_point, &preimage, &signature),
        Err(VerifyError::BadKey)
    );
    // A key of small order (the identity) is unusable too, even with the signature that a
    // permissive verifier accepts for every message (R = identity, S = 0).
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let mut trivial = [0u8; 64];
    trivial[0] = 1;
    assert_eq!(
        verifier.verify(SigAlg::Ed25519, &identity, &preimage, &trivial),
        Err(VerifyError::BadKey)
    );
    // Garbage never passes and never panics.
    let mut rng = Rng::new(0xBAD5);
    for _ in 0..500 {
        let k = rng.bytes(32);
        let s = rng.bytes(64);
        let n = rng.below(60).unwrap_or(0) as usize;
        let m = rng.bytes(n);
        assert!(verifier.verify(SigAlg::Ed25519, &k, &m, &s).is_err());
    }
}

#[test]
fn verifier_accepts_what_keys_sign_and_nothing_else() {
    // P-S17a-2 / C-01: the preimage is ALREADY `signing_input(domain, bytes)`, and what
    // `azlin_keys::sign::sign` makes verifies through the seam over exactly that input.
    let verifier = Ed25519Verifier;
    let mut rng = Rng::new(0x17A);
    let domains = [POINTER_V2, ENVELOPE_V2, LOG_RECORD_V1];
    for round in 0..20u8 {
        let key = keypair(round.wrapping_add(1));
        let public = key.verify_key().as_bytes().to_vec();
        let len = rng.below(90).unwrap_or(0) as usize;
        let message = rng.bytes(len);
        for domain in &domains {
            let signature = sign(domain, &message, &key).as_bytes().to_vec();
            let preimage = signing_input(domain, &message);
            assert_eq!(
                verifier.verify(SigAlg::Ed25519, &public, &preimage, &signature),
                Ok(()),
                "round {round}: {}",
                domain.as_str()
            );
            // Nothing else: another domain, the unframed message, a message with the domain text
            // inside, another message, another key.
            for other in &domains {
                if other != domain {
                    assert_eq!(
                        verifier.verify(
                            SigAlg::Ed25519,
                            &public,
                            &signing_input(other, &message),
                            &signature
                        ),
                        Err(VerifyError::BadSignature),
                        "a {} signature under {}",
                        domain.as_str(),
                        other.as_str()
                    );
                }
            }
            assert_eq!(
                verifier.verify(SigAlg::Ed25519, &public, &message, &signature),
                Err(VerifyError::BadSignature),
                "the unframed message is not the preimage"
            );
            let mut with_domain = domain.as_str().as_bytes().to_vec();
            with_domain.push(0);
            with_domain.extend_from_slice(&message);
            assert_eq!(
                verifier.verify(
                    SigAlg::Ed25519,
                    &public,
                    &signing_input(domain, &with_domain),
                    &signature
                ),
                Err(VerifyError::BadSignature),
                "a message that carries the framing is another message"
            );
            let mut changed = message.clone();
            changed.push(0xA5);
            assert_eq!(
                verifier.verify(
                    SigAlg::Ed25519,
                    &public,
                    &signing_input(domain, &changed),
                    &signature
                ),
                Err(VerifyError::BadSignature)
            );
            let other_key = keypair(round.wrapping_add(100)).verify_key();
            assert_eq!(
                verifier.verify(SigAlg::Ed25519, other_key.as_bytes(), &preimage, &signature),
                Err(VerifyError::BadSignature)
            );
        }
        // A signature of the RAW layer over unframed bytes verifies only over those bytes.
        let raw = sign_preimage(&message, &key).as_bytes().to_vec();
        assert_eq!(
            verifier.verify(SigAlg::Ed25519, &public, &message, &raw),
            Ok(())
        );
        assert_eq!(
            verifier.verify(
                SigAlg::Ed25519,
                &public,
                &signing_input(&POINTER_V2, &message),
                &raw
            ),
            Err(VerifyError::BadSignature)
        );
    }
    // The verifier is the key type's own check: a VerifyKey built from the bytes agrees.
    let key = keypair(9);
    let vk = VerifyKey::from_bytes(*key.verify_key().as_bytes());
    let preimage = signing_input(&POINTER_V2, b"x");
    let signature = sign(&POINTER_V2, b"x", &key);
    assert_eq!(
        azlin_keys::sign::verify(&POINTER_V2, b"x", &vk, &signature),
        Ok(())
    );
    assert_eq!(
        verifier.verify(
            SigAlg::Ed25519,
            vk.as_bytes(),
            &preimage,
            signature.as_bytes()
        ),
        Ok(())
    );
}

#[test]
fn fake_scheme_verifies_only_its_own_signatures() {
    let fake = FakeScheme;
    let key_a = KeyId::from_bytes([0xA1; 32]);
    let key_b = KeyId::from_bytes([0xB2; 32]);
    let preimage = signing_input(&POINTER_V2, b"a head");
    let signature = FakeScheme::sign(&key_a, &preimage);
    assert_eq!(signature.len(), FakeScheme::SIG_LEN);
    assert_eq!(FakeScheme::public_key(&key_a), [0xA1; 32]);

    // Its own signature verifies, and signing is deterministic.
    let public_a = FakeScheme::public_key(&key_a);
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &public_a, &preimage, &signature),
        Ok(())
    );
    assert_eq!(FakeScheme::sign(&key_a, &preimage), signature);
    // Another key, another preimage, a flipped bit, a truncated or extended signature.
    assert_ne!(FakeScheme::sign(&key_b, &preimage), signature);
    assert_eq!(
        fake.verify(
            SigAlg::Ed25519,
            &FakeScheme::public_key(&key_b),
            &preimage,
            &signature
        ),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &public_a, b"another preimage", &signature),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &public_a, b"", &signature),
        Err(VerifyError::BadSignature)
    );
    for bit in 0..signature.len() * 8 {
        let mut bad = signature.clone();
        bad[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            fake.verify(SigAlg::Ed25519, &public_a, &preimage, &bad),
            Err(VerifyError::BadSignature),
            "bit {bit}"
        );
    }
    for len in [0usize, 1, 31] {
        assert_eq!(
            fake.verify(SigAlg::Ed25519, &public_a, &preimage, &signature[..len]),
            Err(VerifyError::Malformed)
        );
    }
    let mut longer = signature.clone();
    longer.push(0);
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &public_a, &preimage, &longer),
        Err(VerifyError::Malformed)
    );
    // A key of the wrong length.
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &public_a[..31], &preimage, &signature),
        Err(VerifyError::BadKey)
    );

    // It verifies ONLY its own: a real Ed25519 signature is refused (wrong shape), and the fake's
    // signature is refused by the real verifier (wrong shape), whatever the key.
    let real_key = keypair(1);
    let real_sig = sign_preimage(&preimage, &real_key).as_bytes().to_vec();
    assert_eq!(
        fake.verify(
            SigAlg::Ed25519,
            real_key.verify_key().as_bytes(),
            &preimage,
            &real_sig
        ),
        Err(VerifyError::Malformed)
    );
    assert_eq!(
        Ed25519Verifier.verify(SigAlg::Ed25519, &public_a, &preimage, &signature),
        Err(VerifyError::Malformed)
    );
    // The key is part of what is signed: a signature of key A is none of key B even for the same
    // preimage, over many random keys and preimages.
    let mut rng = Rng::new(0xFA4E);
    for _ in 0..200 {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        rng.fill_bytes(&mut a);
        rng.fill_bytes(&mut b);
        let n = rng.below(50).unwrap_or(0) as usize;
        let m = rng.bytes(n);
        let sig = FakeScheme::sign(&KeyId::from_bytes(a), &m);
        assert_eq!(fake.verify(SigAlg::Ed25519, &a, &m, &sig), Ok(()));
        if a != b {
            assert!(fake.verify(SigAlg::Ed25519, &b, &m, &sig).is_err());
        }
    }
}

#[test]
fn unsupported_alg_is_an_error_not_a_pass() {
    // ML-DSA has no vetted crate in either lock and ECDSA P-256 belongs to WebAuthn (S17d): both
    // fail closed, even when the key and the signature are a perfectly good Ed25519 pair (the
    // label alone decides, there is no fallback to Ed25519) and even when they are garbage.
    let key = keypair(3);
    let public = key.verify_key().as_bytes().to_vec();
    let preimage = signing_input(&POINTER_V2, b"a head");
    let real_sig = sign_preimage(&preimage, &key).as_bytes().to_vec();
    let fake_key = KeyId::from_bytes([7; 32]);
    let fake_public = FakeScheme::public_key(&fake_key).to_vec();
    let fake_sig = FakeScheme::sign(&fake_key, &preimage);

    let real = Ed25519Verifier;
    let fake = FakeScheme;
    for alg in [SigAlg::MlDsa65, SigAlg::EcdsaP256] {
        assert_eq!(
            real.verify(alg, &public, &preimage, &real_sig),
            Err(VerifyError::Unsupported(alg)),
            "{alg:?} with a good Ed25519 pair"
        );
        assert_eq!(
            real.verify(alg, b"", b"", b""),
            Err(VerifyError::Unsupported(alg)),
            "{alg:?} with nothing"
        );
        assert_eq!(
            fake.verify(alg, &fake_public, &preimage, &fake_sig),
            Err(VerifyError::Unsupported(alg)),
            "{alg:?} with a good fake pair"
        );
        assert_eq!(
            fake.verify(alg, b"", b"", b""),
            Err(VerifyError::Unsupported(alg))
        );
    }
    // The error carries the algorithm and says so.
    assert_eq!(
        VerifyError::Unsupported(SigAlg::MlDsa65).to_string(),
        "the signature algorithm MlDsa65 is not supported here"
    );
    // The control: the same pairs pass with the right label.
    assert_eq!(
        real.verify(SigAlg::Ed25519, &public, &preimage, &real_sig),
        Ok(())
    );
    assert_eq!(
        fake.verify(SigAlg::Ed25519, &fake_public, &preimage, &fake_sig),
        Ok(())
    );
    // Both are usable behind the trait object, which is how the pure crates hold them.
    let verifiers: [&dyn SignatureVerifier; 2] = [&real, &fake];
    assert_eq!(
        verifiers
            .iter()
            .filter(|v| v.verify(SigAlg::MlDsa65, b"k", b"m", b"s").is_err())
            .count(),
        2
    );
}
