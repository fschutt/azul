//! The one way a file of the user's, of this computer's state or of a cache is put in place
//! (ARCH28 RF41 / AR5a-1).
//!
//! A new version is written into a file of its own in the SAME folder (never the system's
//! temporary folder: a rename across disks is a copy), given its permission bits and its time,
//! synced to the disk, checked against what the name must still hold, renamed over the name, and
//! the folder synced:
//!
//! ```text
//! [Stage, Write.., SyncFile, Check, Rename, SyncDir]      (Durability::State)
//! [Stage, Write.., Check, Rename]                          (Durability::Cache)
//! ```
//!
//! so that a reader never sees half a file, a crash leaves the old version or the new one and
//! never an empty file under the real name, a failure (a full disk, a cancel, a file that changed
//! meanwhile) leaves the old version and no temporary file, and the file that lands is the one
//! the caller asked for ([`Expect`]). Overwriting is a value, not a default: [`Expect::Any`] is
//! written out at the call site, and "replace" never means "delete first".
//!
//! - [`publish`]: a new version from a reader; [`publish_staged`]: from a complete file of
//!   another folder of the same disk (a download spooled beside the folder's bookkeeping).
//! - [`stage`] + [`commit`]: the same in two steps, for a caller that has work to do between.
//! - [`replace`] / [`replace_cache`]: a document from bytes, by path (settings, state, caches).
//! - [`replace_keeping`]: the new version in place and the old one kept in a slot of the
//!   caller's, the new version staged whole BEFORE the old one leaves its place.
//! - [`Landed`]: the receipt of a publish - what a step that retires a source may require.
//!
//! The raw calls (`fs::rename`, `fs::copy`, `File::create`, `fs::write`) that put a file in place
//! live here and in [`crate::beneath`] and nowhere else (`azul-srcgate`'s `durable_gate`).

use std::{
    fmt,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    beneath::{self, BeneathError, Dir, Fingerprint, Kind, Stat, Tri},
    ctl::{Ctl, Quiet, Stopped},
    DriveError,
};

/// Bytes moved per read and per write.
const CHUNK: usize = 256 * 1024;

/// The permission bits the new version gets (Unix; elsewhere the system's own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `rw-------`, and a folder made on the way `rwx------`: secrets, keys, a device's state. On
    /// Windows a protected access list for this user, the system and the administrators, set
    /// before the first byte is written ([`create_private_dirs`] for folders).
    Private,
    /// What a new file gets by default: `rw-rw-rw-` less the user's umask (`rw-r--r--`).
    Shared,
    /// The bits of the file that is replaced; [`Mode::Shared`] when there is none.
    Keep,
    /// Exactly these bits - the file this one is a copy of has them, or a fixed `0o644` - never
    /// setuid, setgid or sticky.
    FromSource(u32),
    /// A sync's rule: the replaced file's read and write bits (the user's own choice), else
    /// `rw-r--r--`; and the executable bit wherever a read bit is when the drive's version is
    /// executable. Never setuid, setgid or sticky. A remote entry brings no permission of its own.
    Synced {
        /// The drive's version is executable (and the disk keeps executable bits).
        exec: bool,
    },
}

/// How much of the order a publish takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// The file is synced before it is renamed in and the folder after: a power cut leaves the
    /// old version or the new one.
    State,
    /// The rename only: for a file that checks itself when it is read (a listing cache, a node
    /// list), where a power cut may cost the file but never makes a wrong one count.
    Cache,
}

/// What the name must hold when the new version is renamed over it.
#[derive(Clone, Copy, Debug)]
pub enum Expect {
    /// Nothing: the name is free. Taken in one step with the rename, so a file made
    /// meanwhile is never replaced.
    Absent,
    /// The file the fingerprint describes, untouched ([`Fingerprint::matches`] is `Same`: a
    /// file whose identity cannot be told is not "the same" - the caller refuses or hashes).
    Is(Fingerprint),
    /// Whatever is there. Written out where it is meant.
    Any,
}

/// How a new version is put in place.
pub struct Publish<'a> {
    pub expect: Expect,
    pub mode: Mode,
    /// The modification time to give the new file; `None`: now.
    pub mtime: Option<SystemTime>,
    pub durability: Durability,
    /// Told the bytes written and asked, per chunk, whether to stop.
    pub ctl: &'a dyn Ctl,
}

