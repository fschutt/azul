//! Moving ONE object between a drive and a local file: what "Download", "Open"
//! and "Upload" do in AzDrive, and what an export does in AzMail.
//!
//! - A download of more than one chunk fetches [`PARALLEL_RANGES`] ranges at once into a
//!   hidden file next to the destination (`.<name>.azdownload`, with its state beside it), and
//!   renames it into place when it is whole and the object did not change meanwhile. A download
//!   that failed half way resumes with the ranges it still lacks - when the object is still the
//!   same version (its ETag, else its size and date); otherwise it starts over.
//! - An upload sends the file itself ([`Drive::put_file`]: a bucket sends its parts several at
//!   once and resumes an upload a killed app left behind).
//! - A copy between two drives that are not folders here streams ranged GETs into one
//!   [`Drive::put_from`]: no object passes through memory whole.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, PoisonError},
};

use serde::{Deserialize, Serialize};

use crate::{
    key::safe_file_name,
    local::{temp_sibling, write_atomically},
    multipart::in_parallel,
    ByteRange, Drive, DriveError, ObjectInfo,
};

/// Bytes per ranged GET when an object is bigger than this (the part size of an upload).
pub const CHUNK: u64 = crate::s3::PART_SIZE as u64;
/// Ranged GETs of one download in flight at once.
pub const PARALLEL_RANGES: usize = crate::s3::PARALLEL_PARTS;

/// A reader that tells `progress` the bytes read so far after every read: a stream's upload
/// progress ([`Drive::put_file`]'s default).
pub struct ProgressReader<'a> {
    inner: &'a mut dyn Read,
    read: u64,
    progress: &'a (dyn Fn(u64) + Sync),
}

impl<'a> ProgressReader<'a> {
    pub fn new(inner: &'a mut dyn Read, progress: &'a (dyn Fn(u64) + Sync)) -> Self {
        ProgressReader {
            inner,
            read: 0,
            progress,
        }
    }
}

impl Read for ProgressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.read += n as u64;
            (self.progress)(self.read);
        }
        Ok(n)
    }
}

/// Runs `work` on a thread of its own with a progress callback any thread may call; what it
/// hears reaches `progress` on this thread, as it comes (never a smaller number than before).
fn relay_progress<T: Send>(
    progress: &mut dyn FnMut(u64),
    work: impl FnOnce(&(dyn Fn(u64) + Sync)) -> T + Send,
) -> T {
    let (sender, heard) = mpsc::channel::<u64>();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let sender = Mutex::new(sender);
            let tell = |n: u64| {
                let _ = sender
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .send(n);
            };
            work(&tell)
        });
        let mut told = None;
        for n in heard {
            if told.is_none_or(|before| n > before) {
                told = Some(n);
                progress(n);
            }
        }
        match worker.join() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

/// A ranged reader of one object: what a copy between two remote drives streams from.
struct RangeReader<'a> {
    drive: &'a dyn Drive,
    key: &'a str,
    size: u64,
    chunk: u64,
    offset: u64,
    buffer: Vec<u8>,
    at: usize,
}

