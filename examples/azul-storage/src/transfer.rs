//! Moving ONE object between a drive and a local file: what "Download", "Open"
//! and "Upload" do in AzDrive, and what an export does in AzMail.
//!
//! - A download of more than one chunk fetches [`PARALLEL_RANGES`] ranges at once into a
//!   hidden file next to the destination (`.<name>.download.part`, its state beside it), and
//!   renames it into place when it is whole and the object did not change meanwhile. A download
//!   that failed half way resumes with the ranges it still lacks - when the object is still the
//!   same version (its ETag, else its size and date); otherwise it starts over.
//! - An upload sends the file itself ([`Drive::put_file`]: a bucket sends its parts several at
//!   once and resumes an upload a killed app left behind).
//! - A copy between two drives that are not folders here streams ranged GETs into one
//!   [`Drive::put_from`]: no object passes through memory whole.

use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, PoisonError},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    beneath::{self, Dir},
    cap::{Cap, Posture},
    ctl::{Ctl, Quiet, Stopped},
    durable::{self, Blob, Expect, Mode, Publish},
    key::safe_file_name,
    local::{beneath_error, source_bits},
    multipart::{in_parallel, STALE_AFTER_SECS},
    quarantine, ByteRange, Drive, DriveError, ObjectInfo, Precondition,
};

/// What the look at the disk's room before a big download begins does when the disk cannot say
/// its free space (a network share, a FUSE mount, a sandbox): the download begins, a full disk says
/// so at a write, and the report says that the look was left out ([`crate::cap::take_unchecked`]).
const DOWNLOAD_ROOM_POSTURE: Posture = Posture::Degrade(
    "the free space of the disk cannot be read: the download begins without the look at its room",
);

/// Bytes per ranged GET when an object is bigger than this (the part size of an upload).
pub const CHUNK: u64 = crate::s3::PART_SIZE as u64;
/// Ranged GETs of one download in flight at once.
pub const PARALLEL_RANGES: usize = crate::s3::PARALLEL_PARTS;

/// A reader that tells `ctl` the bytes read so far after every read and stops, with
/// [`Stopped::io`] (which `?` turns into [`DriveError::Cancelled`]), once `ctl` says so: a
/// stream's upload progress and its cancel ([`Drive::put_file`]'s default).
pub struct ProgressReader<'a> {
    inner: &'a mut dyn Read,
    read: u64,
    ctl: &'a dyn Ctl,
}

impl<'a> ProgressReader<'a> {
    pub fn new(inner: &'a mut dyn Read, ctl: &'a dyn Ctl) -> Self {
        ProgressReader {
            inner,
            read: 0,
            ctl,
        }
    }
}

impl Read for ProgressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.ctl.cancelled() {
            return Err(Stopped::io());
        }
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.read += n as u64;
            self.ctl.bytes(self.read);
        }
        Ok(n)
    }
}

/// A reader that asks `ctl` before every read and stops, with [`Stopped::io`], once it says so; it
/// reports no bytes (the progress of the phase it belongs to was told already). The upload of a
/// spool that was filled while the progress was told, the encryption of a file into its copy: long
/// phases a stop used to go unheard in (X-DELTA-AZDRIVE-FILES-13).
#[cfg(feature = "encryption")]
pub(crate) struct StopReader<'a> {
    inner: &'a mut dyn Read,
    ctl: &'a dyn Ctl,
}

#[cfg(feature = "encryption")]
impl<'a> StopReader<'a> {
    pub(crate) fn new(inner: &'a mut dyn Read, ctl: &'a dyn Ctl) -> Self {
        StopReader { inner, ctl }
    }
}

#[cfg(feature = "encryption")]
impl Read for StopReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.ctl.cancelled() {
            return Err(Stopped::io());
        }
        self.inner.read(buf)
    }
}

/// [`DriveError::Cancelled`] when `ctl` says stop.
fn stop_if_asked(ctl: &dyn Ctl) -> Result<(), DriveError> {
    if ctl.cancelled() {
        Err(DriveError::Cancelled)
    } else {
        Ok(())
    }
}

/// What `work` hears on its thread of its own: the progress goes to the thread that waits for it,
/// and the stop is the caller's.
struct Relay<'a> {
    sender: Mutex<mpsc::Sender<u64>>,
    stop: &'a dyn Ctl,
}

