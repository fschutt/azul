//! M1, M2, M12 and M14: the hash rules, the known answers and the printing of a hash.

use sha2::{Digest, Sha256};

use super::{hex, hex_bytes, li, ts, Rng};
use crate::reference;
use crate::{
    empty_root, leaf_hash, node_hash, verify_consistency, verify_inclusion, Hash, LeafIndex,
    MemStore, Tree, TreeSize,
};

fn sha256(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    Hash(hasher.finalize().into())
}

#[test]
fn m1_empty_root_is_sha256_of_nothing() {
    let known = hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(empty_root(), known);
    assert_eq!(empty_root(), sha256(&[]));
    // The tree of no leaf has exactly this root, at the only size it can be asked for.
    let tree = Tree::new(MemStore::new()).unwrap();
    assert_eq!(tree.size(), TreeSize(0));
    assert_eq!(tree.root().unwrap(), known);
    assert_eq!(tree.root_at(TreeSize(0)).unwrap(), known);
    assert_eq!(reference::mth(&[]), known);
}

#[test]
fn m2_a_leaf_is_not_a_node() {
    // Known answer: the hash of the leaf "" is SHA-256 of the single byte 0x00.
    assert_eq!(
        leaf_hash(b""),
        hex("6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d")
    );
    let mut rng = Rng::new(2);
    let mut samples: Vec<Vec<u8>> = vec![vec![], b"a".to_vec(), b"hello world".to_vec()];
    samples.push(vec![0u8; 64]);
    samples.push(vec![0xff; 100]);
    for _ in 0..50 {
        samples.push(rng.hash().0.to_vec());
    }
    for sample in &samples {
        // The 0x00 prefix is there, and plain SHA-256 of the data is not the leaf hash.
        assert_eq!(leaf_hash(sample), sha256(&[&[0x00], sample]));
        assert_ne!(leaf_hash(sample), sha256(&[sample]));
        assert_eq!(leaf_hash(sample), reference::leaf(sample));
    }
    for _ in 0..100 {
        let (left, right) = (rng.hash(), rng.hash());
        let mut joined = left.0.to_vec();
        joined.extend_from_slice(&right.0);
        // A leaf over the 64 bytes of a node's two children is not that node.
        assert_ne!(leaf_hash(&joined), node_hash(&left, &right));
        // The second-preimage attempt: the node's whole 65-byte input, offered as a leaf.
        let mut node_input = vec![0x01];
        node_input.extend_from_slice(&joined);
        assert_ne!(leaf_hash(&node_input), node_hash(&left, &right));
        // And the node is what the RFC says, in the order given.
        assert_eq!(
            node_hash(&left, &right),
            sha256(&[&[0x01], &left.0, &right.0])
        );
        assert_eq!(node_hash(&left, &right), reference::node(&left, &right));
        assert_ne!(node_hash(&left, &right), node_hash(&right, &left));
    }
    // A tree of one leaf has the hash of that leaf as its root.
    let mut tree = Tree::new(MemStore::new()).unwrap();
    assert_eq!(tree.append(b"only").unwrap(), LeafIndex(0));
    assert_eq!(tree.size(), TreeSize(1));
    assert_eq!(tree.root().unwrap(), leaf_hash(b"only"));
    assert_eq!(tree.root_at(TreeSize(1)).unwrap(), leaf_hash(b"only"));
    assert_eq!(tree.root_at(TreeSize(0)).unwrap(), empty_root());
}

/// The eight leaves of the Certificate Transparency reference tests (RFC 6962 trees are RFC 9162 trees).
const LEAVES: [&str; 8] = [
    "",
    "00",
    "10",
    "2021",
    "3031",
    "40414243",
    "5051525354555657",
    "606162636465666768696a6b6c6d6e6f",
];

/// The roots of the first 1..=8 of them, as the lane brief recalls them (from memory: where one differs
/// from the model, the model is the authority and the difference is a drift row).
const ROOTS: [&str; 8] = [
    "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
    "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
    "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
    "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
    "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
    "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
    "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
    "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
];

