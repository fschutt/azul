//! Unit tests of the recovery module that need its private functions: the ORDER of the checks of
//! every unwrap (nothing is derived before the cheap checks), the recombination of a tampered
//! share, the symbol arithmetic of the recovery code's text, the layout of its groups and the slot
//! ids of the algorithm 3 and 4 wraps. The behaviour a caller sees is tested in `tests/recovery_*`.
//!
//! Implements planning/architecture.md section 5.5 (checks C2, C4 and C7 of the brief).

use core::cell::Cell;
use std::sync::OnceLock;

use super::code::{decode_symbol, group_of, normalise_char, symbol, RecoveryCode};
use super::error::RecoveryError;
use super::open::slot_rcpt;
use super::passphrase::{
    unwrap_code_with, wrap_code_with_params, KdfFloor, KdfParams, KdfProfile, MAX_KDF_LANES,
};
use super::prf::wrap_with_prf;
use super::shamir::{combine, interpolate, split};
use crate::envelope::{Alg, Ctx, Envelope, OpenError, Params, RecoveryMethod};
use crate::ids::{DeviceId, MemberId};
use crate::seam::SeededEntropy;
use crate::secret::Secret;
use crate::sign::{MapDirectory, SignKeypair, Signature, VerifyAt};

/// One wrap of a member key under a code (Test profile: made once for all the tests), with the
/// device that signed it and a directory that knows the device.
struct Fixture {
    code: RecoveryCode,
    member: MemberId,
    ctx: Ctx,
    device: DeviceId,
    key: SignKeypair,
    dir: MapDirectory,
    env: Envelope,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mut ent = SeededEntropy::from_u64(41);
        let member = MemberId::generate(&mut ent).expect("an id");
        let device = DeviceId::generate(&mut ent).expect("an id");
        let key = SignKeypair::generate(&mut ent).expect("a key");
        let mut dir = MapDirectory::new();
        dir.insert_active(device, key.verify_key());
        let code = RecoveryCode::generate(&mut ent).expect("a code");
        let ctx = Ctx::recovery(&member, RecoveryMethod::Code);
        let env = wrap_code_with_params(
            &[7u8; 32],
            &code,
            &ctx,
            KdfProfile::Test.params(),
            &device,
            &key,
            &mut ent,
        )
        .expect("a wrap");
        Fixture {
            code,
            member,
            ctx,
            device,
            key,
            dir,
            env,
        }
    })
}

/// `unwrap_with_code` with a derivation that counts its calls and gives a zero key.
fn run(
    env: &Envelope,
    expect: &Ctx,
    dir: &MapDirectory,
    floor: KdfFloor,
    calls: &Cell<u32>,
) -> Result<Vec<u8>, RecoveryError> {
    unwrap_code_with(env, expect, dir, VerifyAt::Now, floor, |_salt, _params| {
        calls.set(calls.get() + 1);
        Ok(Secret::from_bytes([0u8; 32]))
    })
    .map(|plaintext| plaintext.to_vec())
}

