//! Files under a root that are never reached through a link.
//!
//! A synced folder takes names from other devices and other people, and a folder in it can be a
//! link the user made (`inbox -> ~/Library/LaunchAgents`) or one swapped in while a pass runs.
//! Plain paths follow such links, so a write meant for the folder lands outside it. Here every
//! path is walked from an open descriptor of the root, one name at a time, each opened with
//! `O_NOFOLLOW`, and every create, rename and delete names its file relative to its folder's
//! descriptor (`openat`, `mkdirat`, `renameat`, `unlinkat`). A folder swapped for a link makes
//! the walk fail; it never redirects the write. A folder of another file system mounted inside
//! the root is never entered either.
//!
//! Regular files are opened with `O_NONBLOCK` (a FIFO swapped in cannot block the open) and
//! `fstat`ed, so the caller checks it is still the file it listed.
//!
//! Windows has no descriptor-relative calls in std: there each name of the path is looked at right
//! before the step ([`crate::win`]: a link, a junction or a mount point is a [`Kind::Link`]; a file
//! a filter serves - a compressed one, a hydrated cloud file - is itself), and what a step opened
//! is asked afterwards where it really is - under the root's resolved path, whatever swapped in
//! between. A step that opens nothing (rename, unlink, rmdir) asks its folder right before: the
//! window that is left is one system call wide.

#[cfg(unix)]
use std::ffi::OsStr;
use std::{
    ffi::OsString,
    fmt,
    fs::File,
    io,
    path::{Path, PathBuf},
};

use crate::{cap::Cap, names::LocalKey};

/// Why a path under the root cannot be walked.
#[derive(Debug)]
pub enum BeneathError {
    /// A folder of the path (named, from the root) is a link or no folder: nothing is read or
    /// written through it.
    ThroughLink(String),
    /// A folder of the path is another file system mounted inside the root.
    CrossesMount(String),
    /// The name cannot be one file name (empty, `.`, `..`, a separator or NUL in it).
    BadName(String),
    Io(io::Error),
}

impl fmt::Display for BeneathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BeneathError::ThroughLink(at) => write!(
                f,
                "\"{at}\" here is a link or no folder: nothing is written or read through it"
            ),
            BeneathError::CrossesMount(at) => write!(
                f,
                "\"{at}\" here is another disk mounted in the folder: it is never entered"
            ),
            BeneathError::BadName(name) => write!(f, "\"{name}\" cannot be a file name"),
            BeneathError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BeneathError {}

impl From<io::Error> for BeneathError {
    fn from(e: io::Error) -> Self {
        BeneathError::Io(e)
    }
}

impl BeneathError {
    /// Whether the path (or a folder of it) does not exist.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, BeneathError::Io(e) if e.kind() == io::ErrorKind::NotFound)
    }

    /// The disk is full (`ENOSPC`).
    #[must_use]
    pub fn is_disk_full(&self) -> bool {
        matches!(self, BeneathError::Io(e) if is_disk_full(e))
    }

    /// Something has the name already (a create or a rename that must not replace found a file).
    #[must_use]
    pub fn is_already_exists(&self) -> bool {
        matches!(self, BeneathError::Io(e) if e.kind() == io::ErrorKind::AlreadyExists)
    }

    /// The system cannot do it here (a file system without hard links).
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, BeneathError::Io(e) if e.kind() == io::ErrorKind::Unsupported)
    }
}

/// Whether `e` says the disk is full.
#[must_use]
pub fn is_disk_full(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::StorageFull {
        return true;
    }
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::ENOSPC)
    }
    #[cfg(windows)]
    {
        // ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL
        matches!(e.raw_os_error(), Some(112) | Some(39))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        false
    }
}

/// Windows: `file`, just opened below the root, still lies under the root's resolved path. Paths are
/// walked name by name and each name is looked at (`symlink_metadata`) right before the step - but a
/// folder swapped for a junction after its look makes the system open the next name in another tree,
/// for the whole length of a scan (AZ-W14). Where the opened file really is does not depend on how it
/// was reached: it is asked once the file is open.
///
/// `anchor` `None` is `Posture::Degrade`: the root's resolved path could not be had when it was
/// opened, and the walk is checked before each step only, as it was.
#[cfg(windows)]
fn still_beneath(anchor: Option<&str>, file: &File, rel: &str) -> Result<(), BeneathError> {
    let Some(anchor) = anchor else {
        return Ok(());
    };
    let at = crate::win::final_path(file)?;
    if crate::win::is_under(anchor, &at) {
        Ok(())
    } else {
        Err(BeneathError::ThroughLink(rel.to_string()))
    }
}

/// Whether `e` says another program has the file - open without sharing it, or a range of it locked
/// (Windows' sharing and lock violations; no other system has such an error): the file is not
/// broken, it is busy, and a later try finds it free.
#[must_use]
pub fn is_held_open(e: &io::Error) -> bool {
    crate::win::is_held_open(e)
}

/// Whether another app holds `file` locked against a reader - it is being written. On Unix a shared
/// lock of ours is asked for without waiting and kept until the file is closed: `flock` is advisory,
/// so ours stands in nobody's way. Windows' locks are mandatory: a shared lock of ours would deny
/// every other writer of the file for as long as it is read (a log, a database, a file an editor saves
/// in place failed with a lock violation while the sync hashed it, AZ-W8), so there none is taken, and
/// a read of a range another app has locked fails with a lock violation ([`is_held_open`]), which the
/// reader treats as the same "busy". A disk that keeps no locks answers an error, which says nothing
/// about the file.
#[must_use]
pub fn held_by_another(file: &File) -> bool {
    #[cfg(unix)]
    {
        matches!(
            file.try_lock_shared(),
            Err(std::fs::TryLockError::WouldBlock)
        )
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        false
    }
}

/// What an entry is, as `lstat` says it (a link is never followed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Link,
    Fifo,
    Socket,
    Device,
    Other,
}

/// An entry's `lstat`: what it is, its size and times, and which file it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub kind: Kind,
    pub size: u64,
    /// Modification and status-change times (nanoseconds since 1970; 0 where unknown). No tool
    /// can set the status-change time back, so it sees edits `touch -r` hides.
    pub mtime_ns: u64,
    pub ctime_ns: u64,
    /// The inode and the device (0 where the system has none).
    pub ino: u64,
    pub dev: u64,
    /// The permission bits and the file type, as `st_mode`.
    pub mode: u32,
    /// How many names the file has; 0 once it was deleted while open.
    pub nlink: u64,
}

/// The answer to "is it the same?" when the answer may be "cannot tell". `Unknown` is a third
/// answer of its own, never `Same` and never `Different`: a caller that deletes, replaces or
/// reuses on the strength of "the same" takes `Same` only; one that only has to be careful may
/// take `Different` as the one answer that stops it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    /// Known, and the same.
    Same,
    /// Known, and not the same.
    Different,
    /// The system gave no identity (or no time) to tell by.
    Unknown,
}

