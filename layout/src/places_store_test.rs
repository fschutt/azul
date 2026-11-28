//! The file of kept folders: round trips, replacement, the cap, damaged lines, and a failed write that keeps the old list.

use std::{fs, path::PathBuf};

use super::*;
use crate::managers::share_area::new_token;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("azul-places-test-{}", new_token()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn store(&self) -> PlacesStore {
        PlacesStore::new(self.0.join("Library").join("places.tsv"))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn place(name: &str, bookmark: &[u8], ms: u64) -> StoredPlace {
    StoredPlace {
        name: name.to_string(),
        bookmark: bookmark.to_vec(),
        granted_ms: ms,
    }
}

#[test]
fn a_missing_file_is_an_empty_list() {
    let dir = TestDir::new();
    assert!(dir.store().load().is_empty());
}

#[test]
fn a_kept_folder_comes_back_with_its_name_bookmark_and_date() {
    let dir = TestDir::new();
    let store = dir.store();
    store.add(place("Downloads", &[1, 2, 3, 255], 1_700_000_000_000)).unwrap();
    store.add(place("Ordner mit Tab\there und \u{e4}\u{1F600}", &[9; 300], 0)).unwrap();
    let kept = store.load();
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0], place("Downloads", &[1, 2, 3, 255], 1_700_000_000_000));
    assert_eq!(kept[1].name, "Ordner mit Tab\there und \u{e4}\u{1F600}", "a name with a tab and an emoji survives");
    assert_eq!(kept[1].bookmark.len(), 300);
}

#[test]
fn the_same_folder_granted_again_replaces_its_entry_and_moves_to_the_end() {
    let dir = TestDir::new();
    let store = dir.store();
    store.add(place("A", &[1], 1)).unwrap();
    store.add(place("B", &[2], 2)).unwrap();
    store.add(place("A again", &[1], 3)).unwrap();
    let kept = store.load();
    assert_eq!(kept.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["B", "A again"]);
}

#[test]
fn past_the_cap_the_oldest_go_and_are_handed_back() {
    let dir = TestDir::new();
    let store = dir.store();
    for n in 0..MAX_PLACES {
        assert!(store.add(place(&format!("f{n}"), &[u8::try_from(n).unwrap(), 7], 0)).unwrap().is_empty());
    }
    let dropped = store.add(place("one more", &[200, 200], 0)).unwrap();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].name, "f0", "the oldest: the platform releases its grant");
    let kept = store.load();
    assert_eq!(kept.len(), MAX_PLACES);
    assert_eq!(kept.last().unwrap().name, "one more");
}

#[test]
fn a_forgotten_folder_is_gone_and_a_folder_that_was_not_there_says_so() {
    let dir = TestDir::new();
    let store = dir.store();
    store.add(place("A", &[1], 0)).unwrap();
    store.add(place("B", &[2], 0)).unwrap();
    assert!(store.remove(&[1]).unwrap());
    assert!(!store.remove(&[1]).unwrap());
    assert_eq!(store.load().len(), 1);
}

#[test]
fn a_damaged_line_costs_that_line_and_not_the_list() {
    let dir = TestDir::new();
    let store = dir.store();
    store.add(place("Good", &[1, 2], 5)).unwrap();
    let mut text = fs::read_to_string(store.path()).unwrap();
    text.push_str("v1\tzz\t5\t0102\n"); // a name that is not hex
    text.push_str("v2\t00\t5\t0102\n"); // another version
    text.push_str("v1\t41\tnot-a-number\t0102\n");
    text.push_str("v1\t41\t5\t\n"); // no bookmark
    text.push_str("v1\t41\t5\t0102\textra\n"); // a field too many
    text.push_str("garbage\n\n");
    fs::write(store.path(), &text).unwrap();
    store.add(place("After", &[3], 6)).unwrap();
    let kept = store.load();
    assert_eq!(kept.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Good", "After"]);
}

#[test]
fn a_write_that_fails_keeps_the_old_list() {
    let dir = TestDir::new();
    let store = dir.store();
    store.add(place("Keep", &[1], 1)).unwrap();
    // the temporary file cannot be made: a folder is where it would go
    fs::create_dir(store.path().with_extension("tmp")).unwrap();
    assert!(store.add(place("Lost", &[2], 2)).is_err());
    let kept = store.load();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name, "Keep", "the old list is intact");
}
