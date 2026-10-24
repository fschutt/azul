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
//!   Small files go in batches ([`Drive::put_many`]): an encrypted drive packs them into shared
//!   objects with one index commit per pack, a plain one writes them one after the other. Large
//!   files go several at once ([`Drive::put_from_many`]), a batch named in one commit.
//! - **A move:** a new file whose content is a file this run deletes there (a folder renamed
//!   or moved here) is moved on the drive in one [`Drive::move_many`]: an encrypted drive
//!   changes its index once, nothing is uploaded again.
//! - **A delete** first checks the version is still the one read: an edit beats a delete. A
//!   run's deletes go in one [`Drive::delete_many`]: one commit of an encrypted drive's index,
//!   1,000 keys a request on a bucket.
//! - **The commit** is the files themselves; the run's marker is a hash of the names and
//!   versions, and the generation counts this device's commits.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use azul_storage::{
    ops, BatchDelete, BatchMove, BatchPut, BatchStream, Drive, DriveError, Precondition,
    CONTENT_HASH_METADATA, SMALL_FILE_BYTES,
};
use serde::{Deserialize, Serialize};

use super::{
    blob_job, job_bytes, local, lock, open_job_file, remote, uploaded_hash, Action, BlobJob,
    BlobSource,
    CheckedRead, RemoteFile, RemoteIndex, SyncEvent, SyncRemote, SyncReport, Uploads,
};
use crate::{
    error::{fail, CloudError, CloudResult},
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
    /// The drive keys (without the prefix) this run moved away: their delete is done already.
    moved: Mutex<BTreeSet<String>>,
    /// Whether an upload of this run reached the drive, and the first that failed by itself
    /// (no connection, a busy node): when none went through the run fails with it, as the
    /// index sync's does - the drive cannot be reached, which "files cannot sync" would hide.
    sent_any: AtomicBool,
    transient: Mutex<Option<CloudError>>,
}

/// Large files a batch takes before it is sent (and named in one commit of an encrypted
/// drive's index): enough to keep several uploads in flight, few enough open files.
const LARGE_BATCH_FILES: usize = 16;
/// The large files' bytes a batch takes before it is sent: a crash before its commit sends at
/// most this much again.
const LARGE_BATCH_BYTES: u64 = 1 << 30;

/// A file the run sends: its key and target, what the target must hold, its content's hash.
struct Sending {
    /// The file's key in the folder (what the report and the events name).
    key: String,
    /// Where it goes in the folder: the key, or a conflict copy's name.
    target: String,
    /// What the drive must hold at the target: absent, or the version this run read.
    condition: Precondition,
    /// The BLAKE3 of what goes up.
    hash: String,
    /// Where its bytes come from.
    job: BlobJob,
}

/// A large file's bytes on their way up.
enum LargeBody {
    /// From its file, checked against its hash as it is read: a file that changed meanwhile
    /// fails its read, so its upload is never completed.
    Streamed(CheckedRead),
    /// Read whole (a merged JSON object) and checked already.
    Whole(Cursor<Vec<u8>>),
}

impl LargeBody {
    /// Whether the file changed while it was read.
    fn changed(&self) -> bool {
        matches!(self, LargeBody::Streamed(reader) if reader.changed())
    }

    /// The bytes read from the file, when it was streamed.
    fn bytes(&self) -> Option<u64> {
        match self {
            LargeBody::Streamed(reader) => Some(reader.bytes()),
            LargeBody::Whole(_) => None,
        }
    }
}

impl Read for LargeBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            LargeBody::Streamed(reader) => reader.read(buf),
            LargeBody::Whole(bytes) => bytes.read(buf),
        }
    }
}

/// A small file read for the next batch.
struct Batched {
    /// The file's key in the folder (what the report and the events name).
    key: String,
    /// Where it goes in the folder: the key, or a conflict copy's name.
    target: String,
    condition: Precondition,
    hash: String,
    bytes: Vec<u8>,
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
            moved: Mutex::new(BTreeSet::new()),
            sent_any: AtomicBool::new(false),
            transient: Mutex::new(None),
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
        remote::version_stand_in(etag)
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

