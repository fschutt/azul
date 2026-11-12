//! The three-way merge: every row of its table, the conflict copies' names, the JSON merge.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::sync::{
    local::{BaseEntry, Content, LocalFile, LocalView},
    merge::{conflict_name, copy_name, deleted_elsewhere, merge_json, plan, Action, PlanInput},
    names,
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
        ..LocalFile::default()
    }
}

fn remote_file(c: char) -> RemoteFile {
    RemoteFile {
        hash: h(c),
        size: 1,
        mtime: 1,
        mtime_ns: 0,
        gen: 1,
        device: String::from("dev-b"),
        exec: false,
        link: None,
    }
}

fn base(c: char) -> BaseEntry {
    BaseEntry {
        hash: h(c),
        size: 1,
        mtime_ns: 1,
        ..BaseEntry::default()
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
        local: &LocalView::of_files(&locals),
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
fn a_copy_of_a_file_deleted_elsewhere_before_this_side_knew_it_is_kept_and_not_sent_again() {
    // D5: a re-login or a lost base pairs afresh; a file here whose bytes match a tombstone
    // is a file the drive deleted meanwhile. It is never deleted here (no login path removes a
    // file) and never sent up again: it is listed until the user chooses.
    let mut remote = RemoteIndex::empty();
    remote.deleted.insert(
        String::from("old.md"),
        Tombstone {
            hash: h('1'),
            gen: 4,
            at: 0,
            device: String::from("dev-b"),
            confirmed: false,
        },
    );
    let locals: BTreeMap<String, LocalFile> = [
        (String::from("old.md"), local('1')),
        (String::from("new.md"), local('2')),
    ]
    .into();
    let never = |_: &str| false;
    let input = PlanInput {
        base: &BTreeMap::new(),
        local: &LocalView::of_files(&locals),
        remote: &remote,
        excluded: &never,
        device: "d",
        date: "x",
    };
    assert_eq!(
        plan(&input),
        vec![Action::Upload {
            key: String::from("new.md")
        }]
    );
    assert_eq!(deleted_elsewhere(&input), vec![String::from("old.md")]);
}

#[test]
fn the_executable_bit_is_part_of_a_files_content() {
    // A file made executable here, the bytes unchanged, goes up as a change of the file.
    let mut remote = RemoteIndex::empty();
    remote
        .files
        .insert(String::from("run.sh"), remote_file('1'));
    let bases: BTreeMap<String, BaseEntry> = [(String::from("run.sh"), base('1'))].into();
    let mut file = local('1');
    file.exec = true;
    let locals: BTreeMap<String, LocalFile> = [(String::from("run.sh"), file)].into();
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &bases,
        local: &LocalView::of_files(&locals),
        remote: &remote,
        excluded: &never,
        device: "d",
        date: "x",
    });
    let expected = if crate::sync::merge::TRACKS_EXEC {
        Action::Upload {
            key: String::from("run.sh"),
        }
    } else {
        Action::Agree {
            key: String::from("run.sh"),
        }
    };
    assert_eq!(actions, vec![expected]);
}