impl<'a> Publish<'a> {
    /// A publish of [`Durability::State`] with the time of now.
    #[must_use]
    pub fn new(expect: Expect, mode: Mode, ctl: &'a dyn Ctl) -> Publish<'a> {
        Publish {
            expect,
            mode,
            mtime: None,
            durability: Durability::State,
            ctl,
        }
    }

    /// With this modification time.
    #[must_use]
    pub fn with_mtime(mut self, mtime: SystemTime) -> Publish<'a> {
        self.mtime = Some(mtime);
        self
    }

    /// With [`Durability::Cache`].
    #[must_use]
    pub fn cached(mut self) -> Publish<'a> {
        self.durability = Durability::Cache;
        self
    }
}

/// The receipt of a publish: the new version is in place. The one thing a step that retires a
/// source (a move deletes it after the copy landed) may be required to hold. It has one
/// constructor, here; it cannot be copied, cloned, defaulted or read from a file.
#[derive(Debug)]
pub struct Landed {
    /// Bytes written.
    pub len: u64,
    /// The BLAKE3 of the bytes written; `None` for a staged file that was not read again.
    pub blake3: Option<[u8; 32]>,
    /// The file in place, as an `lstat` after the rename shows it.
    pub print: Fingerprint,
    _sealed: (),
}

/// Why a new version was not put in place. In every case the old version is as it was and no
/// temporary file is left (a `Staged` that was not committed removes its file when dropped).
#[derive(Debug)]
pub enum PublishError {
    /// The name no longer holds what [`Expect`] said: the next run takes it from there.
    Changed,
    /// [`Ctl::cancelled`] said stop.
    Cancelled,
    /// The disk is full.
    DiskFull,
    Io(io::Error),
    /// The folder could not be walked or the name is no name ([`BeneathError`]).
    Walk(BeneathError),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishError::Changed => {
                write!(f, "the file changed meanwhile: the old one stays as it was")
            }
            PublishError::Cancelled => write!(f, "cancelled: the old one stays as it was"),
            PublishError::DiskFull => write!(f, "the disk is full: the old one stays as it was"),
            PublishError::Io(e) => write!(f, "{e}"),
            PublishError::Walk(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PublishError {}

impl From<io::Error> for PublishError {
    fn from(e: io::Error) -> Self {
        if Stopped::is(&e) {
            // A body that was told to stop (a reader of a `Ctl`): a cancel, not a failure.
            PublishError::Cancelled
        } else if beneath::is_disk_full(&e) {
            PublishError::DiskFull
        } else {
            PublishError::Io(e)
        }
    }
}

impl From<BeneathError> for PublishError {
    fn from(e: BeneathError) -> Self {
        match e {
            BeneathError::Io(e) => PublishError::from(e),
            other => PublishError::Walk(other),
        }
    }
}

impl From<PublishError> for io::Error {
    fn from(e: PublishError) -> Self {
        match e {
            PublishError::Io(e) => e,
            PublishError::DiskFull => {
                io::Error::new(io::ErrorKind::StorageFull, "the disk is full")
            }
            PublishError::Cancelled => Stopped::io(),
            other => io::Error::other(other.to_string()),
        }
    }
}

impl From<PublishError> for BeneathError {
    fn from(e: PublishError) -> Self {
        match e {
            PublishError::Walk(e) => e,
            other => BeneathError::Io(other.into()),
        }
    }
}

impl PublishError {
    /// As the error of a drive call on `key`.
    #[must_use]
    pub fn into_drive(self, key: &str) -> DriveError {
        match self {
            PublishError::Changed => DriveError::Conflict {
                key: key.to_string(),
            },
            PublishError::Cancelled => DriveError::Cancelled,
            PublishError::Walk(BeneathError::BadName(_)) => DriveError::InvalidKey {
                key: key.to_string(),
                reason: "it has a name that cannot be a file name",
            },
            PublishError::Walk(BeneathError::ThroughLink(_) | BeneathError::CrossesMount(_)) => {
                DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: crate::local::THROUGH_LINK,
                }
            }
            other => DriveError::Io(format!("{key}: {other}")),
        }
    }

    /// Whether the new version stopped because the name changed.
    #[must_use]
    pub fn is_changed(&self) -> bool {
        matches!(self, PublishError::Changed)
    }
}

/// One step of a publish, in the order they are taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The file of its own is made.
    Stage,
    /// A chunk is written (once per chunk).
    Write,
    /// The file is synced to the disk.
    SyncFile,
    /// The name is looked at against what it must hold.
    Check,
    /// The file is renamed over the name.
    Rename,
    /// [`replace_keeping`] only: the old file has its place in the slot, and the new one is about
    /// to take its name. A kill here leaves the old file at its name (a test stops the world here).
    Replace,
    /// The folder is synced.
    SyncDir,
}

