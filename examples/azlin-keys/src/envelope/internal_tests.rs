//! Unit tests that need the private pieces of the envelope module (check C3, the second half:
//! "the AEAD alone also refuses a changed header field").
//!
//! `open` checks the signature before it decrypts, and the signature covers every field, so a
//! flipped bit is usually refused by the signature before the cipher sees it. These tests call
//! the decryption step alone (`decrypt`, the last step of `open`) and the AEAD with a recomputed
//! associated data, to show that the cipher binds `epk`, `kem_ct`, `nonce`, `ct`, `ctx`, `rcpt`,
//! `signer` and `alg` on its own, whatever the signature does. They run with the crate's own
//! test doubles (`SeededEntropy`, `MapDirectory`), which exist under `cfg(test)`.

use zeroize::Zeroizing;

use super::combiner::{alg1_key, Alg1Input};
use super::opening::{decrypt, open_without_signature_for_test};
use super::x25519::dh;
use super::*;
use crate::ids::{DeviceId, Epoch, GroupId, KeyId, MemberId, SpaceId};
use crate::seam::SeededEntropy;
use crate::secret::Secret;
use crate::sign::{MapDirectory, SignKeypair, VerifyAt};

const PLAINTEXT: &[u8] = b"thirty-two bytes of key material";

fn flip(bytes: &mut [u8], bit: usize) {
    bytes[bit / 8] ^= 1 << (bit % 8);
}

struct Parts {
    me: RecipientSecret,
    ctx: Ctx,
    device: DeviceId,
    signer: SignKeypair,
}

fn parts() -> Parts {
    Parts {
        me: RecipientSecret::new(
            &Secret::from_bytes([0x21; 32]),
            Some(&Secret::from_bytes([0x42; 64])),
        ),
        ctx: Ctx::space_epoch(&SpaceId::from_bytes([0xab; 16]), Epoch::new(3).unwrap()),
        device: DeviceId::from_bytes([0xd1; 16]),
        signer: SignKeypair::from_seed(&Secret::from_bytes([0x51; 32])),
    }
}

/// What the AEAD alone is allowed to answer for a changed envelope: it never opens it.
fn refused(result: Result<Zeroizing<Vec<u8>>, OpenError>) -> bool {
    matches!(
        result,
        Err(OpenError::Auth | OpenError::NonContributory | OpenError::Malformed(_))
    )
}

/// Every other context a changed `ctx` could be: another epoch, another space, another kind.
fn other_contexts(of: &Ctx) -> Vec<Ctx> {
    let space = SpaceId::from_bytes([0xab; 16]);
    let others = vec![
        Ctx::space_epoch(&space, Epoch::new(4).unwrap()),
        Ctx::space_epoch(&SpaceId::from_bytes([0xac; 16]), Epoch::new(3).unwrap()),
        Ctx::group_epoch(&GroupId::from_bytes([0xab; 16]), Epoch::new(3).unwrap()),
        Ctx::chain(&space, Epoch::new(3).unwrap()),
        Ctx::member_key(&MemberId::from_bytes([0xab; 16])),
    ];
    assert!(others.iter().all(|c| c != of));
    others
}

#[test]
fn c3_the_aead_alone_binds_every_header_field_of_alg1() {
    let p = parts();
    let mut ent = SeededEntropy::from_u64(1);
    let env = wrap_x25519(
        &p.me.recipient_x25519(),
        &p.ctx,
        PLAINTEXT,
        &p.device,
        &p.signer,
        &mut ent,
    )
    .unwrap();
    // The control: the unchanged envelope opens through the decryption step alone.
    assert_eq!(decrypt(&env, &p.me).unwrap().as_slice(), PLAINTEXT);

    for bit in 0..256 {
        let mut e = env.clone();
        let mut epk = e.epk.unwrap();
        flip(&mut epk, bit);
        e.epk = Some(epk);
        assert!(refused(decrypt(&e, &p.me)), "epk bit {bit}");

        let mut e = env.clone();
        let mut rcpt = *e.rcpt.as_bytes();
        flip(&mut rcpt, bit);
        e.rcpt = KeyId::from_bytes(rcpt);
        assert!(refused(decrypt(&e, &p.me)), "rcpt bit {bit}");
    }
    for bit in 0..192 {
        let mut e = env.clone();
        flip(&mut e.nonce, bit);
        assert!(refused(decrypt(&e, &p.me)), "nonce bit {bit}");
    }
    for bit in 0..env.ct.len() * 8 {
        let mut e = env.clone();
        flip(&mut e.ct, bit);
        assert!(refused(decrypt(&e, &p.me)), "ct bit {bit}");
    }
    for bit in 0..128 {
        let mut e = env.clone();
        let mut signer = *e.signer.as_bytes();
        flip(&mut signer, bit);
        e.signer = DeviceId::from_bytes(signer);
        assert!(refused(decrypt(&e, &p.me)), "signer bit {bit}");
    }
    for other in other_contexts(&env.ctx) {
        let mut e = env.clone();
        e.ctx = other;
        assert!(refused(decrypt(&e, &p.me)), "ctx {}", e.ctx);
    }
}