impl Tri {
    /// Known and the same: the only answer a destructive step goes on.
    #[must_use]
    pub fn is_same(self) -> bool {
        self == Tri::Same
    }

    /// Known and not the same: the only answer that is a change for certain.
    #[must_use]
    pub fn is_different(self) -> bool {
        self == Tri::Different
    }

    /// Both answers at once: `Different` beats `Unknown`, which beats `Same`.
    #[must_use]
    pub fn and(self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::Different, _) | (_, Tri::Different) => Tri::Different,
            (Tri::Unknown, _) | (_, Tri::Unknown) => Tri::Unknown,
            (Tri::Same, Tri::Same) => Tri::Same,
        }
    }
}

/// Which file: the device and the inode, where the system tells them (Windows' file index
/// once `win.rs` reads it). There is no `==` on it: two files whose identity is unknown are not
/// "the same file", so [`FileId::same`] answers a [`Tri`].
#[derive(Clone, Copy, Debug)]
pub struct FileId(Option<(u64, u64)>);

impl FileId {
    /// No identity: the system gave none.
    pub const UNKNOWN: FileId = FileId(None);

    /// The file of inode `ino` on device `dev`; an inode of 0 is no identity (the value the
    /// systems that have none, and the records written before ids were kept, hold).
    #[must_use]
    pub fn new(dev: u64, ino: u64) -> FileId {
        if ino == 0 {
            FileId(None)
        } else {
            FileId(Some((dev, ino)))
        }
    }

    /// Whether the system gave an identity.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.0.is_some()
    }

    /// The device and the inode, when known.
    #[must_use]
    pub fn known(&self) -> Option<(u64, u64)> {
        self.0
    }

    /// The device and the inode as a record keeps them (`(0, 0)` for none).
    #[must_use]
    pub fn parts(&self) -> (u64, u64) {
        self.0.unwrap_or((0, 0))
    }

    /// Whether `other` is the same file: `Unknown` when either has no identity.
    #[must_use]
    pub fn same(&self, other: &FileId) -> Tri {
        match (self.0, other.0) {
            (Some(a), Some(b)) if a == b => Tri::Same,
            (Some(_), Some(_)) => Tri::Different,
            _ => Tri::Unknown,
        }
    }
}

/// What an entry was when it was looked at: enough to tell, later, whether it is still that
/// file and nothing wrote to it ([`Fingerprint::matches`]). A step that replaces or removes a
/// file on the strength of a fingerprint goes on only when the answer is [`Tri::Same`].
#[derive(Clone, Copy, Debug)]
pub struct Fingerprint {
    pub kind: Kind,
    pub size: u64,
    /// Nanoseconds since 1970; 0 where the system gives none.
    pub mtime_ns: u64,
    /// The status-change time; 0 where the system gives none (then it is not compared).
    pub ctime_ns: u64,
    pub id: FileId,
}

impl Fingerprint {
    /// The fingerprint of an `lstat`.
    #[must_use]
    pub fn of(st: &Stat) -> Fingerprint {
        Fingerprint {
            kind: st.kind,
            size: st.size,
            mtime_ns: st.mtime_ns,
            ctime_ns: st.ctime_ns,
            id: st.id(),
        }
    }

    /// The fingerprint a record kept as its parts (an inode of 0: no identity).
    #[must_use]
    pub fn from_parts(
        kind: Kind,
        size: u64,
        mtime_ns: u64,
        ctime_ns: u64,
        dev: u64,
        ino: u64,
    ) -> Fingerprint {
        Fingerprint {
            kind,
            size,
            mtime_ns,
            ctime_ns,
            id: FileId::new(dev, ino),
        }
    }

    /// Whether `st` (an `lstat` now) is still this file untouched: [`Tri::Different`] when its
    /// kind, size or times differ or it is another file, [`Tri::Same`] only when all of them
    /// match and the identity is known on both sides, [`Tri::Unknown`] when everything that can
    /// be told matches and the identity cannot (then a destructive step degrades to a content
    /// hash or refuses - it does not go on).
    #[must_use]
    pub fn matches(&self, st: &Stat) -> Tri {
        if self.kind != st.kind || self.size != st.size || self.mtime_ns != st.mtime_ns {
            return Tri::Different;
        }
        if self.ctime_ns != 0 && st.ctime_ns != 0 && self.ctime_ns != st.ctime_ns {
            return Tri::Different;
        }
        self.id.same(&st.id())
    }
}

impl Stat {
    /// Which file it is.
    #[must_use]
    pub fn id(&self) -> FileId {
        FileId::new(self.dev, self.ino)
    }

    /// What it is now, to be matched against later ([`Fingerprint::matches`]).
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(self)
    }

    /// Whether it is, for certain, the same file as `other` (inode and device known and
    /// equal). Two entries the system gave no identity are not.
    #[must_use]
    pub fn same_file(&self, other: &Stat) -> bool {
        self.id().same(&other.id()).is_same()
    }

    /// Whether it is, for certain, the same file with the same size and times as `other`:
    /// nothing wrote to it in between.
    #[must_use]
    pub fn unchanged(&self, other: &Stat) -> bool {
        self.fingerprint().matches(other).is_same()
    }

    /// The user's executable bit.
    #[must_use]
    pub fn executable(&self) -> bool {
        self.mode & 0o100 != 0
    }
}

/// One name of a path: no separator, no NUL, not `.`, `..` or empty.
fn check_name(name: &str) -> Result<(), BeneathError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(BeneathError::BadName(name.to_string()));
    }
    // Windows reads `C:x` as a path of its own (relative to drive C) and `a:s` as a stream:
    // joined to a folder's path, such a name leaves the folder.
    if crate::key::System::here() == crate::key::System::Windows
        && crate::key::windows_name_problem(name).is_some()
    {
        return Err(BeneathError::BadName(name.to_string()));
    }
    Ok(())
}

/// Whether `name` can be one file name of a folder here: no separator, no NUL, not `.`, `..` or
/// empty, and on Windows none that system misreads.
///
/// # Errors
///
/// [`BeneathError::BadName`].
pub fn check_file_name(name: &str) -> Result<(), BeneathError> {
    check_name(name)
}

/// The folder part and the name of a key: `a/b/c.txt` -> (`a/b`, `c.txt`).
#[must_use]
pub fn split_key(key: &str) -> (&str, &str) {
    match key.rfind('/') {
        Some(i) => (&key[..i], &key[i + 1..]),
        None => ("", key),
    }
}

/// A root folder, opened once: every path below it is walked from here.
pub struct Root {
    path: PathBuf,
    dev: u64,
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    /// Linux: the mount points below the root (bind mounts share the root's device number, so
    /// only the mount table tells them apart).
    mounts: Vec<PathBuf>,
    /// Windows: the root's own path as the system resolves it (links and junctions followed), which
    /// every file and folder opened below the root must still lie under (AZ-W14). `None`: the
    /// system did not say; the walk is then checked before each step only.
    #[cfg(windows)]
    anchor: Option<std::sync::Arc<str>>,
}

