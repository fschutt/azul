//! A drive whose bucket holds nothing but ciphertext under random names: [`EncryptedDrive`],
//! a decorator over any [`Drive`] (the pattern of [`crate::ScopedDrive`]).
//!
//! - Every file version is one AZL1 object ([`crate::crypto::azl1`]) at a random key,
//!   `data/<2 hex digits>/<32 hex digits>`. Writing a file writes a new object; nothing is
//!   ever rewritten in place.
//! - Names, paths, folders, sizes and dates live in a [`NameIndex`]: path -> [`IndexEntry`]
//!   (the object, the plaintext size, the date, the BLAKE3 of the plaintext, the file key
//!   wrapped with the drive key). The bucket's encrypted metadata git repository will
//!   implement it; [`MemoryIndex`] keeps it in memory (the tests, a drive used for one
//!   session).
//! - Listing and `head` ask the index only. A read asks the index for the object and its
//!   wrapped file key, then the bucket for the object: a whole file in one GET; a range in the
//!   header and the tail (two small ranged GETs, cached per object: objects never change) and
//!   one ranged GET of the segments that cover it.
//! - Rename and copy change the index only; the ciphertext stays where it is. A copy names the
//!   same object twice: objects are immutable, so a later write to either path makes a new
//!   object and leaves the other path as it was. A delete or an overwrite removes an object
//!   from the bucket once the index says nothing names it any more (the objects
//!   [`NameIndex::apply`] releases; an index that keeps history releases none and leaves the
//!   old objects to its own collector).
//! - A write puts the object first - conditionally (`If-None-Match: *`) where the inner drive
//!   can, a plain PUT where it cannot (the random id makes a collision a non-event) - and then
//!   the index entry, as a compare-and-swap in the index: [`Drive::put_if`] works whatever the
//!   inner drive supports. A crash between the two leaves an object nothing names (garbage for
//!   a collector), never a name without its object; an index entry that lost its race takes
//!   its object back out of the bucket.
//! - [`EncryptedDrive::write_from`] streams: one segment of plaintext in memory, the object
//!   put together in a spool (memory up to 8 MiB, then a temporary file of ciphertext only,
//!   because the header is written last) and handed to the inner drive's `put_from`.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use crate::{
    crypto::{
        azl1::{self, Azl1Reader, ObjectSource, ObjectSummary, OpenObject, WriteOptions},
        device, random_bytes, to_hex, CryptoError, DriveKey, KeyId, ObjectId, ShareKey,
        WrappedKey,
    },
    key::{check_path_key, check_path_prefix},
    keyring::KeyringStore,
    ops::check_folder,
    time::now_unix,
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
    DEFAULT_PAGE_SIZE,
};

/// The object a file version's bytes are in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredObject {
    pub id: ObjectId,
    /// The object's bytes in the bucket.
    pub stored_size: u64,
    /// BLAKE3 of the plaintext.
    pub blake3: [u8; 32],
    /// The file key, wrapped with the drive key.
    pub wrapped_key: WrappedKey,
    /// Whether a segment is compressed.
    pub compressed: bool,
}

/// What the index keeps for one path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    /// The plaintext's bytes (what quotas count); 0 for a folder marker.
    pub size: u64,
    /// Last modified, in seconds since 1970-01-01 UTC.
    pub modified: Option<u64>,
    /// The AZL1 object; `None` for a folder marker (a path ending in `/`).
    pub object: Option<StoredObject>,
}

impl IndexEntry {
    /// The id of the entry's object.
    #[must_use]
    pub fn object_id(&self) -> Option<ObjectId> {
        self.object.as_ref().map(|object| object.id)
    }
}

/// What an index change expects of its path before it applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Whatever is there.
    Any,
    /// Nothing is there.
    Absent,
    /// Something is there.
    Present,
    /// The entry names this object (the version a writer read).
    Object(ObjectId),
}

impl Expect {
    /// Whether the expectation holds of `current`.
    #[must_use]
    pub fn holds(&self, current: Option<&IndexEntry>) -> bool {
        match self {
            Expect::Any => true,
            Expect::Absent => current.is_none(),
            Expect::Present => current.is_some(),
            Expect::Object(id) => current.and_then(IndexEntry::object_id) == Some(*id),
        }
    }
}

/// One change of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexChange {
    /// `path` names `entry` from now on.
    Put {
        path: String,
        entry: IndexEntry,
        expect: Expect,
    },
    /// `path` names nothing from now on (nothing there is fine with [`Expect::Any`]).
    Remove { path: String, expect: Expect },
}