/// A look at every step before it is taken: a test records the order and cuts a step by
/// failing it (the injectable seam of the acceptance gate); production takes none.
pub(crate) trait Probe {
    fn before(&self, step: Step) -> io::Result<()>;
}

pub(crate) struct Unprobed;

impl Probe for Unprobed {
    fn before(&self, _step: Step) -> io::Result<()> {
        Ok(())
    }
}

/// The bits a new file is made with, and the bits it is given before it is synced (`None`: the
/// ones it was made with stand).
fn bits(mode: Mode, old: Option<&Stat>) -> (u32, Option<u32>) {
    let old_file = old.filter(|st| st.kind == Kind::File);
    match mode {
        Mode::Private => (0o600, None),
        Mode::Shared => (0o666, None),
        Mode::Keep => match old_file {
            Some(st) => (0o600, Some(st.mode & 0o777)),
            None => (0o666, None),
        },
        Mode::FromSource(source) => (0o600, Some(source & 0o777)),
        Mode::Synced { exec } => {
            let mut bits = old_file.map_or(0o644, |st| (st.mode & 0o666) | 0o600);
            if exec {
                bits |= (bits & 0o444) >> 2;
            }
            (0o600, Some(bits))
        }
    }
}

#[cfg(unix)]
fn set_bits(file: &File, bits: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(bits))
}

#[cfg(not(unix))]
fn set_bits(_file: &File, _bits: u32) -> io::Result<()> {
    Ok(())
}

/// A read that is retried when a signal interrupted it.
fn read_some(body: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match body.read(buf) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            other => return other,
        }
    }
}

/// A new version written, synced and not yet in place: a file of its own in `dir`. Dropped
/// without being committed, the file it made is removed (a file it only adopted is the
/// caller's and stays).
pub struct Staged<'a> {
    dir: &'a Dir,
    name: String,
    len: u64,
    blake3: Option<[u8; 32]>,
    stat: Stat,
    owned: bool,
    placed: bool,
}

impl Staged<'_> {
    /// Bytes in the staged file.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the staged file is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The file's name in its folder (for a test that looks for left-over files).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The file's path from the root its folder was reached under: what a journal writes down
    /// before the staged file goes over a name, so that a restart finds it again.
    #[must_use]
    pub fn rel(&self) -> String {
        match self.dir.rel() {
            "" => self.name.clone(),
            folder => format!("{folder}/{}", self.name),
        }
    }
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        if self.owned && !self.placed {
            let _ = self.dir.unlink(&self.name);
        }
    }
}

/// A complete file of the caller's, to be adopted by [`publish_staged`]: its folder and name.
#[derive(Clone, Copy)]
pub struct Blob<'a> {
    pub dir: &'a Dir,
    pub name: &'a str,
}

/// Where a file that is replaced is kept ([`replace_keeping`]): the folder and the name it gets.
#[derive(Clone, Copy)]
pub struct TrashSlot<'a> {
    pub dir: &'a Dir,
    pub name: &'a str,
}

/// Writes `body` into a new file of its own in `dir` (named after `hint`, the name it is meant
/// for), with its bits and its time, synced when `o.durability` asks: everything of a publish but
/// the check and the rename.
///
/// # Errors
///
/// [`PublishError::Cancelled`] when `o.ctl` says stop, [`PublishError::DiskFull`], or the I/O
/// error that stopped the write; no file is left then.
pub fn stage<'a>(
    dir: &'a Dir,
    hint: &str,
    body: &mut dyn Read,
    o: &Publish<'_>,
) -> Result<Staged<'a>, PublishError> {
    stage_probed(dir, hint, body, o, &Unprobed)
}

