//! `Documents`: files and folders the user owns, by locator, with typed failures (the behaviour behind `CallbackInfo::documents`).
//!
//! The data model is `azul_core::documents`. This module is the TRAIT every platform's backend implements ([`DocumentBackend`]),
//! the HANDLE an app takes from a callback and moves to a worker thread ([`Documents`]: cloneable, `Send + Sync`, no process-wide state
//! behind it), and the backend of the desktops and of everything that has paths ([`LocalDocuments`], over `std::fs`), which is also
//! what the unit tests of every consumer (a Drive over a folder, an app's browser) run against. Android's backend (a granted tree of
//! `content://` documents through `DocumentsContract`) and iOS's (security-scoped, bookmarked folders) are `azul-dll`'s
//! (`desktop/extra/documents`); each is installed into the App's services at the start (`AppServices::documents`).
//!
//! Every call BLOCKS until the platform answers (a cloud provider takes seconds): call from a worker thread. Every limit is a cap with
//! the reason in the answer (a read is at most [`MAX_READ`] bytes; a name is one plain segment), never a crash.

use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::fmt;
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        mpsc, Mutex, PoisonError,
    },
    time::{Duration, UNIX_EPOCH},
};

use azul_core::documents::{
    AllFilesAccess, DocumentEntry, DocumentEntryVec, DocumentError, DocumentOrigin, OptionDocumentOrigin, Place, PlaceKind, PlaceVec,
};
use azul_css::{AzString, EmptyStruct, OptionU64, U8Vec};

use crate::mime;

/// The most bytes one [`Documents::read`] returns: a file is read in ranges, never whole into memory by the engine.
pub const MAX_READ: u64 = 64 * 1024 * 1024;

/// The most entries one listing returns: a folder of more is cut at this many and the last entry names the rest.
pub const MAX_ENTRIES: usize = 20_000;

/// A readable, seekable stream of one document (a file descriptor on a phone, a file on a desktop).
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// What a platform does for [`Documents`].
pub trait DocumentBackend: Send + Sync + fmt::Debug {
    /// The name of the backend ("local", "android-saf"), for logs and `PlatformCapability`.
    fn name(&self) -> &'static str;
    /// The places the app may list without asking: its own storage, the folders the person granted, shared storage.
    fn places(&self) -> Vec<Place>;
    /// Forget a granted place (release the permission): the folder itself is untouched.
    fn forget(&self, locator: &str) -> Result<(), DocumentError>;
    /// What the platform says of one entry.
    fn stat(&self, locator: &str) -> Result<DocumentEntry, DocumentError>;
    /// The entries of a folder (at most [`MAX_ENTRIES`]), folders first, then by name.
    fn children(&self, locator: &str) -> Result<Vec<DocumentEntry>, DocumentError>;
    /// Up to `len` bytes of the file from `offset` (fewer at the end; none past it).
    fn read(&self, locator: &str, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError>;
    /// The whole file as a stream (for copying a big file without holding it).
    fn open_read(&self, locator: &str) -> Result<Box<dyn ReadSeek>, DocumentError>;
    /// Replace the content of an existing file with `bytes` (truncating it).
    fn write(&self, locator: &str, bytes: &[u8]) -> Result<DocumentEntry, DocumentError>;
    /// Make a new empty file called `name` of the type `mime` in the folder `parent`.
    fn create_file(&self, parent: &str, name: &str, mime: &str) -> Result<DocumentEntry, DocumentError>;
    /// Make a new folder called `name` in the folder `parent`.
    fn create_dir(&self, parent: &str, name: &str) -> Result<DocumentEntry, DocumentError>;
    /// Rename an entry in place; the answer has the new locator (it may differ from the old one).
    fn rename(&self, locator: &str, new_name: &str) -> Result<DocumentEntry, DocumentError>;
    /// Delete a file, or a folder with everything in it.
    fn delete(&self, locator: &str) -> Result<(), DocumentError>;
    /// "All files access" (Android's `MANAGE_EXTERNAL_STORAGE` of a build that asks for it).
    fn all_files_access(&self) -> AllFilesAccess;
    /// Open the system's page where the person can give it (from a user gesture); `false` when there is none.
    fn request_all_files_access(&self) -> bool;
}

/// The handle an app holds: clone it, move it to a worker, call it. Every clone shares the one backend, which is dropped with the last
/// of them.
///
/// The handle is ONE pointer to the shared backend (a `Box` around the `Arc`, so that it is thin) and a flag, like the other handles
/// of the API (`HttpClient`): that is the shape that crosses the C ABI.
#[repr(C)]
pub struct Documents {
    /// The backend, shared by every clone.
    pub ptr: Box<Arc<dyn DocumentBackend>>,
    /// The C view of the handle runs the destructor of the box only for a handle it owns.
    pub run_destructor: bool,
}

impl Clone for Documents {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for Documents {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl fmt::Debug for Documents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Documents").field("backend", &self.ptr.name()).finish_non_exhaustive()
    }
}

/// Folders first, then by name without regard to case: the order every listing of the engine has (the exact name breaks a tie, so
/// the order does not depend on the order the provider answered in).
pub fn sort_listing(entries: &mut [DocumentEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.as_str().to_lowercase().cmp(&b.name.as_str().to_lowercase()))
            .then_with(|| a.name.as_str().cmp(b.name.as_str()))
    });
}