#[test]
fn m12_known_answers_rfc6962() {
    let data: Vec<Vec<u8>> = LEAVES.iter().map(|leaf| hex_bytes(leaf)).collect();
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for (n, expected) in ROOTS.iter().enumerate() {
        tree.append(&data[n]).unwrap();
        let size = n + 1;
        assert_eq!(
            tree.root().unwrap(),
            hex(expected),
            "root of the first {size} leaves"
        );
        assert_eq!(
            reference::mth(&data[..size]),
            hex(expected),
            "model, first {size} leaves"
        );
    }
    // The finished tree answers for every prefix.
    for (n, expected) in ROOTS.iter().enumerate() {
        assert_eq!(
            tree.root_at(ts(n + 1)).unwrap(),
            hex(expected),
            "root_at({})",
            n + 1
        );
    }
    // A tree rebuilt over the same leaf hashes has the same roots.
    let hashes: Vec<Hash> = data.iter().map(|leaf| leaf_hash(leaf)).collect();
    let rebuilt = Tree::new(MemStore::from_leaves(hashes)).unwrap();
    assert_eq!(rebuilt.root().unwrap(), hex(ROOTS[7]));
}

/// The proofs of the same Certificate Transparency reference tests, over the same eight leaves (recalled
/// like the roots above: a difference from the model is a drift row, the model wins).
#[test]
fn m12_known_proofs_of_the_ct_reference_tests() {
    let data: Vec<Vec<u8>> = LEAVES.iter().map(|leaf| hex_bytes(leaf)).collect();
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for entry in &data {
        tree.append(entry).unwrap();
    }
    let a = "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7";
    let b = "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e";
    let c = "6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4";
    let d = "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b";
    let e = "ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0";
    let f = "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a";
    // (index, size, the siblings from the leaf up)
    let inclusion: [(usize, usize, Vec<&str>); 5] = [
        (0, 1, vec![]),
        (0, 8, vec![a, b, c]),
        (5, 8, vec![d, e, ROOTS[3]]),
        (2, 3, vec![ROOTS[1]]),
        (1, 5, vec![ROOTS[0], b, d]),
    ];
    for (index, size, path) in inclusion {
        let proof = tree.inclusion_proof(li(index), ts(size)).unwrap();
        let expected: Vec<Hash> = path.iter().map(|h| hex(h)).collect();
        assert_eq!(
            proof.path, expected,
            "inclusion proof of leaf {index} in {size}"
        );
        let root = tree.root_at(ts(size)).unwrap();
        assert_eq!(
            verify_inclusion(&leaf_hash(&data[index]), &proof, &root),
            Ok(())
        );
    }
    // (old, new, the proof list)
    let consistency: [(usize, usize, Vec<&str>); 4] = [
        (1, 1, vec![]),
        (1, 8, vec![a, b, c]),
        (6, 8, vec![f, e, ROOTS[3]]),
        (2, 5, vec![b, d]),
    ];
    for (old, new, path) in consistency {
        let proof = tree.consistency_proof(ts(old), ts(new)).unwrap();
        let expected: Vec<Hash> = path.iter().map(|h| hex(h)).collect();
        assert_eq!(
            proof.path, expected,
            "consistency proof from {old} to {new}"
        );
        let (old_root, new_root) = (hex(ROOTS[old - 1]), hex(ROOTS[new - 1]));
        assert_eq!(verify_consistency(&proof, &old_root, &new_root), Ok(()));
    }
}

#[test]
fn m14_hash_debug_and_display_are_hex_and_no_secret_exists() {
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let hash = Hash(bytes);
    let expected = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    assert_eq!(format!("{hash:?}"), expected);
    assert_eq!(format!("{hash}"), expected);
    assert_eq!(format!("{:?}", Hash([0xab; 32])), "ab".repeat(32));
    assert_eq!(format!("{}", Hash([0; 32])), "0".repeat(64));
    let mut rng = Rng::new(14);
    for _ in 0..100 {
        let shown = format!("{:?}", rng.hash());
        assert_eq!(shown.len(), 64);
        assert!(
            shown
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{shown}"
        );
        assert_eq!(hex(&shown), hex(&format!("{}", hex(&shown))));
    }
    // Nothing here is secret: no secret type or zeroizing wrapper exists in the crate's sources.
    let sources = [
        include_str!("../lib.rs"),
        include_str!("../hash.rs"),
        include_str!("../store.rs"),
        include_str!("../tree.rs"),
        include_str!("../proof.rs"),
    ];
    for source in sources {
        for needle in [
            concat!("Sec", "ret"),
            concat!("zero", "ize"),
            concat!("Zero", "ize"),
        ] {
            assert!(!source.contains(needle), "the crate mentions {needle}");
        }
    }
}
