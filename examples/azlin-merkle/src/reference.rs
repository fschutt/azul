//! The naive model of the tests (compiled only under `cfg(test)`): RFC 9162 sections 2.1.1 - 2.1.4
//! written exactly as the RFC writes them, as recursions over the list of leaf DATA, with its own
//! SHA-256 calls and none of the optimized tree's code (planning/architecture.md 2.20).
//!
//! It is the authority of every check: the tree's roots and proofs must equal what it computes, hash for
//! hash. Nothing here is fast (every call hashes every leaf again) and nothing here is public API.
//!
//! - [`mth`] is `MTH(D[n])`, with `k` the largest power of two smaller than `n` ([`k_below`]).
//! - [`path`] is `PATH(m, D[n])`; [`subproof`] and [`proof`] are `SUBPROOF(m, D[n], b)` and
//!   `PROOF(m, D[n])`; [`shape`] is the side each node of a `PATH` list stands on.
//! - [`root_from_path`] and [`consistency_holds`] are the verifiers as recursions that read the lists
//!   backwards (a second, independent formulation of RFC 9162 2.1.3.2 and 2.1.4.2: the crate's verifiers
//!   follow the RFC's iterative `fn`/`sn` text).

use sha2::{Digest, Sha256};

use crate::Hash;

/// SHA-256 over the concatenation of `parts`.
fn sha256(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    Hash(hasher.finalize().into())
}

/// `HASH()`: the root of the empty list.
pub fn empty() -> Hash {
    sha256(&[])
}

/// `HASH(0x00 || data)`.
pub fn leaf(data: &[u8]) -> Hash {
    sha256(&[&[0x00], data])
}

/// `HASH(0x01 || left || right)`.
pub fn node(left: &Hash, right: &Hash) -> Hash {
    sha256(&[&[0x01], &left.0, &right.0])
}

/// The largest power of two smaller than `n` (`n >= 2`): `k < n <= 2k`.
pub fn k_below(n: usize) -> usize {
    assert!(n >= 2, "k is defined for n >= 2");
    let mut k = 1;
    while k * 2 < n {
        k *= 2;
    }
    k
}

/// `MTH(D[n])` of the leaf data `d`.
pub fn mth(d: &[Vec<u8>]) -> Hash {
    match d.len() {
        0 => empty(),
        1 => leaf(&d[0]),
        n => {
            let k = k_below(n);
            node(&mth(&d[..k]), &mth(&d[k..]))
        }
    }
}

/// `PATH(m, D[n])`, `0 <= m < n`: the siblings from the leaf up to the root.
pub fn path(m: usize, d: &[Vec<u8>]) -> Vec<Hash> {
    let n = d.len();
    assert!(m < n, "PATH needs m < n");
    if n == 1 {
        return Vec::new();
    }
    let k = k_below(n);
    if m < k {
        let mut p = path(m, &d[..k]);
        p.push(mth(&d[k..]));
        p
    } else {
        let mut p = path(m - k, &d[k..]);
        p.push(mth(&d[..k]));
        p
    }
}

/// Which side of the node computed so far a sibling of a `PATH` list stands on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The sibling is the left child: `HASH(0x01 || sibling || node)`.
    Left,
    /// The sibling is the right child: `HASH(0x01 || node || sibling)`.
    Right,
}

/// The sides of the elements of `PATH(m, D[n])`, in the order of the list (no hashing): two pairs
/// `(m, n)` with the same shape are verified by the same sequence of hashing steps.
pub fn shape(m: usize, n: usize) -> Vec<Side> {
    assert!(m < n, "PATH needs m < n");
    if n == 1 {
        return Vec::new();
    }
    let k = k_below(n);
    if m < k {
        let mut s = shape(m, k);
        s.push(Side::Right);
        s
    } else {
        let mut s = shape(m - k, n - k);
        s.push(Side::Left);
        s
    }
}