impl fmt::Debug for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Root").field("path", &self.path).finish()
    }
}

/// A folder below a [`Root`], reached without a link.
pub struct Dir {
    /// Its path from the root (`""` is the root, `a/b` a folder in it).
    rel: String,
    path: PathBuf,
    /// The device it is on (0 where the system gives none): a folder below it on another device is
    /// a disk mounted there, or - on Linux - a subvolume of the same file system.
    dev: u64,
    /// Windows: the root's resolved path ([`Root`]).
    #[cfg(windows)]
    anchor: Option<std::sync::Arc<str>>,
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
}

impl fmt::Debug for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dir").field("rel", &self.rel).finish()
    }
}

impl Root {
    /// Opens the folder at `path` (the path itself may be a link the user chose: it is the
    /// root; nothing below it is followed).
    ///
    /// # Errors
    ///
    /// When it does not exist or is no folder.
    pub fn open(path: &Path) -> io::Result<Root> {
        #[cfg(unix)]
        {
            let fd = sys::open_dir_path(path)?;
            let dev = sys::fstat(&fd)?.dev;
            Ok(Root {
                path: path.to_path_buf(),
                dev,
                fd,
                mounts: mounts_below(path),
            })
        }
        #[cfg(not(unix))]
        {
            let meta = std::fs::metadata(path)?;
            if !meta.is_dir() {
                return Err(io::Error::other("not a folder"));
            }
            Ok(Root {
                path: path.to_path_buf(),
                dev: 0,
                mounts: Vec::new(),
                #[cfg(windows)]
                anchor: crate::win::resolved(path),
            })
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The root's own folder.
    ///
    /// # Errors
    ///
    /// When it cannot be opened again.
    pub fn top(&self) -> Result<Dir, BeneathError> {
        #[cfg(unix)]
        {
            Ok(Dir {
                rel: String::new(),
                path: self.path.clone(),
                dev: self.dev,
                fd: self.fd.try_clone()?,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Dir {
                rel: String::new(),
                path: self.path.clone(),
                dev: self.dev,
                #[cfg(windows)]
                anchor: self.anchor.clone(),
            })
        }
    }

    /// The folder `rel` (`""`, `a`, `a/b`), walked one name at a time; with `create`, the
    /// folders that are missing are made.
    ///
    /// # Errors
    ///
    /// [`BeneathError::ThroughLink`] when a name of it is a link or a file,
    /// [`BeneathError::CrossesMount`] when one is another disk, an I/O error otherwise (a
    /// missing folder without `create`: not found).
    pub fn dir(&self, rel: &str, create: bool) -> Result<Dir, BeneathError> {
        let mut dir = self.top()?;
        for name in rel.split('/').filter(|s| !s.is_empty()) {
            dir = dir.child(name, create)?;
            if self.is_mount_point(&dir.rel) {
                return Err(BeneathError::CrossesMount(dir.rel.clone()));
            }
        }
        Ok(dir)
    }

    /// The folder of `key` and the key's last name: `a/b/c.txt` -> (the folder `a/b`, `c.txt`).
    /// The key is a [`LocalKey`]: a name from a drive, a device, a user or a listing reaches a path
    /// only after [`crate::names::Policy::admit`] let it through (ARCH28 RF42), so no new entry
    /// point can skip the name policy.
    ///
    /// # Errors
    ///
    /// As [`Root::dir`], and [`BeneathError::BadName`] for a key without a name.
    pub fn parent_of<'k>(
        &self,
        key: &'k LocalKey,
        create: bool,
    ) -> Result<(Dir, &'k str), BeneathError> {
        let (folder, name) = split_key(key.as_str());
        check_name(name)?;
        Ok((self.dir(folder, create)?, name))
    }

    /// Whether the folder `rel` is a mount point the device numbers do not show (Linux bind
    /// mounts).
    #[must_use]
    pub fn is_mount_point(&self, rel: &str) -> bool {
        !rel.is_empty() && !self.mounts.is_empty() && {
            let mut path = self.path.clone();
            for name in rel.split('/') {
                path.push(name);
            }
            self.mounts.contains(&path)
        }
    }

    /// The device number of the root's file system.
    #[must_use]
    pub fn dev(&self) -> u64 {
        self.dev
    }
}

impl Dir {
    /// Its path from the root.
    #[must_use]
    pub fn rel(&self) -> &str {
        &self.rel
    }

    /// Its path (for messages, and the calls that only take paths).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Windows: the folder is still under the root's resolved path, asked right before a step that
    /// names a file by path and opens nothing (rename, unlink, rmdir) - a junction swapped in for a
    /// folder above it since it was walked would send the step to another tree. The window between
    /// this look and the step is that of one system call; only a handle-relative step closes it
    /// (AZ-W14: `NtCreateFile` relative to a directory handle, which std does not give).
    #[cfg(windows)]
    fn still_here(&self) -> Result<(), BeneathError> {
        still_beneath(
            self.anchor.as_deref(),
            &crate::win::open_dir(&self.path)?,
            &self.rel,
        )
    }

    fn rel_of(&self, name: &str) -> String {
        if self.rel.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.rel)
        }
    }