/// C4 [M]: a cost below the floor, a cost above the limits, another context, another algorithm, a
/// signature that does not verify, an unknown signer and a directory that cannot answer are each
/// refused WITHOUT the key derivation ever running; only an envelope that passes every check
/// reaches it, once.
#[test]
fn c4_nothing_is_derived_before_the_cheap_checks() {
    let f = fixture();
    let calls = Cell::new(0u32);

    // The wrap is made under the Test profile: the production floor refuses it.
    assert_eq!(
        run(&f.env, &f.ctx, &f.dir, KdfFloor::PRODUCTION, &calls),
        Err(RecoveryError::BelowFloor)
    );
    // Fewer passes than the floor, memory enough.
    let one_pass_more = KdfFloor::new(KdfProfile::Test.params().memory, 2);
    assert_eq!(
        run(&f.env, &f.ctx, &f.dir, one_pass_more, &calls),
        Err(RecoveryError::BelowFloor)
    );

    // Every limit on its own (the signature no longer verifies: the limits come first).
    for (m_kib, t, p) in [
        (4_194_304u32, 1u32, 2u32),
        (8_192, 1_000, 2),
        (1_048_576, 5, 2),
        (8_192, 1, MAX_KDF_LANES + 1),
        (1_048_577, 1, 2),
        (8_192, 9, 2),
    ] {
        let mut costly = f.env.clone();
        costly.params = Params::Argon2id {
            salt: [0u8; 16],
            m_kib,
            t,
            p,
        };
        assert_eq!(
            run(&costly, &f.ctx, &f.dir, KdfFloor::TEST, &calls),
            Err(RecoveryError::AboveLimit),
            "{m_kib} KiB, {t} passes, {p} lanes"
        );
    }

    // Another member's context, and a context that is not the code's.
    let other = Ctx::recovery(&MemberId::from_bytes([9u8; 16]), RecoveryMethod::Code);
    assert_eq!(
        run(&f.env, &other, &f.dir, KdfFloor::TEST, &calls),
        Err(RecoveryError::Open(OpenError::CtxMismatch))
    );
    let not_the_code = Ctx::recovery(&f.member, RecoveryMethod::Prf);
    assert_eq!(
        run(&f.env, &not_the_code, &f.dir, KdfFloor::TEST, &calls),
        Err(RecoveryError::WrongCtx)
    );

    // Another algorithm.
    let mut ent = SeededEntropy::from_u64(3);
    let prf_ctx = Ctx::recovery(&f.member, RecoveryMethod::Prf);
    let prf_env = wrap_with_prf(
        &Secret::<32>::from_bytes([1u8; 32]),
        &Secret::<32>::from_bytes([2u8; 32]),
        &[3u8; 32],
        &[4u8; 32],
        &prf_ctx,
        &f.device,
        &f.key,
        &mut ent,
    )
    .expect("a passkey wrap");
    assert_eq!(prf_env.alg, Alg::Prf);
    assert_eq!(
        run(&prf_env, &f.ctx, &f.dir, KdfFloor::TEST, &calls),
        Err(RecoveryError::WrongAlg(Alg::Prf))
    );

    // The signature: a bit of it changed, an unknown signer, a directory that is down.
    let mut forged = f.env.clone();
    let mut sig = *forged.sig.as_bytes();
    sig[0] ^= 0x01;
    forged.sig = Signature::from_bytes(sig);
    assert_eq!(
        run(&forged, &f.ctx, &f.dir, KdfFloor::TEST, &calls),
        Err(RecoveryError::Open(OpenError::BadSignature))
    );
    assert_eq!(
        run(&f.env, &f.ctx, &MapDirectory::new(), KdfFloor::TEST, &calls),
        Err(RecoveryError::Open(OpenError::UnknownSigner))
    );
    let mut down = MapDirectory::new();
    down.set_unavailable(true);
    assert_eq!(
        run(&f.env, &f.ctx, &down, KdfFloor::TEST, &calls),
        Err(RecoveryError::Open(OpenError::DirectoryUnavailable))
    );

    assert_eq!(calls.get(), 0, "the key derivation ran before a check");

    // An envelope that passes every check reaches the derivation, exactly once (the zero key then
    // fails the cipher).
    assert_eq!(
        run(&f.env, &f.ctx, &f.dir, KdfFloor::TEST, &calls),
        Err(RecoveryError::Auth)
    );
    assert_eq!(calls.get(), 1);
}

/// C7 [M]: one changed byte of one share gives another code, whose check value is not the one the
/// shares carry and which does not open the wrap; `combine` says so instead of returning it.
#[test]
fn c7_a_tampered_share_recombines_to_another_check_value_and_the_wrap_does_not_open() {
    let f = fixture();
    let mut ent = SeededEntropy::from_u64(5);
    let mut shares = split(f.code.as_secret(), 2, 3, &mut ent).expect("a split");
    let pair = |shares: &[super::Share]| interpolate(&shares[1..]);
    // The honest recombination is the code.
    assert!(pair(shares.as_slice()).ct_eq(f.code.as_secret()));
    // One changed byte.
    shares[1].y.expose_mut()[3] ^= 0x40;
    let wrong = RecoveryCode::from_secret(pair(shares.as_slice()));
    assert_ne!(wrong.check_value(), f.code.check_value());
    assert_ne!(wrong.check_value(), shares[1].split_id);
    assert_eq!(
        super::passphrase::unwrap_with_code(
            &f.env,
            &wrong,
            &f.ctx,
            &f.dir,
            VerifyAt::Now,
            KdfFloor::TEST
        )
        .map(|plaintext| plaintext.to_vec()),
        Err(RecoveryError::Auth)
    );
    assert_eq!(
        combine(&shares[1..]).map(|secret| *secret.expose()),
        Err(RecoveryError::CheckMismatch)
    );
}