impl IndexChange {
    fn path(&self) -> &str {
        match self {
            IndexChange::Put { path, .. } | IndexChange::Remove { path, .. } => path,
        }
    }

    fn expect(&self) -> &Expect {
        match self {
            IndexChange::Put { expect, .. } | IndexChange::Remove { expect, .. } => expect,
        }
    }
}

/// One page of an index listing: S3 semantics, the folders being common prefixes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexPage {
    pub folders: Vec<String>,
    pub entries: Vec<(String, IndexEntry)>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<String>,
}

/// Where an encrypted drive keeps its names: path -> entry. Every call blocks.
pub trait NameIndex: Send + Sync {
    /// The entry of `path`, if any.
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError>;

    /// One page of the entries under `request.prefix` (S3 ListObjectsV2 semantics).
    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError>;

    /// Applies `changes` all together or not at all, in their order (a batch may remove a
    /// path and put another, as a rename does): [`DriveError::Conflict`] naming the path whose
    /// expectation did not hold, and nothing changed. Returns the objects that nothing in the
    /// index names any more, for the drive to delete from the bucket; an index that keeps
    /// history returns none.
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<ObjectId>, DriveError>;
}

/// Opens the persistent [`NameIndex`] of an encrypted drive: the bucket's encrypted metadata
/// repository implements it (the metadata repository module); an app hands its provider to
/// [`open_encrypted`]. Nothing in the apps uses a [`MemoryIndex`]: it forgets every name when
/// the app closes, and the objects would stay in the bucket nameless.
pub trait IndexProvider: Send + Sync {
    /// The index of the drive `drive` (its id) whose bucket is `bucket`, its own objects sealed
    /// with keys derived from `drive_key`.
    fn open_index(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError>;
}

/// The encrypted drive `drive` over its bucket: the drive key from this device's keyring (else
/// its member wrap in the bucket, [`crate::crypto::device::unlock`]), the index from
/// `provider`. `Denied` when this device holds no key for the drive yet: it joins with a code
/// from a device that does, or unlocks with the recovery code.
pub fn open_encrypted(
    bucket: Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    provider: &dyn IndexProvider,
) -> Result<EncryptedDrive<Arc<dyn Drive>>, DriveError> {
    let Some(drive_key) = device::unlock(bucket.as_ref(), keyring, drive)? else {
        return Err(DriveError::Denied {
            message: format!(
                "this device has no key for \"{drive}\" yet: join it with a code from a device \
                 that has one, or unlock it with the recovery code"
            ),
        });
    };
    let index = provider.open_index(drive, Arc::clone(&bucket), &drive_key)?;
    Ok(EncryptedDrive::new(bucket, drive_key, index))
}

/// A drive's bucket that may hold an encrypted drive, for an app that opens drives on its UI
/// thread and calls them on worker threads: the FIRST call (on a worker thread) decides. The
/// drive is encrypted when this device keeps its key (sticky: a bucket that hides its key files
/// cannot turn this device back to plaintext) or the bucket holds key files; it is then used
/// through [`open_encrypted`] (the index from `provider`; without one it is refused - this
/// build cannot name its files), else as the plain bucket. A failed decision is tried again on
/// the next call. A drive encrypted by another device while this one is open stays plain here
/// until it is opened again.
pub struct AutoEncrypted {
    bucket: Arc<dyn Drive>,
    drive: String,
    keyring: Arc<dyn KeyringStore>,
    provider: Option<Arc<dyn IndexProvider>>,
    opened: Mutex<Option<Arc<dyn Drive>>>,
}

impl fmt::Debug for AutoEncrypted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AutoEncrypted")
            .field("drive", &self.drive)
            .field("has_provider", &self.provider.is_some())
            .finish_non_exhaustive()
    }
}

impl AutoEncrypted {
    /// The bucket of the drive `drive` (its id), the keyring its key is kept in, the provider
    /// of encrypted drives' indexes (`None`: this build opens plain drives only). Sends nothing.
    #[must_use]
    pub fn new(
        bucket: Arc<dyn Drive>,
        drive: &str,
        keyring: Arc<dyn KeyringStore>,
        provider: Option<Arc<dyn IndexProvider>>,
    ) -> AutoEncrypted {
        AutoEncrypted {
            bucket,
            drive: drive.to_string(),
            keyring,
            provider,
            opened: Mutex::new(None),
        }
    }

    /// The plain bucket, below any encryption (the keys live there).
    #[must_use]
    pub fn bucket(&self) -> &Arc<dyn Drive> {
        &self.bucket
    }

