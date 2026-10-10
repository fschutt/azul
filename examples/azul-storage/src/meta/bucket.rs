//! The bucket as the metadata repository sees it: whole objects with versions,
//! conditional reads and conditional writes ([`Bucket`]).
//!
//! - [`MemoryBucket`]: in memory, with S3's conditional semantics, the requests it
//!   answered counted, and a hook to run another device's write just before a
//!   swap (the tests' races).
//! - [`FolderBucket`]: a folder on disk (a USB disk, a NAS share, the tests); the
//!   conditional writes hold an OS file lock next to the object.
//! - [`DriveBucket`]: any [`Drive`] whose writes can be conditional ([`Drive::put_if`],
//!   S3's `If-None-Match: *` / `If-Match`). Its conditional read is a HEAD, then a GET
//!   when the version moved.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::MetaError;
use crate::{
    key::check_path_key, local::write_atomically, sigv4::sha256_hex, ByteRange, Drive,
    DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

/// An object's version as the bucket names it (S3: the entity tag without its
/// quotes). Opaque: compared, never parsed.
pub type Version = String;

/// What a conditional read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// Still the version the caller knows (S3: 304 Not Modified).
    NotModified,
    /// No object has the key.
    Missing,
    /// Another version, with its bytes.
    Changed { bytes: Vec<u8>, version: Version },
}

/// Whole objects with versions. Every call blocks.
pub trait Bucket: Send + Sync {
    /// The object and its version; `None` when no object has the key.
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError>;
    /// The object only when it is no longer `known` (S3: GET with `If-None-Match`).
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError>;
    /// Part of an object (S3: GET with `Range`).
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError>;
    /// Writes the object only when no object has the key (S3: `If-None-Match: *`).
    /// The new version when the bucket tells it; [`MetaError::Conflict`] (nothing
    /// written) when an object has the key.
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError>;
    /// Replaces the object only when it is still `known` (S3: `If-Match`). The new
    /// version when the bucket tells it; [`MetaError::Conflict`] (nothing written)
    /// when it changed or is gone.
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError>;
    /// Removes the object; a missing object is not an error.
    fn remove(&self, key: &str) -> Result<(), MetaError>;
}

impl<B: Bucket + ?Sized> Bucket for Arc<B> {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        (**self).read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        (**self).read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        (**self).read_range(key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        (**self).create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        (**self).replace(key, bytes, known)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        (**self).remove(key)
    }
}

/// The bytes of `range` in an object of `len` bytes (S3's rules: the end is
/// clamped, a start past the end is an error).
fn slice_range(key: &str, bytes: &[u8], range: ByteRange) -> Result<Vec<u8>, MetaError> {
    let len = bytes.len() as u64;
    if range.start >= len {
        return Err(MetaError::Drive(DriveError::InvalidRange {
            key: key.to_string(),
        }));
    }
    let end = range.end.map_or(len - 1, |end| end.min(len - 1));
    if end < range.start {
        return Err(MetaError::Drive(DriveError::InvalidRange {
            key: key.to_string(),
        }));
    }
    Ok(bytes[range.start as usize..=end as usize].to_vec())
}

/// How many requests of each kind a [`MemoryBucket`] answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestCounts {
    /// Plain reads (GET).
    pub reads: u64,
    /// Conditional reads (GET with `If-None-Match`), whatever they answered.
    pub conditional_reads: u64,
    /// Conditional reads that answered "not modified".
    pub not_modified: u64,
    /// Ranged reads.
    pub range_reads: u64,
    /// HEAD requests (through [`Drive::head`]).
    pub heads: u64,
    /// Listings (through [`Drive::list`]).
    pub lists: u64,
    /// Writes of every kind: create, replace, plain put.
    pub writes: u64,
    /// Conditional writes that lost.
    pub conflicts: u64,
    /// Removes.
    pub removes: u64,
}

/// Runs before a conditional write: another device's turn.
type Hook = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct MemoryState {
    objects: BTreeMap<String, (Vec<u8>, Version)>,
    next_version: u64,
    counts: RequestCounts,
    /// The keys of the whole reads, in order.
    whole_reads: Vec<String>,
    /// Runs before the next `replace` of the key (then removed).
    before_replace: BTreeMap<String, Hook>,
    /// The next create or replace of the key fails with this, writing nothing.
    fail_next: BTreeMap<String, MetaError>,
}

impl MemoryState {
    fn new_version(&mut self) -> Version {
        self.next_version += 1;
        format!("v{}", self.next_version)
    }
}

