//! C18: no input panics. `open_chunk` and `SealedChunk::from_stored` meet bytes that a node, a peer or a
//! hostile server wrote; the entry point of the fuzz target `chunk_open` runs here on 20,000 seeded
//! inputs, and on the bit flips, truncations, extensions and byte replacements of valid blobs, which are
//! always refused and never panic. (The chunker's part of C18, odd piece sizes, is in
//! `chunker_props.rs`.) The fuzz target file exists and is declared in the fuzz project.

mod azl2_common;

use std::fs;
use std::path::PathBuf;

use azl2_common::fuzz_entry::chunk_open;
use azl2_common::{k_chunk, space, SplitMix64};
use azlin_content::{
    identity, open_chunk, seal_stored, ChunkLen, Codec, OpenAs, SealedChunk, StoredOnly,
};

#[test]
fn c18_fuzz_inputs_never_panic() {
    // 20,000 seeded inputs of the shape the fuzz target reads: selector, declared length, bytes.
    let mut rng = SplitMix64::new(18);
    for _ in 0..20_000u32 {
        let len = rng.below(300) as usize;
        let mut data = rng.bytes(len);
        if len >= 5 && rng.below(2) == 0 {
            // A small declared raw length, so the authentic path passes the length checks too.
            let declared = (rng.below(400) as u32).to_le_bytes();
            data[1..5].copy_from_slice(&declared);
        }
        chunk_open(&data);
        // The same bytes as a raw blob, whatever their length.
        let _ = SealedChunk::from_stored(&data);
    }

    // Valid blobs, mutated: every mutation is refused, none panics.
    let k = k_chunk(18);
    let sp = space(18);
    let mut refused = 0usize;
    for case in 0..200u64 {
        let plain_len = 1 + rng.below(2_000) as usize;
        let plain = rng.bytes(plain_len);
        let (full, id) = identity(&k, &plain);
        let codec = if case % 2 == 0 {
            Codec::Stored
        } else {
            Codec::Zstd
        };
        let blob = seal_stored(&sp, &full, &id, codec, &plain).expect("seals");
        let raw = ChunkLen::from_len(plain.len()).expect("in range");
        let valid = blob.as_bytes().to_vec();
        for _ in 0..50 {
            let mut bytes = valid.clone();
            match rng.below(4) {
                0 => {
                    // one to three flipped bits
                    for _ in 0..=rng.below(3) {
                        let at = rng.below(bytes.len() as u64) as usize;
                        bytes[at] ^= 1 << rng.below(8);
                    }
                }
                1 => bytes.truncate(rng.below(bytes.len() as u64) as usize),
                2 => {
                    let extra_len = 1 + rng.below(40) as usize;
                    bytes.extend_from_slice(&rng.bytes(extra_len));
                }
                _ => {
                    let at = rng.below(bytes.len() as u64) as usize;
                    bytes[at] = bytes[at].wrapping_add(1 + rng.below(255) as u8);
                }
            }
            if bytes == valid {
                continue; // a flip that flipped back (two flips of one bit)
            }
            for who in [OpenAs::Member(&k), OpenAs::Share] {
                let result = open_chunk(&sp, &full, &id, &bytes, raw, who, &StoredOnly);
                assert!(result.is_err(), "case {case}: a mutated blob was opened");
                refused += 1;
            }
        }
    }
    assert!(refused > 15_000, "{refused} refusals");
}

#[test]
fn c18_the_fuzz_target_exists_and_is_declared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = fs::read_to_string(root.join("fuzz/fuzz_targets/chunk_open.rs"))
        .expect("fuzz/fuzz_targets/chunk_open.rs exists");
    assert!(target.contains("fuzz_target!"), "a libfuzzer target");
    assert!(
        target.contains("tests/azl2_common/fuzz_entry.rs"),
        "it runs the entry point the seeded test runs"
    );
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    assert!(manifest.contains("name = \"chunk_open\""));
    assert!(manifest.contains("path = \"fuzz_targets/chunk_open.rs\""));
}
