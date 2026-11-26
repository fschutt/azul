//! The file tree as a model: folders listed when they are opened, their rows, the rules of what is shown. No disk, no
//! window: the listings are `DirEntry` lists as the file source gives them.

use super::*;
use crate::symbols_source::{DirEntry, EntryKind};

fn dir(name: &str) -> DirEntry {
    DirEntry { name: name.to_string(), kind: EntryKind::Dir, size: 0, mtime_ms: 0 }
}

fn file(name: &str) -> DirEntry {
    DirEntry { name: name.to_string(), kind: EntryKind::File, size: 100, mtime_ms: 1 }
}

fn link(name: &str) -> DirEntry {
    DirEntry { name: name.to_string(), kind: EntryKind::Other, size: 0, mtime_ms: 0 }
}

fn names(tree: &Tree) -> Vec<String> {
    tree.rows().iter().map(|r| r.name.clone()).collect()
}

/// root { src { deep { b.rs }, a.rs }, c.rs }, everything listed and the folders open.
fn small() -> Tree {
    let mut t = Tree::new();
    t.list(t.root(), &[file("c.rs"), dir("src")], &Rules::default());
    let src = t.find("src").expect("src");
    t.list(src, &[file("a.rs"), dir("deep")], &Rules::default());
    let deep = t.find("src/deep").expect("deep");
    t.list(deep, &[file("b.rs")], &Rules::default());
    t.set_expanded(src, true);
    t.set_expanded(deep, true);
    t
}

#[test]
fn a_new_tree_is_its_root_and_nothing_has_been_listed() {
    let t = Tree::new();
    assert_eq!(t.path(t.root()), "");
    assert_eq!(t.kind(t.root()), Some(Kind::Dir));
    assert!(!t.is_listed(t.root()));
    assert!(t.rows().is_empty());
    assert_eq!((t.file_count(), t.dir_count()), (0, 0));
    assert_eq!(t.first_file(), None);
}

#[test]
fn a_listing_shows_the_folders_first_then_the_files_each_by_name_whatever_the_case() {
    let mut t = Tree::new();
    let listed = t.list(
        t.root(),
        &[file("b.rs"), dir("src"), file("README.md"), dir("Docs"), file("a.rs")],
        &Rules::default(),
    );
    assert_eq!(listed.added, 5);
    assert_eq!(names(&t), vec!["Docs", "src", "a.rs", "b.rs", "README.md"]);
    assert!(t.is_listed(t.root()));
    assert_eq!((t.file_count(), t.dir_count()), (3, 2));
}

#[test]
fn what_a_review_does_not_read_is_not_in_the_tree_and_is_counted() {
    let mut t = Tree::new();
    let listed = t.list(
        t.root(),
        &[
            dir(".git"),
            dir("target"),
            dir("node_modules"),
            dir("src"),
            file(".gitignore"),
            file("image.png"),
            file("lib.rs"),
            file("README.md"),
            link("alias"),
        ],
        &Rules::default(),
    );
    assert_eq!(names(&t), vec!["src", "lib.rs", "README.md"]);
    assert_eq!(listed.added, 3);
    assert_eq!(listed.left_out, 6, "hidden, build output, links and what is no text of code");
}

#[test]
fn a_build_folder_is_output_only_beside_a_manifest() {
    let mut beside = Tree::new();
    beside.list(beside.root(), &[dir("build"), file("Cargo.toml"), dir("src")], &Rules::default());
    assert_eq!(names(&beside), vec!["src", "Cargo.toml"], "output");
    let mut alone = Tree::new();
    alone.list(alone.root(), &[dir("build"), file("main.rs")], &Rules::default());
    assert_eq!(names(&alone), vec!["build", "main.rs"], "a module");
}

#[test]
fn a_name_that_is_no_name_is_left_out_so_no_path_leaves_the_workspace() {
    let mut t = Tree::new();
    let listed = t.list(
        t.root(),
        &[file(""), file(".."), file("."), file("a/b.rs"), file("a\\b.rs"), file("nul\0.rs"), file("fine.rs")],
        &Rules::default(),
    );
    assert_eq!(names(&t), vec!["fine.rs"]);
    assert_eq!(listed.left_out, 6);
}

#[test]
fn opening_a_folder_that_was_not_listed_asks_for_its_listing_and_a_folder_that_was_does_not() {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("src"), file("x.rs")], &Rules::default());
    let src = t.find("src").expect("src");
    assert!(!t.is_expanded(src));
    assert_eq!(t.toggle(src), Toggle::Opened { needs_listing: true });
    assert!(t.is_expanded(src));
    t.list(src, &[file("a.rs")], &Rules::default());
    assert_eq!(names(&t), vec!["src", "a.rs", "x.rs"]);
    assert_eq!(t.toggle(src), Toggle::Closed);
    assert_eq!(names(&t), vec!["src", "x.rs"], "closed: its files are kept, not shown");
    assert_eq!(t.toggle(src), Toggle::Opened { needs_listing: false });
    assert_eq!(names(&t), vec!["src", "a.rs", "x.rs"]);
}