#[test]
fn an_excluded_key_is_never_downloaded_nor_deleted_on_either_side() {
    let mut remote = RemoteIndex::empty();
    remote.files.insert(String::from("x.log"), remote_file('1'));
    let bases: BTreeMap<String, BaseEntry> = [(String::from("x.log"), base('1'))].into();
    let logs = |k: &str| k.ends_with(".log");
    let actions = plan(&PlanInput {
        base: &bases,
        local: &LocalView::empty(),
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
        local: &LocalView::of_files(&locals),
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
        local: &LocalView::of_files(&locals),
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

/// A pass cut after its commit put this side's conflict copy on the drive, before it renamed
/// the file here: the next plan finds that copy (this device's name for `a.md`, this side's
/// content, new to this side) and finishes the same conflict - one copy, never a second, and
/// the copy is not downloaded beside the file it is.
#[test]
fn a_conflict_whose_copy_the_drive_already_holds_from_this_side_is_finished_not_copied_again() {
    let copy = "a (conflict laptop 2026-10-07).md";
    let mut bases = BTreeMap::new();
    bases.insert(String::from("a.md"), base('0'));
    let mut locals = BTreeMap::new();
    locals.insert(String::from("a.md"), local('h'));
    let mut remote = RemoteIndex::empty();
    remote.files.insert(String::from("a.md"), remote_file('t'));
    let mut mine = remote_file('h');
    mine.device = String::from("laptop");
    remote.files.insert(String::from(copy), mine);
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &bases,
        local: &LocalView::of_files(&locals),
        remote: &remote,
        excluded: &never,
        device: "laptop",
        date: "2026-10-08",
    });
    assert_eq!(
        actions,
        vec![Action::Conflict {
            key: String::from("a.md"),
            copy: String::from(copy),
        }]
    );

    // Another device's copy, or a copy with other content, is a file of its own.
    let mut other = remote.clone();
    other.files.get_mut(copy).unwrap().hash = h('x');
    let actions = plan(&PlanInput {
        base: &bases,
        local: &LocalView::of_files(&locals),
        remote: &other,
        excluded: &never,
        device: "laptop",
        date: "2026-10-08",
    });
    assert!(actions.contains(&Action::Download { key: String::from(copy) }), "{actions:?}");
    assert!(
        actions.contains(&Action::Conflict {
            key: String::from("a.md"),
            copy: String::from("a (conflict laptop 2026-10-08).md"),
        }),
        "{actions:?}"
    );
}

/// RF46: what the scan could not tell is neither present nor absent. A name under a folder that
/// could not be read, a link or another disk is told with it; a folder that took a file's name
/// makes the file absent (known); a view not listed to its end knows no name absent.
#[test]
fn rf46_a_view_tells_present_absent_and_unknown_apart() {
    use crate::sync::local::{Entry, Local, Why};
    let files: BTreeMap<String, LocalFile> = [(String::from("notes/a.md"), local('1'))].into();
    let view = LocalView::of_files(&files)
        .with("locked", Entry::Unknown(Why::Locked))
        .with("secret", Entry::Unknown(Why::Unreadable))
        .with("ln", Entry::Link(String::from("notes")))
        .with("disk", Entry::Mount)
        .with("folder", Entry::Dir);
    assert!(matches!(view.get("notes/a.md"), Local::Present(f) if f.hash == h('1')));
    assert_eq!(
        view.get("notes/none.md"),
        Local::Absent,
        "listed completely: absent"
    );
    assert_eq!(view.get("locked"), Local::Unknown(Why::Locked));
    assert_eq!(
        view.get("secret/deep/er.md"),
        Local::Unknown(Why::Unreadable),
        "below it too"
    );
    assert_eq!(view.get("ln"), Local::Unknown(Why::Link));
    assert_eq!(
        view.get("ln/x.md"),
        Local::Unknown(Why::Link),
        "behind a link: unknown"
    );
    assert_eq!(view.get("disk/x.md"), Local::Unknown(Why::Mount));
    assert_eq!(
        view.get("folder"),
        Local::Absent,
        "a folder took the name: the file is not there"
    );
    assert_eq!(
        view.get("lockedx"),
        Local::Absent,
        "only a whole name hides what is below it"
    );
    let names: Vec<&str> = view.files().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["notes/a.md"]);
    // A view that was not listed to its end knows no name absent.
    let partial = LocalView::empty().incomplete();
    assert_eq!(partial.get("anything.md"), Local::Unknown(Why::Unreadable));
    // A cloud-only entry is here, unchanged; one the scan could not tell stays unknown.
    let mut bases = BTreeMap::new();
    let mut cloud = base('9');
    cloud.cloud_only = true;
    bases.insert(String::from("away.bin"), cloud.clone());
    bases.insert(String::from("locked"), cloud);
    let with = view.with_cloud_only(&bases);
    assert!(matches!(with.get("away.bin"), Local::Present(f) if f.hash == h('9')));
    assert_eq!(with.get("locked"), Local::Unknown(Why::Locked));
}

