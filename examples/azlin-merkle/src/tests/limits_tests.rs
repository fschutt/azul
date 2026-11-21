//! M8: every limit is checked before any work: a refused call reads nothing, writes nothing and hashes
//! nothing. The armed store panics on the first read or write, which is the proof that none happened.

use std::cell::Cell;
use std::rc::Rc;

use super::{datas, leaf_hashes};
use crate::{
    verify_consistency, verify_inclusion, ConsistencyProof, Hash, InclusionProof, LeafIndex,
    LeafStore, MemStore, ProofError, StoreError, Tree, TreeError, TreeSize, MAX_LEAVES,
    MAX_PROOF_HASHES,
};

/// A store that panics when a leaf is read or appended while `armed` is set, and may claim any size.
struct Armed {
    inner: MemStore,
    armed: Rc<Cell<bool>>,
    claim: Option<u64>,
}

impl LeafStore for Armed {
    fn size(&self) -> Result<TreeSize, StoreError> {
        match self.claim {
            Some(n) => Ok(TreeSize(n)),
            None => self.inner.size(),
        }
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        assert!(
            !self.armed.get(),
            "the store was read (leaf {index:?}) after the refusal point"
        );
        self.inner.leaf(index)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        assert!(
            !self.armed.get(),
            "the store was written after the refusal point"
        );
        self.inner.append(hash)
    }
}

#[test]
fn m8_limits_before_work() {
    // Calls with a size past the tree or an index past the size are refused with a typed error and no
    // store access (a read would panic).
    let armed = Rc::new(Cell::new(false));
    let store = Armed {
        inner: MemStore::from_leaves(leaf_hashes(&datas(10))),
        armed: armed.clone(),
        claim: None,
    };
    let tree = Tree::new(store).unwrap();
    armed.set(true);
    for beyond in [11, 12, 1 << 32, (1 << 32) + 1, u64::MAX - 1, u64::MAX] {
        let size = TreeSize(beyond);
        let too_big = Some(TreeError::SizeOutOfRange);
        assert_eq!(tree.root_at(size).err(), too_big, "root_at({beyond})");
        assert_eq!(tree.inclusion_proof(LeafIndex(0), size).err(), too_big);
        assert_eq!(
            tree.inclusion_proof(LeafIndex(u64::MAX), size).err(),
            too_big
        );
        assert_eq!(tree.consistency_proof(TreeSize(0), size).err(), too_big);
        assert_eq!(tree.consistency_proof(TreeSize(10), size).err(), too_big);
        assert_eq!(tree.consistency_proof(size, size).err(), too_big);
    }
    for index in [10, 11, 1 << 40, u64::MAX] {
        let refused = tree.inclusion_proof(LeafIndex(index), TreeSize(10));
        assert_eq!(
            refused.err(),
            Some(TreeError::IndexOutOfRange),
            "index {index}"
        );
    }
    let empty = tree.inclusion_proof(LeafIndex(0), TreeSize(0));
    assert_eq!(empty.err(), Some(TreeError::IndexOutOfRange));
    for (old, new) in [(4, 3), (u64::MAX, 3), (u64::MAX, u64::MAX - 1), (11, 10)] {
        let refused = tree.consistency_proof(TreeSize(old), TreeSize(new));
        assert_eq!(
            refused.err(),
            Some(TreeError::OldAfterNew),
            "({old}, {new})"
        );
    }
    // The tree is unharmed once the store may be read again.
    armed.set(false);
    assert!(tree.root().is_ok());

    // A store that claims more leaves than the tree holds at most is refused before one leaf is read.
    for claim in [MAX_LEAVES.0 + 1, u64::MAX - 1, u64::MAX] {
        let store = Armed {
            inner: MemStore::new(),
            armed: Rc::new(Cell::new(true)),
            claim: Some(claim),
        };
        assert_eq!(
            Tree::new(store).err(),
            Some(TreeError::TooManyLeaves),
            "claim {claim}"
        );
    }
    // A tree that is full refuses another leaf before the store is read or written.
    let store = Armed {
        inner: MemStore::new(),
        armed: Rc::new(Cell::new(true)),
        claim: Some(MAX_LEAVES.0),
    };
    let mut full = Tree::claiming(store, MAX_LEAVES.0);
    assert_eq!(full.append(b"one more"), Err(TreeError::TooManyLeaves));
    assert_eq!(
        full.append_hash(Hash([0; 32])),
        Err(TreeError::TooManyLeaves)
    );
    assert_eq!(full.size(), MAX_LEAVES);
}

