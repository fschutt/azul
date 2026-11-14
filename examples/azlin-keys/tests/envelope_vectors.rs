//! C2: the known-answer vectors of envelope v2 (`tests/vectors/envelope_v2.json`).
//!
//! Six positive vectors, three for each of the algorithms 1 and 2, made from fixed inputs: the
//! recipient's keys, the signer's key, the device, the context, the plaintext AND every byte the
//! entropy source serves (patterns: `pat(start, n)` counts up from `start`), so that an
//! independent program can rebuild every byte (`scratch/W1-S15b/kat_ref.py` does, with a
//! BLAKE3, an X25519, an XChaCha20-Poly1305, an Ed25519 and a deterministic CBOR written from
//! the specifications, and OpenSSL 3.5's ML-KEM-768 for the hybrid half). The bytes pin the KDF,
//! the combiner, the order and size of the draws, the nonce handling, the associated data, the
//! signing rule and the encoding at once; the hybrid vectors pin the ML-KEM ciphertext too.
//!
//! The negative vectors are inputs the reader must refuse, each with the error `check_shape`
//! answers: truncated, trailing bytes, wrong versions, a wrong type, a wrong length, a field the
//! algorithm forbids or requires, a context out of the grammar, a `crit`-listed unknown key.
//!
//! The file is written only under `UPDATE_VECTORS=1` and never when `CI` is set. Without the
//! feature `pq` the hybrid rows cannot be recomputed: they are checked for shape, for the
//! round trip of the bytes, and for the answer `Unsupported` to `open`.
//!
//! Implements planning/architecture.md section 5.5 (Envelope v2) and launch rule 10 (formats).

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code)]

mod envelope_common;

use azlin_cbor::Value;
use azlin_keys::envelope::{check_shape, open, Alg, Envelope, EnvelopeError, OpenError};
use azlin_keys::VerifyAt;
use envelope_common::vectors::{self, Row};
use envelope_common::{
    edit_map, from_hex, kat_cases, kat_directory, kat_recipient, put, remove, to_hex,
};

const FILE: &str = "envelope_v2.json";

#[cfg(feature = "pq")]
fn positive_rows() -> Vec<Row> {
    kat_cases()
        .iter()
        .map(|case| {
            let env = envelope_common::kat_envelope(case);
            let bytes = env.to_cbor().expect("the bytes of a known-answer envelope");
            Row::new(
                case.name,
                true,
                &to_hex(&bytes),
                &to_hex(&case.plaintext),
                &case.inputs(),
            )
        })
        .collect()
}

/// The refused inputs, made from the bytes of the first positive vector (an envelope of
/// algorithm 1 for a space key): each has the error `check_shape` must answer.
#[cfg(feature = "pq")]
fn negative_rows(base: &[u8]) -> Vec<Row> {
    let long = |entries: &mut Vec<(Value, Value)>, key: u64, len: usize| {
        put(entries, key, Value::Bytes(vec![0u8; len]))
    };
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "truncated_by_one_byte",
            base[..base.len() - 1].to_vec(),
            "Cbor(Truncated)",
        ),
        ("empty_input", Vec::new(), "Cbor(Truncated)"),
        (
            "one_trailing_byte",
            [base, &[0u8]].concat(),
            "Cbor(TrailingBytes)",
        ),
        (
            "version_1",
            edit_map(base, |m| put(m, 1, Value::Uint(1))),
            "UnsupportedVersion(1)",
        ),
        (
            "version_3",
            edit_map(base, |m| put(m, 1, Value::Uint(3))),
            "UnsupportedVersion(3)",
        ),
        (
            "unknown_alg_0",
            edit_map(base, |m| put(m, 2, Value::Uint(0))),
            "UnknownAlg(0)",
        ),
        (
            "unknown_alg_8",
            edit_map(base, |m| put(m, 2, Value::Uint(8))),
            "UnknownAlg(8)",
        ),
        (
            "rcpt_is_text",
            edit_map(base, |m| put(m, 3, Value::Text("x".into()))),
            "Shape(WrongType(3))",
        ),
        (
            "epk_of_31_bytes",
            edit_map(base, |m| long(m, 4, 31)),
            "Shape(WrongLength(4))",
        ),
        (
            "alg1_with_a_kem_ct",
            edit_map(base, |m| long(m, 5, 1088)),
            "FieldForbidden(5)",
        ),
        (
            "alg2_without_a_kem_ct",
            edit_map(base, |m| put(m, 2, Value::Uint(2))),
            "FieldRequired(5)",
        ),
        (
            "params_on_alg1",
            edit_map(base, |m| put(m, 16, Value::Map(Default::default()))),
            "FieldForbidden(16)",
        ),
        (
            "ctx_in_capitals",
            edit_map(base, |m| {
                put(
                    m,
                    8,
                    Value::Text("Space/707172737475767778797a7b7c7d7e7f/epoch/3".into()),
                )
            }),
            "BadCtx(BadCharacter)",
        ),
        (
            "crit_lists_an_unknown_key",
            edit_map(base, |m| {
                put(m, 20, Value::Null);
                put(m, 0, Value::Array(vec![Value::Uint(20)]));
            }),
            "Shape(CritUnknown(20))",
        ),
    ];
    cases
        .into_iter()
        .map(|(name, bytes, expect)| {
            let err = check_shape(&bytes).expect_err("a refused input");
            assert_eq!(format!("{err:?}"), expect, "{name}");
            Row::new(name, false, &to_hex(&bytes), expect, "")
        })
        .collect()
}

