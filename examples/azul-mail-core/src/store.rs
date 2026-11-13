//! The synced mail as files, laid out like an object store's keys.
//!
//! Under the account's mail folder (see `account::mail_root`):
//!
//! ```text
//! mail/<folder>/<yyyy>/<mm>/<uid>.eml   one message, the exact bytes the server sent
//! mail/<folder>/index.jsonl             one line per message (IndexEntry)
//! mail/<folder>/state.json              UIDVALIDITY and the last synced UID (FolderState)
//! stale/<folder>/<uidvalidity>/...      a folder's files from before the server renumbered it
//! removed/<folder>/<stamp>/...          what an Azlin account's pull took out of a folder because
//!                                       the drive no longer lists it, held for 90 days
//! ```
//!
//! `<yyyy>/<mm>` is the month the server received the message (its INTERNALDATE, in UTC), so a
//! message's path is known before its body is fetched. Every key is a `/`-separated relative path,
//! the same string an S3 bucket would use under the account's prefix.
//!
//! [`MailStore`] is the one writer, and it writes through the shared azul-storage `Drive`
//! (`examples/azul-storage`): ONE `LocalDrive` at the data root, the account's folder as its
//! scoped prefix ([`DriveFolder`]), so every file lands in the data tree's one `.azlin/cache`
//! manifest and the later S3 sync swaps the drive, nothing else. The drive writes a file whole or
//! not at all (a temporary file, then a rename). An account synced to a folder outside the data
//! tree gets a drive of its own there that keeps no manifest.

use std::path::{Component, Path, PathBuf};

use azul_storage::{Drive, DriveError, LocalDrive, ScopedDrive};
use serde::{Deserialize, Serialize};

use crate::message;

/// The prefix of every synced folder.
pub const MAIL_PREFIX: &str = "mail";
/// Where a renumbered folder's old files go.
pub const STALE_PREFIX: &str = "stale";
/// Where an Azlin account's pull keeps what the drive no longer lists, for the span a drive keeps
/// its own history: `removed/<folder>/<stamp>/<yyyy>/<mm>/<uid>.eml` and, beside them, the folder's
/// index lines of what left at `<stamp>` (`index.jsonl`). A pull never destroys a copy; the sweep
/// of this folder is the only thing that does, after the span (X-DELTA-APPS3-3).
pub const REMOVED_PREFIX: &str = "removed";
pub const INDEX_FILE: &str = "index.jsonl";
pub const STATE_FILE: &str = "state.json";
/// The `format` of a folder's state file.
pub const STATE_FORMAT: &str = "azmail.folder";
/// The state file version this AzMail writes, and the newest it reads.
pub const STATE_VERSION: u64 = 1;

/// Whether `key` is a relative `/`-separated path with no empty, `.` or `..` segment, no
/// backslash and no NUL: the keys AzMail writes, and nothing that could leave the folder.
pub fn is_valid_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('\\')
        && !key.contains('\0')
        && key
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// `mail/<folder>/<yyyy>/<mm>/<uid>.eml`. A year or month of 0 (no date) is `0000/00`.
pub fn message_key(folder: &str, year: i32, month: u32, uid: u32) -> String {
    format!("{MAIL_PREFIX}/{folder}/{year:04}/{month:02}/{uid}.eml")
}

/// `mail/<folder>`
pub fn folder_prefix(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}")
}

/// `mail/<folder>/index.jsonl`
pub fn index_key(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}/{INDEX_FILE}")
}

/// `mail/<folder>/state.json`
pub fn state_key(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}/{STATE_FILE}")
}

/// `stale/<folder>/<uidvalidity>`
pub fn stale_prefix(folder: &str, uidvalidity: u32) -> String {
    format!("{STALE_PREFIX}/{folder}/{uidvalidity}")
}

/// `removed/<folder>/<stamp>` (`<stamp>` is the SigV4 date of the moment the pull took the mail
/// out: `20261008T091500Z`). To NAME a holding folder that is there (the sweep reads its stamp back
/// from the key); a hold that is MADE takes its date from [`HoldAt`], whose one constructor is
/// [`Clocks::hold_time`].
pub fn removed_prefix(folder: &str, stamp: &str) -> String {
    format!("{REMOVED_PREFIX}/{folder}/{stamp}")
}

// ==== The clock a hold of mail is dated by ====

/// How far behind the newest Send/Receive of an account the clock of this computer may be before no hold is
/// dated by it: a correction, the winter time, a zone changed by hand.
pub const CLOCK_BEHIND_SECS: u64 = 24 * 60 * 60;

/// How far ahead of everything an account and its drive remember the clock of this computer may be before no
/// hold is dated by it: a computer that was away for a year is a computer that was away, and a clock a year
/// ahead of the newest Send/Receive AND the newest mail of the drive is a clock that is wrong.
pub const CLOCK_AHEAD_SECS: u64 = 366 * 24 * 60 * 60;

/// The moment a holding folder of `removed/` is dated: the stamp of `removed/<folder>/<stamp>/`, which the sweep
/// of the account counts its 90 days from. A hold dated by a clock that is BEHIND (a reset RTC, a virtual
/// machine resumed before the time sync: 1970, 2000) was swept by the next Send/Receive with a right clock - the
/// held copy lasted ONE sync - and one dated by a clock that is ahead was kept for years. So the ONE constructor
/// is [`Clocks::hold_time`], which sets the clock against what the account remembers of the time; every writer
/// of `removed/` takes its date from it: the pull of an Azlin account (`azlin_sync::hold`) and Delete in the
/// Outbox ([`crate::send::remove_failed_entry`], which dated its hold by the raw clock until X-DELTA-APPS5-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldAt(u64);

impl HoldAt {
    /// Seconds since 1970.
    #[must_use]
    pub fn secs(self) -> u64 {
        self.0
    }

    /// `removed/<folder>/<stamp>` of this moment.
    #[must_use]
    pub fn prefix(self, folder: &str) -> String {
        removed_prefix(folder, &azul_storage::time::amz_date(self.0))
    }
}

