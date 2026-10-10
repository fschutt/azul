use std::collections::BTreeMap;

use crate::meta::{
    merge::{conflict_name, keep_both, merge_base, merge_trees},
    tree::{apply, walk},
    Change, Commit, ConflictKind, Mode, ObjectId, Objects, Resolution, Signature,
};

/// A drive of `files` (path, content): its root tree.
fn drive(objects: &mut Objects, files: &[(&str, &str)]) -> ObjectId {
    let changes: Vec<Change> = files
        .iter()
        .map(|(path, content)| Change::Put {
            path: (*path).to_string(),
            id: objects.write_blob(content.as_bytes()),
        })
        .collect();
    apply(objects, None, &changes).unwrap()
}

/// Every file of the tree with its content.
fn files(objects: &Objects, root: &ObjectId) -> BTreeMap<String, String> {
    walk(objects, root)
        .unwrap()
        .into_iter()
        .filter(|(_, (mode, _))| *mode == Mode::File)
        .map(|(path, (_, id))| {
            let text = String::from_utf8(objects.blob(&id).unwrap().to_vec()).unwrap();
            (path, text)
        })
        .collect()
}

fn expect(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
        .collect()
}

#[test]
fn what_changed_on_one_side_only_is_taken_without_a_conflict() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    let mine = drive(
        &mut objects,
        &[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c"), ("new.txt", "mine")],
    );
    let theirs = drive(&mut objects, &[("a.txt", "a"), ("b.txt", "b2")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both)
        .unwrap();
    assert!(merged.conflicts.is_empty());
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[("a.txt", "a"), ("b.txt", "b2"), ("new.txt", "mine")])
    );
}

#[test]
fn the_same_change_on_both_sides_is_not_a_conflict() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("a.txt", "a")]);
    let both = drive(&mut objects, &[("a.txt", "a2"), ("x/y.txt", "y")]);
    let merged =
        merge_trees(&mut objects, Some(&base), &both, &both, "Laptop", &mut keep_both).unwrap();
    assert!(merged.conflicts.is_empty());
    assert_eq!(merged.tree, both);
}

#[test]
fn both_changing_a_file_keeps_theirs_at_the_path_and_mine_next_to_it() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("docs/report.docx", "v1")]);
    let mine = drive(&mut objects, &[("docs/report.docx", "mine")]);
    let theirs = drive(&mut objects, &[("docs/report.docx", "theirs")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both)
        .unwrap();
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[
            ("docs/report (conflict, Laptop).docx", "mine"),
            ("docs/report.docx", "theirs"),
        ])
    );
    assert_eq!(merged.conflicts.len(), 1);
    let resolved = &merged.conflicts[0];
    assert_eq!(resolved.conflict.path, "docs/report.docx");
    assert_eq!(resolved.conflict.kind, ConflictKind::BothChanged);
    assert_eq!(resolved.resolution, Resolution::KeepBoth);
    assert_eq!(
        resolved.copy.as_deref(),
        Some("docs/report (conflict, Laptop).docx")
    );
}

#[test]
fn keep_mine_and_take_theirs_keep_one_version() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("a.txt", "v1"), ("b.txt", "v1")]);
    let mine = drive(&mut objects, &[("a.txt", "mine"), ("b.txt", "mine")]);
    let theirs = drive(&mut objects, &[("a.txt", "theirs"), ("b.txt", "theirs")]);
    let mut asked = Vec::new();
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut |c| {
        asked.push(c.path.clone());
        if c.path == "a.txt" {
            Resolution::KeepMine
        } else {
            Resolution::TakeTheirs
        }
    })
    .unwrap();
    assert_eq!(asked, ["a.txt", "b.txt"]);
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[("a.txt", "mine"), ("b.txt", "theirs")])
    );
    assert!(merged.conflicts.iter().all(|r| r.copy.is_none()));
}

#[test]
fn a_file_changed_on_one_side_and_deleted_on_the_other_is_kept_when_keeping_both() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("a.txt", "v1"), ("b.txt", "v1"), ("keep", "k")]);
    let mine = drive(&mut objects, &[("a.txt", "mine"), ("keep", "k")]);
    let theirs = drive(&mut objects, &[("b.txt", "theirs"), ("keep", "k")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both)
        .unwrap();
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[("a.txt", "mine"), ("b.txt", "theirs"), ("keep", "k")])
    );
    assert_eq!(merged.conflicts.len(), 2);
    assert!(merged
        .conflicts
        .iter()
        .all(|r| r.conflict.kind == ConflictKind::ChangedAndDeleted && r.copy.is_none()));
}

#[test]
fn a_file_and_a_folder_of_one_name_keep_the_folder_and_rename_the_file() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("other", "o")]);
    let mine = drive(&mut objects, &[("other", "o"), ("notes", "my notes")]);
    let theirs = drive(&mut objects, &[("other", "o"), ("notes/today.txt", "theirs")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Phone", &mut keep_both)
        .unwrap();
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[
            ("notes (conflict, Phone)", "my notes"),
            ("notes/today.txt", "theirs"),
            ("other", "o"),
        ])
    );
    assert_eq!(merged.conflicts[0].conflict.kind, ConflictKind::FileAndFolder);
}

