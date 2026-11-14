//! C3 (algorithm 3: the member key under the recovery code, Argon2id), C4 (the key derivation
//! floor and limits), C5 (algorithm 4: the passkey PRF) and C12 (the shape-only algorithms are not
//! opened here) of lane W1-S15d.
//!
//! Implements planning/architecture.md section 5.5 ("The recovery code ... and the passkey PRF (D51)
//! wrap the member key", "Envelope v2"). The known answers are rows of
//! `tests/vectors/recovery_wrap.json`, made by the independent program of the lane (Argon2id by
//! OpenSSL, the rest from the specifications); every test runs under `KdfProfile::Test`, and the
//! production cost is asserted by its numbers and never run.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code, clippy::needless_range_loop)]

mod recovery_common;

use azlin_keys::envelope::{check_shape, Alg, Ctx, Envelope, OpenError, Params, RecoveryMethod};
use azlin_keys::recovery::{
    new_prf_salt, open_share, prf_inputs, unwrap_with_code, unwrap_with_prf, wrap_with_code,
    wrap_with_prf, KdfFloor, KdfParams, KdfProfile, Kib, PrfInputs, RecoveryCode, RecoveryError,
    MAX_KDF_LANES, MAX_KDF_MEMORY, MAX_KDF_PASSES, MAX_KDF_WORK,
};
use azlin_keys::{EntropyError, FailingEntropy, MapDirectory, Secret, SeededEntropy, VerifyAt};
use recovery_common::vectors::{self, row};
use recovery_common::{
    from_hex, kat_code, kat_contact_secret, kat_ctx, kat_device, kat_directory, kat_member,
    kat_other_member, kat_signer, pat, pat16, pat32, to_hex, word, ScriptedEntropy,
};

// ---------------------------------------------------------------------------------------------
// The known-answer cases (the inputs are patterns: see the `inputs` of each row)
// ---------------------------------------------------------------------------------------------

struct Alg3Case {
    name: &'static str,
    code: [u8; 16],
    salt: u8,
    nonce: u8,
    plaintext: Vec<u8>,
}

fn alg3_cases() -> Vec<Alg3Case> {
    vec![
        Alg3Case {
            name: "alg3_test_32_bytes",
            code: pat16(0x10),
            salt: 0x20,
            nonce: 0x30,
            plaintext: pat(0x00, 32),
        },
        Alg3Case {
            name: "alg3_test_96_bytes",
            code: pat16(0x10),
            salt: 0x21,
            nonce: 0x31,
            plaintext: pat(0x40, 96),
        },
        Alg3Case {
            name: "alg3_test_empty_plaintext",
            code: [0xA5; 16],
            salt: 0xE0,
            nonce: 0xE8,
            plaintext: Vec::new(),
        },
    ]
}

fn code_of(bytes: [u8; 16]) -> RecoveryCode {
    RecoveryCode::from_secret(Secret::from_bytes(bytes))
}

/// `wrap_with_code` of a plaintext of one of the sizes of the cases (the key's size is a const
/// generic).
fn wrap_plain(
    plaintext: &[u8],
    code: &RecoveryCode,
    ctx: &Ctx,
    ent: &mut dyn azlin_keys::Entropy,
) -> Result<Envelope, RecoveryError> {
    let (device, signer) = (kat_device(), kat_signer());
    match plaintext.len() {
        0 => wrap_with_code(
            &Secret::<0>::from_bytes([]),
            code,
            ctx,
            KdfProfile::Test,
            &device,
            &signer,
            ent,
        ),
        32 => wrap_with_code(
            &Secret::<32>::from_slice(plaintext).expect("32 bytes"),
            code,
            ctx,
            KdfProfile::Test,
            &device,
            &signer,
            ent,
        ),
        96 => wrap_with_code(
            &Secret::<96>::from_slice(plaintext).expect("96 bytes"),
            code,
            ctx,
            KdfProfile::Test,
            &device,
            &signer,
            ent,
        ),
        other => panic!("no case of {other} bytes"),
    }
}

// ---------------------------------------------------------------------------------------------
// C3 [M]
// ---------------------------------------------------------------------------------------------

