//! The append-only tree over a [`LeafStore`] (RFC 9162 sections 2.1.1 - 2.1.4; planning/architecture.md
//! 2.20: "every signed release ... is appended to Azlin's public log, a Merkle tree").
//!
//! The store keeps the leaf hashes; the tree keeps one hash for every COMPLETE subtree above them (leaf
//! pairs, pairs of pairs, ...: at most one hash per leaf in all), which is what makes a root or a proof
//! cost a handful of hash lookups and at most 64 node hashes instead of a pass over the leaves. The cache
//! holds nothing the store cannot give back: [`Tree::new`] rebuilds it with one pass over the stored
//! leaves, so a store whose leaf was altered yields another root.
//!
//! Every call that cannot be answered from the arguments alone checks them first and refuses with a typed
//! error before it reads the store (architecture 1.4 rule 4); every store failure is returned as
//! [`TreeError::Store`], never turned into a root computed from partial data.

use alloc::vec::Vec;
use core::fmt;

use crate::hash::{empty_root, leaf_hash, node_hash, Hash, LeafIndex, TreeSize};
use crate::proof::{ConsistencyProof, InclusionProof, MAX_PROOF_HASHES};
use crate::store::{LeafStore, StoreError};

/// The most leaves a [`Tree`] holds (architecture 1.4 rule 4): 2^32. A store that claims more is refused
/// by [`Tree::new`] before one leaf is read, and a tree at the limit refuses [`Tree::append`]; its proofs
/// then have at most 33 hashes. (The verifiers take any size below 2^64.)
pub const MAX_LEAVES: TreeSize = TreeSize(1 << 32);

/// Why a [`Tree`] call failed. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeError {
    /// The leaf store failed; nothing was computed from partial data, and a failed `append` appended
    /// nothing.
    Store(StoreError),
    /// The leaf index is not below the tree size.
    IndexOutOfRange,
    /// The size asked for is larger than the tree.
    SizeOutOfRange,
    /// Consistency: the older tree is larger than the newer one.
    OldAfterNew,
    /// The store holds, or the append would make, more than [`MAX_LEAVES`] leaves.
    TooManyLeaves,
    /// The store's size is not the number of leaves the tree knows: another writer appended, or an earlier
    /// failed `append` half happened. The tree refuses to go on; rebuild it with [`Tree::new`] over the
    /// store ([`Tree::into_store`] gives it back).
    StoreSizeChanged,
    /// The tree's cache does not fit its size: a bug of this crate, never a normal event.
    Inconsistent,
}

impl From<StoreError> for TreeError {
    fn from(error: StoreError) -> TreeError {
        TreeError::Store(error)
    }
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::Store(error) => write!(f, "the leaf store failed: {error}"),
            TreeError::IndexOutOfRange => f.write_str("the leaf index is not below the tree size"),
            TreeError::SizeOutOfRange => f.write_str("the size is larger than the tree"),
            TreeError::OldAfterNew => f.write_str("the older tree is larger than the newer tree"),
            TreeError::TooManyLeaves => f.write_str("the tree would hold more than MAX_LEAVES"),
            TreeError::StoreSizeChanged => {
                f.write_str("the store's size is not the size of the tree")
            }
            TreeError::Inconsistent => f.write_str("the tree's cache does not fit its size"),
        }
    }
}

impl core::error::Error for TreeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            TreeError::Store(error) => Some(error),
            _ => None,
        }
    }
}

/// The tree over a store. `levels[j - 1][i]` is the hash of the complete subtree of leaves
/// `i * 2^j .. (i + 1) * 2^j` (level 0, the leaves, stays in the store).
pub struct Tree<S: LeafStore> {
    store: S,
    size: u64,
    levels: Vec<Vec<Hash>>,
}

impl<S: LeafStore> Tree<S> {
    /// The tree over the leaves already in `store`: one pass over them (O(n) reads, n - 1 node hashes).
    /// A store claiming more than [`MAX_LEAVES`] is refused before a leaf is read.
    ///
    /// Seam: first non-test callers are S45c (`azlin-tlog`) and AzCtl's release flow (S45-INT).
    pub fn new(store: S) -> Result<Tree<S>, TreeError> {
        let size = store.size()?.0;
        if size > MAX_LEAVES.0 {
            return Err(TreeError::TooManyLeaves);
        }
        let mut levels = Vec::new();
        let mut previous = None;
        for index in 0..size {
            let hash = store.leaf(LeafIndex(index))?;
            let finished = finished_nodes(&levels, index, hash, previous)?;
            commit_nodes(&mut levels, finished);
            previous = Some(hash);
        }
        Ok(Tree {
            store,
            size,
            levels,
        })
    }