    /// Forgets the decision: the next call decides again (after the drive was encrypted, or
    /// this device got its key).
    pub fn reopen(&self) {
        *lock(&self.opened) = None;
    }

    /// Whether the decision so far is "encrypted" (`None`: not decided yet).
    #[must_use]
    pub fn is_encrypted(&self) -> Option<bool> {
        lock(&self.opened)
            .as_ref()
            .map(|drive| !Arc::ptr_eq(drive, &self.bucket))
    }

    /// The drive every call goes to: decided on the first call.
    fn resolved(&self) -> Result<Arc<dyn Drive>, DriveError> {
        let mut opened = lock(&self.opened);
        if let Some(drive) = opened.as_ref() {
            return Ok(Arc::clone(drive));
        }
        // A keyring that cannot be read (no keyring on this system, a declined prompt) leaves
        // the bucket's word; an encrypted drive asks the keyring again for its key.
        let kept = device::load_drive_key(self.keyring.as_ref(), &self.drive)
            .unwrap_or(None)
            .is_some();
        let encrypted = kept || device::is_encrypted(self.bucket.as_ref())?;
        let drive: Arc<dyn Drive> = if encrypted {
            let Some(provider) = &self.provider else {
                return Err(DriveError::Unsupported(String::from(
                    "this drive is encrypted, and this build of the app has no drive index to \
                     open encrypted drives with",
                )));
            };
            Arc::new(open_encrypted(
                Arc::clone(&self.bucket),
                self.keyring.as_ref(),
                &self.drive,
                provider.as_ref(),
            )?)
        } else {
            Arc::clone(&self.bucket)
        };
        *opened = Some(Arc::clone(&drive));
        Ok(drive)
    }
}

impl Drive for AutoEncrypted {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.resolved()?.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.resolved()?.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.resolved()?.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.resolved()?.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.resolved()?.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.resolved()?.head(key)
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.resolved()?.put_from(key, body)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.resolved()?.put_if(key, bytes, condition)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.resolved()?.copy(from, to)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.resolved()?.create_folder(prefix)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.resolved()?.rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.resolved()?.delete_folder(prefix)
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.resolved()?.metadata(key)
    }
}

/// The mutex's value, also after a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An index in memory: for the tests, and for a drive used during one session only. Releasing
/// objects looks through every entry (fine for thousands of files, not for millions).
#[derive(Default)]
pub struct MemoryIndex {
    entries: Mutex<BTreeMap<String, IndexEntry>>,
}

impl MemoryIndex {
    #[must_use]
    pub fn new() -> MemoryIndex {
        MemoryIndex::default()
    }

    /// Entries (files and folder markers).
    #[must_use]
    pub fn len(&self) -> usize {
        lock(&self.entries).len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        lock(&self.entries).is_empty()
    }
}

impl NameIndex for MemoryIndex {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        Ok(lock(&self.entries).get(path).cloned())
    }

    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        let entries = lock(&self.entries);
        // (key, entry): an entry, or `None` for a folder (a common prefix).
        let mut found: Vec<(String, Option<IndexEntry>)> = Vec::new();
        for (path, entry) in entries.range(request.prefix.clone()..) {
            if !path.starts_with(&request.prefix) {
                break;
            }
            let rest = &path[request.prefix.len()..];
            let folder = request
                .delimiter
                .as_deref()
                .filter(|d| !d.is_empty())
                .and_then(|d| rest.find(d).map(|i| &rest[..i + d.len()]));
            match folder {
                Some(folder) => {
                    let folder = format!("{}{folder}", request.prefix);
                    if found.last().map(|(k, _)| k.as_str()) != Some(folder.as_str()) {
                        found.push((folder, None));
                    }
                }
                None => found.push((path.clone(), Some(entry.clone()))),
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        if let Some(after) = request.continuation.as_deref() {
            found.retain(|(key, _)| key.as_str() > after);
        }
        let size = if request.max_keys == 0 {
            DEFAULT_PAGE_SIZE as usize
        } else {
            request.max_keys as usize
        };
        let next = (found.len() > size).then(|| found[size - 1].0.clone());
        found.truncate(size);
        let mut page = IndexPage {
            next,
            ..IndexPage::default()
        };
        for (key, entry) in found {
            match entry {
                Some(entry) => page.entries.push((key, entry)),
                None => page.folders.push(key),
            }
        }
        Ok(page)
    }

    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<ObjectId>, DriveError> {
        let mut entries = lock(&self.entries);
        // Every expectation against the state the earlier changes of the batch leave.
        let mut staged: BTreeMap<String, Option<IndexEntry>> = BTreeMap::new();
        for change in &changes {
            let path = change.path();
            let current = match staged.get(path) {
                Some(staged) => staged.clone(),
                None => entries.get(path).cloned(),
            };
            if !change.expect().holds(current.as_ref()) {
                return Err(DriveError::Conflict {
                    key: path.to_string(),
                });
            }
            let next = match change {
                IndexChange::Put { entry, .. } => Some(entry.clone()),
                IndexChange::Remove { .. } => None,
            };
            staged.insert(path.to_string(), next);
        }
        let mut before: Vec<ObjectId> = staged
            .keys()
            .filter_map(|path| entries.get(path).and_then(IndexEntry::object_id))
            .collect();
        for (path, next) in staged {
            match next {
                Some(entry) => {
                    entries.insert(path, entry);
                }
                None => {
                    entries.remove(&path);
                }
            }
        }
        before.sort();
        before.dedup();
        before.retain(|id| !entries.values().any(|e| e.object_id() == Some(*id)));
        Ok(before)
    }
}

