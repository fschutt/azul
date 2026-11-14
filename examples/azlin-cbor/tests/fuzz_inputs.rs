//! C13: the entry functions of the fuzz targets (`fuzz/fuzz_targets/{decode,int_map}.rs`, which
//! include `tests/common/fuzz_entry.rs` by path) run on inputs that need no fuzzer: the empty
//! input, every 1- and 2-byte input, 20,000 seeded random inputs of 0..512 bytes, structs written
//! by the builder and mutated, and single-bit flips and cuts of every vector. A panic is a failed
//! test here and an abort in the shipped profile (`panic = "abort"`).

mod common;

use azlin_cbor::{encode, IntMapBuilder, Limits, Value};
use common::fuzz_entry::{decode_entry, int_map_entry};
use common::{from_hex, gen_value, load_vectors, mutate, SplitMix64};

fn both(data: &[u8]) {
    decode_entry(data);
    int_map_entry(data);
}

#[test]
fn c13_fuzz_inputs_never_panic() {
    // The empty input, every 1-byte and every 2-byte input.
    both(&[]);
    for a in 0..=255u8 {
        both(&[a]);
        for b in 0..=255u8 {
            both(&[a, b]);
        }
    }
    // 20,000 seeded random inputs of 0..512 bytes.
    let mut rng = SplitMix64::new(0x13);
    for _ in 0..20_000 {
        let len = rng.below(512) as usize;
        let data = rng.bytes(len);
        both(&data);
    }
    // Random bytes behind a plausible head: arrays, maps, strings and integers of every length form
    // followed by random content, so the decoder gets past the first byte.
    for head in 0x00u8..=0xff {
        for _ in 0..8 {
            let len = rng.below(40) as usize;
            let mut data = vec![head];
            data.extend(rng.bytes(len));
            both(&data);
        }
    }
}

#[test]
fn c13_every_vector_flipped_and_cut_never_panics() {
    let rows = load_vectors();
    let mut inputs = 0usize;
    for r in &rows {
        let bytes = from_hex(&r.hex);
        both(&bytes);
        for i in 0..bytes.len() {
            for bit in 0..8 {
                let mut flipped = bytes.clone();
                flipped[i] ^= 1 << bit;
                both(&flipped);
                inputs += 1;
            }
            both(&bytes[..i]);
            both(&bytes[i..]);
            inputs += 2;
        }
    }
    assert!(inputs > 10_000, "{inputs} inputs");
}

#[test]
fn c13_structs_and_their_mutants_never_panic() {
    // Structs as the formats write them (integer keys, `crit`, nested values), whole and damaged.
    let mut rng = SplitMix64::new(0x1313);
    for _ in 0..5_000 {
        let mut builder = IntMapBuilder::new();
        for _ in 0..rng.below(8) {
            let key = 1 + rng.below(20);
            builder = builder.put(key, gen_value(&mut rng, 2));
        }
        let crit: Vec<u64> = (0..rng.below(3)).map(|_| 1 + rng.below(20)).collect();
        let built = builder.crit(&crit).build();
        let Ok(value) = built else {
            // A key put twice: the builder says so; nothing to feed.
            continue;
        };
        let bytes = encode(&value, &Limits::OBJECT).expect("a small struct encodes");
        both(&bytes);
        for _ in 0..4 {
            both(&mutate(&mut rng, &bytes));
        }
        // And the struct inside another value.
        let wrapped = Value::Array(vec![value, Value::Null]);
        both(&encode(&wrapped, &Limits::OBJECT).expect("encodes"));
    }
}
