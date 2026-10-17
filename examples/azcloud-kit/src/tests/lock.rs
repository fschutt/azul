//! The locks every process of this user shares: one holder at a time - threads of one process
//! and other processes (an OS file lock) alike -, an error after the wait instead of a second
//! holder, a file per lock that never leaves its folder.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use azul_storage::testing::TempDir;

use crate::lock::LockDir;

#[test]
fn a_lock_has_one_holder_at_a_time_and_the_next_one_waits_for_it() {
    let dir = TempDir::new("azcloud-lock");
    let log = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..4)
        .map(|i| {
            // A LockDir of its own each, as every process has: the same folder.
            let locks = LockDir::new(dir.path());
            let log = log.clone();
            std::thread::spawn(move || {
                let _held = locks
                    .lock("azul-storage/s3/d_1", Duration::from_secs(10))
                    .unwrap();
                log.lock().unwrap().push(format!("in {i}"));
                std::thread::sleep(Duration::from_millis(30));
                log.lock().unwrap().push(format!("out {i}"));
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 8);
    for pair in log.chunks(2) {
        assert_eq!(pair[0].replace("in", "out"), pair[1], "two holders at once: {log:?}");
    }
}

#[test]
fn a_lock_held_too_long_is_an_error_after_the_wait() {
    let dir = TempDir::new("azcloud-lock");
    let locks = LockDir::new(dir.path());
    let held = locks.lock("azcloud/checkouts", Duration::from_secs(1)).unwrap();
    let started = Instant::now();
    let other = {
        let locks = locks.clone();
        std::thread::spawn(move || {
            locks
                .lock("azcloud/checkouts", Duration::from_millis(200))
                .map(|_| ())
        })
        .join()
        .unwrap()
    };
    assert!(other.is_err(), "a second holder");
    assert!(started.elapsed() >= Duration::from_millis(200));
    // Another name is another lock.
    let _other_name = locks
        .lock("azul-storage/s3/d_2", Duration::from_millis(200))
        .unwrap();
    drop(held);
    let _again = locks
        .lock("azcloud/checkouts", Duration::from_millis(200))
        .unwrap();
}

#[test]
fn a_lock_another_process_holds_is_waited_for_through_its_file() {
    let dir = TempDir::new("azcloud-lock");
    let locks = LockDir::new(dir.path());
    let path = locks.file_of("azul-storage/s3/d_1");
    // The other process: the same file, opened on its own, locked with the OS's file lock.
    let other = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .unwrap();
    other.lock().unwrap();
    assert!(
        locks
            .lock("azul-storage/s3/d_1", Duration::from_millis(200))
            .is_err(),
        "taken while another process holds it"
    );
    other.unlock().unwrap();
    let held = locks
        .lock("azul-storage/s3/d_1", Duration::from_millis(500))
        .unwrap();
    assert!(
        matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "the other process takes it while this one holds it"
    );
    drop(held);
    other.try_lock().expect("released with its holder");
    assert!(path.exists(), "a lock file stays: a waiter never locks a file nobody else does");
}

#[test]
fn a_lock_name_never_leaves_its_folder() {
    let dir = TempDir::new("azcloud-lock");
    let locks = LockDir::new(dir.path());
    let long = "x".repeat(300);
    for name in [
        "azul-storage/s3/d_1",
        "../../etc/passwd",
        "a\\b",
        "",
        "C:",
        long.as_str(),
    ] {
        let file = locks.file_of(name);
        assert_eq!(file.parent(), Some(dir.path()), "{name:?}");
        let file_name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert!(file_name.ends_with(".lock"), "{file_name}");
        assert!(file_name.len() <= 120, "{file_name}");
        assert!(
            file_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "{file_name}"
        );
    }
    assert_ne!(
        locks.file_of("azul-storage/s3/d_1"),
        locks.file_of("azul-storage/s3/d_2")
    );
}