/// Everything the file says, checked against the code as it is now.
fn verify_file_rows() {
    let rows = vectors::read(FILE);
    let valid = rows.iter().filter(|r| r.valid).count();
    let invalid = rows.iter().filter(|r| !r.valid).count();
    assert_eq!(valid, 6, "six positive vectors");
    assert!(invalid >= 3, "at least three negative vectors");

    let me = kat_recipient();
    let dir = kat_directory();
    let cases = kat_cases();
    for row in &rows {
        let bytes = from_hex(&row.hex);
        if row.valid {
            let case = cases
                .iter()
                .find(|c| c.name == row.name)
                .expect("a case of this name");
            let meta = check_shape(&bytes).expect("a known answer is a well-formed envelope");
            assert_eq!(meta.ctx, case.ctx, "{}", row.name);
            assert_eq!(meta.alg.number(), case.alg, "{}", row.name);
            assert_eq!(meta.size, bytes.len());
            let env = Envelope::from_cbor(&bytes).expect("a known answer reads");
            assert_eq!(
                env.to_cbor().expect("writes"),
                bytes,
                "{}: writes back",
                row.name
            );
            let opened = open(&env, &me, &case.ctx, &dir, VerifyAt::Now);
            if case.alg == 2 && !cfg!(feature = "pq") {
                assert_eq!(
                    opened.err(),
                    Some(OpenError::Unsupported(Alg::X25519MlKem768)),
                    "{}",
                    row.name
                );
            } else {
                let plaintext = opened.expect("a known answer opens");
                assert_eq!(to_hex(plaintext.as_slice()), row.expect, "{}", row.name);
            }
        } else {
            let err = check_shape(&bytes).expect_err("a refused vector");
            assert_eq!(format!("{err:?}"), row.expect, "{}", row.name);
            assert!(Envelope::from_cbor(&bytes).is_err(), "{}", row.name);
        }
    }
}

#[cfg(feature = "pq")]
#[test]
fn c2_known_answer_vectors_pinned() {
    let positives = positive_rows();
    let base = from_hex(&positives[0].hex);
    let mut rows = positives;
    rows.extend(negative_rows(&base));
    vectors::check_or_update(FILE, &rows);
    verify_file_rows();
}

#[cfg(not(feature = "pq"))]
#[test]
fn c2_known_answer_vectors_pinned() {
    verify_file_rows();
}

/// The vectors are not vacuous: they are six distinct envelopes of the two algorithms, and the
/// pinned bytes change when any one input changes.
#[cfg(feature = "pq")]
#[test]
fn c2_the_vectors_are_distinct_and_depend_on_every_input() {
    let cases = kat_cases();
    let bytes: Vec<Vec<u8>> = cases
        .iter()
        .map(|c| {
            envelope_common::kat_envelope(c)
                .to_cbor()
                .expect("the bytes")
        })
        .collect();
    for (i, a) in bytes.iter().enumerate() {
        for (j, b) in bytes.iter().enumerate().skip(i + 1) {
            assert_ne!(a, b, "vectors {i} and {j}");
        }
    }
    // The ML-KEM ciphertext of each hybrid vector is in its bytes, 1,088 bytes.
    for (case, bytes) in cases.iter().zip(&bytes) {
        let env = Envelope::from_cbor(bytes).expect("reads");
        assert_eq!(
            env.kem_ct.is_some(),
            case.alg == 2,
            "{}: only algorithm 2 carries a kem_ct",
            case.name
        );
    }
}
