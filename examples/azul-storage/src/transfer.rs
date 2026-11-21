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
//!   [`Drive::put_from`]: no object passes through memory whole. The ranges are of one version of
//!   the object (each asks for it where the drive can, and the end of the copy is a look at the
//!   object - a second reading of it, for a drive that names no tag): a file another device saves
//!   meanwhile is a conflict, never a splice of two versions under the final name.

use std::{
    collections::{BTreeSet, HashSet},
    fmt,
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
    limits::Limit,
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
///
/// Every range is a request of its own, and nothing ties two of them to one version: an object
/// another device saves while the copy streams would arrive as a splice of two versions under the
/// final name (X-DELTA-SYNC2-3). The reader is of the version `before` - one look when the copy
/// began. Every range asks for that version where the drive can ask ([`Drive::get_range_if`]: S3's
/// `If-Match`, the object an encrypted drive's index names), so an object replaced and put back
/// between two ranges is refused at the range that meets the other version; and the reader says it
/// is at its end only when a look then finds the object still that version; else its read fails, so
/// that the upload that reads it is aborted before it completes.
///
/// A drive that names no tag leaves that look with a size and a date, which an edit that keeps both
/// (a file saved over in the same second, a tool that puts the old date back) leaves as they were:
/// for such an object the end is a SECOND READING of the whole object, hashed, against the hash of
/// what was streamed ([`RangeReader::read_again`]) - twice the bytes from a source that cannot say
/// which version it serves, and never a copy that nothing vouches for.
struct RangeReader<'a> {
    drive: &'a dyn Drive,
    key: &'a str,
    /// The object as it was when the copy began.
    before: ObjectInfo,
    chunk: u64,
    offset: u64,
    buffer: Vec<u8>,
    at: usize,
    /// The bytes streamed, hashed as they come: kept only for an object whose drive names no tag.
    streamed: Option<blake3::Hasher>,
    /// Heard between the ranges of the second reading (the first one is the upload's, which asks
    /// the same `ctl` before every read).
    ctl: &'a dyn Ctl,
    /// The drive said it cannot read a range only while the object is the version `before` names
    /// ([`Drive::get_range_if`]): its ranges are read as they are and the look at the end vouches.
    cannot_pin: bool,
    /// The end was reached and vouched for: said again, it is not looked at again.
    whole: bool,
    /// The look at the end found another version: the copy is not of one.
    changed: bool,
    /// The look at the end failed: nothing vouches for the copy.
    look_failed: Option<DriveError>,
}