/// C3 [M]: the wrap under the Test profile is the pinned vector, the vector's bytes open to the
/// plaintext, the draws are the 16-byte salt and then the 24-byte nonce.
#[test]
fn c3_alg3_roundtrip_and_kat() {
    let rows = vectors::read("recovery_wrap.json");
    let ctx = kat_ctx(RecoveryMethod::Code);
    let dir = kat_directory();
    for case in alg3_cases() {
        let r = row(&rows, case.name);
        let code = code_of(case.code);
        let mut ent = ScriptedEntropy::new([pat(case.salt, 16), pat(case.nonce, 24)].concat());
        let env = wrap_plain(&case.plaintext, &code, &ctx, &mut ent).expect("a wrap");
        assert_eq!(ent.draws, [16, 24], "{}: the draws", case.name);
        assert_eq!(ent.left(), 0, "{}: every scripted byte is used", case.name);
        assert_eq!(
            to_hex(&env.to_cbor().expect("bytes")),
            r.hex,
            "{}: the envelope's bytes",
            case.name
        );
        // The vector's own bytes are the envelope, and they open.
        let from_file = Envelope::from_cbor(&from_hex(&r.hex)).expect("an envelope");
        assert_eq!(from_file, env, "{}", case.name);
        let opened = unwrap_with_code(&from_file, &code, &ctx, &dir, VerifyAt::Now, KdfFloor::TEST)
            .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
        assert_eq!(to_hex(&opened), r.expect, "{}", case.name);
        assert_eq!(opened.as_slice(), case.plaintext.as_slice());
    }
}

/// C3 [M]: a wrong code fails `Auth`, a code that differs in one symbol too; the right one opens.
#[test]
fn c3_a_wrong_code_fails_auth() {
    let rows = vectors::read("recovery_wrap.json");
    let r = row(&rows, "alg3_test_32_bytes");
    let env = Envelope::from_cbor(&from_hex(&r.hex)).expect("an envelope");
    let ctx = kat_ctx(RecoveryMethod::Code);
    let dir = kat_directory();
    let open = |code: &RecoveryCode| {
        unwrap_with_code(&env, code, &ctx, &dir, VerifyAt::Now, KdfFloor::TEST)
            .map(|plaintext| plaintext.to_vec())
    };
    assert_eq!(open(&kat_code()).expect("the right code"), pat(0x00, 32));
    assert_eq!(open(&code_of(pat16(0x11))), Err(RecoveryError::Auth));
    let mut one_bit = pat16(0x10);
    one_bit[15] ^= 0x01;
    assert_eq!(open(&code_of(one_bit)), Err(RecoveryError::Auth));
}

/// C3 [M]: the salt is fresh for every wrap (two wraps of one key under one code differ in their
/// salt and ciphertext), and the same entropy gives the same bytes.
#[test]
fn c3_the_salt_is_fresh_per_wrap() {
    let code = kat_code();
    let ctx = kat_ctx(RecoveryMethod::Code);
    let (device, signer) = (kat_device(), kat_signer());
    let key = Secret::<32>::from_bytes(pat32(0x50));
    let wrap = |seed: u64| {
        wrap_with_code(
            &key,
            &code,
            &ctx,
            KdfProfile::Test,
            &device,
            &signer,
            &mut SeededEntropy::from_u64(seed),
        )
        .expect("a wrap")
    };
    let (a, b, a_again) = (wrap(1), wrap(2), wrap(1));
    let salt_of = |env: &Envelope| match &env.params {
        Params::Argon2id { salt, .. } => *salt,
        other => panic!("not Argon2id params: {other:?}"),
    };
    assert_ne!(salt_of(&a), salt_of(&b));
    assert_ne!(a.ct, b.ct);
    assert_ne!(a.nonce, b.nonce);
    assert_eq!(
        a.to_cbor().expect("bytes"),
        a_again.to_cbor().expect("bytes")
    );
}

