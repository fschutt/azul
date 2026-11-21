//! C6 (the strict codec: every negative), C11 (the node-side check needs no secret), C12 (algorithms
//! 3 to 7 are shape-checked only here, and the algorithm table equals the plan's row), and the
//! rules of copying a structure (R18: `crit` and unknown keys survive a copy byte for byte).
//!
//! Implements planning/architecture.md section 5.5 (Envelope v2), section 5.1 principle 6 (strict,
//! extensible formats: unknown fields ignored unless listed in `crit`) and section 5.3 (`reg_wrap`:
//! the server stores wraps it never opens).

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code)]

mod common;
mod envelope_common;

use azlin_cbor::{DecodeError, Limits, Map, ShapeError, Value};
use azlin_keys::envelope::{
    check_shape, open, open_with_key, rules, seal_with_key, verify_signature, wrap_hybrid,
    wrap_x25519, Alg, Ctx, CtxError, Envelope, EnvelopeError, EnvelopeMeta, Header, OpenError,
    Params, ParamsError, ParamsKind, Presence, RecoveryMethod, WrapError, ALG_TABLE, KEM_CT_LEN,
    MAX_ENVELOPE,
};
use azlin_keys::{
    DeviceDirectory, DeviceId, Epoch, MemberId, Secret, SeededEntropy, SpaceId, VerifyAt,
};
use common::SplitMix64;
use envelope_common::{edit_map, get, pat, pat16, pat32, put, remove, to_hex, Fixture};

fn base(f: &Fixture) -> (Envelope, Vec<u8>) {
    let env = f.wrap1(b"a space key", 61);
    let bytes = env.to_cbor().expect("bytes");
    (env, bytes)
}

fn refused(bytes: &[u8]) -> EnvelopeError {
    Envelope::from_cbor(bytes).expect_err("these bytes must be refused")
}

// ---------------------------------------------------------------------------------------------
// C6: the strict codec
// ---------------------------------------------------------------------------------------------

#[test]
fn c6_truncated_at_every_prefix_and_with_trailing_bytes() {
    let f = Fixture::new(6);
    let (_, bytes) = base(&f);
    for len in 0..bytes.len() {
        let e = refused(&bytes[..len]);
        assert!(
            matches!(e, EnvelopeError::Cbor(_)),
            "prefix of {len} bytes: {e:?}"
        );
        if len < 2 {
            assert_eq!(
                e,
                EnvelopeError::Cbor(DecodeError::Truncated),
                "prefix {len}"
            );
        }
        assert!(check_shape(&bytes[..len]).is_err());
    }
    for extra in [vec![0u8], vec![0xff], vec![0x00, 0x00], bytes.clone()] {
        let mut longer = bytes.clone();
        longer.extend(&extra);
        assert_eq!(
            refused(&longer),
            EnvelopeError::Cbor(DecodeError::TrailingBytes)
        );
    }
}

#[test]
fn c6_the_cbor_must_be_the_one_deterministic_encoding() {
    let f = Fixture::new(6);
    let (_, bytes) = base(&f);
    // The map head says nine entries (0xa9).
    assert_eq!(bytes[0], 0xa9);
    // A key twice: another `sig` after the last one.
    let mut duplicate = bytes.clone();
    duplicate[0] = 0xaa;
    duplicate.extend([0x0a, 0x40]);
    assert_eq!(
        refused(&duplicate),
        EnvelopeError::Cbor(DecodeError::DuplicateMapKey)
    );
    // A key out of order: key 9 after key 10.
    let mut unsorted = bytes.clone();
    unsorted[0] = 0xaa;
    unsorted.extend([0x09, 0x40]);
    assert_eq!(
        refused(&unsorted),
        EnvelopeError::Cbor(DecodeError::UnsortedMapKeys)
    );
    // `v` = 2 written in a longer form (0x18 0x02 instead of 0x02): `v` is the value after key 1.
    assert_eq!((bytes[1], bytes[2]), (0x01, 0x02));
    let mut long_form = vec![bytes[0], 0x01, 0x18, 0x02];
    long_form.extend_from_slice(&bytes[3..]);
    assert_eq!(
        refused(&long_form),
        EnvelopeError::Cbor(DecodeError::NotShortest)
    );
    // A float, an indefinite-length map, a tag, an empty input, a lone break.
    assert_eq!(
        refused(&[0xf9, 0x00, 0x00]),
        EnvelopeError::Cbor(DecodeError::DisallowedType)
    );
    assert_eq!(
        refused(&[0xbf, 0xff]),
        EnvelopeError::Cbor(DecodeError::IndefiniteLength)
    );
    assert_eq!(
        refused(&[0xc0, 0x00]),
        EnvelopeError::Cbor(DecodeError::DisallowedType)
    );
    assert_eq!(refused(&[]), EnvelopeError::Cbor(DecodeError::Truncated));
    assert_eq!(
        refused(&[0xff]),
        EnvelopeError::Cbor(DecodeError::ReservedBreak)
    );
    // Not a map, or a map with a key that is not an unsigned integer.
    assert_eq!(
        refused(&[0x80]),
        EnvelopeError::Shape(ShapeError::NotAnIntMap)
    );
    assert_eq!(
        refused(&[0xa0]),
        EnvelopeError::Shape(ShapeError::Missing(1))
    );
    let text_key = azlin_cbor::encode(
        &Value::Map(
            Map::from_entries(vec![(Value::Text("v".into()), Value::Uint(2))]).expect("a map"),
        ),
        &Limits::OBJECT,
    )
    .expect("bytes");
    assert_eq!(
        refused(&text_key),
        EnvelopeError::Shape(ShapeError::NotAnIntMap)
    );
}

