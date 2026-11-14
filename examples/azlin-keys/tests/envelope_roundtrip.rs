//! C1 (wrap then open for algorithms 1 and 2), C9 (entropy discipline), C10 (the hybrid binds both
//! halves), C13 (the AEAD layer other algorithms reuse), and the two seams other lanes build on:
//! `seal_with_key`/`open_with_key` (W1-S15d) and `Recipient::wrap` (the P-6 default algorithm).
//!
//! Implements planning/architecture.md section 5.5 (Envelope v2, "every wrap").
//! Run with `--features test-support` (the test doubles) and, for algorithm 2, `pq`.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code)]

mod envelope_common;

use std::collections::HashSet;

use azlin_keys::derive::ENVELOPE_ALG2_KEY;
use azlin_keys::envelope::combiner::{combine, Transcript};
use azlin_keys::envelope::{
    open_under, open_with_key, seal_under, seal_with_key, wrap_hybrid, wrap_x25519, Alg, Ctx,
    Envelope, Header, OpenError, Params, Recipient, RecipientHybrid, RecipientX25519,
    RecoveryMethod, WrapError, KEM_CT_LEN, MAX_PLAINTEXT, NONCE_LEN, PQ_KEY_LEN, TAG_LEN,
};
use azlin_keys::{
    Entropy, EntropyError, FailingEntropy, MemberId, Secret, SeededEntropy, VerifyAt,
};
use envelope_common::{pat, pat16, pat32, Fixture, ScriptedEntropy};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

const SIZES: [usize; 7] = [0, 1, 31, 32, 64, 1000, 65536];

fn plaintext_of(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| i.wrapping_mul(7).wrapping_add(3) as u8)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// C1
// ---------------------------------------------------------------------------------------------

#[test]
fn c1_alg1_roundtrip_for_every_size() {
    let f = Fixture::new(1);
    for (i, &size) in SIZES.iter().enumerate() {
        let plaintext = plaintext_of(size);
        let mut ent = SeededEntropy::from_u64(1000 + i as u64);
        let env = wrap_x25519(
            &f.me.recipient_x25519(),
            &f.ctx,
            &plaintext,
            &f.device,
            &f.signer,
            &mut ent,
        )
        .expect("a wrap of algorithm 1");
        assert_eq!(env.alg, Alg::X25519SealedBox);
        assert!(env.epk.is_some() && env.kem_ct.is_none());
        assert_eq!(env.rcpt, f.me.x25519_id());
        assert_eq!(env.ct.len(), size + TAG_LEN, "ciphertext = plaintext + tag");
        assert_eq!(
            ent.drawn(),
            32 + NONCE_LEN,
            "the ephemeral key, then the nonce"
        );

        // `open` returns the plaintext in a zeroizing buffer (the type is part of the check).
        let opened: Zeroizing<Vec<u8>> = f.open(&env).expect("the envelope opens");
        assert_eq!(opened.as_slice(), plaintext.as_slice(), "size {size}");

        // The same through the bytes of the wire.
        let bytes = env.to_cbor().expect("the bytes");
        let again = Envelope::from_cbor(&bytes).expect("the bytes read back");
        assert_eq!(again, env, "size {size}");
        assert_eq!(
            f.open(&again).expect("opens").as_slice(),
            plaintext.as_slice()
        );
    }
}

#[cfg(feature = "pq")]
#[test]
fn c1_alg2_roundtrip_for_every_size() {
    let f = Fixture::new(2);
    for (i, &size) in SIZES.iter().enumerate() {
        let plaintext = plaintext_of(size);
        let mut ent = SeededEntropy::from_u64(2000 + i as u64);
        let env = wrap_hybrid(
            &f.me.recipient_hybrid().expect("an ML-KEM key"),
            &f.ctx,
            &plaintext,
            &f.device,
            &f.signer,
            &mut ent,
        )
        .expect("a wrap of algorithm 2");
        assert_eq!(env.alg, Alg::X25519MlKem768);
        assert!(env.epk.is_some());
        assert_eq!(env.kem_ct.as_ref().map(Vec::len), Some(KEM_CT_LEN));
        assert_eq!(env.rcpt, f.me.hybrid_id().expect("a hybrid id"));
        assert_eq!(env.ct.len(), size + TAG_LEN);
        assert_eq!(
            ent.drawn(),
            32 + 32 + NONCE_LEN,
            "the ephemeral key, the ML-KEM message, then the nonce"
        );

        let opened: Zeroizing<Vec<u8>> = f.open(&env).expect("the envelope opens");
        assert_eq!(opened.as_slice(), plaintext.as_slice(), "size {size}");

        let bytes = env.to_cbor().expect("the bytes");
        let again = Envelope::from_cbor(&bytes).expect("the bytes read back");
        assert_eq!(again, env, "size {size}");
        assert_eq!(
            f.open(&again).expect("opens").as_slice(),
            plaintext.as_slice()
        );
    }
}