    /// The folder `name` in it, never through a link and never into another disk; with
    /// `create`, made when missing.
    ///
    /// # Errors
    ///
    /// As [`Root::dir`].
    pub fn child(&self, name: &str, create: bool) -> Result<Dir, BeneathError> {
        check_name(name)?;
        let rel = self.rel_of(name);
        #[cfg(unix)]
        {
            let fd = match sys::openat_dir(&self.fd, OsStr::new(name)) {
                Ok(fd) => fd,
                Err(e) if e.kind() == io::ErrorKind::NotFound && create => {
                    match sys::mkdirat(&self.fd, OsStr::new(name), 0o755) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.into()),
                    }
                    sys::openat_dir(&self.fd, OsStr::new(name)).map_err(|e| link_or_io(e, &rel))?
                }
                Err(e) => return Err(link_or_io(e, &rel)),
            };
            let st = sys::fstat(&fd)?;
            let path = self.path.join(name);
            // Another device is another disk mounted here - unless the mount table does not list the
            // folder: btrfs gives every subvolume a device number of its own without mounting it
            // (AZ-W15), and a folder in one is as much in the synced folder as any other.
            if st.dev != self.dev && is_disk(&path) {
                return Err(BeneathError::CrossesMount(rel));
            }
            Ok(Dir {
                rel,
                path,
                dev: st.dev,
                fd,
            })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            let st = match lstat_path(&path) {
                Ok(st) => st,
                Err(e) if e.kind() == io::ErrorKind::NotFound && create => {
                    std::fs::create_dir(&path).or_else(|e| {
                        if e.kind() == io::ErrorKind::AlreadyExists {
                            Ok(())
                        } else {
                            Err(e)
                        }
                    })?;
                    lstat_path(&path)?
                }
                Err(e) => return Err(e.into()),
            };
            if st.kind != Kind::Dir {
                return Err(BeneathError::ThroughLink(rel));
            }
            // The folder opened, and where it really is: a junction swapped in for a folder above it
            // since the walk looked sends the open to another tree.
            #[cfg(windows)]
            still_beneath(self.anchor.as_deref(), &crate::win::open_dir(&path)?, &rel)?;
            Ok(Dir {
                rel,
                path,
                dev: self.dev,
                #[cfg(windows)]
                anchor: self.anchor.clone(),
            })
        }
    }

    /// The folder's own `fstat`.
    ///
    /// # Errors
    ///
    /// When the system refuses it.
    pub fn stat(&self) -> io::Result<Stat> {
        #[cfg(unix)]
        {
            sys::fstat(&self.fd)
        }
        #[cfg(not(unix))]
        {
            lstat_path(&self.path)
        }
    }

    /// `lstat` of the entry `name` (a link is the link).
    ///
    /// # Errors
    ///
    /// When it does not exist (not found) or cannot be read.
    pub fn lstat(&self, name: &str) -> io::Result<Stat> {
        check_name(name).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        #[cfg(unix)]
        {
            sys::fstatat(&self.fd, OsStr::new(name))
        }
        #[cfg(not(unix))]
        {
            lstat_path(&self.path.join(name))
        }
    }

    /// The names in the folder (without `.` and `..`), read whole: an error part of the way is
    /// an error, never a shorter list.
    ///
    /// # Errors
    ///
    /// When the folder cannot be read.
    pub fn names(&self) -> io::Result<Vec<OsString>> {
        #[cfg(unix)]
        {
            sys::list(&self.fd)
        }
        #[cfg(not(unix))]
        {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(&self.path)? {
                out.push(entry?.file_name());
            }
            Ok(out)
        }
    }

    /// The target of the link `name`.
    ///
    /// # Errors
    ///
    /// When it is no link or cannot be read.
    pub fn read_link(&self, name: &str) -> io::Result<PathBuf> {
        #[cfg(unix)]
        {
            sys::readlinkat(&self.fd, OsStr::new(name)).map(PathBuf::from)
        }
        #[cfg(not(unix))]
        {
            std::fs::read_link(self.path.join(name))
        }
    }

    /// The regular file `name`, opened for reading without following a link and without
    /// blocking on a FIFO, with its `fstat`.
    ///
    /// # Errors
    ///
    /// [`BeneathError::ThroughLink`] when it is a link or no regular file, an I/O error
    /// otherwise (not found when it is gone).
    pub fn open_read(&self, name: &str) -> Result<(File, Stat), BeneathError> {
        check_name(name)?;
        let rel = self.rel_of(name);
        #[cfg(unix)]
        {
            let fd = sys::open_read(&self.fd, OsStr::new(name)).map_err(|e| link_or_io(e, &rel))?;
            let st = sys::fstat(&fd)?;
            if st.kind != Kind::File {
                return Err(BeneathError::ThroughLink(rel));
            }
            // Reads of a regular file never block; the flag only kept a FIFO from blocking
            // the open itself.
            sys::clear_nonblock(&fd)?;
            Ok((File::from(fd), st))
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            if lstat_path(&path)?.kind != Kind::File {
                return Err(BeneathError::ThroughLink(rel));
            }
            let file = File::open(&path)?;
            // Opened through a path, so a folder swapped for a junction since the walk would have
            // sent the open to another tree: the file says where it is.
            #[cfg(windows)]
            still_beneath(self.anchor.as_deref(), &file, &rel)?;
            let st = fstat_file(&file)?;
            if st.kind != Kind::File {
                return Err(BeneathError::ThroughLink(rel));
            }
            Ok((file, st))
        }
    }

    /// A new file `name` (it must not exist; a link there is not followed), with `mode`.
    ///
    /// # Errors
    ///
    /// When it exists or cannot be made.
    pub fn create_new(&self, name: &str, mode: u32) -> Result<File, BeneathError> {
        check_name(name)?;
        #[cfg(unix)]
        {
            Ok(File::from(sys::create_new(
                &self.fd,
                OsStr::new(name),
                mode,
            )?))
        }
        #[cfg(not(unix))]
        {
            let _ = mode;
            // Read as well as write: a handle that cannot read attributes cannot be asked for its
            // identity and times ([`stat_of`]).
            let path = self.path.join(name);
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)?;
            #[cfg(windows)]
            if let Err(e) = still_beneath(self.anchor.as_deref(), &file, &self.rel_of(name)) {
                // The folder was swapped for a junction since it was walked, and the file was made
                // in another tree. It is the one this call just made: it goes again.
                drop(file);
                // collapse-ok: a file we made a moment ago that cannot be removed again is the
                // error already being returned; the refusal is what matters.
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
            Ok(file)
        }
    }

    /// Renames `from` in this folder to `to` in `target` (both of the same root; a link named
    /// `from` moves as the link). An existing `to` is replaced, as `rename` does: callers that
    /// must not replace check first.
    ///
    /// # Errors
    ///
    /// When the system refuses it.
    pub fn rename(&self, from: &str, target: &Dir, to: &str) -> Result<(), BeneathError> {
        check_name(from)?;
        check_name(to)?;
        #[cfg(unix)]
        {
            Ok(sys::renameat(
                &self.fd,
                OsStr::new(from),
                &target.fd,
                OsStr::new(to),
            )?)
        }
        #[cfg(windows)]
        {
            self.still_here()?;
            target.still_here()?;
            Ok(crate::win::rename(
                &self.path.join(from),
                &target.path.join(to),
                true,
            )?)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(std::fs::rename(self.path.join(from), target.path.join(to))?)
        }
    }

    /// [`Dir::rename`] that never replaces: `to` is taken only when nothing has that name, in
    /// one step no other writer can come between (`renameat2(RENAME_NOREPLACE)` on Linux,
    /// `renameatx_np(RENAME_EXCL)` on macOS; a disk that has neither gets a hard link and an
    /// unlink, and one that has no links either gets a look and a rename - the weakest of the
    /// three, a short window between the look and the rename).
    ///
    /// # Errors
    ///
    /// [`BeneathError::Io`] of kind `AlreadyExists` ([`BeneathError::is_already_exists`]) when
    /// something has the name; otherwise as [`Dir::rename`].
    pub fn rename_noreplace(&self, from: &str, target: &Dir, to: &str) -> Result<(), BeneathError> {
        check_name(from)?;
        check_name(to)?;
        #[cfg(unix)]
        {
            Ok(sys::renameat_noreplace(
                &self.fd,
                OsStr::new(from),
                &target.fd,
                OsStr::new(to),
            )?)
        }
        #[cfg(windows)]
        {
            // `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`: taken in the one step, or
            // `AlreadyExists`.
            self.still_here()?;
            target.still_here()?;
            Ok(crate::win::rename(
                &self.path.join(from),
                &target.path.join(to),
                false,
            )?)
        }
        #[cfg(not(any(unix, windows)))]
        {
            if std::fs::symlink_metadata(target.path.join(to)).is_ok() {
                return Err(BeneathError::Io(io::Error::from(
                    io::ErrorKind::AlreadyExists,
                )));
            }
            Ok(std::fs::rename(self.path.join(from), target.path.join(to))?)
        }
    }

    /// Gives the file `from` a second name `to` in `target` (a hard link), never over a name that
    /// is taken: the one way to put a file under another name while it keeps its own. A file that
    /// is renamed out of its place leaves the name empty until the next step; a link leaves both
    /// (X-DELTA-AZDRIVE-FILES-26).
    ///
    /// # Errors
    ///
    /// [`BeneathError::Io`] of kind `AlreadyExists` when `to` is taken, `NotFound` when `from` is
    /// not there, `Unsupported` when the file system makes no hard links ([`BeneathError::is_unsupported`]).
    pub fn link_noreplace(&self, from: &str, target: &Dir, to: &str) -> Result<(), BeneathError> {
        check_name(from)?;
        check_name(to)?;
        #[cfg(unix)]
        {
            match sys::linkat(&self.fd, OsStr::new(from), &target.fd, OsStr::new(to)) {
                Ok(()) => Ok(()),
                Err(e) if sys::no_links(&e) => Err(BeneathError::Io(io::Error::from(
                    io::ErrorKind::Unsupported,
                ))),
                Err(e) => Err(e.into()),
            }
        }
        #[cfg(windows)]
        {
            self.still_here()?;
            target.still_here()?;
            std::fs::hard_link(self.path.join(from), target.path.join(to)).map_err(|e| {
                // FAT and exFAT answer ERROR_INVALID_FUNCTION (1), a share ERROR_NOT_SUPPORTED (50).
                if matches!(e.raw_os_error(), Some(1 | 50)) {
                    BeneathError::Io(io::Error::from(io::ErrorKind::Unsupported))
                } else {
                    BeneathError::Io(e)
                }
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(BeneathError::Io(io::Error::from(
                io::ErrorKind::Unsupported,
            )))
        }
    }

    /// The folder at `path` as a [`Dir`], links followed: the folder a user chose (the folder of
    /// a download, of a settings file), whose own links are the user's, not a name from another
    /// device. Everything below it is reached without a link, as for any [`Dir`].
    ///
    /// # Errors
    ///
    /// When it does not exist or is no folder.
    pub fn open_following(path: &Path) -> io::Result<Dir> {
        let root = Root::open(path)?;
        root.top().map_err(|e| match e {
            BeneathError::Io(e) => e,
            other => io::Error::other(other.to_string()),
        })
    }

    /// Removes the file (or link) `name`.
    ///
    /// # Errors
    ///
    /// When it is a folder, is gone or cannot be removed.
    pub fn unlink(&self, name: &str) -> Result<(), BeneathError> {
        check_name(name)?;
        #[cfg(unix)]
        {
            Ok(sys::unlinkat(&self.fd, OsStr::new(name), false)?)
        }
        #[cfg(not(unix))]
        {
            #[cfg(windows)]
            self.still_here()?;
            Ok(std::fs::remove_file(self.path.join(name))?)
        }
    }

    /// Makes the link `name` here holding `target` (only the text: nothing is resolved or
    /// followed). A system whose links a sync does not make (Windows) refuses it.
    ///
    /// # Errors
    ///
    /// When something has that name, or the link cannot be made.
    pub fn symlink(&self, target: &str, name: &str) -> Result<(), BeneathError> {
        check_name(name)?;
        #[cfg(unix)]
        {
            Ok(sys::symlinkat(
                OsStr::new(target),
                &self.fd,
                OsStr::new(name),
            )?)
        }
        #[cfg(not(unix))]
        {
            let _ = target;
            Err(BeneathError::Io(io::Error::new(
                io::ErrorKind::Unsupported,
                "links are not made on this system",
            )))
        }
    }

    /// Removes the empty folder `name`.
    ///
    /// # Errors
    ///
    /// When it is not empty, is no folder or cannot be removed.
    pub fn rmdir(&self, name: &str) -> Result<(), BeneathError> {
        check_name(name)?;
        #[cfg(unix)]
        {
            Ok(sys::unlinkat(&self.fd, OsStr::new(name), true)?)
        }
        #[cfg(not(unix))]
        {
            #[cfg(windows)]
            self.still_here()?;
            Ok(std::fs::remove_dir(self.path.join(name))?)
        }
    }

    /// Makes the folder's entries durable (after a rename into it), and forgets how it went: for
    /// the callers that have nothing to decide on it. A write that is about to be called landed -
    /// and a source deleted on that word - uses [`Dir::sync_checked`].
    pub fn sync(&self) {
        #[cfg(unix)]
        {
            // collapse-ok: best effort by contract; the one caller that decides on it asks
            // `sync_checked`.
            let _ = sys::fsync(&self.fd);
        }
    }

    /// Makes the folder's entries durable and says when that could not be done. A file system that
    /// cannot sync a folder at all (FAT on macOS, a network or FUSE file system: `EINVAL`,
    /// `ENOTSUP`, `EOPNOTSUPP`, `ENOSYS`, [`folder_sync_refused`]) is `Ok`: there is nothing to
    /// make durable there. An I/O error (`EIO`, `ENOSPC`, `EDQUOT`) is an error: the entry that was
    /// just renamed in may not be on the disk (X-DELTA-AZDRIVE-FILES-14).
    ///
    /// # Errors
    ///
    /// The system's error, when it is not a refusal to sync a folder.
    pub fn sync_checked(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            match sys::fsync(&self.fd) {
                Err(e) if !folder_sync_refused(&e) => Err(e),
                _ => Ok(()),
            }
        }
        #[cfg(not(unix))]
        {
            // A rename is written through when it is made (`MOVEFILE_WRITE_THROUGH`): there is no
            // folder to flush.
            Ok(())
        }
    }
}