pub(crate) fn stage_probed<'a>(
    dir: &'a Dir,
    hint: &str,
    body: &mut dyn Read,
    o: &Publish<'_>,
    probe: &dyn Probe,
) -> Result<Staged<'a>, PublishError> {
    beneath::check_file_name(hint)?;
    probe.before(Step::Stage)?;
    let old = dir.lstat(hint).ok();
    let (create, finally) = bits(o.mode, old.as_ref());
    let name = beneath::temp_name(hint);
    let mut file = dir.create_new(&name, create)?;
    // From here on the file is ours until it is renamed in: every way out removes it.
    let written = (|| -> Result<(u64, [u8; 32], Stat), PublishError> {
        // Private to this user before a byte of it is written (Unix made it `rw-------`).
        if o.mode == Mode::Private {
            keep_new_file_private(dir, &name)?;
        }
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            if o.ctl.cancelled() {
                return Err(PublishError::Cancelled);
            }
            let n = read_some(body, &mut buf)?;
            if n == 0 {
                break;
            }
            probe.before(Step::Write)?;
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            total += n as u64;
            o.ctl.bytes(total);
        }
        if let Some(bits) = finally {
            set_bits(&file, bits)?;
        }
        if let Some(mtime) = o.mtime {
            file.set_modified(mtime)?;
        }
        if o.durability == Durability::State {
            probe.before(Step::SyncFile)?;
            file.sync_all()?;
        }
        let stat = beneath::stat_of(&file)?;
        Ok((total, *hasher.finalize().as_bytes(), stat))
    })();
    drop(file);
    match written {
        Ok((len, hash, stat)) => Ok(Staged {
            dir,
            name,
            len,
            blake3: Some(hash),
            stat,
            owned: true,
            placed: false,
        }),
        Err(e) => {
            let _ = dir.unlink(&name);
            Err(e)
        }
    }
}

/// A complete file of the caller's made ready to be renamed in: its bits and its time set, synced
/// when `o.durability` asks. `old` is the file it will replace (for [`Mode::Keep`] and
/// [`Mode::Synced`]).
fn adopt<'a>(
    blob: Blob<'a>,
    old: Option<&Stat>,
    o: &Publish<'_>,
    probe: &dyn Probe,
) -> Result<Staged<'a>, PublishError> {
    probe.before(Step::Stage)?;
    let (file, stat) = blob.dir.open_read(blob.name)?;
    let (_, finally) = bits(o.mode, old);
    #[cfg(not(unix))]
    let file = {
        // A file opened to read cannot be given a time or synced here: open it to write - and to
        // read as well, so the handle can be asked for its identity and times afterwards.
        drop(file);
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(blob.dir.path().join(blob.name))?
    };
    if let Some(bits) = finally {
        set_bits(&file, bits)?;
    }
    if let Some(mtime) = o.mtime {
        file.set_modified(mtime)?;
    }
    if o.durability == Durability::State {
        probe.before(Step::SyncFile)?;
        file.sync_all()?;
    }
    let stat = beneath::stat_of(&file).unwrap_or(stat);
    Ok(Staged {
        dir: blob.dir,
        name: blob.name.to_string(),
        len: stat.size,
        blake3: None,
        stat,
        owned: false,
        placed: false,
    })
}

/// A complete file of the caller's (a download spooled beside the folder's bookkeeping) made
/// ready to be put in place by [`replace_keeping`] or [`commit`]: its bits and time set, synced
/// when `o.durability` asks. `old` is the file it will replace (for [`Mode::Keep`] and
/// [`Mode::Synced`]). The blob stays the caller's: it is not removed when the staged file drops.
///
/// # Errors
///
/// The I/O error of making it ready.
pub fn stage_blob<'a>(
    blob: Blob<'a>,
    old: Option<&Stat>,
    o: &Publish<'_>,
) -> Result<Staged<'a>, PublishError> {
    adopt(blob, old, o, &Unprobed)
}

/// What `expect` says about the name now: `Ok` when the new version may go on.
fn check_expect(dir: &Dir, name: &str, expect: Expect) -> Result<(), PublishError> {
    match expect {
        Expect::Any => Ok(()),
        Expect::Absent => match dir.lstat(name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(PublishError::Changed),
            Err(e) => Err(e.into()),
        },
        Expect::Is(print) => match dir.lstat(name) {
            Ok(st) if print.matches(&st).is_same() => Ok(()),
            // Gone, another file, written to - or not told apart for certain.
            _ => Err(PublishError::Changed),
        },
    }
}

