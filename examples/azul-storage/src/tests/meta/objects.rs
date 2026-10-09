//! The ids below come from git 2.39 itself, in a repository made with
//! `git init --object-format=sha256` (`git hash-object`, `git mktree`,
//! `git commit-tree` with fixed author, committer and dates).

use crate::meta::{
    objects::git_order, Commit, Kind, MetaError, Mode, ObjectId, Objects, Signature, Tree,
    TreeEntry,
};

fn id(hex: &str) -> ObjectId {
    ObjectId::from_hex(hex).unwrap()
}

const HELLO: &str = "2cf8d83d9ee29543b34a87727421fdecb7e3f3a183d337639025de576db9ebb4";
const X: &str = "4b6cea43da6e13c24f191bcb97b51a58781d1ccdd8281d96291a2582f5177b78";
const EMPTY_BLOB: &str = "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813";
const EMPTY_TREE: &str = "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321";
const SUB: &str = "087e103d499f24fea761c61e1f1b97db789d31714a7bb8f14be2279a2a4b1310";
const ROOT: &str = "442983fdfc7ec4ea2b5530daa53e70c1f97cc3d456bdda7cbbf48a99b5a66aca";
const COMMIT: &str = "55fc76839941e6dd18596905c8680092ca36735e29017f31bbeae1959a17b064";
const COMMIT2: &str = "b2a74cf37ba689bdc2256c58e07245c349da2392833db79cf5506a6559fd2fd6";

fn file(name: &str, id_hex: &str) -> TreeEntry {
    TreeEntry {
        name: name.to_string(),
        mode: Mode::File,
        id: id(id_hex),
    }
}

fn folder(name: &str, id_hex: &str) -> TreeEntry {
    TreeEntry {
        name: name.to_string(),
        mode: Mode::Tree,
        id: id(id_hex),
    }
}

fn root_tree() -> Tree {
    let mut tree = Tree::new();
    // In another order than git's on purpose.
    tree.insert(file("ab", HELLO)).unwrap();
    tree.insert(folder("a", SUB)).unwrap();
    tree.insert(file("a.b", EMPTY_BLOB)).unwrap();
    tree.insert(file("a-c", HELLO)).unwrap();
    tree
}

fn laptop() -> Signature {
    Signature {
        name: "Laptop".to_string(),
        email: "device-1".to_string(),
        time: 1_760_000_000,
        offset_minutes: 120,
    }
}

#[test]
fn the_empty_blob_and_the_empty_tree_have_the_ids_git_gives_them() {
    assert_eq!(ObjectId::of(Kind::Blob, b"").to_hex(), EMPTY_BLOB);
    assert_eq!(ObjectId::of(Kind::Tree, &Tree::new().encode()).to_hex(), EMPTY_TREE);
}

#[test]
fn a_blob_has_the_id_git_gives_it() {
    assert_eq!(ObjectId::of(Kind::Blob, b"hello\n").to_hex(), HELLO);
    assert_eq!(ObjectId::of(Kind::Blob, b"x").to_hex(), X);
}

#[test]
fn a_tree_orders_its_entries_like_git_and_has_the_id_git_gives_it() {
    let mut sub = Tree::new();
    sub.insert(file("x", X)).unwrap();
    assert_eq!(ObjectId::of(Kind::Tree, &sub.encode()).to_hex(), SUB);

    let tree = root_tree();
    let names: Vec<&str> = tree.entries().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a-c", "a.b", "a", "ab"]);
    assert_eq!(ObjectId::of(Kind::Tree, &tree.encode()).to_hex(), ROOT);
}

#[test]
fn a_folder_sorts_as_if_its_name_ended_in_a_slash() {
    let a_file = file("a", HELLO);
    let a_dot = file("a.b", HELLO);
    let a_folder = folder("a", SUB);
    assert!(git_order(&a_file, &a_dot).is_lt());
    assert!(git_order(&a_dot, &a_folder).is_lt());
}

#[test]
fn a_tree_decodes_to_the_tree_that_was_encoded() {
    let tree = root_tree();
    assert_eq!(Tree::decode(&tree.encode()).unwrap(), tree);
    assert_eq!(Tree::decode(b"").unwrap(), Tree::new());
}

#[test]
fn a_tree_refuses_a_name_that_is_not_one_path_segment() {
    let mut tree = Tree::new();
    for name in ["", ".", "..", "a/b", "a\0b"] {
        assert!(tree.insert(file(name, HELLO)).is_err(), "{name:?}");
    }
    assert!(tree.is_empty());
}