/// Whether `e` says the file system cannot sync a folder at all - a fact of the volume, no
/// failure of a write.
#[must_use]
pub fn folder_sync_refused(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        // Compared one by one: ENOTSUP and EOPNOTSUPP are one number on Linux.
        e.raw_os_error().is_some_and(|code| {
            code == libc::EINVAL
                || code == libc::ENOTSUP
                || code == libc::EOPNOTSUPP
                || code == libc::ENOSYS
        })
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// The name of a temporary file for `name` in its own folder; [`is_temp_name`] knows it.
#[must_use]
pub fn temp_name(name: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    // A long name is cut so the temporary one stays a valid name (255 bytes).
    let mut stem: String = String::new();
    for c in name.chars() {
        if stem.len() + c.len_utf8() > 160 {
            break;
        }
        stem.push(c);
    }
    format!(
        ".{stem}.azul-storage-{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// A temporary file of [`temp_name`] (or one a crash left).
#[must_use]
pub fn is_temp_name(name: &str) -> bool {
    name.starts_with('.') && name.contains(".azul-storage-") && name.ends_with(".tmp")
}

/// The `fstat` of an open file (after a read: did it change while it was read?).
///
/// # Errors
///
/// When the system refuses it.
pub fn stat_of(file: &File) -> io::Result<Stat> {
    #[cfg(unix)]
    {
        sys::fstat(file)
    }
    #[cfg(not(unix))]
    {
        fstat_file(file)
    }
}

/// The `stat` of the file at `path`, a link followed: for a path the user or an app named (a file
/// to upload), not for a name that came from a drive.
///
/// # Errors
///
/// When the system refuses it (gone: not found).
pub fn stat_path(path: &Path) -> io::Result<Stat> {
    #[cfg(unix)]
    {
        sys::stat_path(path)
    }
    #[cfg(not(unix))]
    {
        stat_following(path)
    }
}

/// The largest file the disk of `path` holds, when the file system limits it below what a sync
/// meets (FAT32: 4 GiB - 1 byte, [`crate::win::FAT_FILE_MAX`]); `None` otherwise, or where it
/// cannot be told.
#[must_use]
pub fn file_size_limit(path: &Path) -> Option<u64> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        sys::fs_type_name(path)
            .filter(|name| name == "msdos")
            .map(|_| crate::win::FAT_FILE_MAX)
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // MSDOS_SUPER_MAGIC: vfat; exFAT has no 4 GiB limit.
        sys::fs_type_magic(path)
            .filter(|magic| *magic == 0x4d44)
            .map(|_| crate::win::FAT_FILE_MAX)
    }
    #[cfg(windows)]
    {
        // FAT and FAT32 (a USB stick, an SD card); NTFS, exFAT and ReFS have no such limit.
        crate::win::file_system_name(path)
            .ok()
            .and_then(|name| crate::win::fat_cap(&name))
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android",
        windows
    )))]
    {
        let _ = path;
        None
    }
}

