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
//! Windows has no descriptor-relative calls in std: there each name of the path is checked with
//! `symlink_metadata` right before the step (links and junctions are reparse points). That
//! leaves a short window between the check and the step.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    fs::File,
    io,
    path::{Path, PathBuf},
};

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
}

/// Whether `e` says the disk is full.
#[must_use]
pub fn is_disk_full(e: &io::Error) -> bool {
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

impl Stat {
    /// Whether it is the same file as `other` (inode and device).
    #[must_use]
    pub fn same_file(&self, other: &Stat) -> bool {
        self.ino == other.ino && self.dev == other.dev
    }

    /// Whether it is the same file with the same size and times as `other`: nothing wrote to
    /// it in between.
    #[must_use]
    pub fn unchanged(&self, other: &Stat) -> bool {
        self.same_file(other)
            && self.kind == other.kind
            && self.size == other.size
            && self.mtime_ns == other.mtime_ns
            && self.ctime_ns == other.ctime_ns
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
    Ok(())
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
    root_dev: u64,
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
                return Err(io::Error::new(io::ErrorKind::Other, "not a folder"));
            }
            Ok(Root {
                path: path.to_path_buf(),
                dev: 0,
                mounts: Vec::new(),
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
                root_dev: self.dev,
                fd: self.fd.try_clone()?,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Dir {
                rel: String::new(),
                path: self.path.clone(),
                root_dev: self.dev,
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
    ///
    /// # Errors
    ///
    /// As [`Root::dir`], and [`BeneathError::BadName`] for a key without a name.
    pub fn parent_of<'k>(
        &self,
        key: &'k str,
        create: bool,
    ) -> Result<(Dir, &'k str), BeneathError> {
        let (folder, name) = split_key(key);
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
            self.mounts.iter().any(|m| *m == path)
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
            if st.dev != self.root_dev {
                return Err(BeneathError::CrossesMount(rel));
            }
            Ok(Dir {
                rel,
                path: self.path.join(name),
                root_dev: self.root_dev,
                fd,
            })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound && create => {
                    std::fs::create_dir(&path).or_else(|e| {
                        if e.kind() == io::ErrorKind::AlreadyExists {
                            Ok(())
                        } else {
                            Err(e)
                        }
                    })?;
                    std::fs::symlink_metadata(&path)?
                }
                Err(e) => return Err(e.into()),
            };
            if is_reparse(&meta) || !meta.is_dir() {
                return Err(BeneathError::ThroughLink(rel));
            }
            Ok(Dir {
                rel,
                path,
                root_dev: self.root_dev,
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
            std::fs::symlink_metadata(&self.path).map(|m| stat_of_meta(&m))
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
            std::fs::symlink_metadata(self.path.join(name)).map(|m| stat_of_meta(&m))
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
            let meta = std::fs::symlink_metadata(&path)?;
            if is_reparse(&meta) || !meta.is_file() {
                return Err(BeneathError::ThroughLink(rel));
            }
            let file = File::open(&path)?;
            let st = stat_of_meta(&file.metadata()?);
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
            Ok(std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.path.join(name))?)
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
        #[cfg(not(unix))]
        {
            Ok(std::fs::rename(self.path.join(from), target.path.join(to))?)
        }
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
            Ok(std::fs::remove_file(self.path.join(name))?)
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
            Ok(std::fs::remove_dir(self.path.join(name))?)
        }
    }

    /// Makes the folder's entries durable (after a rename into it). Some file systems (FAT on
    /// macOS) refuse it for a folder: that is no failure of the write.
    pub fn sync(&self) {
        #[cfg(unix)]
        {
            let _ = sys::fsync(&self.fd);
        }
    }

    /// Writes `bytes` to `name` through a new temporary file in this folder: written, synced to
    /// disk, then renamed over `name`, and the folder synced. A reader never sees half a file,
    /// a crash leaves the old version or the new one, a failure leaves the old one and no
    /// temporary file.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn write_atomically(
        &self,
        name: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), BeneathError> {
        self.write_from(name, &mut &bytes[..], mode).map(|_| ())
    }

    /// [`Dir::write_atomically`] from a reader; the bytes written.
    ///
    /// # Errors
    ///
    /// When it cannot be read or written.
    pub fn write_from(
        &self,
        name: &str,
        body: &mut dyn io::Read,
        mode: u32,
    ) -> Result<u64, BeneathError> {
        check_name(name)?;
        let tmp = temp_name(name);
        let mut file = self.create_new(&tmp, mode)?;
        let written = io::copy(body, &mut file).and_then(|n| file.sync_all().map(|()| n));
        drop(file);
        let written = match written {
            Ok(n) => n,
            Err(e) => {
                let _ = self.unlink(&tmp);
                return Err(e.into());
            }
        };
        if let Err(e) = self.rename(&tmp, self, name) {
            let _ = self.unlink(&tmp);
            return Err(e);
        }
        self.sync();
        Ok(written)
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
        file.metadata().map(|m| stat_of_meta(&m))
    }
}

/// The bytes free for this user on the disk of `path`; `None` where it cannot be told.
#[must_use]
pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        sys::free_space(path).ok()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
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

#[cfg(not(unix))]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

#[cfg(not(unix))]
fn stat_of_meta(meta: &std::fs::Metadata) -> Stat {
    let kind = if is_reparse(meta) {
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

/// Linux: the mount points strictly below `root` (from `/proc/self/mountinfo`).
fn mounts_below(root: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let Ok(real) = std::fs::canonicalize(root) else {
            return Vec::new();
        };
        let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for line in text.lines() {
            let Some(point) = line.split(' ').nth(4) else {
                continue;
            };
            let point = PathBuf::from(unescape_mount(point));
            if point != real && point.starts_with(&real) {
                // Kept under the root's path as the caller spells it.
                if let Ok(rest) = point.strip_prefix(&real) {
                    out.push(root.join(rest));
                }
            }
        }
        out
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        Vec::new()
    }
}

/// A mountinfo field with its octal escapes (`\040` is a space) undone.
#[cfg(target_os = "linux")]
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

    pub fn fsync(fd: &OwnedFd) -> io::Result<()> {
        // SAFETY: an open descriptor.
        cvt(unsafe { libc::fsync(fd.as_raw_fd()) })?;
        Ok(())
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

    pub fn free_space(path: &Path) -> io::Result<u64> {
        let c = cstr(path.as_os_str())?;
        // SAFETY: zeroed is a valid `statvfs`; statvfs fills it.
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        cvt(unsafe { libc::statvfs(c.as_ptr(), &mut st) })?;
        Ok((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
    }
}
