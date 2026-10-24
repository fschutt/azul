//! Changes under a folder that never go through a link.
//!
//! A path joined from a key and used by name follows every symbolic link on its way. A sync
//! that writes `inbox/evil.plist` for another device would land wherever a local `inbox` link
//! points (`~/Library/LaunchAgents`, run at the next login). So here every folder below the
//! root is opened by its name in the folder above it, refusing a link, and the last step -
//! create, rename, remove - is made relative to the folder it lies in: a link swapped in
//! meanwhile is never followed either. On Unix that is `openat` with `O_NOFOLLOW`. Elsewhere
//! (Windows) every folder on the way is looked at without following it, before the change and
//! again right before its last step; a junction or any other reparse point counts as a link.
//! The root itself is the user's choice and may be a link.

use std::{
    fmt,
    io::{self, ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

/// What a name in a folder is, its last link not followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
    /// A symbolic link (on Windows also a junction or another reparse point).
    Link,
    /// A FIFO, a socket, a device.
    Other,
}

/// Why a change was refused: a link, or something that is no folder, lies on its way.
#[derive(Debug)]
struct Refused {
    /// The path of the link (or non-folder) under the root.
    path: PathBuf,
    link: bool,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.link {
            write!(
                f,
                "{} here is a symbolic link: nothing is written through it",
                self.path.display()
            )
        } else {
            write!(
                f,
                "{} here is not a folder: nothing is written into it",
                self.path.display()
            )
        }
    }
}

impl std::error::Error for Refused {}

/// Whether `e` is a change refused because a link lies on its way.
#[must_use]
pub fn through_link(e: &io::Error) -> bool {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<Refused>())
        .is_some_and(|refused| refused.link)
}

/// A name of one folder level: never empty, `.`, `..` or with a separator, so no name can
/// leave the folder it is opened in.
fn check_name(name: &str) -> io::Result<()> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0');
    if bad {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("\"{name}\" cannot be a name in a folder"),
        ));
    }
    Ok(())
}

/// A folder under a root, reached one name at a time without following a link.
pub struct Folder {
    dir: imp::Dir,
    /// Its path, for messages and for the platforms without `openat`.
    path: PathBuf,
}

