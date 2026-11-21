use crate::meta::{
    merge::{keep_both, merge_trees},
    shard::{is_shard_name, is_sharded, shard_of, SHARD_ABOVE, SHARD_BELOW, SHARD_PREFIX},
    tree::{apply, entry_at, folder_at, walk},
    Change, Mode, ObjectId, Objects, Tree,
};

fn put(objects: &mut Objects, path: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: objects.write_blob(path.as_bytes()),
    }
}

fn delete(path: &str) -> Change {
    Change::Delete {
        path: path.to_string(),
    }
}

/// A drive with the folder `big` of `count` files.
fn big_folder(objects: &mut Objects, count: usize) -> ObjectId {
    let changes: Vec<Change> = (0..count)
        .map(|i| put(objects, &format!("big/file-{i:05}.txt")))
        .collect();
    apply(objects, None, &changes).unwrap()
}

/// The tree `big` is stored as, read raw (shards not gathered).
fn raw_big(objects: &Objects, root: &ObjectId) -> Tree {
    let entry = objects.tree(root).unwrap().get("big").cloned().unwrap();
    objects.tree(&entry.id).unwrap()
}

#[test]
fn a_shard_name_is_the_prefix_and_two_lowercase_hex_digits() {
    assert!(shard_of("report.docx").starts_with(SHARD_PREFIX));
    assert!(is_shard_name(&shard_of("report.docx")));
    assert_eq!(shard_of("a"), shard_of("a"));
    assert!(!is_shard_name(".azlin-shard-AB"));
    assert!(!is_shard_name(".azlin-shard-abc"));
    assert!(!is_shard_name("azlin-shard-ab"));
}

#[test]
fn a_folder_of_more_than_the_limit_is_stored_in_shards_and_read_whole() {
    let mut objects = Objects::new();
    let count = SHARD_ABOVE + 52;
    let root = big_folder(&mut objects, count);
    let raw = raw_big(&objects, &root);
    assert!(is_sharded(&raw));
    assert!(raw.entries().len() <= 256);
    assert!(raw.entries().iter().all(|e| is_shard_name(&e.name)));

    let folder = folder_at(&objects, &root, "big").unwrap().unwrap();
    assert_eq!(folder.entries().len(), count);
    assert!(folder.entries().iter().all(|e| e.mode == Mode::File));
    let found = entry_at(&objects, &root, "big/file-00077.txt").unwrap().unwrap();
    assert_eq!(objects.blob(&found.id).unwrap(), b"big/file-00077.txt");
    assert_eq!(entry_at(&objects, &root, "big/nothing").unwrap(), None);
    assert_eq!(walk(&objects, &root).unwrap().len(), count + 1);
}

#[test]
fn a_folder_at_the_limit_is_one_tree() {
    let mut objects = Objects::new();
    let root = big_folder(&mut objects, SHARD_ABOVE);
    assert!(!is_sharded(&raw_big(&objects, &root)));
}

#[test]
fn a_change_in_a_sharded_folder_rewrites_one_shard() {
    let mut objects = Objects::new();
    let root = big_folder(&mut objects, SHARD_ABOVE + 10);
    let before = objects.len();
    let change = put(&mut objects, "big/new.txt");
    let next = apply(&mut objects, Some(&root), &[change]).unwrap();
    // The blob, one shard, the folder's shard list, the root.
    assert_eq!(objects.len(), before + 4);
    assert!(entry_at(&objects, &next, "big/new.txt").unwrap().is_some());
}

#[test]
fn a_sharded_folder_goes_back_to_one_tree_only_below_the_lower_limit() {
    let mut objects = Objects::new();
    let count = SHARD_ABOVE + 10;
    let root = big_folder(&mut objects, count);
    // Down to just above the lower limit: still in shards.
    let first: Vec<Change> = (0..count - SHARD_BELOW - 1)
        .map(|i| delete(&format!("big/file-{i:05}.txt")))
        .collect();
    let middle = apply(&mut objects, Some(&root), &first).unwrap();
    assert_eq!(folder_at(&objects, &middle, "big").unwrap().unwrap().entries().len(), SHARD_BELOW + 1);
    assert!(is_sharded(&raw_big(&objects, &middle)));
    // One more: one tree.
    let last = delete(&format!("big/file-{:05}.txt", count - SHARD_BELOW - 1));
    let small = apply(&mut objects, Some(&middle), &[last]).unwrap();
    assert!(!is_sharded(&raw_big(&objects, &small)));
    assert_eq!(folder_at(&objects, &small, "big").unwrap().unwrap().entries().len(), SHARD_BELOW);
}

#[test]
fn the_same_files_make_the_same_sharded_tree_in_any_order() {
    let mut a = Objects::new();
    let forward: Vec<Change> = (0..SHARD_ABOVE + 5)
        .map(|i| put(&mut a, &format!("big/{i}")))
        .collect();
    let backward: Vec<Change> = forward.iter().rev().cloned().collect();
    let one = apply(&mut a, None, &forward).unwrap();
    let two = apply(&mut a, None, &backward).unwrap();
    assert_eq!(one, two);
}

#[test]
fn a_path_through_a_shard_name_is_refused() {
    let mut objects = Objects::new();
    let change = put(&mut objects, ".azlin-shard-00");
    assert!(apply(&mut objects, None, &[change]).is_err());
    let change = put(&mut objects, "a/.azlin-shard-7f/b");
    assert!(apply(&mut objects, None, &[change]).is_err());
}

#[test]
fn two_devices_adding_to_one_sharded_folder_merge_without_a_conflict() {
    let mut objects = Objects::new();
    let base = big_folder(&mut objects, SHARD_ABOVE + 1);
    let mine_change = put(&mut objects, "big/mine.txt");
    let mine = apply(&mut objects, Some(&base), &[mine_change]).unwrap();
    let theirs_change = put(&mut objects, "big/theirs.txt");
    let theirs = apply(&mut objects, Some(&base), &[theirs_change]).unwrap();
    let merged =
        merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both).unwrap();
    assert!(merged.conflicts.is_empty());
    let folder = folder_at(&objects, &merged.tree, "big").unwrap().unwrap();
    assert_eq!(folder.entries().len(), SHARD_ABOVE + 3);
    assert!(is_sharded(&raw_big(&objects, &merged.tree)));
    // No conflict path names a shard.
    assert!(folder.get("mine.txt").is_some() && folder.get("theirs.txt").is_some());
}
