//! The watcher is a hint; full passes run at start, after sleep and every 15 minutes.

use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant, SystemTime},
};

use azul_storage::testing::TempDir;

use crate::sync::{
    problems::{Code, Severity},
    watcher::{
        coverage_problem, folder_key, merge, poll, poll_wait, unwatchable_problem, Coverage,
        RescanClock, Wake, Watcher, FULL_RESCAN, PARTIAL_RESCAN, POLL_MAX, POLL_MIN,
    },
};

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

/// AZ-W10: inotify has a limit of watches per user; a folder it could not watch woke nothing and
/// nobody was told - only the quarter-hour rescan found its changes. The watcher counts what it
/// missed, and the problem list says so.
#[test]
fn az_w10_a_folder_nothing_watches_is_counted_and_said_and_a_complete_one_says_nothing() {
    let mut coverage = Coverage::default();
    assert!(coverage.is_complete());
    assert_eq!(coverage.sentence(), None);
    assert!(coverage_problem(&coverage).is_none());

    coverage.missed("the user's limit of watches, fs.inotify.max_user_watches, is used up");
    let one = coverage.sentence().expect("a folder is missing");
    assert!(one.starts_with("1 folder is not watched"), "{one}");
    assert!(one.contains("fs.inotify.max_user_watches"), "{one}");
    coverage.missed("a folder cannot be entered");
    let two = coverage.sentence().unwrap();
    assert!(two.starts_with("2 folders are not watched"), "{two}");
    assert!(
        two.contains("fs.inotify.max_user_watches") && !two.contains("cannot be entered"),
        "the first reason is the one said: {two}"
    );
    assert!(two.contains(&format!("within {} seconds", PARTIAL_RESCAN.as_secs())));

    // For the problem list: a warning that holds nothing back and has no file of its own.
    let problem = coverage_problem(&coverage).expect("a partly watched folder is a problem");
    assert_eq!(problem.code, Code::WatchLimited);
    assert_eq!(problem.severity, Severity::Warning);
    assert_eq!(problem.key, "");
    assert_eq!(problem.code.as_str(), "watch-limited");
    assert!(!problem.code.holds_back());
    assert!(problem.code.is_pass_wide());
    assert!(PARTIAL_RESCAN < FULL_RESCAN);
}

/// AZ-W10: what inotify's refusal means, from the errno of `inotify_add_watch`.
#[cfg(unix)]
#[test]
fn az_w10_a_refused_watch_is_a_miss_unless_the_folder_went_away() {
    use crate::sync::watcher::miss_reason;
    assert_eq!(
        miss_reason(libc::ENOENT),
        None,
        "gone meanwhile: nothing to watch"
    );
    let limit = miss_reason(libc::ENOSPC).expect("the limit");
    assert!(limit.contains("fs.inotify.max_user_watches"), "{limit}");
    let other = miss_reason(libc::EACCES).expect("a folder that cannot be entered");
    assert!(other.contains("cannot be entered"), "{other}");
}

#[test]
fn az_w10_a_folder_the_watcher_can_watch_everywhere_is_complete() {
    let root = TempDir::new("watcher-coverage");
    std::fs::create_dir_all(root.path().join("docs/sub")).unwrap();
    std::fs::create_dir_all(root.path().join("music")).unwrap();
    let watcher = Watcher::start(root.path()).unwrap();
    assert!(watcher.coverage().is_complete(), "{:?}", watcher.coverage());
}