/// Cut a listing at [`MAX_ENTRIES`]; `true` when something was cut.
pub fn cap_listing(entries: &mut Vec<DocumentEntry>) -> bool {
    let cut = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    cut
}

/// The most bytes [`read_range`] skips by reading when a stream cannot seek (a cloud provider's pipe): a range further in is
/// refused with the reason instead of reading gigabytes to throw them away.
pub const MAX_SKIP: u64 = 256 * 1024 * 1024;

/// Up to `len` bytes of `stream` from `offset` (fewer at its end, none past it). It seeks where the stream can; where it cannot (a
/// pipe) it reads and drops what comes before, up to [`MAX_SKIP`]. `locator` names the document in the failure.
///
/// # Errors
///
/// `Unsupported` for an offset a pipe would have to read too much to reach, or what the stream's I/O said.
pub fn read_range<R: Read + Seek>(stream: &mut R, locator: &str, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError> {
    if len == 0 {
        return Ok(Vec::new());
    }
    // always positioned: the stream may be anywhere (a reader that was used before); a pipe that cannot seek is at its start
    if stream.seek(SeekFrom::Start(offset)).is_err() && offset > 0 {
        if offset > MAX_SKIP {
            return Err(DocumentError::Unsupported(AzString::from(alloc::format!(
                "{locator}: this provider cannot seek, and {offset} bytes are more than the {MAX_SKIP} that can be skipped by reading: read it from the start"
            ))));
        }
        let skipped = std::io::copy(&mut stream.by_ref().take(offset), &mut std::io::sink()).map_err(|e| io_error(locator, &e))?;
        if skipped < offset {
            // the stream ended before the offset
            return Ok(Vec::new());
        }
    }
    let mut out = Vec::with_capacity(usize::try_from(len.min(1 << 20)).unwrap_or(0));
    stream.take(len).read_to_end(&mut out).map_err(|e| io_error(locator, &e))?;
    Ok(out)
}

fn plain_name(name: &str) -> Result<(), DocumentError> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.contains(['/', '\\', '\0']);
    if bad {
        return Err(DocumentError::Failed(AzString::from(alloc::format!(
            "{name:?} is not a plain name (one segment, no / or \\, at most 255 bytes)"
        ))));
    }
    Ok(())
}

impl Documents {
    /// A handle on `backend`.
    #[must_use]
    pub fn new(backend: Arc<dyn DocumentBackend>) -> Self {
        Self {
            ptr: Box::new(backend),
            run_destructor: true,
        }
    }

    /// The backend of the desktops and of anything with paths.
    #[must_use]
    pub fn local() -> Self {
        Self::new(Arc::new(LocalDocuments::default()))
    }

    /// The name of the backend behind this handle.
    #[must_use]
    pub fn backend_name(&self) -> AzString {
        AzString::from(self.ptr.name())
    }

