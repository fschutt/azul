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

fn folder(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: true,
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
    /// The calls made on it, in order: `get:<key>`, `head:<key>`, `delete:<key>`.
    log: Mutex<Vec<String>>,
}

impl NoTags {
    fn with(files: &[(&str, &[u8])], dates: bool) -> NoTags {
        let drive = NoTags {
            files: Mutex::new(BTreeMap::new()),
            clock: AtomicU64::new(0),
            dates,
            edit_after_read: Mutex::new(None),
            head_fails: std::sync::atomic::AtomicBool::new(false),
            log: Mutex::new(Vec::new()),
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
        self.log.lock().unwrap().push(format!("get:{key}"));
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
        self.log.lock().unwrap().push(format!("delete:{key}"));
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.log.lock().unwrap().push(format!("head:{key}"));
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

// ---- finding 15: the empty folders of a folder that is moved

fn two_disks(name: &str) -> (TempDir, LocalDrive, LocalDrive) {
    let tmp = TempDir::new(name);
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    (tmp, home, other)
}

/// A folder on disk has no marker: its empty folders are in no recursive listing. A Move to another
/// drive removed them from the source and never made them at the target - the structure existed
/// nowhere. The plan names them and the run makes them before the source is touched.
#[test]
fn x_delta_azdrive_files_15_a_move_takes_the_empty_folders_along() {
    let (tmp, home, other) = two_disks("dx15-move");
    home.put("docs/a.txt", b"alpha").unwrap();
    home.create_folder("docs/empty/").unwrap();
    home.create_folder("docs/sub/deep/er/").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    for wanted in ["docs/empty/", "docs/sub/deep/", "docs/sub/deep/er/"] {
        assert!(
            plan.folders.iter().any(|f| f == wanted),
            "{wanted} is planned: {:?}",
            plan.folders
        );
    }
    // The parent of a folder is planned before the folder.
    let at = |key: &str| plan.folders.iter().position(|f| f == key).unwrap();
    assert!(at("docs/sub/deep/") < at("docs/sub/deep/er/"));
    let report = go(&plan, &home, &other, true);
    assert!(
        report.failed.is_empty() && report.left.is_empty(),
        "{report:?}"
    );
    let there = tmp.path().join("other").join("docs");
    assert!(there.join("empty").is_dir(), "the empty folder came along");
    assert!(there.join("sub").join("deep").join("er").is_dir());
    assert_eq!(other.get("docs/sub/b.txt").unwrap(), b"beta");
    assert!(
        !tmp.path().join("home").join("docs").exists(),
        "the source is gone, as a move leaves it"
    );
}

/// A Copy takes them too: the copy of a folder is the folder.
#[test]
fn x_delta_azdrive_files_15_a_copy_takes_the_empty_folders_along() {
    let (tmp, home, other) = two_disks("dx15-copy");
    home.put("docs/a.txt", b"alpha").unwrap();
    home.create_folder("docs/empty/").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", false, "Copy").unwrap();
    let report = go(&plan, &home, &other, false);
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(tmp.path().join("other/docs/empty").is_dir());
    assert!(
        tmp.path().join("home/docs/empty").is_dir(),
        "a copy leaves the source whole"
    );
}

/// A link to a folder is not entered and not planned: no loop, and nothing outside the folder is
/// made into its structure.
#[cfg(unix)]
#[test]
fn x_delta_azdrive_files_15_a_link_to_a_folder_is_not_planned() {
    let (tmp, home, other) = two_disks("dx15-link");
    home.put("docs/a.txt", b"alpha").unwrap();
    std::fs::create_dir_all(tmp.path().join("outside").join("inner")).unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("outside"),
        tmp.path().join("home").join("docs").join("link"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("home").join("docs"),
        tmp.path().join("home").join("docs").join("loop"),
    )
    .unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", false, "Copy").unwrap();
    assert!(
        plan.folders.iter().all(|f| f == "docs/"),
        "only the folder itself: {:?}",
        plan.folders
    );
}

/// A folder the target would not make stays at the source: an empty folder is in no file's way, and
/// the move that removed it would end it everywhere.
#[test]
fn x_delta_azdrive_files_15_a_folder_the_target_refused_stays_at_the_source() {
    let (tmp, home, other) = two_disks("dx15-refused");
    home.put("docs/a.txt", b"alpha").unwrap();
    home.create_folder("docs/empty/").unwrap();
    // A FILE of that name at the target: its folder cannot be made.
    other.put("docs/empty", b"in the way").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    let report = go(&plan, &home, &other, true);
    assert!(
        report.failed.iter().any(|(what, _)| what == "docs/empty/"),
        "the refusal is the folder's failure: {report:?}"
    );
    assert!(
        tmp.path().join("home/docs/empty").is_dir(),
        "the empty folder is still at the source"
    );
    assert_eq!(report.left, vec![String::from("docs/")], "{report:?}");
    assert_eq!(other.get("docs/a.txt").unwrap(), b"alpha");
}

// ---- finding 32: names a drive gives

/// Paste, download, drop and "Copy everything" of a drive wrote the names it gave through
/// `LocalDrive::path_of`, which checks the shape only: `.GIT/config` of a shared drive is
/// `.git/config` on APFS and NTFS (a conflict question, then a file in a real repository). A spelling
/// the disk would read as a protected name is refused by the plan, each as a failure of its own;
/// the exact `.git` of a repository the user copies on purpose is not.
#[test]
fn x_delta_azdrive_files_32_a_name_the_disk_reads_as_git_is_not_written_from_a_drive() {
    let tmp = TempDir::new("dx32-names");
    let source = Versioned::with(&[
        ("proj/.GIT/config", b"[core]\nfsmonitor = x"),
        ("proj/GIT~1/hooks/post-commit", b"#!/bin/sh"),
        ("proj/.AZLIN/x", b"y"),
        ("proj/.git/config", b"[core]"),
        ("proj/readme.txt", b"hello"),
    ]);
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let plan = make_plan(&source, &[folder("proj/")], &target, "", true, "Copy").unwrap();
    let mut refused: Vec<&str> = plan.refused.iter().map(|(key, _)| key.as_str()).collect();
    refused.sort_unstable();
    assert_eq!(
        refused,
        [
            "proj/.AZLIN/x",
            "proj/.GIT/config",
            "proj/GIT~1/hooks/post-commit"
        ]
    );
    let planned: Vec<&str> = plan.files.iter().map(|f| f.target_key.as_str()).collect();
    assert!(!planned
        .iter()
        .any(|key| key.contains("GIT") && !key.contains(".git/")));
    let report = go(&plan, &source, &target, true);
    let refusals = report
        .failed
        .iter()
        .filter(|(_, why)| matches!(why, DriveError::InvalidKey { .. }))
        .count();
    assert_eq!(refusals, 3, "{report:?}");
    assert_eq!(report.done, 2, "{report:?}");
    assert_eq!(target.get("proj/readme.txt").unwrap(), b"hello");
    assert_eq!(target.get("proj/.git/config").unwrap(), b"[core]");
    let mut made: Vec<String> = std::fs::read_dir(tmp.path().join("other").join("proj"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    made.sort();
    assert_eq!(
        made,
        [".git", "readme.txt"],
        "nothing was made for the refused names"
    );
    // A move leaves the refused names where they are.
    assert_eq!(
        source.get("proj/.GIT/config").unwrap(),
        b"[core]\nfsmonitor = x"
    );
    assert!(
        source.get("proj/readme.txt").is_err(),
        "the copied file moved"
    );
}

/// A file picked by itself is checked too, and a copy from one folder of this disk to another is
/// not: those names are on the disk already.
#[test]
fn x_delta_azdrive_files_32_a_single_file_is_checked_and_a_copy_on_this_disk_is_not() {
    let tmp = TempDir::new("dx32-single");
    let source = Versioned::with(&[(".GIT", b"gitdir: ../elsewhere")]);
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let plan = make_plan(&source, &[file(".GIT")], &target, "", false, "Copy").unwrap();
    assert_eq!(plan.refused.len(), 1, "{plan:?}");
    assert!(plan.files.is_empty());
    assert!(
        !plan.is_empty(),
        "a plan that refused something is not 'nothing to do'"
    );
    // The same names between two folders of this computer: nobody downloaded them.
    let (_tmp, home, other) = two_disks("dx32-local");
    home.put("proj/.GIT/x", b"mine").unwrap();
    let plan = make_plan(&home, &[folder("proj/")], &other, "", false, "Copy").unwrap();
    assert!(plan.refused.is_empty(), "{plan:?}");
    let report = go(&plan, &home, &other, false);
    assert!(report.failed.is_empty(), "{report:?}");
}

/// The names below the folder a drive's items go into are the drive's; the folder is the user's.
#[test]
fn x_delta_azdrive_files_32_only_the_names_below_the_target_folder_are_checked() {
    let tmp = TempDir::new("dx32-prefix");
    // A folder of the user's own, spelled the way a drive's name may not be.
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    target.put(".GIT/keep.txt", b"theirs").unwrap();
    let source = Versioned::with(&[("a.txt", b"alpha")]);
    let plan = make_plan(&source, &[file("a.txt")], &target, ".GIT/", false, "Copy").unwrap();
    assert!(plan.refused.is_empty(), "{plan:?}");
    assert_eq!(plan.files.len(), 1);
}

/// X-DELTA-SYNC2-7. `retire_sources` looked at EVERY source of a Move off a drive with no tags first
/// and deleted afterwards: ten thousand files at 50 ms a look were eight minutes between the first
/// file's look and its delete, and an edit saved in them was deleted with the source. It looks and
/// deletes in groups of `VERSIONED_GROUP` now, as a bucket's versioned delete does: the first
/// delete comes after at most one group's looks, and a later group is looked at after the earlier
/// one was deleted. RED on the old code: forty looks stand before the first delete.
#[test]
fn x_delta_sync2_7_a_move_off_a_tagless_source_looks_and_deletes_in_groups() {
    use crate::s3::VERSIONED_GROUP;
    const FILES: usize = 40;
    assert!(FILES > 2 * VERSIONED_GROUP, "the move spans three groups");
    let tmp = TempDir::new("dx-sync2-7");
    let target = LocalDrive::without_manifest(tmp.path().join("other"));
    let names: Vec<String> = (0..FILES).map(|i| format!("f{i:02}.txt")).collect();
    let files: Vec<(&str, &[u8])> = names
        .iter()
        .map(|name| (name.as_str(), &b"the same text"[..]))
        .collect();
    let source = NoTags::with(&files, true);
    let items: Vec<Item> = names
        .iter()
        .map(|name| Item {
            key: name.clone(),
            is_folder: false,
            size: Some(13),
        })
        .collect();
    let plan = make_plan(&source, &items, &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, true);
    // What the run asked of the source, before this test asks anything of it.
    let log = source.log.lock().unwrap().clone();
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, FILES);
    for name in &names {
        assert!(source.get(name).is_err(), "{name} was retired");
        assert_eq!(target.get(name).unwrap(), b"the same text");
    }
    // The retire phase: what the source was asked after its last read.
    let retire: Vec<&String> = log
        .iter()
        .skip(
            log.iter()
                .rposition(|call| call.starts_with("get:"))
                .unwrap()
                + 1,
        )
        .filter(|call| !call.starts_with("get:"))
        .collect();
    let looks_before_first_delete = retire
        .iter()
        .take_while(|call| call.starts_with("head:"))
        .count();
    assert!(
        looks_before_first_delete <= VERSIONED_GROUP,
        "{looks_before_first_delete} looks stood between the first look and the first delete \
         (more than one group of {VERSIONED_GROUP}): {retire:?}"
    );
    // And the later groups are looked at after the earlier ones were deleted: the log has
    // delete..., head... more than once.
    let switches = retire
        .windows(2)
        .filter(|pair| pair[0].starts_with("delete:") && pair[1].starts_with("head:"))
        .count();
    assert_eq!(
        switches,
        FILES.div_ceil(VERSIONED_GROUP) - 1,
        "each group is deleted before the next one is looked at: {retire:?}"
    );
}

// ==== X-DELTA-SYNC2-8: a look at the target that failed is no "no version" ====

/// A target in front of a folder on disk whose look at ONE name answers `look` (a drive that lists
/// the file and then cannot say what it is: a timeout, a refusal, a server that has no HEAD).
struct BadLook<'a> {
    inner: &'a LocalDrive,
    key: &'static str,
    look: fn() -> DriveError,
}

impl Drive for BadLook<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if key == self.key {
            return Err((self.look)());
        }
        self.inner.head(key)
    }
}

/// `docs/a.txt` is in both drives, so copying the folder `docs/` meets a conflict at the target.
fn docs_in_both(name: &str) -> (TempDir, LocalDrive, LocalDrive) {
    let (tmp, home, other) = two_disks(name);
    home.put("docs/a.txt", b"the new text").unwrap();
    other.put("docs/a.txt", b"what is there").unwrap();
    (tmp, home, other)
}

/// X-DELTA-SYNC2-8. The plan lists the target's folder (the name is known to be there) and looks at
/// the name for its version; when that look FAILED the plan went on with no version and no size, and
/// the user's Replace became an unconditional write over whatever another device had saved there
/// since. The plan fails now, with the drive's own error, and nothing is written. RED on the old
/// code: `plan` answers Ok and `unwrap_err` panics.
#[test]
fn x_delta_sync2_8_a_failed_look_at_a_taken_name_fails_the_plan() {
    let (_tmp, home, other) = docs_in_both("dx-sync2-8-failed");
    let target = BadLook {
        inner: &other,
        key: "docs/a.txt",
        look: || DriveError::Transport(String::from("the connection timed out")),
    };
    let failed = make_plan(&home, &[folder("docs/")], &target, "", false, "Copy")
        .expect_err("a plan that could not look at a name it must not overwrite fails");
    assert!(
        matches!(failed, DriveError::Transport(_)),
        "the drive's own error: {failed:?}"
    );
    assert_eq!(other.get("docs/a.txt").unwrap(), b"what is there");
}

/// The documented exception: a target that cannot look at a file at all (no HEAD) tells no versions,
/// and what the user is asked about is replaced as it stands.
#[test]
fn x_delta_sync2_8_a_target_with_no_look_tells_no_versions_and_the_plan_goes_on() {
    let (_tmp, home, other) = docs_in_both("dx-sync2-8-unsupported");
    let target = BadLook {
        inner: &other,
        key: "docs/a.txt",
        look: || DriveError::Unsupported(String::from("this drive cannot look at a file")),
    };
    let plan = make_plan(&home, &[folder("docs/")], &target, "", false, "Copy").unwrap();
    assert_eq!(plan.conflicts().len(), 1, "{plan:?}");
    assert_eq!(plan.files[0].target_etag, None);
}

/// A name that was listed and is gone when it is looked at is free: no conflict to ask about, and
/// the write that creates it asks for exactly that.
#[test]
fn x_delta_sync2_8_a_name_that_is_gone_when_it_is_looked_at_is_no_conflict() {
    let (_tmp, home, other) = docs_in_both("dx-sync2-8-gone");
    let target = BadLook {
        inner: &other,
        key: "docs/a.txt",
        look: || DriveError::NotFound {
            key: String::from("docs/a.txt"),
        },
    };
    let plan = make_plan(&home, &[folder("docs/")], &target, "", false, "Copy").unwrap();
    assert!(plan.conflicts().is_empty(), "{plan:?}");
    assert_eq!(plan.files.len(), 1);
}

// ==== X-DELTA-SYNC2-3: an object that changes while a copy between two drives streams it ====

/// A drive in memory that plays another device saving an object while a copy streams it: right
/// after the first ranged read of a key, `then` (another version, the same size) takes its place.
/// What a drive that has no folder here is to a copy - nothing but `get_range` and `put_from`.
#[derive(Default)]
struct Swapping {
    files: Mutex<BTreeMap<String, (Vec<u8>, u64)>>,
    clock: AtomicU64,
    /// `(key, bytes)`: put in place after the first ranged read of the key.
    then: Mutex<Option<(String, Vec<u8>)>>,
    /// A look at an object fails once a range was read (a connection that drops at the end).
    head_fails_after_a_range: std::sync::atomic::AtomicBool,
    /// A look at an object says it is empty (a drive that does not tell its sizes).
    head_says_empty: std::sync::atomic::AtomicBool,
    /// The drive names no tag and no date of an object (a database, a service that gives neither):
    /// all a look can say is its size.
    tagless: std::sync::atomic::AtomicBool,
    ranged: AtomicU32,
    /// The streams that were read to their end and stored.
    completed: AtomicU32,
}

impl Swapping {
    fn with(files: &[(&str, &[u8])]) -> Swapping {
        let drive = Swapping::default();
        for (key, bytes) in files {
            drive.write(key, bytes);
        }
        drive
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let version = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), (bytes.to_vec(), version));
    }

    fn info(&self, key: &str, bytes: &[u8], version: u64) -> ObjectInfo {
        let tagless = self.tagless.load(Ordering::SeqCst);
        ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: (!tagless).then_some(1),
            etag: (!tagless).then(|| format!("v{version}")),
        }
    }
}

