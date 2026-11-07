//! The engine against what the delta review of the file lanes found (X-DELTA-AZDRIVE-FILES-*):
//! each test enters through `xfer::run`, the function production calls, with a plan made for it,
//! and names the finding. The fakes here are drives the lane's earlier fakes were not: one that asks
//! its condition when an upload COMPLETES (a bucket), one whose source has no version to name.

use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Mutex,
    },
};

use super::{xfer_local::Versioned, TempDir};
use crate::{
    ctl::{Ctl, Quiet},
    xfer::{plan as make_plan, run, Choice, Item, Plan, Report},
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo, Precondition,
    SMALL_FILE_BYTES,
};

fn file(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

fn go(plan: &Plan, source: &dyn Drive, target: &dyn Drive, removes_source: bool) -> Report {
    run(plan, source, target, removes_source, &Quiet, &mut |_| {})
}

fn conflict(report: &Report, key: &str) -> bool {
    report
        .failed
        .iter()
        .any(|(what, why)| what == key && matches!(why, DriveError::Conflict { .. }))
}

/// A bucket that asks its condition when a write COMPLETES, as S3 does on the request that ends an
/// upload, and lets another device save a name while the upload is in flight: `meanwhile` is
/// written right before the condition is looked at, by every kind of write.
#[derive(Default)]
pub(super) struct Racing {
    files: Mutex<BTreeMap<String, (Vec<u8>, u64)>>,
    clock: AtomicU64,
    /// `(key, bytes)`: saved by another device when the next write of `key` completes.
    meanwhile: Mutex<Option<(String, Vec<u8>)>>,
    /// Writes that went through with no condition asked.
    pub(super) unconditional: AtomicU32,
    /// Writes that asked a condition.
    pub(super) conditional: AtomicU32,
}

impl Racing {
    pub(super) fn with(files: &[(&str, &[u8])]) -> Racing {
        let drive = Racing::default();
        for (key, bytes) in files {
            drive.write(key, bytes);
        }
        drive
    }

    pub(super) fn write(&self, key: &str, bytes: &[u8]) {
        let version = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), (bytes.to_vec(), version));
    }

    /// Another device saves `key` (with `bytes`) while the next upload of it is on its way.
    pub(super) fn save_meanwhile(&self, key: &str, bytes: &[u8]) {
        *self.meanwhile.lock().unwrap() = Some((key.to_string(), bytes.to_vec()));
    }

    /// The write that ends an upload: the other device's save lands first, then the condition is
    /// asked (when there is one) and the bytes go in.
    fn complete(
        &self,
        key: &str,
        bytes: &[u8],
        condition: Option<&Precondition>,
    ) -> Result<Option<String>, DriveError> {
        let theirs = {
            let mut slot = self.meanwhile.lock().unwrap();
            match slot.take() {
                Some((name, bytes)) if name == key => Some(bytes),
                other => {
                    *slot = other;
                    None
                }
            }
        };
        if let Some(theirs) = theirs {
            self.write(key, &theirs);
        }
        match condition {
            None => {
                self.unconditional.fetch_add(1, Ordering::SeqCst);
            }
            Some(condition) => {
                self.conditional.fetch_add(1, Ordering::SeqCst);
                let files = self.files.lock().unwrap();
                let holds = match (condition, files.get(key)) {
                    (Precondition::Absent, None) => true,
                    (Precondition::Matches(tag), Some((_, version))) => {
                        *tag == format!("v{version}")
                    }
                    _ => false,
                };
                if !holds {
                    return Err(DriveError::Conflict {
                        key: key.to_string(),
                    });
                }
            }
        }
        self.write(key, bytes);
        Ok(Some(format!("v{}", self.clock.load(Ordering::SeqCst))))
    }
}

fn read_all(body: &mut dyn Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes).unwrap();
    bytes
}

impl Drive for Racing {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let files = self.files.lock().unwrap();
        let mut page = ListPage::default();
        for (key, (bytes, version)) in files.iter() {
            if key.starts_with(&request.prefix) {
                page.objects.push(ObjectInfo {
                    key: key.clone(),
                    size: bytes.len() as u64,
                    modified: Some(1),
                    etag: Some(format!("v{version}")),
                });
            }
        }
        Ok(page)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
    fn get_range(&self, key: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.complete(key, bytes, None).map(|_| ())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, version)| ObjectInfo {
                key: key.to_string(),
                size: bytes.len() as u64,
                modified: Some(1),
                etag: Some(format!("v{version}")),
            })
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        let bytes = read_all(body);
        self.complete(key, &bytes, None)?;
        Ok(bytes.len() as u64)
    }
    fn put_file(&self, key: &str, path: &Path, _: &dyn Ctl) -> Result<u64, DriveError> {
        let bytes = std::fs::read(path)?;
        self.complete(key, &bytes, None)?;
        Ok(bytes.len() as u64)
    }
    fn conditional_writes(&self) -> bool {
        true
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.complete(key, bytes, Some(condition))
    }
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let bytes = read_all(body);
        self.complete(key, &bytes, Some(condition))
    }
    fn put_file_if(
        &self,
        key: &str,
        path: &Path,
        condition: &Precondition,
        _: &dyn Ctl,
    ) -> Result<Option<String>, DriveError> {
        let bytes = std::fs::read(path)?;
        self.complete(key, &bytes, Some(condition))
    }
}

