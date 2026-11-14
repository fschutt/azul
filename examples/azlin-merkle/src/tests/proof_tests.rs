//! M4-M7: the proofs against the model for every index and size (or pair) up to 65, the tightness of the two
//! verifiers, and two differential checks of the verifiers (the RFC's iterative text) against the model's
//! recursive formulation on thousands of damaged proofs.

use super::{datas, flip, leaf_hashes, li, tree_of, ts, Rng};
use crate::reference;
use crate::{
    empty_root, verify_consistency, verify_inclusion, ConsistencyProof, Hash, InclusionProof,
    LeafIndex, ProofError, TreeError, TreeSize,
};

#[test]
fn m4_inclusion_proofs_verify_for_every_index_and_size_up_to_65() {
    let d = datas(65);
    let leaves = leaf_hashes(&d);
    let tree = tree_of(&d);
    for size in 1..=65usize {
        let root = tree.root_at(ts(size)).unwrap();
        assert_eq!(root, reference::mth(&d[..size]));
        for index in 0..size {
            let proof = tree.inclusion_proof(li(index), ts(size)).unwrap();
            assert_eq!((proof.index, proof.size), (li(index), ts(size)));
            let expected = reference::path(index, &d[..size]);
            assert_eq!(proof.path, expected, "path of leaf {index} in {size}");
            assert!(
                proof.path.len() <= 7,
                "{} hashes for 65 leaves",
                proof.path.len()
            );
            assert_eq!(
                verify_inclusion(&leaves[index], &proof, &root),
                Ok(()),
                "({index}, {size})"
            );
            assert!(reference::inclusion_holds(
                index,
                size,
                leaves[index],
                &proof.path,
                root
            ));
        }
        // No leaf at or past the size.
        assert_eq!(
            tree.inclusion_proof(li(size), ts(size)),
            Err(TreeError::IndexOutOfRange)
        );
    }
    // The tree of no leaf holds no leaf.
    assert_eq!(
        tree.inclusion_proof(li(0), ts(0)),
        Err(TreeError::IndexOutOfRange)
    );
    assert_eq!(
        tree.inclusion_proof(li(0), ts(66)),
        Err(TreeError::SizeOutOfRange)
    );
}

#[test]
fn m5_inclusion_verification_is_tight() {
    let d = datas(33);
    let leaves = leaf_hashes(&d);
    let tree = tree_of(&d);
    let root = tree.root().unwrap();
    for index in 0..33usize {
        let leaf = leaves[index];
        let proof = tree.inclusion_proof(li(index), ts(33)).unwrap();
        assert_eq!(
            verify_inclusion(&leaf, &proof, &root),
            Ok(()),
            "index {index}"
        );
        // One flipped bit anywhere in the leaf hash, the root or any hash of the path: refused.
        for bit in 0..256 {
            assert_eq!(
                verify_inclusion(&flip(&leaf, bit), &proof, &root),
                Err(ProofError::RootMismatch),
                "leaf {index} bit {bit}"
            );
            assert_eq!(
                verify_inclusion(&leaf, &proof, &flip(&root, bit)),
                Err(ProofError::RootMismatch),
                "root, leaf {index} bit {bit}"
            );
            for at in 0..proof.path.len() {
                let mut damaged = proof.clone();
                damaged.path[at] = flip(&damaged.path[at], bit);
                assert_eq!(
                    verify_inclusion(&leaf, &damaged, &root),
                    Err(ProofError::RootMismatch),
                    "leaf {index} path[{at}] bit {bit}"
                );
            }
        }
        // The index moved by one, either way: refused (two positions never share a shape).
        let moves = [index.checked_sub(1), Some(index + 1)];
        for other in moves.into_iter().flatten() {
            let moved = InclusionProof {
                index: li(other),
                ..proof.clone()
            };
            assert!(
                verify_inclusion(&leaf, &moved, &root).is_err(),
                "index {index} read as {other}"
            );
        }
        // The size moved by one. A smaller tree is always refused. A larger one is refused unless the
        // position has the very same sibling sides there (sizes 33 and 34 for every position below 32): the
        // root does not commit to the size, so the same hashes lead to the same root; the model says which.
        for size in [32usize, 34] {
            let moved = InclusionProof {
                size: ts(size),
                ..proof.clone()
            };
            let verdict = verify_inclusion(&leaf, &moved, &root);
            let same_shape =
                index < size && reference::shape(index, size) == reference::shape(index, 33);
            if same_shape {
                assert_eq!(
                    verdict,
                    Ok(()),
                    "index {index} read at size {size}: same shape"
                );
            } else {
                assert!(verdict.is_err(), "index {index} read at size {size}");
            }
            if size == 32 {
                assert!(
                    verdict.is_err(),
                    "a smaller tree: index {index} read at size {size}"
                );
            }
        }
    }
}