impl Drive for Swapping {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let files = self.files.lock().unwrap();
        let mut page = ListPage::default();
        for (key, (bytes, version)) in files.iter() {
            if key.starts_with(&request.prefix) {
                page.objects.push(self.info(key, bytes, *version));
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
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        let end = range
            .end
            .map_or(bytes.len() - 1, |end| (end as usize).min(bytes.len() - 1));
        let part = bytes[range.start as usize..=end].to_vec();
        if self.ranged.fetch_add(1, Ordering::SeqCst) == 0 {
            let swap = self.then.lock().unwrap().take();
            if let Some((then_key, then_bytes)) = swap {
                self.write(&then_key, &then_bytes);
            }
        }
        Ok(part)
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
        if self.head_fails_after_a_range.load(Ordering::SeqCst)
            && self.ranged.load(Ordering::SeqCst) > 0
        {
            return Err(DriveError::Transport(String::from(
                "the connection dropped",
            )));
        }
        let empty = self.head_says_empty.load(Ordering::SeqCst);
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, version)| {
                let mut info = self.info(key, bytes, *version);
                if empty {
                    info.size = 0;
                }
                info
            })
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        // A stream that fails to its end stores nothing, as every real upload does.
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)?;
        self.write(key, &bytes);
        self.completed.fetch_add(1, Ordering::SeqCst);
        Ok(bytes.len() as u64)
    }
}