/// C1: the arithmetic that makes and reads a symbol is Crockford's alphabet, for all 256 byte
/// values and some characters above them.
#[test]
fn c1_the_symbol_arithmetic_is_the_crockford_alphabet() {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    for value in 0..32u8 {
        let want = ALPHABET[usize::from(value)];
        assert_eq!(symbol(value), want, "symbol {value}");
        assert_eq!(decode_symbol(char::from(want)), Some(value));
        assert_eq!(
            decode_symbol(char::from(want.to_ascii_lowercase())),
            Some(value)
        );
        assert_eq!(normalise_char(char::from(want)), want);
    }
    // The aliases of a person who types.
    for c in ['o', 'O'] {
        assert_eq!(decode_symbol(c), Some(0), "{c}");
        assert_eq!(normalise_char(c), b'0');
    }
    for c in ['i', 'I', 'l', 'L'] {
        assert_eq!(decode_symbol(c), Some(1), "{c}");
        assert_eq!(normalise_char(c), b'1');
    }
    // Every other byte value is refused: `U`, the punctuation, the controls, the Latin-1 letters.
    for byte in 0..=255u8 {
        let in_alphabet = ALPHABET.contains(&byte)
            || (byte.is_ascii_alphabetic() && ALPHABET.contains(&byte.to_ascii_uppercase()))
            || b"oOiIlL".contains(&byte);
        assert_eq!(
            decode_symbol(char::from(byte)).is_some(),
            in_alphabet,
            "byte {byte}"
        );
        if !in_alphabet {
            assert_eq!(normalise_char(char::from(byte)), 0, "byte {byte}");
        }
    }
    for c in ['é', '€', '\u{1d7d8}', '\u{ff10}'] {
        assert_eq!(decode_symbol(c), None, "{c}");
    }
}

/// C2: the five groups tile the 26 symbols in the order 5, 5, 5, 5, 6; position 0 and 6 are none.
#[test]
fn c2_the_groups_follow_the_5_5_5_5_6_layout() {
    let numbered: Vec<u8> = (0..26u8).collect();
    let lengths: Vec<usize> = (1..=5u8)
        .map(|position| group_of(&numbered, position).expect("a group").len())
        .collect();
    assert_eq!(lengths, [5, 5, 5, 5, 6]);
    assert!(group_of(&numbered, 0).is_none());
    assert!(group_of(&numbered, 6).is_none());
    assert!(group_of(&numbered, 255).is_none());
    let joined: Vec<u8> = (1..=5u8)
        .flat_map(|position| group_of(&numbered, position).expect("a group").to_vec())
        .collect();
    assert_eq!(joined, numbered);
    // Fewer symbols than 26: the last group is cut off, not made up.
    assert!(group_of(&numbered[..25], 5).is_none());
}

/// The slot id of an algorithm 3 or 4 wrap names the slot: it differs by algorithm, by context and
/// by the credential, and is the same for the same slot.
#[test]
fn c3_the_slot_ids_differ_by_algorithm_context_and_credential() {
    let member = MemberId::from_bytes([5u8; 16]);
    let other = MemberId::from_bytes([6u8; 16]);
    let code = Ctx::recovery(&member, RecoveryMethod::Code);
    let prf = Ctx::recovery(&member, RecoveryMethod::Prf);
    let code_of_other = Ctx::recovery(&other, RecoveryMethod::Code);
    let a = slot_rcpt(Alg::Argon2id, &code, &[]);
    assert_eq!(a, slot_rcpt(Alg::Argon2id, &code, &[]));
    assert_ne!(a, slot_rcpt(Alg::Argon2id, &code_of_other, &[]));
    assert_ne!(a, slot_rcpt(Alg::Prf, &code, &[]));
    let p = slot_rcpt(Alg::Prf, &prf, &[1u8; 32]);
    assert_ne!(p, slot_rcpt(Alg::Prf, &prf, &[2u8; 32]));
    assert_ne!(p, slot_rcpt(Alg::Prf, &prf, &[]));
    assert_ne!(p, a);
}