    /// The places the app may list without asking.
    #[must_use]
    pub fn places(&self) -> PlaceVec {
        PlaceVec::from_vec(self.ptr.places())
    }

    /// Forget a granted place (the permission is released; the folder is untouched).
    ///
    /// # Errors
    ///
    /// Why the platform could not release it.
    pub fn forget(&self, locator: AzString) -> Result<EmptyStruct, DocumentError> {
        self.ptr.forget(locator.as_str()).map(|()| EmptyStruct::default())
    }

    /// What the platform says of one entry.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Denied`, `TimedOut`, or what the platform said.
    pub fn stat(&self, locator: AzString) -> Result<DocumentEntry, DocumentError> {
        self.ptr.stat(locator.as_str())
    }

    /// The entries of a folder, folders first, then by name; at most [`MAX_ENTRIES`].
    ///
    /// # Errors
    ///
    /// `NotFound`, `Denied` (the grant was taken back), `NotADirectory`, `TimedOut`, or what the platform said.
    pub fn children(&self, locator: AzString) -> Result<DocumentEntryVec, DocumentError> {
        self.ptr.children(locator.as_str()).map(DocumentEntryVec::from_vec)
    }

    /// Up to `len` bytes of a file from `offset`: fewer at its end, none past it. At most [`MAX_READ`] a call.
    ///
    /// # Errors
    ///
    /// `Failed` for a `len` over the limit, `NotFound`, `Denied`, `TimedOut`, or what the platform said.
    pub fn read(&self, locator: AzString, offset: u64, len: u64) -> Result<U8Vec, DocumentError> {
        if len > MAX_READ {
            return Err(DocumentError::Failed(AzString::from(alloc::format!(
                "a read is at most {} MiB ({} MiB asked): read a big file in ranges",
                MAX_READ / (1024 * 1024),
                len.div_ceil(1024 * 1024)
            ))));
        }
        self.ptr.read(locator.as_str(), offset, len).map(U8Vec::from_vec)
    }

    /// The whole file as a stream, for copying it without holding it (a Rust-side API: a stream is not a C type).
    ///
    /// # Errors
    ///
    /// Why the file cannot be opened.
    pub fn open_read(&self, locator: &str) -> Result<Box<dyn ReadSeek>, DocumentError> {
        self.ptr.open_read(locator)
    }

    /// Replace the content of an existing file with `bytes`.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Denied`, `TimedOut`, or what the platform said (the file is as it was).
    pub fn write(&self, locator: AzString, bytes: U8Vec) -> Result<DocumentEntry, DocumentError> {
        self.ptr.write(locator.as_str(), bytes.as_ref())
    }

    /// Make a new empty file called `name` of the type `mime` in the folder `parent`; `name` is one plain segment.
    ///
    /// # Errors
    ///
    /// `Exists`, `NotFound` (the folder), `Denied`, `TimedOut`, or what the platform said.
    pub fn create_file(&self, parent: AzString, name: AzString, mime: AzString) -> Result<DocumentEntry, DocumentError> {
        plain_name(name.as_str())?;
        self.ptr.create_file(parent.as_str(), name.as_str(), mime.as_str())
    }

    /// Make a new folder called `name` in the folder `parent`; `name` is one plain segment.
    ///
    /// # Errors
    ///
    /// `Exists`, `NotFound` (the parent), `Denied`, `TimedOut`, or what the platform said.
    pub fn create_dir(&self, parent: AzString, name: AzString) -> Result<DocumentEntry, DocumentError> {
        plain_name(name.as_str())?;
        self.ptr.create_dir(parent.as_str(), name.as_str())
    }

    /// Rename an entry in place; the answer carries the new locator.
    ///
    /// # Errors
    ///
    /// `Exists` when the name is taken, `NotFound`, `Denied`, `Unsupported`, or what the platform said.
    pub fn rename(&self, locator: AzString, new_name: AzString) -> Result<DocumentEntry, DocumentError> {
        plain_name(new_name.as_str())?;
        self.ptr.rename(locator.as_str(), new_name.as_str())
    }