impl<'a> RangeReader<'a> {
    fn new(
        drive: &'a dyn Drive,
        key: &'a str,
        before: ObjectInfo,
        chunk: u64,
        ctl: &'a dyn Ctl,
    ) -> RangeReader<'a> {
        // No tag: the look at the end can tell a size and a date and nothing more.
        let streamed = before.etag.is_none().then(blake3::Hasher::new);
        RangeReader {
            drive,
            key,
            before,
            chunk,
            offset: 0,
            buffer: Vec::new(),
            at: 0,
            streamed,
            ctl,
            cannot_pin: false,
            whole: false,
            changed: false,
            look_failed: None,
        }
    }

    fn torn(&self) -> std::io::Error {
        std::io::Error::other(format!("\"{}\" changed while it was copied", self.key))
    }

    /// One range of the object: of the version the copy began with where the drive can ask for it
    /// ([`Drive::get_range_if`]: an object that was replaced and put back between two ranges would
    /// pass a look at the end that finds the tag it began with, and its middle ranges would be the
    /// other version's); as it is, for a drive that says it cannot, or names no tag to ask for.
    fn range(&mut self, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        if let (Some(tag), false) = (&self.before.etag, self.cannot_pin) {
            let condition = Precondition::Matches(tag.clone());
            match self.drive.get_range_if(self.key, range, &condition) {
                Err(DriveError::Unsupported(_)) => self.cannot_pin = true,
                answer => return answer,
            }
        }
        self.drive.get_range(self.key, range)
    }

    /// The look at the end: the object is still the version the copy began with, and - for a drive
    /// that names no tag - a second reading of it is what was streamed. Said again after a failure
    /// it fails again.
    fn still_whole(&mut self) -> std::io::Result<()> {
        if self.changed {
            return Err(self.torn());
        }
        if let Some(e) = &self.look_failed {
            return Err(std::io::Error::other(e.to_string()));
        }
        if self.whole {
            return Ok(());
        }
        match info_of(self.drive, self.key) {
            Ok(now) if same_version(&self.before, &now) => {}
            Ok(_) => {
                self.changed = true;
                return Err(self.torn());
            }
            Err(e) => {
                let told = std::io::Error::other(e.to_string());
                self.look_failed = Some(e);
                return Err(told);
            }
        }
        // Kept until the second reading ends well: a stop in the middle of it leaves the end unvouched
        // for, and a read asked again reads again.
        if let Some(streamed) = self.streamed.as_ref().map(blake3::Hasher::finalize) {
            self.read_again(streamed)?;
        }
        self.whole = true;
        Ok(())
    }

    /// Reads the whole object again, range by range, and compares its hash with that of the bytes
    /// that were streamed: another hash, or an object that is shorter or longer now, is a copy of
    /// two versions ([`RangeReader::changed`]); a reading that fails vouches for nothing. A stop of
    /// `ctl` is heard before every range.
    fn read_again(&mut self, streamed: blake3::Hash) -> std::io::Result<()> {
        let size = self.before.size;
        let mut again = blake3::Hasher::new();
        let mut offset = 0u64;
        while offset < size {
            if self.ctl.cancelled() {
                return Err(Stopped::io());
            }
            let end = (offset + self.chunk).min(size) - 1;
            let asked = end - offset + 1;
            match self.range(ByteRange::new(offset, Some(end))) {
                // As the first reading took them: a range may come back short, never empty and
                // never longer than asked.
                Ok(bytes) if !bytes.is_empty() && bytes.len() as u64 <= asked => {
                    again.update(&bytes);
                    offset += bytes.len() as u64;
                }
                // Empty, longer than the range asked, or no such range any more: the object is not
                // the one that was streamed.
                Ok(_) | Err(DriveError::InvalidRange { .. } | DriveError::Conflict { .. }) => {
                    self.changed = true;
                    return Err(self.torn());
                }
                Err(e) => {
                    let told = std::io::Error::other(e.to_string());
                    self.look_failed = Some(e);
                    return Err(told);
                }
            }
        }
        if again.finalize() != streamed {
            self.changed = true;
            return Err(self.torn());
        }
        Ok(())
    }
}

impl Read for RangeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at >= self.buffer.len() {
            if self.offset >= self.before.size {
                // Asked again after a failure it fails again: the end is never "whole" after a look
                // that said otherwise.
                self.still_whole()?;
                return Ok(0);
            }
            let size = self.before.size;
            let end = (self.offset + self.chunk).min(size) - 1;
            let bytes = match self.range(ByteRange::new(self.offset, Some(end))) {
                Ok(bytes) => bytes,
                // The object has no such range any more (it is shorter than it was when the copy
                // began), or is another version than the one the copy began with.
                Err(DriveError::InvalidRange { .. } | DriveError::Conflict { .. }) => {
                    self.changed = true;
                    return Err(self.torn());
                }
                Err(e) => return Err(std::io::Error::other(e)),
            };
            if bytes.is_empty() {
                return Err(std::io::Error::other(format!(
                    "\"{}\" ended at byte {} of {}",
                    self.key, self.offset, size
                )));
            }
            // More than the range asked is no part of the object the copy began with.
            if bytes.len() as u64 > end - self.offset + 1 {
                self.changed = true;
                return Err(self.torn());
            }
            self.offset += bytes.len() as u64;
            if let Some(streamed) = &mut self.streamed {
                streamed.update(&bytes);
            }
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

/// Whether `now` is the version `before` was: the tag, the size and the date the drive gave - the
/// one question a ranged download and a ranged copy ask of their object when they end.
fn same_version(before: &ObjectInfo, now: &ObjectInfo) -> bool {
    now.etag == before.etag && now.size == before.size && now.modified == before.modified
}

/// What a download that puts a file in place asks of the name it replaces: the rename's own
/// expectation, and the look once more right before it for a rename that asks nothing
/// ([`When::last_look`]).
#[derive(Clone, Copy)]
struct Guarded<'a> {
    expect: Expect,
    last_look: Option<Look<'a>>,
}

/// Fetches the ranges of `info`'s object into its hidden file next to `dest` - the ones a
/// failed download of the same version left there are kept - several at once, then puts it in
/// place when the object did not change meanwhile ([`durable::publish_staged`]: synced to the
/// disk, renamed, the folder synced).
///
/// Every range is pinned to the version the download began with where the drive tells a tag
/// ([`Drive::get_range_if`]), so an object replaced and put back while the ranges were read is
/// refused, not spliced. The RESIDUE (X-DELTA-SYNC4-12, named and not closed): a drive that names no
/// tag for the object (SFTP, FTP, a service without entity tags, a folder on disk) cannot pin a range;
/// its ranges are plain reads and only the look at the end - tag, size and date - tells a replaced
/// object, so one replaced by bytes that keep size and date (`touch -r`, `rsync -t`) is published
/// under the final name. There is no source hash to compare the staged file with here.
fn download_ranges(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    guard: Guarded<'_>,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    download_ranges_checked(drive, info, dest, chunk, guard, ctl, &durable::Unprobed)
}

