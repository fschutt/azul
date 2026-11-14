//! C15: no input panics. `PackHeader::parse`, `check_header`, `check_new_pack`, `randomness_check`,
//! `read_trailer` and `rebuild_entries` meet bytes that a node, a peer or a hostile server wrote; the entry
//! points of the fuzz targets `pack_header` and `pack_trailer` run here on 20,000 seeded inputs each, and
//! on the bit flips, cuts and extensions of valid packs, which are always refused and never panic. The
//! fuzz target files exist and are declared in the fuzz project.

mod pack_common;

use std::fs;
use std::path::PathBuf;

use azlin_content::pack::{check_header, check_new_pack, rebuild_entries, PackHeader, Padding};
use azlin_content::{ChunkId, PackClass, PackLen, SealedChunk};
use pack_common::fuzz_entry::{pack_header, pack_trailer};
use pack_common::{build, chunk, k_chunk, scaled, seal_key, small_geometry, space, SplitMix64};

#[test]
fn c15_fuzz_inputs_never_panic() {
    let mut rng = SplitMix64::new(15);

    // 20,000 seeded inputs of the shape each target reads: bytes of any length. (`pack_trailer` builds,
    // flips and reads a real pack per input: 20,000 of them in an optimised build, 4,000 in a debug one,
    // where the ciphers are slow.)
    let trailer_inputs = scaled("c15 pack_trailer inputs", 20_000, 4_000);
    for i in 0..20_000usize {
        let len = rng.below(700) as usize;
        let data = rng.bytes(len);
        pack_header(&data);
        if i < trailer_inputs {
            pack_trailer(&data);
        }
    }

    // Structured inputs for `pack_header`: the header of a real pack under a declared length that is right,
    // one byte off, and random; and with one to three bytes of the header changed. Every field gets its turn.
    let sp = space(71);
    let k = k_chunk(72);
    let real = build(
        &sp,
        PackClass::Content,
        3,
        0x44,
        small_geometry(),
        Padding::Buckets,
        true,
        &[chunk(&sp, &k, 1, 300), chunk(&sp, &k, 2, 900)],
        1,
    );
    let total = real.len().get();
    let mut accepted = 0usize;
    for i in 0..20_000u32 {
        let mut header = real.bytes()[..24].to_vec();
        match i % 4 {
            0 => {}
            1 => {
                for _ in 0..=rng.below(3) {
                    let at = rng.below(24) as usize;
                    header[at] = rng.next_u64() as u8;
                }
            }
            2 => {
                let at = rng.below(24) as usize;
                header[at] ^= 1 << rng.below(8);
            }
            _ => {
                // A size field set to a number around an edge of the rules.
                let field = 12 + 4 * rng.below(3) as usize;
                let edge = [
                    0u32,
                    1,
                    2,
                    40,
                    41,
                    82,
                    100,
                    122,
                    262,
                    4096,
                    16_384,
                    16_385,
                    u32::MAX,
                ][rng.below(13) as usize];
                header[field..field + 4].copy_from_slice(&edge.to_le_bytes());
            }
        }
        let declared = match rng.below(3) {
            0 => total,
            1 => total.wrapping_add(rng.below(3) as u32).wrapping_sub(1),
            _ => rng.next_u64() as u32,
        };
        let mut data = declared.to_le_bytes().to_vec();
        data.extend_from_slice(&header);
        let tail = rng.below(40) as usize;
        data.extend_from_slice(&rng.bytes(tail));
        pack_header(&data);
        accepted += usize::from(PackHeader::parse(&header, PackLen::new(declared)).is_ok());
    }
    println!("c15: {accepted} of the 20,000 structured headers were accepted, the rest refused");
    assert!(
        accepted > 1_000,
        "the structured inputs reach the accepting paths ({accepted})"
    );

    // Valid packs, flipped, cut and extended: always refused, never a panic.
    let packs = scaled("c15 packs", 80, 25);
    let key = seal_key(3, 0x44);
    let mut refusals = 0usize;
    for seed in 0..packs as u64 {
        let n = 1 + rng.below(6) as usize;
        let chunks: Vec<(ChunkId, SealedChunk)> = (0..n)
            .map(|i| {
                chunk(
                    &sp,
                    &k,
                    500 + seed * 10 + i as u64,
                    1 + rng.below(1_500) as usize,
                )
            })
            .collect();
        let pack = build(
            &sp,
            PackClass::ALL[(seed % 3) as usize],
            3,
            0x44,
            small_geometry(),
            Padding::Buckets,
            seed.is_multiple_of(2),
            &chunks,
            seed,
        );
        for _ in 0..150 {
            let mut bytes = pack.bytes().to_vec();
            match rng.below(4) {
                0 => {
                    for _ in 0..=rng.below(3) {
                        let at = rng.below(bytes.len() as u64) as usize;
                        bytes[at] ^= 1 << rng.below(8);
                    }
                }
                1 => bytes.truncate(rng.below(bytes.len() as u64) as usize),
                2 => {
                    let extra = 1 + rng.below(40) as usize;
                    bytes.extend_from_slice(&rng.bytes(extra));
                }
                _ => {
                    let at = rng.below(bytes.len() as u64) as usize;
                    bytes[at] = bytes[at].wrapping_add(1 + rng.below(255) as u8);
                }
            }
            if bytes == pack.bytes() {
                continue; // two flips of one bit
            }
            assert!(
                rebuild_entries(&bytes, &sp, &key).is_err(),
                "a changed pack was accepted"
            );
            // The node's checks on the same bytes: no panic, whatever they say.
            let first = &bytes[..bytes.len().min(24 + 4096)];
            let _ = check_header(first, PackLen::from_len(bytes.len()).expect("fits"));
            let _ = check_new_pack(first, PackLen::from_len(bytes.len()).expect("fits"));
            refusals += 1;
        }
    }
    println!("c15: {refusals} changed packs refused");
    assert!(refusals > 2_000, "{refusals} changed packs");
}

#[test]
fn c15_the_fuzz_targets_exist_and_are_declared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    for name in ["pack_header", "pack_trailer"] {
        let target = fs::read_to_string(root.join(format!("fuzz/fuzz_targets/{name}.rs")))
            .unwrap_or_else(|_| panic!("fuzz/fuzz_targets/{name}.rs exists"));
        assert!(
            target.contains("fuzz_target!"),
            "{name}: a libfuzzer target"
        );
        assert!(
            target.contains("tests/pack_common/fuzz_entry.rs"),
            "{name}: it runs the entry point the seeded test runs"
        );
        assert!(
            target.contains(&format!("fuzz_entry::{name}(data)")),
            "{name}: its own entry"
        );
        assert!(
            manifest.contains(&format!("name = \"{name}\"")),
            "{name}: declared"
        );
        assert!(
            manifest.contains(&format!("path = \"fuzz_targets/{name}.rs\"")),
            "{name}: its path"
        );
    }
}