/// Two versions of a file one chunk and a little over (two ranged reads), the same size.
fn two_versions() -> (Vec<u8>, Vec<u8>) {
    let size = crate::transfer::CHUNK as usize + 4096;
    (vec![1u8; size], vec![2u8; size])
}

/// X-DELTA-SYNC2-3. A copy of an object over one chunk between two drives that are no folders streams
/// ranged GETs - a request each, tied to no version - into one upload that completes when the reader
/// ends. A file another device saved while it streamed arrived as a SPLICE of two versions under the
/// final name, a Move kept its source (it differs by tag) but left the torn file at the target, and a
/// plain copy reported "done". The reader looks at the object when it begins and again at its end; a
/// different version fails its read, so the upload is aborted before it completes: Conflict, nothing
/// under the final name, the source (which holds the other device's save) kept. RED on the old code:
/// the target holds the first chunk of one version and the rest of the other, and the report says done.
#[test]
fn x_delta_sync2_3_a_file_replaced_while_it_streams_between_two_remote_drives_is_no_finished_copy()
{
    let (v1, v2) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    *source.then.lock().unwrap() = Some((String::from("big.bin"), v2.clone()));
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, true);
    assert!(conflict(&report, "big.bin"), "{report:?}");
    assert_eq!(report.done, 0, "{report:?}");
    assert!(
        target.get("big.bin").is_err(),
        "nothing - whole or torn - stands under the final name"
    );
    assert_eq!(
        target.completed.load(Ordering::SeqCst),
        0,
        "the upload was not allowed to complete"
    );
    assert_eq!(
        source.get("big.bin").unwrap(),
        v2,
        "the Move keeps the source: it holds the other device's save"
    );
}