/// Why no hold is dated by the clock of this computer ([`Clocks::hold_time`]). The mail stays where it is, and
/// whoever asked says why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClockRefusal {
    /// The clock is not set (0, or before 1970).
    NotSet,
    /// The clock is more than [`CLOCK_BEHIND_SECS`] before the newest Send/Receive of the account.
    Behind { now: u64, last: u64 },
    /// The clock is more than [`CLOCK_AHEAD_SECS`] after everything the account and its drive remember;
    /// `drive`: the newest mail of the drive was one of the readings.
    Ahead { now: u64, known: u64, drive: bool },
    /// What the account remembers of its last Send/Receive cannot be read (a listing of `mail/` that failed, a
    /// state file that is damaged): the text of the error. A floor that cannot be read is no floor of nothing -
    /// a clock set against nothing would date a hold by any clock (X-DELTA-APPS5-6).
    FloorUnreadable(String),
}

impl std::fmt::Display for ClockRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let said = |secs: u64| message::rfc3339_utc(i64::try_from(secs).unwrap_or(i64::MAX));
        match self {
            ClockRefusal::NotSet => write!(f, "the clock of this computer is not set"),
            ClockRefusal::Behind { now, last } => write!(
                f,
                "the clock of this computer says {}, more than a day before the last Send/Receive ({})",
                said(*now),
                said(*last)
            ),
            ClockRefusal::Ahead { now, known, drive } => write!(
                f,
                "the clock of this computer says {}, more than a year after the last Send/Receive{} ({})",
                said(*now),
                if *drive {
                    " and the newest mail of the drive"
                } else {
                    ""
                },
                said(*known)
            ),
            ClockRefusal::FloorUnreadable(why) => write!(
                f,
                "the time of the last Send/Receive cannot be read ({why})"
            ),
        }
    }
}

/// The readings of the time that the clock of this computer is set against when mail is held
/// ([`Clocks::hold_time`]). The clock of a computer is wrong more often than a delete is meant, and a hold
/// is the one thing a wrong clock can make last one sync (X-DELTA-APPS4-3, X-APPS3-1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clocks {
    /// What the account remembers of its newest Send/Receive ([`MailStore::last_sync`]).
    floor: Floor,
    /// The newest `modified` of the mail the drive listed at the start of the Send/Receive: the drive's own
    /// word that it lived at least until then - a LOWER bound of its time, never a later one than it is.
    /// `None`: no drive (the Outbox's Delete has none at hand), or it lists no mail.
    drive: Option<u64>,
}

/// What an account remembers of the time of its last Send/Receive: the newest `synced_at` of its folders as the
/// newest Send/Receive found them - what this computer's own clock said at the last one - or why that could not
/// be read. The failure has its own arm: it is never "no Send/Receive yet".
#[derive(Clone, Debug, PartialEq, Eq)]
enum Floor {
    /// The newest stamp; `None`: the account never synced.
    Read(Option<u64>),
    /// The text of the error that stopped the read.
    Unreadable(String),
}

impl Default for Floor {
    fn default() -> Floor {
        Floor::Read(None)
    }
}

impl Clocks {
    /// The readings: the newest `synced_at` of the account (`None`: no Send/Receive yet) and the newest mail
    /// of the drive (`None`: none listed, or no drive).
    #[must_use]
    pub fn new(last_sync: Option<u64>, drive: Option<u64>) -> Clocks {
        Clocks {
            floor: Floor::Read(last_sync),
            drive,
        }
    }

    /// The readings of the account in `store`, with the newest mail of the drive (`None`: none listed, or no
    /// drive). A floor that cannot be READ is an arm of its own, not an empty one: no hold is dated and nothing
    /// is swept against it ([`Clocks::hold_time`], [`Clocks::sweep_reading`]).
    #[must_use]
    pub fn of(store: &MailStore, drive: Option<u64>) -> Clocks {
        let floor = match store.last_sync() {
            Ok(last_sync) => Floor::Read(last_sync),
            Err(e) => Floor::Unreadable(e.to_string()),
        };
        Clocks { floor, drive }
    }

    /// The newest Send/Receive of the account; `None` too when it cannot be read.
    #[must_use]
    pub fn last_sync(&self) -> Option<u64> {
        match &self.floor {
            Floor::Read(last_sync) => *last_sync,
            Floor::Unreadable(_) => None,
        }
    }

    /// The newest mail of the drive.
    #[must_use]
    pub fn drive(&self) -> Option<u64> {
        self.drive
    }

    /// The drive's reading that the sweep of the holding area sets the clock against ([`Clocks::drive`]) - and
    /// none when what the account remembers of the time cannot be read: nothing is swept then, the next
    /// Send/Receive that can read it sweeps (X-DELTA-APPS5-6).
    #[must_use]
    pub fn sweep_reading(&self) -> Option<u64> {
        match &self.floor {
            Floor::Read(_) => self.drive,
            Floor::Unreadable(_) => None,
        }
    }

    /// The moment a hold made now (`now`: seconds since 1970 by the clock of this computer) is dated, or why
    /// none is made. The stamp is the later of the clock and the newest Send/Receive - a clock a few hours
    /// behind never dates a hold before the last sync. Nothing is held on a clock that is not set, one that is
    /// more than [`CLOCK_BEHIND_SECS`] behind the last Send/Receive, or one that is more than
    /// [`CLOCK_AHEAD_SECS`] ahead of both the last Send/Receive and the newest mail the drive lists: the mail
    /// stays where it is and the caller says why.
    ///
    /// # Errors
    ///
    /// The clock cannot date a hold: [`ClockRefusal`] - a floor that cannot be read among its reasons.
    pub fn hold_time(&self, now: i64) -> Result<HoldAt, ClockRefusal> {
        let Some(now) = u64::try_from(now).ok().filter(|secs| *secs > 0) else {
            return Err(ClockRefusal::NotSet);
        };
        let last_sync = match &self.floor {
            Floor::Read(last_sync) => *last_sync,
            Floor::Unreadable(why) => return Err(ClockRefusal::FloorUnreadable(why.clone())),
        };
        if let Some(last) = last_sync {
            if now.saturating_add(CLOCK_BEHIND_SECS) < last {
                return Err(ClockRefusal::Behind { now, last });
            }
        }
        if let Some(known) = [last_sync, self.drive].into_iter().flatten().max() {
            if now > known.saturating_add(CLOCK_AHEAD_SECS) {
                return Err(ClockRefusal::Ahead {
                    now,
                    known,
                    drive: self.drive.is_some(),
                });
            }
        }
        Ok(HoldAt(now.max(last_sync.unwrap_or(0))))
    }