/// Without the feature `pq` algorithm 2 is `Unsupported`, before any entropy is drawn, and an
/// envelope of algorithm 2 is `Unsupported` to open: there is no silent fallback to algorithm 1.
#[cfg(not(feature = "pq"))]
#[test]
fn c1_alg2_is_unsupported_without_the_feature() {
    let f = Fixture::new(2);
    let recipient = RecipientHybrid {
        kx: pat32(0x31),
        pq: vec![0u8; PQ_KEY_LEN],
    };
    let mut ent = SeededEntropy::from_u64(1);
    let r = wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent);
    assert_eq!(r.err(), Some(WrapError::Unsupported(Alg::X25519MlKem768)));
    assert_eq!(ent.drawn(), 0, "refused before any draw");
    assert!(f.me.pq_public().is_none(), "no ML-KEM key can be computed");
    assert!(f.me.has_ml_kem_seed(), "but the recipient kept its seed");
    assert!(f.me.recipient_hybrid().is_none());
    assert_eq!(
        f.me.recipient(),
        Recipient::X25519(f.me.recipient_x25519()),
        "a recipient without a computable ML-KEM key is an X25519 recipient"
    );
}

// ---------------------------------------------------------------------------------------------
// C9: entropy discipline
// ---------------------------------------------------------------------------------------------

#[test]
fn c9_a_failing_entropy_source_makes_alg1_fail_and_make_no_envelope() {
    let f = Fixture::new(9);
    // 32 bytes for the ephemeral key and 24 for the nonce: every shorter supply fails.
    for good in 0..(32 + NONCE_LEN) {
        let mut ent = FailingEntropy::after(good);
        let r = wrap_x25519(
            &f.me.recipient_x25519(),
            &f.ctx,
            b"key",
            &f.device,
            &f.signer,
            &mut ent,
        );
        assert_eq!(
            r.err(),
            Some(WrapError::Entropy(EntropyError::Exhausted)),
            "{good} good bytes"
        );
    }
    let mut ent = FailingEntropy::after(32 + NONCE_LEN);
    assert!(wrap_x25519(
        &f.me.recipient_x25519(),
        &f.ctx,
        b"key",
        &f.device,
        &f.signer,
        &mut ent
    )
    .is_ok());
}

#[cfg(feature = "pq")]
#[test]
fn c9_a_failing_entropy_source_makes_alg2_fail_and_make_no_envelope() {
    let f = Fixture::new(9);
    let recipient = f.me.recipient_hybrid().expect("an ML-KEM key");
    for good in 0..(32 + 32 + NONCE_LEN) {
        let mut ent = FailingEntropy::after(good);
        let r = wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent);
        assert_eq!(
            r.err(),
            Some(WrapError::Entropy(EntropyError::Exhausted)),
            "{good} good bytes"
        );
    }
    let mut ent = FailingEntropy::after(32 + 32 + NONCE_LEN);
    assert!(wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent).is_ok());
}

#[test]
fn c9_every_wrap_draws_a_fresh_ephemeral_key_and_nonce() {
    let f = Fixture::new(10);
    let mut ent = SeededEntropy::from_u64(77);
    let mut epks = HashSet::new();
    let mut nonces = HashSet::new();
    for _ in 0..1000 {
        let env = wrap_x25519(
            &f.me.recipient_x25519(),
            &f.ctx,
            b"key",
            &f.device,
            &f.signer,
            &mut ent,
        )
        .expect("a wrap");
        assert!(epks.insert(env.epk.expect("an epk")), "an epk repeated");
        assert!(nonces.insert(env.nonce), "a nonce repeated");
    }
    assert_eq!(ent.drawn(), 1000 * (32 + NONCE_LEN));
}

