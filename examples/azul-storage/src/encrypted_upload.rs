//! An encrypted file's upload that outlives a dropped connection or a killed app.
//!
//! A file of its own object is encrypted once into its AZL1 object, in the folder where the
//! bucket keeps what its cut uploads resume from ([`Drive::resume_folder`]), and that file of
//! ciphertext goes up as the bucket's resumable upload of a local file: the upload id and the
//! parts sent are kept in the bucket's state file, so the next upload of the same file sends
//! only the parts the bucket lacks. The ciphertext is written beside that state - on the disk
//! it is used on, its free space checked first - under a name of its own, and renamed into
//! place only when it is whole; a record beside it keeps what the drive's index needs of the
//! object and which version of the file it is (its size and date, the BLAKE3 a sync named it
//! by). Another version of the file is encrypted anew. Once the object is up and the index
//! names it, the ciphertext and its record are gone, as a streamed upload leaves nothing
//! behind; until then they stay: an index commit that fails after the object went up (a service
//! that answers 503, a quota refusal) is retried with the same object - the next try finds it
//! up whole and only names it, it does not encrypt the file again under another id and send it
//! again. One nobody resumes is swept after [`STALE_AFTER_SECS`].
//!
//! The ciphertext holds no plaintext and the record no key: the file key in it is wrapped
//! with the drive key, as in the object's own header.

// panic = "abort" in every shipped profile, and this module reads bytes other people wrote: no
// unwrap, expect, panic, todo or unreachable in its shipped code (ARCH28 RF50).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    beneath::{self, Dir, Fingerprint, Kind, Stat},
    cap::{Cap, Posture},
    checked::{self, FileCheck, HashCheck},
    crypto::{
        azl1::{self, FileHint, ObjectSummary, TransformId},
        random_bytes, to_hex, ObjectId, WrappedKey,
    },
    ctl::{Ctl, Quiet},
    durable::{self, Blob, Mode, Publish},
    encrypted::{EncryptedDrive, Expect},
    key::check_key,
    multipart::STALE_AFTER_SECS,
    Drive, DriveError,
};

/// The folder of the encrypted copies under a resume folder.
const SPOOL_DIR: &str = "encrypted";
/// What the look at the disk's room before the copy is written does when the disk cannot say its
/// free space (a network share, a FUSE mount, a sandbox): the copy is written anyway, and the
/// report says that the look was left out ([`crate::cap::take_unchecked`]). The look is an early
/// warning, not the guard - a disk that is full says so at the write
/// ([`crate::durable::PublishError::DiskFull`]).
const ROOM_POSTURE: Posture = Posture::Degrade(
    "the free space of the disk cannot be read: the copy is written without the early look",
);
/// The version of a record this code writes.
const RECORD_VERSION: u32 = 1;