/// C3: what the envelope says: algorithm 3, the Test cost, the member's code context, the signer,
/// no ephemeral key; and the registry's key-less check accepts it.
#[test]
fn c3_the_envelope_carries_the_cost_and_the_context() {
    let rows = vectors::read("recovery_wrap.json");
    let r = row(&rows, "alg3_test_32_bytes");
    let bytes = from_hex(&r.hex);
    let env = Envelope::from_cbor(&bytes).expect("an envelope");
    assert_eq!(env.alg, Alg::Argon2id);
    assert_eq!(
        env.params,
        Params::Argon2id {
            salt: <[u8; 16]>::try_from(pat(0x20, 16)).expect("16 bytes"),
            m_kib: 8192,
            t: 1,
            p: 2
        }
    );
    assert_eq!(env.ctx, kat_ctx(RecoveryMethod::Code));
    assert_eq!(
        env.ctx.as_str(),
        format!("recovery/{}/code", to_hex(&pat(0x71, 16)))
    );
    assert_eq!(env.signer, kat_device());
    assert!(env.epk.is_none() && env.kem_ct.is_none());
    let meta = check_shape(&bytes).expect("the registry's shape check");
    assert_eq!(meta.alg, Alg::Argon2id);
    assert_eq!(meta.ctx, env.ctx);
    assert_eq!(meta.signer, kat_device());
    assert_eq!(meta.rcpt, env.rcpt);
}

/// C3: the production profile is asserted by its numbers (256 MiB, 3 passes, 4 lanes: DRIFT
/// X-DELTA-W1S15D-6), never run; the test profile is 8 MiB, 1 pass; the floors and limits are the
/// brief's and today's `RecoveryKdf`.
#[test]
fn c3_the_profiles_floors_and_limits_are_the_numbers_of_the_brief() {
    let production = KdfProfile::Production.params();
    assert_eq!(production.memory, Kib::new(256 * 1024));
    assert_eq!(production.memory.get(), 262_144);
    assert_eq!(production.passes, 3);
    assert_eq!(production.lanes, 4);
    let test = KdfProfile::Test.params();
    assert_eq!((test.memory.get(), test.passes, test.lanes), (8_192, 1, 2));
    assert_eq!(KdfFloor::PRODUCTION.memory().get(), 65_536);
    assert_eq!(KdfFloor::PRODUCTION.passes(), 2);
    assert_eq!(KdfFloor::TEST.memory().get(), 8_192);
    assert_eq!(KdfFloor::TEST.passes(), 1);
    assert_eq!(KdfFloor::new(Kib::new(5), 7).memory(), Kib::new(5));
    assert_eq!(KdfFloor::new(Kib::new(5), 7).passes(), 7);
    // The production cost is above the production floor and within the limits; the test cost is
    // below the production floor.
    assert!(production.memory >= KdfFloor::PRODUCTION.memory());
    assert!(production.passes >= KdfFloor::PRODUCTION.passes());
    assert!(test.memory < KdfFloor::PRODUCTION.memory());
    assert!(production.memory <= MAX_KDF_MEMORY);
    assert!(production.passes <= MAX_KDF_PASSES);
    assert!(production.lanes <= MAX_KDF_LANES);
    assert!(u64::from(production.memory.get()) * u64::from(production.passes) <= MAX_KDF_WORK);
    // The limits of today's `RecoveryKdf`.
    assert_eq!(MAX_KDF_MEMORY.get(), 1024 * 1024);
    assert_eq!(MAX_KDF_PASSES, 8);
    assert_eq!(MAX_KDF_LANES, 8);
    assert_eq!(MAX_KDF_WORK, 4 * 1024 * 1024);
    // Kib orders by size.
    assert!(Kib::new(1) < Kib::new(2));
}