#[cfg(feature = "pq")]
#[test]
fn c9_every_hybrid_wrap_draws_a_fresh_ephemeral_key_nonce_and_message() {
    let f = Fixture::new(10);
    let recipient = f.me.recipient_hybrid().expect("an ML-KEM key");
    let mut ent = SeededEntropy::from_u64(78);
    let mut epks = HashSet::new();
    let mut nonces = HashSet::new();
    let mut kems = HashSet::new();
    for _ in 0..100 {
        let env = wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent)
            .expect("a wrap");
        assert!(epks.insert(env.epk.expect("an epk")));
        assert!(nonces.insert(env.nonce));
        assert!(
            kems.insert(env.kem_ct.expect("a kem_ct")),
            "an ML-KEM ciphertext repeated"
        );
    }
    assert_eq!(ent.drawn(), 100 * (32 + 32 + NONCE_LEN));
}

/// The draws are exactly the ephemeral secret and then the nonce, each a buffer of its own: the
/// nonce in the envelope IS the second draw and the ephemeral key is the public key of the
/// first, so no draw is reused, split or stretched.
#[test]
fn c9_the_draws_are_the_ephemeral_secret_then_the_nonce_and_nothing_else() {
    let f = Fixture::new(11);
    let mut ent = ScriptedEntropy::new([pat(0x10, 32), pat(0x40, 24)].concat());
    let env = wrap_x25519(
        &f.me.recipient_x25519(),
        &f.ctx,
        b"key",
        &f.device,
        &f.signer,
        &mut ent,
    )
    .expect("a wrap");
    assert_eq!(ent.draws, vec![32, 24]);
    assert_eq!(ent.left(), 0);
    assert_eq!(env.nonce.to_vec(), pat(0x40, 24));
    let ephemeral = StaticSecret::from(pat32(0x10));
    assert_eq!(env.epk, Some(PublicKey::from(&ephemeral).to_bytes()));
}

#[cfg(feature = "pq")]
#[test]
fn c9_the_hybrid_draws_are_the_ephemeral_secret_the_ml_kem_message_then_the_nonce() {
    use ml_kem::{EncapsulateDeterministic, EncodedSizeUser, KemCore, MlKem768, B32};

    let f = Fixture::new(11);
    let mut ent = ScriptedEntropy::new([pat(0x10, 32), pat(0x30, 32), pat(0x40, 24)].concat());
    let recipient = f.me.recipient_hybrid().expect("an ML-KEM key");
    let env =
        wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent).expect("a wrap");
    assert_eq!(ent.draws, vec![32, 32, 24]);
    assert_eq!(ent.left(), 0);
    assert_eq!(env.nonce.to_vec(), pat(0x40, 24));
    let ephemeral = StaticSecret::from(pat32(0x10));
    assert_eq!(env.epk, Some(PublicKey::from(&ephemeral).to_bytes()));
    // The KEM ciphertext is the encapsulation of exactly the second draw to the recipient's key
    // (computed here with the ML-KEM crate directly, not through the envelope code).
    type Ek = <MlKem768 as KemCore>::EncapsulationKey;
    let encoded = <&ml_kem::Encoded<Ek>>::try_from(recipient.pq.as_slice()).expect("1184 bytes");
    let ek = Ek::from_bytes(encoded);
    let m = B32::from(pat32(0x30));
    let (ct, _shared) = ek.encapsulate_deterministic(&m).expect("encapsulates");
    assert_eq!(env.kem_ct.expect("a kem_ct"), ct.as_slice().to_vec());
}

// ---------------------------------------------------------------------------------------------
// C10: the hybrid binds both halves
// ---------------------------------------------------------------------------------------------