/// The listing's and `head`'s view of an entry: the plaintext size, the date, the object id
/// as the entity tag (a new one for every write; [`Precondition::Matches`] takes it).
fn info_of(path: String, entry: &IndexEntry) -> ObjectInfo {
    ObjectInfo {
        key: path,
        size: entry.size,
        modified: entry.modified,
        etag: entry.object_id().map(|id| id.to_hex()),
    }
}

/// A failed read of an object through a drive: the drive's own error, or the format's.
#[derive(Debug)]
pub enum ReadError {
    Drive(DriveError),
    Crypto(CryptoError),
}

impl From<CryptoError> for ReadError {
    fn from(e: CryptoError) -> Self {
        ReadError::Crypto(e)
    }
}

impl From<ReadError> for io::Error {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Drive(e) => io::Error::other(e),
            ReadError::Crypto(e) => e.into(),
        }
    }
}

/// An object in a drive as an AZL1 source: every read is one ranged GET.
pub struct DriveObject<'a> {
    drive: &'a dyn Drive,
    key: String,
}

impl<'a> DriveObject<'a> {
    #[must_use]
    pub fn new(drive: &'a dyn Drive, object: &ObjectId) -> DriveObject<'a> {
        DriveObject {
            drive,
            key: object.bucket_key(),
        }
    }
}

impl ObjectSource for DriveObject<'_> {
    type Error = ReadError;

    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, ReadError> {
        let cut = || ReadError::Crypto(CryptoError::Damaged(String::from("the object is cut short")));
        if len == 0 {
            return Ok(Vec::new());
        }
        let range = ByteRange::new(offset, Some(offset + len - 1));
        let bytes = match self.drive.get_range(&self.key, range) {
            Ok(bytes) => bytes,
            Err(DriveError::InvalidRange { .. }) => return Err(cut()),
            Err(e) => return Err(ReadError::Drive(e)),
        };
        if bytes.len() as u64 != len {
            return Err(cut());
        }
        Ok(bytes)
    }
}

/// A read's error as the drive's, about `path`: an object missing from the bucket is a
/// damaged drive, not a missing file.
fn read_error(e: ReadError, path: &str) -> DriveError {
    match e {
        ReadError::Drive(DriveError::NotFound { .. }) => DriveError::Corrupt {
            key: path.to_string(),
            reason: String::from("its object is missing from the bucket"),
        },
        ReadError::Drive(e) => e,
        ReadError::Crypto(e) => e.for_key(path),
    }
}

/// Bytes a spool keeps in memory before it moves to a temporary file.
const SPOOL_MEMORY: usize = 8 << 20;

/// Opened objects kept (their header and trailer read): reads of ranges of the same files
/// need no more than the GET of their segments.
const OPENED_CACHE: usize = 64;

/// Where a streamed object is put together before it goes to the inner drive (its header is
/// written last): memory up to [`SPOOL_MEMORY`] bytes, then a temporary file that holds
/// ciphertext only and is removed when the spool is dropped.
struct Spool {
    memory: Cursor<Vec<u8>>,
    file: Option<(File, PathBuf)>,
}

impl Spool {
    fn new() -> Spool {
        Spool {
            memory: Cursor::new(Vec::new()),
            file: None,
        }
    }

    /// Moves what is in memory to a new temporary file, keeping the position.
    fn move_to_file(&mut self) -> io::Result<()> {
        let mut name = [0u8; 8];
        random_bytes(&mut name)?;
        let path = std::env::temp_dir().join(format!("azul-storage-{}.azl1-spool", to_hex(&name)));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let moved = file
            .write_all(self.memory.get_ref())
            .and_then(|()| file.seek(SeekFrom::Start(self.memory.position())));
        if let Err(e) = moved {
            let _ = fs::remove_file(&path);
            return Err(e);
        }
        self.memory = Cursor::new(Vec::new());
        self.file = Some((file, path));
        Ok(())
    }
}

