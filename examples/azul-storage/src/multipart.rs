//! Big uploads to a bucket ([`S3Drive`]): multipart uploads in parts of [`PART_SIZE`],
//! [`PARALLEL_PARTS`] in flight at once.
//!
//! - A streamed body ([`Drive::put_from`], [`Drive::put_from_if`]) is read a few parts ahead
//!   (at most [`PARALLEL_PARTS`] in memory), and read to its end before the upload is completed:
//!   a reader that checks what it read (a sync's BLAKE3) stops a wrong upload by failing at its
//!   end. A body of one part or less is one PUT. A failed upload is aborted: a reader cannot be
//!   read again, so nothing could resume it.
//! - A local file ([`Drive::put_file`]) is read part by part where each part lies, and its
//!   upload outlives the app: with a resume folder ([`S3Drive::with_resume_dir`], or the app's
//!   [`set_resume_folder`]) a state file per upload keeps the upload id, the parts' ETags, and
//!   the file's size, date and the BLAKE3 of its first MiB. The next upload of the same file to
//!   the same key asks the service which parts it holds (ListParts) and sends only the missing
//!   ones. An upload that is stale ([`STALE_AFTER_SECS`]) or whose file changed is aborted and
//!   started again - it is never completed with parts of two versions - and so is one whose
//!   file changed while it was sent. [`S3Drive::abort_stale_uploads`] sweeps the ones nobody
//!   resumes.
//! - A conditional upload (`If-None-Match: *`, `If-Match`) asks its condition when it is
//!   completed (S3 checks it on CompleteMultipartUpload): a 412 is [`DriveError::Conflict`],
//!   and its parts are aborted.
//!
//! Blocking, like every drive call; the parts travel on threads of their own.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex, MutexGuard, PoisonError, RwLock,
    },
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

pub use crate::s3::{PARALLEL_PARTS, PART_SIZE};
use crate::{local::write_atomically, s3::S3Drive, DriveError, Precondition};

/// An unfinished upload older than this is not resumed: aborted and started again (buckets
/// commonly drop unfinished uploads after a week).
pub const STALE_AFTER_SECS: u64 = 6 * 24 * 3600;
/// How much of a file's start its state file checks by BLAKE3 (with its size and date).
pub const PREFIX_CHECK_BYTES: u64 = 1024 * 1024;
/// The most parts of one upload (S3's limit).
pub const MAX_PARTS: usize = 10_000;
/// The version of a state file this code writes.
const STATE_VERSION: u32 = 1;

/// The resume folder of every bucket that names none ([`set_resume_folder`]).
static RESUME_FOLDER: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Keeps the state files of resumable uploads in `folder` (the app's cache: AzDrive's
/// `<cache>/AzDrive/uploads`) for every bucket opened without a folder of its own; `None`:
/// uploads are not resumable.
pub fn set_resume_folder(folder: Option<PathBuf>) {
    *RESUME_FOLDER
        .write()
        .unwrap_or_else(PoisonError::into_inner) = folder;
}

/// The folder [`set_resume_folder`] set.
#[must_use]
pub fn resume_folder() -> Option<PathBuf> {
    RESUME_FOLDER
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `job(0)`, `job(1)`, ... `job(count - 1)` on up to `parallel` threads; the answers in
/// index order, or the first error (no job starts after one failed).
pub fn in_parallel<T: Send, E: Send>(
    count: usize,
    parallel: usize,
    job: impl Fn(usize) -> Result<T, E> + Sync,
) -> Result<Vec<T>, E> {
    let next = AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicBool::new(false);
    let slots: Mutex<Vec<Option<Result<T, E>>>> = Mutex::new((0..count).map(|_| None).collect());
    let workers = parallel.max(1).min(count.max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if failed.load(Ordering::SeqCst) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= count {
                    break;
                }
                let result = job(i);
                if result.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
                lock(&slots)[i] = Some(result);
            });
        }
    });
    // A slot stays empty only when a job failed: then that error is the answer.
    let mut out = Vec::with_capacity(count);
    let mut first_error = None;
    for slot in slots.into_inner().unwrap_or_else(PoisonError::into_inner) {
        match slot {
            Some(Ok(value)) => out.push(value),
            Some(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            None => {}
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// One part the service holds of an upload (ListParts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedPart {
    /// From 1.
    pub number: usize,
    /// As the service sent it (quotes included).
    pub etag: String,
    pub size: u64,
}

/// What a file looked like when its upload started.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    size: u64,
    /// Nanoseconds since 1970 (0 when the system does not say).
    modified_ns: u64,
    /// BLAKE3 of the first [`PREFIX_CHECK_BYTES`].
    prefix_blake3: String,
}

fn fingerprint(path: &Path) -> Result<Fingerprint, DriveError> {
    let meta = fs::metadata(path)?;
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let mut prefix = Vec::new();
    File::open(path)?
        .take(PREFIX_CHECK_BYTES)
        .read_to_end(&mut prefix)?;
    Ok(Fingerprint {
        size: meta.len(),
        modified_ns,
        prefix_blake3: blake3::hash(&prefix).to_hex().to_string(),
    })
}

/// The state file of one upload of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UploadState {
    version: u32,
    endpoint: String,
    bucket: String,
    key: String,
    /// The file, as this computer names it.
    source: String,
    size: u64,
    modified_ns: u64,
    prefix_blake3: String,
    part_size: u64,
    upload_id: String,
    /// When the upload started, in seconds since 1970.
    started_at: u64,
    /// The parts sent: number -> ETag (as the service sent it).
    parts: BTreeMap<usize, String>,
}