/// The combiner is the documented formula over the documented parts, and each part alone
/// changes the key.
#[test]
fn c10_the_combiner_binds_both_halves_and_every_public_value() {
    let ss_mlkem = Secret::<32>::from_bytes(pat32(0x01));
    let ss_x25519 = Secret::<32>::from_bytes(pat32(0x02));
    let kem_ct = pat(0x03, KEM_CT_LEN);
    let epk = pat32(0x04);
    let rcpt_kx = pat32(0x05);
    let pq = pat(0x06, PQ_KEY_LEN);
    let base = combine(
        &ss_mlkem,
        &ss_x25519,
        &Transcript {
            kem_ct: &kem_ct,
            epk: &epk,
            rcpt_kx: &rcpt_kx,
            pq: &pq,
        },
    );

    // The formula of the brief, computed by hand with BLAKE3.
    let mut material = Vec::new();
    material.extend(pat(0x01, 32));
    material.extend(pat(0x02, 32));
    material.extend(&kem_ct);
    material.extend(epk);
    material.extend(rcpt_kx);
    material.extend(blake3::hash(&pq).as_bytes());
    let want = blake3::derive_key(ENVELOPE_ALG2_KEY.as_str(), &material);
    assert_eq!(
        base.expose(),
        &want,
        "the combiner is the documented formula"
    );

    // Replacing ONE input changes the key.
    let other_secret = Secret::<32>::from_bytes(pat32(0x99));
    let mut other_kem_ct = kem_ct.clone();
    other_kem_ct[0] ^= 1;
    let mut other_epk = epk;
    other_epk[31] ^= 0x80;
    let mut other_kx = rcpt_kx;
    other_kx[0] ^= 1;
    let mut other_pq = pq.clone();
    other_pq[1183] ^= 1;
    let transcript = |kem_ct: &[u8],
                      epk: &[u8; 32],
                      kx: &[u8; 32],
                      pq: &[u8],
                      mlkem: &Secret<32>,
                      x: &Secret<32>| {
        combine(
            mlkem,
            x,
            &Transcript {
                kem_ct,
                epk,
                rcpt_kx: kx,
                pq,
            },
        )
    };
    let variants = [
        (
            "the ML-KEM half",
            transcript(&kem_ct, &epk, &rcpt_kx, &pq, &other_secret, &ss_x25519),
        ),
        (
            "the X25519 half",
            transcript(&kem_ct, &epk, &rcpt_kx, &pq, &ss_mlkem, &other_secret),
        ),
        (
            "the KEM ciphertext",
            transcript(&other_kem_ct, &epk, &rcpt_kx, &pq, &ss_mlkem, &ss_x25519),
        ),
        (
            "the ephemeral key",
            transcript(&kem_ct, &other_epk, &rcpt_kx, &pq, &ss_mlkem, &ss_x25519),
        ),
        (
            "the recipient's X25519 key",
            transcript(&kem_ct, &epk, &other_kx, &pq, &ss_mlkem, &ss_x25519),
        ),
        (
            "the recipient's ML-KEM key",
            transcript(&kem_ct, &epk, &rcpt_kx, &other_pq, &ss_mlkem, &ss_x25519),
        ),
        (
            "the two halves swapped",
            transcript(&kem_ct, &epk, &rcpt_kx, &pq, &ss_x25519, &ss_mlkem),
        ),
    ];
    for (what, key) in &variants {
        assert!(!key.ct_eq(&base), "changing {what} left the key unchanged");
    }
}

/// Corrupting the KEM ciphertext of a correctly signed envelope gives `Auth`: ML-KEM rejects
/// implicitly (a pseudorandom secret, no error and no panic), so the AEAD is what fails, with
/// the one word every AEAD failure has.
#[cfg(feature = "pq")]
#[test]
fn c10_a_corrupted_kem_ciphertext_is_auth_and_nothing_else() {
    let f = Fixture::new(12);
    let env = f.wrap2(b"a space key", 3);
    assert!(f.open(&env).is_ok());
    for position in [0usize, 1, 31, 32, 543, 544, 1086, 1087] {
        for bit in [0u8, 3, 7] {
            let mut bad = env.clone();
            bad.kem_ct.as_mut().expect("a kem_ct")[position] ^= 1 << bit;
            // Signed again by the legitimate device, so that only the cipher can refuse it.
            bad.sign(&f.signer).expect("signs");
            assert_eq!(
                f.open(&bad).err(),
                Some(OpenError::Auth),
                "kem_ct byte {position} bit {bit}"
            );
        }
    }
}

