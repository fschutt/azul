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
//! by). Another version of the file is encrypted anew. Once the object is up, the ciphertext
//! and its record are gone before the index names the object, as a streamed upload leaves
//! nothing behind; one nobody resumes is swept after [`STALE_AFTER_SECS`].
//!
//! The ciphertext holds no plaintext and the record no key: the file key in it is wrapped
//! with the drive key, as in the object's own header.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    checked::{self, FileCheck, HashCheck},
    crypto::{
        azl1::{self, FileHint, ObjectSummary, TransformId},
        random_bytes, to_hex, ObjectId, WrappedKey,
    },
    encrypted::{EncryptedDrive, Expect},
    key::check_path_key,
    local::write_atomically,
    multipart::STALE_AFTER_SECS,
    Drive, DriveError,
};

/// The folder of the encrypted copies under a resume folder.
const SPOOL_DIR: &str = "encrypted";
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
    /// The file on this computer, and its size and date when it was encrypted.
    source: String,
    size: u64,
    modified_ns: u64,
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

impl Record {
    fn of(
        drive_key: &str,
        key: &str,
        source: &Path,
        stat: Stat,
        summary: &ObjectSummary,
    ) -> Record {
        Record {
            version: RECORD_VERSION,
            drive_key: drive_key.to_string(),
            key: key.to_string(),
            source: source.display().to_string(),
            size: stat.size,
            modified_ns: stat.modified_ns,
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

/// A file's size and date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stat {
    size: u64,
    modified_ns: u64,
}

impl Stat {
    fn of(file: &File) -> io::Result<Stat> {
        let meta = file.metadata()?;
        Ok(Stat {
            size: meta.len(),
            modified_ns: checked::modified_ns(&meta),
        })
    }

    fn at(path: &Path) -> io::Result<Stat> {
        let meta = fs::metadata(path)?;
        Ok(Stat {
            size: meta.len(),
            modified_ns: checked::modified_ns(&meta),
        })
    }
}

/// The encrypted copy of `source` for `key`, its record, in `dir`: one name per drive key,
/// path and file.
fn copy_paths(dir: &Path, drive_key: &str, key: &str, source: &Path) -> (PathBuf, PathBuf) {
    let named = format!("{drive_key}\n{key}\n{}", source.display());
    let hash = blake3::hash(named.as_bytes()).to_hex();
    let stem = &hash.as_str()[..32];
    (dir.join(format!("{stem}.azl1")), dir.join(format!("{stem}.json")))
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

/// How the upload of a file ended, when it went the resumable way.
pub(crate) enum Spooled {
    /// The object holds the file's bytes: how many.
    Sent(u64),
    /// The file is not the one the upload was asked for (it changed): nothing was completed.
    Changed,
}

impl<D: Drive> EncryptedDrive<D> {
    /// Writes the local file `path` as `key`, an object of its own, so that a cut upload resumes
    /// at its next part (see the module); `check`: the file must be the one it describes (a
    /// sync's). `None` when this drive does not upload that way: its bucket resumes no uploads,
    /// or the file is small enough to be packed - the caller streams it then.
    pub(crate) fn put_file_resumable(
        &self,
        key: &str,
        path: &Path,
        check: Option<&FileCheck>,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<Option<Spooled>, DriveError> {
        check_path_key(key)?;
        let Some(folder) = self.inner.resume_folder() else {
            return Ok(None);
        };
        let before = Stat::at(path)?;
        if self
            .pack_threshold()
            .is_some_and(|threshold| before.size < threshold)
        {
            return Ok(None);
        }
        if check.is_some_and(|c| c.size != before.size || c.modified_ns != before.modified_ns) {
            return Ok(Some(Spooled::Changed));
        }
        let dir = folder.join(SPOOL_DIR);
        fs::create_dir_all(&dir)?;
        sweep(&dir);
        let drive_key = self.drive_key.id().to_hex();
        let (copy, record_path) = copy_paths(&dir, &drive_key, key, path);
        let kept = fs::read(&record_path)
            .ok()
            .and_then(|text| serde_json::from_slice::<Record>(&text).ok())
            .filter(|record| {
                record.version == RECORD_VERSION
                    && record.drive_key == drive_key
                    && record.key == key
                    && record.source == path.display().to_string()
                    && record.size == before.size
                    && record.modified_ns == before.modified_ns
                    && check.is_none_or(|c| record.blake3.eq_ignore_ascii_case(c.blake3.trim()))
                    && fs::metadata(&copy).is_ok_and(|m| m.len() == record.object_len)
            })
            .and_then(|record| record.summary());
        let summary = match kept {
            Some(summary) => summary,
            None => {
                let _ = fs::remove_file(&copy);
                let _ = fs::remove_file(&record_path);
                match self.encrypt_copy(key, path, check, before, &copy, &record_path)? {
                    Some(summary) => summary,
                    None => return Ok(Some(Spooled::Changed)),
                }
            }
        };
        // The bucket hears the object's bytes; the caller is told the file's.
        let plain = summary.plaintext_size;
        let stored = summary.object_len.max(1);
        let told = |sent: u64| {
            let share = u128::from(sent) * u128::from(plain) / u128::from(stored);
            progress(u64::try_from(share).unwrap_or(plain).min(plain));
        };
        // A cut upload keeps the copy and its record: the next one resumes with them.
        self.inner
            .put_file(&summary.object_id.bucket_key(), &copy, &told)?;
        let _ = fs::remove_file(&copy);
        let _ = fs::remove_file(&record_path);
        self.bind(key, summary, Expect::Any)?;
        Ok(Some(Spooled::Sent(plain)))
    }

    /// Encrypts `path` (the file `before` describes, the one `check` names when there is one)
    /// into the copy `copy`, its record into `record_path`; `None` when the file changed while
    /// it was read (nothing is kept).
    fn encrypt_copy(
        &self,
        key: &str,
        path: &Path,
        check: Option<&FileCheck>,
        before: Stat,
        copy: &Path,
        record_path: &Path,
    ) -> Result<Option<ObjectSummary>, DriveError> {
        let dir = copy.parent().unwrap_or_else(|| Path::new("."));
        if let Some(free) = crate::beneath::free_space(dir) {
            let needed = room_for(before.size);
            if free < needed {
                return Err(DriveError::Io(format!(
                    "no space left on the disk of {} for the encrypted copy of {key}: it needs \
                     {needed} bytes, {free} are free",
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
        if Stat::of(&file)? != before {
            return Ok(None);
        }
        // A piece of its own until it is whole: a cut copy is never taken for one.
        let mut name = [0u8; 8];
        random_bytes(&mut name).map_err(|e| e.for_key(key))?;
        let piece = copy.with_extension(format!("{}.part", to_hex(&name)));
        let written = self.encrypt_into(key, file, check, &piece);
        let summary = match written {
            Ok(Some(summary)) => summary,
            other => {
                let _ = fs::remove_file(&piece);
                return other;
            }
        };
        if Stat::at(path)? != before {
            let _ = fs::remove_file(&piece);
            return Ok(None);
        }
        let record = Record::of(
            &self.drive_key.id().to_hex(),
            key,
            path,
            before,
            &summary,
        );
        let text = serde_json::to_vec_pretty(&record)
            .map_err(|e| DriveError::Io(format!("the record of {key}'s upload: {e}")))?;
        let kept = write_atomically(record_path, &text).and_then(|()| fs::rename(&piece, copy));
        if let Err(e) = kept {
            let _ = fs::remove_file(&piece);
            let _ = fs::remove_file(record_path);
            return Err(e.into());
        }
        Ok(Some(summary))
    }

    /// The object of `file` (read through its check when there is one) written to `piece`;
    /// `None` when the file's bytes are not the checked ones.
    fn encrypt_into(
        &self,
        key: &str,
        file: File,
        check: Option<&FileCheck>,
        piece: &Path,
    ) -> Result<Option<ObjectSummary>, DriveError> {
        let id = ObjectId::generate().map_err(|e| e.for_key(key))?;
        let hint = FileHint::for_path(key, file.metadata().ok().map(|m| m.len()));
        let mut sink = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(piece)?;
        let summary = match check {
            Some(c) => {
                let mut hashed = HashCheck::new(file, &c.blake3);
                let summary = azl1::encrypt_stream_as(
                    &mut hashed,
                    &mut sink,
                    id,
                    &self.drive_key,
                    &self.options,
                    &hint,
                );
                if hashed.changed() {
                    return Ok(None);
                }
                summary
            }
            None => {
                let mut file = file;
                azl1::encrypt_stream_as(
                    &mut file,
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