/// The ordinary case: nothing changes, the object arrives whole and the Move retires its source.
#[test]
fn x_delta_sync2_3_an_unchanged_big_file_is_copied_whole_and_moved() {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, true);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, 1);
    assert_eq!(target.get("big.bin").unwrap(), v1);
    assert!(source.get("big.bin").is_err(), "retired");
}

/// A look at the end that FAILS vouches for nothing: the upload is aborted with the drive's own
/// error, the target has nothing and the source stays.
#[test]
fn x_delta_sync2_3_a_failed_look_at_the_end_of_the_copy_is_no_finished_copy() {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    source
        .head_fails_after_a_range
        .store(true, Ordering::SeqCst);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, true);
    assert_eq!(report.done, 0, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(
        matches!(report.failed[0].1, DriveError::Transport(_)),
        "the drive's own error: {report:?}"
    );
    assert!(target.get("big.bin").is_err());
    assert_eq!(target.completed.load(Ordering::SeqCst), 0);
    assert_eq!(source.get("big.bin").unwrap(), v1, "the source stays");
}

/// X-DELTA-SYNC2-3, the drive that cannot pin. A source that names no tag and no date (a database,
/// a service that gives neither) leaves the look at the end with a size - and a file saved over
/// with another version of the same size is the same size. The first fix compared tag, size and
/// date: for such a drive that is "the same", and the splice of two versions arrived under the final
/// name. The end of the copy is now a second reading of the whole object against the hash of what was
/// streamed. RED on the first fix: the target holds the first chunk of one version and the rest of
/// the other, and the report says done.
#[test]
fn x_delta_sync2_3_a_file_replaced_while_a_drive_that_names_no_tag_streams_it_is_no_finished_copy()
{
    let (v1, v2) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    source.tagless.store(true, Ordering::SeqCst);
    *source.then.lock().unwrap() = Some((String::from("big.bin"), v2.clone()));
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, true);
    assert!(conflict(&report, "big.bin"), "{report:?}");
    assert_eq!(report.done, 0, "{report:?}");
    assert!(
        target.get("big.bin").is_err(),
        "nothing - whole or torn - stands under the final name"
    );
    assert_eq!(
        target.completed.load(Ordering::SeqCst),
        0,
        "the upload was not allowed to complete"
    );
    assert_eq!(
        source.get("big.bin").unwrap(),
        v2,
        "the Move keeps the source: it holds the other device's save"
    );
}

