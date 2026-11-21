//! The hash rules of RFC 9162 section 2.1.1 (the CT v2 Merkle tree, SHA-256) and the three small types
//! the crate speaks in (planning/architecture.md 2.20).
//!
//! A leaf is hashed as `SHA-256(0x00 || data)`, a node as `SHA-256(0x01 || left || right)`, the empty tree
//! as `SHA-256("")`. The two prefixes keep a leaf from ever being read as a node (the second-preimage
//! attack of a tree without them).

use core::fmt;

use sha2::{Digest, Sha256};

/// The length of a hash in bytes (SHA-256).
pub const HASH_LEN: usize = 32;

/// A Merkle tree hash: the hash of a leaf, of a node, or of the empty tree. Nothing here is secret: a
/// hash is public data, so it is `Copy`, comparable and printable; `Debug` and `Display` both print the
/// 64 lower-case hex digits and nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(pub [u8; HASH_LEN]);

impl Hash {
    /// The 32 bytes of the hash.
    pub fn as_bytes(&self) -> &[u8; HASH_LEN] {
        &self.0
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The position of a leaf in the tree, counted from 0 in the order the leaves were appended.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LeafIndex(pub u64);

/// The number of leaves a tree (or a prefix of a tree) holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TreeSize(pub u64);

/// The root of the tree with no leaf: `SHA-256("")` (RFC 9162 2.1.1, `MTH({})`).
pub fn empty_root() -> Hash {
    Hash(Sha256::digest(b"").into())
}

/// The hash of one leaf: `SHA-256(0x00 || data)` (RFC 9162 2.1.1, `MTH({d(0)})`).
pub fn leaf_hash(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([0x00u8]);
    hasher.update(data);
    Hash(hasher.finalize().into())
}

/// The hash of an inner node over its two children: `SHA-256(0x01 || left || right)` (RFC 9162 2.1.1).
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([0x01u8]);
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    Hash(hasher.finalize().into())
}