#[test]
fn a_file_is_opened_not_toggled() {
    let mut t = Tree::new();
    t.list(t.root(), &[file("x.rs")], &Rules::default());
    let x = t.find("x.rs").expect("x.rs");
    assert_eq!(t.toggle(x), Toggle::File);
}

#[test]
fn the_rows_are_the_open_folders_flattened_with_their_depth() {
    let mut t = small();
    let rows = t.rows();
    let shown: Vec<(&str, usize, bool)> = rows.iter().map(|r| (r.name.as_str(), r.depth, r.kind == Kind::Dir)).collect();
    assert_eq!(
        shown,
        vec![("src", 0, true), ("deep", 1, true), ("b.rs", 2, false), ("a.rs", 1, false), ("c.rs", 0, false)]
    );
    let deep = t.find("src/deep").expect("deep");
    t.set_expanded(deep, false);
    assert_eq!(names(&t), vec!["src", "deep", "a.rs", "c.rs"]);
    let src = t.find("src").expect("src");
    t.set_expanded(src, false);
    assert_eq!(names(&t), vec!["src", "c.rs"]);
}

#[test]
fn a_row_says_whether_its_folder_is_open_listed_or_being_listed() {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("src"), dir("lib")], &Rules::default());
    let src = t.find("src").expect("src");
    t.toggle(src);
    t.set_listing(src, true);
    let row = t.rows().into_iter().find(|r| r.name == "src").expect("src row");
    assert!(row.expanded && row.loading && !row.listed);
    let lib = t.rows().into_iter().find(|r| r.name == "lib").expect("lib row");
    assert!(!lib.expanded && !lib.loading && !lib.listed);
    t.list(src, &[file("a.rs")], &Rules::default());
    let row = t.rows().into_iter().find(|r| r.name == "src").expect("src row");
    assert!(row.listed && !row.loading, "a listing ends the wait");
}

#[test]
fn a_path_is_found_by_its_names_and_only_where_it_was_listed() {
    let t = small();
    assert_eq!(t.path(t.find("src/deep/b.rs").expect("b.rs")), "src/deep/b.rs");
    assert_eq!(t.find(""), Some(t.root()));
    assert_eq!(t.find("src/nothing.rs"), None);
    assert_eq!(t.find("src/deep/b.rs/x"), None, "a file has no children");
    assert_eq!(t.find("../src"), None);
    assert_eq!(t.find("src//a.rs"), None);
}

#[test]
fn revealing_a_path_opens_its_folders_and_names_the_ones_still_to_list_from_the_root_down() {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("src"), file("x.rs")], &Rules::default());
    assert_eq!(t.reveal("src/deep/b.rs"), vec!["src".to_string()], "src is not listed: deep is not known yet");
    let src = t.find("src").expect("src");
    assert!(t.is_expanded(src));
    t.list(src, &[dir("deep")], &Rules::default());
    assert_eq!(t.reveal("src/deep/b.rs"), vec!["src/deep".to_string()]);
    let deep = t.find("src/deep").expect("deep");
    t.list(deep, &[file("b.rs")], &Rules::default());
    assert!(t.reveal("src/deep/b.rs").is_empty());
    assert_eq!(names(&t), vec!["src", "deep", "b.rs", "x.rs"]);
}

#[test]
fn the_first_file_is_the_first_file_of_the_root() {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("a_dir"), file("z.rs"), file("m.rs")], &Rules::default());
    assert_eq!(t.first_file().map(|f| t.path(f)), Some("m.rs".to_string()));
    let mut only_dirs = Tree::new();
    only_dirs.list(only_dirs.root(), &[dir("src")], &Rules::default());
    assert_eq!(only_dirs.first_file(), None);
}

#[test]
fn with_no_file_in_the_root_the_first_file_is_the_first_of_the_first_listed_folder_that_has_one() {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("a"), dir("b")], &Rules::default());
    assert_eq!(t.first_file(), None, "nothing is listed under them");
    let b = t.find("b").expect("b");
    t.list(b, &[file("x.rs")], &Rules::default());
    assert_eq!(t.first_file().map(|f| t.path(f)), Some("b/x.rs".to_string()));
    let a = t.find("a").expect("a");
    t.list(a, &[file("y.rs")], &Rules::default());
    assert_eq!(t.first_file().map(|f| t.path(f)), Some("a/y.rs".to_string()), "the order they are shown");
}