    /// The `synced_at` a folder records at the end of a Send/Receive made at `now`: the clock of this
    /// computer when it can date a hold ([`Clocks::hold_time`]: set, and not far off the last Send/Receive
    /// and the drive), never earlier than the last Send/Receive; otherwise NONE - a clock that was refused is
    /// not recorded, whatever else is known: it would be the floor that every later right clock is refused
    /// against, or the one that dates a hold years ahead (X-DELTA-APPS5-1: the first Send/Receive of a copy
    /// recorded the clock it had just refused, `now`, when there was no earlier one). The caller keeps what
    /// the folder's own file said.
    #[must_use]
    pub fn recorded(&self, now: i64) -> Option<u64> {
        self.hold_time(now).ok().map(HoldAt::secs)
    }
}

/// A folder of AzMail's files on a drive: the drive's root on this computer, whether that root
/// is the data tree (its drive keeps the `.azlin/cache` manifest) and the folder's key prefix in
/// the drive (empty, or `/`-separated names each ending in `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFolder {
    root: PathBuf,
    data_tree: bool,
    prefix: String,
}

impl DriveFolder {
    /// `path` (a folder on this computer) placed against the data tree at `data_root`: inside
    /// it, a folder of the tree's ONE drive (its files land in the tree's manifest); outside it
    /// (a folder the user chose elsewhere, `AZMAIL_DATA`), the root of a drive of its own that
    /// keeps no manifest.
    pub fn of(path: &Path, data_root: &Path) -> DriveFolder {
        if data_root.as_os_str().is_empty() {
            return DriveFolder::outside(path.to_path_buf());
        }
        let Ok(rest) = path.strip_prefix(data_root) else {
            return DriveFolder::outside(path.to_path_buf());
        };
        let mut prefix = String::new();
        for component in rest.components() {
            match component {
                Component::Normal(name) => match name.to_str() {
                    Some(name) if is_valid_key(name) => {
                        prefix.push_str(name);
                        prefix.push('/');
                    }
                    _ => return DriveFolder::outside(path.to_path_buf()),
                },
                Component::CurDir => {}
                _ => return DriveFolder::outside(path.to_path_buf()),
            }
        }
        DriveFolder {
            root: data_root.to_path_buf(),
            data_tree: true,
            prefix,
        }
    }

    /// `path` as the root of a drive of its own that keeps no manifest (not the data tree).
    pub fn outside(path: PathBuf) -> DriveFolder {
        DriveFolder {
            root: path,
            data_tree: false,
            prefix: String::new(),
        }
    }

    /// `path` placed as this folder's drive places it: on the data tree's drive when this one
    /// is the data tree's and `path` is inside it, else a drive of its own.
    pub fn locate(&self, path: &Path) -> DriveFolder {
        if self.data_tree {
            DriveFolder::of(path, &self.root)
        } else {
            DriveFolder::outside(path.to_path_buf())
        }
    }

    /// The folder `name` (one name, no `/`) in this one.
    pub fn child(&self, name: &str) -> DriveFolder {
        DriveFolder {
            root: self.root.clone(),
            data_tree: self.data_tree,
            prefix: format!("{}{name}/", self.prefix),
        }
    }

    /// Whether the folder is part of the data tree (its drive keeps the manifest).
    pub fn is_data_tree(&self) -> bool {
        self.data_tree
    }

    /// The folder on this computer.
    pub fn path(&self) -> PathBuf {
        self.prefix
            .split('/')
            .filter(|name| !name.is_empty())
            .fold(self.root.clone(), |path, name| path.join(name))
    }

    /// The drive of the folder: the data tree's (with its manifest) or the folder's own,
    /// scoped to the folder.
    fn drive(&self) -> Result<ScopedDrive<LocalDrive>, DriveError> {
        let local = if self.data_tree {
            LocalDrive::new(self.root.clone())
        } else {
            LocalDrive::without_manifest(self.root.clone())
        };
        ScopedDrive::new(local, &self.prefix, true)
    }
}

/// A drive's error as the `std::io::Error` AzMail's callers read (a missing file is `NotFound`).
fn io_error(e: DriveError) -> std::io::Error {
    let kind = match &e {
        DriveError::NotFound { .. } => std::io::ErrorKind::NotFound,
        DriveError::InvalidKey { .. } => std::io::ErrorKind::InvalidInput,
        DriveError::Denied { .. } | DriveError::Locked { .. } => {
            std::io::ErrorKind::PermissionDenied
        }
        _ => std::io::ErrorKind::Other,
    };
    std::io::Error::new(kind, e.to_string())
}

/// The files of one folder of AzMail (an account's synced mail, its own folder, the AzMail
/// folder), read and written through its drive. Blocking: call it from an azul `Thread`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailStore {
    folder: DriveFolder,
}

impl MailStore {
    pub fn new(folder: DriveFolder) -> MailStore {
        MailStore { folder }
    }

    /// Where the files are.
    pub fn folder(&self) -> &DriveFolder {
        &self.folder
    }

    fn drive(&self) -> std::io::Result<ScopedDrive<LocalDrive>> {
        self.folder.drive().map_err(io_error)
    }

