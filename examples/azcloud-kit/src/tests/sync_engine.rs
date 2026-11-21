//! The engine around a pass, through real passes against an S3 service in memory: the loop
//! detector stops a flip-flop, the operations log hears every delete, the problem list is
//! published, the journal is recovered first, other people's files get a quarantine mark.

use std::{fs, path::PathBuf};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore},
};
use crate::sync::{
    engine::{Engine, PassSummary},
    journal::{Intent, Journal},
    local::{hash_bytes, path_of},
    oplog::OpKind,
    problems::Code,
    quarantine::{self, OwnDevices},
    sync_to, LocalRoot, RunHooks, SyncOptions, SyncReport, Target,
};

const PREFIX: &str = "e2e/engine/";

struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
    own: Vec<String>,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("engine-root-{name}")),
            state: TempDir::new(&format!("engine-state-{name}")),
            own: vec![name.to_string()],
        }
    }

    fn engine(&self) -> Engine {
        Engine::new(
            self.state.path(),
            self.folder.path(),
            Box::new(OwnDevices(self.own.clone())),
        )
    }

    fn index(&self) -> PathBuf {
        self.state.path().join("index.json")
    }

    fn pass(&self, store: &dyn RemoteStore) -> (Option<SyncReport>, PassSummary) {
        let root = LocalRoot::folder(self.folder.path());
        let opts = SyncOptions::new(PREFIX, "d-test", self.name).unwrap();
        let index = self.index();
        let (result, summary) = self.engine().pass(RunHooks::default(), |hooks| {
            sync_to(Target::Index(store), &root, &index, &opts, hooks)
        });
        (result.ok(), summary)
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn remove(&self, key: &str) {
        fs::remove_file(path_of(self.folder.path(), key)).unwrap();
    }

    fn has(&self, key: &str) -> bool {
        path_of(self.folder.path(), key).exists()
    }
}

#[test]
fn a_file_another_device_keeps_deleting_and_putting_back_is_frozen_and_listed() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("keep.txt", b"unrelated");
    a.write("flap.txt", b"flip-flop");
    a.pass(&store);
    b.pass(&store);
    assert!(b.has("flap.txt"));
    // dev-a deletes and restores it, again and again; dev-b follows until it sees the loop.
    let mut frozen_at = None;
    for round in 0..4 {
        a.remove("flap.txt");
        a.pass(&store);
        let (_, deleted) = b.pass(&store);
        a.write("flap.txt", b"flip-flop");
        a.pass(&store);
        let (_, restored) = b.pass(&store);
        let listed = |s: &PassSummary| s.problems.problems.iter().any(|p| p.code == Code::LoopFrozen && p.key == "flap.txt");
        if frozen_at.is_none() && (listed(&deleted) || listed(&restored)) {
            frozen_at = Some(round);
        }
    }
    assert!(frozen_at.is_some(), "the flip-flop is never frozen");
    let deletes = b
        .engine()
        .oplog()
        .of_key("flap.txt")
        .into_iter()
        .filter(|op| matches!(op.op, OpKind::Delete | OpKind::Trash))
        .count();
    assert!(deletes <= 2, "dev-b deleted flap.txt {deletes} times");
    assert_eq!(fs::read(path_of(b.folder.path(), "keep.txt")).unwrap(), b"unrelated");
}

#[test]
fn a_delete_here_is_in_the_operations_log_with_the_content_it_had() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("one.txt", b"one");
    a.write("two.txt", b"two");
    a.pass(&store);
    b.pass(&store);
    a.remove("one.txt");
    a.pass(&store);
    b.pass(&store);
    assert!(!b.has("one.txt"));
    let ops = b.engine().oplog().of_key("one.txt");
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert_eq!(ops[0].hash.as_deref(), Some(hash_bytes(b"one").as_str()));
}

