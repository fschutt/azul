//! A folder on disk as a drive: the user's data tree (every app's durable
//! data, with its `.azlin/cache` manifest - see [`crate::manifest`]), and,
//! without the manifest, any other folder (AzDrive's Home and Downloads).

use std::{
    fs::{self, File},
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    beneath::{self, BeneathError, Dir, Fingerprint, Kind},
    ctl::Quiet,
    dir_read::{self, DirItem, ItemKind},
    durable::{self, Expect, Landed, Mode, Publish},
    key::{check_path_key, check_path_prefix, folder_of},
    manifest::{self, is_reserved_key, Changes, Manifest, ManifestEntry, Record, MANIFEST_DIR},
    names::{self, LocalKey},
    ops::check_folder,
    same_etag,
    sigv4::{sha256_hex, sha256_hex_of},
    xfer::Site,
    BatchMove, ByteRange, Drive, DriveError, Limit, ListPage, ListRequest, ObjectInfo,
    Precondition,
};

/// A folder on disk: the key `a/b.txt` is the file `<root>/a/b.txt`. Keys that
/// would leave the root (`..`, absolute paths) are refused before any file is
/// touched. Symbolic links inside the root are followed when listing one
/// folder, not when walking a whole prefix (no loops). A [`LocalDrive::contained`]
/// drive follows none: its reads and writes walk the key from the root without a link.
///
/// [`LocalDrive::new`] is the user's DATA TREE: every put / delete / rename /
/// copy / folder operation is recorded in `<root>/.azlin/cache` (see
/// [`crate::manifest`]), so the later S3 sync only diffs. A folder that is
/// not the data tree (Downloads, the user's home) is opened with
/// [`LocalDrive::without_manifest`]. Either way `.azlin/` at the root is the
/// drive's own bookkeeping: never listed, no key may name it.
#[derive(Debug, Clone)]
pub struct LocalDrive {
    root: PathBuf,
    /// Keeps `<root>/.azlin/cache`.
    manifest: bool,
    /// Reads and writes walk the key from the root without following a link ([`beneath`]).
    contained: bool,
}

/// One entry of a listing before paging.
enum Entry {
    Folder(String),
    Object(ObjectInfo),
    /// A file whose size and date the folder read did not carry: only the entries that make
    /// it onto the page are stat'ed for them.
    File(String),
}

impl Entry {
    fn key(&self) -> &str {
        match self {
            Entry::Folder(prefix) => prefix,
            Entry::Object(info) => &info.key,
            Entry::File(key) => key,
        }
    }
}

/// The entry of a file the folder read named.
fn file_entry(key: String, item: &DirItem) -> Entry {
    match (item.size, item.modified) {
        (Some(size), Some(modified)) => Entry::Object(ObjectInfo {
            key,
            size,
            modified: Some(modified),
            etag: None,
        }),
        _ => Entry::File(key),
    }
}

/// Writes `bytes` to `path` through a temporary file, so a reader never sees half
/// a file and a failure leaves the old one. The file is on the disk before it is renamed
/// in and the folder after: a power cut leaves the old version or the new one, never an
/// empty file under the real name. ([`crate::durable::replace`], which every writer of a
/// document goes through; this name stays for the callers that have it.)
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::durable::replace(path, bytes, crate::durable::Mode::Shared)
}

/// A walk of a contained drive that failed, as the drive says it.
pub(crate) fn beneath_error(key: &str, e: BeneathError) -> DriveError {
    match e {
        BeneathError::ThroughLink(_) | BeneathError::CrossesMount(_) => DriveError::InvalidKey {
            key: key.to_string(),
            reason: THROUGH_LINK,
        },
        BeneathError::BadName(_) => DriveError::InvalidKey {
            key: key.to_string(),
            reason: "it has a name that cannot be a file name",
        },
        BeneathError::Io(e) => not_found_or_io(key, e),
    }
}

/// Empties `dir`: files and links are unlinked (a link's target stays where it is), folders
/// are emptied the same way and removed. It never leaves `dir` through a link, nor into
/// another disk ([`beneath::Dir::child`] refuses both).
fn remove_all(dir: &beneath::Dir) -> Result<(), BeneathError> {
    for name in dir.names()? {
        let Some(name) = name.to_str() else {
            return Err(BeneathError::BadName(name.to_string_lossy().into_owned()));
        };
        if dir.lstat(name)?.kind == Kind::Dir {
            remove_all(&dir.child(name, false)?)?;
            dir.rmdir(name)?;
        } else {
            dir.unlink(name)?;
        }
    }
    Ok(())
}

/// Why a contained drive refuses a key whose folder here is a link or another disk.
pub const THROUGH_LINK: &str =
    "a folder of its path here is a link or another disk: nothing is written or read through it";