/// An envelope of algorithm 2 for a recipient without a valid ML-KEM key is refused AT WRAP TIME,
/// before any entropy is drawn.
#[cfg(feature = "pq")]
#[test]
fn c10_a_hybrid_recipient_without_a_valid_pq_key_is_refused_at_wrap_time() {
    let f = Fixture::new(13);
    let good = f.me.recipient_hybrid().expect("an ML-KEM key");
    let mut non_canonical = good.pq.clone();
    // Three bytes of 0xff are two 12-bit coefficients of 4095, above q = 3329.
    non_canonical[0] = 0xff;
    non_canonical[1] = 0xff;
    non_canonical[2] = 0xff;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("1183 bytes", good.pq[..PQ_KEY_LEN - 1].to_vec()),
        ("1185 bytes", [good.pq.clone(), vec![0]].concat()),
        ("all zero", vec![0u8; PQ_KEY_LEN]),
        ("non-canonical coefficients", non_canonical),
    ];
    for (what, pq) in cases {
        let mut ent = SeededEntropy::from_u64(1);
        let recipient = RecipientHybrid { kx: good.kx, pq };
        let r = wrap_hybrid(&recipient, &f.ctx, b"key", &f.device, &f.signer, &mut ent);
        // (An all-zero key is canonical: it is a valid, if useless, encapsulation key.)
        if what == "all zero" {
            assert!(r.is_ok(), "{what}");
        } else {
            assert_eq!(r.err(), Some(WrapError::BadRecipientKey), "{what}");
            assert_eq!(ent.drawn(), 0, "{what}: refused before any draw");
        }
    }
}

/// A recipient is its two keys together: another ML-KEM seed under the same X25519 key is
/// another `rcpt`, and an envelope of algorithm 2 is not for it.
#[cfg(feature = "pq")]
#[test]
fn c10_the_recipient_is_both_keys_together() {
    let f = Fixture::new(14);
    let env = f.wrap2(b"a space key", 4);
    let same_kx_other_pq = azlin_keys::envelope::RecipientSecret::new(
        &Secret::from_bytes(pat32(14)),
        Some(&Secret::from_bytes(envelope_common::pat64(0xc0))),
    );
    assert_eq!(same_kx_other_pq.kx_public(), f.me.kx_public());
    assert_ne!(same_kx_other_pq.hybrid_id(), f.me.hybrid_id());
    assert_eq!(
        azlin_keys::envelope::open(&env, &same_kx_other_pq, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::NotForMe)
    );
    // A recipient with no ML-KEM seed at all.
    let x25519_only =
        azlin_keys::envelope::RecipientSecret::new(&Secret::from_bytes(pat32(14)), None);
    assert!(x25519_only.hybrid_id().is_none() && x25519_only.recipient_hybrid().is_none());
    assert_eq!(
        azlin_keys::envelope::open(&env, &x25519_only, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::NotForMe)
    );
}

// ---------------------------------------------------------------------------------------------
// C13: the AEAD layer
// ---------------------------------------------------------------------------------------------

#[test]
fn c13_seal_under_and_open_under_roundtrip_and_bind_the_associated_data() {
    let key = Secret::<32>::from_bytes(pat32(0x42));
    let ad = b"the associated data";
    let mut ent = SeededEntropy::from_u64(5);
    let (nonce, ct) = seal_under(&key, ad, b"plaintext", &mut ent).expect("seals");
    assert_eq!(ent.drawn(), NONCE_LEN, "exactly one nonce is drawn");
    assert_eq!(ct.len(), b"plaintext".len() + TAG_LEN);

    // The nonce is the first 24 bytes of the seam's stream.
    let mut same_stream = SeededEntropy::from_u64(5);
    let mut expect = [0u8; NONCE_LEN];
    same_stream.fill(&mut expect).expect("draws");
    assert_eq!(nonce, expect, "the nonce comes from the seam");

    let opened = open_under(&key, ad, &nonce, &ct).expect("opens");
    assert_eq!(opened.as_slice(), b"plaintext");

    // A flipped byte of the associated data fails; so does a changed length.
    for i in 0..ad.len() {
        let mut bad = ad.to_vec();
        bad[i] ^= 1;
        assert_eq!(
            open_under(&key, &bad, &nonce, &ct).err(),
            Some(OpenError::Auth),
            "ad byte {i}"
        );
    }
    assert_eq!(
        open_under(&key, &ad[..ad.len() - 1], &nonce, &ct).err(),
        Some(OpenError::Auth)
    );
    assert_eq!(
        open_under(&key, b"", &nonce, &ct).err(),
        Some(OpenError::Auth)
    );
    // A flipped bit of the ciphertext, the nonce, or the key; a ciphertext too short for a tag.
    for bit in 0..ct.len() * 8 {
        let mut bad = ct.clone();
        bad[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            open_under(&key, ad, &nonce, &bad).err(),
            Some(OpenError::Auth),
            "ct bit {bit}"
        );
    }
    for bit in 0..NONCE_LEN * 8 {
        let mut bad = nonce;
        bad[bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            open_under(&key, ad, &bad, &ct).err(),
            Some(OpenError::Auth),
            "nonce bit {bit}"
        );
    }
    let wrong = Secret::<32>::from_bytes(pat32(0x43));
    assert_eq!(
        open_under(&wrong, ad, &nonce, &ct).err(),
        Some(OpenError::Auth)
    );
    assert_eq!(
        open_under(&key, ad, &nonce, &ct[..TAG_LEN - 1]).err(),
        Some(OpenError::Auth)
    );
    assert_eq!(
        open_under(&key, ad, &nonce, &[]).err(),
        Some(OpenError::Auth)
    );
}

