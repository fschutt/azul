//! The whiteboard's file reads and writes without a window: what the start finds, what a board's file is read as, what is
//! written and what is refused.

use std::path::PathBuf;

use super::*;
use crate::whiteboard::LIMITS;

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("azreview-wbio-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

#[test]
fn the_start_finds_the_list_of_boards_and_the_one_board_file_when_they_are_there() {
    let dir = scratch("start");
    std::fs::write(dir.join("index.json"), b"{\"a\":1}").unwrap();
    std::fs::write(dir.join("legacy.zip"), b"zip").unwrap();
    let found = look(&dir.join("index.json"), &dir.join("legacy.zip"));
    assert_eq!(found, Started { index: Some(b"{\"a\":1}".to_vec()), legacy_file: true });
    let none = look(&dir.join("nothing.json"), &dir.join("nothing.zip"));
    assert_eq!(none, Started { index: None, legacy_file: false });
    // a folder where a file is expected is no file
    std::fs::create_dir_all(dir.join("folder.zip")).unwrap();
    assert!(!look(&dir.join("index.json"), &dir.join("folder.zip")).legacy_file);
}

#[test]
fn a_list_of_boards_longer_than_any_list_is_read_as_far_as_it_is_seen_to_be_too_long() {
    let dir = scratch("long");
    let long = vec![b'x'; usize::try_from(MAX_INDEX_BYTES).unwrap() + 4_000];
    std::fs::write(dir.join("index.json"), &long).unwrap();
    let found = look(&dir.join("index.json"), &dir.join("none.zip"));
    let bytes = found.index.expect("it was read");
    assert_eq!(bytes.len() as u64, MAX_INDEX_BYTES + 1, "the first bytes and one more: enough to tell, not the whole file");
}

#[test]
fn a_board_file_that_is_not_there_is_none_and_one_that_is_no_board_comes_back_with_its_bytes() {
    let dir = scratch("board");
    let none = read_board(&dir.join("missing.zip"), &LIMITS);
    assert!(none.raw.is_none() && none.result.is_none());
    std::fs::write(dir.join("junk.zip"), b"this is not an archive").unwrap();
    let junk = read_board(&dir.join("junk.zip"), &LIMITS);
    assert_eq!(junk.raw.as_deref(), Some(&b"this is not an archive"[..]), "kept, so that a copy of it can be put aside");
    assert!(matches!(junk.result, Some(Err(StoreError::NotABoard(_)))), "{:?}", junk.result.map(|r| r.map(|_| ())));
}

fn effect(name: &str, bytes: &[u8]) -> Effect {
    Effect { name: name.to_string(), bytes: bytes.to_vec() }
}

#[test]
fn the_effects_are_written_under_their_names_in_folders_that_are_made_for_them() {
    let dir = scratch("write");
    let out = write_effects(&dir, &[effect("boards/a.json", b"one"), effect("b.json", b"two")]);
    assert_eq!(out, Written { failed: Vec::new(), written: 2 });
    assert_eq!(std::fs::read(dir.join("boards/a.json")).unwrap(), b"one");
    assert_eq!(std::fs::read(dir.join("b.json")).unwrap(), b"two");
    // none of the files is left half written beside the file
    let names: Vec<String> = std::fs::read_dir(dir.join("boards")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["a.json".to_string()]);
}

#[test]
fn a_name_that_leaves_the_folder_is_refused_and_the_others_are_written() {
    let dir = scratch("escape");
    let inner = dir.join("inside");
    std::fs::create_dir_all(&inner).unwrap();
    let out = write_effects(
        &inner,
        &[effect("../outside.json", b"x"), effect("/abs.json", b"x"), effect("a\\b.json", b"x"), effect("", b"x"), effect("fine.json", b"ok")],
    );
    assert_eq!(out.written, 1);
    assert_eq!(out.failed.len(), 4, "{:?}", out.failed);
    assert!(!dir.join("outside.json").exists());
    assert!(inner.join("fine.json").exists());
}
