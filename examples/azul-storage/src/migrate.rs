//! Moving a plaintext drive's files into its encrypted namespace, resumably (feature
//! `encryption`).
//!
//! A drive that was plaintext holds its files in its bucket under their names. Once the drive
//! is encrypted ([`crate::crypto::device::setup_new_drive`]), [`migrate`] moves every such file
//! through the [`EncryptedDrive`]: read in ranges of 8 MiB, written as an AZL1 object under a
//! random key (named in the index only, `Expect::Absent`), checked (the BLAKE3 the index keeps
//! against the one of the bytes read), then - only then - the plaintext object is deleted.
//!
//! Resumable: the caller saves the [`MigrationState`] after every file (a state file); a run
//! stopped half way (a crash, a closed laptop, `stop`) starts over by listing what is still
//! plaintext - the files moved are gone from the listing. A file a crash left half moved (in
//! the index, its plaintext not yet deleted) is finished: same bytes, the plaintext goes; other
//! bytes, both stay and the file is reported. A file that changes while it is moved keeps its
//! plaintext (the new bytes), its encrypted copy is taken back, and the next run moves it.
//!
//! Not plaintext files: the data objects (`data/<2 hex>/<32 hex>`) and the bucket's own
//! bookkeeping under `.azlin/` (the keys, the metadata repository). A folder marker becomes an
//! index marker. A key an encrypted drive cannot name (`..`, an empty segment, a backslash)
//! stays where it is and is reported.
//!
//! The other devices must have stopped writing plaintext (they see `.azlin/keys/` and open the
//! drive encrypted): a plaintext write between the last check and the delete would be lost.

use std::{
    collections::BTreeMap,
    io::{self, Read},
};

use serde::{Deserialize, Serialize};

use crate::{
    crypto::ObjectId,
    encrypted::{EncryptedDrive, Expect},
    key::check_path_key,
    ops::list_all,
    transfer::CHUNK,
    ByteRange, Drive, DriveError, ObjectInfo,
};

/// The `format` of a saved state.
pub const STATE_FORMAT: &str = "azul-storage.migration";

/// Where a migration is: what the caller saves after every file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationState {
    pub format: String,
    /// Files moved (an interrupted move finished counts too).
    pub moved: u64,
    /// Their bytes.
    pub bytes: u64,
    /// Plaintext keys left where they are, with why: shown to the user, not tried again.
    #[serde(default)]
    pub skipped: BTreeMap<String, String>,
    /// Every plaintext file was moved or skipped.
    #[serde(default)]
    pub done: bool,
}

impl Default for MigrationState {
    fn default() -> Self {
        MigrationState {
            format: STATE_FORMAT.to_string(),
            moved: 0,
            bytes: 0,
            skipped: BTreeMap::new(),
            done: false,
        }
    }
}

impl MigrationState {
    /// The state as the state file keeps it.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A state file read back.
    pub fn from_json(text: &str) -> Result<MigrationState, DriveError> {
        let state: MigrationState = serde_json::from_str(text).map_err(|e| {
            DriveError::InvalidConfig(format!("not a migration state (line {})", e.line()))
        })?;
        if state.format != STATE_FORMAT {
            return Err(DriveError::InvalidConfig(format!(
                "not a migration state (format {:?})",
                state.format
            )));
        }
        Ok(state)
    }
}

/// Whether `key` of a bucket is a plaintext file of the drive (not its encrypted objects, not
/// its bookkeeping).
#[must_use]
pub fn is_plaintext_key(key: &str) -> bool {
    !key.starts_with(".azlin/") && ObjectId::from_bucket_key(key).is_none()
}

/// An object read in ranged GETs of [`CHUNK`] bytes: one chunk in memory.
struct Ranges<'a> {
    drive: &'a dyn Drive,
    key: &'a str,
    size: u64,
    offset: u64,
    chunk: Vec<u8>,
    at: usize,
}

impl Read for Ranges<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.chunk.len() {
            if self.offset >= self.size {
                return Ok(0);
            }
            let end = (self.offset + CHUNK).min(self.size) - 1;
            self.chunk = self
                .drive
                .get_range(self.key, ByteRange::new(self.offset, Some(end)))
                .map_err(io::Error::other)?;
            if self.chunk.is_empty() {
                return Err(io::Error::other(format!(
                    "\"{}\" ended at byte {} of {}",
                    self.key, self.offset, self.size
                )));
            }
            self.offset += self.chunk.len() as u64;
            self.at = 0;
        }
        let n = buf.len().min(self.chunk.len() - self.at);
        buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// A reader that hashes what passes through it.