/// The bytes free for this user on the disk of `path`, or why they cannot be told. The caller
/// matches it and applies a [`crate::cap::Posture`]: an unknown is never "unlimited".
pub fn free_space(path: &Path) -> Cap<u64> {
    #[cfg(unix)]
    {
        match sys::free_space(path) {
            Ok(bytes) => Cap::Known(bytes),
            Err(_) => Cap::Unknown("the system did not tell the free space of this disk"),
        }
    }
    #[cfg(windows)]
    {
        match crate::win::free_space(path) {
            Ok(bytes) => Cap::Known(bytes),
            Err(_) => Cap::Unknown("the system did not tell the free space of this disk"),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Cap::Unknown("this system does not tell the free space of a disk yet")
    }
}

#[cfg(unix)]
fn link_or_io(e: io::Error, rel: &str) -> BeneathError {
    // A link opened with O_NOFOLLOW: ELOOP (EMLINK on FreeBSD); a file where a folder was
    // asked for: ENOTDIR.
    match e.raw_os_error() {
        Some(code) if code == libc::ELOOP || code == libc::ENOTDIR || code == libc::EMLINK => {
            BeneathError::ThroughLink(rel.to_string())
        }
        _ => BeneathError::Io(e),
    }
}

/// `lstat` of the entry at `path`: a link is the link. For the systems without descriptors.
#[cfg(not(unix))]
fn lstat_path(path: &Path) -> io::Result<Stat> {
    #[cfg(windows)]
    {
        crate::win::lstat(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(path).map(|m| stat_of_meta(&m))
    }
}

/// `stat` of the entry at `path`, a link followed.
#[cfg(not(unix))]
fn stat_following(path: &Path) -> io::Result<Stat> {
    #[cfg(windows)]
    {
        crate::win::stat(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::metadata(path).map(|m| stat_of_meta(&m))
    }
}

/// `fstat` of an open file.
#[cfg(not(unix))]
fn fstat_file(file: &File) -> io::Result<Stat> {
    #[cfg(windows)]
    {
        crate::win::fstat(file)
    }
    #[cfg(not(windows))]
    {
        file.metadata().map(|m| stat_of_meta(&m))
    }
}

/// A system that is neither Unix nor Windows: what std's metadata says, and no identity.
#[cfg(not(any(unix, windows)))]
fn stat_of_meta(meta: &std::fs::Metadata) -> Stat {
    let kind = if meta.file_type().is_symlink() {
        Kind::Link
    } else if meta.is_dir() {
        Kind::Dir
    } else if meta.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64);
    Stat {
        kind,
        size: meta.len(),
        mtime_ns,
        ctime_ns: 0,
        ino: 0,
        dev: 0,
        mode: if meta.permissions().readonly() {
            0o444
        } else {
            0o644
        },
        nlink: 1,
    }
}

/// The mount points `/proc/self/mountinfo` lists (its fifth field of each line), unescaped.
#[cfg(unix)]
pub(crate) fn mount_points(text: &str) -> impl Iterator<Item = PathBuf> + '_ {
    text.lines()
        .filter_map(|line| line.split(' ').nth(4))
        .map(|point| PathBuf::from(unescape_mount(point)))
}

/// The mount points strictly below `root`, as the caller spells it (Linux: bind mounts share the
/// root's device number, so only the mount table tells them apart; elsewhere the table cannot be read
/// and the device numbers do).
#[cfg(unix)]
fn mounts_below(root: &Path) -> Vec<PathBuf> {
    let Ok(real) = std::fs::canonicalize(root) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(MOUNT_TABLE) else {
        return Vec::new();
    };
    mount_points(&text)
        .filter(|point| *point != real)
        .filter_map(|point| point.strip_prefix(&real).ok().map(|rest| root.join(rest)))
        .collect()
}

/// The kernel's table of mounts.
#[cfg(unix)]
const MOUNT_TABLE: &str = "/proc/self/mountinfo";

/// Whether the folder at `path`, which is on another device than the folder it is in, is a disk
/// mounted there. `table` is the mount table's text (`None`: it could not be read - macOS has none -
/// so the device number is all there is), `real` the folder's resolved path (`None`: it could not
/// be resolved). Only a table that was read and does not list the folder says it is no disk.
#[cfg(unix)]
pub(crate) fn is_disk_in(table: Option<&str>, real: Option<&Path>) -> bool {
    match (table, real) {
        (Some(table), Some(real)) => mount_points(table).any(|point| point == real),
        _ => true,
    }
}

/// [`is_disk_in`] for the table and the folder as they are now.
#[cfg(unix)]
fn is_disk(path: &Path) -> bool {
    let table = std::fs::read_to_string(MOUNT_TABLE).ok();
    let real = std::fs::canonicalize(path).ok();
    is_disk_in(table.as_deref(), real.as_deref())
}

/// A mountinfo field with its octal escapes (`\040` is a space) undone.
#[cfg(unix)]
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = i + 3 < bytes.len()
            && bytes[i] == b'\\'
            && bytes[i + 1..=i + 3]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b));
        if octal {
            let digit = |b: u8| u32::from(b - b'0');
            let n = digit(bytes[i + 1]) * 64 + digit(bytes[i + 2]) * 8 + digit(bytes[i + 3]);
            out.push(n as u8);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(unix)]