/// The last step of a publish: the folders the rename changed are synced, so the new entry is on the
/// disk before the caller is told it landed. A file system that cannot sync a folder at all (FAT on
/// macOS: [`beneath::folder_sync_refused`]) is no failure - the file is in place and was synced. An
/// I/O error IS one: `Landed` used to be made whatever the sync found, and a Move deleted its source
/// on that word, while the target's entry could be lost with the disk's cache (EIO, ENOSPC on a
/// failing disk; X-DELTA-AZDRIVE-FILES-14). The file is in its place and stays; the publish says it
/// could not make it durable, and nothing is retired on its word.
fn sync_folders(probe: &dyn Probe, folders: &[&Dir]) -> Result<(), PublishError> {
    let synced = probe.before(Step::SyncDir).and_then(|()| {
        for folder in folders {
            folder.sync_checked()?;
        }
        Ok(())
    });
    match synced {
        Err(e) if !beneath::folder_sync_refused(&e) => Err(PublishError::Io(e)),
        _ => Ok(()),
    }
}

/// Checks the name against `expect` and renames the staged file over it, then syncs the folder:
/// the last two steps of a publish. A failure leaves the old version as it was (and removes an
/// owned staged file when `staged` drops).
///
/// # Errors
///
/// [`PublishError::Changed`] when the name no longer holds what `expect` says;
/// [`PublishError::Cancelled`] when `ctl` says stop; else the I/O error.
pub fn commit(
    staged: Staged<'_>,
    dir: &Dir,
    name: &str,
    expect: Expect,
    durability: Durability,
    ctl: &dyn Ctl,
) -> Result<Landed, PublishError> {
    commit_probed(staged, dir, name, expect, durability, ctl, &Unprobed)
}

pub(crate) fn commit_probed(
    mut staged: Staged<'_>,
    dir: &Dir,
    name: &str,
    expect: Expect,
    durability: Durability,
    ctl: &dyn Ctl,
    probe: &dyn Probe,
) -> Result<Landed, PublishError> {
    beneath::check_file_name(name)?;
    probe.before(Step::Check)?;
    if ctl.cancelled() {
        return Err(PublishError::Cancelled);
    }
    check_expect(dir, name, expect)?;
    probe.before(Step::Rename)?;
    let renamed = match expect {
        // Taken in the one step the rename is: a file made since the look is not replaced.
        Expect::Absent => staged.dir.rename_noreplace(&staged.name, dir, name),
        Expect::Is(_) | Expect::Any => staged.dir.rename(&staged.name, dir, name),
    };
    match renamed {
        Ok(()) => staged.placed = true,
        Err(e) if e.is_already_exists() => return Err(PublishError::Changed),
        Err(e) => return Err(e.into()),
    }
    if durability == Durability::State {
        let mut folders = vec![dir];
        if staged.dir.rel() != dir.rel() {
            folders.push(staged.dir);
        }
        sync_folders(probe, &folders)?;
    }
    let print = dir
        .lstat(name)
        .map_or_else(|_| Fingerprint::of(&staged.stat), |st| Fingerprint::of(&st));
    Ok(Landed {
        len: staged.len,
        blake3: staged.blake3,
        print,
        _sealed: (),
    })
}

/// Puts `body` in place as `name` of `dir`: [`stage`] and [`commit`].
///
/// # Errors
///
/// As [`stage`] and [`commit`].
pub fn publish(
    dir: &Dir,
    name: &str,
    body: &mut dyn Read,
    o: Publish<'_>,
) -> Result<Landed, PublishError> {
    publish_probed(dir, name, body, o, &Unprobed)
}

pub(crate) fn publish_probed(
    dir: &Dir,
    name: &str,
    body: &mut dyn Read,
    o: Publish<'_>,
    probe: &dyn Probe,
) -> Result<Landed, PublishError> {
    beneath::check_file_name(name)?;
    let staged = stage_probed(dir, name, body, &o, probe)?;
    commit_probed(staged, dir, name, o.expect, o.durability, o.ctl, probe)
}

/// Puts the complete file `blob` (in a folder of the same disk: a spool) in place as `name` of
/// `dir`. The blob is the caller's: it is renamed away when it lands and left where it is when it
/// does not.
///
/// # Errors
///
/// As [`commit`], and the I/O error of making the blob ready.
pub fn publish_staged(
    dir: &Dir,
    name: &str,
    blob: Blob<'_>,
    o: Publish<'_>,
) -> Result<Landed, PublishError> {
    publish_staged_probed(dir, name, blob, o, &Unprobed)
}

pub(crate) fn publish_staged_probed(
    dir: &Dir,
    name: &str,
    blob: Blob<'_>,
    o: Publish<'_>,
    probe: &dyn Probe,
) -> Result<Landed, PublishError> {
    beneath::check_file_name(name)?;
    let old = dir.lstat(name).ok();
    let staged = adopt(blob, old.as_ref(), &o, probe)?;
    commit_probed(staged, dir, name, o.expect, o.durability, o.ctl, probe)
}