fn object_info(key: String, meta: &fs::Metadata) -> ObjectInfo {
    ObjectInfo {
        key,
        size: meta.len(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        etag: None,
    }
}

/// The entity tag of a file on disk: what its [`Fingerprint`] says of it - size, times, device
/// and inode - as text. A tag differs from the one before as soon as the file was written to,
/// replaced or touched, and a file that was replaced by another has another inode: a step that
/// must act on THIS version (a replace, a retire after a move) asks for the tag it saw
/// ([`Precondition::Matches`], [`crate::BatchDelete::of_version`]).
fn etag_of(print: &Fingerprint) -> String {
    let (dev, ino) = print.id.parts();
    format!(
        "L1-{:x}-{:x}-{:x}-{:x}-{:x}",
        print.size, print.mtime_ns, print.ctime_ns, dev, ino
    )
}

/// The fingerprint a tag of [`etag_of`] describes; `None` for any other tag.
fn print_of_etag(tag: &str) -> Option<Fingerprint> {
    let rest = tag.trim().trim_matches('"').strip_prefix("L1-")?;
    let mut parts = rest.split('-').map(|p| u64::from_str_radix(p, 16).ok());
    let size = parts.next()??;
    let mtime_ns = parts.next()??;
    let ctime_ns = parts.next()??;
    let dev = parts.next()??;
    let ino = parts.next()??;
    parts
        .next()
        .is_none()
        .then(|| Fingerprint::from_parts(Kind::File, size, mtime_ns, ctime_ns, dev, ino))
}

/// What a file on disk with this tag must still be for a write to replace it, when the system
/// gives files an identity ([`Expect::Is`]); `None` for a tag of another kind and for a system
/// that gives none (the caller then compares what it can, as [`expectation`] does).
pub(crate) fn expect_of_tag(tag: &str) -> Option<Expect> {
    print_of_etag(tag)
        .filter(|print| print.id.is_known())
        .map(Expect::Is)
}

/// What the name `name` of `dir` must hold for a write that asked `condition`. A tag whose file
/// the system gave no identity (a file system with no file index, a network share) can only be
/// compared by kind, size and times: that is checked here, now, and the write then goes on without
/// one - a short window the identity would close.
fn expectation(
    dir: &Dir,
    name: &str,
    condition: &Precondition,
    key: &str,
) -> Result<Expect, DriveError> {
    let changed = || DriveError::Conflict {
        key: key.to_string(),
    };
    match condition {
        Precondition::Absent => Ok(Expect::Absent),
        Precondition::Matches(tag) => {
            let print = print_of_etag(tag).ok_or_else(changed)?;
            if print.id.is_known() {
                return Ok(Expect::Is(print));
            }
            match dir.lstat(name) {
                Ok(st)
                    if st.kind == print.kind
                        && st.size == print.size
                        && st.mtime_ns == print.mtime_ns
                        && (st.ctime_ns == 0
                            || print.ctime_ns == 0
                            || st.ctime_ns == print.ctime_ns) =>
                {
                    Ok(Expect::Any)
                }
                _ => Err(changed()),
            }
        }
    }
}

/// Whether the rename failed because the two names are on different disks.
fn crosses_disks(e: &BeneathError) -> bool {
    match e {
        BeneathError::Io(io) => {
            io.kind() == ErrorKind::CrossesDevices
                || cfg!(unix) && io.raw_os_error() == exdev_code()
        }
        _ => false,
    }
}

#[cfg(unix)]
fn exdev_code() -> Option<i32> {
    Some(libc::EXDEV)
}

#[cfg(not(unix))]
fn exdev_code() -> Option<i32> {
    None
}

/// Whether removing a folder failed because something is in it.
fn is_not_empty(e: &BeneathError) -> bool {
    matches!(
        e,
        BeneathError::Io(io)
            if matches!(io.kind(), ErrorKind::DirectoryNotEmpty | ErrorKind::AlreadyExists)
    )
}

/// The files a file manager leaves in a folder it showed, which nobody counts as content when a
/// folder is emptied: removed with the folder that holds nothing else.
const JUNK: [&str; 4] = [".DS_Store", "Thumbs.db", "desktop.ini", ".localized"];

/// Whether `folder` holds nothing but empty folders (and the junk a file manager leaves): a read
/// that changes nothing. A name that is not text, a link, a socket, a file: something.
fn holds_only_empty_folders(folder: &Dir) -> Result<bool, BeneathError> {
    for entry in folder.names()? {
        let Some(entry) = entry.to_str() else {
            return Ok(false);
        };
        match folder.lstat(entry) {
            Ok(st) if st.kind == Kind::Dir => {
                let inner = match folder.child(entry, false) {
                    Ok(inner) => inner,
                    // Gone since the listing.
                    Err(e) if e.is_not_found() => continue,
                    Err(e) => return Err(e),
                };
                if !holds_only_empty_folders(&inner)? {
                    return Ok(false);
                }
            }
            Ok(st) if st.kind == Kind::File && JUNK.contains(&entry) => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

/// Removes the folder `name` of `parent` when nothing is in it but empty folders (and the junk a
/// file manager leaves): `true` when it is gone, `false` when something is in it - a file, a
/// link, anything a listing might not show - and then nothing of it is removed, not even an empty
/// folder inside. Never recursive on content: a file that appears while it is removed stops it.
fn remove_empty_tree(parent: &Dir, name: &str) -> Result<bool, BeneathError> {
    let folder = match parent.child(name, false) {
        Ok(folder) => folder,
        Err(e) if e.is_not_found() => return Ok(true),
        Err(e) => return Err(e),
    };
    if !holds_only_empty_folders(&folder)? {
        return Ok(false);
    }
    remove_folder_of_empties(parent, name, &folder)
}

/// The removal pass of [`remove_empty_tree`]: the look just made said it may go.
fn remove_folder_of_empties(parent: &Dir, name: &str, folder: &Dir) -> Result<bool, BeneathError> {
    for entry in folder.names()? {
        let Some(entry) = entry.to_str() else {
            return Ok(false);
        };
        match folder.lstat(entry) {
            Ok(st) if st.kind == Kind::Dir => {
                let inner = match folder.child(entry, false) {
                    Ok(inner) => inner,
                    Err(e) if e.is_not_found() => continue,
                    Err(e) => return Err(e),
                };
                if !remove_folder_of_empties(folder, entry, &inner)? {
                    return Ok(false);
                }
            }
            Ok(st) if st.kind == Kind::File && JUNK.contains(&entry) => {
                match folder.unlink(entry) {
                    Err(e) if !e.is_not_found() => return Err(e),
                    _ => {}
                }
            }
            // Made since the look: it stays, and so does its folder.
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    match parent.rmdir(name) {
        Ok(()) => {
            parent.sync();
            Ok(true)
        }
        Err(e) if e.is_not_found() => Ok(true),
        Err(e) if is_not_empty(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

fn reserved(key: &str) -> DriveError {
    DriveError::InvalidKey {
        key: key.to_string(),
        reason: "it is the drive's own bookkeeping (.azlin)",
    }
}

/// The key of a contained walk as a path argument: its shape was checked by `path_of` (or
/// `any_path`) of the call already, so this is the typed form of that check, not a second one.
fn walk_key(key: &str) -> Result<LocalKey, DriveError> {
    names::local_key(key).map_err(DriveError::from)
}

fn not_found_or_io(key: &str, e: std::io::Error) -> DriveError {
    if crate::ctl::Stopped::is(&e) {
        DriveError::Cancelled
    } else if e.kind() == ErrorKind::NotFound {
        DriveError::NotFound {
            key: key.to_string(),
        }
    } else {
        DriveError::Io(format!("{key}: {e}"))
    }
}

/// The permission bits and the modification time of an open file, for the copy of it
/// ([`Mode::FromSource`]; the system's own bits where it has none).
pub fn source_bits(file: &File) -> (Mode, Option<SystemTime>) {
    match beneath::stat_of(file) {
        Ok(st) => {
            let mtime = (st.mtime_ns > 0).then(|| UNIX_EPOCH + Duration::from_nanos(st.mtime_ns));
            (Mode::FromSource(st.mode), mtime)
        }
        Err(_) => (Mode::Shared, None),
    }
}

impl LocalDrive {
    /// The user's data tree at `root`: keeps the `.azlin/cache` manifest.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalDrive {
            root: root.into(),
            manifest: true,
            contained: false,
        }
    }

    /// A folder that is not the data tree (Downloads, the user's home, a
    /// folder the user added as a drive): no manifest, so browsing and
    /// writing there leave no `.azlin/` behind.
    #[must_use]
    pub fn without_manifest(root: impl Into<PathBuf>) -> Self {
        LocalDrive {
            root: root.into(),
            manifest: false,
            contained: false,
        }
    }

    /// The same folder, every read and write walked from the root without following a link
    /// ([`beneath`]): a key under a folder that is a link (or another disk mounted there) is
    /// refused ([`THROUGH_LINK`]), so nothing a key names is read or written outside the root.
    /// A synced folder's view of itself: its names come from other devices and other people.
    #[must_use]
    pub fn contained(mut self) -> Self {
        self.contained = true;
        self
    }

    /// Whether reads and writes follow no link ([`LocalDrive::contained`]).
    #[must_use]
    pub fn is_contained(&self) -> bool {
        self.contained
    }

    /// Records in the manifest a file the caller wrote under the root itself (a folder sync
    /// writes through [`beneath`]): the file of `key` as it is now.
    pub fn note_written(&self, key: &str) {
        self.record_file(key);
    }

    /// Records in the manifest that the caller removed (or moved away) the file of `key`.
    pub fn note_deleted(&self, key: &str) {
        self.record(&Record::Delete {
            key: key.to_string(),
        });
    }

    /// Records in the manifest that the caller renamed the file `from` to `to`.
    pub fn note_renamed(&self, from: &str, to: &str) {
        self.record(&Record::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    /// The folder of a (checked) key, walked from the root without a link, and the key's name.
    fn contained_parent(
        &self,
        key: &str,
        create: bool,
    ) -> Result<(beneath::Dir, String), DriveError> {
        self.path_of(key)?;
        let walked = walk_key(key)?;
        let root = beneath::Root::open(&self.root).map_err(|e| not_found_or_io(key, e))?;
        let (dir, name) = root
            .parent_of(&walked, create)
            .map_err(|e| beneath_error(key, e))?;
        Ok((dir, name.to_string()))
    }

    /// A contained drive's file of `key`, opened for reading.
    fn contained_open(&self, key: &str) -> Result<File, DriveError> {
        let (dir, name) = self.contained_parent(key, false)?;
        match dir.open_read(&name) {
            Ok((file, _)) => Ok(file),
            // A folder (or a link) of this name is no object of the drive.
            Err(BeneathError::ThroughLink(_)) => Err(DriveError::NotFound {
                key: key.to_string(),
            }),
            Err(e) => Err(beneath_error(key, e)),
        }
    }

    /// A contained drive's write of `key` from `body`: through a new file in the key's own
    /// folder, synced, renamed in, the folder synced ([`durable::publish`]).
    fn contained_write(
        &self,
        key: &str,
        body: &mut dyn Read,
        mode: Mode,
        mtime: Option<SystemTime>,
        condition: Option<&Precondition>,
    ) -> Result<Landed, DriveError> {
        let (dir, name) = self.contained_parent(key, true)?;
        self.write_into(key, &dir, &name, body, mode, mtime, condition)
    }

    /// `body` as the file `name` of `dir` (the folder of `key`): refused where a folder has the
    /// name, else put in place whole or not at all - and, when `condition` is asked, only if the
    /// name still holds what it says, in the one step that puts the file in place.
    fn write_into(
        &self,
        key: &str,
        dir: &Dir,
        name: &str,
        body: &mut dyn Read,
        mode: Mode,
        mtime: Option<SystemTime>,
        condition: Option<&Precondition>,
    ) -> Result<Landed, DriveError> {
        if dir.lstat(name).is_ok_and(|st| st.kind == Kind::Dir) {
            return Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "a folder has this name",
            });
        }
        let expect = match condition {
            Some(condition) => expectation(dir, name, condition, key)?,
            None => Expect::Any,
        };
        let mut publish = Publish::new(expect, mode, &Quiet);
        if let Some(mtime) = mtime {
            publish = publish.with_mtime(mtime);
        }
        durable::publish(dir, name, body, publish).map_err(|e| e.into_drive(key))
    }

    /// The folder of the path `path` (made when missing, links followed: it is the user's own
    /// folder) and the file's name in it, for a drive that is not contained.
    fn open_parent(&self, key: &str, path: &Path) -> Result<(Dir, String), DriveError> {
        let parent = path.parent().unwrap_or(&self.root);
        fs::create_dir_all(parent)?;
        let name =
            path.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: "it has a name that cannot be a file name",
                })?;
        let dir = Dir::open_following(parent).map_err(|e| not_found_or_io(key, e))?;
        Ok((dir, name.to_string()))
    }

    /// A contained drive's folder `prefix` (`a/b/`), walked from the root without a link and
    /// made where it is missing.
    fn contained_make_folder(&self, prefix: &str) -> Result<(), DriveError> {
        let walked = walk_key(prefix.trim_end_matches('/'))?;
        let root = beneath::Root::open(&self.root).map_err(|e| not_found_or_io(prefix, e))?;
        let (parent, name) = root
            .parent_of(&walked, true)
            .map_err(|e| beneath_error(prefix, e))?;
        if parent.lstat(name).is_ok_and(|st| st.kind == Kind::File) {
            return Err(DriveError::InvalidKey {
                key: prefix.to_string(),
                reason: "a file has this name",
            });
        }
        parent
            .child(name, true)
            .map(|_| ())
            .map_err(|e| beneath_error(prefix, e))
    }

    /// A contained drive's removal of the folder `prefix` with everything in it, walked from
    /// the root without a link. A link inside it goes as a link; what it points at stays.
    /// `false` when there is no such folder.
    fn contained_remove_folder(&self, prefix: &str) -> Result<bool, DriveError> {
        let walked = walk_key(prefix.trim_end_matches('/'))?;
        let root = beneath::Root::open(&self.root).map_err(|e| not_found_or_io(prefix, e))?;
        let (parent, name) = match root.parent_of(&walked, false) {
            Ok(found) => found,
            Err(e) if e.is_not_found() => return Ok(false),
            Err(e) => return Err(beneath_error(prefix, e)),
        };
        match parent.lstat(name) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(DriveError::Io(format!("{prefix}: {e}"))),
            Ok(st) if st.kind == Kind::File => {
                return Err(DriveError::InvalidKey {
                    key: prefix.to_string(),
                    reason: "it is a file",
                })
            }
            // The folder's own name is a link (or no folder at all): nothing behind it goes.
            Ok(st) if st.kind != Kind::Dir => {
                return Err(DriveError::InvalidKey {
                    key: prefix.to_string(),
                    reason: THROUGH_LINK,
                })
            }
            Ok(_) => {}
        }
        let folder = parent
            .child(name, false)
            .map_err(|e| beneath_error(prefix, e))?;
        remove_all(&folder).map_err(|e| beneath_error(prefix, e))?;
        parent.rmdir(name).map_err(|e| beneath_error(prefix, e))?;
        parent.sync();
        Ok(true)
    }

    /// [`Drive::put_from`], or - with a `condition` - [`Drive::put_from_if`]: the receipt of the
    /// file put in place.
    fn put_from_guarded(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: Option<&Precondition>,
    ) -> Result<Landed, DriveError> {
        let path = self.path_of(key)?;
        if self.contained {
            let landed = self.contained_write(key, body, Mode::Shared, None, condition)?;
            self.record_file(key);
            return Ok(landed);
        }
        if path.is_dir() {
            return Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "a folder has this name",
            });
        }
        let (dir, name) = self.open_parent(key, &path)?;
        let landed = self.write_into(key, &dir, &name, body, Mode::Shared, None, condition)?;
        self.record_file(key);
        Ok(landed)
    }

    /// The folder that holds the file or folder `key` (a folder key ends with `/`) and its name
    /// there, for a rename or a move: with `create`, made where missing; a drive that is not
    /// contained follows the links of the user's own folders, a contained one walks without any.
    fn parent_dir(&self, key: &str, create: bool) -> Result<(Dir, String), DriveError> {
        if self.contained {
            let walked = walk_key(key.trim_end_matches('/'))?;
            let root = beneath::Root::open(&self.root).map_err(|e| not_found_or_io(key, e))?;
            let (dir, name) = root
                .parent_of(&walked, create)
                .map_err(|e| beneath_error(key, e))?;
            return Ok((dir, name.to_string()));
        }
        let path = self.any_path(key)?;
        let parent = path.parent().unwrap_or(&self.root);
        if create {
            fs::create_dir_all(parent)?;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| DriveError::InvalidKey {
                key: key.to_string(),
                reason: "it has a name that cannot be a file name",
            })?
            .to_string();
        let dir = Dir::open_following(parent).map_err(|e| not_found_or_io(key, e))?;
        Ok((dir, name))
    }

    /// Moves the file or folder `from` to `to`: one rename on the disk that puts it in place only
    /// if nothing has the name - in one step, so a name made meanwhile is never replaced. The one
    /// exception is a name spelt in another case on a disk that reads both as one file: that is
    /// the same file, and its rename is the user's way of changing the case. Another disk
    /// (a mount inside the root) cannot rename: the file is copied in, checked, and removed only
    /// while it is still the file that was copied; a folder is moved file by file.
    /// `version`: the move happens only while `from` is the file with that tag.
    fn move_entry(&self, from: &str, to: &str, version: Option<&str>) -> Result<(), DriveError> {
        let (source, from_name) = match self.parent_dir(from, false) {
            Ok(found) => found,
            Err(DriveError::NotFound { .. }) => {
                return Err(DriveError::NotFound {
                    key: from.to_string(),
                })
            }
            Err(e) => return Err(e),
        };
        let seen = match source.lstat(&from_name) {
            Ok(st) => st,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(DriveError::NotFound {
                    key: from.to_string(),
                })
            }
            Err(e) => return Err(DriveError::Io(format!("{from}: {e}"))),
        };
        if let Some(version) = version {
            let tag = etag_of(&seen.fingerprint());
            if !same_etag(Some(&tag), version) {
                return Err(DriveError::Conflict {
                    key: from.to_string(),
                });
            }
        }
        let (target, to_name) = self.parent_dir(to, true)?;
        let taken = || DriveError::InvalidKey {
            key: to.to_string(),
            reason: "something has this name already",
        };
        let same_entry = match target.lstat(&to_name) {
            Ok(there) => {
                let fold = |name: &str| {
                    names::fold(
                        name,
                        &names::Volume {
                            case_insensitive: true,
                            norm_insensitive: true,
                            ..names::Volume::default()
                        },
                    )
                };
                // Two names of one file that differ only as a disk reads them: a case change.
                if seen.id().same(&there.id()).is_same()
                    && fold(&from_name) == fold(&to_name)
                    && from_name != to_name
                {
                    true
                } else {
                    return Err(taken());
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => false,
            Err(e) => return Err(DriveError::Io(format!("{to}: {e}"))),
        };
        let moved = if same_entry {
            source.rename(&from_name, &target, &to_name)
        } else {
            source.rename_noreplace(&from_name, &target, &to_name)
        };
        match moved {
            Ok(()) => {}
            Err(e) if e.is_already_exists() => return Err(taken()),
            Err(e) if crosses_disks(&e) => return self.move_across(from, to, version),
            Err(e) => return Err(beneath_error(from, e)),
        }
        target.sync();
        source.sync();
        // A rename keeps the modification time: the entries move as they are.
        self.record(&Record::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
        Ok(())
    }

    /// [`LocalDrive::move_entry`] between two disks.
    fn move_across(&self, from: &str, to: &str, version: Option<&str>) -> Result<(), DriveError> {
        if from.ends_with('/') {
            // Every file copied and then removed on its own, the folders that are empty after.
            crate::ops::rename_by_copy(self, from, to)?;
            self.remove_empty_folder(from)?;
            return Ok(());
        }
        let (source, from_name) = self.parent_dir(from, false)?;
        let (mut file, opened) = source
            .open_read(&from_name)
            .map_err(|e| beneath_error(from, e))?;
        if let Some(version) = version {
            if !same_etag(Some(&etag_of(&opened.fingerprint())), version) {
                return Err(DriveError::Conflict {
                    key: from.to_string(),
                });
            }
        }
        let (target, to_name) = self.parent_dir(to, true)?;
        let (mode, mtime) = source_bits(&file);
        let mut publish = Publish::new(Expect::Absent, mode, &Quiet);
        if let Some(mtime) = mtime {
            publish = publish.with_mtime(mtime);
        }
        durable::publish(&target, &to_name, &mut file, publish).map_err(|e| e.into_drive(to))?;
        // The copy is in place: the source goes only while it is the file that was copied.
        match source.lstat(&from_name) {
            Ok(now) if opened.fingerprint().matches(&now).is_same() => {
                source
                    .unlink(&from_name)
                    .map_err(|e| beneath_error(from, e))?;
                source.sync();
            }
            Ok(_) => {
                return Err(DriveError::Conflict {
                    key: from.to_string(),
                })
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(DriveError::Io(format!("{from}: {e}"))),
        }
        self.record(&Record::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
        Ok(())
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether this drive keeps the manifest (it is the data tree).
    #[must_use]
    pub fn keeps_manifest(&self) -> bool {
        self.manifest
    }

    /// The manifest's file: `<root>/.azlin/cache`.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root
            .join(MANIFEST_DIR)
            .join(crate::manifest::CACHE_FILE)
    }

    /// What the manifest records (empty when there is none yet, or for a
    /// drive without one).
    pub fn manifest(&self) -> Result<Manifest, DriveError> {
        if !self.manifest {
            return Ok(Manifest::default());
        }
        Ok(manifest::load(&self.manifest_path())?.0)
    }

    /// Brings the manifest up to date with the folder: what changed outside
    /// this drive's own calls (another program, files from before the
    /// manifest existed) is hashed and recorded, and returned. The sync's
    /// first step; blocking, call it from a worker thread.
    pub fn refresh_manifest(&self) -> Result<Changes, DriveError> {
        if !self.manifest {
            return Err(DriveError::Unsupported(String::from(
                "this folder keeps no manifest",
            )));
        }
        let mut found = Changes::default();
        manifest::rewrite(&self.manifest_path(), |m| {
            let (changes, touched) = manifest::scan(m, self)?;
            for key in changes.added.iter().chain(&changes.modified) {
                match self.entry_of(key) {
                    Some(entry) => m.insert(key, entry),
                    None => m.remove(key), // gone since the listing
                }
            }
            for key in &changes.deleted {
                m.remove(key);
            }
            for t in touched {
                m.insert(&t.key, t.entry);
            }
            found = changes;
            Ok(())
        })?;
        Ok(found)
    }

    /// The manifest entry of the file of `key` as it is now (its bytes read
    /// in pieces); `None` when there is no such file.
    fn entry_of(&self, key: &str) -> Option<ManifestEntry> {
        let path = self.path_of(key).ok()?;
        let meta = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
        let hash = File::open(&path).and_then(sha256_hex_of).ok()?;
        let info = object_info(key.to_string(), &meta);
        Some(ManifestEntry {
            size: info.size,
            modified: info.modified,
            hash,
        })
    }

    /// Records `record` in the manifest, when this drive keeps one. Never
    /// fails the drive call that made the change: the data is written, and
    /// the manifest is a cache that `diff` / `refresh_manifest` repair.
    fn record(&self, record: &Record) {
        if self.manifest {
            let _ = manifest::append(&self.manifest_path(), record);
        }
    }

    /// Records the file of `key` as it is now.
    fn record_file(&self, key: &str) {
        if !self.manifest {
            return;
        }
        if let Some(entry) = self.entry_of(key) {
            self.record(&Record::Put {
                key: key.to_string(),
                entry,
            });
        }
    }

    /// The file of a (checked) key.
    fn path_of(&self, key: &str) -> Result<PathBuf, DriveError> {
        // On Windows a name like `C:x` would make the pushed path leave the root: the check
        // refuses every name this system misreads.
        check_path_key(key)?;
        if is_reserved_key(key) {
            return Err(reserved(key));
        }
        let mut path = self.root.clone();
        for segment in key.split('/') {
            path.push(segment);
        }
        Ok(path)
    }

    /// The directory of a (checked) folder prefix: `a/b/` -> `<root>/a/b`.
    fn dir_of(&self, folder: &str) -> PathBuf {
        let mut path = self.root.clone();
        for segment in folder.split('/').filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }

    /// The directory of a folder name (`docs/sub/`), checked: not the root,
    /// ending in `/`, inside the root.
    fn folder_path(&self, prefix: &str) -> Result<PathBuf, DriveError> {
        check_folder(prefix)?;
        check_path_prefix(prefix)?;
        if is_reserved_key(prefix) {
            return Err(reserved(prefix));
        }
        Ok(self.dir_of(prefix))
    }

    /// The file or directory of a key or folder name, checked.
    fn any_path(&self, key: &str) -> Result<PathBuf, DriveError> {
        if key.ends_with('/') {
            self.folder_path(key)
        } else {
            self.path_of(key)
        }
    }

    /// The entries of `folder`'s directory a listing shows (no temporary files, not the
    /// bookkeeping at the root), read in batches (`dir_read`: the names and kinds, and on macOS
    /// the sizes and dates, without a stat per entry; a symbolic link is followed).
    fn read_items(&self, folder: &str) -> Result<Vec<DirItem>, DriveError> {
        let dir = self.dir_of(folder);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut items = dir_read::read_dir_all(&dir)?;
        items.retain(|item| {
            !(beneath::is_temp_name(&item.name)
                || folder.is_empty() && crate::names::Protected::is_bookkeeping(&item.name))
        });
        Ok(items)
    }

    /// The folders and files directly in `folder` whose keys start with `prefix`.
    fn read_level(
        &self,
        folder: &str,
        prefix: &str,
        out: &mut Vec<Entry>,
    ) -> Result<(), DriveError> {
        for item in self.read_items(folder)? {
            let key = format!("{folder}{}", item.name);
            if !key.starts_with(prefix) {
                continue;
            }
            match item.kind {
                ItemKind::Folder => out.push(Entry::Folder(format!("{key}/"))),
                ItemKind::File => out.push(file_entry(key, &item)),
            }
        }
        Ok(())
    }

    /// Every file under `folder`, at any depth (symbolic links to folders are not
    /// followed: no loops), as [`Entry::Object`] or [`Entry::File`].
    fn walk(&self, folder: &str, out: &mut Vec<Entry>) -> Result<(), DriveError> {
        for item in self.read_items(folder)? {
            let key = format!("{folder}{}", item.name);
            match item.kind {
                ItemKind::Folder if item.link => {}
                ItemKind::Folder => self.walk(&format!("{key}/"), out)?,
                ItemKind::File => out.push(file_entry(key, &item)),
            }
        }
        Ok(())
    }

    /// A page's file whose size and date the folder read did not carry: one stat. `None` when
    /// it is gone since, or no file any more.
    fn stat_file(&self, key: &str) -> Option<ObjectInfo> {
        // The key was made of the directory's own names: no check, as when it was listed.
        match dir_read::stat_item(&self.dir_of(key))? {
            (ItemKind::File, size, modified) => Some(ObjectInfo {
                key: key.to_string(),
                size: size.unwrap_or(0),
                modified,
                etag: None,
            }),
            (ItemKind::Folder, ..) => None,
        }
    }
}

impl Drive for LocalDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        check_path_prefix(&request.prefix)?;
        let prefix = request.prefix.as_str();
        let folder = folder_of(prefix);
        if is_reserved_key(folder) {
            return Ok(ListPage::default()); // the bookkeeping is not content
        }
        let mut entries = Vec::new();
        match request.delimiter.as_deref() {
            Some("/") => self.read_level(folder, prefix, &mut entries)?,
            delimiter => {
                let mut files = Vec::new();
                self.walk(folder, &mut files)?;
                for file in files.into_iter().filter(|o| o.key().starts_with(prefix)) {
                    let rest = &file.key()[prefix.len()..];
                    match delimiter.filter(|d| !d.is_empty()).and_then(|d| {
                        rest.find(d)
                            .map(|i| format!("{prefix}{}", &rest[..i + d.len()]))
                    }) {
                        Some(common) => entries.push(Entry::Folder(common)),
                        None => entries.push(file),
                    }
                }
            }
        }
        // S3 order: by key, byte-wise; a folder once.
        entries.sort_by(|a, b| a.key().cmp(b.key()));
        entries.dedup_by(|a, b| a.key() == b.key());
        if let Some(after) = request.continuation.as_deref() {
            entries.retain(|e| e.key() > after);
        }
        let size = request.page_size() as usize;
        let next = if entries.len() > size {
            Some(entries[size - 1].key().to_string())
        } else {
            None
        };
        entries.truncate(size);
        let mut page = ListPage {
            next,
            ..ListPage::default()
        };
        for entry in entries {
            match entry {
                Entry::Folder(prefix) => page.folders.push(prefix),
                Entry::Object(info) => page.objects.push(info),
                // Stat'ed now that it is on the page: a listing in pages of a large folder
                // stats each file once, not the whole folder for every page.
                Entry::File(key) => page.objects.extend(self.stat_file(&key)),
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        if self.contained {
            let mut bytes = Vec::new();
            self.contained_open(key)?
                .read_to_end(&mut bytes)
                .map_err(|e| not_found_or_io(key, e))?;
            return Ok(bytes);
        }
        let path = self.path_of(key)?;
        if path.is_dir() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        fs::read(&path).map_err(|e| not_found_or_io(key, e))
    }

    /// The size is looked at before a byte is read, and the read is cut one byte past the limit:
    /// a file that grows meanwhile is refused too.
    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        let mut file = if self.contained {
            self.contained_open(key)?
        } else {
            File::open(self.path_of(key)?).map_err(|e| not_found_or_io(key, e))?
        };
        let meta = file.metadata().map_err(|e| not_found_or_io(key, e))?;
        if meta.is_dir() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        limit.check_object(key, meta.len())?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(limit.max.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| not_found_or_io(key, e))?;
        limit.check_object(key, bytes.len() as u64)?;
        Ok(bytes)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let path = self.path_of(key)?;
        let mut file = if self.contained {
            self.contained_open(key)?
        } else {
            File::open(&path).map_err(|e| not_found_or_io(key, e))?
        };
        let meta = file.metadata()?;
        if meta.is_dir() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        let len = meta.len();
        let out_of_range = || DriveError::InvalidRange {
            key: key.to_string(),
        };
        if range.start >= len {
            return Err(out_of_range());
        }
        let end = range.end.map_or(len - 1, |end| end.min(len - 1));
        if end < range.start {
            return Err(out_of_range());
        }
        file.seek(SeekFrom::Start(range.start))?;
        let mut bytes = vec![0u8; (end - range.start + 1) as usize];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let path = self.path_of(key)?;
        if self.contained {
            self.contained_write(key, &mut &bytes[..], Mode::Shared, None, None)?;
            if self.manifest {
                if let Ok(meta) = fs::symlink_metadata(&path) {
                    let info = object_info(key.to_string(), &meta);
                    self.record(&Record::Put {
                        key: key.to_string(),
                        entry: ManifestEntry {
                            size: info.size,
                            modified: info.modified,
                            hash: sha256_hex(bytes),
                        },
                    });
                }
            }
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if path.is_dir() {
            return Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "a folder has this name",
            });
        }
        write_atomically(&path, bytes)?;
        if self.manifest {
            if let Ok(meta) = fs::metadata(&path) {
                let info = object_info(key.to_string(), &meta);
                self.record(&Record::Put {
                    key: key.to_string(),
                    entry: ManifestEntry {
                        size: info.size,
                        modified: info.modified,
                        hash: sha256_hex(bytes),
                    },
                });
            }
        }
        Ok(())
    }

    /// The bytes go into a temporary file next to the target as they are read, then the
    /// file is synced and renamed over it ([`durable::publish`]): a reader never sees half a
    /// file, a failed read leaves the old one.
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.put_from_guarded(key, body, None)
            .map(|landed| landed.len)
    }

    /// Writes the object only when `condition` holds, in the one step that puts it in place: a
    /// file made meanwhile is not replaced, one written to since the tag was read is not either
    /// ([`Precondition`]; the tag is [`Drive::head`]'s).
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.put_from_if(key, &mut &bytes[..], condition)
    }

    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.put_from_guarded(key, body, Some(condition))
            .map(|landed| Some(etag_of(&landed.print)))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let path = self.path_of(key)?;
        if self.contained {
            let (dir, name) = match self.contained_parent(key, false) {
                Ok(found) => found,
                // No folder of it: nothing to delete.
                Err(DriveError::NotFound { .. }) => return Ok(()),
                Err(e) => return Err(e),
            };
            match dir.lstat(&name) {
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(DriveError::Io(format!("{key}: {e}"))),
                Ok(st) if st.kind == Kind::Dir => {
                    return Err(DriveError::InvalidKey {
                        key: key.to_string(),
                        reason: "it is a folder",
                    })
                }
                Ok(_) => match dir.unlink(&name) {
                    Err(e) if !e.is_not_found() => return Err(beneath_error(key, e)),
                    _ => dir.sync(),
                },
            }
            self.record(&Record::Delete {
                key: key.to_string(),
            });
            return Ok(());
        }
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(DriveError::Io(format!("{key}: {e}"))),
            Ok(meta) if meta.is_dir() => {
                return Err(DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: "it is a folder",
                })
            }
            Ok(_) => match fs::remove_file(&path) {
                Err(e) if e.kind() != ErrorKind::NotFound => {
                    return Err(DriveError::Io(format!("{key}: {e}")))
                }
                _ => {}
            },
        }
        self.record(&Record::Delete {
            key: key.to_string(),
        });
        Ok(())
    }

    /// The file's size and date and its tag ([`etag_of`]): what a step that acts on THIS version
    /// - a replace, a delete after a move - asks for later.
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let path = self.path_of(key)?;
        let stat = if self.contained {
            let file = self.contained_open(key)?;
            beneath::stat_of(&file).map_err(|e| not_found_or_io(key, e))?
        } else {
            beneath::stat_path(&path).map_err(|e| not_found_or_io(key, e))?
        };
        if stat.kind != Kind::File {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        Ok(ObjectInfo {
            key: key.to_string(),
            size: stat.size,
            modified: (stat.mtime_ns > 0).then_some(stat.mtime_ns / 1_000_000_000),
            etag: Some(etag_of(&stat.fingerprint())),
        })
    }

    /// A file copy on disk, through a temporary file next to the target; the copy has the
    /// file's permission bits (never setuid, setgid or sticky) and its modification time, as a
    /// copy in a file manager has.
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        let source = self.path_of(from)?;
        let target = self.path_of(to)?;
        if self.contained {
            let mut file = self.contained_open(from)?;
            let (mode, mtime) = source_bits(&file);
            self.contained_write(to, &mut file, mode, mtime, None)?;
            self.record_file(to);
            return Ok(());
        }
        match fs::metadata(&source) {
            Err(e) => return Err(not_found_or_io(from, e)),
            Ok(meta) if !meta.is_file() => {
                return Err(DriveError::InvalidKey {
                    key: from.to_string(),
                    reason: "it is a folder",
                })
            }
            Ok(_) => {}
        }
        if target.is_dir() {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder has this name",
            });
        }
        let mut file = File::open(&source).map_err(|e| not_found_or_io(from, e))?;
        let (mode, mtime) = source_bits(&file);
        let (dir, name) = self.open_parent(to, &target)?;
        self.write_into(to, &dir, &name, &mut file, mode, mtime, None)?;
        self.record_file(to);
        Ok(())
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        let dir = self.folder_path(prefix)?;
        if self.contained {
            return self.contained_make_folder(prefix);
        }
        if dir.is_file() {
            return Err(DriveError::InvalidKey {
                key: prefix.to_string(),
                reason: "a file has this name",
            });
        }
        fs::create_dir_all(&dir).map_err(|e| DriveError::Io(format!("{prefix}: {e}")))
    }

    /// One `rename` on disk: a file, or a directory with everything in it - never over something
    /// that is there ([`LocalDrive::move_entry`]).
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if from.ends_with('/') != to.ends_with('/') {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a file and a folder cannot trade places",
            });
        }
        if from.ends_with('/') && to.starts_with(from) {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder cannot move into itself",
            });
        }
        self.any_path(from)?;
        self.any_path(to)?;
        self.move_entry(from, to, None)
    }

    /// Each file renamed on its own disk, never over something that is there; a file that is not
    /// the version an item names stays ([`DriveError::Conflict`]). A name on another disk is copied
    /// and removed only while it is still the file that was copied.
    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        items
            .iter()
            .map(|item| {
                if item.from.ends_with('/') || item.to.ends_with('/') {
                    if item.from.ends_with('/') != item.to.ends_with('/') {
                        return Err(DriveError::InvalidKey {
                            key: item.to.to_string(),
                            reason: "a file and a folder cannot trade places",
                        });
                    }
                    return self.rename(item.from, item.to);
                }
                self.path_of(item.from)?;
                self.path_of(item.to)?;
                self.move_entry(item.from, item.to, item.version)
            })
            .collect()
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        let dir = self.folder_path(prefix)?;
        if self.contained {
            if self.contained_remove_folder(prefix)? {
                self.record(&Record::Delete {
                    key: prefix.to_string(),
                });
            }
            return Ok(());
        }
        match fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DriveError::Io(format!("{prefix}: {e}"))),
            Ok(meta) if !meta.is_dir() => Err(DriveError::InvalidKey {
                key: prefix.to_string(),
                reason: "it is a file",
            }),
            Ok(_) => {
                fs::remove_dir_all(&dir).map_err(|e| DriveError::Io(format!("{prefix}: {e}")))?;
                self.record(&Record::Delete {
                    key: prefix.to_string(),
                });
                Ok(())
            }
        }
    }

    /// The folder removed if nothing is in it but empty folders (and the junk a file manager
    /// leaves): never a file, never recursive on content ([`remove_empty_tree`]).
    fn remove_empty_folder(&self, prefix: &str) -> Result<bool, DriveError> {
        self.folder_path(prefix)?;
        let (parent, name) = match self.parent_dir(prefix, false) {
            Ok(found) => found,
            // No folder of it: nothing in it either.
            Err(DriveError::NotFound { .. }) => return Ok(true),
            Err(e) => return Err(e),
        };
        match parent.lstat(&name) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(DriveError::Io(format!("{prefix}: {e}"))),
            Ok(st) if st.kind == Kind::File => {
                return Err(DriveError::InvalidKey {
                    key: prefix.to_string(),
                    reason: "it is a file",
                })
            }
            // A link where the folder was: nothing behind it is touched.
            Ok(st) if st.kind != Kind::Dir => return Ok(false),
            Ok(_) => {}
        }
        let removed = remove_empty_tree(&parent, &name).map_err(|e| beneath_error(prefix, e))?;
        if removed {
            self.record(&Record::Delete {
                key: prefix.to_string(),
            });
        }
        Ok(removed)
    }

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.any_path(key).ok()
    }

    fn site(&self) -> Site {
        Site::disk(self.root.clone())
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let path = self.any_path(key)?;
        let meta = fs::metadata(&path).map_err(|e| not_found_or_io(key, e))?;
        let mut pairs = vec![(
            String::from("Location"),
            path.to_string_lossy().into_owned(),
        )];
        if let Some(created) = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        {
            pairs.push((String::from("Created"), created.as_secs().to_string()));
        }
        pairs.push((
            String::from("Read-only"),
            String::from(if meta.permissions().readonly() {
                "Yes"
            } else {
                "No"
            }),
        ));
        Ok(pairs)
    }
}