/// The ordinary case of a drive that names no tag: nothing changes, the object arrives whole and
/// the Move retires its source - after the object was read twice (two ranges, twice), which is what
/// vouching for it costs and the test pins.
#[test]
fn x_delta_sync2_3_an_unchanged_big_file_of_a_drive_that_names_no_tag_is_read_twice_and_copied_whole(
) {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    source.tagless.store(true, Ordering::SeqCst);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, false);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, 1);
    assert_eq!(target.get("big.bin").unwrap(), v1);
    assert_eq!(
        source.ranged.load(Ordering::SeqCst),
        4,
        "two ranges to copy it and the same two to check it"
    );
}

/// A drive that names a tag is looked at once more and read no more: the second reading is for the
/// drive that cannot say which version it serves.
#[test]
fn x_delta_sync2_3_a_drive_that_names_a_tag_is_not_read_twice() {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    let report = go(&plan, &source, &target, false);
    assert_eq!(report.done, 1, "{report:?}");
    assert_eq!(source.ranged.load(Ordering::SeqCst), 2);
}

/// Stops once `source` has been asked for `after` ranges: the stop of a user who cancels while the
/// object is being read the second time.
struct StopAfterRanges<'a> {
    source: &'a Swapping,
    after: u32,
}

impl Ctl for StopAfterRanges<'_> {
    fn bytes(&self, _: u64) {}
    fn cancelled(&self) -> bool {
        self.source.ranged.load(Ordering::SeqCst) >= self.after
    }
}