/// A bucket in memory with S3's conditional semantics. A clone is the same
/// bucket (two devices in a test share one). Every write makes a new version,
/// even of the same bytes.
#[derive(Clone, Default)]
pub struct MemoryBucket {
    state: Arc<Mutex<MemoryState>>,
}

impl MemoryBucket {
    #[must_use]
    pub fn new() -> Self {
        MemoryBucket::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The requests answered so far.
    #[must_use]
    pub fn counts(&self) -> RequestCounts {
        self.lock().counts
    }

    /// The keys every whole read (not a ranged one) asked for, in order.
    #[must_use]
    pub fn whole_reads(&self) -> Vec<String> {
        self.lock().whole_reads.clone()
    }

    /// Every object: key and bytes, in key order.
    #[must_use]
    pub fn objects(&self) -> Vec<(String, Vec<u8>)> {
        self.lock()
            .objects
            .iter()
            .map(|(key, (bytes, _))| (key.clone(), bytes.clone()))
            .collect()
    }

    /// Runs `hook` just before the next [`Bucket::replace`] of `key` checks its
    /// condition, outside the bucket's lock: whatever it writes comes first.
    pub fn before_next_replace(&self, key: &str, hook: impl FnOnce() + Send + 'static) {
        self.lock()
            .before_replace
            .insert(key.to_string(), Box::new(hook));
    }

    /// Makes the next create or replace of `key` fail with `error`, writing
    /// nothing (a test's 409 or lost connection).
    pub fn fail_next_write(&self, key: &str, error: MetaError) {
        self.lock().fail_next.insert(key.to_string(), error);
    }

    /// Writes the object whatever is there (a test's tampering).
    pub fn overwrite(&self, key: &str, bytes: &[u8]) {
        let mut state = self.lock();
        let version = state.new_version();
        state
            .objects
            .insert(key.to_string(), (bytes.to_vec(), version));
    }
}

impl Bucket for MemoryBucket {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        let mut state = self.lock();
        state.counts.reads += 1;
        state.whole_reads.push(key.to_string());
        Ok(state.objects.get(key).cloned())
    }

    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        let mut state = self.lock();
        state.counts.conditional_reads += 1;
        let found = state.objects.get(key).cloned();
        Ok(match found {
            None => Fetched::Missing,
            Some((_, version)) if version == known => {
                state.counts.not_modified += 1;
                Fetched::NotModified
            }
            Some((bytes, version)) => Fetched::Changed { bytes, version },
        })
    }

    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        let mut state = self.lock();
        state.counts.range_reads += 1;
        match state.objects.get(key) {
            None => Err(MetaError::Drive(DriveError::NotFound {
                key: key.to_string(),
            })),
            Some((bytes, _)) => slice_range(key, bytes, range),
        }
    }

    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        let mut state = self.lock();
        state.counts.writes += 1;
        if let Some(error) = state.fail_next.remove(key) {
            return Err(error);
        }
        if state.objects.contains_key(key) {
            state.counts.conflicts += 1;
            return Err(MetaError::Conflict {
                key: key.to_string(),
            });
        }
        let version = state.new_version();
        state
            .objects
            .insert(key.to_string(), (bytes.to_vec(), version.clone()));
        Ok(Some(version))
    }

    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        let hook = self.lock().before_replace.remove(key);
        if let Some(hook) = hook {
            hook();
        }
        let mut state = self.lock();
        state.counts.writes += 1;
        if let Some(error) = state.fail_next.remove(key) {
            return Err(error);
        }
        let current = state.objects.get(key).map(|(_, version)| version.clone());
        if current.as_deref() != Some(known) {
            state.counts.conflicts += 1;
            return Err(MetaError::Conflict {
                key: key.to_string(),
            });
        }
        let version = state.new_version();
        state
            .objects
            .insert(key.to_string(), (bytes.to_vec(), version.clone()));
        Ok(Some(version))
    }

    fn remove(&self, key: &str) -> Result<(), MetaError> {
        let mut state = self.lock();
        state.counts.removes += 1;
        state.objects.remove(key);
        Ok(())
    }
}