impl Ctl for Relay<'_> {
    fn bytes(&self, done: u64) {
        let _ = self
            .sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(done);
        self.stop.bytes(done);
    }

    fn cancelled(&self) -> bool {
        self.stop.cancelled()
    }
}

/// Runs `work` on a thread of its own with a [`Ctl`] any thread may call; what it hears reaches
/// `progress` on this thread, as it comes (never a smaller number than before). The `Ctl` never
/// cancels: a caller that wants a stop passes its own `Ctl` to the `_with_ctl` function.
fn relay_progress<T: Send>(
    progress: &mut dyn FnMut(u64),
    work: impl FnOnce(&dyn Ctl) -> T + Send,
) -> T {
    relay_progress_with(progress, &Quiet, work)
}

/// [`relay_progress`] whose `Ctl` also stops when `stop` says so, and tells `stop` the bytes it
/// hears (the bytes of the one object being copied).
fn relay_progress_with<T: Send>(
    progress: &mut dyn FnMut(u64),
    stop: &dyn Ctl,
    work: impl FnOnce(&dyn Ctl) -> T + Send,
) -> T {
    let (sender, heard) = mpsc::channel::<u64>();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let relay = Relay {
                sender: Mutex::new(sender),
                stop,
            };
            work(&relay)
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

/// A hidden file beside `dest`, named `.<name>.<tail>`. A name longer than the stem allows is
/// cut and given a hash of the whole, so that the hidden name is a valid name too (255 bytes)
/// and two long names that start alike do not share a partial file (AZ-W19).
fn hidden_sibling(dest: &Path, tail: &str) -> PathBuf {
    /// The longest stem of a hidden name: with a dot, a hash and the longest tail it stays far
    /// below the 255 bytes a file name may have.
    const STEM_MAX: usize = 160;
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = if name.len() <= STEM_MAX {
        name
    } else {
        let mut cut = String::new();
        for c in name.chars() {
            if cut.len() + c.len_utf8() > STEM_MAX - 9 {
                break;
            }
            cut.push(c);
        }
        let hash = blake3::hash(name.as_bytes()).to_hex();
        format!("{cut}~{}", &hash.as_str()[..8])
    };
    dest.with_file_name(format!(".{stem}.{tail}"))
}

/// The hidden file a download of `dest` keeps its bytes in, and its state file.
fn partial_paths(dest: &Path) -> (PathBuf, PathBuf) {
    // Both end in `.part`: a sync never takes them (its base rules leave `*.part` out).
    (
        hidden_sibling(dest, "download.part"),
        hidden_sibling(dest, "download-state.part"),
    )
}

/// The folder of `dest` (made when missing; links followed: it is the user's own) and the file's
/// name in it.
fn folder_of(dest: &Path, key: &str) -> Result<(Dir, String), DriveError> {
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DriveError::InvalidKey {
            key: key.to_string(),
            reason: "it has a name that cannot be a file name",
        })?;
    let dir = Dir::open_following(parent)?;
    sweep_once(&dir, parent);
    Ok((dir, name.to_string()))
}

/// Removes, once per folder and process, what writes that were cut off left in it
/// ([`durable::sweep_temps`]): the hidden temporary files and partial downloads a failed or
/// stopped copy of an earlier run left in the user's folder, which no listing shows and nothing
/// else removes (AZ-U15).
fn sweep_once(dir: &Dir, folder: &Path) {
    static SWEPT: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);
    let first = SWEPT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashSet::new)
        .insert(folder.to_path_buf());
    if first {
        durable::sweep_temps(dir, Duration::from_secs(STALE_AFTER_SECS));
    }
}

/// The object as `head` says, keyed as asked (a drive may answer with another key form).
fn info_of(drive: &dyn Drive, key: &str) -> Result<ObjectInfo, DriveError> {
    let mut info = drive.head(key)?;
    info.key = key.to_string();
    Ok(info)
}

/// Fetches the ranges of `info`'s object into its hidden file next to `dest` - the ones a
/// failed download of the same version left there are kept - several at once, then puts it in
/// place when the object did not change meanwhile ([`durable::publish_staged`]: synced to the
/// disk, renamed, the folder synced).
fn download_ranges(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    expect: Expect,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    download_ranges_probed(drive, info, dest, chunk, expect, ctl, &durable::Unprobed)
}

