//! A folder synced to a drive BY NAME ([`super::Target::Named`]): every file under its own key
//! (the prefix and its path), the drive's own index naming it. An encrypted drive keeps names,
//! sizes and the BLAKE3 of each file in its metadata repository, so the bucket holds no index
//! of the sync - only ciphertext under random names - and the drive browses as usual.
//!
//! - **The drive's state** is its listing under the prefix (an encrypted drive answers from its
//!   index, without a listing of the bucket): each file's hash is its BLAKE3 as the drive
//!   names it ([`azul_storage::CONTENT_HASH_METADATA`]), else the one this device knows for
//!   that version (it wrote or read it: a memo beside the local index), else a stand-in that
//!   matches no content (`version:<tag>`: the file changed there).
//! - **An upload** writes the file itself, conditional on the version the run read
//!   (`If-Match`; `If-None-Match` for a new name): a file another device changed meanwhile is
//!   never overwritten - the upload waits, and the next run sees both changes (a conflict).
//! - **A delete** first checks the version is still the one read: an edit beats a delete.
//! - **The commit** is the files themselves; the run's marker is a hash of the names and
//!   versions, and the generation counts this device's commits.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use azul_storage::{ops, Drive, DriveError, Precondition, CONTENT_HASH_METADATA};
use serde::{Deserialize, Serialize};

use super::{
    blob_job, job_bytes, local, lock, remote, uploaded_hash, Action, RemoteFile, RemoteIndex,
    SyncEvent, SyncRemote, SyncReport, Uploads,
};
use crate::{
    error::{fail, CloudResult},
    state::{read_json, write_json},
};

/// What this device knows of the drive's versions, kept beside the local index.
#[derive(Default, Serialize, Deserialize)]
struct Memo {
    /// key -> (entity tag, BLAKE3 of that version's content).
    #[serde(default)]
    versions: BTreeMap<String, (String, String)>,
    /// This device's commits so far (the drive side's generation).
    #[serde(default)]
    generation: u64,
}

/// The drive side of a folder synced by name.
pub(super) struct NamedFiles<'d> {
    drive: &'d dyn Drive,
    prefix: String,
    memo_path: PathBuf,
    memo: Mutex<Memo>,
    dirty: AtomicBool,
    /// The versions the last read saw: key -> entity tag (an upload's condition).
    seen: Mutex<BTreeMap<String, String>>,
}

/// An entity tag as the drives compare it: without its quotes.
fn bare(etag: &str) -> String {
    etag.trim_matches('"').to_string()
}

/// The run's marker of an index: a hash of its names and versions.
fn marker_of(index: &RemoteIndex) -> String {
    let mut hasher = blake3::Hasher::new();
    for (key, file) in &index.files {
        hasher.update(key.as_bytes());
        hasher.update(b"\t");
        hasher.update(file.hash.as_bytes());
        hasher.update(b"\n");
    }
    let hex = hasher.finalize().to_hex();
    hex.as_str()[..32].to_string()
}