/// [`download_ranges`] with a look at the step that makes a range durable (a test cuts it by
/// failing it).
#[cfg(test)]
pub(crate) fn download_ranges_probed(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    expect: Expect,
    ctl: &dyn Ctl,
    probe: &(dyn durable::Probe + Sync),
) -> Result<u64, DriveError> {
    let guard = Guarded {
        expect,
        last_look: None,
    };
    download_ranges_checked(drive, info, dest, chunk, guard, ctl, probe)
}

/// The ranged download itself: [`download_ranges`] with a look at the step that makes a range
/// durable (`probe`: a test cuts it by failing it), and `last_look` asked once more when every range
/// is in and the object is still the version the download began with, right before the file is
/// renamed into place: a look that fails ([`DriveError::Conflict`]: the name no longer holds what
/// the plan saw) leaves the hidden file and its state for the next download, which takes the same
/// version without a byte more.
fn download_ranges_checked(
    drive: &dyn Drive,
    info: &ObjectInfo,
    dest: &Path,
    chunk: u64,
    guard: Guarded<'_>,
    ctl: &dyn Ctl,
    probe: &(dyn durable::Probe + Sync),
) -> Result<u64, DriveError> {
    let Guarded { expect, last_look } = guard;
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
    // Every range asks for the version the download began with where the drive can say so
    // ([`Drive::get_range_if`]): an object replaced and put back while the ranges were read would
    // pass the look at the end that finds the tag it began with, and the file would be the splice of
    // two versions under the final name. A drive that cannot pin, or names no tag to pin by, is read
    // as before and looked at when it is done (X-DELTA-SYNC4-12).
    let cannot_pin = std::sync::atomic::AtomicBool::new(false);
    let read_range = |range: ByteRange| -> Result<Vec<u8>, DriveError> {
        if let (Some(tag), false) = (
            &info.etag,
            cannot_pin.load(std::sync::atomic::Ordering::SeqCst),
        ) {
            match drive.get_range_if(key, range, &Precondition::Matches(tag.clone())) {
                Err(DriveError::Unsupported(_)) => {
                    cannot_pin.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                answer => return answer,
            }
        }
        drive.get_range(key, range)
    };
    let result = in_parallel(missing.len(), PARALLEL_RANGES, |n| {
        // A stop is looked for before every range: the ranges in flight finish, none starts.
        stop_if_asked(ctl)?;
        let i = missing[n];
        let start = i * chunk;
        let len = length(i);
        let bytes = read_range(ByteRange::new(start, Some(start + len - 1)))?;
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
    if !same_version(info, &now) {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&state_path);
        return Err(DriveError::Conflict {
            key: key.to_string(),
        });
    }
    // The last look at the target's name, for a rename that asks nothing of the name it replaces.
    if let Some(look) = last_look {
        look()?;
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
    let guard = Guarded {
        expect,
        last_look: None,
    };
    download_looked(drive, key, size, dest, chunk, guard, ctl)
}

/// [`download_guarded`] that asks `last_look` once more right before the file is put in place
/// ([`When::last_look`]): for a name whose `expect` cannot ask the file system for its identity.
fn download_looked(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
    guard: Guarded<'_>,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    let Guarded { expect, last_look } = guard;
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
    // The version (ETag, date) a resumed download must still find.
    if size <= chunk {
        // The size is the listing's: an object that grew since (a log) is not read whole into
        // memory because the drive's `get` has no limit - it is refused at the limit, and the
        // ranges take it by the size it has now (X-DELTA-AZDRIVE-FILES-36).
        match drive.get_limited(key, Limit::new("a listed file", chunk)) {
            Ok(bytes) => {
                stop_if_asked(ctl)?;
                let (folder, name) = folder_of(dest, key)?;
                if let Some(look) = last_look {
                    look()?;
                }
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
            Err(DriveError::Protocol(why)) => {
                let now = info_of(drive, key)?;
                if now.size <= chunk {
                    return Err(DriveError::Protocol(why));
                }
                info = Some(now);
            }
            Err(e) => return Err(e),
        }
    }
    let info = match info {
        Some(info) => info,
        None => info_of(drive, key)?,
    };
    download_ranges(drive, &info, dest, chunk, guard, ctl)
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

/// The question a copy asks once more right before it makes a file visible at its target, when the
/// step that makes it visible cannot ask it itself ([`When::last_look`]): `Ok` while the name still
/// holds what the plan saw, [`DriveError::Conflict`] when it does not.
pub type Look<'a> = &'a (dyn Fn() -> Result<(), DriveError> + Sync);

/// A reader that asks its [`Look`] once, when its body has ended and before it says so: the last
/// thing between the bytes of a copy and the write that completes it, for a target whose write asks
/// no condition of its own (X-DELTA-SYNC2-9). A look that fails is the reader's failure, so that
/// the write that reads it is stopped before it completes, and is kept for the caller to answer with
/// ([`EndLook::failed`]); asked again after a failure it fails again.
struct EndLook<'a> {
    inner: &'a mut dyn Read,
    look: Option<Look<'a>>,
    asked: bool,
    failed: Option<DriveError>,
}

impl<'a> EndLook<'a> {
    fn new(inner: &'a mut dyn Read, look: Option<Look<'a>>) -> EndLook<'a> {
        EndLook {
            inner,
            look,
            asked: false,
            failed: None,
        }
    }
}

impl Read for EndLook<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(why) = &self.failed {
            return Err(std::io::Error::other(why.to_string()));
        }
        let n = self.inner.read(buf)?;
        if n == 0 && !buf.is_empty() && !self.asked {
            self.asked = true;
            if let Some(look) = self.look {
                if let Err(e) = look() {
                    let told = std::io::Error::other(e.to_string());
                    self.failed = Some(e);
                    return Err(told);
                }
            }
        }
        Ok(n)
    }
}

/// Copies the file `source` to `dest` through a temporary file next to it, synced and renamed
/// in ([`durable::publish`]), telling `ctl` the bytes copied so far after every chunk and
/// stopping, with the target as it was and no temporary file, once `ctl` says so. The copy has
/// the file's permission bits (never setuid, setgid or sticky) and its modification time: a
/// 0600 key stays private and a photo keeps its date. `last_look` asks the target's name once more
/// when the bytes have all been read, before the rename.
fn copy_file(
    source: &Path,
    dest: &Path,
    expect: Expect,
    last_look: Option<Look<'_>>,
    ctl: &dyn Ctl,
) -> Result<u64, DriveError> {
    let mut from = File::open(source)?;
    let (mode, mtime) = source_bits(&from);
    let shown = dest.to_string_lossy();
    let (folder, name) = folder_of(dest, &shown)?;
    let mut publish = Publish::new(expect, mode, ctl);
    if let Some(mtime) = mtime {
        publish = publish.with_mtime(mtime);
    }
    let mut body = EndLook::new(&mut from, last_look);
    let landed = durable::publish(&folder, &name, &mut body, publish);
    match (landed, body.failed.take()) {
        (_, Some(changed)) => Err(changed),
        (landed, None) => landed
            .map(|landed| landed.len)
            .map_err(|e| e.into_drive(&shown)),
    }
}

/// What the name at the target must hold when a copied file is put in place there, asked in the
/// step that makes the file visible - after the hours a big copy may have taken, not at its start
/// (X-DELTA-AZDRIVE-FILES-3).
///
/// Three kinds of target, three answers: a folder whose file system gives files an identity asks
/// [`When::expect`] in the rename (nothing can come between); a drive that writes conditionally asks
/// [`When::condition`] in the request that completes the object; any other - a drive that cannot
/// write conditionally, a folder on a system that gives files no identity - was looked at when the
/// copy began and is looked at once more, by [`When::last_look`], right before the write: what is
/// left is the write's own step (a PUT, a rename), never the copy that took minutes
/// (X-DELTA-SYNC2-9).
pub struct When<'a> {
    /// For a folder on this computer: checked by the rename that puts the file in place.
    pub expect: Expect,
    /// For a drive that [writes conditionally](Drive::conditional_writes): checked by the request
    /// that completes the object (a bucket) or the index change that names it (an encrypted
    /// drive). A drive that cannot is not asked: the caller looks at its name first.
    pub condition: Option<&'a Precondition>,
    /// For a target that asks no condition of its own write (a drive that cannot, or a folder whose
    /// `expect` is [`Expect::Any`] because the file system gave the file no identity): asked once
    /// more when the last byte has been read and before the write that completes the copy. A drive
    /// that [writes conditionally](Drive::conditional_writes) is not asked this: its request is.
    pub last_look: Option<Look<'a>>,
}