/// What is kept beside an encrypted copy: which file and which version of it the copy is of,
/// and what the drive's index names of the object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    version: u32,
    /// The drive key the copy is sealed for (its public id).
    drive_key: String,
    /// The drive path.
    key: String,
    /// The file on this computer, and its size, date and identity when it was encrypted: the
    /// copy is reused only for the file that is, for certain, still that file untouched. A
    /// record without the identity (an older one) is never reused.
    source: String,
    size: u64,
    modified_ns: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    ctime_ns: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    dev: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    ino: u64,
    object_id: String,
    plaintext_size: u64,
    object_len: u64,
    segment_count: u64,
    /// BLAKE3 of the plaintext, lowercase hex.
    blake3: String,
    compressed: bool,
    /// The whole-file transform (its id and version bytes), when the object has one.
    transform: Option<(u8, u8)>,
    /// The file key wrapped with the drive key, as in the object's header.
    wrapped_key: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Record {
    fn of(
        drive_key: &str,
        key: &str,
        source: &Path,
        stat: &Stat,
        summary: &ObjectSummary,
    ) -> Record {
        Record {
            version: RECORD_VERSION,
            drive_key: drive_key.to_string(),
            key: key.to_string(),
            source: source.display().to_string(),
            size: stat.size,
            modified_ns: stat.mtime_ns,
            ctime_ns: stat.ctime_ns,
            dev: stat.dev,
            ino: stat.ino,
            object_id: summary.object_id.to_hex(),
            plaintext_size: summary.plaintext_size,
            object_len: summary.object_len,
            segment_count: summary.segment_count,
            blake3: blake3::Hash::from(summary.blake3).to_hex().to_string(),
            compressed: summary.compressed,
            transform: summary.transform.map(TransformId::bytes),
            wrapped_key: summary.wrapped_key.to_hex(),
        }
    }

    /// The file as it was when it was encrypted.
    fn print(&self) -> Fingerprint {
        Fingerprint::from_parts(
            Kind::File,
            self.size,
            self.modified_ns,
            self.ctime_ns,
            self.dev,
            self.ino,
        )
    }

    /// The summary the index names the object by; `None` for a record that does not read.
    fn summary(&self) -> Option<ObjectSummary> {
        let transform = match self.transform {
            Some((id, version)) => Some(TransformId::from_bytes(id, version).ok()?),
            None => None,
        };
        Some(ObjectSummary {
            object_id: ObjectId::from_hex(&self.object_id)?,
            plaintext_size: self.plaintext_size,
            object_len: self.object_len,
            segment_count: self.segment_count,
            blake3: *blake3::Hash::from_hex(&self.blake3).ok()?.as_bytes(),
            compressed: self.compressed,
            transform,
            wrapped_key: WrappedKey::from_hex(&self.wrapped_key)?,
        })
    }
}

/// The file at `path` as it is now (a link followed: the path is the caller's own).
fn stat_at(path: &Path) -> io::Result<Stat> {
    beneath::stat_path(path)
}

/// Whether `now` is, as far as can be told, the file `before` described and nothing wrote to
/// it since: a size, a date or a status-change time that differs, or a known other file, says no.
/// An identity the system does not give is not asked here (this is a check that the file held
/// still while it was read); a copy is REUSED only for [`Record::print`] matching for certain.
fn held_still(before: &Stat, now: &Stat) -> bool {
    !before.fingerprint().matches(now).is_different()
}

/// The record of the encrypted copy of `source` for `key`, in `dir`: one name per drive key, path
/// and file. It names the copy it describes ([`copy_of`]): two processes that encrypt one file
/// each make a copy of their own, and a record is never beside the other process's copy.
fn record_path(dir: &Path, drive_key: &str, key: &str, source: &Path) -> PathBuf {
    let named = format!("{drive_key}\n{key}\n{}", source.display());
    let hash = blake3::hash(named.as_bytes()).to_hex();
    dir.join(format!("{}.json", &hash.as_str()[..32]))
}

/// The copy a record describes: its name holds the object's id, which only this encryption has.
fn copy_name(record_path: &Path, object_id: &str) -> String {
    let stem = record_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{stem}-{object_id}.azl1")
}

/// The bytes an encrypted copy of `size` bytes takes at most: the file, a header, a tag per
/// segment and a trailer, with room to spare.
fn room_for(size: u64) -> u64 {
    size.saturating_add(size / 256).saturating_add(1 << 20)
}

/// Removes the copies and records in `dir` nobody resumed for [`STALE_AFTER_SECS`], and the
/// pieces of copies a killed app left half written.
fn sweep(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let stale = Duration::from_secs(STALE_AFTER_SECS);
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| SystemTime::now().duration_since(at).ok())
            .is_some_and(|age| age > stale);
        if old {
            let _ = fs::remove_file(&path);
        }
    }
}

/// The record at `path`, when there is one that reads.
fn read_record(path: &Path) -> Option<Record> {
    fs::read(path)
        .ok()
        .and_then(|text| serde_json::from_slice::<Record>(&text).ok())
}

/// The caller's [`Ctl`] as the bucket's upload of the encrypted copy hears it: the bucket counts
/// the object's bytes, the caller is told the file's; a stop passes through as it is.
struct Scaled<'a> {
    ctl: &'a dyn Ctl,
    /// The file's bytes.
    plain: u64,
    /// The object's bytes (at least 1).
    stored: u64,
}