/// [`download_ranges`] with a look at the step that makes a range durable (a test cuts it by
/// failing it).
pub(crate) fn download_ranges_probed(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    expect: Expect,
    ctl: &dyn Ctl,
    probe: &(dyn durable::Probe + Sync),
) -> Result<u64, DriveError> {
    let key = info.key.as_str();
    let size = info.size;
    let (partial, state_path) = partial_paths(dest);
    let (folder, dest_name) = folder_of(dest, key)?;
    let partial_name = partial
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let kept = fs::read(&state_path)
        .ok()
        .and_then(|text| serde_json::from_slice::<DownloadState>(&text).ok())
        .filter(|state| state.same_version(info, chunk))
        .filter(|_| fs::metadata(&partial).is_ok_and(|m| m.len() == size));
    let state = match kept {
        Some(state) => state,
        None => {
            // The room first: a hidden file as long as the object is made next, and an object
            // that says it is bigger than the disk (a lying listing, a huge file) must not make
            // a sparse file of that length nor a list of its ranges.
            let room = match (beneath::free_space(folder.path()), DOWNLOAD_ROOM_POSTURE) {
                (Cap::Known(free), _) => Some(free),
                (Cap::Unknown(why), Posture::Degrade(note)) => {
                    crate::cap::note_unchecked(&format!("{note} ({why})"));
                    None
                }
                (Cap::Unknown(why), Posture::FailClosed) => {
                    return Err(DriveError::Io(format!(
                        "{why}: \"{key}\" is not downloaded without knowing it fits"
                    )))
                }
            };
            if let Some(free) = room.filter(|free| *free < size) {
                return Err(DriveError::Io(format!(
                    "no space left on the disk of {} for \"{key}\": it needs {size} bytes, {free} \
                     are free",
                    dest.display()
                )));
            }
            // A new file of its own, as long as the object (sparse until its ranges are in).
            let _ = folder.unlink(&partial_name);
            let file = folder
                .create_new(&partial_name, 0o666)
                .map_err(|e| beneath_error(key, e))?;
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
    ctl.bytes(have);
    let fetched = std::sync::atomic::AtomicU64::new(have);
    let state = Mutex::new(state);
    let result = in_parallel(missing.len(), PARALLEL_RANGES, |n| {
        // A stop is looked for before every range: the ranges in flight finish, none starts.
        stop_if_asked(ctl)?;
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
        // The range is on the disk BEFORE the state says it is done: the state file's rename can
        // reach the disk before the delayed writes of this file's pages (nothing orders two
        // files), and the retry after a power cut would skip the range as done and publish the
        // hole of zeros as the download - and a Move would then delete the cloud original, the
        // only intact copy (X-DELTA-AZDRIVE-FILES-5). `flush` of a `File` does nothing.
        probe.before(durable::Step::SyncFile)?;
        file.sync_data()?;
        {
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            state.done.insert(i);
            if let Ok(text) = serde_json::to_vec(&*state) {
                // A hint for the next try (it checks the version and the file's length): a lost
                // one costs the ranges, never a wrong file - the data it names is durable.
                let _ = durable::replace_cache(&state_path, &text);
            }
        }
        ctl.bytes(fetched.fetch_add(len, std::sync::atomic::Ordering::SeqCst) + len);
        Ok::<(), DriveError>(())
    });
    // A failure, or a stop, keeps the hidden file and its state: the next download resumes (what
    // no download resumes is swept with the folder's other leftovers, `sweep_once`).
    result?;
    stop_if_asked(ctl)?;
    // The ranges must all be of one version.
    let now = info_of(drive, key)?;
    if now.etag != info.etag || now.size != info.size || now.modified != info.modified {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&state_path);
        return Err(DriveError::Conflict {
            key: key.to_string(),
        });
    }
    let blob = Blob {
        dir: &folder,
        name: &partial_name,
    };
    durable::publish_staged(
        &folder,
        &dest_name,
        blob,
        Publish::new(expect, Mode::Shared, &Quiet),
    )
    .map_err(|e| e.into_drive(key))?;
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
    download_with_ctl(drive, key, size, dest, chunk, &Quiet)
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
    relay_progress(progress, |ctl| {
        download_with_ctl(drive, key, size, dest, chunk, ctl)
    })
}

/// [`download_to_file`] that tells `ctl` the bytes written so far (from the threads the ranges
/// travel on) and stops, with [`DriveError::Cancelled`], once `ctl` says so: before the next
/// range of a big object, which keeps what is in its hidden file for the next download, or
/// before the one GET of a small one, which leaves nothing.
pub fn download_with_ctl(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    download_guarded(drive, key, size, dest, chunk, Expect::Any, ctl)
}

/// [`download_with_ctl`] that puts the file in place only if the name still holds what `expect`
/// says ([`copy_object_guarded`]): a download that was resumed, or that took minutes, does not
/// replace a file made meanwhile.
pub fn download_guarded(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
    expect: Expect,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    stop_if_asked(ctl)?;
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
    if size <= chunk {
        let bytes = drive.get(key)?;
        stop_if_asked(ctl)?;
        let (folder, name) = folder_of(dest, key)?;
        durable::publish(
            &folder,
            &name,
            &mut &bytes[..],
            Publish::new(expect, Mode::Shared, &Quiet),
        )
        .map_err(|e| e.into_drive(key))?;
        ctl.bytes(bytes.len() as u64);
        return Ok(bytes.len() as u64);
    }
    // The version (ETag, date) a resumed download must still find.
    let info = match info {
        Some(info) => info,
        None => info_of(drive, key)?,
    };
    download_ranges(drive, &info, dest, chunk, expect, ctl)
}

/// Where a download of `key` goes in `folder`: the key's file name made safe
/// ([`crate::key::safe_file_name`]), with ` (1)`, ` (2)`, ... before the
/// extension when that name is taken. `None` when the key has no usable name.
///
/// The answer is a look, not a claim: the name can be taken between this call and the file's
/// landing (X-AR5a-9). Put the file there with [`download_guarded`] and [`Expect::Absent`], which
/// refuses to replace what appeared meanwhile.
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
    drive.put_file(key, path, &Quiet)
}