impl Folder {
    /// The folder `names` lead to under `root`; with `make` the missing ones are made.
    ///
    /// # Errors
    ///
    /// `NotFound` when one is missing (and not made); a refusal ([`through_link`]) when one is
    /// a link or no folder; the I/O error that stopped it.
    pub fn open(root: &Path, names: &[&str], make: bool) -> io::Result<Folder> {
        let mut dir = imp::Dir::open_root(root)?;
        let mut path = root.to_path_buf();
        for name in names {
            check_name(name)?;
            path.push(name);
            let next = match dir.open_child(name, &path) {
                Ok(next) => next,
                Err(e) if e.kind() == ErrorKind::NotFound && make => {
                    dir.make_child(name)?;
                    dir.open_child(name, &path)
                        .map_err(|e| refusal(&dir, name, &path, e))?
                }
                Err(e) if e.kind() == ErrorKind::NotFound => return Err(e),
                Err(e) => return Err(refusal(&dir, name, &path, e)),
            };
            dir = next;
        }
        Ok(Folder { dir, path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What `name` is in this folder (a link is not followed); `None` when nothing has it.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the look.
    pub fn kind(&self, name: &str) -> io::Result<Option<Kind>> {
        check_name(name)?;
        self.dir.kind(name)
    }

    /// Writes `body` to `name` through a temporary file next to it, renamed over it: a reader
    /// never sees half a file, a failed read leaves the old one. A link at `name` is replaced,
    /// never written through. The bytes written.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped it (the temporary file is removed then).
    pub fn write(&self, name: &str, body: &mut dyn Read) -> io::Result<u64> {
        check_name(name)?;
        let tmp = crate::local::temp_name(name);
        let written = self.dir.create_new(&tmp).and_then(|mut file| {
            let written = io::copy(body, &mut file)?;
            file.flush()?;
            Ok(written)
        });
        let written = written.and_then(|written| {
            self.dir.rename(&tmp, &self.dir, name)?;
            Ok(written)
        });
        if written.is_err() {
            let _ = self.dir.remove_file(&tmp);
        }
        written
    }

    /// Renames `from` here to `to` in the folder `into` (this one or another under the same
    /// root): one rename, of a file or a folder with all it holds.
    ///
    /// # Errors
    ///
    /// The I/O error of the rename.
    pub fn rename(&self, from: &str, into: &Folder, to: &str) -> io::Result<()> {
        check_name(from)?;
        check_name(to)?;
        self.dir.rename(from, &into.dir, to)
    }

    /// Removes the file (or link) `name`; a folder is refused.
    ///
    /// # Errors
    ///
    /// The I/O error of the removal (`NotFound` when nothing has the name).
    pub fn remove_file(&self, name: &str) -> io::Result<()> {
        check_name(name)?;
        self.dir.remove_file(name)
    }

    /// Removes the folder `name` when it is empty.
    ///
    /// # Errors
    ///
    /// When it is not empty, no folder, or cannot be removed.
    pub fn remove_dir(&self, name: &str) -> io::Result<()> {
        check_name(name)?;
        self.dir.remove_dir(name)
    }
}

/// The error of a folder level that could not be opened: a refusal when it is a link or no
/// folder, else `e` itself.
fn refusal(dir: &imp::Dir, name: &str, path: &Path, e: io::Error) -> io::Error {
    match dir.kind(name) {
        Ok(Some(Kind::Folder)) | Ok(None) | Err(_) => e,
        Ok(Some(kind)) => io::Error::other(Refused {
            path: path.to_path_buf(),
            link: kind == Kind::Link,
        }),
    }
}

#[cfg(unix)]
mod imp {
    use std::{
        ffi::CString,
        fs::File,
        io,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        },
        path::Path,
    };

    use super::Kind;

    /// An open folder: every call names an entry relative to it.
    pub(super) struct Dir(OwnedFd);

    fn c_name(name: &str) -> io::Result<CString> {
        CString::new(name).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in a name"))
    }

    /// The fd of a call that returned one.
    fn owned(fd: libc::c_int) -> io::Result<OwnedFd> {
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a new descriptor the call just opened; nothing else owns it.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn done(result: libc::c_int) -> io::Result<()> {
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    impl Dir {
        pub(super) fn open_root(root: &Path) -> io::Result<Dir> {
            let path = CString::new(root.as_os_str().as_bytes())?;
            // SAFETY: a valid C string; the descriptor is owned at once.
            let fd = unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            owned(fd).map(Dir)
        }

        pub(super) fn open_child(&self, name: &str, _path: &Path) -> io::Result<Dir> {
            let name = c_name(name)?;
            // SAFETY: as in `open_root`; O_NOFOLLOW refuses a link as the entry itself.
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            owned(fd).map(Dir)
        }

        pub(super) fn make_child(&self, name: &str) -> io::Result<()> {
            let name = c_name(name)?;
            // SAFETY: a valid descriptor and C string.
            match done(unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o777) }) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                other => other,
            }
        }

        pub(super) fn kind(&self, name: &str) -> io::Result<Option<Kind>> {
            let name = c_name(name)?;
            // SAFETY: `stat` is plain data the call fills in.
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: a valid descriptor, C string and buffer.
            let result = unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            match done(result) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            }
            Ok(Some(match st.st_mode & libc::S_IFMT {
                libc::S_IFDIR => Kind::Folder,
                libc::S_IFREG => Kind::File,
                libc::S_IFLNK => Kind::Link,
                _ => Kind::Other,
            }))
        }

        pub(super) fn create_new(&self, name: &str) -> io::Result<File> {
            let name = c_name(name)?;
            // SAFETY: as in `open_root`; O_EXCL with O_NOFOLLOW never opens what is there.
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0o666 as libc::c_uint,
                )
            };
            owned(fd).map(File::from)
        }

        pub(super) fn rename(&self, from: &str, into: &Dir, to: &str) -> io::Result<()> {
            let (from, to) = (c_name(from)?, c_name(to)?);
            // SAFETY: valid descriptors and C strings.
            done(unsafe {
                libc::renameat(self.0.as_raw_fd(), from.as_ptr(), into.0.as_raw_fd(), to.as_ptr())
            })
        }

        pub(super) fn remove_file(&self, name: &str) -> io::Result<()> {
            let name = c_name(name)?;
            // SAFETY: a valid descriptor and C string; unlinkat removes the entry, never its target.
            done(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) })
        }

        pub(super) fn remove_dir(&self, name: &str) -> io::Result<()> {
            let name = c_name(name)?;
            // SAFETY: as in `remove_file`; a link is no folder, so AT_REMOVEDIR refuses it.
            done(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) })
        }
    }
}