impl Write for Spool {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.file.is_none() && self.memory.position() as usize + buf.len() > SPOOL_MEMORY {
            self.move_to_file()?;
        }
        match &mut self.file {
            Some((file, _)) => file.write(buf),
            None => self.memory.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.file {
            Some((file, _)) => file.flush(),
            None => Ok(()),
        }
    }
}

impl Seek for Spool {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match &mut self.file {
            Some((file, _)) => file.seek(to),
            None => self.memory.seek(to),
        }
    }
}

impl Read for Spool {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.file {
            Some((file, _)) => file.read(buf),
            None => self.memory.read(buf),
        }
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        if let Some((file, path)) = self.file.take() {
            drop(file);
            let _ = fs::remove_file(path);
        }
    }
}

/// A file shared under a share key: the object and the file key wrapped for the share. With
/// the share key and the object (the bucket, or a link to the object), [`read_shared`] reads
/// this file and no other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFile {
    pub object: ObjectId,
    pub stored_size: u64,
    pub wrapped_key: WrappedKey,
}

/// The plaintext of a shared file from `bucket` (the drive's bucket, or a drive that reaches
/// the object through a link), opened with the share key.
pub fn read_shared(
    bucket: &dyn Drive,
    file: &SharedFile,
    share: &ShareKey,
) -> Result<Vec<u8>, DriveError> {
    let key = file.object.bucket_key();
    let file_key = share
        .unwrap_file_key(&file.wrapped_key, &file.object)
        .map_err(|e| e.for_key(&key))?;
    let bytes = bucket.get(&key)?;
    azl1::decrypt(&bytes, &file.object, &file_key).map_err(|e| e.for_key(&key))
}

/// `inner` (the bucket) seen through its drive key and its index: what it shows are the
/// index's names with the plaintext of their objects; what it stores are AZL1 objects under
/// random keys. See the module documentation.
pub struct EncryptedDrive<D: Drive> {
    inner: D,
    drive_key: DriveKey,
    index: Arc<dyn NameIndex>,
    options: WriteOptions,
    opened: Mutex<HashMap<ObjectId, Arc<OpenObject>>>,
}

