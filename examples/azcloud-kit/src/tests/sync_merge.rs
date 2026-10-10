//! The three-way merge: every row of its table, the conflict copies' names, the JSON merge.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::sync::{
    local::{BaseEntry, Content, LocalFile},
    merge::{conflict_name, merge_json, plan, Action, PlanInput},
    remote::{RemoteFile, RemoteIndex, Tombstone},
};

fn h(c: char) -> String {
    std::iter::repeat(c).take(64).collect()
}

fn local(c: char) -> LocalFile {
    LocalFile {
        hash: h(c),
        size: 1,
        mtime_ns: 1,
        blob_size: 1,
        content: Content::Raw,
    }
}

fn remote_file(c: char) -> RemoteFile {
    RemoteFile {
        hash: h(c),
        size: 1,
        mtime: 1,
        gen: 1,
        device: String::from("dev-b"),
    }
}

fn base(c: char) -> BaseEntry {
    BaseEntry {
        hash: h(c),
        size: 1,
        mtime_ns: 1,
        cloud_only: false,
    }
}

/// One file through the merge: its base, local and remote hash (or none).
fn one(b: Option<char>, l: Option<char>, r: Option<char>) -> Action {
    let mut bases = BTreeMap::new();
    let mut locals = BTreeMap::new();
    let mut remote = RemoteIndex::empty();
    if let Some(c) = b {
        bases.insert(String::from("a.md"), base(c));
    }
    if let Some(c) = l {
        locals.insert(String::from("a.md"), local(c));
    }
    if let Some(c) = r {
        remote.files.insert(String::from("a.md"), remote_file(c));
    }
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &bases,
        local: &locals,
        remote: &remote,
        excluded: &never,
        device: "laptop",
        date: "2026-10-08",
    });
    assert_eq!(actions.len(), 1, "{actions:?}");
    actions.into_iter().next().unwrap()
}

fn up() -> Action {
    Action::Upload {
        key: String::from("a.md"),
    }
}

fn down() -> Action {
    Action::Download {
        key: String::from("a.md"),
    }
}

#[test]
fn every_row_of_the_table_does_what_it_says() {
    let k = || String::from("a.md");
    assert_eq!(
        one(Some('1'), Some('1'), Some('1')),
        Action::Agree { key: k() }
    );
    assert_eq!(one(None, Some('2'), Some('2')), Action::Agree { key: k() });
    assert_eq!(one(Some('1'), Some('2'), Some('1')), up(), "changed here");
    assert_eq!(one(None, Some('2'), None), up(), "new here");
    assert_eq!(
        one(Some('1'), Some('1'), Some('2')),
        down(),
        "changed there"
    );
    assert_eq!(one(None, None, Some('2')), down(), "new there");
    assert_eq!(
        one(Some('1'), Some('1'), None),
        Action::DeleteLocal { key: k() },
        "deleted there"
    );
    assert_eq!(
        one(Some('1'), None, Some('1')),
        Action::DeleteRemote { key: k() },
        "deleted here"
    );
    assert_eq!(
        one(Some('1'), Some('2'), None),
        up(),
        "an edit beats a delete there"
    );
    assert_eq!(
        one(Some('1'), None, Some('2')),
        down(),
        "an edit beats a delete here"
    );
    assert_eq!(one(Some('1'), None, None), Action::Forget { key: k() });
    assert_eq!(
        one(Some('1'), Some('2'), Some('3')),
        Action::Conflict {
            key: k(),
            copy: String::from("a (conflict laptop 2026-10-08).md")
        }
    );
    assert_eq!(
        one(None, Some('2'), Some('3')),
        Action::Conflict {
            key: k(),
            copy: String::from("a (conflict laptop 2026-10-08).md")
        },
        "new on both sides with other contents"
    );
}

#[test]
fn a_stale_copy_of_a_file_deleted_elsewhere_is_deleted_not_brought_back() {
    let mut remote = RemoteIndex::empty();
    remote.deleted.insert(
        String::from("old.md"),
        Tombstone {
            hash: h('1'),
            gen: 4,
            at: 0,
            device: String::from("dev-b"),
        },
    );
    let locals: BTreeMap<String, LocalFile> = [
        (String::from("old.md"), local('1')),
        (String::from("new.md"), local('2')),
    ]
    .into();
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &BTreeMap::new(),
        local: &locals,
        remote: &remote,
        excluded: &never,
        device: "d",
        date: "x",
    });
    assert_eq!(
        actions,
        vec![
            Action::Upload {
                key: String::from("new.md")
            },
            Action::DeleteLocal {
                key: String::from("old.md")
            },
        ]
    );
}