#[test]
fn the_paths_of_the_files_are_in_the_order_they_are_shown_and_at_most_so_many() {
    let t = Tree::from_paths(&["README.md", "src/lib.rs", "src/deep/b.rs", "z/q.rs"]);
    assert_eq!(t.file_paths(100), vec!["src/deep/b.rs", "src/lib.rs", "z/q.rs", "README.md"]);
    assert_eq!(t.file_paths(2), vec!["src/deep/b.rs", "src/lib.rs"]);
    assert!(t.file_paths(0).is_empty());
}

#[test]
fn a_tree_has_at_most_so_many_nodes_and_says_so() {
    let mut t = Tree::with_cap(10);
    let entries: Vec<DirEntry> = (0..25).map(|i| file(&format!("f{i:02}.rs"))).collect();
    let listed = t.list(t.root(), &entries, &Rules::default());
    assert_eq!(listed.added, 9, "the root is a node too");
    assert!(listed.capped);
    assert_eq!(t.file_count(), 9);
    let again = t.list(t.root(), &entries, &Rules::default());
    assert_eq!(again.added, 0);
    assert!(again.capped);
}

#[test]
fn listing_a_folder_again_replaces_what_it_held() {
    let mut t = Tree::new();
    t.list(t.root(), &[file("a.rs"), file("b.rs")], &Rules::default());
    t.list(t.root(), &[file("b.rs"), file("c.rs")], &Rules::default());
    assert_eq!(names(&t), vec!["b.rs", "c.rs"]);
    assert_eq!(t.file_count(), 2);
}

#[test]
fn a_file_keeps_its_size_and_the_lines_counted_for_it() {
    let mut t = Tree::new();
    t.list(t.root(), &[file("a.rs")], &Rules::default());
    let a = t.find("a.rs").expect("a.rs");
    assert_eq!(t.node(a).map(|n| (n.size, n.lines)), Some((100, None)));
    t.set_lines(a, 42);
    assert_eq!(t.node(a).and_then(|n| n.lines), Some(42));
}

#[test]
fn only_the_rows_that_are_open_are_built_for_a_tree_of_thousands() {
    let mut t = Tree::new();
    let top: Vec<DirEntry> = (0..20).map(|i| dir(&format!("d{i:02}"))).collect();
    t.list(t.root(), &top, &Rules::default());
    for i in 0..20 {
        let d = t.find(&format!("d{i:02}")).expect("dir");
        let inner: Vec<DirEntry> = (0..1_000).map(|j| file(&format!("f{j:04}.rs"))).collect();
        t.list(d, &inner, &Rules::default());
    }
    assert_eq!((t.file_count(), t.dir_count()), (20_000, 20));
    assert_eq!(t.rows().len(), 20, "all closed");
    let d3 = t.find("d03").expect("d03");
    t.set_expanded(d3, true);
    assert_eq!(t.rows().len(), 20 + 1_000);
}

// ---- which rows are on the screen ----

#[test]
fn the_window_of_rows_is_what_the_viewport_shows_and_a_few_rows_over() {
    let w = |scroll: f32| row_window(1_000, scroll, 300.0, 22.0, 3);
    assert_eq!(w(0.0), 0..17, "300 px of 22 px rows is 14, and 3 over");
    assert_eq!(w(220.0), 7..27, "the 10th row is at the top; 3 over before it, 3 after the last");
    let last = row_window(1_000, 1.0e9, 300.0, 22.0, 3);
    assert!(last.end == 1_000 && last.start >= 970, "{last:?}: past the end is the end");
}

#[test]
fn the_window_of_rows_is_never_more_than_there_is_and_never_a_panic() {
    assert_eq!(row_window(0, 0.0, 300.0, 22.0, 3), 0..0);
    assert_eq!(row_window(5, 0.0, 300.0, 22.0, 3), 0..5);
    for (scroll, viewport, row) in [(f32::NAN, 300.0, 22.0), (-50.0, 300.0, 22.0), (10.0, f32::INFINITY, 22.0), (10.0, 300.0, 0.0), (10.0, -1.0, 22.0)] {
        let r = row_window(100, scroll, viewport, row, 3);
        assert!(r.start <= r.end && r.end <= 100, "{scroll} {viewport} {row}: {r:?}");
    }
}

#[test]
fn a_tree_made_of_paths_is_listed_and_open_all_the_way() {
    let t = Tree::from_paths(&["README.md", "src/lib.rs", "src/deep/b.rs"]);
    assert_eq!(names(&t), vec!["src", "deep", "b.rs", "lib.rs", "README.md"]);
    assert_eq!(t.first_file().map(|f| t.path(f)), Some("README.md".to_string()));
    assert!(t.find("src/deep/b.rs").is_some());
}