impl<'d> NamedFiles<'d> {
    /// `drive`'s files under `prefix`, for the folder whose local index is `index_path`.
    pub(super) fn new(drive: &'d dyn Drive, prefix: &str, index_path: &Path) -> NamedFiles<'d> {
        let memo_path = index_path.with_extension("named.json");
        let memo = read_json::<Memo>(&memo_path).ok().flatten().unwrap_or_default();
        NamedFiles {
            drive,
            prefix: prefix.to_string(),
            memo_path,
            memo: Mutex::new(memo),
            dirty: AtomicBool::new(false),
            seen: Mutex::new(BTreeMap::new()),
        }
    }

    fn full(&self, key: &str) -> String {
        format!("{}{key}", self.prefix)
    }

    /// The content hash of `key` at version `etag`.
    fn hash_of(&self, key: &str, etag: &str, memo: &mut Memo) -> String {
        if !etag.is_empty() {
            if let Some((tag, hash)) = memo.versions.get(key) {
                if tag == etag {
                    return hash.clone();
                }
            }
        }
        // The drive may name it (an encrypted drive's index keeps it).
        if let Ok(rows) = self.drive.metadata(&self.full(key)) {
            let named = rows
                .into_iter()
                .find(|(name, value)| name == CONTENT_HASH_METADATA && remote::is_hash(value));
            if let Some((_, hash)) = named {
                if !etag.is_empty() {
                    memo.versions
                        .insert(key.to_string(), (etag.to_string(), hash.clone()));
                    self.dirty.store(true, Ordering::SeqCst);
                }
                return hash;
            }
        }
        format!("version:{etag}")
    }

    /// This device knows `key`'s version `etag` holds `hash`.
    fn remember(&self, key: &str, etag: &str, hash: &str) {
        if etag.is_empty() {
            return;
        }
        lock(&self.memo)
            .versions
            .insert(key.to_string(), (etag.to_string(), hash.to_string()));
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// The tag of `key`'s version on the drive now (`""`: the drive says none).
    fn tag_now(&self, key: &str) -> Result<Option<String>, DriveError> {
        match self.drive.head(&self.full(key)) {
            Ok(info) => Ok(Some(info.etag.as_deref().map(bare).unwrap_or_default())),
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Drop for NamedFiles<'_> {
    fn drop(&mut self) {
        if self.dirty.load(Ordering::SeqCst) {
            let _ = write_json(&self.memo_path, &*lock(&self.memo), false);
        }
    }
}

impl SyncRemote for NamedFiles<'_> {
    fn read(
        &self,
        _cached: Option<&(String, RemoteIndex)>,
    ) -> CloudResult<(RemoteIndex, Option<String>)> {
        let objects = ops::list_all(self.drive, &self.prefix)?;
        let mut memo = lock(&self.memo);
        let mut index = RemoteIndex::empty();
        index.generation = memo.generation;
        let mut seen = BTreeMap::new();
        for object in objects {
            let Some(key) = object.key.strip_prefix(&self.prefix) else {
                continue;
            };
            // Folder markers, and the bookkeeping of a plain sync under the same prefix.
            if key.is_empty()
                || key.ends_with('/')
                || key.split('/').any(|segment| segment == remote::META_DIR)
            {
                continue;
            }
            // A name that cannot be a file here (`..`, a backslash) is never synced.
            if remote::check_key(key).is_err() {
                continue;
            }
            let etag = object.etag.as_deref().map(bare).unwrap_or_default();
            let hash = self.hash_of(key, &etag, &mut memo);
            index.files.insert(
                key.to_string(),
                RemoteFile {
                    hash,
                    size: object.size,
                    mtime: object.modified.and_then(|m| i64::try_from(m).ok()).unwrap_or(0),
                    gen: memo.generation,
                    device: String::new(),
                },
            );
            seen.insert(key.to_string(), etag);
        }
        let before = memo.versions.len();
        memo.versions.retain(|key, _| seen.contains_key(key));
        if memo.versions.len() != before {
            self.dirty.store(true, Ordering::SeqCst);
        }
        drop(memo);
        *lock(&self.seen) = seen;
        let marker = marker_of(&index);
        Ok((index, Some(marker)))
    }

    fn fetch(&self, key: &str, hash: &str, _size: u64) -> CloudResult<Vec<u8>> {
        let bytes = self.drive.get(&self.full(key))?;
        let got = local::hash_bytes(&bytes);
        if remote::is_hash(hash) {
            if got != hash {
                fail!("{key} changed on the drive while it was fetched; the next run takes it");
            }
            return Ok(bytes);
        }
        // A version whose content was unknown: known now, if the drive still has it.
        let seen = lock(&self.seen).get(key).cloned();
        if let (Some(seen), Ok(Some(now))) = (seen, self.tag_now(key)) {
            if seen == now {
                self.remember(key, &now, &got);
            }
        }
        Ok(bytes)
    }

    fn fetch_hash(&self, hash: &str, _size: u64) -> CloudResult<Vec<u8>> {
        let key = lock(&self.memo)
            .versions
            .iter()
            .find(|(_, (_, h))| h == hash)
            .map(|(key, _)| key.clone());
        match key {
            Some(key) => {
                let bytes = self.drive.get(&self.full(&key))?;
                if local::hash_bytes(&bytes) != hash {
                    fail!("the version {hash} of {key} is gone from the drive");
                }
                Ok(bytes)
            }
            None => fail!("the drive keeps no file with the content {hash} any more"),
        }
    }

    fn upload(
        &self,
        work: &Uploads<'_>,
        report: &mut SyncReport,
    ) -> CloudResult<BTreeSet<String>> {
        let seen = lock(&self.seen).clone();
        let mut abandoned = BTreeSet::new();
        // One file at a time: each write is a commit of the drive's index.
        for action in work.actions {
            let Some(hash) = uploaded_hash(action, work.scan, work.merged) else {
                continue;
            };
            let key = action.key().to_string();
            let target = match action {
                Action::Conflict { copy, .. } => copy.clone(),
                _ => key.clone(),
            };
            if work.hooks.cancelled() {
                abandoned.insert(key);
                continue;
            }
            let job = blob_job(action, work, hash);
            work.hooks.tell(SyncEvent::Started {
                key: key.clone(),
                up: true,
                bytes: job.size,
            });
            let finished = |error: Option<String>| {
                work.hooks.tell(SyncEvent::Finished {
                    key: key.clone(),
                    up: true,
                    bytes: job.size,
                    error,
                });
            };
            let bytes = match job_bytes(&job) {
                Ok(bytes) => bytes,
                Err(why) => {
                    report.changed_during_sync.push(format!("{key}: {why}"));
                    finished(Some(why));
                    abandoned.insert(key);
                    continue;
                }
            };
            let condition = match seen.get(&target) {
                Some(etag) if !etag.is_empty() => Precondition::Matches(etag.clone()),
                _ => Precondition::Absent,
            };
            match self.drive.put_if(&self.full(&target), &bytes, &condition) {
                Ok(etag) => {
                    let etag = match etag {
                        Some(etag) => bare(&etag),
                        None => self.tag_now(&target).ok().flatten().unwrap_or_default(),
                    };
                    self.remember(&target, &etag, hash);
                    report.blobs_up += 1;
                    report.bytes_up += bytes.len() as u64;
                    finished(None);
                }
                Err(DriveError::Conflict { .. }) => {
                    let why = String::from(
                        "changed on the drive while it was synced; the next run compares them",
                    );
                    report.changed_during_sync.push(format!("{key}: {why}"));
                    finished(Some(why));
                    abandoned.insert(key);
                }
                Err(e) => {
                    finished(Some(e.to_string()));
                    return Err(e.into());
                }
            }
        }
        Ok(abandoned)
    }

    fn commit(
        &self,
        remote: &RemoteIndex,
        next: &RemoteIndex,
        _etag: Option<&str>,
        report: &mut SyncReport,
    ) -> CloudResult<Option<String>> {
        let seen = lock(&self.seen).clone();
        for key in remote.files.keys().filter(|key| !next.files.contains_key(*key)) {
            // A delete never takes a version this run did not see: an edit beats a delete.
            match self.tag_now(key)? {
                None => {}
                Some(now) if seen.get(key).is_some_and(|tag| *tag == now) => {
                    self.drive.delete(&self.full(key))?;
                    lock(&self.memo).versions.remove(key);
                    self.dirty.store(true, Ordering::SeqCst);
                }
                Some(_) => report.notes.push(format!(
                    "{key} changed on the drive before it was deleted; it stays there"
                )),
            }
        }
        lock(&self.memo).generation = next.generation;
        self.dirty.store(true, Ordering::SeqCst);
        Ok(Some(marker_of(next)))
    }
}