#[test]
fn m6_consistency_proofs_verify_for_every_old_new_pair_up_to_65() {
    let d = datas(65);
    let tree = tree_of(&d);
    let roots: Vec<Hash> = (0..=65).map(|m| reference::mth(&d[..m])).collect();
    for new in 0..=65usize {
        for old in 0..=new {
            let proof = tree.consistency_proof(ts(old), ts(new)).unwrap();
            assert_eq!((proof.old, proof.new), (ts(old), ts(new)));
            assert_eq!(
                proof.path,
                reference::proof(old, &d[..new]),
                "proof({old}, {new})"
            );
            if old == 0 || old == new {
                assert!(proof.path.is_empty(), "({old}, {new}) has the empty proof");
            }
            assert_eq!(
                verify_consistency(&proof, &roots[old], &roots[new]),
                Ok(()),
                "({old}, {new})"
            );
            assert!(reference::consistency_holds(
                old,
                new,
                &proof.path,
                roots[old],
                roots[new]
            ));
        }
    }
    // The two edges, defined: the empty old tree is a prefix of every tree (its root must be the empty
    // root; the new root is any root) and an equal pair has the empty proof and equal roots.
    let empty = ConsistencyProof {
        old: ts(0),
        new: ts(9),
        path: vec![],
    };
    assert_eq!(verify_consistency(&empty, &empty_root(), &roots[9]), Ok(()));
    assert_eq!(
        verify_consistency(&empty, &roots[1], &roots[9]),
        Err(ProofError::RootMismatch)
    );
    let both_empty = ConsistencyProof {
        old: ts(0),
        new: ts(0),
        path: vec![],
    };
    assert_eq!(
        verify_consistency(&both_empty, &empty_root(), &empty_root()),
        Ok(())
    );
    assert_eq!(
        verify_consistency(&both_empty, &empty_root(), &roots[1]),
        Err(ProofError::RootMismatch)
    );
    let nonempty = ConsistencyProof {
        old: ts(0),
        new: ts(9),
        path: vec![roots[1]],
    };
    assert_eq!(
        verify_consistency(&nonempty, &empty_root(), &roots[9]),
        Err(ProofError::PathLength)
    );
    let equal = ConsistencyProof {
        old: ts(9),
        new: ts(9),
        path: vec![],
    };
    assert_eq!(verify_consistency(&equal, &roots[9], &roots[9]), Ok(()));
    assert_eq!(
        verify_consistency(&equal, &roots[9], &roots[8]),
        Err(ProofError::RootMismatch)
    );
    let padded = ConsistencyProof {
        old: ts(9),
        new: ts(9),
        path: vec![roots[1]],
    };
    assert_eq!(
        verify_consistency(&padded, &roots[9], &roots[9]),
        Err(ProofError::PathLength)
    );
    // The tree refuses what has no proof.
    assert_eq!(
        tree.consistency_proof(ts(5), ts(4)),
        Err(TreeError::OldAfterNew)
    );
    assert_eq!(
        tree.consistency_proof(ts(5), ts(66)),
        Err(TreeError::SizeOutOfRange)
    );
}