/// RF46 acceptance gate: the three-way table over base {none, h0, h1} x local {absent, unknown,
/// h0, h1, h2} x remote {none, tombstone, h0, h1, h2} - 75 cases - with the invariants every row
/// must keep, checked on a two-sided model:
///  - nothing is planned for a name that is unknown here;
///  - a download comes only over a file that is what the base says (or over nothing);
///  - a file here is deleted only when it is what the base says (it goes to the trash);
///  - a delete on the drive comes only for a file known to be gone here, that the drive still
///    holds as the base says;
///  - conservation: every content that existed on either side and is not the base's (so it is a
///    change since the last sync) exists after the action on a side, as a conflict copy or in the
///    trash.
#[test]
fn rf46_the_three_way_table_keeps_its_invariants_over_75_cases() {
    use crate::sync::local::{Entry, Why};

    #[derive(Clone, Copy, Debug)]
    enum Here {
        Absent,
        Unknown,
        Is(char),
    }
    #[derive(Clone, Copy, Debug)]
    enum There {
        None,
        Tombstone,
        Is(char),
    }

    let mut cases = 0;
    for b in [None, Some('0'), Some('1')] {
        for l in [
            Here::Absent,
            Here::Unknown,
            Here::Is('0'),
            Here::Is('1'),
            Here::Is('2'),
        ] {
            for r in [
                There::None,
                There::Tombstone,
                There::Is('0'),
                There::Is('1'),
                There::Is('2'),
            ] {
                cases += 1;
                let mut bases = BTreeMap::new();
                if let Some(c) = b {
                    bases.insert(String::from("a.md"), base(c));
                }
                let mut view = LocalView::empty();
                match l {
                    Here::Absent => {}
                    Here::Unknown => view = view.with("a.md", Entry::Unknown(Why::Unreadable)),
                    Here::Is(c) => view = view.with("a.md", Entry::File(local(c))),
                }
                let mut remote = RemoteIndex::empty();
                match r {
                    There::None => {}
                    There::Tombstone => {
                        remote.deleted.insert(
                            String::from("a.md"),
                            Tombstone {
                                hash: h('0'),
                                gen: 1,
                                at: 0,
                                device: String::from("dev-b"),
                                confirmed: false,
                            },
                        );
                    }
                    There::Is(c) => {
                        remote.files.insert(String::from("a.md"), remote_file(c));
                    }
                }
                let never = |_: &str| false;
                let actions = plan(&PlanInput {
                    base: &bases,
                    local: &view,
                    remote: &remote,
                    excluded: &never,
                    device: "laptop",
                    date: "2026-10-08",
                });
                let case = format!("base {b:?}, here {l:?}, there {r:?}: {actions:?}");
                assert!(actions.len() <= 1, "{case}");
                let action = actions.first();
                let here = match l {
                    Here::Is(c) => Some(c),
                    _ => None,
                };
                let there = match r {
                    There::Is(c) => Some(c),
                    _ => None,
                };
                // Nothing is planned for a name that is unknown here.
                if matches!(l, Here::Unknown) {
                    assert!(action.is_none(), "{case}");
                }
                // A download only over a file that is what the base says, or over nothing.
                if matches!(action, Some(Action::Download { .. })) {
                    assert!(here.is_none() || here == b, "{case}");
                }
                // A file here is deleted only when it is what the base says.
                if matches!(action, Some(Action::DeleteLocal { .. })) {
                    assert!(here.is_some() && here == b, "{case}");
                }
                // A delete on the drive: gone here for sure, and the drive holds what the base says.
                if matches!(action, Some(Action::DeleteRemote { .. })) {
                    assert!(matches!(l, Here::Absent), "{case}");
                    assert!(there.is_some() && there == b, "{case}");
                }
                // Conservation on a two-sided model.
                let (mut after_here, mut after_there) = (here, there);
                let (mut copy, mut trash) = (None, None);
                match action {
                    Some(Action::Upload { .. }) => after_there = here,
                    Some(Action::Download { .. }) => after_here = there,
                    Some(Action::DeleteLocal { .. }) => {
                        trash = here;
                        after_here = None;
                    }
                    Some(Action::DeleteRemote { .. }) => after_there = None,
                    Some(Action::Conflict { .. }) => {
                        copy = here;
                        after_here = there;
                    }
                    _ => {}
                }
                for content in [here, there].into_iter().flatten() {
                    if Some(content) == b {
                        continue;
                    }
                    assert!(
                        [after_here, after_there, copy, trash].contains(&Some(content)),
                        "content {content} of a change since the last sync is lost: {case}"
                    );
                }
            }
        }
    }
    assert_eq!(cases, 75);
}

/// X-DELTA-EDGE-4 (E2b, T4, the name). A copy's name never ended: `copy_name` added its label to the
/// stem as it was, 33 to 60 bytes a level, and a copy of a copy added a second label to the first
/// one's. A name past 255 bytes is no name a folder holds. A stem of 120 accented letters and a
/// device name of 60 bytes, and the result fed to `conflict_name` again six times: every name is a
/// key the name table accepts, one name is 255 bytes at most, the extension and the date are kept,
/// the label of the copy it was made from is folded into the new one (one `(conflict` in a name),
/// and a name that is taken - the key itself and the copies made so far - is not given twice. RED on
/// the old code: the second name is 326 bytes.
#[test]
fn a_conflict_copy_of_a_conflict_copy_gets_a_valid_name_and_never_a_second_label() {
    let device = "d".repeat(60);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut key = format!("p/{}.md", "\u{e9}".repeat(120));
    seen.insert(key.clone());
    for level in 0..6 {
        let copy = conflict_name(&key, &device, "2026-11-11", &|name| seen.contains(name));
        assert_eq!(names::key_problem(&copy), None, "level {level}: {copy}");
        let name = copy.rsplit('/').next().unwrap();
        assert!(name.len() <= 255, "level {level}: {} bytes", name.len());
        assert!(
            copy.ends_with(").md") && copy.contains(" 2026-11-11"),
            "{copy}"
        );
        assert!(
            copy.starts_with("p/\u{e9}"),
            "the stem is cut, not dropped: {copy}"
        );
        assert_eq!(copy.matches("(conflict ").count(), 1, "one label: {copy}");
        assert_ne!(copy, key, "level {level}: the copy is another name");
        assert!(
            seen.insert(copy.clone()),
            "level {level}: {copy} was given before"
        );
        key = copy;
    }
    // The cut is at a character: a stem of four-byte letters has a budget that falls inside one.
    let emoji = conflict_name(
        &format!("{}.md", "\u{1f600}".repeat(120)),
        &device,
        "2026-11-11",
        &|_| false,
    );
    assert!(
        emoji.len() <= 255 && names::key_problem(&emoji).is_none(),
        "{emoji}"
    );
    assert!(emoji.starts_with('\u{1f600}'));
    // A copy for a device whose name is long: most of the stem goes, and the result is still a name.
    let wide = "w".repeat(200);
    let copy = conflict_name("a.md", &wide, "2026-11-11", &|_| false);
    assert!(copy.len() <= 255, "{} bytes", copy.len());
    // The same cut holds for the second copy shape the sync makes, a trashed version put back.
    let put_back = copy_name(
        &format!("{}.txt", "s".repeat(300)),
        "restored 2026-11-11",
        &|_| false,
    );
    assert!(put_back.len() <= 255, "{} bytes", put_back.len());
    assert!(
        put_back.ends_with("(restored 2026-11-11).txt"),
        "{put_back}"
    );
}