    /// Writes `batch` in one [`Drive::put_many`], then tells each file's outcome as the single
    /// upload does; the first error other than a lost condition stops the run, after every file
    /// of the batch was told.
    fn send_batch(
        &self,
        batch: Vec<Batched>,
        work: &Uploads<'_>,
        report: &mut SyncReport,
        abandoned: &mut BTreeSet<String>,
    ) -> CloudResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        if work.hooks.cancelled() {
            // Stopped before the batch went: none of it is written, the next run sends it.
            for b in batch {
                tell_finished(work, &b.key, b.bytes.len() as u64, Some(String::from("stopped")));
                abandoned.insert(b.key);
            }
            return Ok(());
        }
        let fulls: Vec<String> = batch.iter().map(|b| self.full(&b.target)).collect();
        let items: Vec<BatchPut<'_>> = batch
            .iter()
            .zip(&fulls)
            .map(|(b, full)| BatchPut::new(full, &b.bytes).when(&b.condition))
            .collect();
        let results = self.drive.put_many(&items);
        let mut stop = None;
        for (b, result) in batch.iter().zip(results) {
            let bytes = b.bytes.len() as u64;
            match result {
                Ok(etag) => {
                    self.sent(&b.target, etag, &b.hash);
                    report.blobs_up += 1;
                    report.bytes_up += bytes;
                    tell_finished(work, &b.key, bytes, None);
                    work.hooks.point("after-blob");
                }
                Err(DriveError::Conflict { .. }) => {
                    lost_race(work, report, abandoned, &b.key, bytes);
                }
                Err(e) => {
                    tell_finished(work, &b.key, bytes, Some(e.to_string()));
                    let e = CloudError::from(e);
                    if !super::stops_the_run(&e) {
                        super::listed_upload_failure(report, &b.key, &e);
                        abandoned.insert(b.key.clone());
                        if super::passes(&e) {
                            self.passing(e);
                        }
                    } else {
                        stop.get_or_insert(e);
                    }
                }
            }
        }
        match stop {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Writes the large files of `batch` in one [`Drive::put_from_many`] (several at once; an
    /// encrypted drive names them in one commit), each streamed from its file and checked
    /// against its hash as it is read: one that changed meanwhile waits for the next run.
    fn send_large(
        &self,
        batch: Vec<(Sending, LargeBody)>,
        work: &Uploads<'_>,
        report: &mut SyncReport,
        abandoned: &mut BTreeSet<String>,
    ) -> CloudResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        if work.hooks.cancelled() {
            for (sending, _) in batch {
                tell_finished(work, &sending.key, sending.job.size, Some(String::from("stopped")));
                abandoned.insert(sending.key);
            }
            return Ok(());
        }
        let (sendings, mut bodies): (Vec<Sending>, Vec<LargeBody>) = batch.into_iter().unzip();
        let fulls: Vec<String> = sendings.iter().map(|s| self.full(&s.target)).collect();
        let results = {
            let mut items: Vec<BatchStream<'_>> = sendings
                .iter()
                .zip(&fulls)
                .zip(bodies.iter_mut())
                .map(|((sending, full), body)| {
                    BatchStream::new(full, body as &mut (dyn Read + Send)).when(&sending.condition)
                })
                .collect();
            self.drive.put_from_many(&mut items)
        };
        let mut stop = None;
        for ((sending, body), result) in sendings.iter().zip(&bodies).zip(results) {
            if body.changed() {
                give_up(
                    work,
                    report,
                    abandoned,
                    sending,
                    String::from("changed while it was synced"),
                );
                continue;
            }
            match result {
                Ok(etag) => {
                    self.sent(&sending.target, etag, &sending.hash);
                    report.blobs_up += 1;
                    report.bytes_up += body.bytes().unwrap_or(sending.job.size);
                    tell_finished(work, &sending.key, sending.job.size, None);
                    work.hooks.point("after-blob");
                }
                Err(DriveError::Conflict { .. }) => {
                    lost_race(work, report, abandoned, &sending.key, sending.job.size);
                }
                Err(e) => {
                    tell_finished(work, &sending.key, sending.job.size, Some(e.to_string()));
                    let e = CloudError::from(e);
                    if !super::stops_the_run(&e) {
                        super::listed_upload_failure(report, &sending.key, &e);
                        abandoned.insert(sending.key.clone());
                        if super::passes(&e) {
                            self.passing(e);
                        }
                    } else {
                        stop.get_or_insert(e);
                    }
                }
            }
        }
        match stop {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Moves on the drive the files this run found moved here (the same content under a new
    /// name, its old name going): one [`Drive::move_many`] - an encrypted drive changes its
    /// index once and moves no object, a bucket copies on the service. The ones that could not
    /// move (the old name changed meanwhile, the new one is taken) come back to be sent.
    fn send_moves(
        &self,
        moves: Vec<(Sending, String)>,
        work: &Uploads<'_>,
        report: &mut SyncReport,
        seen: &BTreeMap<String, String>,
        abandoned: &mut BTreeSet<String>,
    ) -> Vec<Sending> {
        if moves.is_empty() {
            return Vec::new();
        }
        if work.hooks.cancelled() {
            for (sending, _) in moves {
                tell_finished(work, &sending.key, sending.job.size, Some(String::from("stopped")));
                abandoned.insert(sending.key);
            }
            return Vec::new();
        }
        let paths: Vec<(String, String)> = moves
            .iter()
            .map(|(sending, from)| (self.full(from), self.full(&sending.target)))
            .collect();
        let items: Vec<BatchMove<'_>> = moves
            .iter()
            .zip(&paths)
            .map(|((_, from), (old, new))| {
                let item = BatchMove::new(old, new);
                match seen.get(from) {
                    Some(tag) if !tag.is_empty() => item.of_version(tag),
                    _ => item,
                }
            })
            .collect();
        let results = self.drive.move_many(&items);
        let mut unmoved = Vec::new();
        for ((sending, from), result) in moves.into_iter().zip(results) {
            if result.is_err() {
                unmoved.push(sending);
                continue;
            }
            lock(&self.moved).insert(from.clone());
            lock(&self.memo).versions.remove(&from);
            self.dirty.store(true, Ordering::SeqCst);
            self.sent(&sending.target, None, &sending.hash);
            report.blobs_reused += 1;
            tell_finished(work, &sending.key, sending.job.size, None);
        }
        unmoved
    }

    /// An upload failed by itself (it waits for the next run, listed): the first such error.
    fn passing(&self, e: CloudError) {
        lock(&self.transient).get_or_insert(e);
    }

    /// `target` now holds `hash` on the drive, at the version `etag` (asked of the drive when
    /// the write did not say).
    fn sent(&self, target: &str, etag: Option<String>, hash: &str) {
        self.sent_any.store(true, Ordering::SeqCst);
        let etag = match etag {
            Some(etag) => bare(&etag),
            None => self.tag_now(target).ok().flatten().unwrap_or_default(),
        };
        self.remember(target, &etag, hash);
    }
}

impl Drop for NamedFiles<'_> {
    fn drop(&mut self) {
        if self.dirty.load(Ordering::SeqCst) {
            let _ = write_json(&self.memo_path, &*lock(&self.memo), true);
        }
    }
}

impl SyncRemote for NamedFiles<'_> {
    fn names_versions(&self) -> bool {
        true
    }

    /// The drive's index names a file's content, not its permission bits.
    fn tracks_exec(&self) -> bool {
        false
    }

    fn read(
        &self,
        _cached: Option<&(String, RemoteIndex)>,
    ) -> CloudResult<(RemoteIndex, Option<String>)> {
        // The drive as it is now: a pass must see what another device committed a moment ago.
        let objects = ops::list_all_fresh(self.drive, &self.prefix)?;
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
                    mtime_ns: 0,
                    gen: memo.generation,
                    device: String::new(),
                    exec: false,
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

    /// Into the file `dest` in ranges (never whole in memory), checked like [`Self::fetch`].
    fn fetch_to(&self, key: &str, hash: &str, size: u64, dest: &Path) -> CloudResult<String> {
        self.fetch_to_with(key, hash, size, dest, &|| {})
    }

    fn fetch_to_with(
        &self,
        key: &str,
        hash: &str,
        size: u64,
        dest: &Path,
        mid: &dyn Fn(),
    ) -> CloudResult<String> {
        super::drive_store::download_told(self.drive, &self.full(key), size, dest, mid)?;
        let got = local::hash_file(dest)?;
        if remote::is_hash(hash) {
            if got != hash {
                let _ = std::fs::remove_file(dest);
                fail!("{key} changed on the drive while it was fetched; the next run takes it");
            }
            return Ok(got);
        }
        let seen = lock(&self.seen).get(key).cloned();
        if let (Some(seen), Ok(Some(now))) = (seen, self.tag_now(key)) {
            if seen == now {
                self.remember(key, &now, &got);
            }
        }
        Ok(got)
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
        // The files this run deletes on the drive, by content: a new file here with the same
        // bytes is the same file moved (a folder renamed here), moved on the drive, never sent.
        let mut leaving: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for action in work.actions {
            if let Action::DeleteRemote { key } = action {
                if let Some(file) = work.remote.files.get(key) {
                    if remote::is_hash(&file.hash) {
                        leaving.entry(file.hash.as_str()).or_default().push(key.as_str());
                    }
                }
            }
        }
        let mut sends: Vec<Sending> = Vec::new();
        let mut moves: Vec<(Sending, String)> = Vec::new();
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
            let condition = match seen.get(&target) {
                Some(etag) if !etag.is_empty() => Precondition::Matches(etag.clone()),
                _ => Precondition::Absent,
            };
            let sending = Sending {
                key,
                target,
                condition,
                hash: hash.to_string(),
                job,
            };
            let from = match action {
                Action::Upload { .. } => leaving.get_mut(hash).and_then(Vec::pop),
                _ => None,
            };
            match from {
                Some(from) => moves.push((sending, from.to_string())),
                None => sends.push(sending),
            }
        }
        // The moves first, in one batch: what could not move is sent as usual.
        sends.extend(self.send_moves(moves, work, report, &seen, &mut abandoned));
        // Small files together, a batch at a time (an encrypted drive commits its index once per
        // pack); large ones streamed from their files, several at once, a batch named in one
        // commit.
        let mut small: Vec<Batched> = Vec::new();
        let mut small_bytes = 0u64;
        let mut large: Vec<(Sending, LargeBody)> = Vec::new();
        let mut large_bytes = 0u64;
        for sending in sends {
            work.hooks.point("mid-read");
            // Small files are never streamed (that starts at BIG_BLOB): read whole, sent together.
            if sending.job.size < SMALL_FILE_BYTES {
                match job_bytes(&sending.job) {
                    Ok(bytes) => {
                        small_bytes += bytes.len() as u64;
                        small.push(Batched {
                            key: sending.key,
                            target: sending.target,
                            condition: sending.condition,
                            hash: sending.hash,
                            bytes,
                        });
                        if small_bytes >= azul_storage::SMALL_BATCH_BYTES {
                            let full = std::mem::take(&mut small);
                            self.send_batch(full, work, report, &mut abandoned)?;
                            small_bytes = 0;
                        }
                    }
                    Err(why) => give_up(work, report, &mut abandoned, &sending, why),
                }
                continue;
            }
            match large_body(&sending.job) {
                Ok(body) => {
                    large_bytes += sending.job.size;
                    large.push((sending, body));
                    if large.len() >= LARGE_BATCH_FILES || large_bytes >= LARGE_BATCH_BYTES {
                        let full = std::mem::take(&mut large);
                        self.send_large(full, work, report, &mut abandoned)?;
                        large_bytes = 0;
                    }
                }
                Err(why) => give_up(work, report, &mut abandoned, &sending, why),
            }
        }
        self.send_batch(small, work, report, &mut abandoned)?;
        self.send_large(large, work, report, &mut abandoned)?;
        // Nothing went through and something failed by itself: the drive cannot be reached.
        if !self.sent_any.load(Ordering::SeqCst) {
            if let Some(e) = lock(&self.transient).take() {
                return Err(e);
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
        let moved = lock(&self.moved).clone();
        // Every delete in one batch (an encrypted drive: one commit of its index), each only
        // while the file is the version this run read: an edit beats a delete. A file this run
        // moved left its old name already.
        let gone: Vec<&String> = remote
            .files
            .keys()
            .filter(|key| !next.files.contains_key(*key) && !moved.contains(*key))
            .collect();
        let fulls: Vec<String> = gone.iter().map(|key| self.full(key)).collect();
        let items: Vec<BatchDelete<'_>> = gone
            .iter()
            .zip(&fulls)
            .map(|(key, full)| match seen.get(*key) {
                Some(tag) if !tag.is_empty() => BatchDelete::new(full).of_version(tag),
                _ => BatchDelete::new(full),
            })
            .collect();
        let results = self.drive.delete_many(&items);
        let mut stop = None;
        for (key, result) in gone.iter().zip(results) {
            match result {
                Ok(()) => {
                    lock(&self.memo).versions.remove(*key);
                    self.dirty.store(true, Ordering::SeqCst);
                }
                Err(DriveError::Conflict { .. }) => report.notes.push(format!(
                    "{key} changed on the drive before it was deleted; it stays there"
                )),
                Err(e) => {
                    stop.get_or_insert(e);
                }
            }
        }
        if let Some(e) = stop {
            return Err(e.into());
        }
        lock(&self.memo).generation = next.generation;
        self.dirty.store(true, Ordering::SeqCst);
        Ok(Some(marker_of(next)))
    }
}

/// Tells the app a file of the run arrived (`error`: or why not).
fn tell_finished(work: &Uploads<'_>, key: &str, bytes: u64, error: Option<String>) {
    work.hooks.tell(SyncEvent::Finished {
        key: key.to_string(),
        up: true,
        bytes,
        error,
    });
}

/// A file that cannot go this run (`why`): told, and left for the next one.
fn give_up(
    work: &Uploads<'_>,
    report: &mut SyncReport,
    abandoned: &mut BTreeSet<String>,
    sending: &Sending,
    why: String,
) {
    report
        .changed_during_sync
        .push(format!("{}: {why}", sending.key));
    tell_finished(work, &sending.key, sending.job.size, Some(why));
    abandoned.insert(sending.key.clone());
}

/// A file another device changed while this one sent it: the next run compares them.
fn lost_race(
    work: &Uploads<'_>,
    report: &mut SyncReport,
    abandoned: &mut BTreeSet<String>,
    key: &str,
    bytes: u64,
) {
    let why = String::from("changed on the drive while it was synced; the next run compares them");
    report.changed_during_sync.push(format!("{key}: {why}"));
    tell_finished(work, key, bytes, Some(why));
    abandoned.insert(key.to_string());
}

/// A large file's reader: streamed from its file and checked against its hash as it is read
/// (a file on disk), or a merged JSON object read whole and checked already.
fn large_body(job: &BlobJob) -> Result<LargeBody, String> {
    match &job.source {
        // Reached without a link and checked to be the scanned file, never opened by its path.
        BlobSource::File(_) => open_job_file(job)
            .map(|(file, _)| LargeBody::Streamed(CheckedRead::new(file, &job.hash))),
        _ => job_bytes(job).map(|bytes| LargeBody::Whole(Cursor::new(bytes))),
    }
}
