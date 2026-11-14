//! Inclusion and consistency proofs and their verifiers (RFC 9162 sections 2.1.3 and 2.1.4;
//! planning/architecture.md 2.20: "Nodes, AzWorkspace and the apps install a release only with an
//! inclusion proof under a witnessed checkpoint").
//!
//! The verifiers are pure functions: hashes in, a decision out, no store, no clock, no allocation. They
//! follow the RFC's iterative text (`fn` and `sn` shifted level by level). A proof longer than
//! [`MAX_PROOF_HASHES`] is refused before any hashing, a wrong length for the claimed sizes is refused
//! with its own error, and a proof is accepted only when every hash it implies equals the roots given.

use alloc::vec::Vec;
use core::fmt;

use crate::hash::{empty_root, node_hash, Hash, LeafIndex, TreeSize};

/// The most hashes a proof may carry (architecture 1.4 rule 4): 64, the length of the longest inclusion
/// proof of any tree below 2^64 leaves (and of any consistency proof of a tree up to 2^63 leaves; the
/// crate's own tree stops at [`crate::MAX_LEAVES`], 2^32, so its proofs have at most 33). A longer proof
/// is refused with [`ProofError::TooLong`] before the first hash is computed.
pub const MAX_PROOF_HASHES: usize = 64;

/// RFC 9162 2.1.3.1: the siblings on the way from leaf `index` up to the root of the tree of `size`
/// leaves, leaf side first. `PATH(index, D[size])`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InclusionProof {
    /// The position of the leaf.
    pub index: LeafIndex,
    /// The number of leaves of the tree the proof is for.
    pub size: TreeSize,
    /// The sibling hashes, leaf side first.
    pub path: Vec<Hash>,
}

/// RFC 9162 2.1.4.1: the nodes that show the tree of `old` leaves is a prefix of the tree of `new`
/// leaves. `PROOF(old, D[new])`; empty for `old = 0` and for `old = new` (the roots are compared).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsistencyProof {
    /// The number of leaves of the older tree.
    pub old: TreeSize,
    /// The number of leaves of the newer tree.
    pub new: TreeSize,
    /// The proof hashes, in the order of the RFC's `PROOF` list.
    pub path: Vec<Hash>,
}

/// Why a proof was refused. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProofError {
    /// The proof carries more than [`MAX_PROOF_HASHES`] hashes (refused before any hashing).
    TooLong,
    /// Inclusion: the leaf index is not below the tree size (a tree of no leaf holds no leaf).
    IndexOutOfRange,
    /// Consistency: the older tree is larger than the newer one.
    OldAfterNew,
    /// The proof has more or fewer hashes than the claimed sizes need.
    PathLength,
    /// The hashes were combined and the result is not the root (or roots) given.
    RootMismatch,
}

impl fmt::Display for ProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProofError::TooLong => "the proof has more hashes than any tree needs",
            ProofError::IndexOutOfRange => "the leaf index is not below the tree size",
            ProofError::OldAfterNew => "the older tree is larger than the newer tree",
            ProofError::PathLength => "the proof has the wrong number of hashes for the sizes",
            ProofError::RootMismatch => "the proof does not lead to the root",
        })
    }
}

impl core::error::Error for ProofError {}

