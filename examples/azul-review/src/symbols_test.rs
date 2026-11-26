//! The symbol index in memory: files put in and taken out, names found by any spelling, the caps,
//! the manifests' folders and the statistics. No window, no azul.

use super::*;
use crate::symbols_source::{FileClass, Lang, SkipReason};

fn raw(name: &str, kind: Kind, line: u32) -> RawDef {
    RawDef::new(name, kind, line)
}

fn scanned(defs: Vec<RawDef>) -> Scanned {
    Scanned {
        lang: Some(Lang::Rust),
        size: 100,
        lines: 50,
        defs,
        ..Scanned::default()
    }
}

fn names(found: &[DefInfo]) -> Vec<String> {
    let mut n: Vec<String> = found.iter().map(|d| d.name.clone()).collect();
    n.sort();
    n
}

#[test]
fn a_name_is_found_by_the_words_of_any_spelling() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned(
        "src/a.rs",
        scanned(vec![
            raw("CssError", Kind::Struct, 10),
            raw("css_error", Kind::Fn, 30),
            raw("CSS_ERROR", Kind::Const, 40),
            raw("CssErrors", Kind::Struct, 50),
        ]),
    );
    let found = idx.lookup(&["css", "error"]);
    assert_eq!(names(&found), ["CSS_ERROR", "CssError", "css_error"]);
    assert!(idx.lookup(&["css"]).is_empty());
    assert!(idx.lookup(&["nobody", "defines", "this"]).is_empty());
    assert!(idx.lookup::<&str>(&[]).is_empty());
}

#[test]
fn a_def_carries_its_kind_file_line_extent_parent_and_visibility() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned(
        "src/dom/node.rs",
        scanned(vec![RawDef::new("insert_before", Kind::Method, 120).lines(14).parent("NodeData").public()]),
    );
    let found = idx.lookup(&["insert", "before"]);
    assert_eq!(found.len(), 1);
    let d = &found[0];
    assert_eq!(d.name, "insert_before");
    assert_eq!(d.kind, Kind::Method);
    assert_eq!(d.file, "src/dom/node.rs");
    assert_eq!((d.line, d.lines), (120, 14));
    assert_eq!(d.parent.as_deref(), Some("NodeData"));
    assert_eq!(d.vis, Vis::Public);
    assert!(!d.test);
}

#[test]
fn one_name_in_two_files_gives_two_defs_and_putting_a_file_again_replaces_only_its_own() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned("a.rs", scanned(vec![raw("Error", Kind::Enum, 1), raw("only_a", Kind::Fn, 5)]));
    idx.put_scanned("b.rs", scanned(vec![raw("Error", Kind::Struct, 7)]));
    assert_eq!(idx.lookup(&["error"]).len(), 2);
    idx.put_scanned("a.rs", scanned(vec![raw("only_a_now", Kind::Fn, 9)]));
    let errors = idx.lookup(&["error"]);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].file, "b.rs");
    assert!(idx.lookup(&["only", "a"]).is_empty());
    assert_eq!(idx.lookup(&["only", "a", "now"]).len(), 1);
    assert_eq!(idx.stats().defs, 2);
}

#[test]
fn taking_a_file_out_takes_its_defs_out_and_a_file_that_is_not_there_is_no_harm() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned("a.rs", scanned(vec![raw("Alpha", Kind::Struct, 1)]));
    idx.put_scanned("b.rs", scanned(vec![raw("Beta", Kind::Struct, 1)]));
    assert!(idx.remove_file("a.rs"));
    assert!(!idx.remove_file("a.rs"));
    assert!(!idx.remove_file("never.rs"));
    assert!(idx.lookup(&["alpha"]).is_empty());
    assert_eq!(idx.lookup(&["beta"]).len(), 1);
    assert_eq!(idx.stats().defs, 1);
    assert_eq!(idx.stats().files, 1);
    assert!(idx.defs_of_file("a.rs").is_empty());
}

#[test]
fn the_defs_of_a_file_come_in_line_order() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned("a.rs", scanned(vec![raw("third", Kind::Fn, 30), raw("first", Kind::Fn, 3), raw("second", Kind::Fn, 12)]));
    let lines: Vec<u32> = idx.defs_of_file("a.rs").iter().map(|d| d.line).collect();
    assert_eq!(lines, [3, 12, 30]);
}

#[test]
fn the_words_of_every_name_are_known_words() {
    let mut idx = Index::new(Caps::default());
    idx.put_scanned("a.rs", scanned(vec![raw("DataTableCell", Kind::Struct, 1), raw("utf8_len", Kind::Fn, 2)]));
    for word in ["data", "table", "cell", "utf", "8", "len"] {
        assert!(idx.token_count(word) >= 1, "{word}");
    }
    assert_eq!(idx.token_count("css"), 0);
    assert!(idx.has_token("table"));
    assert!(!idx.has_token("tabl"));
}