    /// The number of leaves.
    pub fn size(&self) -> TreeSize {
        TreeSize(self.size)
    }

    /// Gives the store back (to rebuild the tree after [`TreeError::StoreSizeChanged`]).
    ///
    /// Seam: first caller is S45c (`azlin-tlog`).
    pub fn into_store(self) -> S {
        self.store
    }

    /// Appends the leaf for `data` (its hash is `leaf_hash(data)`) and returns its index.
    pub fn append(&mut self, data: &[u8]) -> Result<LeafIndex, TreeError> {
        self.append_hash(leaf_hash(data))
    }

    /// Appends a leaf whose hash is `hash` (the caller computed `leaf_hash` of the entry) and returns its
    /// index. All reads happen before the store is written, so a failure leaves the tree as it was.
    pub fn append_hash(&mut self, hash: Hash) -> Result<LeafIndex, TreeError> {
        if self.store.size()?.0 != self.size {
            return Err(TreeError::StoreSizeChanged);
        }
        if self.size >= MAX_LEAVES.0 {
            return Err(TreeError::TooManyLeaves);
        }
        let previous = if self.size & 1 == 1 {
            Some(self.store.leaf(LeafIndex(self.size - 1))?)
        } else {
            None
        };
        let finished = finished_nodes(&self.levels, self.size, hash, previous)?;
        self.store.append(hash)?;
        let index = LeafIndex(self.size);
        self.size += 1;
        commit_nodes(&mut self.levels, finished);
        Ok(index)
    }

    /// The root of the whole tree (`MTH(D[n])`; [`empty_root`] for no leaf).
    pub fn root(&self) -> Result<Hash, TreeError> {
        self.range_hash(0, self.size)
    }

    /// The root the tree had when it held `size` leaves (`size <= self.size()`).
    pub fn root_at(&self, size: TreeSize) -> Result<Hash, TreeError> {
        if size.0 > self.size {
            return Err(TreeError::SizeOutOfRange);
        }
        self.range_hash(0, size.0)
    }

    /// The inclusion proof of leaf `index` in the tree of the first `size` leaves (RFC 9162 2.1.3.1,
    /// `PATH(index, D[size])`).
    pub fn inclusion_proof(
        &self,
        index: LeafIndex,
        size: TreeSize,
    ) -> Result<InclusionProof, TreeError> {
        if size.0 > self.size {
            return Err(TreeError::SizeOutOfRange);
        }
        if index.0 >= size.0 {
            return Err(TreeError::IndexOutOfRange);
        }
        // Walk down from the root to the leaf, taking the sibling part at every split; the list is the
        // RFC's, leaf side first, so it is reversed at the end. Every split shrinks the range, and the walk
        // is bounded by the longest proof there is (architecture 1.4 rule 4: no loop without a bound).
        let mut path = Vec::new();
        let (mut low, mut high) = (0u64, size.0);
        for _ in 0..MAX_PROOF_HASHES {
            if high - low <= 1 {
                break;
            }
            let k = largest_power_of_two_below(high - low);
            if index.0 - low < k {
                path.push(self.range_hash(low + k, high)?);
                high = low + k;
            } else {
                path.push(self.range_hash(low, low + k)?);
                low += k;
            }
        }
        if high - low > 1 {
            return Err(TreeError::Inconsistent);
        }
        path.reverse();
        Ok(InclusionProof { index, size, path })
    }

    /// The consistency proof that the tree of the first `old` leaves is a prefix of the tree of the first
    /// `new` leaves (RFC 9162 2.1.4.1, `PROOF(old, D[new])`); empty for `old = 0` and for `old = new`.
    pub fn consistency_proof(
        &self,
        old: TreeSize,
        new: TreeSize,
    ) -> Result<ConsistencyProof, TreeError> {
        if old.0 > new.0 {
            return Err(TreeError::OldAfterNew);
        }
        if new.0 > self.size {
            return Err(TreeError::SizeOutOfRange);
        }
        let mut path = Vec::new();
        if old.0 != 0 && old.0 != new.0 {
            // SUBPROOF(m, D[low..high], b) as a loop; the list is built top-down and reversed. Bounded like
            // the inclusion walk (the last step is the base case, which may add one hash).
            let (mut low, mut high, mut m, mut b) = (0u64, new.0, old.0, true);
            let mut done = false;
            for _ in 0..=MAX_PROOF_HASHES {
                let n = high - low;
                if m == n {
                    if !b {
                        path.push(self.range_hash(low, high)?);
                    }
                    done = true;
                    break;
                }
                let k = largest_power_of_two_below(n);
                if m <= k {
                    path.push(self.range_hash(low + k, high)?);
                    high = low + k;
                } else {
                    path.push(self.range_hash(low, low + k)?);
                    low += k;
                    m -= k;
                    b = false;
                }
            }
            if !done {
                return Err(TreeError::Inconsistent);
            }
            path.reverse();
        }
        Ok(ConsistencyProof { old, new, path })
    }