impl<D: Drive> fmt::Debug for EncryptedDrive<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedDrive")
            .field("drive_key", &self.drive_key.id())
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<D: Drive> EncryptedDrive<D> {
    /// The bucket `inner` with the drive's key and its index.
    pub fn new(inner: D, drive_key: DriveKey, index: Arc<dyn NameIndex>) -> EncryptedDrive<D> {
        EncryptedDrive {
            inner,
            drive_key,
            index,
            options: WriteOptions::default(),
            opened: Mutex::new(HashMap::new()),
        }
    }

    /// Writes with these options (segment size, compression) from now on.
    #[must_use]
    pub fn with_options(mut self, options: WriteOptions) -> EncryptedDrive<D> {
        self.options = options;
        self
    }

    /// The bucket.
    #[must_use]
    pub fn inner(&self) -> &D {
        &self.inner
    }

    #[must_use]
    pub fn index(&self) -> &Arc<dyn NameIndex> {
        &self.index
    }

    /// The id of the drive key this drive reads and writes with.
    #[must_use]
    pub fn drive_key_id(&self) -> KeyId {
        self.drive_key.id()
    }

    /// Forgets the opened objects (their derived keys are wiped): when the drive locks.
    pub fn forget_opened(&self) {
        lock(&self.opened).clear();
    }

    /// The index entry of `path` (a file, or a folder marker ending in `/`).
    pub fn entry(&self, path: &str) -> Result<IndexEntry, DriveError> {
        if path.ends_with('/') {
            check_folder(path)?;
            check_path_prefix(path)?;
        } else {
            check_path_key(path)?;
        }
        self.index.get(path)?.ok_or_else(|| DriveError::NotFound {
            key: path.to_string(),
        })
    }

    /// The object of the file `path` (a folder marker has none: not found as a file).
    fn object_of(&self, path: &str) -> Result<StoredObject, DriveError> {
        self.entry(path)?.object.ok_or_else(|| DriveError::NotFound {
            key: path.to_string(),
        })
    }

    /// `path`'s object opened (its header and trailer read and checked against the index),
    /// from the cache when it was opened before.
    fn opened(&self, path: &str, object: &StoredObject) -> Result<Arc<OpenObject>, DriveError> {
        if let Some(open) = lock(&self.opened).get(&object.id) {
            return Ok(Arc::clone(open));
        }
        let file_key = self
            .drive_key
            .unwrap_file_key(&object.wrapped_key, &object.id)
            .map_err(|e| e.for_key(path))?;
        let source = DriveObject::new(&self.inner, &object.id);
        let open = OpenObject::open(&source, &object.id, &file_key)
            .map_err(|e| read_error(e, path))?;
        let same_hash = blake3::Hash::from_bytes(open.blake3()) == object.blake3;
        if open.object_len() != object.stored_size || !same_hash {
            return Err(DriveError::Corrupt {
                key: path.to_string(),
                reason: String::from("the object is not the one the index names"),
            });
        }
        let open = Arc::new(open);
        let mut cache = lock(&self.opened);
        if cache.len() >= OPENED_CACHE {
            cache.clear();
        }
        cache.insert(object.id, Arc::clone(&open));
        Ok(open)
    }

    /// A reader of `path`'s plaintext, a segment at a time (one ranged GET each).
    pub fn open_reader(&self, path: &str) -> Result<Azl1Reader<DriveObject<'_>>, DriveError> {
        let object = self.object_of(path)?;
        let file_key = self
            .drive_key
            .unwrap_file_key(&object.wrapped_key, &object.id)
            .map_err(|e| e.for_key(path))?;
        let source = DriveObject::new(&self.inner, &object.id);
        let open = OpenObject::open(&source, &object.id, &file_key)
            .map_err(|e| read_error(e, path))?;
        Ok(open.into_reader(source))
    }

    /// Puts a new object's bytes: conditionally where the inner drive can.
    fn put_object(&self, id: &ObjectId, bytes: &[u8]) -> Result<(), DriveError> {
        let key = id.bucket_key();
        match self.inner.put_if(&key, bytes, &Precondition::Absent) {
            Ok(_) => Ok(()),
            Err(DriveError::Unsupported(_)) => self.inner.put(&key, bytes),
            Err(DriveError::Conflict { .. }) => Err(DriveError::Protocol(String::from(
                "a new object's random id was taken: write again",
            ))),
            Err(e) => Err(e),
        }
    }

    /// Names the new object in the index (when `expect` holds); takes the object back out of
    /// the bucket when the index refuses, and deletes what the index released.
    fn bind(
        &self,
        path: &str,
        summary: ObjectSummary,
        expect: Expect,
    ) -> Result<ObjectInfo, DriveError> {
        let id = summary.object_id;
        let entry = IndexEntry {
            size: summary.plaintext_size,
            modified: Some(now_unix()),
            object: Some(StoredObject {
                id,
                stored_size: summary.object_len,
                blake3: summary.blake3,
                wrapped_key: summary.wrapped_key,
                compressed: summary.compressed,
            }),
        };
        let info = info_of(path.to_string(), &entry);
        let change = IndexChange::Put {
            path: path.to_string(),
            entry,
            expect,
        };
        match self.index.apply(vec![change]) {
            Ok(released) => {
                self.release(released);
                Ok(info)
            }
            Err(e) => {
                // Nothing names the new object: it goes.
                let _ = self.inner.delete(&id.bucket_key());
                Err(e)
            }
        }
    }

    /// Deletes released objects from the bucket. Best effort: an object the bucket refused to
    /// delete is named by nothing, garbage for a collector, not an error of the call that
    /// released it.
    fn release(&self, released: Vec<ObjectId>) {
        {
            let mut cache = lock(&self.opened);
            for id in &released {
                cache.remove(id);
            }
        }
        for id in released {
            let _ = self.inner.delete(&id.bucket_key());
        }
    }

    /// Writes `path` from `bytes` when `expect` holds of its index entry; what the listing now
    /// shows of it.
    pub fn write(&self, path: &str, bytes: &[u8], expect: Expect) -> Result<ObjectInfo, DriveError> {
        check_path_key(path)?;
        let id = ObjectId::generate().map_err(|e| e.for_key(path))?;
        let (object, summary) =
            azl1::encrypt(bytes, id, &self.drive_key, &self.options).map_err(|e| e.for_key(path))?;
        self.put_object(&id, &object)?;
        self.bind(path, summary, expect)
    }

    /// Writes `path` from what `body` reads, when `expect` holds of its index entry: one
    /// segment of plaintext in memory at a time; the object put together in a spool and
    /// handed to the inner drive's `put_from` (a conditional PUT when it fits in memory).
    pub fn write_from(
        &self,
        path: &str,
        body: &mut dyn Read,
        expect: Expect,
    ) -> Result<ObjectInfo, DriveError> {
        check_path_key(path)?;
        let id = ObjectId::generate().map_err(|e| e.for_key(path))?;
        let mut spool = Spool::new();
        let summary = azl1::encrypt_stream(body, &mut spool, id, &self.drive_key, &self.options)
            .map_err(|e| e.for_key(path))?;
        if spool.file.is_none() {
            let bytes = std::mem::take(spool.memory.get_mut());
            self.put_object(&id, &bytes)?;
        } else {
            spool.seek(SeekFrom::Start(0))?;
            self.inner.put_from(&id.bucket_key(), &mut spool)?;
        }
        drop(spool);
        self.bind(path, summary, expect)
    }

    /// `path`'s file key wrapped for `share`: what a share of the file carries. The drive key
    /// stays out of it; a share of a folder is a share of each of its files (folder keys come
    /// with the metadata repository).
    pub fn share_file(&self, path: &str, share: &ShareKey) -> Result<SharedFile, DriveError> {
        let object = self.object_of(path)?;
        let file_key = self
            .drive_key
            .unwrap_file_key(&object.wrapped_key, &object.id)
            .map_err(|e| e.for_key(path))?;
        let wrapped_key = share
            .wrap_file_key(&file_key, &object.id)
            .map_err(|e| e.for_key(path))?;
        Ok(SharedFile {
            object: object.id,
            stored_size: object.stored_size,
            wrapped_key,
        })
    }

    /// Every entry under `prefix`, across all pages.
    fn all_entries(&self, prefix: &str) -> Result<Vec<(String, IndexEntry)>, DriveError> {
        let mut out = Vec::new();
        let mut request = ListRequest::recursive(prefix);
        loop {
            let page = self.index.list(&request)?;
            out.extend(page.entries);
            match page.next {
                Some(token) => request = request.with_continuation(token),
                None => break,
            }
        }
        Ok(out)
    }

    /// What a remove or a move of `entry` expects: its object, or (a folder marker) that it is
    /// there.
    fn expect_of(entry: &IndexEntry) -> Expect {
        match entry.object_id() {
            Some(id) => Expect::Object(id),
            None => Expect::Present,
        }
    }

    fn taken(key: &str) -> DriveError {
        DriveError::InvalidKey {
            key: key.to_string(),
            reason: "something has this name already",
        }
    }

    /// Moves every entry under the folder `from` to the folder `to`, in one index change.
    fn rename_folder(&self, from: &str, to: &str) -> Result<(), DriveError> {
        for folder in [from, to] {
            check_folder(folder)?;
            check_path_prefix(folder)?;
        }
        if to.starts_with(from) {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder cannot move into itself",
            });
        }
        let moving = self.all_entries(from)?;
        if moving.is_empty() {
            return Err(DriveError::NotFound {
                key: from.to_string(),
            });
        }
        if !self.all_entries(to)?.is_empty() || self.index.get(&to[..to.len() - 1])?.is_some() {
            return Err(Self::taken(to));
        }
        let mut changes = Vec::with_capacity(moving.len() * 2);
        for (path, entry) in moving {
            changes.push(IndexChange::Remove {
                path: path.clone(),
                expect: Self::expect_of(&entry),
            });
            changes.push(IndexChange::Put {
                path: format!("{to}{}", &path[from.len()..]),
                entry,
                expect: Expect::Absent,
            });
        }
        let released = self.index.apply(changes)?;
        self.release(released);
        Ok(())
    }
}