impl fmt::Debug for When<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("When")
            .field("expect", &self.expect)
            .field("condition", &self.condition)
            .field("last_look", &self.last_look.map(|_| "a look"))
            .finish()
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
        last_look: None,
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
    // A write that asks its condition itself needs no last look; any other gets one right before it.
    let last_look = when.last_look.filter(|_| condition.is_none());
    match (source.local_path(key), target.local_path(target_key)) {
        (Some(from), Some(to)) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            copy_file(&from, &to, expect, last_look, ctl)
        }
        (None, Some(to)) => {
            let guard = Guarded { expect, last_look };
            let len = download_looked(source, key, size, &to, CHUNK, guard, ctl)?;
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
            match (condition, last_look) {
                (Some(condition), _) => {
                    let len = fs::metadata(&from)?.len();
                    target
                        .put_file_if(target_key, &from, condition, ctl)
                        .map(|_| len)
                }
                (None, None) => target.put_file(target_key, &from, ctl),
                // A drive that cannot ask: the file streams into its write through the last look
                // (what `put_file` does for it, with the name looked at once more when the last
                // byte has been read and before the write completes).
                (None, Some(look)) => {
                    let mut file = File::open(&from)?;
                    let mut progress = ProgressReader::new(&mut file, ctl);
                    let mut body = EndLook::new(&mut progress, Some(look));
                    let sent = target.put_from(target_key, &mut body);
                    match body.failed.take() {
                        Some(changed) => Err(changed),
                        None => sent,
                    }
                }
            }
        }
        (None, None) => {
            let size = match size {
                Some(size) => size,
                None => source.head(key)?.size,
            };
            if size <= CHUNK {
                // As a download: the listing's size is not the object's if it grew since; it is
                // refused at the limit and goes by ranges (X-DELTA-AZDRIVE-FILES-36).
                match source.get_limited(key, Limit::new("a listed file", CHUNK)) {
                    Ok(bytes) => {
                        stop_if_asked(ctl)?;
                        match condition {
                            Some(condition) => {
                                target.put_if(target_key, &bytes, condition).map(|_| ())?
                            }
                            None => {
                                // The bytes are in memory: the name is looked at as they go.
                                if let Some(look) = last_look {
                                    look()?;
                                }
                                target.put(target_key, &bytes)?
                            }
                        }
                        ctl.bytes(bytes.len() as u64);
                        return Ok(bytes.len() as u64);
                    }
                    Err(DriveError::Protocol(why)) => {
                        let now = source.head(key)?.size;
                        if now <= CHUNK {
                            return Err(DriveError::Protocol(why));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            // The version the ranges must all be of: one look now, the same look when the last range
            // is in (X-DELTA-SYNC2-3). The size is the object's as it is now, not as it was listed.
            let before = info_of(source, key)?;
            // A look that says "empty" of an object that was listed (or looked at) as big is a look
            // nothing vouches for - a drive that does not tell its sizes says 0 - and an empty file
            // under the target's name would be a finished copy of nothing: it is not copied.
            if before.size == 0 {
                return Err(DriveError::Protocol(format!(
                    "\"{key}\": the drive says it is empty, and it was {size} bytes a moment ago: it \
                     is not copied"
                )));
            }
            let size = before.size;
            let mut ranges = RangeReader::new(source, key, before, CHUNK, ctl);
            let (copied, target_changed) = {
                let mut reader = ProgressReader::new(&mut ranges, ctl);
                // The last look, for a target that asks no condition of its own write, comes after
                // the source's own end check: the last thing before the write completes.
                let mut body = EndLook::new(&mut reader, last_look);
                let copied = match condition {
                    Some(condition) => target
                        .put_from_if(target_key, &mut body, condition)
                        .map(|_| size),
                    None => target.put_from(target_key, &mut body),
                };
                (copied, body.failed.take())
            };
            match (copied, target_changed) {
                (Ok(len), _) => Ok(len),
                // The upload read a torn end and was aborted: another device saved the object
                // while it was copied. Nothing is made visible under the target's name.
                (Err(_), _) if ranges.changed => Err(DriveError::Conflict {
                    key: key.to_string(),
                }),
                // The name at the target is not what the plan saw: the upload was stopped before
                // it completed.
                (Err(_), Some(changed)) => Err(changed),
                (Err(e), None) => Err(ranges.look_failed.take().unwrap_or(e)),
            }
        }
    }
}
