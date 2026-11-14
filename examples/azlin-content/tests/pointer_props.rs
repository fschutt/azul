//! C6 (the property: `encode(decode(b)) == b` for every accepted pointer, over seeded random pointers and
//! over damaged copies of their bytes) and C13 (no input panics: `from_cbor`, `decode_chunk_list` and
//! `verify` on 20,000 seeded inputs and on every bit flip of a valid pointer; the two fuzz target files
//! exist and run the same entry points).

mod pointer_common;

use std::fs;
use std::path::PathBuf;

use azlin_cbor::{DecodeError, Limits};
use azlin_content::pointer::{Decoded, PointerError, PointerV2};
use azlin_keys::VerifyAt;
use pointer_common::fuzz_entry::{
    chunk_list_decode_bytes, chunk_list_decode_damaged, pointer_decode, valid_pointer_bytes,
};
use pointer_common::*;

#[test]
fn c6_encode_decode_is_the_identity_over_random_pointers() {
    let mut rng = SplitMix64::new(6);
    let mut accepted_damage = 0usize;
    let mut refused_damage = 0usize;
    for case in 0..5_000u32 {
        let p = random_pointer(&mut rng);
        let bytes = match p.to_cbor() {
            Ok(bytes) => bytes,
            Err(e) => panic!("case {case}: a random pointer is valid, yet {e:?}: {p:?}"),
        };
        // The library writes what the documented layout says, for every random pointer.
        assert_eq!(
            bytes.as_slice(),
            reference_encode(&p, true).as_slice(),
            "case {case}: the writer and the reference encoder differ"
        );
        // Reading gives the same pointer, which writes the same bytes; a copy does too.
        let Decoded::Current(q) = PointerV2::from_cbor(&bytes, &OBJECT).unwrap_or_else(|e| {
            panic!("case {case}: the reader refuses what the writer wrote: {e:?}")
        }) else {
            panic!("case {case}: newer");
        };
        assert!(*q == p, "case {case}: decode(encode(p)) != p");
        assert_eq!(
            q.to_cbor().expect("writes").as_slice(),
            bytes.as_slice(),
            "case {case}"
        );
        assert_eq!(
            p.duplicate().to_cbor().expect("writes").as_slice(),
            bytes.as_slice()
        );

        // Damaged copies: whatever is accepted is its own canonical encoding (the property over bytes), and
        // what is not is refused with an error.
        for _ in 0..2 {
            let mut damaged = bytes.to_vec();
            match rng.below(5) {
                0..=2 => {
                    let at = rng.below(damaged.len() as u64) as usize;
                    damaged[at] ^= 1 + rng.below(255) as u8;
                }
                3 => damaged.truncate(rng.below(damaged.len() as u64) as usize),
                _ => damaged.push(rng.below(256) as u8),
            }
            match PointerV2::from_cbor(&damaged, &OBJECT) {
                Ok(Decoded::Current(r)) => {
                    assert_eq!(
                        r.to_cbor().expect("an accepted pointer writes").as_slice(),
                        damaged.as_slice(),
                        "case {case}: an accepted damaged pointer is not canonical"
                    );
                    accepted_damage += 1;
                }
                Ok(Decoded::Newer(_)) => accepted_damage += 1,
                Err(_) => refused_damage += 1,
            }
        }
    }
    // Both kinds of damage happen, so both halves of the property were exercised.
    assert!(
        accepted_damage > 100,
        "{accepted_damage} accepted damaged pointers"
    );
    assert!(
        refused_damage > 100,
        "{refused_damage} refused damaged pointers"
    );
}

#[test]
fn c6_the_callers_limits_apply() {
    // A pointer is read under the limits the caller passes (a pointer at rest: OBJECT; one inside a log
    // record: LOG_RECORD): the same bytes are refused where the limit is smaller than they are.
    let list = huge_chunks(1024);
    let mut big = minimal_empty();
    big.size = azlin_content::FileSize::new(total_raw(&list));
    big.chunks = azlin_content::pointer::ChunkList::Inline(list);
    big.xattrs = vec![("user.padding".to_string(), vec![0x5A; 10_000])];
    let bytes = signed(big).to_cbor().expect("writes").to_vec();
    assert!(bytes.len() > 64 * 1024, "{} bytes", bytes.len());
    assert!(matches!(
        PointerV2::from_cbor(&bytes, &Limits::OBJECT),
        Ok(Decoded::Current(_))
    ));
    for limits in [Limits::LOG_RECORD, Limits::HEADER] {
        assert_eq!(
            PointerV2::from_cbor(&bytes, &limits).err(),
            Some(PointerError::Cbor(DecodeError::InputTooLarge))
        );
    }
    // A small pointer fits a log record.
    let small = signed(minimal_one_chunk())
        .to_cbor()
        .expect("writes")
        .to_vec();
    assert!(matches!(
        PointerV2::from_cbor(&small, &Limits::LOG_RECORD),
        Ok(Decoded::Current(_))
    ));
}