#[test]
fn a_def_in_a_test_file_or_marked_test_says_so() {
    let mut idx = Index::new(Caps::default());
    let mut s = scanned(vec![raw("a_helper", Kind::Fn, 1)]);
    s.test_file = true;
    idx.put_scanned("src/x_test.rs", s);
    idx.put_scanned("src/y.rs", scanned(vec![RawDef::new("a_unit_test", Kind::Fn, 4).test(), raw("real_code", Kind::Fn, 9)]));
    assert!(idx.lookup(&["a", "helper"])[0].test);
    assert!(idx.lookup(&["a", "unit", "test"])[0].test);
    assert!(!idx.lookup(&["real", "code"])[0].test);
}

#[test]
fn a_name_in_the_cap_is_kept_and_a_longer_one_is_cut_on_a_character_boundary() {
    let mut idx = Index::new(Caps { max_name_bytes: 16, ..Caps::default() });
    idx.put_scanned("a.rs", scanned(vec![raw("ä_very_long_name_indeed_and_more", Kind::Fn, 1)]));
    let d = &idx.defs_of_file("a.rs")[0];
    assert!(d.name.len() <= 16, "{}", d.name);
    assert!(d.name.starts_with("ä_very"));
}

#[test]
fn a_file_with_more_defs_than_the_cap_keeps_the_first_ones_and_says_so() {
    let mut idx = Index::new(Caps { max_defs_per_file: 3, ..Caps::default() });
    let defs: Vec<RawDef> = (0..10).map(|i| raw(&format!("item_{i}"), Kind::Fn, i + 1)).collect();
    idx.put_scanned("big.rs", scanned(defs));
    assert_eq!(idx.defs_of_file("big.rs").len(), 3);
    assert!(idx.notes().iter().any(|n| matches!(n, Note::FileCapped { file, cap: 3 } if file == "big.rs")));
}

#[test]
fn when_the_index_holds_as_many_defs_as_it_may_nothing_more_is_added_and_it_says_so() {
    let mut idx = Index::new(Caps { max_defs: 5, ..Caps::default() });
    idx.put_scanned("a.rs", scanned((0..4).map(|i| raw(&format!("a_{i}"), Kind::Fn, i + 1)).collect()));
    idx.put_scanned("b.rs", scanned((0..4).map(|i| raw(&format!("b_{i}"), Kind::Fn, i + 1)).collect()));
    assert_eq!(idx.stats().defs, 5);
    assert!(idx.stats().full);
    assert!(idx.notes().iter().any(|n| matches!(n, Note::IndexFull { cap: 5 })));
    // A file taken out makes room again.
    idx.remove_file("a.rs");
    idx.put_scanned("c.rs", scanned(vec![raw("c_0", Kind::Fn, 1)]));
    assert_eq!(idx.lookup(&["c", "0"]).len(), 1);
}

#[test]
fn the_memory_cap_stops_the_index_before_it_grows_without_bound() {
    let mut idx = Index::new(Caps { max_heap_bytes: 20_000, ..Caps::default() });
    for f in 0..50 {
        idx.put_scanned(&format!("f{f}.rs"), scanned((0..40).map(|i| raw(&format!("name_{f}_{i}"), Kind::Fn, i + 1)).collect()));
    }
    assert!(idx.stats().full);
    assert!(idx.stats().defs < 50 * 40);
    assert!(idx.stats().est_bytes < 60_000, "{}", idx.stats().est_bytes);
}

#[test]
fn a_workspace_with_more_files_than_the_cap_knows_the_first_ones_and_says_so() {
    let mut idx = Index::new(Caps { max_files: 3, ..Caps::default() });
    for f in 0..6 {
        idx.put_scanned(&format!("f{f}.rs"), scanned(vec![raw(&format!("item_{f}"), Kind::Fn, 1)]));
    }
    assert_eq!(idx.stats().files, 3);
    assert!(idx.stats().full);
    assert!(idx.notes().iter().any(|n| matches!(n, Note::TooManyFiles { cap: 3 })));
    assert_eq!(idx.lookup(&["item", "0"]).len(), 1);
    assert!(idx.lookup(&["item", "5"]).is_empty());
}

#[test]
fn the_names_a_hundred_files_share_are_kept_once() {
    let mut idx = Index::new(Caps::default());
    for f in 0..100 {
        idx.put_scanned(&format!("f{f}.rs"), scanned(vec![raw("new", Kind::Method, 3), raw("len", Kind::Method, 9)]));
    }
    let stats = idx.stats();
    assert_eq!(stats.defs, 200);
    assert_eq!(stats.names, 2);
    assert_eq!(stats.keys, 2);
    assert_eq!(idx.lookup(&["new"]).len(), 100);
}

