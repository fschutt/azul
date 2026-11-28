use super::*;

fn sample() -> Workspace {
    let mut w = Workspace::new(Root {
        drive_root: PathBuf::from("/data"),
        prefix: "code/sample/".to_string(),
        data_tree: true,
        name: "sample".to_string(),
    });
    w.set_listing(
        "",
        vec!["src".to_string(), "docs".to_string()],
        vec!["README.md".to_string(), "Cargo.toml".to_string()],
    );
    w
}

#[derive(Debug, PartialEq)]
struct Doc(String);

impl TabDoc for Doc {
    fn key(&self) -> &str {
        &self.0
    }
}

#[test]
fn the_explorer_lists_folders_first_and_opens_them_lazily() {
    let mut w = sample();
    let names: Vec<String> = w.rows().iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, vec!["docs", "src", "Cargo.toml", "README.md"]);
    assert!(w.toggle("src/", true), "src must be listed first");
    assert_eq!(w.rows().len(), 4, "nothing under src until it is listed");
    w.set_listing("src/", vec![], vec!["main.rs".to_string(), "lib.rs".to_string()]);
    let rows = w.rows();
    let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, vec!["docs/", "src/", "src/lib.rs", "src/main.rs", "Cargo.toml", "README.md"]);
    assert_eq!(rows[2].depth, 1);
    assert!(rows[1].expanded && rows[1].folder);
    assert!(!w.toggle("src/", false), "closing needs no listing");
    assert_eq!(w.rows().len(), 4);
    assert!(!w.toggle("src/", true), "listed already");
    assert_eq!(w.drive_key("src/main.rs"), "code/sample/src/main.rs");
    assert_eq!(w.relative_key("code/sample/src/main.rs").as_deref(), Some("src/main.rs"));
    assert_eq!(file_name("src/main.rs"), "main.rs");
    assert_eq!(file_name("src/"), "src");
    assert_eq!(tab_label("main.rs", true), "main.rs *");
}

#[test]
fn opening_an_open_file_brings_its_tab_and_closing_picks_a_neighbour() {
    let mut t: Tabs<Doc> = Tabs::default();
    assert_eq!(t.open(Doc("a".into())), 0);
    assert_eq!(t.open(Doc("b".into())), 1);
    assert_eq!(t.open(Doc("c".into())), 2);
    assert_eq!(t.open(Doc("a".into())), 0, "already open");
    assert_eq!(t.docs.len(), 3);
    assert_eq!(t.open(Doc("d".into())), 1, "a new tab goes after the active one");
    assert_eq!(t.docs.iter().map(|d| d.0.as_str()).collect::<Vec<_>>(), vec!["a", "d", "b", "c"]);
    t.active = 3;
    assert_eq!(t.close(3), Some(Doc("c".into())));
    assert_eq!(t.active, 2, "the last tab closed: the one left of it");
    t.active = 0;
    t.close(0);
    assert_eq!(t.active().map(|d| d.0.as_str()), Some("d"), "the one right of it");
    t.close(0);
    t.close(0);
    assert!(t.active().is_none());
    assert!(t.close(0).is_none());
}

#[test]
fn the_rows_are_kept_and_follow_every_listing_toggle_and_collapse() {
    let mut w = sample();
    assert_eq!(w.index_of("README.md"), Some(3));
    assert!(w.toggle("src/", true));
    w.set_listing("src/", vec!["deep".to_string()], vec!["main.rs".to_string()]);
    assert_eq!(w.index_of("src/main.rs"), Some(3), "under src, after src/deep/");
    assert!(w.is_expanded("src/"));
    assert!(w.toggle("src/deep/", true));
    w.set_listing("src/deep/", vec![], vec!["mod.rs".to_string()]);
    assert_eq!(w.rows()[3].key, "src/deep/mod.rs");
    assert_eq!(w.rows()[3].depth, 2);
    w.collapse_all();
    assert_eq!(w.rows().len(), 4, "every folder closed");
    assert!(!w.is_expanded("src/") && w.index_of("src/main.rs").is_none());
    assert!(!w.toggle("src/", true), "listed already: opens at once");
    assert_eq!(w.rows().len(), 6, "src/deep/ (closed) and src/main.rs");
    assert_eq!(parent_folder("src/deep/mod.rs"), Some("src/deep/"));
    assert_eq!(parent_folder("src/deep/"), Some("src/"));
    assert_eq!(parent_folder("Cargo.toml"), None);
    assert_eq!(parent_folder("src/"), None);
}