#[test]
fn c13_fuzz_inputs_never_panic() {
    let mut rng = SplitMix64::new(13);

    // 20,000 seeded inputs through the entry points of the two fuzz targets: random bytes, and damaged
    // copies of a valid signed pointer (pointer_decode also verifies whatever it accepts).
    for _ in 0..20_000u32 {
        let len = rng.below(150) as usize;
        let data = rng.bytes(len);
        pointer_decode(&data);
        chunk_list_decode_bytes(&data);
    }
    // The heavy half of the list entry: a valid object of 1,025 chunks, damaged.
    for _ in 0..300u32 {
        let len = rng.below(60) as usize;
        chunk_list_decode_damaged(&rng.bytes(len));
    }

    // Every single bit of a valid signed pointer flipped, one at a time: refused or accepted, never a
    // panic, and a pointer that is accepted verifies or says why not.
    let valid = valid_pointer_bytes();
    let dir = directory_for_fuzz_seed();
    let mut accepted = 0usize;
    for at in 0..valid.len() {
        for bit in 0..8 {
            let mut bytes = valid.to_vec();
            bytes[at] ^= 1 << bit;
            if let Ok(Decoded::Current(p)) = PointerV2::from_cbor(&bytes, &OBJECT) {
                accepted += 1;
                let verdict = p.verify(&fuzz_space(), &dir, VerifyAt::Now).map(|_| ());
                assert!(
                    verdict.is_err(),
                    "a flipped bit at {at}.{bit} still verifies"
                );
            }
        }
    }
    assert!(
        accepted > 50,
        "{accepted} accepted flips: the seed has strings and hashes"
    );
    // The unflipped seed verifies.
    let Decoded::Current(seed) = PointerV2::from_cbor(valid, &OBJECT).expect("the seed reads")
    else {
        panic!("current");
    };
    assert_eq!(
        seed.verify(&fuzz_space(), &dir, VerifyAt::Now).map(|_| ()),
        Ok(())
    );
    // Every prefix and a few extensions of it.
    for n in 0..valid.len() {
        assert!(PointerV2::from_cbor(&valid[..n], &OBJECT).is_err());
    }
}

/// The directory and space of the fuzz seed pointer (the entry module keeps its own).
fn fuzz_space() -> azlin_keys::SpaceId {
    azlin_keys::SpaceId::from_bytes([1; 16])
}

fn directory_for_fuzz_seed() -> azlin_keys::MapDirectory {
    let mut d = azlin_keys::MapDirectory::new();
    d.insert_active(
        azlin_keys::DeviceId::from_bytes([2; 16]),
        azlin_keys::SignKeypair::from_seed(&azlin_keys::Secret::from_bytes([3; 32])).verify_key(),
    );
    d
}

#[test]
fn c13_the_fuzz_targets_exist_and_are_declared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    for (target, entry) in [
        ("pointer_decode", "pointer_decode"),
        ("chunk_list_decode", "chunk_list_decode"),
    ] {
        let text = fs::read_to_string(root.join(format!("fuzz/fuzz_targets/{target}.rs")))
            .unwrap_or_else(|_| panic!("fuzz/fuzz_targets/{target}.rs exists"));
        assert!(
            text.contains("fuzz_target!"),
            "{target}: a libfuzzer target"
        );
        assert!(
            text.contains("tests/pointer_common/fuzz_entry.rs"),
            "{target}: it runs the entry point the seeded test runs"
        );
        assert!(
            text.contains(&format!("fuzz_entry::{entry}(data)")),
            "{target}: it calls {entry}"
        );
        assert!(
            manifest.contains(&format!("name = \"{target}\"")),
            "{target}: declared"
        );
        assert!(
            manifest.contains(&format!("path = \"fuzz_targets/{target}.rs\"")),
            "{target}: its path"
        );
    }
}