impl Ctl for Scaled<'_> {
    fn bytes(&self, sent: u64) {
        let share = u128::from(sent) * u128::from(self.plain) / u128::from(self.stored);
        self.ctl
            .bytes(u64::try_from(share).unwrap_or(self.plain).min(self.plain));
    }

    fn cancelled(&self) -> bool {
        self.ctl.cancelled()
    }
}

/// How the upload of a file ended, when it went the resumable way.
pub(crate) enum Spooled {
    /// The object holds the file's bytes: how many, and the entity tag the index names the new
    /// version by.
    Sent(u64, Option<String>),
    /// The file is not the one the upload was asked for (it changed): nothing was completed.
    Changed,
}

impl<D: Drive> EncryptedDrive<D> {
    /// Writes the local file `path` as `key`, an object of its own, so that a cut upload resumes
    /// at its next part (see the module); `check`: the file must be the one it describes (a
    /// sync's). `expect`: what the index must hold of `key` when the object is named - checked in
    /// the index change that names it, after the hours the upload may have taken
    /// (X-DELTA-AZDRIVE-FILES-3); [`Expect::Any`] for a write that replaces whatever is there.
    /// `None` when this drive does not upload that way: its bucket resumes no uploads, or the
    /// file is small enough to be packed - the caller streams it then.
    pub(crate) fn put_file_resumable(
        &self,
        key: &str,
        path: &Path,
        check: Option<&FileCheck>,
        expect: Expect,
        ctl: &dyn Ctl,
    ) -> Result<Option<Spooled>, DriveError> {
        check_key(key)?;
        self.refuse_changes()?;
        let Some(folder) = self.inner.resume_folder() else {
            return Ok(None);
        };
        let before = stat_at(path)?;
        if self
            .pack_threshold()
            .is_some_and(|threshold| before.size < threshold)
        {
            return Ok(None);
        }
        if check.is_some_and(|c| c.size != before.size || c.modified_ns != before.mtime_ns) {
            return Ok(Some(Spooled::Changed));
        }
        let dir = folder.join(SPOOL_DIR);
        fs::create_dir_all(&dir)?;
        sweep(&dir);
        let drive_key = self.drive_key.id().to_hex();
        let record_path = record_path(&dir, &drive_key, key, path);
        let written = read_record(&record_path);
        // The copy of an earlier try is reused only when its record is of this drive, key and
        // file, the file is, for certain, still that file untouched (size, dates AND identity:
        // an edit that keeps the size and the time still changes the status-change time, and a
        // file put in its place is another inode), the bytes are the checked ones, and the copy
        // is whole.
        let kept = written
            .as_ref()
            .filter(|record| {
                record.version == RECORD_VERSION
                    && record.drive_key == drive_key
                    && record.key == key
                    && record.source == path.display().to_string()
                    && record.print().matches(&before).is_same()
                    && check.is_none_or(|c| record.blake3.eq_ignore_ascii_case(c.blake3.trim()))
                    && fs::metadata(dir.join(copy_name(&record_path, &record.object_id)))
                        .is_ok_and(|m| m.len() == record.object_len)
            })
            .and_then(|record| {
                let copy = dir.join(copy_name(&record_path, &record.object_id));
                record.summary().map(|summary| (summary, copy))
            });
        let (summary, copy, kept_copy) = match kept {
            Some((summary, copy)) => (summary, copy, true),
            None => {
                // The copy of the record that is not reused goes with it.
                if let Some(old) = &written {
                    let _ = fs::remove_file(dir.join(copy_name(&record_path, &old.object_id)));
                }
                let _ = fs::remove_file(&record_path);
                match self.encrypt_copy(key, path, check, &before, &dir, &record_path, ctl)? {
                    Some((summary, copy)) => (summary, copy, false),
                    None => return Ok(Some(Spooled::Changed)),
                }
            }
        };
        // The bucket hears the object's bytes; the caller is told the file's.
        let plain = summary.plaintext_size;
        let stored = summary.object_len.max(1);
        let told = Scaled { ctl, plain, stored };
        // A cut upload keeps the copy and its record: the next one resumes with them. So does a
        // copy whose object is up whole already - the index's commit failed after it (AZ-M3): it
        // is named, not sent again. Its id is 128 random bits only this copy knows, and an object
        // of the copy's length is the copy (a bucket's object appears whole or not at all).
        let object_key = summary.object_id.bucket_key();
        let up_already = kept_copy
            && self
                .inner
                .head(&object_key)
                .is_ok_and(|info| info.size == summary.object_len);
        if up_already {
            ctl.bytes(plain);
        } else {
            // A stop reaches the bucket's upload (it aborts and forgets its state) and leaves
            // nothing behind, as `Drive::put_file` promises: the copy goes with its record. Any
            // other failure keeps both for the next try.
            if let Err(e) = self.inner.put_file(&object_key, &copy, &told) {
                if matches!(e, DriveError::Cancelled) {
                    let _ = fs::remove_file(&copy);
                    if read_record(&record_path)
                        .is_some_and(|record| record.object_id == summary.object_id.to_hex())
                    {
                        let _ = fs::remove_file(&record_path);
                    }
                }
                return Err(e);
            }
        }
        // The copy and its record go only once the index names the object. A commit that fails
        // leaves them, and the next try names the same object again; they used to go first, so
        // it encrypted the file anew under another id, sent it all again, and left the first
        // object named by nothing. The record goes only if it is still this copy's: another
        // process that encrypted the same file meanwhile has a record of its own there.
        let object_id = summary.object_id.to_hex();
        let named = match self.bind(key, summary, expect) {
            Ok(info) => info,
            // The name changed while the object went up: the index refused, and took the object
            // back out of the bucket. The copy is of no use to anyone now (the user decides again,
            // and a copy of a big file is as big as the file): it goes with its record, where a
            // failure that may pass (a 503) keeps both for the retry.
            Err(e @ DriveError::Conflict { .. }) => {
                let _ = fs::remove_file(&copy);
                if read_record(&record_path).is_some_and(|record| record.object_id == object_id) {
                    let _ = fs::remove_file(&record_path);
                }
                return Err(e);
            }
            Err(e) => return Err(e),
        };
        let _ = fs::remove_file(&copy);
        if read_record(&record_path).is_some_and(|record| record.object_id == object_id) {
            let _ = fs::remove_file(&record_path);
        }
        Ok(Some(Spooled::Sent(plain, named.etag)))
    }

