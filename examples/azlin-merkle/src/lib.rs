//! Azlin Merkle: the RFC 9162 Merkle tree of the transparency log, its inclusion and consistency proofs and
//! their verifiers (planning/architecture.md 2.20, 1.4 rules 1-4; build unit S45, lane W1-S45b).
//!
//! RFC 9162 sections 2.1.1 - 2.1.4 (the CT v2 tree) with SHA-256: a leaf is `SHA-256(0x00 || data)`, a node
//! `SHA-256(0x01 || left || right)`, and the tree of `n` leaves splits at `k`, the largest power of two
//! below `n`. Architecture 2.20: every signed release is appended to Azlin's public log, and nodes,
//! AzWorkspace and the apps install a release only with an inclusion proof under a witnessed checkpoint.
//! This crate is the tree and the proofs, nothing else: the signed notes, checkpoints, cosigners and the
//! release record are `azlin-tlog`'s (S45c), which codes against the API below.
//!
//! - [`struct@Hash`], [`LeafIndex`], [`TreeSize`]: the vocabulary. [`leaf_hash`], [`node_hash`], [`empty_root`]:
//!   the hash rules.
//! - [`LeafStore`] (the seam to the disk: three methods, no way to change or remove a leaf) and
//!   [`MemStore`]; [`Tree`] over a store, with [`Tree::root`], [`Tree::root_at`],
//!   [`Tree::inclusion_proof`] and [`Tree::consistency_proof`].
//! - [`verify_inclusion`] and [`verify_consistency`]: pure functions, used by nodes, AzCtl, AzWorkspace,
//!   the apps' updater and (as wasm) the `drive.azlin.io` service worker; hence `sha2` is the only
//!   dependency and the crate is `no_std` with `alloc`.
//! - Limits ([`MAX_PROOF_HASHES`], [`MAX_LEAVES`]) are checked before any work, and every failure is a
//!   typed error ([`StoreError`], [`TreeError`], [`ProofError`]).
//!
//! ```
//! use azlin_merkle::{verify_consistency, verify_inclusion, MemStore, Tree, TreeSize};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut tree = Tree::new(MemStore::new())?;
//! for release in [&b"node image 1"[..], b"drive bundle 7", b"azdrive 0.3.0"] {
//!     tree.append(release)?;
//! }
//! let checkpoint_root = tree.root()?;
//! let leaf = azlin_merkle::leaf_hash(b"drive bundle 7");
//! let proof = tree.inclusion_proof(azlin_merkle::LeafIndex(1), tree.size())?;
//! // The caller compares `proof.size` with the checkpoint's size, then:
//! verify_inclusion(&leaf, &proof, &checkpoint_root)?;
//!
//! // Later the log has grown: the old checkpoint must be a prefix of the new one.
//! tree.append(b"azdrive 0.3.1")?;
//! let consistency = tree.consistency_proof(TreeSize(3), tree.size())?;
//! verify_consistency(&consistency, &checkpoint_root, &tree.root()?)?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod hash;
mod proof;
mod store;
mod tree;

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;

pub use hash::{empty_root, leaf_hash, node_hash, Hash, LeafIndex, TreeSize, HASH_LEN};
pub use proof::{
    verify_consistency, verify_inclusion, ConsistencyProof, InclusionProof, ProofError,
    MAX_PROOF_HASHES,
};
pub use store::{LeafStore, MemStore, StoreError};
pub use tree::{Tree, TreeError, MAX_LEAVES};