    /// Delete a file, or a folder with everything in it.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Denied`, `Unsupported`, or what the platform said.
    pub fn delete(&self, locator: AzString) -> Result<EmptyStruct, DocumentError> {
        self.ptr.delete(locator.as_str()).map(|()| EmptyStruct::default())
    }

    /// Where the working copy at `working_copy` came from (a file another app opened this app with, a file the person picked), or `None`
    /// for a file that was not opened from somewhere else.
    #[must_use]
    pub fn origin(&self, working_copy: AzString) -> OptionDocumentOrigin {
        crate::open_inbox::read_origin(Path::new(working_copy.as_str()))
            .map(|origin| DocumentOrigin {
                uri: AzString::from(origin.uri),
                writable: origin.writable,
            })
            .into()
    }

    /// Save the working copy at `working_copy` back where it came from: the bytes of the copy replace the original (through the same
    /// call as [`Documents::write`]), and the answer is the original as it is now. What the platform cannot do is said:
    /// `Unsupported` for a file with no origin, `Denied` for one that was opened read-only (offer `FileDialog::export_bytes`
    /// instead), `Failed` for a copy over the limit of the inbox (256 MiB).
    ///
    /// # Errors
    ///
    /// The reasons above, or what [`Documents::write`] says; the original is as it was when the answer is an error.
    pub fn write_back(&self, working_copy: AzString) -> Result<DocumentEntry, DocumentError> {
        let copy = Path::new(working_copy.as_str());
        let Some(origin) = crate::open_inbox::read_origin(copy) else {
            return Err(DocumentError::Unsupported(AzString::from(alloc::format!(
                "{} has no origin: it was not opened from somewhere else, so there is nothing to save it back to",
                working_copy.as_str()
            ))));
        };
        if !origin.writable {
            return Err(DocumentError::Denied(AzString::from(alloc::format!(
                "{} was opened read-only: save it somewhere with FileDialog::export_bytes",
                working_copy.as_str()
            ))));
        }
        let limit = crate::open_inbox::OpenLimits::default().max_file_bytes;
        let size = fs::metadata(copy).map_err(|e| io_error(working_copy.as_str(), &e))?.len();
        if size > limit {
            return Err(DocumentError::Failed(AzString::from(alloc::format!(
                "{} is {} MiB, more than the {} MiB that is saved back",
                working_copy.as_str(),
                size.div_ceil(1024 * 1024),
                limit / (1024 * 1024)
            ))));
        }
        let bytes = fs::read(copy).map_err(|e| io_error(working_copy.as_str(), &e))?;
        self.ptr.write(&origin.uri, &bytes)
    }

    /// Whether the app may use the device's shared storage as plain folders.
    #[must_use]
    pub fn all_files_access(&self) -> AllFilesAccess {
        self.ptr.all_files_access()
    }

    /// Open the system's page where the person can give "All files access". Call it from the callback of the person's tap.
    #[must_use]
    pub fn request_all_files_access(&self) -> bool {
        self.ptr.request_all_files_access()
    }
}

/// The slot the platform's backend is installed in (a field of `AppServices`): a [`Documents`] handle on the desktop's backend until
/// a shell installs its own.
#[derive(Debug, Clone)]
pub struct DocumentsSlot {
    current: Arc<Mutex<Documents>>,
}

impl Default for DocumentsSlot {
    fn default() -> Self {
        Self {
            current: Arc::new(Mutex::new(Documents::local())),
        }
    }
}