/// C3: a context that is not `recovery/<member>/code` makes no wrap (before the salt is drawn and
/// before anything is hashed), and a failing source makes none either.
#[test]
fn c3_no_wrap_is_made_for_another_context_or_from_a_failing_source() {
    let code = kat_code();
    let (device, signer) = (kat_device(), kat_signer());
    let key = Secret::<32>::from_bytes(pat32(0x50));
    let member = kat_member();
    for ctx in [
        Ctx::recovery(&member, RecoveryMethod::Prf),
        Ctx::recovery(&member, RecoveryMethod::Share),
        Ctx::member_key(&member),
        Ctx::escrow(&member),
    ] {
        let mut ent = ScriptedEntropy::new(vec![0u8; 64]);
        assert_eq!(
            wrap_with_code(
                &key,
                &code,
                &ctx,
                KdfProfile::Test,
                &device,
                &signer,
                &mut ent
            ),
            Err(RecoveryError::WrongCtx),
            "{}",
            ctx.as_str()
        );
        assert!(
            ent.draws.is_empty(),
            "nothing is drawn for {}",
            ctx.as_str()
        );
    }
    let ctx = kat_ctx(RecoveryMethod::Code);
    // The source fails at the salt: nothing is hashed.
    assert_eq!(
        wrap_with_code(
            &key,
            &code,
            &ctx,
            KdfProfile::Test,
            &device,
            &signer,
            &mut FailingEntropy::after(0)
        ),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    // The salt is served, the nonce is not: no envelope.
    assert_eq!(
        wrap_with_code(
            &key,
            &code,
            &ctx,
            KdfProfile::Test,
            &device,
            &signer,
            &mut FailingEntropy::after(16)
        ),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
}

// ---------------------------------------------------------------------------------------------
// The refusal rows
// ---------------------------------------------------------------------------------------------

/// The word of the error that the unwrap function a refusal row is meant for gives for its bytes.
fn refusal(name: &str, bytes: &[u8]) -> String {
    let env = match Envelope::from_cbor(bytes) {
        Ok(env) => env,
        Err(_) => return "NotAnEnvelope".to_string(),
    };
    let dir = kat_directory();
    let floor = if name == "neg_alg3_passes_below_floor_signed" {
        KdfFloor::new(Kib::new(65_536), 2)
    } else {
        KdfFloor::TEST
    };
    let prf_output = Secret::<32>::from_bytes(pat32(0x40));
    let result: Result<(), RecoveryError> = if name == "neg_alg3_given_to_unwrap_with_prf"
        || (name.starts_with("neg_alg4_") && name != "neg_alg4_given_to_unwrap_with_code")
    {
        unwrap_with_prf(
            &env,
            &prf_output,
            &kat_ctx(RecoveryMethod::Prf),
            &dir,
            VerifyAt::Now,
        )
        .map(|_| ())
    } else if name.starts_with("neg_alg5_") && name != "neg_alg5_given_to_unwrap_with_code" {
        open_share(
            &env,
            &kat_contact_secret(false),
            &kat_ctx(RecoveryMethod::Share),
            &dir,
            VerifyAt::Now,
        )
        .map(|_| ())
    } else {
        unwrap_with_code(
            &env,
            &kat_code(),
            &kat_ctx(RecoveryMethod::Code),
            &dir,
            VerifyAt::Now,
            floor,
        )
        .map(|_| ())
    };
    match result {
        Ok(()) => panic!("{name}: the refusal row was opened"),
        Err(error) => word(&error),
    }
}

/// Runs the named refusal rows and checks each word.
fn run_refusals(names: &[&str]) {
    let rows = vectors::read("recovery_wrap.json");
    for name in names {
        let r = row(&rows, name);
        assert!(!r.valid, "{name} is a refusal row");
        assert_eq!(refusal(name, &from_hex(&r.hex)), r.expect, "{name}");
    }
}

const C4_ROWS: [&str; 6] = [
    "neg_alg3_below_floor_signed",
    "neg_alg3_passes_below_floor_signed",
    "neg_alg3_above_limit_memory",
    "neg_alg3_above_limit_passes",
    "neg_alg3_above_limit_work",
    "neg_alg3_above_limit_lanes",
];
const C12_ROWS: [&str; 5] = [
    "neg_alg4_given_to_unwrap_with_code",
    "neg_alg5_given_to_unwrap_with_code",
    "neg_alg3_given_to_unwrap_with_prf",
    "neg_alg6_attested_is_not_opened_here",
    "neg_alg7_threshold_is_not_opened_here",
];
const C3_ROWS: [&str; 9] = [
    "neg_alg3_ctx_of_another_member",
    "neg_alg3_ct_bit_flipped",
    "neg_alg3_ct_bit_flipped_resigned",
    "neg_alg3_salt_changed_resigned",
    "neg_alg3_lanes_changed_resigned",
    "neg_truncated_envelope",
    "neg_trailing_byte",
    "neg_version_3",
    "neg_alg3_with_the_params_of_alg4",
];
const C5_ROWS: [&str; 2] = [
    "neg_alg4_salt_changed_resigned",
    "neg_alg4_cred_changed_resigned",
];
const C8_ROWS: [&str; 2] = [
    "neg_alg5_x_changed_resigned",
    "neg_alg5_split_id_changed_resigned",
];

/// Every refusal row of the file is run by one of the tests (none is forgotten, none is stale).
#[test]
fn c3_every_refusal_row_is_run_by_a_test() {
    let rows = vectors::read("recovery_wrap.json");
    let mut listed: Vec<&str> = [
        C4_ROWS.as_slice(),
        C12_ROWS.as_slice(),
        C3_ROWS.as_slice(),
        C5_ROWS.as_slice(),
        C8_ROWS.as_slice(),
    ]
    .concat();
    listed.sort_unstable();
    let mut refusals: Vec<&str> = rows
        .iter()
        .filter(|r| !r.valid)
        .map(|r| r.name.as_str())
        .collect();
    refusals.sort_unstable();
    assert_eq!(listed, refusals);
}

/// C3 [M]: the tampered wraps are refused with their word: another member's context, a bit of the
/// ciphertext (the signature refuses; signed again, the cipher refuses), a changed salt or lane
/// count (signed again: another key and other associated data), and bytes that are no envelope.
#[test]
fn c3_the_tampered_wraps_are_refused() {
    run_refusals(&C3_ROWS);
}

/// C8: a sealed share whose index or split id was changed and signed again by the legitimate device
/// is refused by the cipher (they are part of the associated data); the same rows are the negative
/// vectors of the algorithm 5 envelope.
#[test]
fn c8_the_sealed_share_refusal_rows_are_refused() {
    run_refusals(&C8_ROWS);
}

/// C4 [M]: an envelope whose params say 1 MiB or 1 pass (a server that lowers the cost, or an old
/// weak wrap replayed, validly signed) is refused `BelowFloor`; a cost above the limits (4 GiB, 1000
/// passes, 1 GiB with 5 passes, 9 lanes) is refused `AboveLimit`: before anything is hashed or
/// allocated (the unit test `c4_nothing_is_derived_before_the_cheap_checks` counts the derivations).
#[test]
fn c4_kdf_floor_refuses_weakened_params() {
    run_refusals(&C4_ROWS);

    // The right code, the right context, a valid signature: the production floor still refuses a
    // wrap made at the test cost.
    let rows = vectors::read("recovery_wrap.json");
    let env =
        Envelope::from_cbor(&from_hex(&row(&rows, "alg3_test_32_bytes").hex)).expect("an envelope");
    let ctx = kat_ctx(RecoveryMethod::Code);
    let dir = kat_directory();
    let open = |floor: KdfFloor| {
        unwrap_with_code(&env, &kat_code(), &ctx, &dir, VerifyAt::Now, floor).map(|p| p.to_vec())
    };
    assert_eq!(open(KdfFloor::PRODUCTION), Err(RecoveryError::BelowFloor));
    // One KiB more memory than the wrap has, or one pass more: below the floor.
    assert_eq!(
        open(KdfFloor::new(Kib::new(8_193), 1)),
        Err(RecoveryError::BelowFloor)
    );
    assert_eq!(
        open(KdfFloor::new(Kib::new(8_192), 2)),
        Err(RecoveryError::BelowFloor)
    );
    // The floor equal to the cost is not "below": it opens (the memory and the passes are checked
    // against the floor with "below", not "at or below").
    assert_eq!(
        open(KdfFloor::new(Kib::new(8_192), 1)).expect("opens"),
        pat(0x00, 32)
    );
    assert_eq!(
        open(KdfFloor::new(Kib::new(1), 1)).expect("opens"),
        pat(0x00, 32)
    );
}

/// C12 [M]: an algorithm 3 envelope handed to `unwrap_with_prf` is `WrongAlg`, algorithms 4 and 5
/// handed to `unwrap_with_code` too, and algorithms 6 and 7 (shapes only in this crate) are
/// `Unsupported`.
#[test]
fn c12_shape_only_algs_are_not_opened_here() {
    run_refusals(&C12_ROWS);
    let rows = vectors::read("recovery_wrap.json");
    for (name, want) in [
        (
            "neg_alg6_attested_is_not_opened_here",
            RecoveryError::Unsupported(Alg::Attested),
        ),
        (
            "neg_alg7_threshold_is_not_opened_here",
            RecoveryError::Unsupported(Alg::Threshold),
        ),
    ] {
        let env = Envelope::from_cbor(&from_hex(&row(&rows, name).hex)).expect("a shape");
        // The registry's key-less check accepts them as shapes ...
        check_shape(&env.to_cbor().expect("bytes")).expect("a shape");
        // ... and no function of this module opens them or reads their inputs.
        let dir = kat_directory();
        let prf = Secret::<32>::from_bytes(pat32(0x40));
        assert_eq!(
            unwrap_with_prf(
                &env,
                &prf,
                &kat_ctx(RecoveryMethod::Prf),
                &dir,
                VerifyAt::Now
            )
            .map(|_| ()),
            Err(want),
            "{name}"
        );
        assert_eq!(prf_inputs(&env), Err(want), "{name}");
        assert_eq!(
            open_share(
                &env,
                &kat_contact_secret(false),
                &kat_ctx(RecoveryMethod::Share),
                &dir,
                VerifyAt::Now
            )
            .map(|_| ()),
            Err(want),
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C5: algorithm 4
// ---------------------------------------------------------------------------------------------

struct Alg4Case {
    name: &'static str,
    prf: u8,
    salt: u8,
    cred: u8,
    nonce: u8,
    plaintext: Vec<u8>,
}

fn alg4_cases() -> Vec<Alg4Case> {
    vec![
        Alg4Case {
            name: "alg4_32_bytes",
            prf: 0x40,
            salt: 0x50,
            cred: 0x60,
            nonce: 0x70,
            plaintext: pat(0x00, 32),
        },
        Alg4Case {
            name: "alg4_empty_plaintext",
            prf: 0x41,
            salt: 0x51,
            cred: 0x61,
            nonce: 0x71,
            plaintext: Vec::new(),
        },
    ]
}

fn wrap_prf_plain(case: &Alg4Case, ctx: &Ctx, ent: &mut dyn azlin_keys::Entropy) -> Envelope {
    let (device, signer) = (kat_device(), kat_signer());
    let prf = Secret::<32>::from_bytes(pat32(case.prf));
    let (salt, cred) = (pat32(case.salt), pat32(case.cred));
    let wrapped = match case.plaintext.len() {
        0 => wrap_with_prf(
            &Secret::<0>::from_bytes([]),
            &prf,
            &salt,
            &cred,
            ctx,
            &device,
            &signer,
            ent,
        ),
        32 => wrap_with_prf(
            &Secret::<32>::from_slice(&case.plaintext).expect("32 bytes"),
            &prf,
            &salt,
            &cred,
            ctx,
            &device,
            &signer,
            ent,
        ),
        other => panic!("no case of {other} bytes"),
    };
    wrapped.expect("a wrap")
}

/// C5: the wrap under a PRF output is the pinned vector (the only draw is the nonce), the vector's
/// bytes open, a different output, a different context and a different method's context fail, and
/// the salt and the credential hash are what `prf_inputs` reads.
#[test]
fn c5_alg4_prf_roundtrip() {
    let rows = vectors::read("recovery_wrap.json");
    let ctx = kat_ctx(RecoveryMethod::Prf);
    let dir = kat_directory();
    for case in alg4_cases() {
        let r = row(&rows, case.name);
        let mut ent = ScriptedEntropy::new(pat(case.nonce, 24));
        let env = wrap_prf_plain(&case, &ctx, &mut ent);
        assert_eq!(ent.draws, [24], "{}: the only draw is the nonce", case.name);
        assert_eq!(ent.left(), 0);
        assert_eq!(
            to_hex(&env.to_cbor().expect("bytes")),
            r.hex,
            "{}: the bytes",
            case.name
        );
        let from_file = Envelope::from_cbor(&from_hex(&r.hex)).expect("an envelope");
        assert_eq!(from_file, env);
        assert_eq!(env.alg, Alg::Prf);
        assert_eq!(
            env.params,
            Params::Prf {
                salt: pat32(case.salt),
                cred_id_hash: pat32(case.cred)
            }
        );
        assert_eq!(
            prf_inputs(&env),
            Ok(PrfInputs {
                salt: pat32(case.salt),
                cred_id_hash: pat32(case.cred)
            })
        );
        let prf = Secret::<32>::from_bytes(pat32(case.prf));
        let opened = unwrap_with_prf(&from_file, &prf, &ctx, &dir, VerifyAt::Now).expect("opens");
        assert_eq!(to_hex(&opened), r.expect, "{}", case.name);

        // A different output, one bit of the right one, a different member's context, and the
        // other methods' contexts.
        let open = |prf: &Secret<32>, ctx: &Ctx| {
            unwrap_with_prf(&from_file, prf, ctx, &dir, VerifyAt::Now).map(|p| p.to_vec())
        };
        assert_eq!(
            open(&Secret::from_bytes(pat32(0x99)), &ctx),
            Err(RecoveryError::Auth)
        );
        let mut bit = pat32(case.prf);
        bit[31] ^= 0x80;
        assert_eq!(
            open(&Secret::from_bytes(bit), &ctx),
            Err(RecoveryError::Auth)
        );
        assert_eq!(
            open(
                &prf,
                &Ctx::recovery(&kat_other_member(), RecoveryMethod::Prf)
            ),
            Err(RecoveryError::Open(OpenError::CtxMismatch))
        );
        assert_eq!(
            open(&prf, &kat_ctx(RecoveryMethod::Code)),
            Err(RecoveryError::WrongCtx)
        );
        assert_eq!(
            open(&prf, &kat_ctx(RecoveryMethod::Share)),
            Err(RecoveryError::WrongCtx)
        );
    }
    // The rows that change the salt or the credential hash and sign again: the cipher refuses.
    run_refusals(&C5_ROWS);
}

/// C5: the PRF salt and the credential hash change the wrap (other salt, other key and other
/// ciphertext; other credential, other associated data and slot), the wrap is for the passkey's
/// context only, and a failing source makes no wrap and no salt.
#[test]
fn c5_alg4_binds_salt_credential_and_context() {
    let ctx = kat_ctx(RecoveryMethod::Prf);
    let (device, signer) = (kat_device(), kat_signer());
    let key = Secret::<32>::from_bytes(pat32(0x50));
    let prf = Secret::<32>::from_bytes(pat32(0x40));
    let wrap = |salt: u8, cred: u8| {
        wrap_with_prf(
            &key,
            &prf,
            &pat32(salt),
            &pat32(cred),
            &ctx,
            &device,
            &signer,
            &mut SeededEntropy::from_u64(5),
        )
        .expect("a wrap")
    };
    let base = wrap(1, 2);
    let other_salt = wrap(3, 2);
    let other_cred = wrap(1, 4);
    assert_eq!(
        base.nonce, other_salt.nonce,
        "the same entropy, the same nonce"
    );
    assert_ne!(base.ct, other_salt.ct, "another salt, another key");
    assert_ne!(
        base.rcpt, other_cred.rcpt,
        "another credential, another slot"
    );
    assert_eq!(
        base.rcpt, other_salt.rcpt,
        "the salt does not name the slot"
    );
    assert_ne!(
        base.to_cbor().expect("bytes"),
        other_cred.to_cbor().expect("bytes")
    );

    // The context must be the passkey's.
    let member = kat_member();
    for wrong in [
        Ctx::recovery(&member, RecoveryMethod::Code),
        Ctx::recovery(&member, RecoveryMethod::Share),
        Ctx::member_key(&member),
    ] {
        let mut ent = ScriptedEntropy::new(vec![0u8; 24]);
        assert_eq!(
            wrap_with_prf(
                &key,
                &prf,
                &pat32(1),
                &pat32(2),
                &wrong,
                &device,
                &signer,
                &mut ent
            ),
            Err(RecoveryError::WrongCtx),
            "{}",
            wrong.as_str()
        );
        assert!(ent.draws.is_empty());
    }
    // A failing source: no wrap, no salt.
    assert_eq!(
        wrap_with_prf(
            &key,
            &prf,
            &pat32(1),
            &pat32(2),
            &ctx,
            &device,
            &signer,
            &mut FailingEntropy::after(23)
        ),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    assert_eq!(
        new_prf_salt(&mut FailingEntropy::after(31)),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    let mut ent = ScriptedEntropy::new(pat(0x50, 32));
    assert_eq!(new_prf_salt(&mut ent), Ok(pat32(0x50)));
    assert_eq!(ent.draws, [32]);
    // `prf_inputs` of another algorithm's envelope.
    let rows = vectors::read("recovery_wrap.json");
    let alg3 =
        Envelope::from_cbor(&from_hex(&row(&rows, "alg3_test_32_bytes").hex)).expect("an envelope");
    assert_eq!(
        prf_inputs(&alg3),
        Err(RecoveryError::WrongAlg(Alg::Argon2id))
    );
}