/// Puts `staged` in place as `name` of `dir` and keeps the file that was there in `slot`: the new
/// version is whole and synced before the old one leaves its place, the old one is renamed to the
/// slot (never over what is there), then the new one into the free name; when that second rename
/// fails the old one is renamed back. The old file is never deleted by this call.
///
/// # Errors
///
/// As [`commit`]; [`PublishError::Io`] when the slot is taken, or when the new version could not
/// go in AND the old one could not be put back (the message says where the old one is).
pub fn replace_keeping(
    dir: &Dir,
    name: &str,
    staged: Staged<'_>,
    slot: &TrashSlot<'_>,
    expect: Expect,
) -> Result<Landed, PublishError> {
    replace_keeping_probed(dir, name, staged, slot, expect, &Unprobed)
}

pub(crate) fn replace_keeping_probed(
    dir: &Dir,
    name: &str,
    mut staged: Staged<'_>,
    slot: &TrashSlot<'_>,
    expect: Expect,
    probe: &dyn Probe,
) -> Result<Landed, PublishError> {
    beneath::check_file_name(name)?;
    probe.before(Step::Check)?;
    check_expect(dir, name, expect)?;
    probe.before(Step::Rename)?;
    let old = keep_old(dir, name, slot, expect)?;
    probe.before(Step::Replace)?;
    let placed = match old {
        Old::Linked => replace_linked(dir, name, &staged, slot),
        Old::Moved | Old::Nothing => match staged.dir.rename_noreplace(&staged.name, dir, name) {
            Ok(()) => Ok(()),
            Err(e) if old == Old::Nothing => Err(e.into()),
            // The new version could not go in: the old one comes back.
            Err(e) => {
                return match slot.dir.rename_noreplace(slot.name, dir, name) {
                    Ok(()) => Err(e.into()),
                    Err(back) if back.is_not_found() => Err(e.into()),
                    Err(back) => Err(PublishError::Io(io::Error::other(format!(
                        "the new version could not go in ({e}) and the old one could not be put \
                         back ({back}): it is kept as \"{}\"",
                        slot.name
                    )))),
                };
            }
        },
    };
    placed?;
    staged.placed = true;
    let mut folders = vec![dir];
    if staged.dir.rel() != dir.rel() {
        folders.push(staged.dir);
    }
    if slot.dir.rel() != dir.rel() {
        folders.push(slot.dir);
    }
    sync_folders(probe, &folders)?;
    let print = dir
        .lstat(name)
        .map_or_else(|_| Fingerprint::of(&staged.stat), |st| Fingerprint::of(&st));
    Ok(Landed {
        len: staged.len,
        blake3: staged.blake3,
        print,
        _sealed: (),
    })
}

/// How the file that was at the name got its place in the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Old {
    /// A second name (a hard link): the file is at its name AND in the slot.
    Linked,
    /// Renamed out of its place into the slot: the name is empty.
    Moved,
    /// Nothing was at the name.
    Nothing,
}

/// The old file gets its place in the slot WITHOUT leaving its name first, where the file system
/// makes hard links: a second name for it, then the new version renamed over the first. A kill or a
/// power cut between the steps leaves the old file at its name, the new one staged beside it and
/// the slot naming the old one - never an empty name, which the next sync pass reads as "deleted
/// here" and takes to the drive (X-DELTA-AZDRIVE-FILES-26: the failure AZ-S4 closed, at the width
/// of two renames). A volume without links takes the old file out of the way first (two renames).
fn keep_old(
    dir: &Dir,
    name: &str,
    slot: &TrashSlot<'_>,
    expect: Expect,
) -> Result<Old, PublishError> {
    // Nothing was there to keep (or it went since the look): the name is free.
    let nothing = || {
        if matches!(expect, Expect::Is(_)) {
            Err(PublishError::Changed)
        } else {
            Ok(Old::Nothing)
        }
    };
    match dir.link_noreplace(name, slot.dir, slot.name) {
        Ok(()) => return Ok(Old::Linked),
        Err(e) if e.is_not_found() => return nothing(),
        Err(e) if e.is_unsupported() => {}
        Err(e) => return Err(e.into()),
    }
    match dir.rename_noreplace(name, slot.dir, slot.name) {
        Ok(()) => Ok(Old::Moved),
        Err(e) if e.is_not_found() => nothing(),
        Err(e) => Err(e.into()),
    }
}