#[test]
fn folders_changed_on_both_sides_merge_entry_by_entry() {
    let mut objects = Objects::new();
    let base = drive(&mut objects, &[("a/x", "x"), ("a/y", "y")]);
    let mine = drive(&mut objects, &[("a/x", "x2"), ("a/y", "y")]);
    let theirs = drive(&mut objects, &[("a/x", "x"), ("a/y", "y"), ("a/z/deep", "z")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both)
        .unwrap();
    assert!(merged.conflicts.is_empty());
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[("a/x", "x2"), ("a/y", "y"), ("a/z/deep", "z")])
    );
}

#[test]
fn without_a_common_state_two_different_files_of_one_path_are_a_conflict() {
    let mut objects = Objects::new();
    let mine = drive(&mut objects, &[("a.txt", "mine"), ("same", "s")]);
    let theirs = drive(&mut objects, &[("a.txt", "theirs"), ("same", "s")]);
    let merged = merge_trees(&mut objects, None, &mine, &theirs, "Laptop", &mut keep_both).unwrap();
    assert_eq!(merged.conflicts.len(), 1);
    assert_eq!(merged.conflicts[0].conflict.base, None);
    assert_eq!(files(&objects, &merged.tree).len(), 3);
}

#[test]
fn a_conflict_copy_never_takes_a_name_that_is_there() {
    let mut objects = Objects::new();
    let taken = "report (conflict, Laptop).docx";
    let base = drive(&mut objects, &[("report.docx", "v1"), (taken, "older copy")]);
    let mine = drive(&mut objects, &[("report.docx", "mine"), (taken, "older copy")]);
    let theirs = drive(&mut objects, &[("report.docx", "theirs"), (taken, "older copy")]);
    let merged = merge_trees(&mut objects, Some(&base), &mine, &theirs, "Laptop", &mut keep_both)
        .unwrap();
    assert_eq!(
        files(&objects, &merged.tree),
        expect(&[
            ("report (conflict, Laptop 2).docx", "mine"),
            (taken, "older copy"),
            ("report.docx", "theirs"),
        ])
    );
}

#[test]
fn a_conflict_name_keeps_the_extension_and_drops_what_a_name_cannot_hold() {
    assert_eq!(conflict_name("report.docx", "Laptop"), "report (conflict, Laptop).docx");
    assert_eq!(conflict_name("archive.tar.gz", "Laptop"), "archive.tar (conflict, Laptop).gz");
    assert_eq!(conflict_name("notes", "Laptop"), "notes (conflict, Laptop)");
    assert_eq!(conflict_name(".profile", "Laptop"), ".profile (conflict, Laptop)");
    assert_eq!(conflict_name("a.txt", "My/Phone"), "a (conflict, MyPhone).txt");
    assert_eq!(conflict_name("a.txt", " "), "a (conflict, another device).txt");
}

fn signature(time: i64) -> Signature {
    Signature {
        name: "Laptop".to_string(),
        email: "device-1".to_string(),
        time,
        offset_minutes: 0,
    }
}

fn commit(objects: &mut Objects, tree: ObjectId, parents: &[ObjectId], time: i64) -> ObjectId {
    objects.write_commit(&Commit {
        tree,
        parents: parents.to_vec(),
        author: signature(time),
        committer: signature(time),
        message: format!("at {time}\n"),
    })
}

#[test]
fn the_merge_base_is_where_two_histories_forked() {
    let mut objects = Objects::new();
    let tree = drive(&mut objects, &[("a", "a")]);
    let root = commit(&mut objects, tree, &[], 1);
    let fork = commit(&mut objects, tree, &[root], 2);
    let mine = commit(&mut objects, tree, &[fork], 3);
    let mine2 = commit(&mut objects, tree, &[mine], 4);
    let theirs = commit(&mut objects, tree, &[fork], 5);
    assert_eq!(merge_base(&objects, &mine2, &theirs).unwrap(), Some(fork));
    assert_eq!(merge_base(&objects, &theirs, &mine2).unwrap(), Some(fork));
    // One is the other's ancestor: that one.
    assert_eq!(merge_base(&objects, &mine2, &fork).unwrap(), Some(fork));
    // Histories that never met.
    let alone = commit(&mut objects, tree, &[], 6);
    assert_eq!(merge_base(&objects, &alone, &mine2).unwrap(), None);
}

#[test]
fn the_merge_base_is_the_newest_common_state_not_an_older_one() {
    let mut objects = Objects::new();
    let tree = drive(&mut objects, &[("a", "a")]);
    let old = commit(&mut objects, tree, &[], 1);
    let newer = commit(&mut objects, tree, &[old], 2);
    let mine = commit(&mut objects, tree, &[newer], 3);
    // A merge on their side whose second parent is the older state: a
    // breadth-first walk from it meets `old` first.
    let side = commit(&mut objects, tree, &[newer], 4);
    let side2 = commit(&mut objects, tree, &[side], 5);
    let theirs = commit(&mut objects, tree, &[side2, old], 6);
    assert_eq!(merge_base(&objects, &mine, &theirs).unwrap(), Some(newer));
}