impl DocumentsSlot {
    /// A slot of its own with the desktop's backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the platform's backend (the shell, at the start).
    pub fn install(&self, backend: Arc<dyn DocumentBackend>) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Documents::new(backend);
    }

    /// A handle on the installed backend.
    #[must_use]
    pub fn handle(&self) -> Documents {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

// ---- the deadline of a call ----

/// How long a call to a slow provider may take before the caller is told [`DocumentError::TimedOut`].
pub const CALL_DEADLINE: Duration = Duration::from_secs(20);

/// How many calls that outlived their deadline may still be waiting inside a provider before the next call is refused at once
/// (a provider that never answers must not collect a thread per retry).
pub const MAX_STUCK_CALLS: usize = 8;

const RUNNING: u8 = 0;
const FINISHED: u8 = 1;
const ABANDONED: u8 = 2;

/// Runs the calls of a backend that cannot be cancelled (a binder call into another app's provider, a network share) on a worker
/// with a DEADLINE: the caller gets the answer, or [`DocumentError::TimedOut`] when the deadline passes first, and goes on. The
/// call itself cannot be stopped; its late answer is dropped (what it opened is closed with it), and while calls that outlived
/// their deadline are still inside the provider they are COUNTED, so that a provider that never answers is refused
/// ([`MAX_STUCK_CALLS`]) instead of collecting a thread per retry. The count belongs to the guard (to its backend), not to the
/// process.
#[derive(Debug, Clone)]
pub struct CallGuard {
    stuck: Arc<AtomicUsize>,
    deadline: Duration,
    max_stuck: usize,
}

impl Default for CallGuard {
    fn default() -> Self {
        Self::new(CALL_DEADLINE, MAX_STUCK_CALLS)
    }
}

impl CallGuard {
    /// A guard that waits `deadline` for a call and refuses new ones while `max_stuck` late calls are still inside the provider.
    #[must_use]
    pub fn new(deadline: Duration, max_stuck: usize) -> Self {
        Self {
            stuck: Arc::new(AtomicUsize::new(0)),
            deadline,
            max_stuck,
        }
    }

    /// How many calls outlived their deadline and have not come back yet.
    #[must_use]
    pub fn stuck(&self) -> usize {
        self.stuck.load(Ordering::Acquire)
    }

    /// Run `call` on a worker and wait for it at most the deadline. `what` names the place for the failure text.
    ///
    /// # Errors
    ///
    /// What `call` says; [`DocumentError::TimedOut`] when it did not answer in time or the provider still holds too many earlier
    /// calls; [`DocumentError::Failed`] when no worker could be started.
    pub fn run<T: Send + 'static>(
        &self,
        what: &str,
        call: impl FnOnce() -> Result<T, DocumentError> + Send + 'static,
    ) -> Result<T, DocumentError> {
        self.run_within(self.deadline, what, call)
    }

    /// [`CallGuard::run`] with a deadline of this call's own (a question a screen waits for gets a shorter one than a file copy).
    ///
    /// # Errors
    ///
    /// As [`CallGuard::run`].
    pub fn run_within<T: Send + 'static>(
        &self,
        deadline: Duration,
        what: &str,
        call: impl FnOnce() -> Result<T, DocumentError> + Send + 'static,
    ) -> Result<T, DocumentError> {
        if self.stuck() >= self.max_stuck {
            return Err(DocumentError::TimedOut(AzString::from(alloc::format!(
                "{what}: {} earlier calls are still waiting for the provider",
                self.stuck()
            ))));
        }
        let (send, receive) = mpsc::channel();
        let state = Arc::new(AtomicU8::new(RUNNING));
        let worker_state = Arc::clone(&state);
        let worker_stuck = Arc::clone(&self.stuck);
        let spawned = std::thread::Builder::new().name("azul-documents".into()).spawn(move || {
            let answer = call();
            // the caller may have given up: the answer then goes nowhere and what it holds is dropped here
            drop(send.send(answer));
            if worker_state.swap(FINISHED, Ordering::AcqRel) == ABANDONED {
                worker_stuck.fetch_sub(1, Ordering::AcqRel);
            }
        });
        if let Err(e) = spawned {
            return Err(DocumentError::Failed(AzString::from(alloc::format!("{what}: no worker could be started ({e})"))));
        }
        match receive.recv_timeout(deadline) {
            Ok(answer) => answer,
            // the worker dropped its end without an answer: the call panicked (a bug of the backend), it is not inside the provider
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DocumentError::Failed(AzString::from(alloc::format!(
                "{what}: the call stopped without an answer"
            )))),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // counted BEFORE the worker can see the flag, so that its decrement never runs first
                self.stuck.fetch_add(1, Ordering::AcqRel);
                if state.compare_exchange(RUNNING, ABANDONED, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                    return Err(DocumentError::TimedOut(AzString::from(alloc::format!(
                        "{what} did not answer in {} s",
                        deadline.as_secs()
                    ))));
                }
                // the worker finished in the same moment: its answer is in the channel
                self.stuck.fetch_sub(1, Ordering::AcqRel);
                receive.try_recv().unwrap_or_else(|_| {
                    Err(DocumentError::Failed(AzString::from(alloc::format!("{what}: the call ended without an answer"))))
                })
            }
        }
    }
}

