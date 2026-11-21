//! M3, M9 and M13: the roots, against the naive model, for every size, for random operation sequences and
//! at scale.

use super::{datas, leaf_hashes, li, ts, Rng};
use crate::reference;
use crate::{
    leaf_hash, verify_consistency, verify_inclusion, Hash, LeafIndex, MemStore, Tree, TreeError,
    TreeSize,
};

#[test]
fn m3_root_matches_the_model_for_every_size_1_to_130() {
    let all = datas(130);
    // The model's root of every prefix, once: the root of the first m leaves does not depend on n.
    let model: Vec<Hash> = (0..=130).map(|m| reference::mth(&all[..m])).collect();
    let hashes = leaf_hashes(&all);
    let mut grown = Tree::new(MemStore::new()).unwrap();
    for n in 1..=130usize {
        // One tree grown by append, one rebuilt over the stored leaf hashes: both are the model.
        assert_eq!(grown.append(&all[n - 1]).unwrap(), li(n - 1));
        let rebuilt = Tree::new(MemStore::from_leaves(hashes[..n].to_vec())).unwrap();
        for (name, tree) in [("grown", &grown), ("rebuilt", &rebuilt)] {
            assert_eq!(tree.size(), ts(n), "{name} tree of {n}");
            assert_eq!(tree.root().unwrap(), model[n], "{name} root of {n} leaves");
            for m in 0..=n {
                assert_eq!(
                    tree.root_at(ts(m)).unwrap(),
                    model[m],
                    "{name} tree of {n}: root_at({m})"
                );
            }
            assert_eq!(tree.root_at(ts(n + 1)), Err(TreeError::SizeOutOfRange));
        }
    }
}

/// One random operation sequence of M9, checked against the model at every step.
fn run_sequence(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut tree = Tree::new(MemStore::new()).unwrap();
    let mut entries: Vec<Vec<u8>> = Vec::new();
    // model_roots[m] is the model's root of the first m entries.
    let mut model_roots = vec![reference::mth(&[])];
    for step in 0..30 {
        let n = entries.len();
        let ctx = format!("seed {seed:#x} step {step} size {n}");
        match rng.below(9) {
            0..=2 => {
                let len = rng.pick(24);
                let entry: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
                let index = if rng.below(2) == 0 {
                    tree.append(&entry)
                } else {
                    tree.append_hash(leaf_hash(&entry))
                };
                assert_eq!(index, Ok(li(n)), "{ctx}: append");
                entries.push(entry);
                model_roots.push(reference::mth(&entries));
                assert_eq!(tree.size(), ts(n + 1), "{ctx}");
                assert_eq!(
                    tree.root().unwrap(),
                    model_roots[n + 1],
                    "{ctx}: root after append"
                );
            }
            3 => assert_eq!(tree.root().unwrap(), model_roots[n], "{ctx}: root"),
            4 => {
                let m = rng.pick(n + 1);
                assert_eq!(
                    tree.root_at(ts(m)).unwrap(),
                    model_roots[m],
                    "{ctx}: root_at({m})"
                );
            }
            5 if n > 0 => {
                let size = 1 + rng.pick(n);
                let index = rng.pick(size);
                let proof = tree.inclusion_proof(li(index), ts(size)).unwrap();
                assert_eq!(
                    proof.path,
                    reference::path(index, &entries[..size]),
                    "{ctx}: path({index}, {size})"
                );
                let leaf = leaf_hash(&entries[index]);
                assert_eq!(
                    verify_inclusion(&leaf, &proof, &model_roots[size]),
                    Ok(()),
                    "{ctx}"
                );
            }
            6 => {
                let new = rng.pick(n + 1);
                let old = rng.pick(new + 1);
                let proof = tree.consistency_proof(ts(old), ts(new)).unwrap();
                assert_eq!(
                    proof.path,
                    reference::proof(old, &entries[..new]),
                    "{ctx}: proof({old}, {new})"
                );
                let verdict = verify_consistency(&proof, &model_roots[old], &model_roots[new]);
                assert_eq!(verdict, Ok(()), "{ctx}: consistency({old}, {new})");
            }
            _ => {
                // Arguments the tree must refuse, whatever its size.
                let beyond = n as u64 + 1 + rng.below(1000);
                assert_eq!(
                    tree.root_at(TreeSize(beyond)),
                    Err(TreeError::SizeOutOfRange),
                    "{ctx}"
                );
                assert_eq!(
                    tree.inclusion_proof(LeafIndex(n as u64), TreeSize(n as u64)),
                    Err(TreeError::IndexOutOfRange),
                    "{ctx}"
                );
                assert_eq!(
                    tree.inclusion_proof(LeafIndex(0), TreeSize(beyond)),
                    Err(TreeError::SizeOutOfRange),
                    "{ctx}"
                );
                assert_eq!(
                    tree.consistency_proof(TreeSize(0), TreeSize(beyond)),
                    Err(TreeError::SizeOutOfRange),
                    "{ctx}"
                );
                assert_eq!(
                    tree.consistency_proof(TreeSize(n as u64 + 1), TreeSize(n as u64)),
                    Err(TreeError::OldAfterNew),
                    "{ctx}"
                );
            }
        }
    }
}