/// The second reading is a loop over the bytes of a big object, and it hears the stop between its
/// ranges: cancelled, the upload not completed, nothing under the final name.
#[test]
fn x_delta_sync2_3_a_stop_is_heard_in_the_second_reading_of_a_drive_that_names_no_tag() {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    source.tagless.store(true, Ordering::SeqCst);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    // Two ranges stream the object; the third is the first of the second reading.
    let stop = StopAfterRanges {
        source: &source,
        after: 3,
    };
    let report = run(&plan, &source, &target, false, &stop, &mut |_| {});
    assert!(report.cancelled, "{report:?}");
    assert_eq!(report.done, 0, "{report:?}");
    assert!(target.get("big.bin").is_err());
    assert_eq!(target.completed.load(Ordering::SeqCst), 0);
    assert_eq!(
        source.ranged.load(Ordering::SeqCst),
        3,
        "heard before the fourth range, not at the end of the reading"
    );
}

// ==== X-DELTA-SYNC2-9: a big file replaces a name inside its own bucket only through the condition ====

/// One bucket as both sides of a copy inside it: its own copy (CopyObject) writes over the name and
/// asks no condition - counted - and everything else is [`Racing`], the bucket that asks its
/// condition when a write completes. Its ranged reads are exact.
struct SameBucket {
    inner: Racing,
    copies: AtomicU32,
}