/// C4 [M]: the limits are inclusive and exact: 1 GiB, 8 passes, 8 lanes and 4 GiB-passes of work are
/// allowed, one more of any is not; the floor is "below", not "at or below".
#[test]
fn c4_the_limits_and_the_floor_are_exact() {
    use super::passphrase::{
        check_cost, check_limits, Kib, MAX_KDF_LANES, MAX_KDF_MEMORY, MAX_KDF_PASSES, MAX_KDF_WORK,
    };
    let params = |m: u32, t: u32, p: u32| KdfParams {
        memory: Kib::new(m),
        passes: t,
        lanes: p,
    };
    let limit = MAX_KDF_MEMORY.get();
    // At the limits: fine (one at a time, because memory times passes is limited too).
    assert_eq!(check_limits(&params(limit, 4, 8)), Ok(()));
    assert_eq!(check_limits(&params(limit / 8, MAX_KDF_PASSES, 8)), Ok(()));
    assert_eq!(u64::from(limit) * 4, MAX_KDF_WORK);
    // Eight passes over half the memory limit is exactly the work limit.
    assert_eq!(
        u64::from(limit / 2) * u64::from(MAX_KDF_PASSES),
        MAX_KDF_WORK
    );
    assert_eq!(check_limits(&params(limit / 2, MAX_KDF_PASSES, 1)), Ok(()));
    assert_eq!(check_limits(&params(8_192, 1, MAX_KDF_LANES)), Ok(()));
    // One more of anything.
    assert_eq!(
        check_limits(&params(limit + 1, 1, 1)),
        Err(RecoveryError::AboveLimit)
    );
    assert_eq!(
        check_limits(&params(8_192, MAX_KDF_PASSES + 1, 1)),
        Err(RecoveryError::AboveLimit)
    );
    assert_eq!(
        check_limits(&params(8_192, 1, MAX_KDF_LANES + 1)),
        Err(RecoveryError::AboveLimit)
    );
    assert_eq!(
        check_limits(&params(limit, 5, 1)),
        Err(RecoveryError::AboveLimit),
        "5 GiB-passes"
    );
    assert_eq!(
        check_limits(&params(limit / 2 + 1, MAX_KDF_PASSES, 1)),
        Err(RecoveryError::AboveLimit),
        "work over by a little"
    );
    // The work is the product, not either factor: 1 GiB x 4 passes is allowed, 256 MiB x 16 is
    // refused by the passes alone.
    assert_eq!(
        check_limits(&params(limit / 4, 16, 1)),
        Err(RecoveryError::AboveLimit)
    );
    // The floor: equal is not below; one less memory or one less pass is.
    let floor = KdfFloor::new(Kib::new(1_000), 2);
    assert_eq!(check_cost(&params(1_000, 2, 1), &floor), Ok(()));
    assert_eq!(
        check_cost(&params(999, 2, 1), &floor),
        Err(RecoveryError::BelowFloor)
    );
    assert_eq!(
        check_cost(&params(1_000, 1, 1), &floor),
        Err(RecoveryError::BelowFloor)
    );
    assert_eq!(check_cost(&params(5_000, 7, 1), &floor), Ok(()));
    // The limits come first: a cost above them is `AboveLimit` even when it is also below the floor.
    let high_floor = KdfFloor::new(Kib::new(limit), 8);
    assert_eq!(
        check_cost(&params(limit + 1, 1, 1), &high_floor),
        Err(RecoveryError::AboveLimit)
    );
}

/// C4 [M]: a wrap is not made above the limits: a cost with too many passes or too many lanes (and
/// so little memory that deriving with it would be cheap) is refused before the salt is drawn.
#[test]
fn c4_a_wrap_is_not_made_above_the_limits() {
    use super::passphrase::{Kib, MAX_KDF_PASSES};
    let f = fixture();
    for params in [
        KdfParams {
            memory: Kib::new(8),
            passes: MAX_KDF_PASSES + 1,
            lanes: 1,
        },
        KdfParams {
            memory: Kib::new(8 * (MAX_KDF_LANES + 1)),
            passes: 1,
            lanes: MAX_KDF_LANES + 1,
        },
    ] {
        let mut ent = SeededEntropy::from_u64(9);
        assert_eq!(
            wrap_code_with_params(&[1u8; 32], &f.code, &f.ctx, params, &f.device, &f.key, &mut ent)
                .err(),
            Some(RecoveryError::AboveLimit),
            "{params:?}"
        );
        assert_eq!(
            ent.drawn(),
            0,
            "nothing is drawn for a cost above the limits"
        );
    }
}
