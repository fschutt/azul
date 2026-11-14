//! The acceptance checks M1-M14 of the lane brief. Every test name starts with its check number and every
//! test enters through the public API (`Tree`, `verify_*`, `leaf_hash`, ...); the authority is the naive
//! model in `crate::reference`, which shares no code with the tree.
//!
//! - `hash_tests`: M1, M2, M12, M14. `root_tests`: M3, M9, M13. `proof_tests`: M4-M7 and the differential
//!   checks of the two verifiers against the model. `limits_tests`: M8. `store_tests`: M10, M11.

// The loops walk an index and a size in lock step with the model's slices on purpose.
#![allow(clippy::needless_range_loop)]

mod hash_tests;
mod limits_tests;
mod proof_tests;
mod root_tests;
mod store_tests;

use crate::{leaf_hash, Hash, LeafIndex, MemStore, Tree, TreeSize};

/// SplitMix64: a seeded generator for the property tests (the seed is in every failure message).
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`).
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A number below `n` as an index (`n > 0`).
    pub(crate) fn pick(&mut self, n: usize) -> usize {
        self.below(n as u64) as usize
    }

    pub(crate) fn hash(&mut self) -> Hash {
        let mut bytes = [0u8; 32];
        for chunk in bytes.chunks_mut(8) {
            chunk.copy_from_slice(&self.next_u64().to_be_bytes());
        }
        Hash(bytes)
    }
}

/// The `i`-th of a family of distinct byte strings (distinct for distinct `i`, varying lengths).
pub(crate) fn data(i: usize) -> Vec<u8> {
    format!("release {i}").into_bytes()
}

pub(crate) fn datas(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(data).collect()
}

pub(crate) fn leaf_hashes(d: &[Vec<u8>]) -> Vec<Hash> {
    d.iter().map(|x| leaf_hash(x)).collect()
}

/// A tree grown one `append` at a time over these entries.
pub(crate) fn tree_of(d: &[Vec<u8>]) -> Tree<MemStore> {
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for entry in d {
        tree.append(entry).unwrap();
    }
    tree
}

/// The hash with bit `bit` (0..256) flipped.
pub(crate) fn flip(hash: &Hash, bit: usize) -> Hash {
    let mut out = *hash;
    out.0[bit / 8] ^= 1 << (bit % 8);
    out
}

pub(crate) fn hex_bytes(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0, "odd hex length");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

pub(crate) fn hex(s: &str) -> Hash {
    let bytes = hex_bytes(s);
    Hash(bytes.try_into().expect("64 hex digits"))
}

pub(crate) fn ts(n: usize) -> TreeSize {
    TreeSize(n as u64)
}

pub(crate) fn li(n: usize) -> LeafIndex {
    LeafIndex(n as u64)
}