#[test]
fn c3_the_aead_binds_the_algorithm_number() {
    let p = parts();
    let mut ent = SeededEntropy::from_u64(2);
    let env = wrap_x25519(
        &p.me.recipient_x25519(),
        &p.ctx,
        PLAINTEXT,
        &p.device,
        &p.signer,
        &mut ent,
    )
    .unwrap();
    // The key as the sender derived it, then the AEAD with the associated data of the true
    // header (it opens) and of the same header under every other algorithm number (it does not).
    let epk = env.epk.unwrap();
    let shared = dh(p.me.kx(), &epk).unwrap();
    let key = alg1_key(&Alg1Input {
        shared: &shared,
        epk: &epk,
        rcpt_kx: p.me.kx_public(),
    });
    let header = Header {
        alg: Alg::X25519SealedBox,
        rcpt: env.rcpt,
        epk: env.epk,
        kem_ct: None,
        params: Params::None,
        ctx: env.ctx.clone(),
        signer: env.signer,
    };
    let ad = header.header_ad().unwrap();
    assert_eq!(ad, env.header_ad().unwrap());
    assert!(open_under(&key, &ad, &env.nonce, &env.ct).is_ok());
    for alg in Alg::ALL {
        if alg == Alg::X25519SealedBox {
            continue;
        }
        let mut changed = header.clone();
        changed.alg = alg;
        let other_ad = changed.header_ad_unchecked_for_test();
        assert_ne!(
            ad,
            other_ad,
            "alg {} gives the same associated data",
            alg.number()
        );
        assert_eq!(
            open_under(&key, &other_ad, &env.nonce, &env.ct).err(),
            Some(OpenError::Auth),
            "alg {}",
            alg.number()
        );
    }
}

#[cfg(feature = "pq")]
#[test]
fn c3_the_aead_alone_binds_every_header_field_of_alg2() {
    let p = parts();
    let mut ent = SeededEntropy::from_u64(3);
    let recipient =
        p.me.recipient_hybrid()
            .expect("a recipient with an ML-KEM key");
    let env = wrap_hybrid(
        &recipient, &p.ctx, PLAINTEXT, &p.device, &p.signer, &mut ent,
    )
    .unwrap();
    assert_eq!(decrypt(&env, &p.me).unwrap().as_slice(), PLAINTEXT);

    for bit in 0..256 {
        let mut e = env.clone();
        let mut epk = e.epk.unwrap();
        flip(&mut epk, bit);
        e.epk = Some(epk);
        assert!(refused(decrypt(&e, &p.me)), "epk bit {bit}");

        let mut e = env.clone();
        let mut rcpt = *e.rcpt.as_bytes();
        flip(&mut rcpt, bit);
        e.rcpt = KeyId::from_bytes(rcpt);
        assert!(refused(decrypt(&e, &p.me)), "rcpt bit {bit}");
    }
    for bit in 0..192 {
        let mut e = env.clone();
        flip(&mut e.nonce, bit);
        assert!(refused(decrypt(&e, &p.me)), "nonce bit {bit}");
    }
    for bit in 0..env.ct.len() * 8 {
        let mut e = env.clone();
        flip(&mut e.ct, bit);
        assert!(refused(decrypt(&e, &p.me)), "ct bit {bit}");
    }
    for bit in 0..128 {
        let mut e = env.clone();
        let mut signer = *e.signer.as_bytes();
        flip(&mut signer, bit);
        e.signer = DeviceId::from_bytes(signer);
        assert!(refused(decrypt(&e, &p.me)), "signer bit {bit}");
    }
    for other in other_contexts(&env.ctx) {
        let mut e = env.clone();
        e.ctx = other;
        assert!(refused(decrypt(&e, &p.me)), "ctx {}", e.ctx);
    }
    // The ML-KEM ciphertext: one bit in each of its 1,088 bytes (every bit position of a byte
    // is hit 136 times). ML-KEM rejects implicitly, so the key the combiner gets is another one
    // and the AEAD fails: `Auth`, never a distinct error and never a panic.
    let kem_len = env.kem_ct.as_ref().unwrap().len();
    assert_eq!(kem_len, KEM_CT_LEN);
    for byte in 0..kem_len {
        let mut e = env.clone();
        let kem_ct = e.kem_ct.as_mut().unwrap();
        flip(kem_ct, byte * 8 + byte % 8);
        assert_eq!(
            decrypt(&e, &p.me).err(),
            Some(OpenError::Auth),
            "kem_ct byte {byte}"
        );
    }
}