#[test]
fn c6_wrong_types_per_field() {
    let f = Fixture::new(6);
    let (_, bytes) = base(&f);
    let wrong: Vec<(u64, Value)> = vec![
        (1, Value::Text("2".into())),
        (2, Value::Text("1".into())),
        (3, Value::Uint(1)),
        (4, Value::Text("epk".into())),
        (6, Value::Uint(7)),
        (7, Value::Uint(7)),
        (8, Value::Bytes(vec![1, 2, 3])),
        (9, Value::Uint(9)),
        (10, Value::Text("sig".into())),
        (1, Value::Nint(0)),
        (3, Value::Null),
        (7, Value::Array(vec![])),
        (8, Value::Bool(true)),
    ];
    for (key, value) in wrong {
        let edited = edit_map(&bytes, |m| put(m, key, value.clone()));
        assert_eq!(
            refused(&edited),
            EnvelopeError::Shape(ShapeError::WrongType(key)),
            "key {key} as {value:?}"
        );
    }
    // A missing field, one at a time.
    for key in [1u64, 2, 3, 4, 6, 7, 8, 9, 10] {
        let edited = edit_map(&bytes, |m| remove(m, key));
        let e = refused(&edited);
        let expected = if key == 4 {
            EnvelopeError::FieldRequired(4)
        } else {
            EnvelopeError::Shape(ShapeError::Missing(key))
        };
        assert_eq!(e, expected, "key {key} removed");
    }
}

#[test]
fn c6_versions_and_algorithms() {
    let f = Fixture::new(6);
    let (_, bytes) = base(&f);
    for v in [0u64, 1, 3, 255, 256, 1 << 32, u64::MAX] {
        let edited = edit_map(&bytes, |m| put(m, 1, Value::Uint(v)));
        assert_eq!(
            refused(&edited),
            EnvelopeError::UnsupportedVersion(v),
            "v {v}"
        );
    }
    // The version is read first: a version 3 envelope with nothing else is `UnsupportedVersion`.
    let only_version = azlin_cbor::encode(
        &Value::Map(Map::from_entries(vec![(Value::Uint(1), Value::Uint(3))]).expect("a map")),
        &Limits::OBJECT,
    )
    .expect("bytes");
    assert_eq!(refused(&only_version), EnvelopeError::UnsupportedVersion(3));
    for alg in [0u64, 8, 9, 255, 256, 1 << 40, u64::MAX] {
        let edited = edit_map(&bytes, |m| put(m, 2, Value::Uint(alg)));
        assert_eq!(
            refused(&edited),
            EnvelopeError::UnknownAlg(alg),
            "alg {alg}"
        );
    }
}

#[test]
fn c6_the_field_rules_of_each_algorithm() {
    let f = Fixture::new(6);
    let (_, bytes) = base(&f);
    let kem = |len: usize| Value::Bytes(vec![0u8; len]);
    // Algorithm 1 with a `kem_ct`, of any length: forbidden.
    for len in [0usize, 32, KEM_CT_LEN - 1, KEM_CT_LEN, KEM_CT_LEN + 1] {
        let edited = edit_map(&bytes, |m| put(m, 5, kem(len)));
        assert_eq!(
            refused(&edited),
            EnvelopeError::FieldForbidden(5),
            "kem_ct of {len}"
        );
    }
    // Algorithm 2 without a `kem_ct`: required; with one of the wrong length: refused.
    let alg2 = |m: &mut Vec<(Value, Value)>| put(m, 2, Value::Uint(2));
    assert_eq!(
        refused(&edit_map(&bytes, alg2)),
        EnvelopeError::FieldRequired(5)
    );
    for len in [0usize, 32, KEM_CT_LEN - 1, KEM_CT_LEN + 1] {
        let edited = edit_map(&bytes, |m| {
            alg2(m);
            put(m, 5, kem(len));
        });
        assert_eq!(
            refused(&edited),
            EnvelopeError::Shape(ShapeError::WrongLength(5)),
            "kem_ct of {len}"
        );
    }
    // The right length is a shape (the cryptography is not the codec's).
    let well_formed = edit_map(&bytes, |m| {
        alg2(m);
        put(m, 5, kem(KEM_CT_LEN));
    });
    assert_eq!(
        check_shape(&well_formed).map(|meta| meta.alg),
        Ok(Alg::X25519MlKem768)
    );
    // Fixed lengths.
    for (key, good, name) in [
        (3u64, 32usize, "rcpt"),
        (4, 32, "epk"),
        (6, 24, "nonce"),
        (9, 16, "signer"),
        (10, 64, "sig"),
    ] {
        for len in [0, good - 1, good + 1, good + 32] {
            let edited = edit_map(&bytes, |m| put(m, key, Value::Bytes(vec![7u8; len])));
            assert_eq!(
                refused(&edited),
                EnvelopeError::Shape(ShapeError::WrongLength(key)),
                "{name} of {len} bytes"
            );
        }
    }
    // The ciphertext is at least one tag.
    let short = edit_map(&bytes, |m| put(m, 7, Value::Bytes(vec![0u8; 15])));
    assert_eq!(
        refused(&short),
        EnvelopeError::Shape(ShapeError::WrongLength(7))
    );
    let one_tag = edit_map(&bytes, |m| put(m, 7, Value::Bytes(vec![0u8; 16])));
    assert!(Envelope::from_cbor(&one_tag).is_ok());
    // `params` on algorithms 1 and 2: forbidden.
    let with_params = edit_map(&bytes, |m| put(m, 16, Value::Map(Map::default())));
    assert_eq!(refused(&with_params), EnvelopeError::FieldForbidden(16));
    // Algorithm 1 without `epk`: required.
    let no_epk = edit_map(&bytes, |m| remove(m, 4));
    assert_eq!(refused(&no_epk), EnvelopeError::FieldRequired(4));
    // A `ctx` out of the grammar.
    let bad_ctx = edit_map(&bytes, |m| put(m, 8, Value::Text("space/x/epoch/3".into())));
    assert_eq!(refused(&bad_ctx), EnvelopeError::BadCtx(CtxError::BadId));
    let long_ctx = edit_map(&bytes, |m| put(m, 8, Value::Text("a".repeat(129))));
    assert_eq!(refused(&long_ctx), EnvelopeError::BadCtx(CtxError::TooLong));
}

