//! Documents with an owner (ARCH28 RF47 / AR5a-4): the drives file is changed by several writers
//! at once - two windows, a window and the command line, a background record of an encryption -
//! and none drops another's change; a file this build does not understand is not turned into an
//! empty one; what a newer build wrote survives an older build's edit.

use std::{
    sync::{Arc, Barrier},
    thread,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::TempDir;
use crate::{
    config::{DriveEntry, DriveLocation, DrivesFile},
    docs::{merge_values, Doc, DocError, DocRead, Document},
};

fn local(id: &str) -> DriveEntry {
    DriveEntry {
        id: id.to_string(),
        name: format!("Drive {id}"),
        location: DriveLocation::Local {
            root: format!("/tmp/{id}"),
        },
    }
}

/// AZ-U21 / X-AR5a-18: five writers loaded the drives file, changed it and saved it back with no
/// lock: two at a time dropped each other's change (a just-bought drive vanished from the list
/// while its session sat in the keyring).
#[test]
fn az_u21_writers_on_many_threads_all_keep_their_change() {
    let dir = TempDir::new("docs-threads");
    let path = dir.path().join("drives.json");
    let writers = 8;
    let barrier = Arc::new(Barrier::new(writers));
    let handles: Vec<_> = (0..writers)
        .map(|n| {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for k in 0..5 {
                    DrivesFile::update(&path, |file| file.add(local(&format!("d{n}-{k}"))))
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let file = DrivesFile::load(&path).unwrap();
    assert_eq!(file.drives.len(), writers * 5, "{:?}", file.drives.len());
    for n in 0..writers {
        for k in 0..5 {
            assert!(
                file.get(&format!("d{n}-{k}")).is_some(),
                "d{n}-{k} was dropped"
            );
        }
    }
}

/// Adding, forgetting and recording an encryption at once: each one's change is in the file.
#[test]
fn x_ar5a_18_add_forget_and_record_interleave_without_losing_a_change() {
    let dir = TempDir::new("docs-mixed");
    let path = dir.path().join("drives.json");
    DrivesFile::update(&path, |file| {
        file.add(local("keep"));
        file.add(local("forget"));
    })
    .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let adder = {
        let (path, barrier) = (path.clone(), Arc::clone(&barrier));
        thread::spawn(move || {
            barrier.wait();
            for k in 0..10 {
                DrivesFile::update(&path, |file| file.add(local(&format!("new{k}")))).unwrap();
            }
        })
    };
    let forgetter = {
        let (path, barrier) = (path.clone(), Arc::clone(&barrier));
        thread::spawn(move || {
            barrier.wait();
            DrivesFile::update(&path, |file| file.remove("forget")).unwrap();
        })
    };
    barrier.wait();
    adder.join().unwrap();
    forgetter.join().unwrap();
    let file = DrivesFile::load(&path).unwrap();
    assert!(
        file.get("forget").is_none(),
        "the forgotten drive came back"
    );
    assert!(file.get("keep").is_some());
    assert_eq!(file.drives.len(), 11, "all ten new ones and the kept one");
}

/// AZ-CN19: a field a newer build added to the file, to a drive of the list or to its location,
/// was dropped by the next save of an older build.
#[test]
fn az_cn19_what_a_newer_build_wrote_survives_an_older_builds_edit() {
    let dir = TempDir::new("docs-newer-fields");
    let path = dir.path().join("drives.json");
    std::fs::write(
        &path,
        r#"{ "format": "azul-storage.drives", "version": 1, "tabs": {"open": 3},
  "drives": [
    { "id": "a", "name": "A", "color": "red",
      "location": { "kind": "local", "root": "/tmp/a", "watch": true } },
    { "id": "b", "name": "B",
      "location": { "kind": "local", "root": "/tmp/b" } } ] }"#,
    )
    .unwrap();
    DrivesFile::update(&path, |file| {
        file.add(local("c"));
        file.remove("b");
    })
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["tabs"]["open"], 3, "a top-level field: {text}");
    let a = json["drives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "a")
        .expect("a");
    assert_eq!(a["color"], "red", "a field of the drive: {text}");
    assert_eq!(
        a["location"]["watch"], true,
        "a field of its location: {text}"
    );
    let ids: Vec<&str> = json["drives"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["a", "c"], "the edit is in the file too");
    // The edit that keeps nothing unknown writes the file in the order it always had.
    let plain = dir.path().join("plain.json");
    DrivesFile::update(&plain, |file| file.add(local("x"))).unwrap();
    let text = std::fs::read_to_string(&plain).unwrap();
    assert!(
        text.find("\"format\"").unwrap() < text.find("\"drives\"").unwrap(),
        "{text}"
    );
}

/// A cut file is not the end of the list: it is renamed aside - its bytes stay - and the update
/// starts a fresh document.
#[test]
fn a_file_that_is_not_json_is_set_aside_and_never_silently_lost() {
    let dir = TempDir::new("docs-damaged");
    let path = dir.path().join("drives.json");
    std::fs::write(&path, b"{ \"format\": \"azul-storage.drives\", \"vers").unwrap();
    let doc: Doc<DrivesFile> = Doc::new(&path);
    assert!(matches!(doc.read(), DocRead::Damaged(_)));
    DrivesFile::update(&path, |file| file.add(local("fresh"))).unwrap();
    assert_eq!(DrivesFile::load(&path).unwrap().drives.len(), 1);
    let aside: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".damaged-"))
        .collect();
    assert_eq!(aside.len(), 1, "the damaged bytes are kept");
    assert_eq!(
        std::fs::read(aside[0].path()).unwrap(),
        b"{ \"format\": \"azul-storage.drives\", \"vers"
    );
}

/// A file of a newer version or another format is another build's: this build does not write it.
#[test]
fn a_newer_or_foreign_file_is_never_written_over() {
    let dir = TempDir::new("docs-foreign");
    for (name, text) in [
        (
            "newer.json",
            r#"{"format":"azul-storage.drives","version":99,"drives":[]}"#,
        ),
        (
            "other.json",
            r#"{"format":"something.else","version":1,"drives":[]}"#,
        ),
        (
            "shape.json",
            r#"{"format":"azul-storage.drives","version":1,"drives":[{"id":"x"}]}"#,
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let doc: Doc<DrivesFile> = Doc::new(&path);
        assert!(matches!(doc.read(), DocRead::Foreign(_)), "{name}");
        let result = doc.update(|file| file.add(local("x")));
        assert!(
            matches!(result, Err(DocError::Foreign(_))),
            "{name}: {result:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            text,
            "{name} is as it was"
        );
        assert!(DrivesFile::update(&path, |file| file.add(local("x"))).is_err());
    }
}

/// A change that leaves the document as it was writes nothing, also where there is no file yet.
#[test]
fn a_change_that_changes_nothing_writes_nothing() {
    let dir = TempDir::new("docs-nothing");
    let path = dir.path().join("sub").join("drives.json");
    assert!(!DrivesFile::record_encryption(&path, "not-there", "00").unwrap());
    assert!(!path.exists(), "no file was made for nothing");
    DrivesFile::update(&path, |file| file.add(local("a"))).unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(!DrivesFile::record_encryption(&path, "not-there", "00").unwrap());
    DrivesFile::update(&path, |file| file.remove("never-was")).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before,
        "the file was not written again"
    );
}

// ---- a whole document kept in memory by a window (view.json)

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct View {
    width: u32,
    sort: String,
    pairings: Vec<Pairing>,
    shares: Vec<Share>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Pairing {
    id: String,
    folder: String,
    paused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Share {
    id: String,
    sealed: String,
}

impl Document for View {}

fn key_of(path: &str, item: &Value) -> Option<String> {
    match path {
        ".pairings" | ".shares" => item["id"].as_str().map(str::to_string),
        _ => None,
    }
}

fn pairing(id: &str) -> Pairing {
    Pairing {
        id: id.to_string(),
        folder: format!("/sync/{id}"),
        paused: false,
    }
}

fn view_in(dir: &TempDir) -> Doc<View> {
    Doc::new(dir.path().join("drive").join("view.json"))
}

/// AZ-U7: "File > Open new window" is a second process with its own copy; any change in B (a
/// column width) rewrote view.json whole from B's start-time snapshot and dropped the sync
/// pairings and the trusted contact's share A had added since.
#[test]
fn az_u7_a_second_windows_save_keeps_what_the_first_added() {
    let dir = TempDir::new("docs-view");
    let doc = view_in(&dir);
    let start = View {
        width: 100,
        sort: String::from("name"),
        ..View::default()
    };
    doc.save_merged(&View::default(), &start, key_of).unwrap();
    // Both windows start from `start`.
    let mut a = start.clone();
    let mut b = start.clone();
    // A pairs a folder and holds a share for a contact.
    a.pairings.push(pairing("d1"));
    a.shares.push(Share {
        id: String::from("k1"),
        sealed: String::from("sealed"),
    });
    let saved_a = doc.save_merged(&start, &a, key_of).unwrap();
    assert_eq!(saved_a, a);
    // B only changes a column width - and saves from its start-time snapshot.
    b.width = 240;
    let saved_b = doc.save_merged(&start, &b, key_of).unwrap();
    assert_eq!(saved_b.width, 240, "B's change is written");
    assert_eq!(saved_b.pairings, vec![pairing("d1")], "A's pairing stays");
    assert_eq!(saved_b.shares.len(), 1, "A's share stays");
    let on_disk = match doc.read() {
        DocRead::Found(view) => view,
        other => panic!("{other:?}"),
    };
    assert_eq!(on_disk, saved_b);
}

#[test]
fn az_u7_both_windows_add_to_a_list_and_both_additions_stay() {
    let dir = TempDir::new("docs-view-both");
    let doc = view_in(&dir);
    let start = View {
        pairings: vec![pairing("d0")],
        ..View::default()
    };
    doc.save_merged(&View::default(), &start, key_of).unwrap();
    let mut a = start.clone();
    a.pairings.push(pairing("d1"));
    let mut b = start.clone();
    b.pairings.push(pairing("d2"));
    doc.save_merged(&start, &a, key_of).unwrap();
    let merged = doc.save_merged(&start, &b, key_of).unwrap();
    let ids: Vec<&str> = merged.pairings.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["d0", "d1", "d2"]);
}

#[test]
fn az_u7_an_entry_one_window_removed_goes_unless_the_other_changed_it() {
    let dir = TempDir::new("docs-view-remove");
    let doc = view_in(&dir);
    let start = View {
        pairings: vec![pairing("d0"), pairing("d1"), pairing("d2")],
        ..View::default()
    };
    doc.save_merged(&View::default(), &start, key_of).unwrap();
    // A pauses d1; B removes d1 and d2 ("Stop syncing" both).
    let mut a = start.clone();
    a.pairings[1].paused = true;
    doc.save_merged(&start, &a, key_of).unwrap();
    let mut b = start.clone();
    b.pairings.truncate(1);
    let merged = doc.save_merged(&start, &b, key_of).unwrap();
    let ids: Vec<&str> = merged.pairings.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["d0", "d1"],
        "d2 went (nobody changed it); d1 stays, A paused it: a change is not lost to a removal"
    );
    assert!(merged.pairings[1].paused);
}

#[test]
fn az_u7_each_field_of_an_entry_is_merged_on_its_own() {
    // B fills a share's sealed text while A (which did not) renames its folder.
    let start = serde_json::json!({"pairings": [{"id":"d1","folder":"/a","paused":false}]});
    let mine = serde_json::json!({"pairings": [{"id":"d1","folder":"/b","paused":false}]});
    let theirs = serde_json::json!({"pairings": [{"id":"d1","folder":"/a","paused":true}]});
    let merged = merge_values(&start, &mine, &theirs, key_of);
    assert_eq!(
        merged,
        serde_json::json!({"pairings": [{"id":"d1","folder":"/b","paused":true}]})
    );
    // A scalar both changed: the one just made wins.
    let start = serde_json::json!({"width": 100});
    assert_eq!(
        merge_values(
            &start,
            &serde_json::json!({"width": 200}),
            &serde_json::json!({"width": 300}),
            key_of
        ),
        serde_json::json!({"width": 200})
    );
    // A list without keys is one value: mine wins when both changed it.
    let start = serde_json::json!({"recent": ["a"]});
    assert_eq!(
        merge_values(
            &start,
            &serde_json::json!({"recent": ["a", "b"]}),
            &serde_json::json!({"recent": ["a", "c"]}),
            key_of
        ),
        serde_json::json!({"recent": ["a", "b"]})
    );
}

/// The first save of a window with no file yet writes the document whole; a save whose merge is
/// what the file holds already writes nothing.
#[test]
fn az_u7_a_first_save_writes_the_document_and_an_unchanged_one_writes_nothing() {
    let dir = TempDir::new("docs-view-first");
    let doc = view_in(&dir);
    let mine = View {
        width: 77,
        ..View::default()
    };
    doc.save_merged(&View::default(), &mine, key_of).unwrap();
    assert!(doc.path().exists());
    let before = std::fs::metadata(doc.path()).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let again = doc.save_merged(&mine, &mine, key_of).unwrap();
    assert_eq!(again, mine);
    assert_eq!(
        std::fs::metadata(doc.path()).unwrap().modified().unwrap(),
        before,
        "nothing changed, nothing written"
    );
}
