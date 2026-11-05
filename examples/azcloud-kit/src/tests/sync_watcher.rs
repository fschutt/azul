//! The watcher is a hint; full passes run at start, after sleep and every 15 minutes.

use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant, SystemTime},
};

use azul_storage::testing::TempDir;

use crate::sync::watcher::{folder_key, merge, RescanClock, Wake, Watcher, FULL_RESCAN};

fn changed(keys: &[&str]) -> Wake {
    Wake::Changed(keys.iter().map(|k| (*k).to_string()).collect::<BTreeSet<_>>())
}

#[test]
fn wakes_add_up_and_a_rescan_wins() {
    assert_eq!(merge(changed(&["a"]), changed(&["b"])), changed(&["a", "b"]));
    assert_eq!(
        merge(changed(&["a"]), Wake::Rescan(String::from("lost"))),
        Wake::Rescan(String::from("lost"))
    );
}

#[test]
fn a_change_names_its_folder_and_the_bookkeeping_wakes_nothing() {
    let root = Path::new("/r");
    assert_eq!(folder_key(root, Path::new("/r/a.txt")).as_deref(), Some(""));
    assert_eq!(folder_key(root, Path::new("/r/docs/sub/a.txt")).as_deref(), Some("docs/sub"));
    assert_eq!(folder_key(root, Path::new("/r/.azlin/sync/root-id")), None);
    assert_eq!(folder_key(root, Path::new("/elsewhere/a.txt")), None);
}

/// X13: the bookkeeping folder is told in every spelling a disk reads as its name, not only the
/// one the engine made (a case-folding disk lists `.AZLIN` for `.azlin`).
#[test]
fn x_ar5a_13_the_bookkeeping_wakes_nothing_in_any_spelling_and_a_folder_of_that_name_deeper_down_does() {
    let root = Path::new("/r");
    for spelling in [".azlin", ".AZLIN", ".Azlin", "AZLIN~1", ".azlin."] {
        assert_eq!(
            folder_key(root, &root.join(spelling).join("sync").join("root-id")),
            None,
            "{spelling}"
        );
    }
    // Only the root's own bookkeeping: `docs/.azlin` is the user's folder.
    assert_eq!(
        folder_key(root, Path::new("/r/docs/.azlin/a.txt")).as_deref(),
        Some("docs/.azlin")
    );
}

#[test]
fn a_full_pass_is_due_at_start_after_sleep_and_every_fifteen_minutes() {
    let wall = SystemTime::now();
    let mono = Instant::now();
    let mut clock = RescanClock::default();
    assert_eq!(clock.due(wall, mono), Some("start"));
    clock.ran(mono);
    assert_eq!(clock.due(wall + Duration::from_secs(5), mono + Duration::from_secs(5)), None);
    // The wall clock ran an hour ahead of the monotonic one: the computer slept.
    assert_eq!(
        clock.due(wall + Duration::from_secs(3605), mono + Duration::from_secs(10)),
        Some("wake")
    );
    clock.ran(mono + Duration::from_secs(10));
    let later = mono + Duration::from_secs(10) + FULL_RESCAN;
    assert_eq!(clock.due(wall + Duration::from_secs(3605) + FULL_RESCAN, later), Some("interval"));
}

#[test]
fn a_file_written_in_the_folder_wakes_the_watcher_within_seconds() {
    let root = TempDir::new("watcher");
    std::fs::create_dir_all(root.path().join("docs")).unwrap();
    let watcher = Watcher::start(root.path()).unwrap();
    // Some backends need a moment before their first event counts.
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(root.path().join("docs/a.txt"), b"a").unwrap();
    let wake = watcher.wait(Duration::from_secs(10), Duration::from_millis(300));
    match wake {
        Some(Wake::Changed(folders)) => assert!(folders.contains("docs"), "{folders:?}"),
        Some(Wake::Rescan(_)) => {}
        None => panic!("the watcher heard nothing"),
    }
}

#[test]
fn a_change_in_the_bookkeeping_folder_alone_wakes_no_pass() {
    let root = TempDir::new("watcher");
    std::fs::create_dir_all(root.path().join(".azlin/sync")).unwrap();
    let watcher = Watcher::start(root.path()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(root.path().join(".azlin/sync/root-id"), b"{}").unwrap();
    let wake = watcher.wait(Duration::from_secs(3), Duration::from_millis(200));
    assert!(!matches!(wake, Some(Wake::Changed(_))), "{wake:?}");
}
