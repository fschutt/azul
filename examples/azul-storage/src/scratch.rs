//! Scratch: the one place a program of this family keeps what it needs for a while and nobody
//! asked for (ARCH28 RF48 / AR5a-8; AZ-C6, AZ-W4, AZ-U13, AZ-U14, X-AR5a-23).
//!
//! The ciphertext of a streamed upload before it is whole, the copy of a cloud file handed to
//! its viewer, a page waiting for the PDF reader, the token vault of a run that knows no
//! configuration folder: each was a path of its own in the system's shared temp folder, under a
//! fixed or guessable name, with the default mode (readable by every user of the computer; a
//! link planted at the name before the program ran was followed), and nothing took it away when
//! the program was killed. Here:
//!
//! * The place is `<cache folder>/scratch` ([`set_root`], once, at the start of a program that
//!   has a cache folder) or, for one that has none, `<temp folder>/azul-scratch-<uid>`. The
//!   folder, and the folder of each purpose below it, are `rwx------`, and they are CHECKED on
//!   every use ([`Scratch::ensure`]): a link, a file or a folder of another user is refused, a
//!   folder of ours that others can enter is closed first.
//! * A file is `<purpose>-<pid>-<run>-<n>` ([`Scratch::file`]): made new (it never opens what is
//!   there), `rw-------`, removed when its [`ScratchFile`] drops. The name says which process
//!   made it, so a sweep knows an orphan: no clock decides that a live program's file goes.
//! * A thing kept for a while - the copy of a cloud file opened in its viewer - is a folder
//!   `k-<hash of its tag>` ([`Scratch::keep`]): whoever asks for the same tag gets the same
//!   folder, so the same version is fetched once.
//! * [`Scratch::sweep`] removes what a process that is gone left, and what is older than the
//!   horizon it is given. A name that is not this module's is never touched. The age is the
//!   wall clock's, and that is acceptable here only because scratch is a COPY of something that
//!   lives elsewhere: the worst a wrong clock does is a copy made again.
//!
//! `std::env::temp_dir()` is read in this module and nowhere else of the shipped code
//! (`azul-srcgate/tests/scratch_gate.rs` counts the sites that remain and who removes them).

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock, PoisonError,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::beneath::{BeneathError, Dir, Kind};

/// Where scratch lives, set once by a program that has a cache folder (`<cache>/scratch`).
static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The front of a kept folder's name.
const KEPT: &str = "k-";

/// Entries one sweep looks at: a folder somebody filled with millions is not walked whole at
/// the start of a program.
const SWEEP_LOOKS: usize = 4096;

/// What the sweep a purpose's first [`Scratch::file`] runs removes by age: a file of a process
/// that may still run is this old before it goes (a file of a process that is gone always goes).
pub const HORIZON: Duration = Duration::from_secs(3 * 24 * 3600);

/// Keeps scratch under `cache/scratch` from now on. Called once, at the start of a program that
/// has a cache folder, before the first [`Scratch::new`]; a second call changes nothing. Returns
/// whether this call set it.
pub fn set_root(cache: Option<&Path>) -> bool {
    ROOT.set(cache.map(|cache| cache.join("scratch"))).is_ok()
}

/// The folder scratch lives in (not made until a [`Scratch`] is used).
#[must_use]
pub fn root() -> PathBuf {
    match ROOT.get() {
        Some(Some(root)) => root.clone(),
        _ => fallback_root(),
    }
}

/// The folder of a program that has no cache folder: one per user in the system's temp folder.
/// The only place the shipped code of the storage crates asks for that folder.
fn fallback_root() -> PathBuf {
    std::env::temp_dir().join(format!("azul-scratch-{}", user_tag()))
}