/// `SUBPROOF(m, D[n], b)`, `0 < m <= n`.
pub fn subproof(m: usize, d: &[Vec<u8>], b: bool) -> Vec<Hash> {
    let n = d.len();
    assert!(0 < m && m <= n, "SUBPROOF needs 0 < m <= n");
    if m == n {
        return if b { Vec::new() } else { vec![mth(d)] };
    }
    let k = k_below(n);
    if m <= k {
        let mut p = subproof(m, &d[..k], b);
        p.push(mth(&d[k..]));
        p
    } else {
        let mut p = subproof(m - k, &d[k..], false);
        p.push(mth(&d[..k]));
        p
    }
}

/// `PROOF(m, D[n]) = SUBPROOF(m, D[n], true)` for `0 < m <= n`; the crate defines the proof of the empty
/// old tree (`m = 0`) as the empty list (the RFC defines none): the empty tree is a prefix of every tree.
pub fn proof(m: usize, d: &[Vec<u8>]) -> Vec<Hash> {
    if m == 0 {
        Vec::new()
    } else {
        subproof(m, d, true)
    }
}

/// The root that `leaf_hash` at position `m` of `n` leaves reaches through the siblings `path` (listed as
/// `PATH` lists them: leaf side first), reading the recursion of `PATH` backwards: the last element is the
/// sibling of the topmost split. `None` when `m >= n` or the list has not the length `PATH(m, D[n])` has.
pub fn root_from_path(m: usize, n: usize, leaf_hash: Hash, path: &[Hash]) -> Option<Hash> {
    if m >= n {
        return None;
    }
    if n == 1 {
        return path.is_empty().then_some(leaf_hash);
    }
    let k = k_below(n);
    let (last, rest) = path.split_last()?;
    if m < k {
        let left = root_from_path(m, k, leaf_hash, rest)?;
        Some(node(&left, last))
    } else {
        let right = root_from_path(m - k, n - k, leaf_hash, rest)?;
        Some(node(last, &right))
    }
}

/// Whether the model accepts an inclusion proof.
pub fn inclusion_holds(m: usize, n: usize, leaf_hash: Hash, path: &[Hash], root: Hash) -> bool {
    root_from_path(m, n, leaf_hash, path) == Some(root)
}

/// The pair (root of the first `m` leaves, root of the `n` leaves) that a `SUBPROOF(m, D[n], b)` list
/// implies. `old_root` stands for `MTH(D[m])` where the old tree ends exactly at a subproof's end
/// (`b = true`); `None` when the list has the wrong length.
fn rebuild(m: usize, n: usize, b: bool, path: &[Hash], old_root: Hash) -> Option<(Hash, Hash)> {
    if m == 0 || m > n {
        return None;
    }
    if m == n {
        return if b {
            path.is_empty().then_some((old_root, old_root))
        } else {
            match path {
                [only] => Some((*only, *only)),
                _ => None,
            }
        };
    }
    let k = k_below(n);
    let (last, rest) = path.split_last()?;
    if m <= k {
        let (old, new) = rebuild(m, k, b, rest, old_root)?;
        Some((old, node(&new, last)))
    } else {
        let (old, new) = rebuild(m - k, n - k, false, rest, old_root)?;
        Some((node(last, &old), node(last, &new)))
    }
}

/// Whether the model accepts a consistency proof of the tree of `old` leaves (root `old_root`) inside the
/// tree of `new` leaves (root `new_root`): `old = 0` and `old = new` have the empty list and compare roots.
pub fn consistency_holds(
    old: usize,
    new: usize,
    path: &[Hash],
    old_root: Hash,
    new_root: Hash,
) -> bool {
    if old > new {
        return false;
    }
    if old == 0 {
        return path.is_empty() && old_root == empty() && (new != 0 || new_root == old_root);
    }
    if old == new {
        return path.is_empty() && old_root == new_root;
    }
    rebuild(old, new, true, path, old_root) == Some((old_root, new_root))
}