#[test]
fn the_explorer_hides_what_vscode_hides_and_a_refresh_lists_every_open_folder_again() {
    let mut w = sample();
    w.set_listing(
        "",
        vec![".git".to_string(), "src".to_string()],
        vec![".DS_Store".to_string(), "a.rs".to_string()],
    );
    let names: Vec<String> = w.rows().iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, vec!["src", "a.rs"]);
    assert!(w.toggle("src/", true));
    w.set_listing("src/", vec![], vec!["main.rs".to_string()]);
    assert_eq!(w.rows().len(), 3);
    assert_eq!(w.refresh(), vec![String::new(), "src/".to_string()]);
    assert!(w.rows().is_empty(), "nothing shows until the listings come back");
    assert!(!w.is_listed("src/"));
}

#[test]
fn a_path_inside_the_workspace_is_its_key_and_a_path_outside_is_not() {
    let user = Root {
        drive_root: PathBuf::from("/home/u/project"),
        prefix: String::new(),
        data_tree: false,
        name: "project".to_string(),
    };
    assert_eq!(
        key_of_path(&user, Path::new("/home/u/project/src/main.rs")).as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(key_of_path(&user, Path::new("/home/u/other/main.rs")), None);
    assert_eq!(key_of_path(&user, Path::new("/home/u/project")), None, "the folder is no file");
    let sample = sample().root;
    assert_eq!(
        key_of_path(&sample, Path::new("/data/code/sample/src/lib.rs")).as_deref(),
        Some("src/lib.rs")
    );
    assert_eq!(key_of_path(&sample, Path::new("/data/code/settings.json")), None, "outside the prefix");
    assert_ne!(doc_ident(&user, "main.rs"), doc_ident(&sample, "main.rs"));
    assert_eq!(doc_ident(&user, "src/main.rs"), doc_ident(&user, "src/main.rs"));
}

#[test]
fn the_recent_folders_keep_the_last_ten_newest_first_each_once() {
    let mut recent: Vec<String> = Vec::new();
    for i in 0..12 {
        recent = remember(&recent, &format!("/p/{i}"));
    }
    assert_eq!(recent.len(), RECENT_MAX);
    assert_eq!(recent[0], "/p/11");
    assert_eq!(recent[9], "/p/2");
    let again = remember(&recent, "/p/5");
    assert_eq!(again[0], "/p/5");
    assert_eq!(again.iter().filter(|f| f.as_str() == "/p/5").count(), 1);
    assert_eq!(again.len(), RECENT_MAX);
    assert_eq!(recent_from_json(&recent_to_json(&again)), again);
    assert!(recent_from_json("not json").is_empty());
    assert!(recent_from_json("").is_empty());
}

#[test]
fn quick_open_finds_files_by_the_letters_of_their_path_names_first() {
    let files: Vec<String> = ["src/main.rs", "src/lib.rs", "README.md", "docs/library.md", "Cargo.toml"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let found = |query: &str| -> Vec<String> {
        quick_matches(&files, query, QUICK_MAX)
            .iter()
            .map(|&i| files[i].clone())
            .collect()
    };
    assert_eq!(found("lib"), vec!["src/lib.rs", "docs/library.md"]);
    assert_eq!(found("smr"), vec!["src/main.rs"]);
    assert_eq!(found("LIB.RS"), vec!["src/lib.rs"], "any case");
    assert!(found("zzz").is_empty());
    assert_eq!(quick_matches(&files, "", 3).len(), 3, "an empty query lists the first files");
    assert!(skipped_folder("target") && skipped_folder(".git") && skipped_folder("node_modules"));
    assert!(!skipped_folder("src") && !skipped_folder("docs"));
}