#[test]
fn the_statistics_count_files_by_state_and_skips_by_reason() {
    let mut idx = Index::new(Caps::default());
    idx.declare_file("a.rs", 10, 1);
    idx.declare_file("b.rs", 10, 1);
    idx.declare_file("c.rs", 10, 1);
    assert_eq!((idx.stats().files, idx.stats().pending, idx.stats().scanned), (3, 3, 0));
    idx.put_scanned("a.rs", scanned(vec![raw("A", Kind::Struct, 1)]));
    idx.put_skipped("b.rs", SkipReason::Binary);
    idx.put_skipped("d.png", SkipReason::UnknownType);
    let s = idx.stats();
    assert_eq!((s.files, s.pending, s.scanned), (4, 1, 1));
    assert_eq!(s.skipped[SkipReason::Binary.index()], 1);
    assert_eq!(s.skipped[SkipReason::UnknownType.index()], 1);
    assert_eq!(s.skipped.iter().sum::<usize>(), 2);
    assert_eq!(s.lines, 50);
}

#[test]
fn a_manifest_names_its_folder_and_a_file_finds_the_nearest_folder_with_one() {
    let mut idx = Index::new(Caps::default());
    idx.note_manifest_dir("crates/core");
    idx.note_manifest_dir("crates/core/macros");
    idx.note_manifest_dir("");
    assert_eq!(idx.crate_dir_of("crates/core/src/a.rs"), Some("crates/core"));
    assert_eq!(idx.crate_dir_of("crates/core/macros/src/m.rs"), Some("crates/core/macros"));
    assert_eq!(idx.crate_dir_of("crates/other/src/a.rs"), Some(""));
    assert_eq!(idx.crate_dir_of("top.rs"), Some(""));
    let mut bare = Index::new(Caps::default());
    assert_eq!(bare.crate_dir_of("a/b.rs"), None);
    bare.note_manifest_dir("a");
    assert_eq!(bare.crate_dir_of("a/b.rs"), Some("a"));
    assert_eq!(bare.crate_dir_of("x/y.rs"), None);
}

#[test]
fn a_package_name_finds_its_folder_with_dashes_and_underscores_alike() {
    let mut idx = Index::new(Caps::default());
    let mut s = scanned(vec![RawDef::new("azul-core", Kind::Package, 2), RawDef::new("serde", Kind::Dependency, 9)]);
    s.lang = None;
    idx.put_scanned("core/Cargo.toml", s);
    assert_eq!(idx.package_dir("azul_core"), Some("core"));
    assert_eq!(idx.package_dir("azul-core"), Some("core"));
    assert_eq!(idx.package_dir("serde"), None);
    assert_eq!(idx.crate_dir_of("core/src/lib.rs"), Some("core"));
    idx.remove_file("core/Cargo.toml");
    assert_eq!(idx.package_dir("azul_core"), None);
}

#[test]
fn what_a_file_imports_is_kept_with_the_file_and_goes_with_it() {
    let mut idx = Index::new(Caps::default());
    let mut s = scanned(vec![raw("a", Kind::Fn, 1)]);
    s.imports = vec!["crate::dom::Node".into(), "azul_core::task::Thread".into()];
    idx.put_scanned("src/a.rs", s);
    assert_eq!(idx.imports_of("src/a.rs"), ["crate::dom::Node", "azul_core::task::Thread"]);
    assert!(idx.imports_of("src/none.rs").is_empty());
    idx.remove_file("src/a.rs");
    assert!(idx.imports_of("src/a.rs").is_empty());
}

#[test]
fn every_kind_has_a_word_and_the_type_like_ones_are_told() {
    for k in Kind::ALL {
        assert!(!k.word().is_empty());
    }
    assert_eq!(Kind::Struct.word(), "struct");
    assert_eq!(Kind::Fn.word(), "fn");
    assert!(Kind::Struct.is_type_like() && Kind::Class.is_type_like() && Kind::Trait.is_type_like());
    assert!(!Kind::Fn.is_type_like() && !Kind::Const.is_type_like() && !Kind::Dependency.is_type_like());
}

#[test]
fn a_note_has_a_localization_key_and_its_arguments() {
    let notes = [
        Note::FileCapped { file: "a.rs".into(), cap: 9 },
        Note::IndexFull { cap: 7 },
        Note::MemoryFull { cap_bytes: 5 * 1024 * 1024 },
        Note::TooManyFiles { cap: 3 },
    ];
    for n in notes {
        assert!(n.l10n_key().starts_with("azreview-symbols-note-"), "{n:?}");
        assert!(!n.l10n_args().is_empty(), "{n:?}");
    }
}

#[test]
fn the_file_class_of_a_path_is_the_one_the_source_module_says() {
    // The index and the scanners agree on what a code file is.
    assert_eq!(crate::symbols_source::classify("a/b.rs"), FileClass::Code(Lang::Rust));
}