// ---- the local backend ----

/// Files by path, over `std::fs`: the backend of the desktops, of iOS (a security-scoped folder is a path once its scope is open) and
/// of Android with "All files access".
#[derive(Debug, Default, Clone)]
pub struct LocalDocuments {
    /// The places this backend lists (empty: the user's well-known folders that exist).
    places: Vec<(String, PathBuf)>,
}

impl LocalDocuments {
    /// A backend that lists `places` (name, folder) instead of the user's well-known folders.
    #[must_use]
    pub const fn with_places(places: Vec<(String, PathBuf)>) -> Self {
        Self { places }
    }
}

/// The error of an I/O failure on `locator`: what the kind of the error says the app does next.
#[must_use]
pub fn io_error(locator: &str, e: &std::io::Error) -> DocumentError {
    use std::io::ErrorKind as K;
    let what = AzString::from(locator);
    match e.kind() {
        K::NotFound => DocumentError::NotFound(what),
        K::PermissionDenied => DocumentError::Denied(what),
        K::AlreadyExists => DocumentError::Exists(what),
        K::TimedOut => DocumentError::TimedOut(what),
        K::NotADirectory => DocumentError::NotADirectory(what),
        _ => DocumentError::Failed(AzString::from(alloc::format!("{locator}: {e}"))),
    }
}

/// The user's well-known folders that exist (the `dirs` of the account where the `extra` feature has them, else under `$HOME`).
#[cfg(all(feature = "std", feature = "extra"))]
fn well_known_folders() -> Vec<(String, PathBuf)> {
    use crate::file::FilePath;
    [
        ("Documents", FilePath::get_document_dir()),
        ("Downloads", FilePath::get_download_dir()),
        ("Pictures", FilePath::get_picture_dir()),
        ("Music", FilePath::get_audio_dir()),
        ("Videos", FilePath::get_video_dir()),
        ("Home", FilePath::get_home_dir()),
    ]
    .into_iter()
    .filter_map(|(name, path)| path.map(|p| (name.to_string(), PathBuf::from(p.as_str()))))
    .filter(|(_, path)| path.is_dir())
    .collect()
}

/// The same without the `dirs` crate: the usual names under `$HOME`.
#[cfg(not(all(feature = "std", feature = "extra")))]
fn well_known_folders() -> Vec<(String, PathBuf)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    ["Documents", "Downloads", "Pictures", "Music", "Videos"]
        .into_iter()
        .map(|name| (name.to_string(), home.join(name)))
        .chain(std::iter::once(("Home".to_string(), home.clone())))
        .filter(|(_, path)| path.is_dir())
        .collect()
}

/// `bytes` into `temp` (flushed to the disk), then `temp` over `target`.
fn write_then_rename(temp: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temp, target)
}

fn entry_of(path: &Path, meta: &fs::Metadata) -> DocumentEntry {
    let name = path
        .file_name()
        .map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
    let is_dir = meta.is_dir();
    let writable = !meta.permissions().readonly();
    DocumentEntry {
        uri: AzString::from(path.to_string_lossy().into_owned()),
        mime: AzString::from(if is_dir { "inode/directory" } else { mime::for_file_name(&name) }),
        name: AzString::from(name),
        is_dir,
        size: if is_dir { OptionU64::None } else { OptionU64::Some(meta.len()) },
        modified_ms: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .into(),
        can_write: writable,
        can_delete: true,
        can_rename: true,
    }
}

