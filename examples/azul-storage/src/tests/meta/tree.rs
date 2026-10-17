use crate::meta::{
    tree::{apply, entry_at, folder_at, walk},
    Change, Kind, MetaError, Mode, ObjectId, Objects, Tree,
};

fn put(path: &str, id: ObjectId) -> Change {
    Change::Put {
        path: path.to_string(),
        id,
    }
}

fn delete(path: &str) -> Change {
    Change::Delete {
        path: path.to_string(),
    }
}

fn folder(path: &str) -> Change {
    Change::Folder {
        path: path.to_string(),
    }
}

fn paths(objects: &Objects, root: &ObjectId) -> Vec<String> {
    walk(objects, root).unwrap().into_keys().collect()
}

#[test]
fn changes_make_the_same_root_tree_as_git() {
    // The tree of the objects tests, which git 2.39 named 442983fd...
    let mut objects = Objects::new();
    let hello = objects.write_blob(b"hello\n");
    let x = objects.write_blob(b"x");
    let empty = objects.write_blob(b"");
    let root = apply(
        &mut objects,
        None,
        &[put("ab", hello), put("a/x", x), put("a.b", empty), put("a-c", hello)],
    )
    .unwrap();
    assert_eq!(
        root.to_hex(),
        "442983fdfc7ec4ea2b5530daa53e70c1f97cc3d456bdda7cbbf48a99b5a66aca"
    );
}

#[test]
fn a_put_makes_the_folders_on_its_way() {
    let mut objects = Objects::new();
    let blob = objects.write_blob(b"pointer");
    let root = apply(
        &mut objects,
        None,
        &[put("docs/a.txt", blob), put("docs/sub/b.txt", blob), put("c.txt", blob)],
    )
    .unwrap();
    assert_eq!(
        paths(&objects, &root),
        ["c.txt", "docs", "docs/a.txt", "docs/sub", "docs/sub/b.txt"]
    );
    assert_eq!(
        entry_at(&objects, &root, "docs/sub/b.txt").unwrap().unwrap().id,
        blob
    );
    assert_eq!(entry_at(&objects, &root, "docs/nothing").unwrap(), None);
    assert_eq!(entry_at(&objects, &root, "c.txt/below").unwrap(), None);
    let docs = folder_at(&objects, &root, "docs").unwrap().unwrap();
    assert_eq!(docs.entries().len(), 2);
    assert_eq!(folder_at(&objects, &root, "c.txt").unwrap(), None);
    assert_eq!(folder_at(&objects, &root, "").unwrap().unwrap().entries().len(), 2);
}

#[test]
fn a_change_writes_only_the_trees_on_its_path() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let two = objects.write_blob(b"two");
    let root = apply(&mut objects, None, &[put("a/x", one), put("b/y", one)]).unwrap();
    let b_before = entry_at(&objects, &root, "b").unwrap().unwrap().id;
    let count = objects.len();
    let next = apply(&mut objects, Some(&root), &[put("a/z", two)]).unwrap();
    // The new "a" and the new root; "b" is the same tree.
    assert_eq!(objects.len(), count + 2);
    assert_eq!(entry_at(&objects, &next, "b").unwrap().unwrap().id, b_before);
    assert_eq!(paths(&objects, &next), ["a", "a/x", "a/z", "b", "b/y"]);
}

#[test]
fn no_change_and_deleting_what_is_not_there_keep_the_root() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let root = apply(&mut objects, None, &[put("a/x", one)]).unwrap();
    assert_eq!(apply(&mut objects, Some(&root), &[]).unwrap(), root);
    assert_eq!(
        apply(&mut objects, Some(&root), &[delete("a/nothing"), delete("b/c")]).unwrap(),
        root
    );
}

#[test]
fn deleting_the_last_file_of_a_folder_keeps_the_folder() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let root = apply(&mut objects, None, &[put("a/x", one)]).unwrap();
    let next = apply(&mut objects, Some(&root), &[delete("a/x")]).unwrap();
    assert_eq!(paths(&objects, &next), ["a"]);
    let empty_tree = ObjectId::of(Kind::Tree, &Tree::new().encode());
    assert_eq!(entry_at(&objects, &next, "a").unwrap().unwrap().id, empty_tree);
}

#[test]
fn deleting_a_folder_removes_everything_in_it() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let root = apply(
        &mut objects,
        None,
        &[put("a/x", one), put("a/b/c", one), put("keep", one)],
    )
    .unwrap();
    let next = apply(&mut objects, Some(&root), &[delete("a")]).unwrap();
    assert_eq!(paths(&objects, &next), ["keep"]);
}

#[test]
fn an_empty_folder_is_kept_and_naming_an_existing_folder_keeps_its_files() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let root = apply(&mut objects, None, &[folder("empty"), put("full/x", one)]).unwrap();
    assert_eq!(paths(&objects, &root), ["empty", "full", "full/x"]);
    let again = apply(&mut objects, Some(&root), &[folder("full"), folder("empty/sub")]).unwrap();
    assert_eq!(paths(&objects, &again), ["empty", "empty/sub", "full", "full/x"]);
    assert_eq!(
        entry_at(&objects, &again, "full").unwrap(),
        entry_at(&objects, &root, "full").unwrap()
    );
}

#[test]
fn changes_apply_in_their_order() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let two = objects.write_blob(b"two");
    let root = apply(
        &mut objects,
        None,
        &[put("a/x", one), delete("a"), put("a/y", two), put("b", one), delete("b")],
    )
    .unwrap();
    assert_eq!(paths(&objects, &root), ["a", "a/y"]);
}

#[test]
fn a_file_in_the_way_of_a_folder_or_a_folder_in_the_way_of_a_file_is_refused() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    let root = apply(&mut objects, None, &[put("a.txt", one), put("dir/x", one)]).unwrap();
    assert!(matches!(
        apply(&mut objects, Some(&root), &[put("a.txt/b", one)]),
        Err(MetaError::Corrupt { .. })
    ));
    assert!(matches!(
        apply(&mut objects, Some(&root), &[put("dir", one)]),
        Err(MetaError::Corrupt { .. })
    ));
    assert!(apply(&mut objects, Some(&root), &[folder("a.txt")]).is_err());
}

#[test]
fn a_path_that_is_not_a_drive_path_is_refused() {
    let mut objects = Objects::new();
    let one = objects.write_blob(b"one");
    for path in ["", "a//b", "../x", "a/./b", "a/", "/a"] {
        assert!(apply(&mut objects, None, &[put(path, one)]).is_err(), "{path:?}");
    }
    let root = apply(&mut objects, None, &[put("a", one)]).unwrap();
    assert!(entry_at(&objects, &root, "").is_err());
    assert_eq!(walk(&objects, &root).unwrap()["a"], (Mode::File, one));
}
