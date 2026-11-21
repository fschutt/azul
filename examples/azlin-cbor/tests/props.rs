//! C9: the canonical identity property, on seeded random values and on mutated bytes: `decode`
//! accepts only the canonical encoding, so `decode(encode(v)) == v` and, for every byte string `b`
//! that `decode` accepts, `encode(decode(b)) == b`. A failing case prints its seed and index:
//! `SplitMix64::new(seed)` replays the sequence up to it.

mod common;

use azlin_cbor::{decode, encode, Value};
use common::{diag, from_hex, gen_value, load_vectors, mutate, to_hex, SplitMix64, BIG};

const SEEDS: [u64; 8] = [
    1,
    2,
    3,
    0xdead_beef,
    0x5eed,
    0x00c0_ffee,
    0x0123_4567_89ab_cdef,
    u64::MAX,
];
const CASES_PER_SEED: usize = 2_500;

/// Whether `bytes` were accepted; an accepted input must be the canonical encoding of its value.
fn identity(bytes: &[u8], what: &str, seed: u64, case: usize) -> bool {
    match decode(bytes, &BIG) {
        Ok(v) => {
            let again = encode(&v, &BIG).unwrap_or_else(|e| {
                panic!(
                    "seed {seed} case {case} ({what}): {} was accepted but does not encode: {e:?}",
                    to_hex(bytes)
                )
            });
            assert_eq!(
                again,
                bytes,
                "seed {seed} case {case} ({what}): decode accepted {} which is not the canonical encoding of {}",
                to_hex(bytes),
                diag(&v)
            );
            true
        }
        Err(_) => false,
    }
}

/// Every map in `v`, nested ones and map keys included, has its keys in strictly ascending order
/// of their encodings.
fn assert_maps_sorted(v: &Value, seed: u64, case: usize) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| assert_maps_sorted(i, seed, case)),
        Value::Map(m) => {
            let keys: Vec<Vec<u8>> = m
                .iter()
                .map(|(k, _)| encode(k, &BIG).expect("a key encodes"))
                .collect();
            assert!(
                keys.windows(2).all(|w| w[0] < w[1]),
                "seed {seed} case {case}: the keys of {} are not strictly ascending by encoding",
                diag(v)
            );
            for (k, val) in m.iter() {
                assert_maps_sorted(k, seed, case);
                assert_maps_sorted(val, seed, case);
            }
        }
        _ => {}
    }
}

#[test]
fn c9_canonical_identity_property() {
    let (mut cases, mut accepted, mut refused) = (0usize, 0usize, 0usize);
    for seed in SEEDS {
        let mut rng = SplitMix64::new(seed);
        for case in 0..CASES_PER_SEED {
            // decode(encode(v)) == v, and the encoding is canonical.
            let v = gen_value(&mut rng, 4);
            assert_maps_sorted(&v, seed, case);
            let bytes = encode(&v, &BIG).unwrap_or_else(|e| {
                panic!(
                    "seed {seed} case {case}: {} does not encode: {e:?}",
                    diag(&v)
                )
            });
            let back = decode(&bytes, &BIG).unwrap_or_else(|e| {
                panic!(
                    "seed {seed} case {case}: decode refused the encoding of {}: {e:?}",
                    diag(&v)
                )
            });
            assert_eq!(back, v, "seed {seed} case {case}: decode(encode(v)) != v");
            assert!(identity(&bytes, "the encoding", seed, case));
            // encode(decode(b)) == b for every b that decode accepts: mutants of the encoding ...
            for _ in 0..4 {
                let changed = mutate(&mut rng, &bytes);
                if identity(&changed, "mutant", seed, case) {
                    accepted += 1;
                } else {
                    refused += 1;
                }
            }
            // ... and short random bytes.
            let len = rng.below(24) as usize;
            let raw = rng.bytes(len);
            identity(&raw, "random", seed, case);
            cases += 1;
        }
    }
    assert!(cases >= 20_000, "{cases} cases");
    assert!(
        accepted > 2_000 && refused > 2_000,
        "the mutants must be both accepted ({accepted}) and refused ({refused}) often"
    );
}

#[test]
fn c9_mutated_vectors_are_refused_or_canonical() {
    // Every single-bit flip, every deleted byte, every inserted byte value at every position of
    // every positive vector (up to 60 bytes: longer ones get flips and deletions only).
    let rows = load_vectors();
    let (mut accepted, mut refused) = (0usize, 0usize);
    for r in rows.iter().filter(|r| r.valid) {
        let bytes = from_hex(&r.hex);
        let mut check = |b: Vec<u8>| {
            if identity(&b, &r.name, 0, 0) {
                accepted += 1;
            } else {
                refused += 1;
            }
        };
        for i in 0..bytes.len() {
            for bit in 0..8 {
                let mut flipped = bytes.clone();
                flipped[i] ^= 1 << bit;
                check(flipped);
            }
            let mut deleted = bytes.clone();
            deleted.remove(i);
            check(deleted);
        }
        if bytes.len() <= 60 {
            for i in 0..=bytes.len() {
                for value in 0..=255u8 {
                    let mut inserted = bytes.clone();
                    inserted.insert(i, value);
                    check(inserted);
                }
            }
        }
    }
    assert!(
        accepted > 500,
        "{accepted} mutants accepted: some single changes make another valid item"
    );
    assert!(refused > 5_000, "{refused} mutants refused");
}

#[test]
fn c9_every_short_input_is_refused_or_canonical() {
    // All inputs of 0, 1 and 2 bytes, and 3-byte inputs with a third byte from a sample.
    assert!(!identity(&[], "empty", 0, 0));
    let mut accepted = 0usize;
    for a in 0..=255u8 {
        accepted += usize::from(identity(&[a], "one byte", 0, 0));
        for b in 0..=255u8 {
            accepted += usize::from(identity(&[a, b], "two bytes", 0, 0));
            for c in [0x00u8, 0x01, 0x17, 0x18, 0x41, 0x61, 0x80, 0xa0, 0xf6, 0xff] {
                accepted += usize::from(identity(&[a, b, c], "three bytes", 0, 0));
            }
        }
    }
    assert!(accepted > 1_000, "{accepted} short inputs accepted");
}