/// The same bucket as a [`Drive`] (S3 semantics, entity tags = versions), to
/// test [`DriveBucket`] and to list what is there.
impl Drive for MemoryBucket {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let mut state = self.lock();
        state.counts.lists += 1;
        let mut page = ListPage::default();
        let mut count = 0u32;
        // The last entry of the page, where the next one continues.
        let mut last: Option<String> = None;
        for (key, (bytes, version)) in state.objects.range(request.prefix.clone()..) {
            if !key.starts_with(&request.prefix) {
                break;
            }
            // After a folder (`a/`), everything in it was that folder.
            if request.continuation.as_deref().is_some_and(|after| {
                key.as_str() <= after || (after.ends_with('/') && key.starts_with(after))
            }) {
                continue;
            }
            let rest = &key[request.prefix.len()..];
            let folder = request
                .delimiter
                .as_deref()
                .filter(|d| !d.is_empty())
                .and_then(|d| rest.find(d).map(|at| at + d.len()))
                .map(|end| format!("{}{}", request.prefix, &rest[..end]));
            if folder.is_some() && folder == last {
                continue;
            }
            if count == request.page_size() {
                page.next = last;
                break;
            }
            count += 1;
            match folder {
                Some(folder) => {
                    last = Some(folder.clone());
                    page.folders.push(folder);
                }
                None => {
                    last = Some(key.clone());
                    page.objects.push(ObjectInfo {
                        key: key.clone(),
                        size: bytes.len() as u64,
                        modified: None,
                        etag: Some(version.clone()),
                    });
                }
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let mut state = self.lock();
        state.counts.reads += 1;
        state.whole_reads.push(key.to_string());
        state
            .objects
            .get(key)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        Bucket::read_range(self, key, range).map_err(|e| match e {
            MetaError::Drive(e) => e,
            other => DriveError::Protocol(other.to_string()),
        })
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let mut state = self.lock();
        state.counts.writes += 1;
        let version = state.new_version();
        state
            .objects
            .insert(key.to_string(), (bytes.to_vec(), version));
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let mut state = self.lock();
        state.counts.removes += 1;
        state.objects.remove(key);
        Ok(())
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let mut state = self.lock();
        state.counts.heads += 1;
        state
            .objects
            .get(key)
            .map(|(bytes, version)| ObjectInfo {
                key: key.to_string(),
                size: bytes.len() as u64,
                modified: None,
                etag: Some(version.clone()),
            })
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let written = match condition {
            Precondition::Absent => Bucket::create(self, key, bytes),
            Precondition::Matches(etag) => Bucket::replace(self, key, bytes, etag),
        };
        written.map_err(|e| match e {
            MetaError::Conflict { key } => DriveError::Conflict { key },
            MetaError::Drive(e) => e,
            other => DriveError::Protocol(other.to_string()),
        })
    }
}

/// A [`Drive`] with conditional writes ([`Drive::put_if`]: S3's
/// `If-None-Match: *` and `If-Match`) as a [`Bucket`]: an S3 bucket (Azlin,
/// AWS, R2, MinIO). A drive without them (a folder on disk) answers every
/// create and swap with its `DriveError::Unsupported`.
///
/// Its versions are the drive's entity tags. A read is a HEAD, then a GET: the
/// bytes are at least as new as the version, so a swap made with that version
/// fails when anything changed in between (never the other way round). The
/// conditional read is a HEAD, plus a GET only when the version moved: the same
/// one request as S3's 304 when nothing changed.
pub struct DriveBucket<D> {
    drive: D,
}

impl<D> DriveBucket<D> {
    #[must_use]
    pub fn new(drive: D) -> Self {
        DriveBucket { drive }
    }

    #[must_use]
    pub fn drive(&self) -> &D {
        &self.drive
    }
}

impl<D: Drive> DriveBucket<D> {
    /// The object's version, `None` when there is no object.
    fn version_of(&self, key: &str) -> Result<Option<Version>, MetaError> {
        match self.drive.head(key) {
            Ok(info) => match info.etag {
                Some(etag) => Ok(Some(etag)),
                None => Err(MetaError::Unsupported(format!(
                    "the drive shows no version of \"{key}\""
                ))),
            },
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(MetaError::Drive(e)),
        }
    }
}

impl<D: Drive> Bucket for DriveBucket<D> {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        let Some(version) = self.version_of(key)? else {
            return Ok(None);
        };
        match self.drive.get(key) {
            Ok(bytes) => Ok(Some((bytes, version))),
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(MetaError::Drive(e)),
        }
    }

    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        let Some(version) = self.version_of(key)? else {
            return Ok(Fetched::Missing);
        };
        if version == known {
            return Ok(Fetched::NotModified);
        }
        match self.drive.get(key) {
            Ok(bytes) => Ok(Fetched::Changed { bytes, version }),
            Err(DriveError::NotFound { .. }) => Ok(Fetched::Missing),
            Err(e) => Err(MetaError::Drive(e)),
        }
    }

    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        self.drive.get_range(key, range).map_err(MetaError::Drive)
    }

    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        self.drive
            .put_if(key, bytes, &Precondition::Absent)
            .map_err(|e| conditional_error(key, e))
    }

    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        self.drive
            .put_if(key, bytes, &Precondition::Matches(known.to_string()))
            .map_err(|e| conditional_error(key, e))
    }

    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.drive.delete(key).map_err(MetaError::Drive)
    }
}