struct Hashing<R> {
    inner: R,
    hasher: blake3::Hasher,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

/// What became of one plaintext file.
enum Outcome {
    /// Moved: its bytes.
    Moved(u64),
    /// Left where it is: why.
    Skipped(String),
    /// It changed while it was moved: left for the next run.
    Again,
}

/// The BLAKE3 of the plaintext object `object`, streamed.
fn hash_of(bucket: &dyn Drive, object: &ObjectInfo) -> Result<blake3::Hash, DriveError> {
    let mut reader = Hashing {
        inner: Ranges {
            drive: bucket,
            key: &object.key,
            size: object.size,
            offset: 0,
            chunk: Vec::new(),
            at: 0,
        },
        hasher: blake3::Hasher::new(),
    };
    io::copy(&mut reader, &mut io::sink())?;
    Ok(reader.hasher.finalize())
}

/// Whether the plaintext object is still the version the listing showed.
fn unchanged(bucket: &dyn Drive, object: &ObjectInfo) -> Result<bool, DriveError> {
    let now = match bucket.head(&object.key) {
        Ok(now) => now,
        Err(DriveError::NotFound { .. }) => return Ok(false),
        Err(e) => return Err(e),
    };
    Ok(now.size == object.size && now.etag == object.etag && now.modified == object.modified)
}

/// Moves one plaintext file.
fn move_one<D: Drive>(
    encrypted: &EncryptedDrive<D>,
    object: &ObjectInfo,
) -> Result<Outcome, DriveError> {
    let bucket: &dyn Drive = encrypted.inner();
    let key = object.key.as_str();
    if key.ends_with('/') {
        return match encrypted.create_folder(key) {
            Ok(()) => {
                bucket.delete(key)?;
                Ok(Outcome::Moved(0))
            }
            Err(DriveError::InvalidKey { reason, .. }) => Ok(Outcome::Skipped(format!(
                "an encrypted drive cannot name this folder: {reason}"
            ))),
            Err(e) => Err(e),
        };
    }
    if let Err(DriveError::InvalidKey { reason, .. }) = check_path_key(key) {
        return Ok(Outcome::Skipped(format!(
            "an encrypted drive cannot name this file: {reason}"
        )));
    }
    match encrypted.entry(key) {
        // An earlier run moved it and stopped before the delete - or the path was written
        // encrypted since: the bytes decide.
        Ok(entry) => {
            let hash = hash_of(bucket, object)?;
            let same = entry
                .object
                .as_ref()
                .is_some_and(|stored| hash == stored.blake3);
            if !same {
                return Ok(Outcome::Skipped(String::from(
                    "the encrypted drive has other contents under this name: both are kept",
                )));
            }
            if !unchanged(bucket, object)? {
                return Ok(Outcome::Again);
            }
            bucket.delete(key)?;
            Ok(Outcome::Moved(object.size))
        }
        Err(DriveError::NotFound { .. }) => {
            let mut reader = Hashing {
                inner: Ranges {
                    drive: bucket,
                    key,
                    size: object.size,
                    offset: 0,
                    chunk: Vec::new(),
                    at: 0,
                },
                hasher: blake3::Hasher::new(),
            };
            match encrypted.write_from(key, &mut reader, Expect::Absent) {
                Ok(_) => {}
                // Written encrypted meanwhile (another device): the next run compares.
                Err(DriveError::Conflict { .. }) => return Ok(Outcome::Again),
                Err(e) => return Err(e),
            }
            let read = reader.hasher.finalize();
            let stored = encrypted.entry(key)?.object;
            let checked = stored.as_ref().is_some_and(|stored| read == stored.blake3);
            if !checked || !unchanged(bucket, object)? {
                // Not the bytes the bucket holds now: the encrypted copy goes, the plaintext
                // stays for the next run.
                encrypted.delete(key)?;
                return Ok(Outcome::Again);
            }
            bucket.delete(key)?;
            Ok(Outcome::Moved(object.size))
        }
        Err(DriveError::InvalidKey { reason, .. }) => Ok(Outcome::Skipped(format!(
            "an encrypted drive cannot name this file: {reason}"
        ))),
        Err(e) => Err(e),
    }
}

/// Moves the plaintext files of `encrypted`'s bucket into it, one at a time, handing `save`
/// the state after every file (keep it: a stopped run resumes from it). `stop` is asked before
/// every file. `Ok(true)` when every plaintext file was moved or skipped (`state.done`),
/// `Ok(false)` when `stop` stopped it or files changed while they moved (run it again). An
/// error (no answer from the bucket, a full disk) stops the run; the state saved so far stays
/// good.
pub fn migrate<D: Drive>(
    encrypted: &EncryptedDrive<D>,
    state: &mut MigrationState,
    save: &mut dyn FnMut(&MigrationState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    let bucket: &dyn Drive = encrypted.inner();
    let plaintext: Vec<ObjectInfo> = list_all(bucket, "")?
        .into_iter()
        .filter(|o| is_plaintext_key(&o.key) && !state.skipped.contains_key(&o.key))
        .collect();
    let mut again = false;
    for object in &plaintext {
        if stop() {
            return Ok(false);
        }
        match move_one(encrypted, object)? {
            Outcome::Moved(bytes) => {
                state.moved += 1;
                state.bytes += bytes;
            }
            Outcome::Skipped(why) => {
                state.skipped.insert(object.key.clone(), why);
            }
            Outcome::Again => again = true,
        }
        save(state)?;
    }
    if again {
        return Ok(false);
    }
    state.done = true;
    save(state)?;
    Ok(true)
}