impl UploadState {
    fn fingerprint(&self) -> Fingerprint {
        Fingerprint {
            size: self.size,
            modified_ns: self.modified_ns,
            prefix_blake3: self.prefix_blake3.clone(),
        }
    }

    fn is_stale(&self, now: u64) -> bool {
        now.saturating_sub(self.started_at) > STALE_AFTER_SECS
    }
}

/// The state file of the upload of `source` to `key` of the drive's bucket.
fn state_path(folder: &Path, drive: &S3Drive, key: &str, source: &Path) -> PathBuf {
    let config = drive.config();
    let name = format!(
        "{}\n{}\n{}\n{}",
        config.endpoint,
        config.bucket,
        key,
        source.display()
    );
    let hash = blake3::hash(name.as_bytes()).to_hex();
    folder.join(format!("upload-{}.json", &hash.as_str()[..32]))
}

fn read_state(path: &Path) -> Option<UploadState> {
    let text = fs::read(path).ok()?;
    serde_json::from_slice::<UploadState>(&text)
        .ok()
        .filter(|state| state.version == STATE_VERSION)
}

/// Best effort: an upload whose state cannot be written is still sent, just not resumable.
fn write_state(path: &Path, state: &UploadState) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_vec_pretty(state) {
        let _ = write_atomically(path, &text);
    }
}

/// Whether `e` says the service no longer knows the upload.
fn upload_gone(e: &DriveError) -> bool {
    match e {
        DriveError::NotFound { .. } => true,
        DriveError::Service(service) => service.code == "NoSuchUpload" || service.status == 404,
        _ => false,
    }
}

/// Whether `e` says the service cannot list an upload's parts (then the state file is trusted).
fn cannot_list(e: &DriveError) -> bool {
    matches!(e, DriveError::Service(service)
        if service.status == 501 || service.status == 405 || service.code == "NotImplemented")
}

/// Up to `size` bytes of `body` (fewer only where it ends).
pub(crate) fn read_part(body: &mut dyn Read, size: usize) -> Result<Vec<u8>, DriveError> {
    let mut part = Vec::with_capacity(size.min(PART_SIZE));
    (&mut *body).take(size as u64).read_to_end(&mut part)?;
    Ok(part)
}