/// The new version renamed over the name, in one step, once the name is still the file that was
/// linked into the slot: a file another writer put at the name since is not replaced (the old one
/// stays in the slot, the other writer's at its name). A rename that fails leaves the old file at
/// its name, and the slot's second name for it goes again.
fn replace_linked(
    dir: &Dir,
    name: &str,
    staged: &Staged<'_>,
    slot: &TrashSlot<'_>,
) -> Result<(), PublishError> {
    let same = match (dir.lstat(name), slot.dir.lstat(slot.name)) {
        (Ok(at_name), Ok(in_slot)) => at_name.id().same(&in_slot.id()),
        _ => Tri::Unknown,
    };
    match same {
        Tri::Same => {}
        // The old file's only name is the slot's now: it is NOT unlinked.
        Tri::Different => return Err(PublishError::Changed),
        Tri::Unknown => {
            return Err(PublishError::Io(io::Error::other(
                "the system does not tell whether the name still holds the file kept in the slot",
            )))
        }
    }
    match staged.dir.rename(&staged.name, dir, name) {
        Ok(()) => Ok(()),
        Err(e) => {
            // collapse-ok: the old file is at its name and was linked a moment ago by this call;
            // a link that cannot go again is a stray name for a file that is kept, no loss.
            let _ = slot.dir.unlink(slot.name);
            Err(e.into())
        }
    }
}

/// Makes the folders of `path`'s parent that are missing (`rwx------` for a private document)
/// and opens it, links followed: the folder is the user's choice.
fn parent_of(path: &Path, private: bool) -> io::Result<(Dir, String)> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name in the path"))?
        .to_string();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        make_dirs(parent, private)?;
    }
    Ok((Dir::open_following(parent)?, name))
}

/// Makes `dir` and the folders above it that are missing; with `private`, the ones made are for this
/// user alone: `rwx------` on Unix, on Windows a protected access list (this user, the system and
/// the administrators) that what is made inside inherits - a data volume's root grants its users
/// read, and the folder made there inherited it (AZ-W12). A disk that keeps no owners (FAT) keeps
/// none, as `chmod` on it does nothing.
fn make_dirs(dir: &Path, private: bool) -> io::Result<()> {
    #[cfg(windows)]
    if private {
        return make_private_dirs(dir);
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        if private {
            builder.mode(0o700);
        }
    }
    #[cfg(not(unix))]
    let _ = private;
    builder.create(dir)
}

/// Windows: the missing folders of `dir`, outermost first, each made and given its access list
/// before anything is made inside it. A folder somebody else made meanwhile is theirs, not ours to
/// restrict.
#[cfg(windows)]
fn make_private_dirs(dir: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut at = Some(dir);
    while let Some(folder) = at {
        if folder.is_dir() {
            break;
        }
        missing.push(folder);
        at = folder.parent().filter(|p| !p.as_os_str().is_empty());
    }
    for folder in missing.into_iter().rev() {
        match std::fs::create_dir(folder) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        crate::win::make_private(folder, true)?;
    }
    Ok(())
}

/// Makes `dir`, and the folders above it that are missing, private to this user: `rwx------` on
/// Unix, an access list for this user, the system and the administrators on Windows (see
/// [`Mode::Private`]). A folder that is there already is left as it is: it may be somebody's, and
/// this does not take it over.
///
/// # Errors
///
/// The I/O error that stopped it; on Windows also a refusal of the access list on a disk that
/// keeps lists - a state folder is not left readable by others because the protection did not take.
pub fn create_private_dirs(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    make_dirs(dir, true)
}

/// A file just made with [`Mode::Private`] given the Windows access list before it is written to
/// (on Unix it was made `rw-------`).
fn keep_new_file_private(dir: &Dir, name: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::win::make_private(&dir.path().join(name), false).map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = (dir, name);
        Ok(())
    }
}