#[test]
fn c13_seal_under_takes_its_nonce_from_the_seam_and_fails_without_one() {
    let key = Secret::<32>::from_bytes(pat32(0x42));
    for good in 0..NONCE_LEN {
        let mut ent = FailingEntropy::after(good);
        assert_eq!(
            seal_under(&key, b"ad", b"plaintext", &mut ent).err(),
            Some(WrapError::Entropy(EntropyError::Exhausted)),
            "{good} good bytes"
        );
    }
    // An empty plaintext is a ciphertext of one tag.
    let mut ent = SeededEntropy::from_u64(6);
    let (_, ct) = seal_under(&key, b"ad", b"", &mut ent).expect("seals");
    assert_eq!(ct.len(), TAG_LEN);
    // The limit is checked before any draw.
    let mut ent = SeededEntropy::from_u64(7);
    let too_big = vec![0u8; MAX_PLAINTEXT + 1];
    assert_eq!(
        seal_under(&key, b"ad", &too_big, &mut ent).err(),
        Some(WrapError::PlaintextTooLarge)
    );
    assert_eq!(ent.drawn(), 0);
    let at_limit = vec![0u8; MAX_PLAINTEXT];
    let mut ent = SeededEntropy::from_u64(7);
    assert!(seal_under(&key, b"ad", &at_limit, &mut ent).is_ok());
}

// ---------------------------------------------------------------------------------------------
// The seams of S15d (`seal_with_key`) and S15c1 (`Recipient::wrap`)
// ---------------------------------------------------------------------------------------------

fn argon2id_header(f: &Fixture) -> Header {
    Header {
        alg: Alg::Argon2id,
        rcpt: f.me.x25519_id(),
        epk: None,
        kem_ct: None,
        params: Params::Argon2id {
            salt: pat16(0x11),
            m_kib: 8192,
            t: 2,
            p: 1,
        },
        ctx: Ctx::recovery(&MemberId::from_bytes(pat16(0x72)), RecoveryMethod::Code),
        signer: f.device,
    }
}