/// AZ-W10: the poll (the backend of Windows) looked at the whole tree every two seconds, a core for
/// a folder of 200 000 files. It waits twenty times as long as a look took.
#[test]
fn az_w10_the_polls_wait_grows_with_its_look_and_is_bounded() {
    assert_eq!(poll_wait(Duration::ZERO), POLL_MIN);
    assert_eq!(
        poll_wait(Duration::from_millis(50)),
        POLL_MIN,
        "a small folder: every two seconds"
    );
    assert_eq!(poll_wait(Duration::from_secs(1)), Duration::from_secs(20));
    assert_eq!(poll_wait(Duration::from_secs(4)), Duration::from_secs(80));
    assert_eq!(poll_wait(Duration::from_secs(1000)), POLL_MAX);
    assert_eq!(poll_wait(Duration::MAX), POLL_MAX, "no overflow");
    // At most a twentieth of the time is spent looking, once the wait is not at the floor.
    for millis in [200u64, 1_000, 4_000, 10_000] {
        let look = Duration::from_millis(millis);
        assert!(look * 20 <= poll_wait(look), "{millis}");
    }
}

/// The poll's own logic, built on every host: an entry that is new, gone or different names its
/// folder; the bookkeeping folder is never looked at.
#[test]
fn az_w10_the_poll_names_the_folders_of_new_gone_and_changed_entries() {
    let root = TempDir::new("watcher-poll");
    let at = |key: &str| root.path().join(key);
    std::fs::create_dir_all(at("docs")).unwrap();
    std::fs::create_dir_all(at("sub/deeper")).unwrap();
    std::fs::create_dir_all(at(".azlin/sync")).unwrap();
    std::fs::write(at("docs/a.txt"), b"a").unwrap();
    std::fs::write(at("sub/deeper/b.txt"), b"b").unwrap();
    std::fs::write(at("c.txt"), b"c").unwrap();
    let mut before = poll::Look::new();
    poll::look(root.path(), "", &mut before);
    assert!(before.contains_key("docs/a.txt") && before.contains_key("sub/deeper"));
    assert!(
        !before.keys().any(|key| key.starts_with(".azlin")),
        "{before:?}"
    );
    assert!(poll::changed(&before, &before).is_empty());

    std::fs::write(at("docs/a.txt"), b"a changed").unwrap(); // different: docs
    std::fs::remove_file(at("sub/deeper/b.txt")).unwrap(); // gone: sub/deeper
    std::fs::write(at("new.txt"), b"n").unwrap(); // new: the root
    std::fs::write(at(".azlin/sync/root-id"), b"{}").unwrap(); // bookkeeping: nothing
    let mut now = poll::Look::new();
    poll::look(root.path(), "", &mut now);
    let folders: Vec<String> = poll::changed(&before, &now).into_iter().collect();
    // The poll lists the folders too: `sub/deeper` changed (its file is gone), which names its own
    // folder `sub`, and the file that is gone names `sub/deeper`.
    assert_eq!(folders, ["", "docs", "sub", "sub/deeper"]);
}

/// AZ-W10: no watcher at all (inotify refuses a new instance past the user's limit) was `.ok()`: the
/// engine ran on its quarter-hour clock and said nothing. It says so, and a folder that is gone is
/// the pass's to report.
#[test]
fn az_w10_a_folder_no_watcher_could_be_started_on_is_said_unless_it_is_gone() {
    let refused = std::io::Error::from_raw_os_error(24); // EMFILE on Unix, anything else elsewhere
    let problem = unwatchable_problem(&refused).expect("a refused watcher is a problem");
    assert_eq!(problem.code, Code::WatchLimited);
    assert_eq!(problem.severity, Severity::Warning);
    assert!(
        problem.detail.contains("cannot be watched"),
        "{}",
        problem.detail
    );
    assert!(
        problem.detail.contains(&refused.to_string()),
        "{}",
        problem.detail
    );
    assert!(
        problem
            .detail
            .contains(&format!("within {} seconds", PARTIAL_RESCAN.as_secs())),
        "{}",
        problem.detail
    );
    let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
    assert!(
        unwatchable_problem(&gone).is_none(),
        "RootMissing is the pass's"
    );
    // The start of a watcher on a folder that is not there is that error.
    let nowhere = TempDir::new("watcher-gone");
    let folder = nowhere.path().join("not-there");
    assert!(Watcher::start(&folder).is_err_and(|e| unwatchable_problem(&e).is_none()));
}