/// RFC 9162 2.1.3.2: `leaf` is the hash of the leaf at position `proof.index` of the tree of `proof.size`
/// leaves whose root is `root`.
///
/// The root does not commit to the tree size beyond the shape of the proof: a proof for position `i` of
/// the tree of `n` leaves also verifies for every size `n'` that gives position `i` the same sibling
/// sides (for example sizes 33 and 34 for any `i < 32`), against the same root. A caller that holds a
/// checkpoint therefore compares `proof.size` with the checkpoint's tree size itself (S45c does), exactly
/// as it takes `root` from the checkpoint.
///
/// Seam: first non-test callers are S45c (`azlin-tlog`) and the nodes' release check (architecture 2.20
/// node check 8).
pub fn verify_inclusion(
    leaf: &Hash,
    proof: &InclusionProof,
    root: &Hash,
) -> Result<(), ProofError> {
    if proof.path.len() > MAX_PROOF_HASHES {
        return Err(ProofError::TooLong);
    }
    let (index, size) = (proof.index.0, proof.size.0);
    if index >= size {
        return Err(ProofError::IndexOutOfRange);
    }
    // `f` is the position of the node reached, `s` the last position of its level (RFC 9162 2.1.3.2).
    let mut f = index;
    let mut s = size - 1;
    let mut r = *leaf;
    for p in &proof.path {
        if s == 0 {
            return Err(ProofError::PathLength);
        }
        if f & 1 == 1 || f == s {
            r = node_hash(p, &r);
            if f & 1 == 0 {
                while f & 1 == 0 && f != 0 {
                    f >>= 1;
                    s >>= 1;
                }
            }
        } else {
            r = node_hash(&r, p);
        }
        f >>= 1;
        s >>= 1;
    }
    if s != 0 {
        return Err(ProofError::PathLength);
    }
    if r != *root {
        return Err(ProofError::RootMismatch);
    }
    Ok(())
}

/// RFC 9162 2.1.4.2: the tree of `proof.old` leaves with root `old_root` is a prefix of the tree of
/// `proof.new` leaves with root `new_root`.
///
/// The two edges the RFC leaves open are defined here: for `old = 0` the empty tree is a prefix of every
/// tree, so the proof must be empty and `old_root` must be [`empty_root`] (`new_root` is then any root;
/// for `new = 0` as well it must be `old_root`); for `old = new` the proof must be empty and the two roots
/// equal. Like [`verify_inclusion`] the proof binds the roots only through its shape: the sizes in the
/// proof are the caller's to compare with the checkpoints the roots came from.
///
/// Seam: first non-test callers are S45c (`azlin-tlog`, the monitor) and AzCtl's release flow (S45-INT).
pub fn verify_consistency(
    proof: &ConsistencyProof,
    old_root: &Hash,
    new_root: &Hash,
) -> Result<(), ProofError> {
    if proof.path.len() > MAX_PROOF_HASHES {
        return Err(ProofError::TooLong);
    }
    let (old, new) = (proof.old.0, proof.new.0);
    if old > new {
        return Err(ProofError::OldAfterNew);
    }
    if old == 0 {
        if !proof.path.is_empty() {
            return Err(ProofError::PathLength);
        }
        if *old_root != empty_root() || (new == 0 && *new_root != *old_root) {
            return Err(ProofError::RootMismatch);
        }
        return Ok(());
    }
    if old == new {
        if !proof.path.is_empty() {
            return Err(ProofError::PathLength);
        }
        if *old_root != *new_root {
            return Err(ProofError::RootMismatch);
        }
        return Ok(());
    }
    // 0 < old < new. RFC 9162 2.1.4.2: step 1 puts `old_root` in front of the list when `old` is a power
    // of two (the old tree is then one node of the new one); steps 2 and 3 set `f` and `s`; the first
    // node of the list starts both running roots; every other node is combined level by level.
    let mut nodes: &[Hash] = &proof.path;
    let (mut fr, mut sr) = if old.is_power_of_two() {
        (*old_root, *old_root)
    } else {
        match nodes.split_first() {
            Some((first, rest)) => {
                nodes = rest;
                (*first, *first)
            }
            None => return Err(ProofError::PathLength),
        }
    };
    let mut f = old - 1;
    let mut s = new - 1;
    while f & 1 == 1 {
        f >>= 1;
        s >>= 1;
    }
    for c in nodes {
        if s == 0 {
            return Err(ProofError::PathLength);
        }
        if f & 1 == 1 || f == s {
            fr = node_hash(c, &fr);
            sr = node_hash(c, &sr);
            if f & 1 == 0 {
                while f & 1 == 0 && f != 0 {
                    f >>= 1;
                    s >>= 1;
                }
            }
        } else {
            sr = node_hash(&sr, c);
        }
        f >>= 1;
        s >>= 1;
    }
    if s != 0 {
        return Err(ProofError::PathLength);
    }
    if fr != *old_root || sr != *new_root {
        return Err(ProofError::RootMismatch);
    }
    Ok(())
}