/// What a lost conditional write of a drive is for the repository: 412 is
/// [`MetaError::Conflict`]; 409 (S3's `ConditionalRequestConflict`: another
/// conditional write of the object was in progress, nothing was written) is
/// [`MetaError::Raced`], to be tried again.
fn conditional_error(key: &str, e: DriveError) -> MetaError {
    match e {
        DriveError::Service(service) if service.status == 409 => MetaError::Raced {
            key: key.to_string(),
        },
        other => MetaError::from(other),
    }
}

/// An exclusive OS lock on `<object>.lock` (`flock` on Unix, `LockFileEx` on
/// Windows), held while the value lives. The system lets go of the lock of a
/// writer that dies, so there is no stale lock to judge by its age. The lock
/// file stays: deleting it while another writer waits on it would let a third
/// one lock a new file of the same name.
struct FileLock {
    _file: File,
}

impl FileLock {
    /// Waits for the lock (its holders hold it for one compare and one write).
    fn acquire(object: &Path) -> Result<FileLock, MetaError> {
        let mut name = object.file_name().unwrap_or_default().to_os_string();
        name.push(".lock");
        let path = object.with_file_name(name);
        let failed =
            |e: std::io::Error| MetaError::Drive(DriveError::Io(format!("{}: {e}", path.display())));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(&failed)?;
        file.lock().map_err(&failed)?;
        Ok(FileLock { _file: file })
    }
}

/// A folder on disk as a bucket: the key `a/b` is the file `<root>/a/b`.
///
/// A version is the first 128 bits of the SHA-256 of the file's bytes (sealed
/// objects never repeat their bytes). A conditional write takes an exclusive
/// OS lock on `<object>.lock`, checks, writes through a temporary file and a
/// rename, and lets go: correct between the processes of a computer, and
/// between computers as far as the share's file locks reach (a USB disk moved
/// between them, a share whose server honours them).
#[derive(Debug, Clone)]
pub struct FolderBucket {
    root: PathBuf,
}

fn version_of_bytes(bytes: &[u8]) -> Version {
    sha256_hex(bytes)[..32].to_string()
}

fn io_error(key: &str, e: &std::io::Error) -> MetaError {
    MetaError::Drive(DriveError::Io(format!("{key}: {e}")))
}

impl FolderBucket {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FolderBucket { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_of(&self, key: &str) -> Result<PathBuf, MetaError> {
        check_path_key(key).map_err(MetaError::Drive)?;
        let mut path = self.root.clone();
        for segment in key.split('/') {
            path.push(segment);
        }
        Ok(path)
    }

    fn read_file(key: &str, path: &Path) -> Result<Option<Vec<u8>>, MetaError> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error(key, &e)),
        }
    }

    fn write_file(key: &str, path: &Path, bytes: &[u8]) -> Result<Version, MetaError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(key, &e))?;
        }
        write_atomically(path, bytes).map_err(|e| io_error(key, &e))?;
        Ok(version_of_bytes(bytes))
    }
}

impl Bucket for FolderBucket {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        let path = self.path_of(key)?;
        Ok(Self::read_file(key, &path)?.map(|bytes| {
            let version = version_of_bytes(&bytes);
            (bytes, version)
        }))
    }

    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        Ok(match self.read(key)? {
            None => Fetched::Missing,
            Some((_, version)) if version == known => Fetched::NotModified,
            Some((bytes, version)) => Fetched::Changed { bytes, version },
        })
    }

    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        let path = self.path_of(key)?;
        match Self::read_file(key, &path)? {
            None => Err(MetaError::Drive(DriveError::NotFound {
                key: key.to_string(),
            })),
            Some(bytes) => slice_range(key, &bytes, range),
        }
    }

    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        let path = self.path_of(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(key, &e))?;
        }
        let _lock = FileLock::acquire(&path)?;
        if path.exists() {
            return Err(MetaError::Conflict {
                key: key.to_string(),
            });
        }
        Self::write_file(key, &path, bytes).map(Some)
    }

    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        let path = self.path_of(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(key, &e))?;
        }
        let _lock = FileLock::acquire(&path)?;
        let current = Self::read_file(key, &path)?.map(|bytes| version_of_bytes(&bytes));
        if current.as_deref() != Some(known) {
            return Err(MetaError::Conflict {
                key: key.to_string(),
            });
        }
        Self::write_file(key, &path, bytes).map(Some)
    }

    fn remove(&self, key: &str) -> Result<(), MetaError> {
        let path = self.path_of(key)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error(key, &e)),
        }
    }
}