/// [`upload_file`] that tells `progress` the bytes sent so far.
pub fn upload_with_progress(
    drive: &dyn Drive,
    path: &Path,
    key: &str,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    relay_progress(progress, |ctl| drive.put_file(key, path, ctl))
}

/// [`upload_file`] that tells `ctl` the bytes sent so far and stops, with
/// [`DriveError::Cancelled`] and nothing left behind, once `ctl` says so ([`Drive::put_file`]).
pub fn upload_with_ctl(
    drive: &dyn Drive,
    path: &Path,
    key: &str,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    drive.put_file(key, path, ctl)
}

/// Copies the file `source` to `dest` through a temporary file next to it, synced and renamed
/// in ([`durable::publish`]), telling `ctl` the bytes copied so far after every chunk and
/// stopping, with the target as it was and no temporary file, once `ctl` says so. The copy has
/// the file's permission bits (never setuid, setgid or sticky) and its modification time: a
/// 0600 key stays private and a photo keeps its date.
fn copy_file(source: &Path, dest: &Path, expect: Expect, ctl: &dyn Ctl) -> Result<u64, DriveError> {
    let mut from = File::open(source)?;
    let (mode, mtime) = source_bits(&from);
    let shown = dest.to_string_lossy();
    let (folder, name) = folder_of(dest, &shown)?;
    let mut publish = Publish::new(expect, mode, ctl);
    if let Some(mtime) = mtime {
        publish = publish.with_mtime(mtime);
    }
    durable::publish(&folder, &name, &mut from, publish)
        .map(|landed| landed.len)
        .map_err(|e| e.into_drive(&shown))
}

/// What the name at the target must hold when a copied file is put in place there, asked in the
/// step that makes the file visible - after the hours a big copy may have taken, not at its start
/// (X-DELTA-AZDRIVE-FILES-3).
#[derive(Debug)]
pub struct When<'a> {
    /// For a folder on this computer: checked by the rename that puts the file in place.
    pub expect: Expect,
    /// For a drive that [writes conditionally](Drive::conditional_writes): checked by the request
    /// that completes the object (a bucket) or the index change that names it (an encrypted
    /// drive). A drive that cannot is not asked: the caller looks at its name first.
    pub condition: Option<&'a Precondition>,
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
    relay_progress(progress, |ctl| {
        copy_object_with_ctl(source, key, size, target, target_key, ctl)
    })
}

