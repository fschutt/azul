//! C1, the text parsers on inputs that need no fuzzer: the entry function of
//! `fuzz/fuzz_targets/id_text.rs` (which includes `tests/common/fuzz_entry.rs` by path) runs on
//! the empty input, every 1- and 2-byte input, 20,000 seeded random inputs, and 15,000 valid
//! texts of the three forms (16-byte id, key id, epoch) with one to three random edits. A panic
//! is a failed test here and an abort in the shipped profile (`panic = "abort"`); a text a parser
//! accepts must be the canonical text of its value (planning/architecture.md section 1.4 rule 3).

mod common;

use azlin_keys::{Epoch, KeyId, SpaceId};
use common::fuzz_entry::id_text_entry;
use common::SplitMix64;

/// The characters an edit draws from: the digits and letters of the forms, their uppercase twins,
/// and the characters a lenient parser might let through.
const ALPHABET: &[u8] = b"0123456789abcdefABCDEFgGxX +-_.\t\n\0";

fn pick(rng: &mut SplitMix64) -> u8 {
    let at = rng.below(ALPHABET.len() as u64) as usize;
    ALPHABET.get(at).copied().unwrap_or(b'0')
}

/// One random edit: change a byte, insert one, delete one, uppercase one, or append one.
fn edit(bytes: &mut Vec<u8>, rng: &mut SplitMix64) {
    let len = bytes.len() as u64;
    match rng.below(5) {
        0 if len > 0 => {
            let i = rng.below(len) as usize;
            bytes[i] = pick(rng);
        }
        1 => {
            let i = rng.below(len + 1) as usize;
            let b = pick(rng);
            bytes.insert(i, b);
        }
        2 if len > 0 => {
            let i = rng.below(len) as usize;
            bytes.remove(i);
        }
        3 if len > 0 => {
            let i = rng.below(len) as usize;
            bytes[i] = bytes[i].to_ascii_uppercase();
        }
        _ => {
            let b = pick(rng);
            bytes.push(b);
        }
    }
}

#[test]
fn c1_fuzz_inputs_never_panic_and_only_canonical_text_is_accepted() {
    // The empty input, every 1-byte and every 2-byte input.
    id_text_entry(&[]);
    for a in 0..=255u8 {
        id_text_entry(&[a]);
        for b in 0..=255u8 {
            id_text_entry(&[a, b]);
        }
    }
    // 20,000 seeded random inputs of 0..80 bytes.
    let mut rng = SplitMix64::new(0xC1F);
    for _ in 0..20_000 {
        let len = rng.below(80) as usize;
        id_text_entry(&rng.bytes(len));
    }
    // Valid texts with one to three edits: the inputs next to the forms, where a lenient parser
    // would let a second spelling through. The unedited text is accepted and must be canonical.
    for kind in 0..3u8 {
        for _ in 0..5_000 {
            let valid = match kind {
                0 => SpaceId::from_bytes(<[u8; 16]>::try_from(rng.bytes(16)).expect("16"))
                    .to_string(),
                1 => {
                    KeyId::from_bytes(<[u8; 32]>::try_from(rng.bytes(32)).expect("32")).to_string()
                }
                _ => Epoch::new(1 + rng.below(u64::from(u32::MAX)) as u32)
                    .expect("a nonzero epoch")
                    .to_string(),
            };
            id_text_entry(valid.as_bytes());
            let mut bytes = valid.into_bytes();
            for _ in 0..1 + rng.below(3) {
                edit(&mut bytes, &mut rng);
            }
            id_text_entry(&bytes);
        }
    }
}

/// The oracle is not vacuous: the unedited valid texts are accepted by the parser they were made
/// for (so the canonical check ran on accepted input), and one edit that only changes the case
/// of a hex letter is refused.
#[test]
fn c1_the_fuzz_oracle_sees_accepted_and_refused_texts() {
    let id = SpaceId::from_bytes([0xab; 16]);
    let text = id.to_string();
    assert_eq!(text.parse::<SpaceId>(), Ok(id));
    id_text_entry(text.as_bytes());
    let mut upper = text.into_bytes();
    upper[0] = upper[0].to_ascii_uppercase();
    assert!(std::str::from_utf8(&upper)
        .expect("ascii")
        .parse::<SpaceId>()
        .is_err());
    id_text_entry(&upper);
}
