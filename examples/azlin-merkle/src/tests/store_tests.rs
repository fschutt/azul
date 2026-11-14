//! M10 and M11: a failing store is an error and never a wrong root, and the store (hence the log) only grows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::{data, datas, flip, leaf_hashes, li, ts};
use crate::reference;
use crate::{
    leaf_hash, verify_consistency, verify_inclusion, Hash, LeafIndex, LeafStore, MemStore,
    StoreError, Tree, TreeError, TreeSize,
};

/// A store whose `fail_at`-th call (of `size`, `leaf` and `append` together, counted from 1) fails with
/// `Unavailable` and changes nothing; every other call works.
struct FailingStore {
    inner: MemStore,
    calls: Rc<Cell<u64>>,
    fail_at: u64,
}

impl FailingStore {
    fn tick(&self) -> Result<(), StoreError> {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        if n == self.fail_at {
            Err(StoreError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl LeafStore for FailingStore {
    fn size(&self) -> Result<TreeSize, StoreError> {
        self.tick()?;
        self.inner.size()
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        self.tick()?;
        self.inner.leaf(index)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.tick()?;
        self.inner.append(hash)
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Append(usize),
    Root,
    RootAt(usize),
    Inclusion(usize, usize),
    Consistency(usize, usize),
}

/// The leaves the store holds before the tree is built over it (so the rebuild reads some).
const PRE: usize = 5;

/// Appends and every kind of read, at odd, even and power-of-two sizes.
fn script() -> Vec<Op> {
    use Op::*;
    vec![
        Root,
        Append(5),
        Root,
        Append(6),
        RootAt(3),
        Append(7),
        Inclusion(2, 7),
        Append(8),
        Consistency(5, 8),
        Append(9),
        Inclusion(8, 9),
        Append(10),
        RootAt(7),
        Consistency(3, 10),
        Append(11),
        Inclusion(0, 11),
        Append(12),
        Consistency(6, 12),
        Inclusion(11, 12),
        Root,
    ]
}

/// Whether the failing call (number `k`) happened inside an operation that began after `before` calls and
/// ended after `now`: the store stops at the failure, so the call counter reaches `k` exactly there.
fn met(before: u64, now: u64, k: u64) -> bool {
    before < k && now >= k
}

/// Runs the script over a store that fails at call `k`. Returns whether the failing call was reached (so a
/// larger `k` may reach more), after checking every answer against the model.
fn run(k: u64) -> bool {
    let calls = Rc::new(Cell::new(0));
    let mut entries = datas(PRE);
    let store = FailingStore {
        inner: MemStore::from_leaves(leaf_hashes(&entries)),
        calls: calls.clone(),
        fail_at: k,
    };
    let mut tree = match Tree::new(store) {
        Ok(tree) => tree,
        Err(error) => {
            // The failure is the tree's answer, at exactly the failing call.
            assert_eq!(error, TreeError::Store(StoreError::Unavailable), "k = {k}");
            assert_eq!(calls.get(), k, "k = {k}: new went on past the failing call");
            return true;
        }
    };
    assert!(
        calls.get() < k,
        "k = {k}: new succeeded although a call of it failed"
    );
    for (step, op) in script().into_iter().enumerate() {
        let before = calls.get();
        let ctx = format!("k = {k} step {step} {op:?}");
        match op {
            Op::Append(i) => {
                let n = entries.len();
                let result = tree.append(&data(i));
                if met(before, calls.get(), k) {
                    assert_eq!(
                        result,
                        Err(TreeError::Store(StoreError::Unavailable)),
                        "{ctx}"
                    );
                    // Nothing happened: the same size, the model's root, and a retry lands at the same index.
                    assert_eq!(tree.size(), ts(n), "{ctx}: size after a failed append");
                    assert_eq!(
                        tree.root().unwrap(),
                        reference::mth(&entries),
                        "{ctx}: root after it"
                    );
                    assert_eq!(tree.append(&data(i)), Ok(li(n)), "{ctx}: retry");
                } else {
                    assert_eq!(result, Ok(li(n)), "{ctx}");
                }
                entries.push(data(i));
                assert_eq!(tree.size(), ts(entries.len()), "{ctx}");
            }
            Op::Root => {
                let result = tree.root();
                if met(before, calls.get(), k) {
                    assert_eq!(
                        result,
                        Err(TreeError::Store(StoreError::Unavailable)),
                        "{ctx}"
                    );
                } else {
                    assert_eq!(result, Ok(reference::mth(&entries)), "{ctx}");
                }
            }
            Op::RootAt(m) => {
                let result = tree.root_at(ts(m));
                if met(before, calls.get(), k) {
                    assert_eq!(
                        result,
                        Err(TreeError::Store(StoreError::Unavailable)),
                        "{ctx}"
                    );
                } else {
                    assert_eq!(result, Ok(reference::mth(&entries[..m])), "{ctx}");
                }
            }
            Op::Inclusion(index, size) => {
                let result = tree.inclusion_proof(li(index), ts(size));
                if met(before, calls.get(), k) {
                    assert_eq!(
                        result.err(),
                        Some(TreeError::Store(StoreError::Unavailable)),
                        "{ctx}"
                    );
                } else {
                    let proof = result.unwrap_or_else(|e| panic!("{ctx}: {e:?}"));
                    assert_eq!(
                        proof.path,
                        reference::path(index, &entries[..size]),
                        "{ctx}"
                    );
                    let root = reference::mth(&entries[..size]);
                    assert_eq!(
                        verify_inclusion(&leaf_hash(&entries[index]), &proof, &root),
                        Ok(())
                    );
                }
            }
            Op::Consistency(old, new) => {
                let result = tree.consistency_proof(ts(old), ts(new));
                if met(before, calls.get(), k) {
                    assert_eq!(
                        result.err(),
                        Some(TreeError::Store(StoreError::Unavailable)),
                        "{ctx}"
                    );
                } else {
                    let proof = result.unwrap_or_else(|e| panic!("{ctx}: {e:?}"));
                    assert_eq!(proof.path, reference::proof(old, &entries[..new]), "{ctx}");
                    let (a, b) = (
                        reference::mth(&entries[..old]),
                        reference::mth(&entries[..new]),
                    );
                    assert_eq!(verify_consistency(&proof, &a, &b), Ok(()), "{ctx}");
                }
            }
        }
    }
    calls.get() >= k
}

#[test]
fn m10_store_failure_is_an_error_not_a_wrong_root() {
    // The store fails at call 1, then at call 2, ... until a run never reaches the failing call: every
    // call of the tree that met the failure returned it as `Err(Store)`, every answer that came back was
    // the model's, a failed append changed nothing, and the tree went on correctly afterwards.
    for (reached, k) in (1..=2_000u64).enumerate() {
        if !run(k) {
            assert!(
                reached >= 35,
                "only {reached} failure points were exercised"
            );
            return;
        }
    }
    panic!("the script still met the failure at call 2000");
}

/// A store several trees (or a tree and a second writer) can reach: the log has another writer.
#[derive(Clone, Default)]
struct Shared(Rc<RefCell<MemStore>>);

impl LeafStore for Shared {
    fn size(&self) -> Result<TreeSize, StoreError> {
        self.0.borrow().size()
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        self.0.borrow().leaf(index)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.0.borrow_mut().append(hash)
    }
}

#[test]
fn m10_a_store_that_changed_behind_the_tree_is_refused_not_followed() {
    let shared = Shared::default();
    let mut tree = Tree::new(shared.clone()).unwrap();
    tree.append(b"a").unwrap();
    // Another writer appends to the same store: the tree does not guess, it refuses and keeps its answers.
    shared
        .0
        .borrow_mut()
        .append(leaf_hash(b"intruder"))
        .unwrap();
    assert_eq!(tree.append(b"b"), Err(TreeError::StoreSizeChanged));
    assert_eq!(tree.size(), TreeSize(1));
    assert_eq!(tree.root().unwrap(), leaf_hash(b"a"));
    // Rebuilt over the store it sees both leaves and the new root.
    let rebuilt = Tree::new(tree.into_store()).unwrap();
    assert_eq!(rebuilt.size(), TreeSize(2));
    let both = [b"a".to_vec(), b"intruder".to_vec()];
    assert_eq!(rebuilt.root().unwrap(), reference::mth(&both));
}

/// A store whose `append` stores the leaf and then reports a failure (a torn acknowledgement).
struct HalfStore {
    inner: MemStore,
    lie_once: bool,
}

impl LeafStore for HalfStore {
    fn size(&self) -> Result<TreeSize, StoreError> {
        self.inner.size()
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        self.inner.leaf(index)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.inner.append(hash)?;
        if self.lie_once {
            self.lie_once = false;
            return Err(StoreError::Unavailable);
        }
        Ok(())
    }
}

#[test]
fn m10_an_append_that_half_happened_is_found_on_the_next_one() {
    let mut tree = Tree::new(HalfStore {
        inner: MemStore::new(),
        lie_once: false,
    })
    .unwrap();
    for i in 0..3 {
        tree.append(&data(i)).unwrap();
    }
    let known = reference::mth(&datas(3));
    // Re-arm the lie through a rebuilt tree over the same store.
    let mut store = tree.into_store();
    store.lie_once = true;
    let mut tree = Tree::new(store).unwrap();
    assert_eq!(
        tree.append(&data(3)),
        Err(TreeError::Store(StoreError::Unavailable))
    );
    // The tree is as it was and answers for what it knew ...
    assert_eq!(tree.size(), TreeSize(3));
    assert_eq!(tree.root().unwrap(), known);
    assert_eq!(
        tree.root_at(TreeSize(2)).unwrap(),
        reference::mth(&datas(2))
    );
    // ... but it does not append on top of a store that moved: it says so.
    assert_eq!(tree.append(&data(3)), Err(TreeError::StoreSizeChanged));
    assert_eq!(tree.size(), TreeSize(3));
    // Rebuilt over the store, the half-appended leaf is a leaf, and the log goes on.
    let mut rebuilt = Tree::new(tree.into_store()).unwrap();
    assert_eq!(rebuilt.size(), TreeSize(4));
    assert_eq!(rebuilt.root().unwrap(), reference::mth(&datas(4)));
    assert_eq!(rebuilt.append(&data(4)), Ok(LeafIndex(4)));
    assert_eq!(rebuilt.root().unwrap(), reference::mth(&datas(5)));
}

/// The three methods of the trait and nothing else: if `LeafStore` had a fourth required method (a `set`,
/// a `truncate`, a `remove`), this impl would not compile.
struct OnlyThree(Vec<Hash>);

impl LeafStore for OnlyThree {
    fn size(&self) -> Result<TreeSize, StoreError> {
        Ok(TreeSize(self.0.len() as u64))
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        let at = usize::try_from(index.0).map_err(|_| StoreError::IndexOutOfRange)?;
        self.0.get(at).copied().ok_or(StoreError::IndexOutOfRange)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.0.push(hash);
        Ok(())
    }
}

#[test]
fn m11_append_only() {
    let d = datas(10);
    let mut tree = Tree::new(OnlyThree(Vec::new())).unwrap();
    for (i, entry) in d.iter().enumerate() {
        assert_eq!(tree.append(entry), Ok(li(i)));
        // An append never changes what a smaller tree's root was.
        for m in 0..=i + 1 {
            assert_eq!(tree.root_at(ts(m)).unwrap(), reference::mth(&d[..m]));
        }
    }
    // A store whose leaf was altered yields another root, from the altered leaf on and not before: a
    // monitor that keeps the roots it has seen notices.
    let hashes = leaf_hashes(&d);
    let honest = Tree::new(MemStore::from_leaves(hashes.clone())).unwrap();
    for altered_at in 0..10 {
        let mut altered = hashes.clone();
        altered[altered_at] = flip(&altered[altered_at], 3);
        let other = Tree::new(MemStore::from_leaves(altered)).unwrap();
        assert_ne!(
            other.root().unwrap(),
            honest.root().unwrap(),
            "leaf {altered_at}"
        );
        for m in 0..=10 {
            let same = other.root_at(ts(m)).unwrap() == honest.root_at(ts(m)).unwrap();
            assert_eq!(
                same,
                m <= altered_at,
                "leaf {altered_at} altered, root_at({m})"
            );
        }
    }
    // The tree and a proof taken before the alteration no longer fit the altered log.
    let proof = honest.inclusion_proof(LeafIndex(7), TreeSize(10)).unwrap();
    let mut altered = hashes;
    altered[2] = flip(&altered[2], 0);
    let other = Tree::new(MemStore::from_leaves(altered)).unwrap();
    let leaf = leaf_hash(&d[7]);
    assert!(verify_inclusion(&leaf, &proof, &other.root().unwrap()).is_err());
    assert_eq!(
        verify_inclusion(&leaf, &proof, &honest.root().unwrap()),
        Ok(())
    );
}