/// [`copy_object_guarded`] that tells `progress` the bytes of this object copied so far, on this
/// thread, as they come - and stops, with [`DriveError::Cancelled`], once `stop` says so (and
/// hears the same bytes, from the thread that moves them).
pub fn copy_object_relayed(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    when: When<'_>,
    stop: &dyn Ctl,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    relay_progress_with(progress, stop, |ctl| {
        copy_object_when(source, key, size, target, target_key, when, ctl)
    })
}

/// [`copy_object`] that tells `ctl` the bytes copied so far (from whichever thread moved them)
/// and stops, with [`DriveError::Cancelled`], once `ctl` says so: a file copy on disk removes its
/// temporary file and leaves the target as it was, a bucket upload is aborted, ranged GETs stop
/// before the next range. Nothing is made visible by a stopped copy.
pub fn copy_object_with_ctl(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    copy_object_guarded(source, key, size, target, target_key, Expect::Any, ctl)
}

/// [`copy_object_with_ctl`] that puts the file in place on a disk only if the name still holds
/// what `expect` says ([`Expect::Absent`]: a name made meanwhile is not replaced;
/// [`Expect::Is`]: only the version the plan saw is), in the one step that renames it in -
/// [`DriveError::Conflict`] and nothing written when it does not. A bucket target has no such step
/// here (the guard of a drive that asks its condition is [`copy_object_when`]'s).
pub fn copy_object_guarded(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    expect: Expect,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    let when = When {
        expect,
        condition: None,
    };
    copy_object_when(source, key, size, target, target_key, when, ctl)
}

/// [`copy_object_guarded`] that also asks the target drive its condition in the step that makes
/// the object visible, when the drive can ([`Drive::conditional_writes`]): a big file into a
/// bucket or an encrypted drive was looked at once, at its start, and written unconditionally an
/// hour later - over a name another device made meanwhile (X-DELTA-AZDRIVE-FILES-3). Now the
/// request that completes the upload asks, and a name that no longer holds what the plan saw is a
/// [`DriveError::Conflict`] with the target as it was. A target that cannot ask is written as
/// before (its caller looked first).
pub fn copy_object_when(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    when: When<'_>,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    stop_if_asked(ctl)?;
    let expect = when.expect;
    // Only a drive that can ask is asked: any other would read a big body for nothing.
    let condition = when.condition.filter(|_| target.conditional_writes());
    match (source.local_path(key), target.local_path(target_key)) {
        (Some(from), Some(to)) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            copy_file(&from, &to, expect, ctl)
        }
        (None, Some(to)) => {
            let len = download_guarded(source, key, size, &to, CHUNK, expect, ctl)?;
            // A file that came down from a drive carries the system's "downloaded from elsewhere"
            // mark, so the system looks at an `.exe`, a script or a shortcut of a shared drive
            // before it runs it (AZ-W20) - whoever calls this: a Download, a Paste, the rescue
            // copy of a read-only drive (X-DELTA-AZDRIVE-FILES-19). A copy from one folder of this
            // disk to another is no download and is not marked.
            quarantine::mark_downloaded(&to);
            Ok(len)
        }
        (Some(from), None) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            match condition {
                Some(condition) => {
                    let len = fs::metadata(&from)?.len();
                    target
                        .put_file_if(target_key, &from, condition, ctl)
                        .map(|_| len)
                }
                None => target.put_file(target_key, &from, ctl),
            }
        }
        (None, None) => {
            let size = match size {
                Some(size) => size,
                None => source.head(key)?.size,
            };
            if size <= CHUNK {
                let bytes = source.get(key)?;
                stop_if_asked(ctl)?;
                match condition {
                    Some(condition) => target.put_if(target_key, &bytes, condition).map(|_| ())?,
                    None => target.put(target_key, &bytes)?,
                }
                ctl.bytes(bytes.len() as u64);
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
            let mut reader = ProgressReader::new(&mut ranges, ctl);
            match condition {
                Some(condition) => target
                    .put_from_if(target_key, &mut reader, condition)
                    .map(|_| size),
                None => target.put_from(target_key, &mut reader),
            }
        }
    }
}