#[test]
fn m9_incremental_tree_equals_batch_root_property() {
    for case in 0..2_000u64 {
        run_sequence(0x5EED_0000 + case);
    }
}

#[test]
fn m13_scale() {
    const N: usize = 100_000;
    let entries: Vec<Vec<u8>> = (0..N as u64).map(|i| i.to_be_bytes().to_vec()).collect();
    let leaves = leaf_hashes(&entries);
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(tree.append(entry).unwrap(), li(i));
    }
    assert_eq!(tree.size(), ts(N));
    // The root is the model's, and the tree rebuilt over the same leaf hashes has it too.
    let root = tree.root().unwrap();
    assert_eq!(root, reference::mth(&entries));
    let rebuilt = Tree::new(MemStore::from_leaves(leaves.clone())).unwrap();
    assert_eq!(rebuilt.root().unwrap(), root);

    let mut rng = Rng::new(13);
    let mut longest_inclusion = 0;
    for _ in 0..1_000 {
        let size = 1 + rng.below(N as u64);
        let index = rng.below(size);
        let proof = tree
            .inclusion_proof(LeafIndex(index), TreeSize(size))
            .unwrap();
        longest_inclusion = longest_inclusion.max(proof.path.len());
        let at_size = tree.root_at(TreeSize(size)).unwrap();
        assert_eq!(
            verify_inclusion(&leaves[index as usize], &proof, &at_size),
            Ok(())
        );
        assert_eq!(
            verify_inclusion(&leaves[index as usize], &proof, &root).is_ok(),
            size == N as u64
        );
    }
    // The ends, and the sizes around a power of two.
    for size in [1, 2, 65_535, 65_536, 65_537, N as u64] {
        for index in [0, size / 2, size - 1] {
            let proof = tree
                .inclusion_proof(LeafIndex(index), TreeSize(size))
                .unwrap();
            longest_inclusion = longest_inclusion.max(proof.path.len());
            let at_size = tree.root_at(TreeSize(size)).unwrap();
            assert_eq!(
                verify_inclusion(&leaves[index as usize], &proof, &at_size),
                Ok(())
            );
        }
    }
    // ceil(log2(100_000)) = 17 hashes at most per inclusion proof; a consistency proof has at most one more
    // (the first node of its list), 18.
    assert!(
        longest_inclusion <= 17,
        "an inclusion proof of {longest_inclusion} hashes"
    );
    let mut longest_consistency = 0;
    for _ in 0..100 {
        let new = 1 + rng.below(N as u64);
        let old = rng.below(new + 1);
        let proof = tree
            .consistency_proof(TreeSize(old), TreeSize(new))
            .unwrap();
        longest_consistency = longest_consistency.max(proof.path.len());
        let old_root = tree.root_at(TreeSize(old)).unwrap();
        let new_root = tree.root_at(TreeSize(new)).unwrap();
        assert_eq!(
            verify_consistency(&proof, &old_root, &new_root),
            Ok(()),
            "({old}, {new})"
        );
    }
    let proof = tree
        .consistency_proof(TreeSize(3), TreeSize(N as u64))
        .unwrap();
    longest_consistency = longest_consistency.max(proof.path.len());
    assert_eq!(
        verify_consistency(&proof, &tree.root_at(TreeSize(3)).unwrap(), &root),
        Ok(())
    );
    assert!(
        longest_consistency <= 18,
        "a consistency proof of {longest_consistency} hashes"
    );
    assert_eq!(proof.path.len(), 18);
}