/// A local folder with a file of `SMALL_FILE_BYTES + 1` bytes (it goes its own way, not in a
/// batch) and a smaller one.
fn local_with_big_file(tmp: &TempDir) -> (LocalDrive, Vec<u8>) {
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let big: Vec<u8> = (0..=SMALL_FILE_BYTES).map(|i| (i % 251) as u8).collect();
    home.put("master.mov", &big).unwrap();
    (home, big)
}

fn replace_plan(source: &dyn Drive, target: &dyn Drive) -> Plan {
    let mut plan = make_plan(source, &[file("master.mov")], target, "", true, "Copy").unwrap();
    assert_eq!(plan.conflicts().len(), 1, "the name is taken at the target");
    plan.choose(0, Choice::Replace);
    plan
}

/// X-DELTA-AZDRIVE-FILES-3 (AZ-U25). The plan saw `master.mov` v1 at the bucket and the user chose
/// Replace; the upload of the big file takes an hour; another device saves the name in that hour.
/// The guard was asked once, at the start, and the upload was written unconditionally over the
/// other device's save (a plain bucket keeps nothing), and a Move then deleted the source. The
/// request that completes the upload asks the condition now: Conflict, the other device's file
/// stays, the source stays.
#[test]
fn x_delta_azdrive_files_3_a_big_file_replaces_only_the_version_the_plan_saw() {
    let tmp = TempDir::new("dx3-replace");
    let (home, big) = local_with_big_file(&tmp);
    let bucket = Racing::with(&[("master.mov", b"v1 of the bucket")]);
    let plan = replace_plan(&home, &bucket);
    bucket.save_meanwhile("master.mov", b"saved by another device");
    let report = go(&plan, &home, &bucket, true);
    assert!(conflict(&report, "master.mov"), "{report:?}");
    assert_eq!(report.done, 0, "{report:?}");
    assert_eq!(
        bucket.get("master.mov").unwrap(),
        b"saved by another device",
        "the other device's file was written over"
    );
    assert_eq!(
        home.get("master.mov").unwrap(),
        big,
        "a Move retired a source whose copy never landed"
    );
    assert_eq!(bucket.unconditional.load(Ordering::SeqCst), 0);
    assert_eq!(bucket.conditional.load(Ordering::SeqCst), 1);
}

/// The same for a name that was free when the plan was made: the conditional is `Absent`.
#[test]
fn x_delta_azdrive_files_3_a_big_file_into_a_free_name_does_not_replace_a_file_made_meanwhile() {
    let tmp = TempDir::new("dx3-absent");
    let (home, big) = local_with_big_file(&tmp);
    let bucket = Racing::with(&[]);
    let plan = make_plan(&home, &[file("master.mov")], &bucket, "", false, "Copy").unwrap();
    assert!(plan.conflicts().is_empty());
    bucket.save_meanwhile("master.mov", b"saved by another device");
    let report = go(&plan, &home, &bucket, false);
    assert!(conflict(&report, "master.mov"), "{report:?}");
    assert_eq!(
        bucket.get("master.mov").unwrap(),
        b"saved by another device"
    );
    assert_eq!(home.get("master.mov").unwrap(), big);
}

/// Nothing changes for the ordinary case: the version the plan saw is replaced, a free name is
/// written, and the write asked its condition once.
#[test]
fn x_delta_azdrive_files_3_the_version_the_plan_saw_is_replaced_and_the_source_retired() {
    let tmp = TempDir::new("dx3-ok");
    let (home, big) = local_with_big_file(&tmp);
    let bucket = Racing::with(&[("master.mov", b"v1 of the bucket")]);
    let plan = replace_plan(&home, &bucket);
    let report = go(&plan, &home, &bucket, true);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, 1);
    assert_eq!(bucket.get("master.mov").unwrap(), big);
    assert!(
        home.get("master.mov").is_err(),
        "the landed source is retired"
    );
    assert_eq!(bucket.conditional.load(Ordering::SeqCst), 1);
}

/// A source with one object of `size` bytes of zeros, served in ranges: no folder, no version
/// beyond its size.
struct Zeros {
    key: &'static str,
    size: u64,
}