impl Read for RangeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at >= self.buffer.len() {
            if self.offset >= self.size {
                return Ok(0);
            }
            let end = (self.offset + self.chunk).min(self.size) - 1;
            let bytes = self
                .drive
                .get_range(self.key, ByteRange::new(self.offset, Some(end)))
                .map_err(std::io::Error::other)?;
            if bytes.is_empty() {
                return Err(std::io::Error::other(format!(
                    "\"{}\" ended at byte {} of {}",
                    self.key, self.offset, self.size
                )));
            }
            self.offset += bytes.len() as u64;
            self.buffer = bytes;
            self.at = 0;
        }
        let n = buf.len().min(self.buffer.len() - self.at);
        buf[..n].copy_from_slice(&self.buffer[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// What a download kept of an object so far (next to its hidden file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DownloadState {
    key: String,
    size: u64,
    etag: Option<String>,
    modified: Option<u64>,
    chunk: u64,
    /// The chunks in the hidden file, by index.
    done: BTreeSet<u64>,
}

impl DownloadState {
    /// Whether the object is still the version this state was kept of.
    fn same_version(&self, info: &ObjectInfo, chunk: u64) -> bool {
        let identified = self.etag.is_some() || self.modified.is_some();
        identified
            && self.key == info.key
            && self.size == info.size
            && self.etag == info.etag
            && self.modified == info.modified
            && self.chunk == chunk
    }
}

/// The hidden file a download of `dest` keeps its bytes in, and its state file.
fn partial_paths(dest: &Path) -> (PathBuf, PathBuf) {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (
        dest.with_file_name(format!(".{name}.azdownload")),
        dest.with_file_name(format!(".{name}.azdownload.json")),
    )
}

/// The object as `head` says, keyed as asked (a drive may answer with another key form).
fn info_of(drive: &dyn Drive, key: &str) -> Result<ObjectInfo, DriveError> {
    let mut info = drive.head(key)?;
    info.key = key.to_string();
    Ok(info)
}

/// Fetches the ranges of `info`'s object into its hidden file next to `dest` - the ones a
/// failed download of the same version left there are kept - several at once, then renames
/// it into place when the object did not change meanwhile.
fn download_ranges(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    progress: &(dyn Fn(u64) + Sync),
) -> Result<u64, DriveError> {
    let key = info.key.as_str();
    let size = info.size;
    let (partial, state_path) = partial_paths(dest);
    let kept = fs::read(&state_path)
        .ok()
        .and_then(|text| serde_json::from_slice::<DownloadState>(&text).ok())
        .filter(|state| state.same_version(info, chunk))
        .filter(|_| fs::metadata(&partial).is_ok_and(|m| m.len() == size));
    let state = match kept {
        Some(state) => state,
        None => {
            let file = File::create(&partial)?;
            file.set_len(size)?;
            let _ = fs::remove_file(&state_path);
            DownloadState {
                key: key.to_string(),
                size,
                etag: info.etag.clone(),
                modified: info.modified,
                chunk,
                done: BTreeSet::new(),
            }
        }
    };
    let count = size.div_ceil(chunk);
    let missing: Vec<u64> = (0..count).filter(|i| !state.done.contains(i)).collect();
    let length = |i: u64| chunk.min(size - i * chunk);
    let have: u64 = state.done.iter().map(|i| length(*i)).sum();
    progress(have);
    let fetched = std::sync::atomic::AtomicU64::new(have);
    let state = Mutex::new(state);
    let result = in_parallel(missing.len(), PARALLEL_RANGES, |n| {
        let i = missing[n];
        let start = i * chunk;
        let len = length(i);
        let bytes = drive.get_range(key, ByteRange::new(start, Some(start + len - 1)))?;
        if bytes.len() as u64 != len {
            return Err(DriveError::Protocol(format!(
                "\"{key}\": the range {start}-{} came back with {} bytes",
                start + len - 1,
                bytes.len()
            )));
        }
        let mut file = OpenOptions::new().write(true).open(&partial)?;
        file.seek(SeekFrom::Start(start))?;
        file.write_all(&bytes)?;
        file.flush()?;
        {
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            state.done.insert(i);
            if let Ok(text) = serde_json::to_vec(&*state) {
                let _ = write_atomically(&state_path, &text);
            }
        }
        progress(fetched.fetch_add(len, std::sync::atomic::Ordering::SeqCst) + len);
        Ok::<(), DriveError>(())
    });
    // A failure keeps the hidden file and its state: the next download resumes.
    result?;
    // The ranges must all be of one version.
    let now = info_of(drive, key)?;
    if now.etag != info.etag || now.size != info.size || now.modified != info.modified {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&state_path);
        return Err(DriveError::Conflict {
            key: key.to_string(),
        });
    }
    fs::rename(&partial, dest)?;
    let _ = fs::remove_file(&state_path);
    Ok(size)
}

/// Downloads `key` to `dest`: one GET when it fits in `chunk` bytes, ranged GETs
/// of `chunk` bytes otherwise, several at once, so a big object never sits in memory whole.
/// `size` comes from the listing; `None` asks with a HEAD first. A small object's bytes go to
/// a temporary file next to `dest`, renamed when complete, so a failed download leaves nothing
/// behind; a big one's go to a hidden file next to it that a failed download leaves for the
/// next one to resume (see the module). Returns the bytes written.
pub fn download_to_file(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
) -> Result<u64, DriveError> {
    download_with_progress(drive, key, size, dest, chunk, &mut |_| {})
}