impl SameBucket {
    fn with(files: &[(&str, &[u8])]) -> SameBucket {
        SameBucket {
            inner: Racing::with(files),
            copies: AtomicU32::new(0),
        }
    }
}

impl Drive for SameBucket {
    fn site(&self) -> crate::xfer::Site {
        crate::xfer::Site::bucket("https://s3.test", "bucket")
    }
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.inner.get(key)?;
        let end = range
            .end
            .map_or(bytes.len() - 1, |end| (end as usize).min(bytes.len() - 1));
        Ok(bytes[range.start as usize..=end].to_vec())
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.copies.fetch_add(1, Ordering::SeqCst);
        let bytes = self.inner.get(from)?;
        self.inner.write(to, &bytes);
        Ok(())
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.inner.put_from(key, body)
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
        self.inner.put_if(key, bytes, condition)
    }
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_from_if(key, body, condition)
    }
}

fn big_file() -> Vec<u8> {
    vec![9u8; crate::transfer::CHUNK as usize + 4096]
}

/// X-DELTA-SYNC2-9. A Replace of a big object inside one bucket was a server-side copy after one
/// early look: the service's CopyObject asks no condition of the name it writes, and while it copies
/// (the bigger the object, the longer) another device could save that name and lose its file. A big
/// file that replaces the version the plan saw goes through the streamed copy now, whose last
/// request carries the condition: Conflict, the other device's file stays, and the bucket's own copy
/// is not used. RED on the old code: the copy is made (`copies` is 1) and nothing is refused.
#[test]
fn x_delta_sync2_9_a_big_file_replaces_a_name_in_its_own_bucket_only_through_the_condition() {
    let big = big_file();
    let bucket = SameBucket::with(&[("in/a.bin", &big), ("out/a.bin", b"the old version")]);
    let mut plan = make_plan(&bucket, &[file("in/a.bin")], &bucket, "out/", false, "Copy").unwrap();
    assert!(plan.same_drive, "one bucket: the shortcut is on offer");
    assert_eq!(plan.conflicts().len(), 1);
    plan.choose(0, Choice::Replace);
    bucket
        .inner
        .save_meanwhile("out/a.bin", b"saved by another device");
    let report = go(&plan, &bucket, &bucket, false);
    assert!(conflict(&report, "in/a.bin"), "{report:?}");
    assert_eq!(
        bucket.copies.load(Ordering::SeqCst),
        0,
        "the bucket's own copy asks no condition: it is not used for this"
    );
    assert_eq!(
        bucket.get("out/a.bin").unwrap(),
        b"saved by another device",
        "the other device's file stays"
    );
}