impl Drive for Zeros {
    fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage::default())
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.get_range(key, ByteRange::new(0, Some(self.size.saturating_sub(1))))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        if key != self.key || range.start >= self.size {
            return Err(DriveError::NotFound { key: key.into() });
        }
        let end = range
            .end
            .map_or(self.size - 1, |end| end.min(self.size - 1));
        Ok(vec![0u8; (end - range.start + 1) as usize])
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(DriveError::Denied {
            message: String::from("a source"),
        })
    }
    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if key != self.key {
            return Err(DriveError::NotFound { key: key.into() });
        }
        Ok(ObjectInfo {
            key: key.to_string(),
            size: self.size,
            modified: Some(1),
            etag: Some(String::from("src-v1")),
        })
    }
}

/// Between two drives that are not folders: a small object (one GET, one write) and a big one
/// (ranges streamed into one write) ask the condition the same way.
#[test]
fn x_delta_azdrive_files_3_an_object_between_two_remote_drives_is_written_only_if_the_name_holds() {
    // A small object.
    let source = Versioned::with(&[("movie.bin", b"the bytes of the source")]);
    let bucket = Racing::with(&[("movie.bin", b"old")]);
    let mut plan = make_plan(&source, &[file("movie.bin")], &bucket, "", false, "Copy").unwrap();
    plan.choose(0, Choice::Replace);
    bucket.save_meanwhile("movie.bin", b"saved by another device");
    let report = go(&plan, &source, &bucket, false);
    assert!(conflict(&report, "movie.bin"), "small: {report:?}");
    assert_eq!(
        bucket.get("movie.bin").unwrap(),
        b"saved by another device",
        "small: the other device's save was written over"
    );
    // An object over one chunk, streamed from ranges.
    let size = crate::transfer::CHUNK + 1;
    let source = Zeros {
        key: "movie.bin",
        size,
    };
    let bucket = Racing::with(&[("movie.bin", b"old")]);
    let items = [Item {
        key: String::from("movie.bin"),
        is_folder: false,
        size: Some(size),
    }];
    let mut plan = make_plan(&source, &items, &bucket, "", false, "Copy").unwrap();
    plan.choose(0, Choice::Replace);
    bucket.save_meanwhile("movie.bin", b"saved by another device");
    let report = go(&plan, &source, &bucket, false);
    assert!(conflict(&report, "movie.bin"), "big: {report:?}");
    assert_eq!(
        bucket.get("movie.bin").unwrap(),
        b"saved by another device",
        "big: the other device's save was written over"
    );
    assert_eq!(bucket.unconditional.load(Ordering::SeqCst), 0);
}

/// A drive that cannot ask is written as before (looked at first): the file lands, nothing is
/// refused as unsupported, and no big body is read for a question that has no answer.
#[test]
fn x_delta_azdrive_files_3_a_target_that_cannot_ask_is_written_as_before() {
    let tmp = TempDir::new("dx3-cannot");
    let (home, big) = local_with_big_file(&tmp);
    let bucket = Versioned::default();
    let plan = make_plan(&home, &[file("master.mov")], &bucket, "", true, "Copy").unwrap();
    let report = go(&plan, &home, &bucket, true);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(bucket.get("master.mov").unwrap(), big);
    assert!(home.get("master.mov").is_err());
}

/// The trait's own default for a drive that cannot ask: refused at once, the file unread.
#[test]
fn x_delta_azdrive_files_3_put_file_if_of_a_drive_that_cannot_ask_is_refused_unread() {
    let tmp = TempDir::new("dx3-default");
    let bucket = Versioned::default();
    let missing = tmp.path().join("not-there.bin");
    let error = bucket
        .put_file_if("x", &missing, &Precondition::Absent, &Quiet)
        .unwrap_err();
    assert!(
        matches!(error, DriveError::Unsupported(_)),
        "the missing file was opened first: {error:?}"
    );
}

// ==== X-DELTA-AZDRIVE-FILES-4: a source retired by a version the drive could not name ====

/// A source whose head and listing carry no entity tag (a WebDAV server that sends none, a data
/// source): at most a size and a date. `dates`: whether it gives the date.
struct NoTags {
    files: Mutex<BTreeMap<String, (Vec<u8>, u64)>>,
    clock: AtomicU64,
    dates: bool,
    /// The key to edit right after a read of it: an editor saving while the copy runs.
    edit_after_read: Mutex<Option<String>>,
    /// A look at a file fails (a connection that dropped for a moment).
    head_fails: std::sync::atomic::AtomicBool,
}