    /// `MTH(D[low..high])` of the stored leaves `low..high` (`low <= high <= size`): the range is cut into
    /// the complete subtrees of its binary length (largest first, as the RFC's `k` splits it) and those
    /// are folded from the right. Alignment: every range the proofs ask for starts at a multiple of the
    /// size of its first subtree, so each subtree is one lookup.
    fn range_hash(&self, low: u64, high: u64) -> Result<Hash, TreeError> {
        if low > high || high > self.size {
            return Err(TreeError::Inconsistent);
        }
        let mut rest = high - low;
        let mut end = high;
        let mut folded: Option<Hash> = None;
        while rest != 0 {
            let width = rest & rest.wrapping_neg();
            let start = end - width;
            let level = width.trailing_zeros();
            if start & (width - 1) != 0 {
                return Err(TreeError::Inconsistent);
            }
            let hash = self.node(level, start >> level)?;
            folded = Some(match folded {
                Some(right) => node_hash(&hash, &right),
                None => hash,
            });
            end = start;
            rest &= rest - 1;
        }
        Ok(match folded {
            Some(hash) => hash,
            None => empty_root(),
        })
    }

    /// The hash of the complete subtree number `index` of `level` (level 0: the leaf, from the store).
    fn node(&self, level: u32, index: u64) -> Result<Hash, TreeError> {
        let Some(above) = level.checked_sub(1) else {
            return Ok(self.store.leaf(LeafIndex(index))?);
        };
        let nodes = self
            .levels
            .get(above as usize)
            .ok_or(TreeError::Inconsistent)?;
        let at = usize::try_from(index).map_err(|_| TreeError::Inconsistent)?;
        nodes.get(at).copied().ok_or(TreeError::Inconsistent)
    }
}

#[cfg(test)]
impl<S: LeafStore> Tree<S> {
    /// A tree that claims `size` leaves and has no cache, to reach the checks at the limit without 2^32
    /// appends (M8). Test-only: it answers nothing but the refusals.
    pub(crate) fn claiming(store: S, size: u64) -> Tree<S> {
        Tree {
            store,
            size,
            levels: Vec::new(),
        }
    }
}

/// The largest power of two smaller than `n` (RFC 9162 2.1.1's `k`, `k < n <= 2k`); `n >= 2`.
fn largest_power_of_two_below(n: u64) -> u64 {
    1 << (u64::BITS - 1 - n.saturating_sub(1).max(1).leading_zeros())
}

/// The hashes of the complete nodes that leaf number `before` (so `before + 1` leaves in all) finishes,
/// from level 1 upward: none when `before` is even, one more level for every trailing zero bit of the new
/// count. `previous` is the leaf before it (needed, and only then, when `before` is odd); the left child
/// of a node above level 1 is read from `levels`. Nothing is changed: the caller commits the result after
/// the store accepted the leaf.
fn finished_nodes(
    levels: &[Vec<Hash>],
    before: u64,
    leaf: Hash,
    previous: Option<Hash>,
) -> Result<Vec<Hash>, TreeError> {
    let count = before + 1;
    let mut finished = Vec::new();
    let mut right = leaf;
    let mut level = 1u32;
    while level < u64::BITS && count & ((1u64 << level) - 1) == 0 {
        let left = if level == 1 {
            previous.ok_or(TreeError::Inconsistent)?
        } else {
            // The right child is node `(count >> (level - 1)) - 1` of level `level - 1`; its left sibling is
            // the one before it, finished earlier and so already in `levels`.
            let nodes = levels
                .get(level as usize - 2)
                .ok_or(TreeError::Inconsistent)?;
            let at =
                usize::try_from((count >> (level - 1)) - 2).map_err(|_| TreeError::Inconsistent)?;
            nodes.get(at).copied().ok_or(TreeError::Inconsistent)?
        };
        right = node_hash(&left, &right);
        finished.push(right);
        level += 1;
    }
    Ok(finished)
}

/// Puts the nodes of [`finished_nodes`] in `levels` (`finished[0]` is a level 1 node). A level is new
/// exactly when the leaf count became a power of two, one level at a time.
fn commit_nodes(levels: &mut Vec<Vec<Hash>>, finished: Vec<Hash>) {
    for (above, hash) in finished.into_iter().enumerate() {
        match levels.get_mut(above) {
            Some(nodes) => nodes.push(hash),
            None => levels.push(alloc::vec![hash]),
        }
    }
}