#[test]
fn m8_a_proof_longer_than_any_tree_needs_is_refused_before_the_first_hash() {
    let h = Hash([7; 32]);
    // Too long is checked first: even a size or an index that is nonsense on its own gets TooLong, and the
    // refusal does not depend on the length (it is the length that is refused, not a walk over it).
    for len in [MAX_PROOF_HASHES + 1, MAX_PROOF_HASHES + 2, 1_000, 10_000] {
        for (index, size) in [(0, 1), (0, u64::MAX), (u64::MAX, u64::MAX), (5, 3), (0, 0)] {
            let proof = InclusionProof {
                index: LeafIndex(index),
                size: TreeSize(size),
                path: vec![h; len],
            };
            let verdict = verify_inclusion(&h, &proof, &h);
            assert_eq!(
                verdict,
                Err(ProofError::TooLong),
                "{len} hashes at ({index}, {size})"
            );
        }
        for (old, new) in [(1, u64::MAX), (0, 0), (9, 3), (u64::MAX, u64::MAX), (3, 7)] {
            let proof = ConsistencyProof {
                old: TreeSize(old),
                new: TreeSize(new),
                path: vec![h; len],
            };
            let verdict = verify_consistency(&proof, &h, &h);
            assert_eq!(
                verdict,
                Err(ProofError::TooLong),
                "{len} hashes at ({old}, {new})"
            );
        }
    }
    // The longest honest inclusion proof (position 0 of a tree of u64::MAX leaves) has exactly
    // MAX_PROOF_HASHES hashes: it is hashed (and fails on the roots only), one fewer is a length error.
    let inclusion = |len: usize| InclusionProof {
        index: LeafIndex(0),
        size: TreeSize(u64::MAX),
        path: vec![h; len],
    };
    assert_eq!(
        verify_inclusion(&h, &inclusion(MAX_PROOF_HASHES), &h),
        Err(ProofError::RootMismatch)
    );
    assert_eq!(
        verify_inclusion(&h, &inclusion(MAX_PROOF_HASHES - 1), &h),
        Err(ProofError::PathLength)
    );
    let consistency = |len: usize| ConsistencyProof {
        old: TreeSize(1),
        new: TreeSize(u64::MAX),
        path: vec![h; len],
    };
    assert_eq!(
        verify_consistency(&consistency(MAX_PROOF_HASHES), &h, &h),
        Err(ProofError::RootMismatch)
    );
    assert_eq!(
        verify_consistency(&consistency(MAX_PROOF_HASHES - 1), &h, &h),
        Err(ProofError::PathLength)
    );
    // The one documented gap: a consistency proof from a tree of 3 leaves to one of u64::MAX leaves would
    // need 65 hashes (the bound holds for trees up to 2^63 leaves; the crate's own tree stops at 2^32), so
    // it is refused as too long, and 64 hashes are too few for it.
    let top = |len: usize| ConsistencyProof {
        old: TreeSize(3),
        new: TreeSize(u64::MAX),
        path: vec![h; len],
    };
    assert_eq!(
        verify_consistency(&top(MAX_PROOF_HASHES + 1), &h, &h),
        Err(ProofError::TooLong)
    );
    assert_eq!(
        verify_consistency(&top(MAX_PROOF_HASHES), &h, &h),
        Err(ProofError::PathLength)
    );
    // Sizes at the top of u64 are plain numbers to the verifiers: nothing overflows.
    let at_top = InclusionProof {
        index: LeafIndex(u64::MAX),
        size: TreeSize(u64::MAX),
        path: vec![],
    };
    assert_eq!(
        verify_inclusion(&h, &at_top, &h),
        Err(ProofError::IndexOutOfRange)
    );
    let none = InclusionProof {
        index: LeafIndex(0),
        size: TreeSize(0),
        path: vec![],
    };
    assert_eq!(
        verify_inclusion(&h, &none, &h),
        Err(ProofError::IndexOutOfRange)
    );
    let reversed = ConsistencyProof {
        old: TreeSize(u64::MAX),
        new: TreeSize(3),
        path: vec![],
    };
    assert_eq!(
        verify_consistency(&reversed, &h, &h),
        Err(ProofError::OldAfterNew)
    );
    let equal_top = ConsistencyProof {
        old: TreeSize(u64::MAX),
        new: TreeSize(u64::MAX),
        path: vec![],
    };
    assert_eq!(verify_consistency(&equal_top, &h, &h), Ok(()));
}