#[cfg(not(unix))]
mod imp {
    use std::{
        fs::{self, File},
        io,
        path::{Path, PathBuf},
    };

    use super::Kind;

    /// A folder by its path, and the names that lead to it from the root: every change looks
    /// at the way again right before its last step.
    pub(super) struct Dir {
        root: PathBuf,
        names: Vec<String>,
        path: PathBuf,
    }

    fn kind_of(meta: &fs::Metadata) -> Kind {
        if is_link(meta) {
            Kind::Link
        } else if meta.is_dir() {
            Kind::Folder
        } else if meta.is_file() {
            Kind::File
        } else {
            Kind::Other
        }
    }

    /// A symbolic link, or on Windows any reparse point (a junction, a mount point).
    fn is_link(meta: &fs::Metadata) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return true;
            }
        }
        meta.file_type().is_symlink()
    }

    /// `path` is a folder and no link.
    fn real_folder(path: &Path) -> io::Result<()> {
        let meta = fs::symlink_metadata(path)?;
        if kind_of(&meta) == Kind::Folder {
            Ok(())
        } else {
            Err(io::Error::other("not a folder"))
        }
    }

    impl Dir {
        pub(super) fn open_root(root: &Path) -> io::Result<Dir> {
            if !fs::metadata(root)?.is_dir() {
                return Err(io::Error::other("not a folder"));
            }
            Ok(Dir {
                root: root.to_path_buf(),
                names: Vec::new(),
                path: root.to_path_buf(),
            })
        }

        pub(super) fn open_child(&self, name: &str, path: &Path) -> io::Result<Dir> {
            real_folder(path)?;
            let mut names = self.names.clone();
            names.push(name.to_string());
            Ok(Dir {
                root: self.root.clone(),
                names,
                path: path.to_path_buf(),
            })
        }

        /// Every folder from the root to this one is still a folder and no link.
        fn check_way(&self) -> io::Result<()> {
            let mut path = self.root.clone();
            for name in &self.names {
                path.push(name);
                real_folder(&path).map_err(|_| {
                    io::Error::other(super::Refused {
                        path: path.clone(),
                        link: true,
                    })
                })?;
            }
            Ok(())
        }

        pub(super) fn make_child(&self, name: &str) -> io::Result<()> {
            self.check_way()?;
            match fs::create_dir(self.path.join(name)) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                other => other,
            }
        }

        pub(super) fn kind(&self, name: &str) -> io::Result<Option<Kind>> {
            match fs::symlink_metadata(self.path.join(name)) {
                Ok(meta) => Ok(Some(kind_of(&meta))),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            }
        }

        pub(super) fn create_new(&self, name: &str) -> io::Result<File> {
            self.check_way()?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.path.join(name))
        }

        pub(super) fn rename(&self, from: &str, into: &Dir, to: &str) -> io::Result<()> {
            self.check_way()?;
            into.check_way()?;
            fs::rename(self.path.join(from), into.path.join(to))
        }

        pub(super) fn remove_file(&self, name: &str) -> io::Result<()> {
            self.check_way()?;
            let path = self.path.join(name);
            // A link to a folder is removed as a folder entry on Windows.
            match fs::symlink_metadata(&path) {
                Ok(meta) if is_link(&meta) && meta.is_dir() => fs::remove_dir(&path),
                Ok(meta) if meta.is_dir() => Err(io::Error::other("it is a folder")),
                _ => fs::remove_file(&path),
            }
        }

        pub(super) fn remove_dir(&self, name: &str) -> io::Result<()> {
            self.check_way()?;
            let path = self.path.join(name);
            if kind_of(&fs::symlink_metadata(&path)?) != Kind::Folder {
                return Err(io::Error::other("not a folder"));
            }
            fs::remove_dir(&path)
        }
    }
}