/// Makes the file at `path`, which the caller has just made (so this user owns it), private to this
/// user where the system has access lists: on Windows a protected list for this user, the system
/// and the administrators, instead of the one the folder it was made in lends it (a state folder
/// that existed before was not made private, and what is made in it inherits its list); on Unix the
/// file was made `rw-------` and nothing is done.
///
/// # Errors
///
/// On Windows, a refusal of the access list on a disk that keeps lists - a secret is not left
/// readable because the protection did not take.
pub fn make_file_private(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::win::make_private(path, false).map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

pub(crate) fn replace_probed(
    path: &Path,
    bytes: &[u8],
    mode: Mode,
    durability: Durability,
    probe: &dyn Probe,
) -> io::Result<()> {
    let (dir, name) = parent_of(path, mode == Mode::Private)?;
    let publish_it = Publish {
        expect: Expect::Any,
        mode,
        mtime: None,
        durability,
        ctl: &Quiet,
    };
    publish_probed(&dir, &name, &mut &bytes[..], publish_it, probe)
        .map(|_| ())
        .map_err(io::Error::from)
}

/// Replaces the document at `path` with `bytes` (settings, a state file, a JSON document): the
/// old version or the new one survives a crash and a power cut ([`Durability::State`]); the
/// folder is made when it is missing. `Expect::Any`: the last writer wins - a document several
/// processes change goes through a lock of its own.
///
/// # Errors
///
/// The I/O error that stopped the write (the old file stays then).
pub fn replace(path: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    replace_probed(path, bytes, mode, Durability::State, &Unprobed)
}

/// [`replace`] for a file that checks itself when it is read (a listing cache, a node list): the
/// rename only ([`Durability::Cache`]), shared bits.
///
/// # Errors
///
/// The I/O error that stopped the write (the old file stays then).
pub fn replace_cache(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace_probed(path, bytes, Mode::Shared, Durability::Cache, &Unprobed)
}

/// Renames the file `path` next to itself as `<name>.damaged-<tag>`, never over a file that has
/// that name: the bytes of a file that is there and cannot be read stay (the next write does not
/// replace the only record of them) and its name is free. The new path.
///
/// # Errors
///
/// When the file cannot be renamed, or the name it would take is taken.
pub fn set_aside(path: &Path, tag: &str) -> io::Result<PathBuf> {
    let (dir, name) = parent_of(path, false)?;
    let aside = format!("{name}.damaged-{tag}");
    dir.rename_noreplace(&name, &dir, &aside)
        .map_err(|e| match e {
            BeneathError::Io(e) => e,
            other => io::Error::other(other.to_string()),
        })?;
    Ok(path.with_file_name(aside))
}

/// What a cut-off write leaves in a folder: a temporary file of [`stage`] (its name carries the
/// process that made it), or a hidden partial download.
enum Leftover {
    Temp,
    Partial,
}

fn leftover(name: &str) -> Option<Leftover> {
    if beneath::is_temp_name(name) {
        Some(Leftover::Temp)
    } else if name.starts_with('.')
        && (name.ends_with(".download.part") || name.ends_with(".download-state.part"))
    {
        Some(Leftover::Partial)
    } else {
        None
    }
}

/// The process a temporary file's name (`.<name>.azul-storage-<pid>-<n>.tmp`) says made it.
fn owner_of(name: &str) -> Option<u32> {
    let rest = &name[name.rfind(".azul-storage-")? + ".azul-storage-".len()..];
    rest.split('-').next()?.parse().ok()
}

/// Whether the process `pid` runs (or may: a process of another user, or a system that cannot
/// be asked, counts as running).
pub(crate) fn process_runs(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return true;
        };
        if pid <= 0 {
            return true;
        }
        // SAFETY: signal 0 only asks whether the process exists.
        let asked = unsafe { libc::kill(pid, 0) };
        asked == 0 || io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Removes what writes that were cut off left in `dir` (a killed app, a power cut): a temporary
/// file of this module whose process is gone or that is older than `older_than`, and a hidden
/// partial download (`.<name>.download.part` with its state file) older than that - a download
/// nobody resumed. A file of a process that runs, or a young partial, stays. How many went.
pub fn sweep_temps(dir: &Dir, older_than: Duration) -> usize {
    let Ok(names) = dir.names() else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for name in &names {
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(what) = leftover(name) else {
            continue;
        };
        let Ok(st) = dir.lstat(name) else {
            continue;
        };
        if st.kind != Kind::File {
            continue;
        }
        let age = now
            .duration_since(UNIX_EPOCH + Duration::from_nanos(st.mtime_ns))
            .unwrap_or_default();
        let old = age >= older_than;
        let orphaned = match what {
            Leftover::Temp => owner_of(name).is_some_and(|pid| !process_runs(pid)),
            Leftover::Partial => false,
        };
        if (old || orphaned) && dir.unlink(name).is_ok() {
            removed += 1;
        }
    }
    removed
}
