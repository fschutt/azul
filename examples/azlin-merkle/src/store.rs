//! Where the leaf hashes live (planning/architecture.md 2.20; 1.4 rule 1: I/O is a parameter).
//!
//! The tree never touches a disk: it asks a [`LeafStore`]. Production passes a file of 32-byte hashes or a
//! set of database rows (a later unit); tests pass [`MemStore`]. The trait has no `set` and no `truncate`:
//! a log only grows, and the one way to change an old leaf is to hand the tree another store, which then
//! has another root (so a monitor that keeps the roots it has seen notices).

use alloc::vec::Vec;
use core::fmt;

use crate::hash::{Hash, LeafIndex, TreeSize};

/// Why a [`LeafStore`] call failed. Never a string: a caller decides on the variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// `leaf` was asked for a position at or past the store's size.
    IndexOutOfRange,
    /// The medium could not be read or written now (a failed disk, a lost connection): retry later.
    Unavailable,
    /// The medium holds bytes that are no leaf hash (a torn file, a flipped bit a check found).
    Corrupt,
    /// The store has no room for another leaf.
    Full,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StoreError::IndexOutOfRange => "the leaf is not in the store",
            StoreError::Unavailable => "the store could not be read or written now",
            StoreError::Corrupt => "the store holds bytes that are no leaf hash",
            StoreError::Full => "the store has no room for another leaf",
        })
    }
}

impl core::error::Error for StoreError {}

/// The leaf hashes of a tree, in the order they were appended.
///
/// Seam: first non-test callers are S45c (`azlin-tlog`) and AzCtl's release flow (S45-INT), which pass a
/// durable store (a file of hashes with an fsync discipline, or database rows).
///
/// The contract, which [`crate::Tree`] relies on: `size` is the number of leaves `append` has accepted;
/// `leaf(i)` for `i < size` returns exactly the hash appended as leaf `i`, every time; an `append` that
/// fails has not appended (a store that cannot promise that makes the next [`crate::Tree::append`] find
/// the size changed and refuse). The trait has these three methods and no others: it has no `set`, no
/// `truncate`, no `remove` (append-only).
pub trait LeafStore {
    /// The number of leaves in the store.
    fn size(&self) -> Result<TreeSize, StoreError>;
    /// The hash of leaf `index`, or [`StoreError::IndexOutOfRange`] when `index >= size`.
    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError>;
    /// Adds a leaf hash at the end; the new leaf's index is the size before the call.
    fn append(&mut self, hash: Hash) -> Result<(), StoreError>;
}

/// A [`LeafStore`] in memory: the store of the tests and of a log that is rebuilt on every start. Its only
/// limit is memory (the tree above it stops at [`crate::MAX_LEAVES`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemStore {
    leaves: Vec<Hash>,
}

impl MemStore {
    /// An empty store.
    pub fn new() -> MemStore {
        MemStore::default()
    }

    /// A store that already holds these leaf hashes (a log reopened from its rows); leaf `i` is
    /// `leaves[i]`.
    pub fn from_leaves(leaves: Vec<Hash>) -> MemStore {
        MemStore { leaves }
    }
}

impl LeafStore for MemStore {
    fn size(&self) -> Result<TreeSize, StoreError> {
        // `usize` is at most 64 bits on every target the crate builds for.
        Ok(TreeSize(self.leaves.len() as u64))
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        let at = usize::try_from(index.0).map_err(|_| StoreError::IndexOutOfRange)?;
        self.leaves
            .get(at)
            .copied()
            .ok_or(StoreError::IndexOutOfRange)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.leaves.push(hash);
        Ok(())
    }
}