    /// The error for a key AzMail never writes.
    fn check(key: &str) -> std::io::Result<()> {
        if is_valid_key(key) {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{key:?} is not a key AzMail writes"),
            ))
        }
    }

    /// Writes the object `key` whole (the drive writes a temporary file and renames it).
    pub fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        Self::check(key)?;
        self.drive()?.put(key, bytes).map_err(io_error)
    }

    /// Reads the object `key`.
    pub fn get(&self, key: &str) -> std::io::Result<Vec<u8>> {
        Self::check(key)?;
        self.drive()?.get(key).map_err(io_error)
    }

    /// Removes the object `key` (on an object store: DeleteObject). A missing one is no error.
    pub fn delete(&self, key: &str) -> std::io::Result<()> {
        Self::check(key)?;
        match self.drive()?.delete(key) {
            Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    }

    /// The size of the object `key`, or `None` when there is none.
    pub fn size_of(&self, key: &str) -> Option<u64> {
        Self::check(key).ok()?;
        self.drive().ok()?.head(key).ok().map(|info| info.size)
    }

    /// Every key under the folder `prefix` (`outbox`, `mail/inbox`; at any depth), in key order.
    /// A listing that failed is an empty one here; a caller that decides on what is NOT there (a
    /// sweep that deletes) asks [`MailStore::keys_checked`].
    pub fn keys(&self, prefix: &str) -> Vec<String> {
        // collapse-ok: for the callers that only look (a folder to move, one to show); a caller that
        // deletes by the list takes `keys_checked`.
        self.keys_checked(prefix).unwrap_or_default()
    }

    /// [`MailStore::keys`] where a listing that FAILED is an error, never an empty list: a folder
    /// that cannot be read is no folder with nothing in it.
    pub fn keys_checked(&self, prefix: &str) -> std::io::Result<Vec<String>> {
        let drive = self.drive()?;
        let folder = format!("{}/", prefix.trim_end_matches('/'));
        let mut keys: Vec<String> = azul_storage::ops::list_all(&drive, &folder)
            .map_err(io_error)?
            .into_iter()
            .map(|o| o.key)
            .collect();
        keys.sort();
        Ok(keys)
    }

    /// The names of the folders directly in the folder `prefix` (empty: this folder's own). A listing that
    /// failed is an empty one here; a caller that decides on what is NOT there asks
    /// [`MailStore::subfolders_checked`].
    pub fn subfolders(&self, prefix: &str) -> Vec<String> {
        // collapse-ok: for the callers that only look (the sidebar's folders, the folders a sync visits); a
        // caller that decides on what is not there takes `subfolders_checked` (X-DELTA-APPS5-6).
        self.subfolders_checked(prefix).unwrap_or_default()
    }

    /// [`MailStore::subfolders`] where a listing that FAILED is an error, never an empty list: a folder that
    /// cannot be read is no folder with nothing in it.
    pub fn subfolders_checked(&self, prefix: &str) -> std::io::Result<Vec<String>> {
        let drive = self.drive()?;
        let folder = match prefix.trim_end_matches('/') {
            "" => String::new(),
            p => format!("{p}/"),
        };
        let level = azul_storage::ops::list_folder_all(&drive, &folder).map_err(io_error)?;
        let mut names: Vec<String> = level
            .folders
            .iter()
            .filter_map(|f| {
                let name = f.strip_prefix(folder.as_str())?.trim_end_matches('/');
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// The newest `synced_at` of the folders' state files: what the clock of this computer said when the
    /// newest Send/Receive of the account ended, the floor that every later clock is set against
    /// ([`Clocks`]). `None`: no folder records one (never synced). A listing of `mail/` that FAILS, a state
    /// file that cannot be READ or that is there and is no state of this AzMail, and a `synced_at` that is
    /// no date are errors, never "no Send/Receive yet": a floor of nothing would date a hold by any clock
    /// (X-DELTA-APPS5-6). Only a folder without a state file (it is no synced folder) and an empty
    /// `synced_at` (a state of a Send/Receive that could record no clock) are no reading.
    ///
    /// # Errors
    ///
    /// The listing or a state file cannot be read, or says nothing this AzMail reads.
    pub fn last_sync(&self) -> std::io::Result<Option<u64>> {
        let mut newest: Option<u64> = None;
        for name in self.subfolders_checked(MAIL_PREFIX)? {
            let key = state_key(&name);
            // A name AzMail never writes is no folder of its own (`folders` skips it the same way).
            if !is_valid_key(&key) {
                continue;
            }
            let bytes = match self.get(&key) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            let unreadable = |what: &str| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{key} {what}"))
            };
            let state = String::from_utf8(bytes)
                .ok()
                .and_then(|text| FolderState::from_json(&text))
                .ok_or_else(|| unreadable("is no state this AzMail reads"))?;
            if state.synced_at.trim().is_empty() {
                continue;
            }
            let at = azul_storage::time::parse_iso8601(&state.synced_at)
                .ok_or_else(|| unreadable("has a `synced_at` that is no date"))?;
            newest = Some(newest.map_or(at, |was| was.max(at)));
        }
        Ok(newest)
    }

    /// Moves every object under `from` to the same place under `to` (on an object store: copy,
    /// then delete). A missing `from` moves nothing; when `to` is taken (moved aside before
    /// under the same UIDVALIDITY), the move goes next to it as `<to>-2`, `<to>-3`, ...
    pub fn move_prefix(&self, from: &str, to: &str) -> std::io::Result<()> {
        Self::check(from)?;
        Self::check(to)?;
        if self.keys(from).is_empty() {
            return Ok(());
        }
        // Taken: objects under it, or (a folder on disk) an empty folder of that name.
        let taken = |name: &str| !self.keys(name).is_empty() || self.folder.path().join(name).exists();
        let mut target = to.to_string();
        let mut n = 2;
        while taken(target.as_str()) {
            target = format!("{to}-{n}");
            n += 1;
        }
        self.drive()?
            .rename(&format!("{from}/"), &format!("{target}/"))
            .map_err(io_error)
    }

    /// Moves the object `from` to `to`: one rename on the disk that puts it in place only if
    /// nothing has the name (nothing is ever replaced), the folders on the way made. A missing
    /// `from` is [`std::io::ErrorKind::NotFound`]; a taken `to` is an error.
    pub fn move_object(&self, from: &str, to: &str) -> std::io::Result<()> {
        Self::check(from)?;
        Self::check(to)?;
        self.drive()?.rename(from, to).map_err(io_error)
    }

    /// Removes the folder `prefix` when nothing is in it but empty folders; `true` when it is gone
    /// (or was never there). Never a file.
    pub fn remove_empty_folder(&self, prefix: &str) -> std::io::Result<bool> {
        Self::check(prefix)?;
        self.drive()?
            .remove_empty_folder(&format!("{}/", prefix.trim_end_matches('/')))
            .map_err(io_error)
    }

    /// The synced folders: every `mail/<folder>` holding a state file, by name.
    pub fn folders(&self) -> Vec<String> {
        self.subfolders(MAIL_PREFIX)
            .into_iter()
            .filter(|name| self.size_of(&state_key(name)).is_some())
            .collect()
    }
}

/// One message in a folder's index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub uid: u32,
    #[serde(default)]
    pub message_id: String,
    /// When it was written (its Date header), else when the server received it; RFC 3339, UTC.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: String,
    /// The IMAP flags when it was synced (`\Seen`, `\Flagged`, ...).
    #[serde(default)]
    pub flags: Vec<String>,
    /// Bytes in the `.eml` file.
    #[serde(default)]
    pub size: u64,
    /// The `.eml` file's key (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`).
    pub path: String,
    /// An Azlin account's message: its object's key in the drive (`mail/Inbox/<name>.eml`,
    /// `AZLIN_MAIL.md`); empty for an IMAP account's, and for mail filed here that is not in the
    /// drive yet (the next Send/Receive puts it there).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote: String,
    /// An Azlin account's message: when this computer first saw its object in the drive - fetched
    /// it there, or put it there - in seconds since 1970 by this computer's clock. A restore of the
    /// drive to the moment `as_of` lost the mail first seen after it; mail first seen before it and
    /// absent from the restored drive was deleted before (X-DELTA-APPS3-13). `None`: a line from
    /// before the field, which a restore pass treats as it always did (the mail goes back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_drive_since: Option<i64>,
}

/// The index file: one JSON object per line, by UID, one line per UID (the later entry wins).
pub fn index_to_jsonl(entries: &[IndexEntry]) -> String {
    let mut by_uid: std::collections::BTreeMap<u32, &IndexEntry> =
        std::collections::BTreeMap::new();
    for entry in entries {
        by_uid.insert(entry.uid, entry);
    }
    let mut text = String::new();
    for entry in by_uid.values() {
        // Strings and numbers only: serializing cannot fail.
        if let Ok(line) = serde_json::to_string(entry) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    text
}

/// Reads an index file. Lines that are not an entry (a line cut short by a crash, an empty line)
/// are left out.
pub fn index_from_jsonl(text: &str) -> Vec<IndexEntry> {
    let mut by_uid: std::collections::BTreeMap<u32, IndexEntry> = std::collections::BTreeMap::new();
    for line in text.lines() {
        if let Ok(entry) = serde_json::from_str::<IndexEntry>(line.trim()) {
            by_uid.insert(entry.uid, entry);
        }
    }
    by_uid.into_values().collect()
}

/// What AzMail knows of a folder it synced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderState {
    pub format: String,
    pub version: u64,
    /// The server's name for the folder (modified UTF-7), for `SELECT`.
    pub server_name: String,
    /// The name the sidebar shows.
    #[serde(default)]
    pub display: String,
    /// The server's UIDVALIDITY: the UIDs below mean these messages only while it is unchanged.
    pub uidvalidity: u32,
    /// Every message up to this UID is in the index (or was gone when it was synced).
    pub last_uid: u32,
    /// Lines in the index.
    #[serde(default)]
    pub messages: u64,
    /// When the folder was last synced; RFC 3339, UTC.
    #[serde(default)]
    pub synced_at: String,
}

impl FolderState {
    pub fn create(server_name: &str, display: &str, uidvalidity: u32) -> FolderState {
        FolderState {
            format: STATE_FORMAT.to_string(),
            version: STATE_VERSION,
            server_name: server_name.to_string(),
            display: display.to_string(),
            uidvalidity,
            last_uid: 0,
            messages: 0,
            synced_at: String::new(),
        }
    }

    /// The state file's contents (pretty JSON, ending in a newline).
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads a state file; `None` for anything that is not one this AzMail reads.
    pub fn from_json(text: &str) -> Option<FolderState> {
        let state: FolderState = serde_json::from_str(text).ok()?;
        (state.format == STATE_FORMAT && (1..=STATE_VERSION).contains(&state.version))
            .then_some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn entry(uid: u32) -> IndexEntry {
        IndexEntry {
            uid,
            message_id: format!("<m{uid}@example.org>"),
            date: String::from("2026-09-30T08:42:00Z"),
            from: String::from("Ada <ada@example.org>"),
            to: String::from("ben@example.org"),
            subject: format!("Message {uid}"),
            flags: vec![String::from("\\Seen")],
            size: 1234,
            path: message_key("inbox", 2026, 9, uid),
            remote: String::new(),
            on_drive_since: None,
        }
    }

    /// The data tree's manifest (`<root>/.azlin/cache`), as text.
    fn manifest_of(root: &Path) -> String {
        std::fs::read_to_string(
            root.join(azul_storage::manifest::MANIFEST_DIR)
                .join(azul_storage::manifest::CACHE_FILE),
        )
        .unwrap_or_default()
    }

    #[test]
    fn an_accounts_folder_in_the_data_tree_is_a_folder_of_the_trees_one_drive() {
        let dir = TempDir::new("store-tree");
        let account = dir.0.join("mail").join("ada@example.org");
        let folder = DriveFolder::of(&account, &dir.0);
        assert!(folder.is_data_tree());
        assert_eq!(folder.path(), account);
        assert_eq!(folder.child("outbox").path(), account.join("outbox"));
        let store = MailStore::new(folder);
        let key = message_key("inbox", 2026, 9, 1);
        store.put(&key, b"From: a\r\n\r\nhi\r\n").unwrap();
        assert!(account.join("mail/inbox/2026/09/1.eml").is_file());
        // ONE manifest, the data tree's: the write is recorded under the account's prefix,
        // and the account's folder holds no second one.
        let manifest = manifest_of(&dir.0);
        assert!(
            manifest.contains("mail/ada%40example.org/mail/inbox/2026/09/1.eml")
                || manifest.contains("mail/ada@example.org/mail/inbox/2026/09/1.eml"),
            "the data tree's manifest does not record the message: {manifest}"
        );
        assert!(!account.join(azul_storage::manifest::MANIFEST_DIR).exists());
    }

    #[test]
    fn a_folder_outside_the_data_tree_is_a_drive_of_its_own_without_a_manifest() {
        let tree = TempDir::new("store-tree");
        let elsewhere = TempDir::new("store-elsewhere");
        let folder = DriveFolder::of(&elsewhere.0, &tree.0);
        assert!(!folder.is_data_tree());
        assert_eq!(folder.path(), elsewhere.0);
        MailStore::new(folder)
            .put(&state_key("inbox"), b"{}")
            .unwrap();
        assert!(elsewhere.0.join("mail/inbox/state.json").is_file());
        assert!(!elsewhere.0.join(azul_storage::manifest::MANIFEST_DIR).exists());
        assert!(!tree.0.join(azul_storage::manifest::MANIFEST_DIR).exists());
        // Seen from the data tree's AzMail folder, a path is placed the same way.
        let azmail = DriveFolder::of(&tree.0.join("mail"), &tree.0);
        assert!(azmail.locate(&tree.0.join("mail").join("x")).is_data_tree());
        assert!(!azmail.locate(&elsewhere.0).is_data_tree());
    }

    #[test]
    fn keys_follow_the_object_store_layout() {
        assert_eq!(
            message_key("inbox", 2026, 9, 42),
            "mail/inbox/2026/09/42.eml"
        );
        assert_eq!(message_key("spam", 1999, 12, 7), "mail/spam/1999/12/7.eml");
        assert_eq!(message_key("inbox", 0, 0, 1), "mail/inbox/0000/00/1.eml");
        assert_eq!(folder_prefix("Work.Projects"), "mail/Work.Projects");
        assert_eq!(index_key("inbox"), "mail/inbox/index.jsonl");
        assert_eq!(state_key("inbox"), "mail/inbox/state.json");
        assert_eq!(stale_prefix("inbox", 7), "stale/inbox/7");
    }

    #[test]
    fn only_relative_paths_inside_the_folder_are_keys() {
        for good in ["mail/inbox/2026/09/1.eml", "a", "mail/Entwürfe/index.jsonl"] {
            assert!(is_valid_key(good), "{good}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "mail//x",
            "mail/../x",
            "./x",
            "mail/x/",
            "a\\b",
            "a\0b",
            "..",
        ] {
            assert!(!is_valid_key(bad), "{bad:?}");
        }
        let store = MailStore::new(DriveFolder::outside(PathBuf::from("/data")));
        assert!(store.get("../x").is_err());
        assert_eq!(
            DriveFolder::of(Path::new("/data/mail/ada"), Path::new("/data"))
                .child("inbox")
                .path(),
            Path::new("/data").join("mail").join("ada").join("inbox")
        );
    }

    #[test]
    fn a_folder_lists_its_keys_and_its_subfolders() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        store.put("outbox/b.json", b"{}").unwrap();
        store.put("outbox/a.json", b"{}").unwrap();
        store.put("outbox/a.eml", b"x").unwrap();
        store.put("ada/account.json", b"{}").unwrap();
        assert_eq!(
            store.keys("outbox"),
            vec!["outbox/a.eml", "outbox/a.json", "outbox/b.json"]
        );
        assert!(store.keys("nothing").is_empty());
        assert_eq!(store.subfolders(""), vec!["ada", "outbox"]);
        store.delete("outbox/a.eml").unwrap();
        store.delete("outbox/a.eml").unwrap();
        assert_eq!(store.size_of("outbox/a.eml"), None);
    }

    #[test]
    fn an_object_is_written_whole_and_leaves_no_temporary_file() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        let key = message_key("inbox", 2026, 9, 1);
        assert_eq!(store.size_of(&key), None);
        store.put(&key, b"From: a\r\n\r\nhi\r\n").unwrap();
        assert_eq!(store.get(&key).unwrap(), b"From: a\r\n\r\nhi\r\n");
        assert_eq!(store.size_of(&key), Some(15));
        store.put(&key, b"again").unwrap();
        assert_eq!(store.get(&key).unwrap(), b"again");
        let names: Vec<String> = std::fs::read_dir(dir.0.join("mail/inbox/2026/09"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![String::from("1.eml")]);
        assert!(store.put("../escape", b"x").is_err());
        assert!(!dir.0.parent().unwrap().join("escape").exists());
    }

    #[test]
    fn a_folder_moves_aside_whole() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        store
            .put(&message_key("inbox", 2026, 9, 1), b"one")
            .unwrap();
        store.put(&index_key("inbox"), b"{}\n").unwrap();
        store
            .move_prefix(&folder_prefix("inbox"), &stale_prefix("inbox", 7))
            .unwrap();
        assert_eq!(store.size_of(&index_key("inbox")), None);
        assert_eq!(store.get("stale/inbox/7/index.jsonl").unwrap(), b"{}\n");
        assert_eq!(store.get("stale/inbox/7/2026/09/1.eml").unwrap(), b"one");
        // Nothing to move is not an error.
        store
            .move_prefix(&folder_prefix("nothing"), &stale_prefix("nothing", 1))
            .unwrap();
        // Moved aside again under the same UIDVALIDITY: next to the first.
        store.put(&index_key("inbox"), b"again\n").unwrap();
        store
            .move_prefix(&folder_prefix("inbox"), &stale_prefix("inbox", 7))
            .unwrap();
        assert_eq!(store.get("stale/inbox/7/index.jsonl").unwrap(), b"{}\n");
        assert_eq!(store.get("stale/inbox/7-2/index.jsonl").unwrap(), b"again\n");
    }

    #[test]
    fn the_synced_folders_are_those_with_a_state_file() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        assert!(store.folders().is_empty());
        let state = FolderState::create("INBOX", "Inbox", 1);
        store
            .put(&state_key("spam"), state.to_json().as_bytes())
            .unwrap();
        store
            .put(&state_key("inbox"), state.to_json().as_bytes())
            .unwrap();
        store
            .put("mail/half-done/2026/09/1.eml", b"x")
            .unwrap();
        assert_eq!(
            store.folders(),
            vec![String::from("inbox"), String::from("spam")]
        );
    }

    #[test]
    fn the_index_is_one_line_per_uid_in_uid_order() {
        let text = index_to_jsonl(&[entry(3), entry(1), entry(2)]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(text.ends_with('\n'));
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["uid"], 1);
        assert_eq!(first["message_id"], "<m1@example.org>");
        assert_eq!(first["date"], "2026-09-30T08:42:00Z");
        assert_eq!(first["from"], "Ada <ada@example.org>");
        assert_eq!(first["to"], "ben@example.org");
        assert_eq!(first["subject"], "Message 1");
        assert_eq!(first["flags"][0], "\\Seen");
        assert_eq!(first["size"], 1234);
        assert_eq!(first["path"], "mail/inbox/2026/09/1.eml");
        assert_eq!(index_from_jsonl(&text), vec![entry(1), entry(2), entry(3)]);
        let changed = IndexEntry {
            subject: String::from("again"),
            ..entry(2)
        };
        let deduped = index_to_jsonl(&[entry(2), changed.clone()]);
        assert_eq!(index_from_jsonl(&deduped), vec![changed]);
        assert_eq!(index_to_jsonl(&[]), "");
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_left_out() {
        let mut text = index_to_jsonl(&[entry(1), entry(2)]);
        text.push_str("{\"uid\": 3, \"subj");
        text.push_str("\n\n");
        assert_eq!(index_from_jsonl(&text), vec![entry(1), entry(2)]);
    }

    #[test]
    fn a_state_file_round_trips_and_a_newer_one_is_not_read() {
        let mut state = FolderState::create("[Gmail]/Spam", "Spam", 600_123);
        state.last_uid = 42;
        state.messages = 40;
        state.synced_at = String::from("2026-09-30T08:42:00Z");
        let text = state.to_json();
        assert!(text.ends_with('\n'));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["format"], "azmail.folder");
        assert_eq!(json["version"], 1);
        assert_eq!(json["uidvalidity"], 600_123);
        assert_eq!(json["last_uid"], 42);
        assert_eq!(FolderState::from_json(&text), Some(state.clone()));
        assert_eq!(
            FolderState::from_json(&text.replace("\"version\": 1", "\"version\": 2")),
            None
        );
        assert_eq!(FolderState::from_json("{}"), None);
        assert_eq!(FolderState::from_json("not json"), None);
    }

    // ==== X-DELTA-APPS5-2: the one constructor of a hold's date ====

    /// 2026-09-30T08:42:00Z
    const SEP_30: u64 = 1_790_757_720;
    const DAY: u64 = 86_400;

    /// The clock of this computer set against what an account remembers of the time: the later of the clock
    /// and the newest Send/Receive; refused when it is not set, more than a day behind the last Send/Receive,
    /// or more than 366 days ahead of the later of the last Send/Receive and the newest mail of the drive - and
    /// accepted, to the second, at those two edges (the table that APPS5 read from the code, as a test).
    #[test]
    fn x_delta_apps5_2_a_hold_is_dated_by_the_clock_set_against_what_the_account_remembers() {
        let known = Clocks::new(Some(SEP_30), Some(SEP_30 + 60));
        let at = |now: u64| known.hold_time(i64::try_from(now).unwrap());
        // A right clock dates the hold; a clock a few hours behind dates it at the last Send/Receive.
        assert_eq!(at(SEP_30 + 5 * 3600).unwrap().secs(), SEP_30 + 5 * 3600);
        assert_eq!(at(SEP_30 - 3 * 3600).unwrap().secs(), SEP_30);
        // The edge behind: exactly a day is accepted, one second more is refused.
        assert_eq!(at(SEP_30 - DAY).unwrap().secs(), SEP_30);
        assert_eq!(
            at(SEP_30 - DAY - 1),
            Err(ClockRefusal::Behind {
                now: SEP_30 - DAY - 1,
                last: SEP_30
            })
        );
        // The edge ahead is measured from the later of the last Send/Receive and the drive's newest mail.
        let top = SEP_30 + 60;
        assert!(at(top + 366 * DAY).is_ok());
        assert_eq!(
            at(top + 366 * DAY + 1),
            Err(ClockRefusal::Ahead {
                now: top + 366 * DAY + 1,
                known: top,
                drive: true
            })
        );
        // A clock that is not set dates nothing.
        for now in [0, -1, i64::MIN] {
            assert_eq!(known.hold_time(now), Err(ClockRefusal::NotSet), "{now}");
        }
        // No Send/Receive yet: only the drive can be set against (the ahead rule); no reading at all: any set
        // clock; no drive (Delete in the Outbox has none): the last Send/Receive alone.
        let first = Clocks::new(None, Some(SEP_30));
        assert!(first
            .hold_time(i64::try_from(SEP_30 - 10 * DAY).unwrap())
            .is_ok());
        assert!(matches!(
            first.hold_time(i64::try_from(SEP_30 + 10 * 366 * DAY).unwrap()),
            Err(ClockRefusal::Ahead { drive: true, .. })
        ));
        assert!(Clocks::new(None, None).hold_time(1).is_ok());
        let outbox = Clocks::new(Some(SEP_30), None);
        let ahead = outbox.hold_time(i64::try_from(SEP_30 + 400 * DAY).unwrap());
        assert!(
            matches!(ahead, Err(ClockRefusal::Ahead { drive: false, .. })),
            "{ahead:?}"
        );
        // The words the window and the Outbox say: the refusal names what it was set against.
        let said = ahead.unwrap_err().to_string();
        assert!(
            said.contains("more than a year after the last Send/Receive (2026-09-30T08:42:00Z)")
                && !said.contains("newest mail of the drive"),
            "{said}"
        );
        let said = at(SEP_30 - DAY - 1).unwrap_err().to_string();
        assert!(
            said.starts_with("the clock of this computer says 2026-09-29T08:41:59Z, more than a day before the last Send/Receive (2026-09-30T08:42:00Z)"),
            "{said}"
        );
        assert_eq!(
            Clocks::default().hold_time(0).unwrap_err().to_string(),
            "the clock of this computer is not set"
        );
    }

    /// X-DELTA-APPS5-6: a floor that cannot be read is an arm of its own. It dates no hold (whatever the clock
    /// says), records no stamp, gives the sweep no reading - and the same account with a floor that reads fine does
    /// all three; a floor read from a store that is not there is a floor of nothing, as it always was.
    #[test]
    fn x_delta_apps5_6_a_floor_that_cannot_be_read_dates_no_hold_records_nothing_and_sweeps_nothing(
    ) {
        let dir = TempDir::new("store-clocks-floor");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        let newest = SEP_30 + 60;
        let now = i64::try_from(SEP_30 + DAY).unwrap();
        // Never synced: a floor of nothing - and the drive's reading goes to the sweep.
        let fresh = Clocks::of(&store, Some(newest));
        assert_eq!(fresh, Clocks::new(None, Some(newest)));
        assert!(fresh.hold_time(now).is_ok());
        assert_eq!(fresh.sweep_reading(), Some(newest));
        // A state that cannot be read: the clock is refused whatever it says (also a year back), nothing is
        // recorded and the sweep has no reading.
        store.put(&state_key("inbox"), b"{ not a state").unwrap();
        let damaged = Clocks::of(&store, Some(newest));
        for at in [now, now - 365 * i64::try_from(DAY).unwrap()] {
            let refused = damaged.hold_time(at).unwrap_err();
            assert!(
                matches!(&refused, ClockRefusal::FloorUnreadable(why) if why.contains("mail/inbox/state.json")),
                "{refused:?}"
            );
            assert!(refused
                .to_string()
                .starts_with("the time of the last Send/Receive cannot be read (mail/inbox/state.json is no state"));
        }
        assert_eq!(damaged.recorded(now), None);
        assert_eq!(damaged.sweep_reading(), None);
        assert_eq!(damaged.last_sync(), None);
        assert_eq!(
            damaged.drive(),
            Some(newest),
            "the drive's reading is still the drive's"
        );
        // The same account once its state reads: all three work again.
        put_state(&store, "inbox", "2026-09-30T08:42:00Z");
        let read = Clocks::of(&store, Some(newest));
        assert_eq!(read, Clocks::new(Some(SEP_30), Some(newest)));
        assert!(read.hold_time(now).is_ok());
        assert_eq!(read.recorded(now), Some(SEP_30 + DAY));
        assert_eq!(read.sweep_reading(), Some(newest));
    }

    /// The folder a hold is made in is the stamp of the moment it is dated at, in the form the sweep reads.
    #[test]
    fn x_delta_apps5_2_a_hold_dated_at_a_moment_is_the_folder_of_its_stamp() {
        let at = Clocks::new(None, None)
            .hold_time(i64::try_from(SEP_30).unwrap())
            .unwrap();
        assert_eq!(at.prefix("outbox"), "removed/outbox/20260930T084200Z");
        assert_eq!(
            at.prefix("inbox"),
            removed_prefix("inbox", "20260930T084200Z")
        );
    }

    /// A state file as Send/Receive writes it, with `synced_at` as given.
    fn put_state(store: &MailStore, folder: &str, synced_at: &str) {
        let mut state = FolderState::create("INBOX", "Inbox", 1);
        state.synced_at = String::from(synced_at);
        store
            .put(&state_key(folder), state.to_json().as_bytes())
            .unwrap();
    }

    /// X-DELTA-APPS5-2 / X-DELTA-APPS5-6: the account's record of the time is the newest `synced_at` of its
    /// folders; only a folder with no state file and an empty `synced_at` are "no reading" - a state that is
    /// there and says nothing this AzMail reads, and a `synced_at` that is no date, are errors that name the
    /// file, never "no Send/Receive yet".
    #[test]
    fn x_delta_apps5_6_the_last_send_receive_is_the_newest_stamp_and_an_unreadable_one_is_an_error()
    {
        let dir = TempDir::new("store-last-sync");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        assert_eq!(store.last_sync().unwrap(), None, "no mail folder yet");
        // A folder with no state file (half made) and a state with no stamp are no reading.
        store.put("mail/half-done/2026/09/1.eml", b"x").unwrap();
        put_state(&store, "spam", "");
        assert_eq!(store.last_sync().unwrap(), None);
        put_state(&store, "inbox", "2026-09-30T08:42:00Z");
        put_state(&store, "archive", "2026-09-29T00:00:00Z");
        assert_eq!(store.last_sync().unwrap(), Some(SEP_30));
        // A state that is not a state of this AzMail: an error naming the file.
        store
            .put(&state_key("work"), b"{ this is not a state")
            .unwrap();
        let damaged = store.last_sync().unwrap_err();
        assert_eq!(damaged.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            damaged
                .to_string()
                .contains("mail/work/state.json is no state this AzMail reads"),
            "{damaged}"
        );
        put_state(&store, "work", "yesterday");
        let undated = store.last_sync().unwrap_err();
        assert!(
            undated
                .to_string()
                .contains("mail/work/state.json has a `synced_at` that is no date"),
            "{undated}"
        );
        // A newer AzMail's state is no state of this one either: not read, not guessed.
        store
            .put(
                &state_key("work"),
                br#"{"format": "azmail.folder", "version": 2, "server_name": "Work", "uidvalidity": 1, "last_uid": 0, "synced_at": "2026-10-30T08:42:00Z"}"#,
            )
            .unwrap();
        assert!(store.last_sync().is_err());
        put_state(&store, "work", "2026-10-01T00:00:00Z");
        assert_eq!(store.last_sync().unwrap(), Some(1_790_812_800));
    }

    /// X-DELTA-APPS5-6: a listing of `mail/` that FAILS is an error too - the folders it would have named are
    /// not "none". A folder that cannot be listed but whose files can be read (mode 111) is the fault; the
    /// lenient `subfolders` still answers an empty list for the callers that only look.
    #[cfg(unix)]
    #[test]
    fn x_delta_apps5_6_a_listing_of_mail_that_fails_is_an_error_not_no_send_receive_yet() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("store-last-sync-listing");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        put_state(&store, "inbox", "2026-09-30T08:42:00Z");
        assert_eq!(store.last_sync().unwrap(), Some(SEP_30));
        let mail = dir.0.join(MAIL_PREFIX);
        std::fs::set_permissions(&mail, std::fs::Permissions::from_mode(0o111)).unwrap();
        let premise = std::fs::read_dir(&mail).is_err();
        let failed = store.last_sync();
        let lenient = store.subfolders(MAIL_PREFIX);
        let checked = store.subfolders_checked(MAIL_PREFIX);
        std::fs::set_permissions(&mail, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            premise,
            "the premise of the test: `mail/` cannot be listed (running as root?)"
        );
        assert!(failed.is_err(), "{failed:?}");
        assert!(checked.is_err(), "{checked:?}");
        assert!(lenient.is_empty(), "the callers that only look get none");
        assert_eq!(store.last_sync().unwrap(), Some(SEP_30));
    }
}