#[test]
fn an_excluded_key_is_never_downloaded_nor_deleted_on_either_side() {
    let mut remote = RemoteIndex::empty();
    remote.files.insert(String::from("x.log"), remote_file('1'));
    let bases: BTreeMap<String, BaseEntry> = [(String::from("x.log"), base('1'))].into();
    let logs = |k: &str| k.ends_with(".log");
    let actions = plan(&PlanInput {
        base: &bases,
        local: &BTreeMap::new(),
        remote: &remote,
        excluded: &logs,
        device: "d",
        date: "x",
    });
    assert!(actions.is_empty(), "{actions:?}");
}

#[test]
fn a_conflict_copy_takes_a_free_name_and_keeps_its_extension() {
    let free = |_: &str| false;
    assert_eq!(
        conflict_name("notes/a.md", "laptop", "2026-10-08", &free),
        "notes/a (conflict laptop 2026-10-08).md"
    );
    assert_eq!(
        conflict_name("Makefile", "l", "d", &free),
        "Makefile (conflict l d)"
    );
    assert_eq!(
        conflict_name("x/.bashrc", "l", "d", &free),
        "x/.bashrc (conflict l d)",
        "a dot file has no extension"
    );
    let first_taken = |n: &str| n == "a (conflict l d).md";
    assert_eq!(
        conflict_name("a.md", "l", "d", &first_taken),
        "a (conflict l d 2).md"
    );
}

#[test]
fn two_conflicts_in_one_plan_never_pick_the_same_copy_name() {
    let locals: BTreeMap<String, LocalFile> = [
        (String::from("a.md"), local('2')),
        (String::from("a (conflict l d).md"), local('5')),
    ]
    .into();
    let mut remote = RemoteIndex::empty();
    remote.files.insert(String::from("a.md"), remote_file('3'));
    remote
        .files
        .insert(String::from("a (conflict l d).md"), remote_file('6'));
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &BTreeMap::new(),
        local: &locals,
        remote: &remote,
        excluded: &never,
        device: "l",
        date: "d",
    });
    let copies: Vec<&str> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Conflict { copy, .. } => Some(copy.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(copies.len(), 2);
    assert_ne!(copies[0], copies[1]);
}

#[test]
fn a_json_merge_takes_each_sides_change_and_lets_the_first_committer_win_a_clash() {
    let obj = |v: Value| v.as_object().unwrap().clone();
    let base = obj(json!({"mode": "light", "currentTheme": "flat", "locale": "de"}));
    let ours = obj(json!({"mode": "dark", "currentTheme": "flat", "locale": "en"}));
    let theirs = obj(json!({"mode": "light", "currentTheme": "flora", "locale": "fr"}));
    let (merged, clashes) = merge_json(&base, &ours, &theirs);
    assert_eq!(merged["mode"], "dark", "changed here only");
    assert_eq!(merged["currentTheme"], "flora", "changed there only");
    assert_eq!(merged["locale"], "fr", "both changed: the drive's");
    assert_eq!(clashes, vec![String::from("locale")]);
    let gone = obj(json!({"mode": "light", "currentTheme": "flat"}));
    let (merged, _) = merge_json(&base, &gone, &base);
    assert!(
        merged.get("locale").is_none(),
        "a key removed here stays removed"
    );
}

#[test]
fn a_json_merge_file_changed_on_both_sides_merges_instead_of_conflicting() {
    let mut locals = BTreeMap::new();
    let mut file = local('2');
    file.content = Content::JsonMerge;
    locals.insert(String::from("config.json"), file);
    let mut remote = RemoteIndex::empty();
    remote
        .files
        .insert(String::from("config.json"), remote_file('3'));
    let bases: BTreeMap<String, BaseEntry> = [(String::from("config.json"), base('1'))].into();
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &bases,
        local: &locals,
        remote: &remote,
        excluded: &never,
        device: "l",
        date: "d",
    });
    assert_eq!(
        actions,
        vec![Action::MergeJson {
            key: String::from("config.json")
        }]
    );
}