#[test]
fn m7_consistency_verification_is_tight() {
    let d = datas(24);
    let tree = tree_of(&d);
    let roots: Vec<Hash> = (0..=24).map(|m| tree.root_at(ts(m)).unwrap()).collect();
    let bits = [0usize, 1, 6, 7, 8, 63, 64, 127, 128, 200, 254, 255];
    for new in 2..=24usize {
        for old in 1..new {
            let proof = tree.consistency_proof(ts(old), ts(new)).unwrap();
            let (old_root, new_root) = (&roots[old], &roots[new]);
            assert_eq!(
                verify_consistency(&proof, old_root, new_root),
                Ok(()),
                "({old}, {new})"
            );
            // A flipped bit in any hash of the path, or in either root: refused.
            for bit in bits {
                for at in 0..proof.path.len() {
                    let mut damaged = proof.clone();
                    damaged.path[at] = flip(&damaged.path[at], bit);
                    assert_eq!(
                        verify_consistency(&damaged, old_root, new_root),
                        Err(ProofError::RootMismatch),
                        "({old}, {new}) path[{at}] bit {bit}"
                    );
                }
                assert!(verify_consistency(&proof, &flip(old_root, bit), new_root).is_err());
                assert!(verify_consistency(&proof, old_root, &flip(new_root, bit)).is_err());
            }
            // Swapped roots.
            assert_eq!(
                verify_consistency(&proof, new_root, old_root),
                Err(ProofError::RootMismatch)
            );
            // The tree before the tree after: old > new.
            let reversed = ConsistencyProof {
                old: ts(new),
                new: ts(old),
                path: proof.path.clone(),
            };
            assert_eq!(
                verify_consistency(&reversed, new_root, old_root),
                Err(ProofError::OldAfterNew)
            );
            // A path one hash too long or one too short.
            let mut longer = proof.clone();
            longer.path.push(roots[1]);
            assert_eq!(
                verify_consistency(&longer, old_root, new_root),
                Err(ProofError::PathLength)
            );
            let mut shorter = proof.clone();
            shorter.path.pop();
            assert_eq!(
                verify_consistency(&shorter, old_root, new_root),
                Err(ProofError::PathLength)
            );
            // The proof of one pair never verifies the roots of another pair.
            for other_new in 0..=24usize {
                for other_old in 0..=other_new {
                    if (other_old, other_new) != (old, new) {
                        let verdict =
                            verify_consistency(&proof, &roots[other_old], &roots[other_new]);
                        assert!(
                            verdict.is_err(),
                            "proof ({old}, {new}) read as ({other_old}, {other_new})"
                        );
                    }
                }
            }
        }
    }
    // A proof of the wrong sizes for its hashes: the same list read at another pair of sizes.
    let proof = tree.consistency_proof(ts(5), ts(13)).unwrap();
    for (old, new) in [(5, 14), (5, 12), (4, 13), (6, 13), (3, 13)] {
        let moved = ConsistencyProof {
            old: ts(old),
            new: ts(new),
            path: proof.path.clone(),
        };
        assert!(
            verify_consistency(&moved, &roots[old], &roots[new]).is_err(),
            "({old}, {new})"
        );
    }
}

/// Damages a proof's hash list as a hostile prover might: one flipped bit, a hash dropped, invented,
/// doubled or two swapped (or nothing).
fn damage_path(rng: &mut Rng, path: &mut Vec<Hash>) {
    match rng.below(5) {
        0 if !path.is_empty() => {
            let at = rng.pick(path.len());
            path[at] = flip(&path[at], rng.pick(256));
        }
        1 if !path.is_empty() => {
            path.remove(rng.pick(path.len()));
        }
        2 => {
            let at = rng.pick(path.len() + 1);
            path.insert(at, rng.hash());
        }
        3 if !path.is_empty() => {
            let at = rng.pick(path.len());
            path.insert(at, path[at]);
        }
        4 if path.len() > 1 => {
            let (a, b) = (rng.pick(path.len()), rng.pick(path.len()));
            path.swap(a, b);
        }
        _ => {}
    }
}