/// The ordinary case of the same replace: nothing was saved meanwhile, the version the plan saw is
/// replaced - by the streamed copy - and the bytes are the new file's.
#[test]
fn x_delta_sync2_9_a_big_replace_of_the_version_the_plan_saw_goes_through() {
    let big = big_file();
    let bucket = SameBucket::with(&[("in/a.bin", &big), ("out/a.bin", b"the old version")]);
    let mut plan = make_plan(&bucket, &[file("in/a.bin")], &bucket, "out/", false, "Copy").unwrap();
    plan.choose(0, Choice::Replace);
    let report = go(&plan, &bucket, &bucket, false);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.done, 1);
    assert_eq!(bucket.get("out/a.bin").unwrap(), big);
}

/// What keeps the shortcut: a small file that replaces a name (the service copies it in no time) and
/// a big file to a free name (nothing to replace).
#[test]
fn x_delta_sync2_9_a_small_replace_and_a_big_copy_to_a_free_name_keep_the_bucket_copy() {
    let big = big_file();
    let bucket = SameBucket::with(&[
        ("in/small.txt", b"small"),
        ("out/small.txt", b"the old small"),
        ("in/big.bin", &big),
    ]);
    let mut plan = make_plan(
        &bucket,
        &[file("in/small.txt"), file("in/big.bin")],
        &bucket,
        "out/",
        false,
        "Copy",
    )
    .unwrap();
    assert_eq!(plan.conflicts().len(), 1, "only the small name is taken");
    plan.choose(plan.conflicts()[0], Choice::Replace);
    let report = go(&plan, &bucket, &bucket, false);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(
        bucket.copies.load(Ordering::SeqCst),
        2,
        "both went by the bucket's own copy"
    );
    assert_eq!(bucket.get("out/small.txt").unwrap(), b"small");
    assert_eq!(bucket.get("out/big.bin").unwrap(), big);
}

/// X-DELTA-SYNC2-3: a drive whose look says "empty" of an object it listed as big - one that does not
/// tell its sizes says 0 - must not make an empty file the finished copy: the ranges were taken by the
/// size of the look, and a size of nothing is a copy of nothing.
#[test]
fn x_delta_sync2_3_a_look_that_says_empty_of_a_big_object_is_not_a_copy_of_nothing() {
    let (v1, _) = two_versions();
    let source = Swapping::with(&[("big.bin", &v1)]);
    let target = Swapping::default();
    let plan = make_plan(&source, &[file("big.bin")], &target, "", true, "Copy").unwrap();
    // The plan was made while the drive still told its size; the run's look says 0.
    source.head_says_empty.store(true, Ordering::SeqCst);
    let report = go(&plan, &source, &target, true);
    assert_eq!(report.done, 0, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(
        matches!(report.failed[0].1, DriveError::Protocol(_)),
        "{report:?}"
    );
    assert!(
        target.get("big.bin").is_err(),
        "no empty file at the target"
    );
    assert_eq!(source.get("big.bin").unwrap(), v1, "the source stays");
}