    /// Encrypts `path` (the file `before` describes, the one `check` names when there is one)
    /// into a copy in `dir` under a name of its own ([`copy_name`]: the object's id is in it),
    /// then its record into `record_path`; `None` when the file changed while it was read
    /// (nothing is kept). The record is written after the copy is in place and names it, so a
    /// record is never beside a copy it is not of, and a cut between the two leaves a copy
    /// nothing names (swept) - not a record without its copy.
    #[allow(clippy::too_many_arguments)]
    fn encrypt_copy(
        &self,
        key: &str,
        path: &Path,
        check: Option<&FileCheck>,
        before: &Stat,
        dir: &Path,
        record_path: &Path,
        ctl: &dyn Ctl,
    ) -> Result<Option<(ObjectSummary, PathBuf)>, DriveError> {
        match (crate::beneath::free_space(dir), ROOM_POSTURE) {
            (Cap::Known(free), _) => {
                let needed = room_for(before.size);
                if free < needed {
                    return Err(DriveError::Io(format!(
                        "no space left on the disk of {} for the encrypted copy of {key}: it \
                         needs {needed} bytes, {free} are free",
                        dir.display()
                    )));
                }
            }
            (Cap::Unknown(why), Posture::Degrade(note)) => {
                crate::cap::note_unchecked(&format!("{note} ({why})"));
            }
            (Cap::Unknown(why), Posture::FailClosed) => {
                return Err(DriveError::Io(format!(
                    "{why}: the encrypted copy of {key} is not written without knowing it fits \
                     the disk of {}",
                    dir.display()
                )));
            }
        }
        let file = match check {
            Some(c) => match checked::open_checked(path, c)? {
                Some(file) => file,
                None => return Ok(None),
            },
            None => File::open(path)?,
        };
        if !held_still(before, &beneath::stat_of(&file)?) {
            return Ok(None);
        }
        // A piece of its own until it is whole: a cut copy is never taken for one.
        let mut name = [0u8; 8];
        random_bytes(&mut name).map_err(|e| e.for_key(key))?;
        let stem = record_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let piece_name = format!("{stem}.{}.part", to_hex(&name));
        let piece = dir.join(&piece_name);
        // The folder is walked once: the piece is made in it, and adopted from it.
        let folder = Dir::open_following(dir).map_err(DriveError::from)?;
        let written = self.encrypt_into(key, file, check, &folder, &piece_name, ctl);
        let summary = match written {
            Ok(Some(summary)) => summary,
            other => {
                let _ = fs::remove_file(&piece);
                return other.map(|_| None);
            }
        };
        if !stat_at(path).is_ok_and(|now| held_still(before, &now)) {
            let _ = fs::remove_file(&piece);
            return Ok(None);
        }
        // The copy in place, whole and synced, under its own name; nothing has that name.
        let copy_name = copy_name(record_path, &summary.object_id.to_hex());
        let placed = durable::publish_staged(
            &folder,
            &copy_name,
            Blob {
                dir: &folder,
                name: &piece_name,
            },
            Publish::new(durable::Expect::Absent, Mode::Shared, &Quiet),
        )
        .map_err(|e| e.into_drive(key));
        if let Err(e) = placed {
            let _ = fs::remove_file(&piece);
            return Err(e);
        }
        let copy = dir.join(&copy_name);
        let record = Record::of(&self.drive_key.id().to_hex(), key, path, before, &summary);
        let text = serde_json::to_vec_pretty(&record)
            .map_err(|e| DriveError::Io(format!("the record of {key}'s upload: {e}")))?;
        if let Err(e) = durable::replace(record_path, &text, Mode::Private) {
            let _ = fs::remove_file(&copy);
            let _ = fs::remove_file(record_path);
            return Err(e.into());
        }
        Ok(Some((summary, copy)))
    }