mod sys {
    use std::{
        ffi::{CStr, CString, OsStr, OsString},
        io,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::{OsStrExt, OsStringExt},
        },
        path::Path,
    };

    use super::{Kind, Stat};

    fn cstr(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in a file name"))
    }

    fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(r)
        }
    }

    fn owned(fd: libc::c_int) -> OwnedFd {
        // SAFETY: `fd` was just returned by a successful open and is owned by no one else.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    pub fn open_dir_path(path: &Path) -> io::Result<OwnedFd> {
        let c = cstr(path.as_os_str())?;
        // SAFETY: a valid C string and plain flags.
        let fd = cvt(unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })?;
        Ok(owned(fd))
    }

    pub fn openat_dir(dir: &OwnedFd, name: &OsStr) -> io::Result<OwnedFd> {
        let c = cstr(name)?;
        // SAFETY: an open descriptor, a valid C string and plain flags.
        let fd = cvt(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        Ok(owned(fd))
    }

    pub fn mkdirat(dir: &OwnedFd, name: &OsStr, mode: u32) -> io::Result<()> {
        let c = cstr(name)?;
        // SAFETY: an open descriptor and a valid C string.
        cvt(unsafe { libc::mkdirat(dir.as_raw_fd(), c.as_ptr(), mode as libc::mode_t) })?;
        Ok(())
    }

    pub fn open_read(dir: &OwnedFd, name: &OsStr) -> io::Result<OwnedFd> {
        let c = cstr(name)?;
        // SAFETY: an open descriptor, a valid C string and plain flags.
        let fd = cvt(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
        Ok(owned(fd))
    }

    pub fn create_new(dir: &OwnedFd, name: &OsStr, mode: u32) -> io::Result<OwnedFd> {
        let c = cstr(name)?;
        // SAFETY: an open descriptor, a valid C string, plain flags and a mode.
        let fd = cvt(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode as libc::c_uint,
            )
        })?;
        Ok(owned(fd))
    }

    pub fn clear_nonblock(fd: &OwnedFd) -> io::Result<()> {
        // SAFETY: an open descriptor; F_GETFL / F_SETFL take and return plain flags.
        let flags = cvt(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) })?;
        cvt(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK) })?;
        Ok(())
    }

    fn ns(secs: i64, nsec: i64) -> u64 {
        if secs < 0 {
            0
        } else {
            (secs as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(nsec.max(0) as u64)
        }
    }

    fn stat_of(st: &libc::stat) -> Stat {
        let mode = st.st_mode as u32;
        let fmt = mode & (libc::S_IFMT as u32);
        let kind = if fmt == libc::S_IFREG as u32 {
            Kind::File
        } else if fmt == libc::S_IFDIR as u32 {
            Kind::Dir
        } else if fmt == libc::S_IFLNK as u32 {
            Kind::Link
        } else if fmt == libc::S_IFIFO as u32 {
            Kind::Fifo
        } else if fmt == libc::S_IFSOCK as u32 {
            Kind::Socket
        } else if fmt == libc::S_IFCHR as u32 || fmt == libc::S_IFBLK as u32 {
            Kind::Device
        } else {
            Kind::Other
        };
        Stat {
            kind,
            size: st.st_size as u64,
            mtime_ns: ns(st.st_mtime as i64, st.st_mtime_nsec as i64),
            ctime_ns: ns(st.st_ctime as i64, st.st_ctime_nsec as i64),
            ino: st.st_ino as u64,
            dev: st.st_dev as u64,
            mode,
            nlink: st.st_nlink as u64,
        }
    }

    pub fn fstat(fd: &impl AsRawFd) -> io::Result<Stat> {
        // SAFETY: zeroed is a valid `stat`; fstat fills it from an open descriptor.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        cvt(unsafe { libc::fstat(fd.as_raw_fd(), &mut st) })?;
        Ok(stat_of(&st))
    }

    pub fn stat_path(path: &Path) -> io::Result<Stat> {
        let c = cstr(path.as_os_str())?;
        // SAFETY: as fstat; the path is a valid C string, a link is followed.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        cvt(unsafe { libc::stat(c.as_ptr(), &mut st) })?;
        Ok(stat_of(&st))
    }

    pub fn fstatat(dir: &OwnedFd, name: &OsStr) -> io::Result<Stat> {
        let c = cstr(name)?;
        // SAFETY: as fstat; the name is a valid C string, a link is not followed.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        cvt(unsafe {
            libc::fstatat(
                dir.as_raw_fd(),
                c.as_ptr(),
                &mut st,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        Ok(stat_of(&st))
    }

    pub fn renameat(d1: &OwnedFd, n1: &OsStr, d2: &OwnedFd, n2: &OsStr) -> io::Result<()> {
        let (c1, c2) = (cstr(n1)?, cstr(n2)?);
        // SAFETY: open descriptors and valid C strings.
        cvt(unsafe { libc::renameat(d1.as_raw_fd(), c1.as_ptr(), d2.as_raw_fd(), c2.as_ptr()) })?;
        Ok(())
    }

    pub fn unlinkat(dir: &OwnedFd, name: &OsStr, folder: bool) -> io::Result<()> {
        let c = cstr(name)?;
        let flags = if folder { libc::AT_REMOVEDIR } else { 0 };
        // SAFETY: an open descriptor and a valid C string.
        cvt(unsafe { libc::unlinkat(dir.as_raw_fd(), c.as_ptr(), flags) })?;
        Ok(())
    }

    pub fn symlinkat(target: &OsStr, dir: &OwnedFd, name: &OsStr) -> io::Result<()> {
        let (t, c) = (cstr(target)?, cstr(name)?);
        // SAFETY: an open descriptor and valid C strings.
        cvt(unsafe { libc::symlinkat(t.as_ptr(), dir.as_raw_fd(), c.as_ptr()) })?;
        Ok(())
    }

    pub fn readlinkat(dir: &OwnedFd, name: &OsStr) -> io::Result<OsString> {
        let c = cstr(name)?;
        let mut buf = vec![0u8; 4096];
        // SAFETY: the buffer is as long as said; readlinkat writes at most that many bytes.
        let n = unsafe {
            libc::readlinkat(
                dir.as_raw_fd(),
                c.as_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(n as usize);
        Ok(OsString::from_vec(buf))
    }

    /// Makes what was written to the descriptor's file or folder durable. On macOS a plain
    /// `fsync` only hands the data to the drive, which may keep it in its cache through a power
    /// cut (and std's `sync_all` asks `F_FULLFSYNC` for the same reason): the full sync is asked
    /// first, and a file system that does not know it (FAT, a network share) gets the plain one.
    pub fn fsync(fd: &OwnedFd) -> io::Result<()> {
        #[cfg(target_vendor = "apple")]
        {
            // SAFETY: an open descriptor; F_FULLFSYNC takes no argument.
            if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
                return Ok(());
            }
        }
        // SAFETY: an open descriptor.
        cvt(unsafe { libc::fsync(fd.as_raw_fd()) })?;
        Ok(())
    }

    /// `linkat`: a second name `n2` for the file `n1` (the name itself, a link is not followed),
    /// failing with `EEXIST` when `n2` is taken.
    pub fn linkat(d1: &OwnedFd, n1: &OsStr, d2: &OwnedFd, n2: &OsStr) -> io::Result<()> {
        let (c1, c2) = (cstr(n1)?, cstr(n2)?);
        // SAFETY: open descriptors and valid C strings; flag 0 links the name, not what it points to.
        cvt(unsafe { libc::linkat(d1.as_raw_fd(), c1.as_ptr(), d2.as_raw_fd(), c2.as_ptr(), 0) })?;
        Ok(())
    }

    /// `renameat` that fails with `EEXIST` instead of replacing `n2`.
    pub fn renameat_noreplace(
        d1: &OwnedFd,
        n1: &OsStr,
        d2: &OwnedFd,
        n2: &OsStr,
    ) -> io::Result<()> {
        let (c1, c2) = (cstr(n1)?, cstr(n2)?);
        #[cfg(target_os = "linux")]
        {
            // SAFETY: open descriptors, valid C strings and the flag the call defines; the
            // arguments are passed as the `long`s the system call takes.
            let done = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    libc::c_long::from(d1.as_raw_fd()),
                    c1.as_ptr() as libc::c_long,
                    libc::c_long::from(d2.as_raw_fd()),
                    c2.as_ptr() as libc::c_long,
                    libc::RENAME_NOREPLACE as libc::c_long,
                )
            };
            if done == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if !flag_unsupported(&e) {
                return Err(e);
            }
        }
        #[cfg(target_vendor = "apple")]
        {
            // SAFETY: open descriptors, valid C strings and the flag the call defines.
            let done = unsafe {
                libc::renameatx_np(
                    d1.as_raw_fd(),
                    c1.as_ptr(),
                    d2.as_raw_fd(),
                    c2.as_ptr(),
                    libc::RENAME_EXCL,
                )
            };
            if done == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if !flag_unsupported(&e) {
                return Err(e);
            }
        }
        // The file system does not take the flag: a hard link takes the new name or fails if it
        // is taken, then the old name goes.
        // SAFETY: open descriptors and valid C strings.
        let linked =
            unsafe { libc::linkat(d1.as_raw_fd(), c1.as_ptr(), d2.as_raw_fd(), c2.as_ptr(), 0) };
        if linked == 0 {
            // SAFETY: an open descriptor and a valid C string.
            cvt(unsafe { libc::unlinkat(d1.as_raw_fd(), c1.as_ptr(), 0) })?;
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::AlreadyExists || (!flag_unsupported(&e) && !no_links(&e)) {
            return Err(e);
        }
        // No links either (FAT, exFAT, some shares): a look, then the rename.
        match fstatat(d2, n2) {
            Ok(_) => Err(io::Error::from(io::ErrorKind::AlreadyExists)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => renameat(d1, n1, d2, n2),
            Err(e) => Err(e),
        }
    }

    /// Whether `e` says the file system does not know the rename flag asked for.
    fn flag_unsupported(e: &io::Error) -> bool {
        matches!(
            e.raw_os_error(),
            Some(code) if code == libc::ENOSYS
                || code == libc::EINVAL
                || code == libc::ENOTSUP
                || code == libc::EOPNOTSUPP
        )
    }

    /// Whether `e` says the file system makes no hard links.
    pub fn no_links(e: &io::Error) -> bool {
        matches!(
            e.raw_os_error(),
            Some(code) if code == libc::EPERM
                || code == libc::ENOTSUP
                || code == libc::EOPNOTSUPP
                || code == libc::EMLINK
        )
    }

    /// errno set to 0, so the end of a listing is told from an error.
    fn clear_errno() {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            // SAFETY: the thread's own errno.
            unsafe { *libc::__errno_location() = 0 };
        }
        #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
        {
            // SAFETY: the thread's own errno.
            unsafe { *libc::__error() = 0 };
        }
    }

    pub fn list(dir: &OwnedFd) -> io::Result<Vec<OsString>> {
        // A copy of the descriptor for the stream (closedir closes it); it shares the offset,
        // so the stream starts again from the first entry.
        // SAFETY: an open descriptor.
        let dup = cvt(unsafe { libc::fcntl(dir.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) })?;
        // SAFETY: `dup` is open and ours; fdopendir takes it over on success.
        let stream = unsafe { libc::fdopendir(dup) };
        if stream.is_null() {
            let e = io::Error::last_os_error();
            // SAFETY: fdopendir failed, so `dup` is still ours to close.
            unsafe { libc::close(dup) };
            return Err(e);
        }
        // SAFETY: a valid stream.
        unsafe { libc::rewinddir(stream) };
        let mut out = Vec::new();
        let result = loop {
            clear_errno();
            // SAFETY: a valid stream; the entry stays valid until the next readdir.
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                let e = io::Error::last_os_error();
                break match e.raw_os_error() {
                    Some(0) | None => Ok(()),
                    Some(_) => Err(e),
                };
            }
            // SAFETY: d_name is a NUL-terminated name inside the entry.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name != b"." && name != b".." {
                out.push(OsString::from_vec(name.to_vec()));
            }
        };
        // SAFETY: a valid stream, closed once (it closes `dup`).
        unsafe { libc::closedir(stream) };
        result.map(|()| out)
    }

    /// The file system's type name (`apfs`, `msdos`, `exfat`).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub fn fs_type_name(path: &Path) -> Option<String> {
        let c = cstr(path.as_os_str()).ok()?;
        // SAFETY: zeroed is a valid `statfs`; statfs fills it.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        // SAFETY: f_fstypename is a NUL-terminated name inside the struct.
        let name = unsafe { CStr::from_ptr(st.f_fstypename.as_ptr()) };
        Some(name.to_string_lossy().into_owned())
    }

    /// The file system's magic number (`f_type`).
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub fn fs_type_magic(path: &Path) -> Option<u64> {
        let c = cstr(path.as_os_str()).ok()?;
        // SAFETY: zeroed is a valid `statfs`; statfs fills it.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        #[allow(clippy::unnecessary_cast)]
        Some(st.f_type as u64)
    }

    pub fn free_space(path: &Path) -> io::Result<u64> {
        let c = cstr(path.as_os_str())?;
        // SAFETY: zeroed is a valid `statvfs`; statvfs fills it.
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        cvt(unsafe { libc::statvfs(c.as_ptr(), &mut st) })?;
        Ok((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
    }
}