/// X-DELTA-EDGE-4: the copy a pass cut short committed is found again by the same cut. A conflict
/// copy of a long-named file is on the drive with this side's bytes under the name `conflict_name`
/// gave it on another day; the next pass finishes that conflict (key -> that copy) and makes no
/// second copy - which it did not when the search prefix was cut differently from the name.
#[test]
fn a_cut_conflict_of_a_long_name_is_finished_and_not_copied_a_second_time() {
    let key = format!("big/{}.md", "s".repeat(250));
    let copy = conflict_name(&key, "laptop", "2026-11-11", &|_| false);
    assert!(copy.rsplit('/').next().unwrap().len() <= 255, "{copy}");
    let base: BTreeMap<String, BaseEntry> = [(
        key.clone(),
        BaseEntry {
            hash: h('1'),
            size: 1,
            ..BaseEntry::default()
        },
    )]
    .into();
    // Here: edited ('2'). On the drive: the other device's edit ('3') under the name, and this
    // device's own version ('2') under the copy's name, committed by the pass that was cut.
    let locals: BTreeMap<String, LocalFile> = [(key.clone(), local('2'))].into();
    let mut remote = RemoteIndex::empty();
    remote.files.insert(key.clone(), remote_file('3'));
    remote.files.insert(copy.clone(), remote_file('2'));
    let never = |_: &str| false;
    let actions = plan(&PlanInput {
        base: &base,
        local: &LocalView::of_files(&locals),
        remote: &remote,
        excluded: &never,
        device: "laptop",
        // Another day: the copy is found by its device, whatever date it carries.
        date: "2026-11-12",
    });
    assert_eq!(
        actions,
        vec![Action::Conflict { key, copy }],
        "the cut conflict is finished under the copy's own name"
    );
}

/// X-DELTA-EDGE-4: a conflict whose copy would have a name no key can be - the folder's path is 4,080
/// bytes and leaves no room - is not committed: the action is taken out of the plan, the key changes
/// on neither side, and the pass lists it as a bad name. The pass's other actions stay.
#[test]
fn a_conflict_whose_copy_no_key_can_be_is_held_and_listed_and_the_rest_goes_on() {
    use crate::sync::{hold_unnameable_copies, problems::Code, SyncReport};
    let key = format!("{}a.md", format!("{}/", "d".repeat(101)).repeat(40));
    assert_eq!(names::key_problem(&key), None, "the file itself is a key");
    let copy = conflict_name(&key, "B", "2026-11-12", &|_| false);
    assert!(
        names::key_problem(&copy).is_some(),
        "its copy cannot be one: {} bytes",
        copy.len()
    );
    let mut actions = vec![
        Action::Upload {
            key: String::from("x.md"),
        },
        Action::Conflict {
            key: key.clone(),
            copy,
        },
        Action::Conflict {
            key: String::from("fine.md"),
            copy: String::from("fine (conflict B 2026-11-12).md"),
        },
    ];
    let mut report = SyncReport::default();
    hold_unnameable_copies(&mut actions, &mut report);
    assert_eq!(actions.len(), 2, "{actions:?}");
    assert!(actions.iter().all(|a| a.key() != key));
    let told: Vec<_> = report.problems.iter().collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(
        (told[0].code, told[0].key.as_str()),
        (Code::BadName, key.as_str())
    );
}