    /// The object of `file` (read through its check when there is one) written to a new file
    /// `piece_name` of `folder`; `None` when the file's bytes are not the checked ones. The
    /// piece is made by the folder handle ([`Dir::create_new`]: it must not exist, a link at its
    /// name is not followed): AZL1 writes its header last, so the sink is a seekable file, and
    /// that is where the raw `create_new` of this file went (RF41's gate).
    fn encrypt_into(
        &self,
        key: &str,
        file: File,
        check: Option<&FileCheck>,
        folder: &Dir,
        piece_name: &str,
        ctl: &dyn Ctl,
    ) -> Result<Option<ObjectSummary>, DriveError> {
        let id = ObjectId::generate().map_err(|e| e.for_key(key))?;
        let hint = FileHint::for_path(key, file.metadata().ok().map(|m| m.len()));
        let mut sink = folder
            .create_new(piece_name, 0o666)
            .map_err(|e| crate::local::beneath_error(key, e))?;
        // The file is read through a reader that asks `ctl` before every read: the encryption of
        // a 20 GB file is the first long phase of the upload, and a stop pressed there used to be
        // heard only when the bucket's upload began (X-DELTA-AZDRIVE-FILES-13).
        let summary = match check {
            Some(c) => {
                let mut hashed = HashCheck::new(file, &c.blake3);
                let summary = {
                    let mut stoppable = crate::transfer::StopReader::new(&mut hashed, ctl);
                    azl1::encrypt_stream_as(
                        &mut stoppable,
                        &mut sink,
                        id,
                        &self.drive_key,
                        &self.options,
                        &hint,
                    )
                };
                if hashed.changed() {
                    return Ok(None);
                }
                summary
            }
            None => {
                let mut file = file;
                let mut stoppable = crate::transfer::StopReader::new(&mut file, ctl);
                azl1::encrypt_stream_as(
                    &mut stoppable,
                    &mut sink,
                    id,
                    &self.drive_key,
                    &self.options,
                    &hint,
                )
            }
        }
        .map_err(|e| e.for_key(key))?;
        sink.flush()?;
        sink.sync_all()?;
        Ok(Some(summary))
    }
}