impl DocumentBackend for LocalDocuments {
    fn name(&self) -> &'static str {
        "local"
    }

    fn places(&self) -> Vec<Place> {
        let listed: Vec<(String, PathBuf)> = if self.places.is_empty() {
            well_known_folders()
        } else {
            self.places.clone()
        };
        listed
            .into_iter()
            .map(|(name, path)| Place {
                uri: AzString::from(path.to_string_lossy().into_owned()),
                name: AzString::from(name),
                writable: fs::metadata(&path).is_ok_and(|m| !m.permissions().readonly()),
                granted_ms: OptionU64::None,
                kind: PlaceKind::SharedStorage,
            })
            .collect()
    }

    fn forget(&self, _locator: &str) -> Result<(), DocumentError> {
        // a path is not a grant: there is nothing to release
        Ok(())
    }

    fn stat(&self, locator: &str) -> Result<DocumentEntry, DocumentError> {
        let path = Path::new(locator);
        let meta = fs::metadata(path).map_err(|e| io_error(locator, &e))?;
        Ok(entry_of(path, &meta))
    }

    fn children(&self, locator: &str) -> Result<Vec<DocumentEntry>, DocumentError> {
        let mut out = Vec::new();
        for item in fs::read_dir(locator).map_err(|e| io_error(locator, &e))? {
            let Ok(item) = item else {
                continue;
            };
            // a link that dangles, a file that vanished between the listing and the stat: skipped, the listing goes on
            let Ok(meta) = fs::metadata(item.path()) else {
                continue;
            };
            out.push(entry_of(&item.path(), &meta));
            if out.len() >= MAX_ENTRIES {
                break;
            }
        }
        sort_listing(&mut out);
        Ok(out)
    }

    fn read(&self, locator: &str, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError> {
        let mut file = fs::File::open(locator).map_err(|e| io_error(locator, &e))?;
        read_range(&mut file, locator, offset, len)
    }

    fn open_read(&self, locator: &str) -> Result<Box<dyn ReadSeek>, DocumentError> {
        let file = fs::File::open(locator).map_err(|e| io_error(locator, &e))?;
        Ok(Box::new(file))
    }

    fn write(&self, locator: &str, bytes: &[u8]) -> Result<DocumentEntry, DocumentError> {
        let path = Path::new(locator);
        if !path.exists() {
            return Err(DocumentError::NotFound(AzString::from(locator)));
        }
        // written beside it and renamed over it, so that a write that fails half way leaves the file as it was
        let temp = path.with_extension(alloc::format!("azul-write-{}", crate::managers::share_area::new_token()));
        if let Err(e) = write_then_rename(&temp, path, bytes) {
            drop(fs::remove_file(&temp));
            return Err(io_error(locator, &e));
        }
        self.stat(locator)
    }

    fn create_file(&self, parent: &str, name: &str, _mime: &str) -> Result<DocumentEntry, DocumentError> {
        let path = Path::new(parent).join(name);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| io_error(&path.to_string_lossy(), &e))?;
        self.stat(&path.to_string_lossy())
    }

    fn create_dir(&self, parent: &str, name: &str) -> Result<DocumentEntry, DocumentError> {
        let path = Path::new(parent).join(name);
        fs::create_dir(&path).map_err(|e| io_error(&path.to_string_lossy(), &e))?;
        self.stat(&path.to_string_lossy())
    }

    fn rename(&self, locator: &str, new_name: &str) -> Result<DocumentEntry, DocumentError> {
        let path = Path::new(locator);
        let Some(parent) = path.parent() else {
            return Err(DocumentError::Failed(AzString::from(alloc::format!("{locator} has no parent to rename in"))));
        };
        let target = parent.join(new_name);
        if target.exists() {
            return Err(DocumentError::Exists(AzString::from(target.to_string_lossy().into_owned())));
        }
        fs::rename(path, &target).map_err(|e| io_error(locator, &e))?;
        self.stat(&target.to_string_lossy())
    }

    fn delete(&self, locator: &str) -> Result<(), DocumentError> {
        let path = Path::new(locator);
        let meta = fs::symlink_metadata(path).map_err(|e| io_error(locator, &e))?;
        let result = if meta.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
        result.map_err(|e| io_error(locator, &e))
    }

    fn all_files_access(&self) -> AllFilesAccess {
        // the desktops need no such grant: every path the person's account may open is open
        AllFilesAccess::Granted
    }

    fn request_all_files_access(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "documents_test.rs"]
mod documents_test;