/// What tells this user's folder from another user's in a folder everyone shares.
fn user_tag() -> String {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid` takes no argument and cannot fail.
        unsafe { libc::geteuid() }.to_string()
    }
    #[cfg(not(unix))]
    {
        // The temp folder of this system is the user's own.
        String::from("user")
    }
}

/// A random number of this process: tells two programs that had one process id apart.
fn run_id() -> u64 {
    static RUN: OnceLock<u64> = OnceLock::new();
    *RUN.get_or_init(crate::ids::random_seed)
}

/// What one purpose's scratch is: a private folder, the files made in it, the folders kept in it.
#[derive(Clone, Debug)]
pub struct Scratch {
    dir: PathBuf,
    purpose: String,
}

impl Scratch {
    /// The scratch of `purpose` in [`root`] (a short word: `spool`, `open`; it becomes a folder
    /// name and the front of the file names; letters, digits and `-` only, the rest is dropped).
    #[must_use]
    pub fn new(purpose: &str) -> Scratch {
        Scratch::under(&root(), purpose)
    }

    /// [`Scratch::new`] with its own `root` (a test's folder).
    #[must_use]
    pub fn under(root: &Path, purpose: &str) -> Scratch {
        let purpose = clean_purpose(purpose);
        Scratch {
            dir: root.join(&purpose),
            purpose,
        }
    }

    /// The purpose's folder (made by [`Scratch::ensure`]).
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Makes the root and the purpose's folder `rwx------` and checks them, as a folder.
    ///
    /// # Errors
    ///
    /// When a part of it is a link, a file or another user's folder, or cannot be made.
    pub fn ensure(&self) -> io::Result<Dir> {
        if let Some(root) = self.dir.parent() {
            private_folder(root)?;
        }
        private_folder(&self.dir)?;
        Dir::open_following(&self.dir)
    }

    /// A new file of this purpose, `rw-------`, open for reading and writing; it is removed when
    /// the [`ScratchFile`] drops. The first call of a process for a purpose also sweeps what
    /// dead processes left ([`HORIZON`]).
    ///
    /// # Errors
    ///
    /// When the folder cannot be made or checked, or the file cannot be made (the disk is full).
    pub fn file(&self) -> io::Result<ScratchFile> {
        let dir = self.ensure()?;
        if first_use(&self.dir) {
            self.sweep(HORIZON);
        }
        // A name is this process's own (a counter), so a name that exists is a leftover of a
        // process that had our pid and run id: another number is free.
        for _ in 0..64 {
            let name = file_name(&self.purpose);
            match dir.create_new(&name, 0o600) {
                Ok(created) => {
                    // `create_new` opens for writing only: the file is opened again, by its
                    // name, in the folder only this user can enter.
                    drop(created);
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(self.dir.join(&name));
                    return match file {
                        Ok(file) => Ok(ScratchFile { file, dir, name }),
                        Err(e) => {
                            let _ = dir.unlink(&name);
                            Err(e)
                        }
                    };
                }
                Err(e) if e.is_already_exists() => {}
                Err(e) => return Err(io_of(e)),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free scratch file name",
        ))
    }

    /// The folder kept under `tag` (a file and its version: what is the same thing twice is one
    /// folder): `k-<hash>`, `rwx------`, made on the first ask. Its contents are the caller's;
    /// a sweep removes it, with everything in it, when it is old.
    ///
    /// # Errors
    ///
    /// When the folders cannot be made or checked.
    pub fn keep(&self, tag: &str) -> io::Result<PathBuf> {
        self.ensure()?;
        let path = self.kept(tag);
        private_folder(&path)?;
        Ok(path)
    }

    /// Where the folder kept under `tag` is ([`Scratch::keep`] makes it): the path alone, nothing
    /// is made. What takes the folder away before its age does (a removed drive's copies) asks
    /// for it here.
    #[must_use]
    pub fn kept(&self, tag: &str) -> PathBuf {
        let hash = blake3::hash(tag.as_bytes()).to_hex();
        self.dir.join(format!("{KEPT}{}", &hash.as_str()[..16]))
    }

    /// Removes what this purpose's folder holds that nobody uses: a file of a process that is
    /// gone (or of an earlier run of this pid), a file of a live process or a kept folder older
    /// than `older_than`. Files of THIS run are never removed (their [`ScratchFile`] does) and
    /// names that are not this module's are left alone. How many entries went.
    pub fn sweep(&self, older_than: Duration) -> usize {
        let Ok(dir) = Dir::open_following(&self.dir) else {
            return 0;
        };
        let Ok(names) = dir.names() else {
            return 0;
        };
        let now = SystemTime::now();
        let mut removed = 0;
        for name in names.iter().take(SWEEP_LOOKS) {
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(st) = dir.lstat(name) else {
                continue;
            };
            let age = now
                .duration_since(UNIX_EPOCH + Duration::from_nanos(st.mtime_ns))
                .unwrap_or_default();
            let old = age >= older_than;
            match (made_by(name, &self.purpose), st.kind) {
                (Some(made), Kind::File) => {
                    if made.is_this_run() {
                        continue;
                    }
                    if (made.orphaned() || old) && dir.unlink(name).is_ok() {
                        removed += 1;
                    }
                }
                (None, Kind::Dir) if name.starts_with(KEPT) => {
                    if old && fs::remove_dir_all(self.dir.join(name)).is_ok() {
                        removed += 1;
                    }
                }
                _ => {}
            }
        }
        removed
    }
}

/// [`Scratch::sweep`] of every purpose below [`root`].
pub fn sweep_all(older_than: Duration) -> usize {
    sweep_all_in(&root(), older_than)
}

/// [`Scratch::sweep`] of every purpose below `root`.
pub fn sweep_all_in(root: &Path, older_than: Duration) -> usize {
    let Ok(dir) = Dir::open_following(root) else {
        return 0;
    };
    let Ok(names) = dir.names() else {
        return 0;
    };
    let mut removed = 0;
    for name in names.iter().take(SWEEP_LOOKS) {
        let Some(purpose) = name.to_str() else {
            continue;
        };
        let is_folder = dir.lstat(purpose).is_ok_and(|st| st.kind == Kind::Dir);
        if is_folder && clean_purpose(purpose) == purpose {
            removed += Scratch::under(root, purpose).sweep(older_than);
        }
    }
    removed
}

/// Makes the file at `path` read-only for everyone (`r--------` where the file system has
/// modes): a program that opens it and saves fails loudly, instead of editing a copy nobody
/// uploads (AZ-U13).
///
/// # Errors
///
/// When the system refuses it.
pub fn make_read_only(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o400))
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)
    }
}

/// A scratch file: read, written and sought like the [`File`] it holds, and removed from its
/// folder when it drops - the drop of a failed or cancelled write leaves nothing behind.
pub struct ScratchFile {
    file: File,
    dir: Dir,
    name: String,
}

impl ScratchFile {
    /// Where the file is.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.dir.path().join(&self.name)
    }

    /// The bytes it holds now.
    ///
    /// # Errors
    ///
    /// When the system refuses to tell.
    pub fn len(&self) -> io::Result<u64> {
        self.file.metadata().map(|meta| meta.len())
    }

    /// Whether it holds nothing.
    ///
    /// # Errors
    ///
    /// When the system refuses to tell.
    pub fn is_empty(&self) -> io::Result<bool> {
        self.len().map(|len| len == 0)
    }
}

impl fmt::Debug for ScratchFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScratchFile({})", self.path().display())
    }
}

impl Read for ScratchFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for ScratchFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for ScratchFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.file.seek(to)
    }
}

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = self.dir.unlink(&self.name);
    }
}

/// The purpose as a folder name and a name front: lowercase letters, digits and `-`.
fn clean_purpose(purpose: &str) -> String {
    let cleaned: String = purpose
        .chars()
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' | '-' => Some(c),
            'A'..='Z' => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    if cleaned.is_empty() {
        String::from("scratch")
    } else {
        cleaned
    }
}

/// A name for a new file of `purpose`: `<purpose>-<pid>-<run>-<n>`.
fn file_name(purpose: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{purpose}-{}-{:x}-{}",
        std::process::id(),
        run_id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// Who made a file, as its name says.
#[derive(Debug, PartialEq, Eq)]
struct Made {
    pid: u32,
    run: u64,
}

impl Made {
    /// Made by this very run of this program.
    fn is_this_run(&self) -> bool {
        self.pid == std::process::id() && self.run == run_id()
    }

    /// Made by a process that is gone: this pid with another run id, or a pid that runs no more.
    fn orphaned(&self) -> bool {
        if self.pid == std::process::id() {
            return self.run != run_id();
        }
        !crate::durable::process_runs(self.pid)
    }
}

/// Who made `name`, when it is a file name of `purpose` ([`file_name`]).
fn made_by(name: &str, purpose: &str) -> Option<Made> {
    let mut parts = name.rsplitn(4, '-');
    let _count: u64 = parts.next()?.parse().ok()?;
    let run = u64::from_str_radix(parts.next()?, 16).ok()?;
    let pid: u32 = parts.next()?.parse().ok()?;
    (parts.next()? == purpose).then_some(Made { pid, run })
}

/// Whether this is the first time this process asks about the purpose folder `dir` (the sweep
/// of a start runs once).
fn first_use(dir: &Path) -> bool {
    /// Folders remembered: a test makes many, a program a few.
    const REMEMBERED: usize = 256;
    static SEEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    if seen.iter().any(|d| d == dir) {
        return false;
    }
    if seen.len() < REMEMBERED {
        seen.push(dir.to_path_buf());
    }
    true
}

/// The folder at `path` as only this user may enter it: made `rwx------` when it is missing
/// (the folders above it with the default mode), checked when it is there.
fn private_folder(path: &Path) -> io::Result<()> {
    match make_private(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(above) = path.parent() {
                fs::create_dir_all(above)?;
            }
            match make_private(path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    }
    check_private(path)
}

fn make_private(path: &Path) -> io::Result<()> {
    // Only the Unix build sets the mode through the builder.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    // Windows: the folder is this user's alone, and what is made in it inherits that.
    #[cfg(windows)]
    crate::win::make_private(path, true)?;
    Ok(())
}

/// `path` is a folder (not a link to one), belongs to this user, and nobody else can enter it;
/// a folder of ours that others can enter is closed.
fn check_private(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is a link or a file, not a folder of its own",
                path.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: `geteuid` takes no argument and cannot fail.
        let me = unsafe { libc::geteuid() };
        if meta.uid() != me {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} belongs to another user", path.display()),
            ));
        }
        if meta.mode() & 0o077 != 0 {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

/// A walk error as the I/O error it wraps, or one that says it.
fn io_of(e: BeneathError) -> io::Error {
    match e {
        BeneathError::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}