#[cfg(unix)]
#[test]
fn the_problem_list_names_a_skipped_link_until_it_is_gone() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("real.txt", b"real");
    std::os::unix::fs::symlink("/etc/hosts", a.folder.path().join("hosts")).unwrap();
    let (_, first) = a.pass(&store);
    assert!(
        first
            .problems
            .problems
            .iter()
            .any(|p| p.code == Code::SymlinkSkipped && p.key == "hosts"),
        "{:?}",
        first.problems
    );
    fs::remove_file(a.folder.path().join("hosts")).unwrap();
    let (_, second) = a.pass(&store);
    assert!(second.problems.problems.iter().all(|p| p.key != "hosts"));
    assert_eq!(second.exit, 0);
}

#[test]
fn a_half_download_a_crash_left_is_cleaned_up_before_the_next_pass_scans() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("doc.txt", b"old");
    a.pass(&store);
    let temp = a.folder.path().join(".doc.txt.azul-storage-99-0.tmp");
    fs::write(&temp, b"half of new").unwrap();
    Journal::new(a.state.path())
        .begin(Intent::Download {
            key: String::from("doc.txt"),
            hash: hash_bytes(b"new"),
        })
        .unwrap();
    let (_, summary) = a.pass(&store);
    assert!(!temp.exists(), "{:?}", summary.recovery);
    assert_eq!(summary.recovery.rolled_back.len(), 1);
    assert_eq!(fs::read(a.folder.path().join("doc.txt")).unwrap(), b"old");
}

#[test]
fn a_file_from_someone_elses_device_gets_the_quarantine_mark_and_ones_own_do_not() {
    let store = S3Bucket::new();
    let stranger = Device::new("stranger");
    let mut mine = Device::new("laptop");
    let desk = Device::new("desk");
    mine.own.push(String::from("desk"));
    stranger.write("tool.command", b"#!/bin/sh\necho hi\n");
    stranger.pass(&store);
    desk.write("notes.txt", b"mine");
    desk.pass(&store);
    mine.pass(&store);
    let tool = path_of(mine.folder.path(), "tool.command");
    let notes = path_of(mine.folder.path(), "notes.txt");
    assert!(tool.exists() && notes.exists());
    assert_eq!(quarantine::is_marked(&tool), quarantine::supported());
    assert!(!quarantine::is_marked(&notes));
}

/// A drive no request reaches.
struct Unreachable;

fn unreachable<T>() -> CloudResult<T> {
    Err(CloudError::Drive(azul_storage::DriveError::Transport(String::from(
        "connection refused",
    ))))
}

impl RemoteStore for Unreachable {
    fn get_unless(&self, _key: &str, _etag: Option<&str>) -> CloudResult<Conditional> {
        unreachable()
    }
    fn fetch(&self, _key: &str, _size: u64) -> CloudResult<Option<Vec<u8>>> {
        unreachable()
    }
    fn put(&self, _key: &str, _data: &[u8]) -> CloudResult<String> {
        unreachable()
    }
    fn put_if(&self, _key: &str, _data: &[u8], _if: Option<&str>) -> CloudResult<Option<String>> {
        unreachable()
    }
    fn head(&self, _key: &str) -> CloudResult<Option<u64>> {
        unreachable()
    }
    fn delete(&self, _key: &str) -> CloudResult<()> {
        unreachable()
    }
    fn list(&self, _prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        unreachable()
    }
}

#[test]
fn a_pass_that_cannot_reach_the_drive_lists_it_and_keeps_the_files_problems() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.txt", b"a");
    let (_, ok) = a.pass(&store);
    assert_eq!(ok.exit, 0);
    let (result, failed) = a.pass(&Unreachable);
    assert!(result.is_none());
    assert!(
        failed.problems.problems.iter().any(|p| p.key.is_empty()),
        "the pass-wide problem is listed: {:?}",
        failed.problems
    );
    assert!(failed.wait_secs > 0.0, "the next pass waits");
}