#[test]
fn a_tree_body_with_entries_out_of_order_is_refused() {
    let mut body = file_entry_bytes("b");
    body.extend(file_entry_bytes("a"));
    assert!(Tree::decode(&body).is_err());
    let mut twice = file_entry_bytes("a");
    twice.extend(file_entry_bytes("a"));
    assert!(Tree::decode(&twice).is_err());
    assert!(Tree::decode(&file_entry_bytes("a")[..20]).is_err());
}

fn file_entry_bytes(name: &str) -> Vec<u8> {
    let mut tree = Tree::new();
    tree.insert(file(name, HELLO)).unwrap();
    tree.encode()
}

#[test]
fn replacing_and_removing_tree_entries_keeps_one_per_name() {
    let mut tree = root_tree();
    tree.insert(file("ab", X)).unwrap();
    assert_eq!(tree.entries().len(), 4);
    assert_eq!(tree.get("ab").unwrap().id, id(X));
    assert!(tree.remove("a"));
    assert!(!tree.remove("a"));
    assert_eq!(tree.entries().len(), 3);
}

#[test]
fn a_commit_has_the_id_git_gives_it() {
    let commit = Commit {
        tree: id(ROOT),
        parents: Vec::new(),
        author: laptop(),
        committer: laptop(),
        message: "sync\n".to_string(),
    };
    assert_eq!(ObjectId::of(Kind::Commit, &commit.encode()).to_hex(), COMMIT);

    let phone = Signature {
        name: "Phone".to_string(),
        email: "device-2".to_string(),
        time: 1_760_000_060,
        offset_minutes: -90,
    };
    let merge = Commit {
        tree: id(SUB),
        parents: vec![id(COMMIT)],
        author: phone.clone(),
        committer: phone,
        message: "merge\n".to_string(),
    };
    assert_eq!(ObjectId::of(Kind::Commit, &merge.encode()).to_hex(), COMMIT2);
    assert_eq!(Commit::decode(&merge.encode()).unwrap(), merge);
}

#[test]
fn a_signature_drops_angle_brackets_and_line_breaks() {
    let commit = Commit {
        tree: id(ROOT),
        parents: Vec::new(),
        author: Signature {
            name: "Evil <x>\nparent 00".to_string(),
            email: "a>b".to_string(),
            time: 0,
            offset_minutes: 0,
        },
        committer: laptop(),
        message: String::new(),
    };
    let text = String::from_utf8(commit.encode()).unwrap();
    assert!(text.contains("author Evil xparent 00 <ab> 0 +0000\n"), "{text}");
    assert_eq!(Commit::decode(text.as_bytes()).unwrap().parents, Vec::new());
}

#[test]
fn objects_refuse_bytes_that_do_not_have_the_id_they_came_with() {
    let mut objects = Objects::new();
    assert!(matches!(
        objects.insert_checked(id(HELLO), Kind::Blob, b"goodbye\n".to_vec()),
        Err(MetaError::Corrupt { .. })
    ));
    objects
        .insert_checked(id(HELLO), Kind::Blob, b"hello\n".to_vec())
        .unwrap();
    assert_eq!(objects.blob(&id(HELLO)).unwrap(), b"hello\n");
}

#[test]
fn objects_read_a_tree_and_a_commit_back_and_refuse_the_wrong_kind() {
    let mut objects = Objects::new();
    let tree_id = objects.write_tree(&root_tree());
    assert_eq!(tree_id, id(ROOT));
    assert_eq!(objects.tree(&tree_id).unwrap(), root_tree());
    assert!(matches!(objects.blob(&tree_id), Err(MetaError::Corrupt { .. })));
    assert!(matches!(
        objects.tree(&id(SUB)),
        Err(MetaError::MissingObject { .. })
    ));
    let commit = Commit {
        tree: tree_id,
        parents: Vec::new(),
        author: laptop(),
        committer: laptop(),
        message: "sync\n".to_string(),
    };
    let commit_id = objects.write_commit(&commit);
    assert_eq!(commit_id, id(COMMIT));
    assert_eq!(objects.commit(&commit_id).unwrap(), commit);
    // Writing it again keeps one.
    objects.write_commit(&commit);
    assert_eq!(objects.len(), 2);
}