/// The hook is the real `open` without the one step it leaves out: with the signature zeroed the
/// hook opens and `open` refuses with `BadSignature`, so the hook shows what the AEAD does alone
/// and nothing else.
#[test]
fn c3_the_hook_differs_from_open_by_the_signature_check_alone() {
    let p = parts();
    let mut ent = SeededEntropy::from_u64(4);
    let mut env = wrap_x25519(
        &p.me.recipient_x25519(),
        &p.ctx,
        PLAINTEXT,
        &p.device,
        &p.signer,
        &mut ent,
    )
    .unwrap();
    let mut dir = MapDirectory::new();
    dir.insert_active(p.device, p.signer.verify_key());
    assert_eq!(
        open(&env, &p.me, &p.ctx, &dir, VerifyAt::Now)
            .unwrap()
            .as_slice(),
        PLAINTEXT
    );
    env.sig = crate::sign::Signature::from_bytes([0u8; 64]);
    assert_eq!(
        open(&env, &p.me, &p.ctx, &dir, VerifyAt::Now).err(),
        Some(OpenError::BadSignature)
    );
    assert_eq!(
        open_without_signature_for_test(&env, &p.me, &p.ctx)
            .unwrap()
            .as_slice(),
        PLAINTEXT
    );
    // The other checks are still in the hook: a wrong context and a wrong recipient.
    let other = Ctx::space_epoch(&SpaceId::from_bytes([0xab; 16]), Epoch::new(4).unwrap());
    assert_eq!(
        open_without_signature_for_test(&env, &p.me, &other).err(),
        Some(OpenError::CtxMismatch)
    );
    let stranger = RecipientSecret::new(&Secret::from_bytes([0x22; 32]), None);
    assert_eq!(
        open_without_signature_for_test(&env, &stranger, &p.ctx).err(),
        Some(OpenError::NotForMe)
    );
}

/// The twelve encodings of the points of small order on Curve25519 (the same list as
/// `tests/envelope_authenticity.rs`, which checks them with an independent ladder).
const LOW_ORDER: [&str; 12] = [
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0100000000000000000000000000000000000000000000000000000000000000",
    "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
    "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
    "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "0000000000000000000000000000000000000000000000000000000000000080",
    "0100000000000000000000000000000000000000000000000000000000000080",
    "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b880",
    "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f11d7",
    "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
];

fn point(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

/// An all-zero shared secret never keys anything: `dh` refuses every point of low order for
/// every secret, and still gives a secret for an ordinary point (so the refusal is not vacuous).
#[test]
fn c7_dh_refuses_every_low_order_point_for_every_secret() {
    for seed in [0x00u8, 0x21, 0x7f, 0xff] {
        let secret = Secret::<32>::from_bytes([seed; 32]);
        for hex in LOW_ORDER {
            assert!(
                dh(&secret, &point(hex)).is_none(),
                "seed {seed:#x}, point {hex}"
            );
        }
        let mut basepoint = [0u8; 32];
        basepoint[0] = 9;
        let shared = dh(&secret, &basepoint).expect("an ordinary point is contributory");
        assert_ne!(shared.expose(), &[0u8; 32]);
        // The public key of the secret is an ordinary point.
        let public = super::x25519::public_of(&secret);
        assert!(dh(&secret, &public).is_some());
    }
}