/// `value` moved by -2..=+2, never below 0.
fn nudge(rng: &mut Rng, value: u64) -> u64 {
    match rng.below(5) {
        0 => value.saturating_sub(2),
        1 => value.saturating_sub(1),
        2 => value,
        3 => value + 1,
        _ => value + 2,
    }
}

#[test]
fn m5_the_inclusion_verifier_accepts_exactly_what_the_recursive_model_accepts() {
    let d = datas(40);
    let leaves = leaf_hashes(&d);
    let tree = tree_of(&d);
    let roots: Vec<Hash> = (0..=40).map(|m| tree.root_at(ts(m)).unwrap()).collect();
    let mut rng = Rng::new(0x1001);
    let (mut accepted, mut refused) = (0u32, 0u32);
    for case in 0..20_000u32 {
        let size = 1 + rng.pick(40);
        let index = rng.pick(size);
        let mut proof = tree.inclusion_proof(li(index), ts(size)).unwrap();
        let (mut leaf, mut root) = (leaves[index], roots[size]);
        for _ in 0..rng.below(3) {
            match rng.below(5) {
                0 => damage_path(&mut rng, &mut proof.path),
                1 => proof.index = LeafIndex(nudge(&mut rng, proof.index.0)),
                2 => proof.size = TreeSize(nudge(&mut rng, proof.size.0)),
                3 => root = roots[rng.pick(41)],
                _ => leaf = leaves[rng.pick(40)],
            }
        }
        let verdict = verify_inclusion(&leaf, &proof, &root);
        let model = reference::inclusion_holds(
            proof.index.0 as usize,
            proof.size.0 as usize,
            leaf,
            &proof.path,
            root,
        );
        assert_eq!(
            verdict.is_ok(),
            model,
            "case {case}: index {} size {} path of {}: {verdict:?}",
            proof.index.0,
            proof.size.0,
            proof.path.len()
        );
        if model {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        accepted > 2_000 && refused > 2_000,
        "accepted {accepted} refused {refused}"
    );
}

#[test]
fn m7_the_consistency_verifier_accepts_exactly_what_the_recursive_model_accepts() {
    let d = datas(40);
    let tree = tree_of(&d);
    let roots: Vec<Hash> = (0..=40).map(|m| tree.root_at(ts(m)).unwrap()).collect();
    let mut rng = Rng::new(0x1002);
    let (mut accepted, mut refused) = (0u32, 0u32);
    for case in 0..20_000u32 {
        let new = rng.pick(41);
        let old = rng.pick(new + 1);
        let mut proof = tree.consistency_proof(ts(old), ts(new)).unwrap();
        let (mut old_root, mut new_root) = (roots[old], roots[new]);
        for _ in 0..rng.below(3) {
            match rng.below(6) {
                0 => damage_path(&mut rng, &mut proof.path),
                1 => proof.old = TreeSize(nudge(&mut rng, proof.old.0)),
                2 => proof.new = TreeSize(nudge(&mut rng, proof.new.0)),
                3 => old_root = roots[rng.pick(41)],
                4 => new_root = roots[rng.pick(41)],
                _ => std::mem::swap(&mut old_root, &mut new_root),
            }
        }
        let verdict = verify_consistency(&proof, &old_root, &new_root);
        let model = reference::consistency_holds(
            proof.old.0 as usize,
            proof.new.0 as usize,
            &proof.path,
            old_root,
            new_root,
        );
        assert_eq!(
            verdict.is_ok(),
            model,
            "case {case}: old {} new {} path of {}: {verdict:?}",
            proof.old.0,
            proof.new.0,
            proof.path.len()
        );
        if model {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        accepted > 2_000 && refused > 2_000,
        "accepted {accepted} refused {refused}"
    );
}