impl NoTags {
    fn with(files: &[(&str, &[u8])], dates: bool) -> NoTags {
        let drive = NoTags {
            files: Mutex::new(BTreeMap::new()),
            clock: AtomicU64::new(0),
            dates,
            edit_after_read: Mutex::new(None),
            head_fails: std::sync::atomic::AtomicBool::new(false),
        };
        for (key, bytes) in files {
            drive.write(key, bytes);
        }
        drive
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let at = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), (bytes.to_vec(), at));
    }

    fn info(&self, key: &str, bytes: &[u8], at: u64) -> ObjectInfo {
        ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: self.dates.then_some(at),
            etag: None,
        }
    }
}

impl Drive for NoTags {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let files = self.files.lock().unwrap();
        let mut page = ListPage::default();
        for (key, (bytes, at)) in files.iter() {
            if key.starts_with(&request.prefix) {
                page.objects.push(self.info(key, bytes, *at));
            }
        }
        Ok(page)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let bytes = self
            .files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| DriveError::NotFound { key: key.into() })?;
        let edit = self.edit_after_read.lock().unwrap().take();
        if edit.as_deref() == Some(key) {
            self.write(key, b"edited meanwhile, and longer");
        }
        Ok(bytes)
    }
    fn get_range(&self, key: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.write(key, bytes);
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if self.head_fails.load(Ordering::SeqCst) {
            return Err(DriveError::Transport(String::from(
                "the connection dropped",
            )));
        }
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, at)| self.info(key, bytes, *at))
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
}

fn move_one(source: &NoTags, target: &LocalDrive, tamper: impl FnOnce(&NoTags)) -> Report {
    let items = [Item {
        key: String::from("a.txt"),
        is_folder: false,
        size: Some(7),
    }];
    let plan = make_plan(source, &items, target, "", true, "Copy").unwrap();
    tamper(source);
    go(&plan, source, target, true)
}

/// X-DELTA-AZDRIVE-FILES-4. A Move from a drive whose head names no tag (a WebDAV server without
/// ETags) retired the source with an unconditional delete: the edit the user saved at the source
/// while the file was copied was deleted with it, the copy holding the old bytes. The source's size
/// and date noted before the copy are looked at again before the delete: it is kept when either
/// differs.
#[test]
fn x_delta_azdrive_files_4_a_source_without_tags_is_kept_when_it_changed_while_it_was_copied() {
    let tmp = TempDir::new("dx4-edit");
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let source = NoTags::with(&[("a.txt", b"v1 text")], true);
    let report = move_one(&source, &target, |source| {
        *source.edit_after_read.lock().unwrap() = Some(String::from("a.txt"));
    });
    assert!(conflict(&report, "a.txt"), "{report:?}");
    assert_eq!(
        source.get("a.txt").unwrap(),
        b"edited meanwhile, and longer",
        "the edit saved while the file was copied was deleted"
    );
    assert_eq!(target.get("a.txt").unwrap(), b"v1 text", "the copy landed");
}

/// The ordinary case of such a source: nothing changed, so the file moves.
#[test]
fn x_delta_azdrive_files_4_a_source_without_tags_that_did_not_change_is_moved() {
    let tmp = TempDir::new("dx4-same");
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let source = NoTags::with(&[("a.txt", b"v1 text")], true);
    let report = move_one(&source, &target, |_| {});
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, 1);
    assert!(source.get("a.txt").is_err(), "retired");
    assert_eq!(target.get("a.txt").unwrap(), b"v1 text");
}

/// A drive that names neither a tag nor a date cannot tell an edit from the file that was copied:
/// the file is copied and kept, and the report says so (not "moved").
#[test]
fn x_delta_azdrive_files_4_a_source_with_no_version_at_all_is_copied_and_kept() {
    let tmp = TempDir::new("dx4-none");
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let source = NoTags::with(&[("a.txt", b"v1 text")], false);
    let report = move_one(&source, &target, |_| {});
    assert_eq!(report.done, 1, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(
        matches!(report.failed[0].1, DriveError::Unsupported(_)),
        "{report:?}"
    );
    assert_eq!(
        source.get("a.txt").unwrap(),
        b"v1 text",
        "deleted on a guess"
    );
    assert_eq!(target.get("a.txt").unwrap(), b"v1 text");
}

/// A look at the source that fails is no "unknown version": the file is neither copied nor
/// retired (the old code took the failure for "no tag" and deleted the source unconditionally).
#[test]
fn x_delta_azdrive_files_4_a_failed_look_at_the_source_moves_nothing() {
    let tmp = TempDir::new("dx4-head");
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let source = NoTags::with(&[("a.txt", b"v1 text")], true);
    let report = move_one(&source, &target, |source| {
        source.head_fails.store(true, Ordering::SeqCst);
    });
    assert_eq!(report.done, 0, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(
        matches!(report.failed[0].1, DriveError::Transport(_)),
        "{report:?}"
    );
    assert_eq!(source.get("a.txt").unwrap(), b"v1 text");
    assert!(target.get("a.txt").is_err(), "nothing was copied");
}