#[test]
fn c13_seal_with_key_makes_signed_envelopes_that_bind_the_parameters() {
    let f = Fixture::new(15);
    let key = Secret::<32>::from_bytes(pat32(0xee));
    let header = argon2id_header(&f);
    let mut ent = SeededEntropy::from_u64(8);
    let env = seal_with_key(&header, &key, b"a member key", &f.signer, &mut ent).expect("seals");
    assert_eq!(env.alg, Alg::Argon2id);
    assert_eq!(env.params, header.params);
    assert!(env.epk.is_none() && env.kem_ct.is_none());
    assert_eq!(ent.drawn(), NONCE_LEN);
    // The header the lane built is the associated data the envelope carries.
    assert_eq!(
        env.header_ad().expect("ad"),
        header.header_ad().expect("ad")
    );

    let opened = open_with_key(&env, &key, &header.ctx, &f.dir, VerifyAt::Now).expect("opens");
    assert_eq!(opened.as_slice(), b"a member key");
    // Through the bytes of the wire.
    let again = Envelope::from_cbor(&env.to_cbor().expect("bytes")).expect("reads");
    assert_eq!(again, env);

    // A wrong key, a wrong context.
    let wrong = Secret::<32>::from_bytes(pat32(0xef));
    assert_eq!(
        open_with_key(&env, &wrong, &header.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::Auth)
    );
    let other_ctx = Ctx::recovery(&MemberId::from_bytes(pat16(0x72)), RecoveryMethod::Prf);
    assert_eq!(
        open_with_key(&env, &key, &other_ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::CtxMismatch)
    );
    // The parameters are signed AND part of the associated data: changed and signed again by the
    // legitimate device, the cipher alone refuses it (a lowered Argon2id cost is not a way in).
    let mut cheaper = env.clone();
    if let Params::Argon2id { t, .. } = &mut cheaper.params {
        *t = 1;
    }
    assert_eq!(
        open_with_key(&cheaper, &key, &header.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::BadSignature)
    );
    cheaper.sign(&f.signer).expect("signs");
    assert_eq!(
        open_with_key(&cheaper, &key, &header.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::Auth)
    );
}

#[test]
fn c13_the_keyed_functions_refuse_algorithms_1_and_2() {
    let f = Fixture::new(16);
    let key = Secret::<32>::from_bytes(pat32(0xee));
    for alg in [Alg::X25519SealedBox, Alg::X25519MlKem768] {
        let header = Header {
            alg,
            ..argon2id_header(&f)
        };
        let mut ent = SeededEntropy::from_u64(9);
        assert_eq!(
            seal_with_key(&header, &key, b"key", &f.signer, &mut ent).err(),
            Some(WrapError::WrongAlg(alg))
        );
        assert_eq!(ent.drawn(), 0);
    }
    let env = f.wrap1(b"key", 10);
    assert_eq!(
        open_with_key(&env, &key, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::WrongAlg(Alg::X25519SealedBox))
    );
}

/// P-6: the default algorithm is hybrid whenever the recipient has an ML-KEM key, and X25519
/// otherwise; a hybrid recipient is never silently downgraded.
#[test]
fn c13_recipient_wrap_chooses_the_algorithm_of_the_recipients_keys() {
    let f = Fixture::new(17);
    let x25519 = Recipient::from_public(*f.me.kx_public(), None);
    assert!(matches!(x25519, Recipient::X25519(_)));
    assert_eq!(x25519.key_id(), f.me.x25519_id());
    let mut ent = SeededEntropy::from_u64(11);
    let env = x25519
        .wrap(&f.ctx, b"key", &f.device, &f.signer, &mut ent)
        .expect("wraps");
    assert_eq!(env.alg, Alg::X25519SealedBox);
    assert_eq!(f.open(&env).expect("opens").as_slice(), b"key");

    // An empty ML-KEM key is no ML-KEM key.
    assert_eq!(
        Recipient::from_public(*f.me.kx_public(), Some(Vec::new())),
        x25519
    );

    #[cfg(feature = "pq")]
    {
        let hybrid = f.me.recipient();
        assert!(matches!(hybrid, Recipient::Hybrid(_)));
        assert_eq!(hybrid.key_id(), f.me.hybrid_id().expect("a hybrid id"));
        let mut ent = SeededEntropy::from_u64(12);
        let env = hybrid
            .wrap(&f.ctx, b"key", &f.device, &f.signer, &mut ent)
            .expect("wraps");
        assert_eq!(env.alg, Alg::X25519MlKem768);
        assert_eq!(f.open(&env).expect("opens").as_slice(), b"key");
    }
    #[cfg(not(feature = "pq"))]
    {
        // A recipient that HAS an ML-KEM key, in a build that cannot use it: refused.
        let hybrid = Recipient::from_public(*f.me.kx_public(), Some(vec![1u8; PQ_KEY_LEN]));
        assert!(matches!(hybrid, Recipient::Hybrid(_)));
        let mut ent = SeededEntropy::from_u64(12);
        assert_eq!(
            hybrid
                .wrap(&f.ctx, b"key", &f.device, &f.signer, &mut ent)
                .err(),
            Some(WrapError::Unsupported(Alg::X25519MlKem768))
        );
    }
}