#[test]
fn c6_unknown_keys_are_kept_and_crit_keys_must_be_understood() {
    let f = Fixture::new(6);
    let (env, bytes) = base(&f);

    // A key this version does not know, signed by a (newer) writer: accepted, kept, and the
    // signature still verifies, because the signature covers it.
    let mut newer = env.clone();
    newer.unknown = vec![
        (Value::Uint(20), Value::Text("a newer field".into())),
        (
            Value::Uint(21),
            Value::Array(vec![Value::Uint(1), Value::Bytes(vec![2, 3])]),
        ),
    ];
    newer.sign(&f.signer).expect("signs");
    let newer_bytes = newer.to_cbor().expect("bytes");
    let read = Envelope::from_cbor(&newer_bytes).expect("an unknown key is ignored");
    assert_eq!(read, newer, "the unknown keys are kept");
    assert_eq!(
        read.to_cbor().expect("bytes"),
        newer_bytes,
        "and written back byte for byte"
    );
    assert_eq!(verify_signature(&read, &f.dir, VerifyAt::Now), Ok(()));
    assert_eq!(f.open(&read).expect("opens").as_slice(), b"a space key");
    assert_eq!(
        check_shape(&newer_bytes).map(|m| m.size),
        Ok(newer_bytes.len())
    );

    // The same key added WITHOUT signing again: it reads, and the old signature no longer
    // verifies (the signature covers the unknown key).
    let tampered = edit_map(&bytes, |m| put(m, 20, Value::Text("injected".into())));
    let read = Envelope::from_cbor(&tampered).expect("an unknown key is ignored");
    assert_eq!(
        read.unknown,
        vec![(Value::Uint(20), Value::Text("injected".into()))]
    );
    assert_eq!(
        verify_signature(&read, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );

    // An unknown key that `crit` lists: refused, not skipped.
    let must_understand = edit_map(&bytes, |m| {
        put(m, 20, Value::Null);
        put(m, 0, Value::Array(vec![Value::Uint(20)]));
    });
    assert_eq!(
        refused(&must_understand),
        EnvelopeError::Shape(ShapeError::CritUnknown(20))
    );
    // A `crit` that is not a non-empty ascending array of distinct keys above 0.
    for bad in [
        Value::Array(vec![]),
        Value::Array(vec![Value::Uint(0)]),
        Value::Array(vec![Value::Uint(9), Value::Uint(8)]),
        Value::Array(vec![Value::Uint(8), Value::Uint(8)]),
        Value::Array(vec![Value::Text("x".into())]),
        Value::Text("crit".into()),
        Value::Uint(8),
    ] {
        let edited = edit_map(&bytes, |m| put(m, 0, bad.clone()));
        assert_eq!(
            refused(&edited),
            EnvelopeError::Shape(ShapeError::BadCrit),
            "crit {bad:?}"
        );
    }
}

/// Rule R18: copying a structure keeps `crit`. A `crit` that lists keys this version understands
/// is read, kept in the struct, and written back, so a copy that is signed again has the same
/// bytes as the original; a copy that drops it does not.
#[test]
fn c6_a_copy_keeps_crit_and_its_bytes() {
    let f = Fixture::new(6);
    let (env, _) = base(&f);
    let mut marked = env.clone();
    // `ctx` (8) is understood; `params` (16) is understood and absent: both may be listed.
    marked.crit = vec![8, 16];
    marked.unknown = vec![(Value::Uint(30), Value::Uint(1))];
    marked.sign(&f.signer).expect("signs");
    let bytes = marked.to_cbor().expect("bytes");

    let read = Envelope::from_cbor(&bytes).expect("a crit of understood keys is fine");
    assert_eq!(read.crit, vec![8, 16]);
    assert_eq!(read, marked);

    // The copy, signed again by the same device: byte for byte the original (Ed25519 is
    // deterministic, and `crit` and the unknown key are written back).
    let mut copy = read.clone();
    copy.sign(&f.signer).expect("signs");
    assert_eq!(
        copy.to_cbor().expect("bytes"),
        bytes,
        "a copy with crit equals the original"
    );
    assert_eq!(verify_signature(&copy, &f.dir, VerifyAt::Now), Ok(()));

    // A copy that drops `crit` is another document: other bytes, and the signature is no longer
    // the one of its bytes.
    let mut dropped = read.clone();
    dropped.crit = Vec::new();
    assert_ne!(dropped.to_cbor().expect("bytes"), bytes);
    assert_eq!(
        verify_signature(&dropped, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
    // A copy that drops the unknown key likewise.
    let mut without_unknown = read.clone();
    without_unknown.unknown.clear();
    assert_ne!(without_unknown.to_cbor().expect("bytes"), bytes);
}

/// What a hand-built envelope must not be able to write: nothing `from_cbor` refuses leaves
/// `to_cbor`.
#[test]
fn c6_to_cbor_refuses_what_from_cbor_would() {
    let f = Fixture::new(6);
    let (env, _) = base(&f);
    let mut with_kem = env.clone();
    with_kem.kem_ct = Some(vec![0u8; KEM_CT_LEN]);
    assert_eq!(with_kem.to_cbor(), Err(EnvelopeError::FieldForbidden(5)));
    let mut no_epk = env.clone();
    no_epk.epk = None;
    assert_eq!(no_epk.to_cbor(), Err(EnvelopeError::FieldRequired(4)));
    let mut with_params = env.clone();
    with_params.params = Params::Prf {
        salt: pat32(1),
        cred_id_hash: pat32(2),
    };
    assert_eq!(
        with_params.to_cbor(),
        Err(EnvelopeError::FieldForbidden(16))
    );
    let mut short_ct = env.clone();
    short_ct.ct = vec![0u8; 15];
    assert_eq!(
        short_ct.to_cbor(),
        Err(EnvelopeError::Shape(ShapeError::WrongLength(7)))
    );
    // Unknown entries that collide with a known key, key 0, a key twice, a key that is no
    // integer.
    for bad in [
        vec![(Value::Uint(3), Value::Null)],
        vec![(Value::Uint(0), Value::Null)],
        vec![
            (Value::Uint(20), Value::Null),
            (Value::Uint(20), Value::Null),
        ],
        vec![(Value::Text("k".into()), Value::Null)],
    ] {
        let mut e = env.clone();
        e.unknown = bad.clone();
        assert_eq!(e.to_cbor(), Err(EnvelopeError::BadUnknown), "{bad:?}");
    }
    // A `crit` that lists 0 or a key that is not understood cannot be written.
    for crit in [vec![0u64], vec![20], vec![8, 20]] {
        let mut e = env.clone();
        e.crit = crit.clone();
        assert_eq!(e.to_cbor(), Err(EnvelopeError::BadCrit), "{crit:?}");
    }
    // `open` and `verify_signature` refuse such an envelope too.
    let mut bad = env.clone();
    bad.kem_ct = Some(vec![0u8; KEM_CT_LEN]);
    assert_eq!(
        verify_signature(&bad, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::FieldForbidden(5))
    );
    assert_eq!(
        f.open(&bad).err(),
        Some(OpenError::Malformed(EnvelopeError::FieldForbidden(5)))
    );
}

// ---------------------------------------------------------------------------------------------
// C11: the node-side check needs no secret
// ---------------------------------------------------------------------------------------------

/// The signatures of the node-side functions, as types: neither takes a key, a recipient secret
/// or a plaintext (only bytes; an envelope and a directory of PUBLIC keys).
#[test]
fn c11_the_node_side_functions_take_no_secret() {
    let _shape: fn(&[u8]) -> Result<EnvelopeMeta, EnvelopeError> = check_shape;
    let _signature: fn(&Envelope, &dyn DeviceDirectory, VerifyAt) -> Result<(), EnvelopeError> =
        verify_signature;
}

#[test]
fn c11_check_shape_and_verify_signature_accept_every_envelope_the_wraps_make() {
    let f = Fixture::new(11);
    let mut rng = SplitMix64::new(0xC11);
    let mut cases = 0usize;
    #[cfg(feature = "pq")]
    let (alg1_cases, alg2_cases) = (4500usize, 500usize);
    #[cfg(not(feature = "pq"))]
    let (alg1_cases, alg2_cases) = (5000usize, 0usize);
    for case in 0..alg1_cases + alg2_cases {
        let len = rng.below(121) as usize;
        let plaintext = rng.bytes(len);
        let ctx = random_ctx(&mut rng);
        let seed = 10_000 + case as u64;
        let mut ent = SeededEntropy::from_u64(seed);
        let (env, alg) = if case < alg1_cases {
            let env = wrap_x25519(
                &f.me.recipient_x25519(),
                &ctx,
                &plaintext,
                &f.device,
                &f.signer,
                &mut ent,
            )
            .expect("a wrap of algorithm 1");
            (env, Alg::X25519SealedBox)
        } else {
            (
                wrap_second(&f, &ctx, &plaintext, &mut ent),
                Alg::X25519MlKem768,
            )
        };
        let bytes = env.to_cbor().expect("bytes");
        assert!(
            bytes.len() <= MAX_ENVELOPE,
            "case {case}: {} bytes",
            bytes.len()
        );
        let meta = check_shape(&bytes).unwrap_or_else(|e| panic!("case {case}: {e:?}"));
        assert_eq!(meta.alg, alg);
        assert_eq!(meta.rcpt, env.rcpt);
        assert_eq!(meta.ctx, ctx);
        assert_eq!(meta.signer, f.device);
        assert_eq!(meta.size, bytes.len());
        let read = Envelope::from_cbor(&bytes).expect("reads");
        assert_eq!(
            verify_signature(&read, &f.dir, VerifyAt::Now),
            Ok(()),
            "case {case}"
        );
        cases += 1;
    }
    assert_eq!(cases, 5000);
}

/// An envelope of algorithm 2 for the fixture's recipient (only built with the feature `pq`).
#[cfg(feature = "pq")]
fn wrap_second(f: &Fixture, ctx: &Ctx, plaintext: &[u8], ent: &mut SeededEntropy) -> Envelope {
    wrap_hybrid(
        &f.me.recipient_hybrid().expect("an ML-KEM key"),
        ctx,
        plaintext,
        &f.device,
        &f.signer,
        ent,
    )
    .expect("a wrap of algorithm 2")
}

#[cfg(not(feature = "pq"))]
fn wrap_second(_: &Fixture, _: &Ctx, _: &[u8], _: &mut SeededEntropy) -> Envelope {
    panic!("no hybrid cases without the feature pq")
}

fn random_ctx(rng: &mut SplitMix64) -> Ctx {
    let mut id = [0u8; 16];
    id.copy_from_slice(&rng.bytes(16));
    let epoch = Epoch::new(1 + rng.below(u64::from(u32::MAX)) as u32).expect("nonzero");
    match rng.below(7) {
        0 => Ctx::space_epoch(&SpaceId::from_bytes(id), epoch),
        1 => Ctx::group_epoch(&azlin_keys::GroupId::from_bytes(id), epoch),
        2 => Ctx::member_key(&MemberId::from_bytes(id)),
        3 => Ctx::chain(&SpaceId::from_bytes(id), epoch),
        4 => Ctx::seal(&azlin_keys::AccountId::from_bytes(id), epoch),
        5 => Ctx::escrow(&MemberId::from_bytes(id)),
        _ => Ctx::recovery(&MemberId::from_bytes(id), RecoveryMethod::Share),
    }
}

#[test]
fn c11_check_shape_refuses_oversize_bad_ctx_and_bad_fields() {
    let f = Fixture::new(11);
    // The limit is on the bytes of the envelope: exactly MAX_ENVELOPE is stored, one more is not.
    let probe = f.wrap1(&vec![0u8; 3800], 111);
    let probe_len = probe.to_cbor().expect("bytes").len();
    assert!(probe_len < MAX_ENVELOPE);
    let exact = f.wrap1(&vec![0u8; 3800 + (MAX_ENVELOPE - probe_len)], 111);
    let exact_bytes = exact.to_cbor().expect("bytes");
    assert_eq!(exact_bytes.len(), MAX_ENVELOPE);
    assert!(check_shape(&exact_bytes).is_ok(), "exactly the limit");
    let over = f.wrap1(&vec![0u8; 3800 + (MAX_ENVELOPE - probe_len) + 1], 111);
    let over_bytes = over.to_cbor().expect("bytes");
    assert_eq!(over_bytes.len(), MAX_ENVELOPE + 1);
    assert_eq!(
        check_shape(&over_bytes).err(),
        Some(EnvelopeError::TooLarge)
    );
    // The reader (not the registry's check) accepts it: the limit is the node's.
    assert!(Envelope::from_cbor(&over_bytes).is_ok());
    // Far over the limit, and bytes that are not an envelope at all, are refused before parsing.
    assert_eq!(
        check_shape(&vec![0xa9u8; 1 << 20]).err(),
        Some(EnvelopeError::TooLarge)
    );
    assert_eq!(
        check_shape(&[0u8; MAX_ENVELOPE + 1]).err(),
        Some(EnvelopeError::TooLarge)
    );
    // A context out of the grammar, and per-algorithm fields that do not fit.
    let (_, bytes) = base(&f);
    let bad_ctx = edit_map(&bytes, |m| put(m, 8, Value::Text("Space/x".into())));
    assert_eq!(
        check_shape(&bad_ctx).err(),
        Some(EnvelopeError::BadCtx(CtxError::BadCharacter))
    );
    let bad_fields = edit_map(&bytes, |m| put(m, 5, Value::Bytes(vec![0u8; KEM_CT_LEN])));
    assert_eq!(
        check_shape(&bad_fields).err(),
        Some(EnvelopeError::FieldForbidden(5))
    );
    // `check_shape` does not look at the signature: a zeroed one is a shape.
    let unsigned = edit_map(&bytes, |m| put(m, 10, Value::Bytes(vec![0u8; 64])));
    assert!(check_shape(&unsigned).is_ok());
    let read = Envelope::from_cbor(&unsigned).expect("reads");
    assert_eq!(
        verify_signature(&read, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
}

// ---------------------------------------------------------------------------------------------
// C12: algorithms 3 to 7 are shape-checked only here
// ---------------------------------------------------------------------------------------------

fn keyed_header(f: &Fixture, alg: Alg) -> Header {
    let member = MemberId::from_bytes(pat16(0x72));
    let (epk, params, ctx) = match alg {
        Alg::Argon2id => (
            None,
            Params::Argon2id {
                salt: pat16(0x11),
                m_kib: 262_144,
                t: 3,
                p: 4,
            },
            Ctx::recovery(&member, RecoveryMethod::Code),
        ),
        Alg::Prf => (
            None,
            Params::Prf {
                salt: pat32(0x21),
                cred_id_hash: pat32(0x22),
            },
            Ctx::recovery(&member, RecoveryMethod::Prf),
        ),
        Alg::Shamir => (
            None,
            Params::Shamir {
                x: 2,
                k: 2,
                n: 3,
                split_id: [1, 2, 3, 4],
            },
            Ctx::recovery(&member, RecoveryMethod::Share),
        ),
        Alg::Attested => (
            Some(pat32(0x31)),
            Params::Attested {
                evidence: pat(0, 100),
                policy: pat(0x50, 32),
            },
            Ctx::member_key(&member),
        ),
        Alg::Threshold => (
            Some(pat32(0x32)),
            Params::Threshold {
                k: 2,
                n: 3,
                holder: pat(0x60, 16),
            },
            Ctx::escrow(&member),
        ),
        other => panic!("{other:?} is not a keyed algorithm"),
    };
    Header {
        alg,
        rcpt: f.me.x25519_id(),
        epk,
        kem_ct: None,
        params,
        ctx,
        signer: f.device,
    }
}

const KEYED: [Alg; 5] = [
    Alg::Argon2id,
    Alg::Prf,
    Alg::Shamir,
    Alg::Attested,
    Alg::Threshold,
];

#[test]
fn c12_algs_3_to_7_decode_with_their_params_and_open_is_unsupported_here() {
    let f = Fixture::new(12);
    let key = Secret::<32>::from_bytes(pat32(0xee));
    for alg in KEYED {
        let header = keyed_header(&f, alg);
        let mut ent = SeededEntropy::from_u64(120 + alg.number() as u64);
        let env = seal_with_key(&header, &key, b"a share", &f.signer, &mut ent).expect("seals");
        let bytes = env.to_cbor().expect("bytes");
        // The shape check passes and reports the algorithm.
        let meta = check_shape(&bytes).unwrap_or_else(|e| panic!("{alg:?}: {e:?}"));
        assert_eq!(meta.alg, alg);
        // The params come back as they went in.
        let read = Envelope::from_cbor(&bytes).expect("reads");
        assert_eq!(read.params, header.params, "{alg:?}");
        assert_eq!(read.params.kind(), rules(alg).params);
        assert_eq!(read, env);
        assert_eq!(verify_signature(&read, &f.dir, VerifyAt::Now), Ok(()));
        // `open` is not the way in for these algorithms: S15d (3, 4, 5) and S17 (6, 7).
        assert_eq!(
            open(&read, &f.me, &header.ctx, &f.dir, VerifyAt::Now).err(),
            Some(OpenError::Unsupported(alg)),
            "{alg:?}"
        );
        // The keyed pipeline opens it.
        let opened = open_with_key(&read, &key, &header.ctx, &f.dir, VerifyAt::Now).expect("opens");
        assert_eq!(opened.as_slice(), b"a share");
    }
    // Algorithm 6's `epk` is optional.
    let mut header = keyed_header(&f, Alg::Attested);
    header.epk = None;
    let mut ent = SeededEntropy::from_u64(126);
    let env = seal_with_key(&header, &key, b"x", &f.signer, &mut ent).expect("seals");
    assert!(check_shape(&env.to_cbor().expect("bytes")).is_ok());
}

/// A Shamir share wrapped to a contact is sealed like algorithm 1 or 2 (W1-S15d's reading), so an
/// envelope of algorithm 5 may carry an ephemeral key, and with it a KEM ciphertext; a KEM
/// ciphertext without its ephemeral key is no envelope.
#[test]
fn c12_alg5_may_carry_the_ephemeral_key_and_the_kem_ciphertext_of_a_sealed_share() {
    let f = Fixture::new(12);
    let key = Secret::<32>::from_bytes(pat32(0xee));
    let seal = |epk: Option<[u8; 32]>, kem_ct: Option<Vec<u8>>| {
        let header = Header {
            epk,
            kem_ct,
            ..keyed_header(&f, Alg::Shamir)
        };
        let mut ent = SeededEntropy::from_u64(5);
        seal_with_key(&header, &key, b"a share", &f.signer, &mut ent)
    };
    assert!(seal(None, None).is_ok());
    let sealed_box = seal(Some(pat32(1)), None).expect("an ephemeral key");
    let hybrid = seal(Some(pat32(1)), Some(vec![0u8; KEM_CT_LEN])).expect("both");
    for env in [&sealed_box, &hybrid] {
        let bytes = env.to_cbor().expect("bytes");
        assert_eq!(check_shape(&bytes).map(|m| m.alg), Ok(Alg::Shamir));
        assert_eq!(&Envelope::from_cbor(&bytes).expect("reads"), env);
        assert_eq!(verify_signature(env, &f.dir, VerifyAt::Now), Ok(()));
    }
    let envelope = |e: EnvelopeError| Some(WrapError::Envelope(e));
    assert_eq!(
        seal(None, Some(vec![0u8; KEM_CT_LEN])).err(),
        envelope(EnvelopeError::FieldRequired(4))
    );
    assert_eq!(
        seal(Some(pat32(1)), Some(vec![0u8; 5])).err(),
        envelope(EnvelopeError::Shape(ShapeError::WrongLength(5)))
    );
    // Algorithms 3 and 4 still forbid both.
    for alg in [Alg::Argon2id, Alg::Prf] {
        let header = Header {
            epk: Some(pat32(1)),
            ..keyed_header(&f, alg)
        };
        let mut ent = SeededEntropy::from_u64(6);
        assert_eq!(
            seal_with_key(&header, &key, b"x", &f.signer, &mut ent).err(),
            envelope(EnvelopeError::FieldForbidden(4)),
            "{alg:?}"
        );
    }
}

#[test]
fn c12_the_field_rules_are_data_and_say_what_the_brief_says() {
    use Presence::{Forbidden, Optional, Required};
    let want = [
        (Alg::X25519SealedBox, Required, Forbidden, ParamsKind::None),
        (Alg::X25519MlKem768, Required, Required, ParamsKind::None),
        (Alg::Argon2id, Forbidden, Forbidden, ParamsKind::Argon2id),
        (Alg::Prf, Forbidden, Forbidden, ParamsKind::Prf),
        (Alg::Shamir, Optional, Optional, ParamsKind::Shamir),
        (Alg::Attested, Optional, Forbidden, ParamsKind::Attested),
        (Alg::Threshold, Required, Forbidden, ParamsKind::Threshold),
    ];
    for (alg, epk, kem_ct, params) in want {
        let r = rules(alg);
        assert_eq!(
            (r.epk, r.kem_ct, r.params),
            (epk, kem_ct, params),
            "{alg:?}"
        );
    }
}

/// The rows of the plan, typed from the quote in the brief (planning/architecture.md section 5.5:
/// "1 X25519 sealed box; 2 X25519 + ML-KEM-768 hybrid; 3 Argon2id passphrase; 4 PRF; 5 Shamir
/// share; 6 sealed to an attested key (`evidence`, `policy`); 7 threshold escrow").
const PLAN_ROWS: [(u8, &str); 7] = [
    (1, "X25519 sealed box"),
    (2, "X25519 + ML-KEM-768 hybrid"),
    (3, "Argon2id passphrase"),
    (4, "PRF"),
    (5, "Shamir share"),
    (6, "sealed to an attested key (`evidence`, `policy`)"),
    (7, "threshold escrow"),
];

#[test]
fn c12_the_alg_table_equals_the_seven_rows_of_the_plan() {
    assert_eq!(ALG_TABLE, &PLAN_ROWS[..]);
    assert_eq!(Alg::ALL.len(), 7);
    for (alg, (number, words)) in Alg::ALL.iter().zip(PLAN_ROWS) {
        assert_eq!(alg.number(), number);
        assert_eq!(alg.title(), words);
        assert_eq!(Alg::from_number(u64::from(number)), Some(*alg));
    }
    for n in [0u64, 8, 9, 255, 256, u64::MAX] {
        assert_eq!(Alg::from_number(n), None, "{n}");
    }
}

/// The table against the plan's own text (R7 of the W1 rules: the plan is found through
/// `AZLIN_PLAN_MD`, else next to the azul checkout; absent and `AZLIN_PLAN_MD` unset is a skip,
/// absent and set is a failure).
#[test]
fn c12_the_alg_table_matches_the_plan_text() {
    let explicit = std::env::var_os("AZLIN_PLAN_MD");
    let path = match &explicit {
        Some(path) => std::path::PathBuf::from(path),
        None => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../azul-apps/planning/architecture.md"),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            assert!(
                explicit.is_none(),
                "AZLIN_PLAN_MD is set and {} cannot be read: {e}",
                path.display()
            );
            println!("SKIPPED: plan not present");
            return;
        }
    };
    let row = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("| `alg` |"))
        .expect("the plan has the `alg` row of the envelope table");
    let cell = row.split('|').nth(2).expect("the second cell").trim();
    let rows: Vec<(u8, String)> = cell
        .split("; ")
        .map(|entry| {
            let (number, words) = entry.split_once(' ').expect("a number and its words");
            (number.parse().expect("a number"), words.to_string())
        })
        .collect();
    assert_eq!(rows.len(), ALG_TABLE.len());
    for ((number, words), (want_number, want_words)) in rows.iter().zip(ALG_TABLE) {
        assert_eq!((number, words.as_str()), (want_number, *want_words));
    }
}

#[test]
fn c12_params_are_closed_and_range_checked() {
    let f = Fixture::new(12);
    let key = Secret::<32>::from_bytes(pat32(0xee));
    let seal = |params: Params, alg: Alg| {
        let header = Header {
            params,
            ..keyed_header(&f, alg)
        };
        let mut ent = SeededEntropy::from_u64(1);
        seal_with_key(&header, &key, b"x", &f.signer, &mut ent).err()
    };
    let out_of_range = |k: u64| {
        Some(WrapError::Envelope(EnvelopeError::BadParams(
            ParamsError::OutOfRange(k),
        )))
    };
    let argon = |m_kib: u32, t: u32, p: u32| Params::Argon2id {
        salt: pat16(1),
        m_kib,
        t,
        p,
    };
    assert_eq!(seal(argon(8192, 2, 1), Alg::Argon2id), None);
    assert_eq!(seal(argon(262_144, 3, 4), Alg::Argon2id), None);
    assert_eq!(seal(argon(8192, 2, 0), Alg::Argon2id), out_of_range(4));
    assert_eq!(seal(argon(8192, 2, 256), Alg::Argon2id), out_of_range(4));
    assert_eq!(seal(argon(8192, 0, 1), Alg::Argon2id), out_of_range(3));
    assert_eq!(seal(argon(8192, 1025, 1), Alg::Argon2id), out_of_range(3));
    assert_eq!(seal(argon(7, 2, 1), Alg::Argon2id), out_of_range(2));
    assert_eq!(
        seal(argon(31, 2, 4), Alg::Argon2id),
        out_of_range(2),
        "8 KiB per lane"
    );
    assert_eq!(seal(argon(4_194_305, 2, 1), Alg::Argon2id), out_of_range(2));
    let shamir = |x: u8, k: u8, n: u8| Params::Shamir {
        x,
        k,
        n,
        split_id: [0; 4],
    };
    assert_eq!(seal(shamir(1, 2, 3), Alg::Shamir), None);
    assert_eq!(seal(shamir(0, 2, 3), Alg::Shamir), out_of_range(1));
    assert_eq!(seal(shamir(1, 0, 3), Alg::Shamir), out_of_range(2));
    assert_eq!(seal(shamir(1, 4, 3), Alg::Shamir), out_of_range(2));
    let attested = |e: usize, p: usize| Params::Attested {
        evidence: vec![1; e],
        policy: vec![2; p],
    };
    assert_eq!(seal(attested(2048, 256), Alg::Attested), None);
    assert_eq!(seal(attested(0, 1), Alg::Attested), out_of_range(1));
    assert_eq!(seal(attested(2049, 1), Alg::Attested), out_of_range(1));
    assert_eq!(seal(attested(1, 0), Alg::Attested), out_of_range(2));
    assert_eq!(seal(attested(1, 257), Alg::Attested), out_of_range(2));
    let threshold = |k: u8, n: u8, h: usize| Params::Threshold {
        k,
        n,
        holder: vec![3; h],
    };
    assert_eq!(seal(threshold(2, 3, 64), Alg::Threshold), None);
    assert_eq!(seal(threshold(0, 3, 1), Alg::Threshold), out_of_range(1));
    assert_eq!(seal(threshold(4, 3, 1), Alg::Threshold), out_of_range(1));
    assert_eq!(seal(threshold(2, 3, 0), Alg::Threshold), out_of_range(3));
    assert_eq!(seal(threshold(2, 3, 65), Alg::Threshold), out_of_range(3));
    // The params of another algorithm.
    assert_eq!(
        seal(argon(8192, 2, 1), Alg::Prf),
        Some(WrapError::Envelope(EnvelopeError::BadParams(
            ParamsError::WrongKind
        )))
    );

    // Read side: a closed map of exactly the keys of the algorithm.
    let header = keyed_header(&f, Alg::Argon2id);
    let mut ent = SeededEntropy::from_u64(2);
    let env = seal_with_key(&header, &key, b"x", &f.signer, &mut ent).expect("seals");
    let bytes = env.to_cbor().expect("bytes");
    let params_map = |entries: Vec<(u64, Value)>| {
        Value::Map(
            Map::from_entries(
                entries
                    .into_iter()
                    .map(|(k, v)| (Value::Uint(k), v))
                    .collect(),
            )
            .expect("a map"),
        )
    };
    let good = vec![
        (1, Value::Bytes(pat(0x11, 16))),
        (2, Value::Uint(8192)),
        (3, Value::Uint(2)),
        (4, Value::Uint(1)),
    ];
    let edited = |entries: Vec<(u64, Value)>| edit_map(&bytes, |m| put(m, 16, params_map(entries)));
    assert!(Envelope::from_cbor(&edited(good.clone())).is_ok());
    let mut extra = good.clone();
    extra.push((5, Value::Null));
    assert_eq!(
        refused(&edited(extra)),
        EnvelopeError::BadParams(ParamsError::UnknownKey(5))
    );
    let mut with_crit = good.clone();
    with_crit.push((0, Value::Array(vec![Value::Uint(1)])));
    assert_eq!(
        refused(&edited(with_crit)),
        EnvelopeError::BadParams(ParamsError::UnknownKey(0))
    );
    let mut wrong_type = good.clone();
    wrong_type[1] = (2, Value::Text("8192".into()));
    assert_eq!(
        refused(&edited(wrong_type)),
        EnvelopeError::BadParams(ParamsError::Shape(ShapeError::WrongType(2)))
    );
    let mut short_salt = good.clone();
    short_salt[0] = (1, Value::Bytes(vec![0; 15]));
    assert_eq!(
        refused(&edited(short_salt)),
        EnvelopeError::BadParams(ParamsError::Shape(ShapeError::WrongLength(1)))
    );
    let mut missing = good.clone();
    missing.pop();
    assert_eq!(
        refused(&edited(missing)),
        EnvelopeError::BadParams(ParamsError::Shape(ShapeError::Missing(4)))
    );
    let mut huge = good.clone();
    huge[3] = (4, Value::Uint(1 << 40));
    assert_eq!(
        refused(&edited(huge)),
        EnvelopeError::BadParams(ParamsError::OutOfRange(4))
    );
    let not_a_map = edit_map(&bytes, |m| put(m, 16, Value::Uint(1)));
    assert_eq!(
        refused(&not_a_map),
        EnvelopeError::BadParams(ParamsError::NotAMap)
    );
    let missing_params = edit_map(&bytes, |m| remove(m, 16));
    assert_eq!(refused(&missing_params), EnvelopeError::FieldRequired(16));
    // Algorithm 3 with an `epk`: forbidden.
    let with_epk = edit_map(&bytes, |m| put(m, 4, Value::Bytes(vec![0u8; 32])));
    assert_eq!(refused(&with_epk), EnvelopeError::FieldForbidden(4));
}