/// The bytes `[offset, offset + len)` of the file `path`.
fn read_at(path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, DriveError> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// The part numbers and their ETags, in order, as CompleteMultipartUpload names them.
fn numbered(parts: &BTreeMap<usize, String>) -> Vec<(usize, String)> {
    parts.iter().map(|(n, e)| (*n, e.clone())).collect()
}

/// Uploads `body` to `key`: one PUT (conditional with `condition`) when it is one part or less,
/// else a multipart upload of parts read a few ahead and sent several at once, completed (with
/// `condition`) after the body ended; aborted when anything fails. The bytes sent and the new
/// version's ETag, when the service said it.
pub(crate) fn upload_stream(
    drive: &S3Drive,
    key: &str,
    body: &mut dyn Read,
    condition: Option<&Precondition>,
) -> Result<(u64, Option<String>), DriveError> {
    let part_size = drive.part_size();
    let first = read_part(body, part_size)?;
    let second = if first.len() < part_size {
        Vec::new()
    } else {
        read_part(body, part_size)?
    };
    if second.is_empty() {
        let n = first.len() as u64;
        let etag = match condition {
            Some(condition) => crate::Drive::put_if(drive, key, &first, condition)?,
            None => drive.put_object(key, first)?,
        };
        return Ok((n, etag));
    }
    let upload = drive.start_multipart(key)?;
    match stream_parts(drive, key, &upload, vec![first, second], body, condition) {
        Ok(done) => Ok(done),
        Err(e) => {
            let _ = drive.abort_multipart(key, &upload);
            Err(e)
        }
    }
}

/// The parts of a streamed upload - `batch`, then what `body` still reads - a batch of up to
/// the drive's parallel parts at a time, then the completion.
fn stream_parts(
    drive: &S3Drive,
    key: &str,
    upload: &str,
    mut batch: Vec<Vec<u8>>,
    body: &mut dyn Read,
    condition: Option<&Precondition>,
) -> Result<(u64, Option<String>), DriveError> {
    let part_size = drive.part_size();
    let parallel = drive.parallel();
    let mut parts: BTreeMap<usize, String> = BTreeMap::new();
    let mut written = 0u64;
    let mut ended = false;
    loop {
        while !ended && batch.len() < parallel {
            let part = read_part(body, part_size)?;
            ended = part.len() < part_size;
            if !part.is_empty() {
                batch.push(part);
            }
        }
        if batch.is_empty() {
            break;
        }
        let first = parts.len() + 1;
        if first + batch.len() - 1 > MAX_PARTS {
            return Err(DriveError::Unsupported(format!(
                "{key}: an upload of more than {MAX_PARTS} parts of {part_size} bytes"
            )));
        }
        written += batch.iter().map(|p| p.len() as u64).sum::<u64>();
        let slots: Vec<Mutex<Option<Vec<u8>>>> =
            batch.drain(..).map(|p| Mutex::new(Some(p))).collect();
        let etags = in_parallel(slots.len(), parallel, |i| {
            let bytes = lock(&slots[i]).take().unwrap_or_default();
            drive.upload_part(key, upload, first + i, bytes)
        })?;
        for (i, etag) in etags.into_iter().enumerate() {
            parts.insert(first + i, etag);
        }
        if ended {
            // Read to its end: a checking reader has had its say before the completion.
            break;
        }
    }
    let etag = drive.complete_multipart(key, upload, &numbered(&parts), condition)?;
    Ok((written, etag))
}

/// Uploads the file `path` to `key` (see the module): one PUT for a file of one part or less;
/// otherwise a multipart upload that resumes from its state file in the drive's resume folder.
/// `progress` hears the bytes the service holds so far, from the parts' threads.
pub(crate) fn upload_file(
    drive: &S3Drive,
    key: &str,
    path: &Path,
    condition: Option<&Precondition>,
    progress: &(dyn Fn(u64) + Sync),
) -> Result<(u64, Option<String>), DriveError> {
    let part_size = drive.part_size() as u64;
    let before = fingerprint(path)?;
    let size = before.size;
    if size <= part_size {
        let bytes = fs::read(path)?;
        let etag = match condition {
            Some(condition) => crate::Drive::put_if(drive, key, &bytes, condition)?,
            None => drive.put_object(key, bytes)?,
        };
        progress(size);
        return Ok((size, etag));
    }
    let count = usize::try_from(size.div_ceil(part_size)).unwrap_or(usize::MAX);
    if count > MAX_PARTS {
        return Err(DriveError::Unsupported(format!(
            "{key}: an upload of more than {MAX_PARTS} parts of {part_size} bytes"
        )));
    }
    let folder = drive.resume_dir();
    let state_file = folder.as_deref().map(|f| state_path(f, drive, key, path));
    let now = drive.now();
    let mut state = None;
    if let Some(old) = state_file.as_deref().and_then(read_state) {
        let same = old.key == key
            && old.bucket == drive.config().bucket
            && old.part_size == part_size
            && old.fingerprint() == before;
        if same && !old.is_stale(now) {
            match drive.list_parts(key, &old.upload_id) {
                Ok(held) => {
                    // The parts the service holds, whole: what the state file names and what was
                    // sent before the state file was written.
                    let mut resumed = old.clone();
                    resumed.parts = held
                        .into_iter()
                        .filter(|p| p.number >= 1 && p.number <= count)
                        .filter(|p| {
                            let start = (p.number as u64 - 1) * part_size;
                            p.size == part_size.min(size - start)
                        })
                        .map(|p| (p.number, p.etag))
                        .collect();
                    state = Some(resumed);
                }
                Err(e) if cannot_list(&e) => state = Some(old),
                Err(e) if upload_gone(&e) => {}
                Err(e) => return Err(e),
            }
        } else {
            let _ = drive.abort_multipart(&old.key, &old.upload_id);
        }
        if state.is_none() {
            if let Some(file) = &state_file {
                let _ = fs::remove_file(file);
            }
        }
    }
    let state = match state {
        Some(state) => state,
        None => {
            let upload_id = drive.start_multipart(key)?;
            let state = UploadState {
                version: STATE_VERSION,
                endpoint: drive.config().endpoint.clone(),
                bucket: drive.config().bucket.clone(),
                key: key.to_string(),
                source: path.display().to_string(),
                size,
                modified_ns: before.modified_ns,
                prefix_blake3: before.prefix_blake3.clone(),
                part_size,
                upload_id,
                started_at: now,
                parts: BTreeMap::new(),
            };
            if let Some(file) = &state_file {
                write_state(file, &state);
            }
            state
        }
    };
    let upload_id = state.upload_id.clone();
    let forget = |state_file: &Option<PathBuf>| {
        if let Some(file) = state_file {
            let _ = fs::remove_file(file);
        }
    };
    let held: u64 = state
        .parts
        .keys()
        .map(|n| part_size.min(size - (*n as u64 - 1) * part_size))
        .sum();
    progress(held);
    let missing: Vec<usize> = (1..=count)
        .filter(|n| !state.parts.contains_key(n))
        .collect();
    let sent = AtomicU64::new(held);
    let shared = Mutex::new(state);
    let result = in_parallel(missing.len(), drive.parallel(), |i| {
        let number = missing[i];
        let start = (number as u64 - 1) * part_size;
        let len = part_size.min(size - start);
        let bytes = read_at(path, start, usize::try_from(len).unwrap_or(usize::MAX))?;
        let etag = drive.upload_part(key, &upload_id, number, bytes)?;
        {
            let mut state = lock(&shared);
            state.parts.insert(number, etag);
            if let Some(file) = &state_file {
                write_state(file, &state);
            }
        }
        progress(sent.fetch_add(len, Ordering::SeqCst) + len);
        Ok::<(), DriveError>(())
    });
    let state = shared.into_inner().unwrap_or_else(PoisonError::into_inner);
    if let Err(e) = result {
        // With a state file the next upload resumes; without one nothing could.
        if state_file.is_none() {
            let _ = drive.abort_multipart(key, &upload_id);
        }
        return Err(e);
    }
    // The file must still be the one whose parts were sent.
    if fingerprint(path)? != before {
        let _ = drive.abort_multipart(key, &upload_id);
        forget(&state_file);
        return Err(DriveError::Io(format!(
            "{} changed while it was uploaded; it is sent again next time",
            path.display()
        )));
    }
    match drive.complete_multipart(key, &upload_id, &numbered(&state.parts), condition) {
        Ok(etag) => {
            forget(&state_file);
            Ok((size, etag))
        }
        Err(e) => {
            if matches!(e, DriveError::Conflict { .. }) || state_file.is_none() {
                let _ = drive.abort_multipart(key, &upload_id);
                forget(&state_file);
            } else if upload_gone(&e) {
                forget(&state_file);
            }
            Err(e)
        }
    }
}

/// Aborts the unfinished uploads of `drive`'s bucket in its resume folder that will not be
/// resumed - stale, or their file is gone or changed - and forgets them; how many.
pub(crate) fn abort_stale(drive: &S3Drive) -> usize {
    let Some(folder) = drive.resume_dir() else {
        return 0;
    };
    let Ok(entries) = fs::read_dir(&folder) else {
        return 0;
    };
    let now = drive.now();
    let config = drive.config();
    let mut aborted = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(state) = read_state(&path) else {
            continue;
        };
        if state.endpoint != config.endpoint || state.bucket != config.bucket {
            continue;
        }
        let unchanged =
            fingerprint(Path::new(&state.source)).is_ok_and(|now| now == state.fingerprint());
        if unchanged && !state.is_stale(now) {
            continue;
        }
        match drive.abort_multipart(&state.key, &state.upload_id) {
            Ok(()) => {}
            Err(e) if upload_gone(&e) => {}
            // No answer: tried again at the next sweep.
            Err(_) => continue,
        }
        let _ = fs::remove_file(&path);
        aborted += 1;
    }
    aborted
}