/// [`download_to_file`] that tells `progress` the bytes written so far after
/// every chunk (once, at the end, for an object that fits in one GET).
pub fn download_with_progress(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    let chunk = chunk.max(1);
    let mut info = None;
    let size = match size {
        Some(size) => size,
        None => {
            let head = info_of(drive, key)?;
            let size = head.size;
            info = Some(head);
            size
        }
    };
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    if size <= chunk {
        let bytes = drive.get(key)?;
        write_atomically(dest, &bytes)?;
        progress(bytes.len() as u64);
        return Ok(bytes.len() as u64);
    }
    // The version (ETag, date) a resumed download must still find.
    let info = match info {
        Some(info) => info,
        None => info_of(drive, key)?,
    };
    relay_progress(progress, |tell| {
        download_ranges(drive, &info, dest, chunk, tell)
    })
}

/// Where a download of `key` goes in `folder`: the key's file name made safe
/// ([`crate::key::safe_file_name`]), with ` (1)`, ` (2)`, ... before the
/// extension when that name is taken. `None` when the key has no usable name.
#[must_use]
pub fn download_path(folder: &Path, key: &str) -> Option<PathBuf> {
    let name = safe_file_name(key)?;
    let first = folder.join(&name);
    if !first.exists() {
        return Some(first);
    }
    let (stem, extension) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    (1..10_000)
        .map(|n| folder.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
}

/// Puts the local file `path` as `key` ([`Drive::put_file`]: a bucket sends a big file's
/// parts several at once and resumes an upload a killed app left behind). Returns the bytes
/// sent.
pub fn upload_file(drive: &dyn Drive, path: &Path, key: &str) -> Result<u64, DriveError> {
    drive.put_file(key, path, &|_| {})
}

/// [`upload_file`] that tells `progress` the bytes sent so far.
pub fn upload_with_progress(
    drive: &dyn Drive,
    path: &Path,
    key: &str,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    relay_progress(progress, |tell| drive.put_file(key, path, tell))
}

/// Bytes per read when a file is copied on disk.
const DISK_CHUNK: usize = 1024 * 1024;

/// Copies the file `source` to `dest` through a temporary file next to it,
/// telling `progress` the bytes copied so far after every megabyte.
fn copy_file(source: &Path, dest: &Path, progress: &mut dyn FnMut(u64)) -> Result<u64, DriveError> {
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_sibling(dest);
    let copied = (|| -> Result<u64, DriveError> {
        let mut from = File::open(source)?;
        let mut to = File::create(&tmp)?;
        let mut buffer = vec![0u8; DISK_CHUNK];
        let mut total = 0u64;
        loop {
            let read = from.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            to.write_all(&buffer[..read])?;
            total += read as u64;
            progress(total);
        }
        to.flush()?;
        drop(to);
        fs::rename(&tmp, dest)?;
        Ok(total)
    })();
    if copied.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    copied
}

/// A reader that tells `progress` (on the reading thread) the bytes read so far.
struct TellingReader<'a> {
    inner: &'a mut dyn Read,
    read: u64,
    progress: &'a mut dyn FnMut(u64),
}

impl Read for TellingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.read += n as u64;
            (self.progress)(self.read);
        }
        Ok(n)
    }
}

/// Copies ONE object from `source` to `target` (the same drive or another):
/// a file copy on disk when both are folders on this computer; ranged GETs of
/// [`CHUNK`] bytes, several at once, into the target's file when only the target
/// is; the file itself ([`Drive::put_file`]) when only the source is; otherwise
/// one GET and one PUT for an object of one chunk or less, and for a bigger one
/// ranged GETs streamed into one [`Drive::put_from`] - never the whole object in
/// memory. `size` comes from the listing; `None` asks with a HEAD when it matters.
/// `progress` hears the bytes copied so far. Returns the bytes copied. Overwrites
/// `target_key`: conflicts are the caller's.
pub fn copy_object(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    match (source.local_path(key), target.local_path(target_key)) {
        (Some(from), Some(to)) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            copy_file(&from, &to, progress)
        }
        (None, Some(to)) => download_with_progress(source, key, size, &to, CHUNK, progress),
        (Some(from), None) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            relay_progress(progress, |tell| target.put_file(target_key, &from, tell))
        }
        (None, None) => {
            let size = match size {
                Some(size) => size,
                None => source.head(key)?.size,
            };
            if size <= CHUNK {
                let bytes = source.get(key)?;
                target.put(target_key, &bytes)?;
                progress(bytes.len() as u64);
                return Ok(bytes.len() as u64);
            }
            let mut ranges = RangeReader {
                drive: source,
                key,
                size,
                chunk: CHUNK,
                offset: 0,
                buffer: Vec::new(),
                at: 0,
            };
            let mut reader = TellingReader {
                inner: &mut ranges,
                read: 0,
                progress,
            };
            target.put_from(target_key, &mut reader)
        }
    }
}