impl<D: Drive> Drive for EncryptedDrive<D> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        check_path_prefix(&request.prefix)?;
        let page = self.index.list(request)?;
        Ok(ListPage {
            folders: page.folders,
            objects: page
                .entries
                .into_iter()
                .map(|(path, entry)| info_of(path, &entry))
                .collect(),
            next: page.next,
        })
    }

    /// One GET of the whole object; its plaintext checked against the header's hash and the
    /// index's.
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let object = self.object_of(key)?;
        let file_key = self
            .drive_key
            .unwrap_file_key(&object.wrapped_key, &object.id)
            .map_err(|e| e.for_key(key))?;
        let bytes = self
            .inner
            .get(&object.id.bucket_key())
            .map_err(|e| read_error(ReadError::Drive(e), key))?;
        let plain = azl1::decrypt(&bytes, &object.id, &file_key).map_err(|e| e.for_key(key))?;
        if blake3::hash(&plain) != object.blake3 {
            return Err(DriveError::Corrupt {
                key: key.to_string(),
                reason: String::from("the object is not the one the index names"),
            });
        }
        Ok(plain)
    }

    /// The header and the tail of the object (cached), then one ranged GET of the segments
    /// covering the range.
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let entry = self.entry(key)?;
        let Some(object) = &entry.object else {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        };
        let out_of_range = || DriveError::InvalidRange {
            key: key.to_string(),
        };
        if range.start >= entry.size {
            return Err(out_of_range());
        }
        let end = range.end.map_or(entry.size - 1, |end| end.min(entry.size - 1));
        if end < range.start {
            return Err(out_of_range());
        }
        let open = self.opened(key, object)?;
        let source = DriveObject::new(&self.inner, &object.id);
        open.read_range(&source, range.start, end)
            .map_err(|e| read_error(e, key))
    }

    /// A folder marker for a key ending in `/` and no bytes (as S3 consoles make them);
    /// otherwise a new object for the file.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if key.ends_with('/') && bytes.is_empty() {
            return self.create_folder(key);
        }
        self.write(key, bytes, Expect::Any).map(|_| ())
    }

    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.write_from(key, body, Expect::Any).map(|info| info.size)
    }

    /// Conditional on the index entry, whatever the inner drive supports: absent, or still
    /// naming the object whose id is the entity tag.
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let expect = match condition {
            Precondition::Absent => Expect::Absent,
            Precondition::Matches(etag) => match ObjectId::from_hex(etag) {
                Some(id) => Expect::Object(id),
                None => {
                    return Err(DriveError::Conflict {
                        key: key.to_string(),
                    })
                }
            },
        };
        self.write(key, bytes, expect).map(|info| info.etag)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        check_path_key(key)?;
        let released = self.index.apply(vec![IndexChange::Remove {
            path: key.to_string(),
            expect: Expect::Any,
        }])?;
        self.release(released);
        Ok(())
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let entry = self.entry(key)?;
        Ok(info_of(key.to_string(), &entry))
    }

    /// In the index only: `to` names the same object as `from` (replacing what `to` named).
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if let Some(folder) = [from, to].into_iter().find(|k| k.ends_with('/')) {
            return Err(DriveError::InvalidKey {
                key: folder.to_string(),
                reason: "a folder is copied object by object",
            });
        }
        check_path_key(to)?;
        let mut entry = self.entry(from)?;
        entry.modified = Some(now_unix());
        let released = self.index.apply(vec![IndexChange::Put {
            path: to.to_string(),
            entry,
            expect: Expect::Any,
        }])?;
        self.release(released);
        Ok(())
    }

    /// A marker in the index; the bucket sees nothing.
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        check_folder(prefix)?;
        check_path_prefix(prefix)?;
        self.index.apply(vec![IndexChange::Put {
            path: prefix.to_string(),
            entry: IndexEntry {
                size: 0,
                modified: Some(now_unix()),
                object: None,
            },
            expect: Expect::Any,
        }])?;
        Ok(())
    }

    /// In the index only: the objects stay where they are.
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if from.ends_with('/') != to.ends_with('/') {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a file and a folder cannot trade places",
            });
        }
        if from.ends_with('/') {
            return self.rename_folder(from, to);
        }
        check_path_key(to)?;
        let entry = self.entry(from)?;
        let changes = vec![
            IndexChange::Remove {
                path: from.to_string(),
                expect: Self::expect_of(&entry),
            },
            IndexChange::Put {
                path: to.to_string(),
                entry,
                expect: Expect::Absent,
            },
        ];
        let released = self.index.apply(changes).map_err(|e| match e {
            DriveError::Conflict { key } if key == to => Self::taken(to),
            other => other,
        })?;
        self.release(released);
        Ok(())
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        check_folder(prefix)?;
        check_path_prefix(prefix)?;
        let changes: Vec<IndexChange> = self
            .all_entries(prefix)?
            .into_iter()
            .map(|(path, _)| IndexChange::Remove {
                path,
                expect: Expect::Any,
            })
            .collect();
        if changes.is_empty() {
            return Ok(());
        }
        let released = self.index.apply(changes)?;
        self.release(released);
        Ok(())
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let entry = self.entry(key)?;
        let mut pairs = vec![(
            String::from("Encryption"),
            String::from("AZL1 (XChaCha20-Poly1305, on this device)"),
        )];
        if let Some(object) = &entry.object {
            pairs.push((
                String::from("Stored size"),
                format!("{} bytes", object.stored_size),
            ));
            pairs.push((
                String::from("Compressed"),
                String::from(if object.compressed { "Yes" } else { "No" }),
            ));
        }
        Ok(pairs)
    }
}
