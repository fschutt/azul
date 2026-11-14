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
    collections::BTreeMap,
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

use serde::{Deserialize, Serialize};

use crate::{
    beneath::{BeneathError, Dir, Kind, Stat},
    durable::Expect,
    DriveError,
};

/// Where scratch lives, set once by a program that has a cache folder (`<cache>/scratch`).
static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The front of a kept folder's name.
const KEPT: &str = "k-";

/// The record a kept folder holds of the files its owner made in it ([`Scratch::note_made`]).
const MADE_RECORD: &str = ".made.json";

/// Entries one sweep looks at: a folder somebody filled with millions is not walked whole at
/// the start of a program.
const SWEEP_LOOKS: usize = 4096;

/// What the sweep a purpose's first [`Scratch::file`] runs removes by age: a file of a process
/// that may still run is this old before it goes (a file of a process that is gone always goes).
pub const HORIZON: Duration = Duration::from_secs(3 * 24 * 3600);

/// How long a copy handed to an app ([`Scratch::open_copy`]) is kept before a sweep removes it:
/// "swept after use" for a copy nobody can tell the end of use of. A folder that holds the user's
/// own work stays whatever its age says.
pub const OPEN_COPY_KEPT: Duration = Duration::from_secs(24 * 3600);

/// The purpose of the copies handed to apps ([`Scratch::open_copies_of`]).
const OPEN_PURPOSE: &str = "open";

/// How many `name (edited N)` files one copy's folder keeps before it refuses to put another aside.
const ASIDE_MAX: u32 = 999;

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

/// The system's shared temp folder. The only place the shipped code of the storage crates asks for
/// it.
fn shared_temp() -> PathBuf {
    std::env::temp_dir()
}

/// A path every system call refuses (it holds a NUL byte): where a caller that cannot have its
/// private folder points what it would have written there, so that nothing is written into a
/// folder another user made first (X-DELTA-AZDRIVE-FILES-29).
#[must_use]
pub fn refused_dir() -> PathBuf {
    PathBuf::from("\0scratch-refused")
}

/// What became of a folder an older program left decrypted copies in ([`remove_legacy_open_copies`],
/// [`remove_legacy_copies_at`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyCopies {
    /// There was no such folder of this user's (or the name is a link, or another user's folder: not
    /// ours to touch).
    Absent,
    /// It held no file, only folders: removed.
    Removed,
    /// It holds `files` entries that are no folder - copies an app was handed writable, and maybe what the
    /// user changed in them, which nothing synced and nothing can tell from the copy (a folder of that time
    /// kept no record of what was written) - and they stay where they are, the folder made private. The
    /// caller says so (X-DELTA-SYNC4-25: the folders were removed whole, with whatever the user had saved
    /// in them; a hidden copy that is rarely edited is no reason to delete). `files` is 0 for a folder
    /// with no file that could not be removed.
    Kept { files: usize },
}

/// The folder of the decrypted copies an older program left in a sync pairing's state folder: `<state>/open`. The one
/// folder of a pairing's state that holds the user's own files and not the program's bookkeeping (a copy an app was
/// handed writable, maybe edited and saved): [`remove_legacy_copies_at`] deals with it - the kit's open, AzDrive's start
/// - and "Remove drive" keeps it when it holds a file (X-DELTA-SYNC9-1). The one spelling of its name.
pub const LEGACY_STATE_OPEN: &str = "open";

/// The shared temp folder's `AzDrive-open`, where [`remove_legacy_open_copies`] looks: for a notice that names it.
#[must_use]
pub fn legacy_open_folder() -> PathBuf {
    shared_temp().join(LEGACY_OPEN)
}

/// Deals with `AzDrive-open` in the system's shared temp folder: where an AzDrive before scratch put the
/// decrypted copy of every file it opened (`AzDrive-open/<drive>/<name>`, folders 0755 and files 0644 -
/// readable by every user of the computer), and where nothing ever removed them (AZ-W4). Only a folder this
/// user owns and that is no link is touched; one another user made is that user's. A folder with no file in
/// it goes; one that holds files is made private and kept ([`LegacyCopies`]).
pub fn remove_legacy_open_copies() -> LegacyCopies {
    remove_legacy_open_copies_in(&shared_temp())
}

/// [`remove_legacy_open_copies`] in the shared folder `temp`.
pub(crate) fn remove_legacy_open_copies_in(temp: &Path) -> LegacyCopies {
    remove_legacy_copies_at(&temp.join(LEGACY_OPEN))
}

/// The folder `folder` an older program left decrypted copies in: removed when it holds no file, else kept
/// and made private ([`LegacyCopies`]). A listing or a look that cannot be done counts as a file: what
/// cannot be seen is not removed. Only a folder this user owns and that is no link is touched.
pub fn remove_legacy_copies_at(folder: &Path) -> LegacyCopies {
    let ours = fs::symlink_metadata(folder)
        .is_ok_and(|meta| meta.file_type().is_dir() && owned_by_me(&meta));
    if !ours {
        return LegacyCopies::Absent;
    }
    let mut looks = SWEEP_LOOKS;
    let files = entries_that_are_no_folder(folder, 0, &mut looks);
    if files > 0 {
        close_to_other_users(folder);
        return LegacyCopies::Kept { files };
    }
    let (Some(parent), Some(name)) = (folder.parent(), folder.file_name().and_then(|n| n.to_str()))
    else {
        return LegacyCopies::Kept { files: 0 };
    };
    let Ok(dir) = Dir::open_following(parent) else {
        return LegacyCopies::Kept { files: 0 };
    };
    match remove_tree(&dir, name, 0) {
        Ok(()) => LegacyCopies::Removed,
        Err(_) => LegacyCopies::Kept { files: 0 },
    }
}

/// How many entries below `folder` are no folder (a file, a link, anything else), counting a listing or a
/// look that fails as one; the folders themselves are walked, never a link. Bounded by `looks`: past it the
/// count is at least one more, so a folder nobody can walk whole is not removed.
fn entries_that_are_no_folder(folder: &Path, depth: usize, looks: &mut usize) -> usize {
    let Ok(listing) = fs::read_dir(folder) else {
        return 1;
    };
    let mut files = 0;
    for entry in listing {
        if *looks == 0 || depth > 16 {
            return files + 1;
        }
        *looks -= 1;
        let Ok(entry) = entry else {
            files += 1;
            continue;
        };
        match fs::symlink_metadata(entry.path()) {
            Ok(meta) if meta.file_type().is_dir() => {
                files += entries_that_are_no_folder(&entry.path(), depth + 1, looks);
            }
            _ => files += 1,
        }
    }
    files
}

/// Closes `folder` to every other user (`rwx------`): the decrypted copies that stay are the user's own.
fn close_to_other_users(folder: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // collapse-ok: a folder that cannot be closed is as it was; nothing is removed and the caller says
        // that the copies stay.
        let _ = fs::set_permissions(folder, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    let _ = folder;
}

/// The folder of the copies an AzDrive before scratch opened.
const LEGACY_OPEN: &str = "AzDrive-open";

/// How many names the fallback root tries before it gives up on a folder of its own.
const FALLBACK_NAMES: usize = 8;

/// The folder of a program that has no cache folder: one per user in the system's temp folder.
fn fallback_root() -> PathBuf {
    fallback_root_in(&shared_temp())
}

/// [`fallback_root`] in the shared folder `temp`: `azul-scratch-<uid>`, a name every user can
/// guess. When somebody else made it first - a folder of another user, a link, a file - the next
/// name (`azul-scratch-<uid>-1`, ...) is tried, so the scratch of this user is never inside a
/// place another user made (X-DELTA-AZDRIVE-FILES-29). Only a name that is free or a folder of
/// ours is taken; if every name is spoken for, the first is returned and [`Scratch::ensure`]
/// refuses it.
pub(crate) fn fallback_root_in(temp: &Path) -> PathBuf {
    let base = format!("azul-scratch-{}", user_tag());
    for n in 0..FALLBACK_NAMES {
        let candidate = if n == 0 {
            temp.join(&base)
        } else {
            temp.join(format!("{base}-{n}"))
        };
        if free_or_ours(&candidate) {
            return candidate;
        }
    }
    temp.join(base)
}

/// Nothing is at `path`, or a folder (not a link) of this user's.
fn free_or_ours(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Err(e) => e.kind() == io::ErrorKind::NotFound,
        Ok(meta) => meta.file_type().is_dir() && owned_by_me(&meta),
    }
}

/// Whether `meta` is of an entry this user owns (a system with no owners: the temp folder is the
/// user's own).
fn owned_by_me(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: `geteuid` takes no argument and cannot fail.
        meta.uid() == unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        true
    }
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

    /// The scratch of `purpose` that belongs to `tag` (a drive's id): a purpose folder of its own,
    /// `<purpose>-<16 hex digits of the tag's hash>`, so that what a removed drive left there is
    /// one folder to look at, and a sweep treats it like any other purpose's.
    #[must_use]
    pub fn tagged(purpose: &str, tag: &str) -> Scratch {
        Scratch::tagged_under(&root(), purpose, tag)
    }

    /// [`Scratch::tagged`] with its own `root` (a test's folder).
    #[must_use]
    pub fn tagged_under(root: &Path, purpose: &str, tag: &str) -> Scratch {
        let hash = blake3::hash(tag.as_bytes()).to_hex();
        Scratch::under(root, &format!("{purpose}-{}", &hash.as_str()[..16]))
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

    /// The purpose's folder, made and checked ([`Scratch::ensure`]): the path for a caller that
    /// hands it on and writes in it itself (a vault, a node list, a page for the PDF reader). A
    /// path taken without this call may be a folder another user made first.
    ///
    /// # Errors
    ///
    /// As [`Scratch::ensure`].
    pub fn checked_dir(&self) -> io::Result<PathBuf> {
        self.ensure()?;
        Ok(self.dir.clone())
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

    /// The scratch of the copies of the drive `drive` handed to apps ([`Scratch::open_copy`]): a
    /// purpose folder of its own per drive (`open-<hash of its id>`), so that a removed drive takes
    /// its own with it and a sweep treats each like any purpose (X-DELTA-AZDRIVE-KEYS-32). The one
    /// owner of the word: AzDrive's own Open and the sync's encrypted copies both ask here.
    #[must_use]
    pub fn open_copies_of(drive: &str) -> Scratch {
        Scratch::tagged(OPEN_PURPOSE, drive)
    }

    /// [`Scratch::open_copies_of`] with its own `root` (a test's folder).
    #[must_use]
    pub fn open_copies_under(root: &Path, drive: &str) -> Scratch {
        Scratch::tagged_under(root, OPEN_PURPOSE, drive)
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

    /// Notes that the file `name` of the kept folder `folder` is exactly what the program wrote:
    /// its size, time and identity, taken NOW (call it after the last thing done to the file: its
    /// mark, its mode). From then on [`Scratch::made_state`] tells whether it is still that file,
    /// and a sweep takes the folder away only while it holds nothing but the files noted here -
    /// an edit saved over the copy or beside it is the user's work, not scratch.
    ///
    /// # Errors
    ///
    /// When the file cannot be looked at or the record cannot be written.
    pub fn note_made(folder: &Path, name: &str) -> io::Result<()> {
        let dir = Dir::open_following(folder)?;
        let st = dir.lstat(name)?;
        let mut record = read_record(folder)?.unwrap_or_default();
        record.files.insert(name.to_string(), MadeFile::of(&st));
        let text = serde_json::to_vec(&record).map_err(io::Error::other)?;
        crate::durable::replace_cache(&folder.join(MADE_RECORD), &text)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// Whether the file `name` of the kept folder `folder` is still what [`Scratch::note_made`]
    /// noted. A record that cannot be read, or none, says [`MadeState::Unknown`]: nothing is
    /// known, and the caller takes the careful side.
    #[must_use]
    pub fn made_state(folder: &Path, name: &str) -> MadeState {
        let Ok(Some(record)) = read_record(folder) else {
            return MadeState::Unknown;
        };
        let Some(made) = record.files.get(name) else {
            return MadeState::Unknown;
        };
        let Ok(dir) = Dir::open_following(folder) else {
            return MadeState::Unknown;
        };
        match dir.lstat(name) {
            Ok(st) if made.is(&st) => MadeState::Untouched,
            Ok(_) => MadeState::Changed,
            Err(e) if e.kind() == io::ErrorKind::NotFound => MadeState::Changed,
            Err(_) => MadeState::Unknown,
        }
    }

    /// The copy of a drive's file that an app is handed (a viewer, an editor, the system's "open"):
    /// THE procedure, for every program that hands one out (ARCH29 C6a; AZ-W4, AZ-U13, AZ-U14,
    /// X-DELTA-AZDRIVE-FILES-7, X-DELTA-AZDRIVE-SYNC-2). It had been written twice, and the second
    /// time without what the first had learned (an edit saved beside the copy was swept after a
    /// few days, the next Open wrote the original over an edit saved over it).
    ///
    /// - a sweep first (a copy older than [`OPEN_COPY_KEPT`] and not holding the user's work goes);
    /// - ONE folder per file AND version ([`CopyOf`]): the same version opened again is the same
    ///   copy, another version another folder, and the first stays as it was;
    /// - a copy that is there and is what was written (its record, [`Scratch::made_state`], or
    ///   when none is left its size) is handed out again as it is, without a fetch;
    /// - whatever else stands at the copy's place - an editor's save renamed over the read-only
    ///   copy, a "Save As" in its folder, a shortened or same-sized file - is the user's work: put
    ///   aside as `name (edited).ext`, never deleted, and said in [`OpenedCopy::kept_edit`];
    /// - `fetch` is asked for the bytes ONLY into a free place ([`Expect::Absent`]): it must fail
    ///   with [`DriveError::Conflict`] when a file turned up there since the look (what
    ///   [`crate::durable::publish`] does with that expectation). A file made by another window in
    ///   the same moment is the same copy; a stranger's is put aside too, once;
    /// - the file is marked as downloaded from elsewhere ([`crate::quarantine::mark_downloaded`]),
    ///   read-only ([`make_read_only`]: a save in place fails loudly) and, last, noted as made
    ///   ([`Scratch::note_made`]: from then on a sweep keeps the folder while it holds anything
    ///   else). A copy that could not be noted is not handed out - an edit of it would be swept.
    ///
    /// # Errors
    ///
    /// [`DriveError::InvalidKey`] when `of.name` is not one usable file name; what `fetch` fails
    /// with (after the one retry above); [`DriveError::Io`] when the folder cannot be made, the
    /// user's file cannot be put aside, or the copy cannot be noted.
    pub fn open_copy(
        &self,
        of: CopyOf<'_>,
        fetch: &mut dyn FnMut(&Path, Expect) -> Result<(), DriveError>,
    ) -> Result<OpenedCopy, DriveError> {
        if !is_copy_name(of.name) {
            return Err(DriveError::InvalidKey {
                key: of.key.to_string(),
                reason: "it has no usable file name",
            });
        }
        // What a run before left, and old copies of this one, go before a new copy comes.
        self.sweep(OPEN_COPY_KEPT);
        let tag = match of.size {
            Some(size) => format!("{}\0{}\0{size}", of.key, of.version),
            None => format!("{}\0{}\0", of.key, of.version),
        };
        let folder = self
            .keep(&tag)
            .map_err(|e| DriveError::Io(format!("{}: {e}", of.key)))?;
        let dest = folder.join(of.name);
        if copy_is_usable(&folder, of.name, of.size) {
            return Ok(OpenedCopy {
                path: dest,
                kept_edit: None,
            });
        }
        // Something that is not the bytes written stands under the name (a copy somebody saved
        // over, shortened or edited): it is put aside, once, and the copy is fetched.
        let mut kept_edit = None;
        if fs::symlink_metadata(&dest).is_ok() {
            kept_edit = put_aside(&folder, of.name)?;
        }
        let mut again = false;
        loop {
            match fetch(&dest, Expect::Absent) {
                Ok(()) => break,
                // Another window made the same copy in the same moment.
                Err(DriveError::Conflict { .. }) if copy_is_usable(&folder, of.name, of.size) => {
                    return Ok(OpenedCopy {
                        path: dest,
                        kept_edit,
                    });
                }
                // A file turned up under the name since the look: it is somebody's, put aside too.
                Err(DriveError::Conflict { .. }) if !again => {
                    again = true;
                    kept_edit = put_aside(&folder, of.name)?.or(kept_edit);
                }
                Err(e) => return Err(e),
            }
        }
        // The system looks at a file from a drive before it runs it (AZ-W20).
        crate::quarantine::mark_downloaded(&dest);
        // collapse-ok: a copy that stays writable is still private to this user, and the record
        // below keeps an edit of it from being swept; what a refusal loses is only the loud
        // failure of a save in place.
        let _ = make_read_only(&dest);
        // Noted last: the mode and the mark are part of the file as it is now.
        Scratch::note_made(&folder, of.name).map_err(|e| {
            DriveError::Io(format!(
                "{}: the copy cannot be noted as made, so an edit of it could not be told from \
                 scratch: {e}",
                of.key
            ))
        })?;
        Ok(OpenedCopy {
            path: dest,
            kept_edit,
        })
    }

    /// Removes what this purpose's folder holds that nobody uses: a file of a process that is
    /// gone (or of an earlier run of this pid), a file of a live process or a kept folder older
    /// than `older_than`. Files of THIS run are never removed (their [`ScratchFile`] does) and
    /// names that are not this module's are left alone. How many entries went.
    ///
    /// Only a folder that is ours, closed to others and no link is swept: a link planted at the
    /// purpose's name, or a folder another user made first, is not walked, and nothing in it is
    /// removed (X-DELTA-AZDRIVE-FILES-29). A kept folder is a name `k-` and 16 hex digits - what
    /// [`Scratch::kept`] makes, not any folder that starts with the letters - and it is as old as
    /// the newest thing in it (a file picked into `k-H/docs` changes `docs`, not `k-H`); it is
    /// removed through the held folder, not through a path resolved again.
    pub fn sweep(&self, older_than: Duration) -> usize {
        if let Some(root) = self.dir.parent() {
            if check_private(root).is_err() {
                return 0;
            }
        }
        if check_private(&self.dir).is_err() {
            return 0;
        }
        let Ok(dir) = Dir::open_following(&self.dir) else {
            return 0;
        };
        let Ok(names) = dir.names() else {
            return 0;
        };
        let now = SystemTime::now();
        let age_of = |mtime_ns: u64| {
            now.duration_since(UNIX_EPOCH + Duration::from_nanos(mtime_ns))
                .unwrap_or_default()
        };
        let mut removed = 0;
        for name in names.iter().take(SWEEP_LOOKS) {
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(st) = dir.lstat(name) else {
                continue;
            };
            match (made_by(name, &self.purpose), st.kind) {
                (Some(made), Kind::File) => {
                    if made.is_this_run() {
                        continue;
                    }
                    if (made.orphaned() || age_of(st.mtime_ns) >= older_than)
                        && dir.unlink(name).is_ok()
                    {
                        removed += 1;
                    }
                }
                (None, Kind::Dir) if is_kept_name(name) => {
                    // A folder that holds anything its owner did not write (a copy the user
                    // edited and saved beside or over it) is that person's work, whatever its
                    // age says (X-DELTA-AZDRIVE-FILES-7).
                    let Ok(kept) = dir.child(name) else {
                        continue;
                    };
                    let mut looks = SWEEP_LOOKS;
                    let newest = newest_mtime_ns(&kept, &mut looks);
                    if age_of(newest) >= older_than
                        && !holds_other_work(&self.dir.join(name))
                        && remove_tree(&dir, name, 0).is_ok()
                    {
                        removed += 1;
                    }
                }
                _ => {}
            }
        }
        removed
    }
}

/// Whether `name` is what [`Scratch::kept`] names a folder: `k-` and 16 lowercase hex digits. A
/// folder of another name that starts with the letters (`k-means`) is somebody's own.
fn is_kept_name(name: &str) -> bool {
    name.strip_prefix(KEPT).is_some_and(|hash| {
        hash.len() == 16 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// The time (nanoseconds since 1970) of the newest thing in `dir`, itself and everything below it
/// included - a file added at any depth is use. `u64::MAX` (the folder is young) when it cannot be
/// read whole or `looks` entries are not enough: what cannot be told is not swept.
fn newest_mtime_ns(dir: &Dir, looks: &mut usize) -> u64 {
    let Ok(own) = dir.stat() else {
        return u64::MAX;
    };
    let mut newest = own.mtime_ns;
    let Ok(names) = dir.names() else {
        return u64::MAX;
    };
    for name in &names {
        let Some(name) = name.to_str() else {
            return u64::MAX;
        };
        if *looks == 0 {
            return u64::MAX;
        }
        *looks -= 1;
        let Ok(st) = dir.lstat(name) else {
            return u64::MAX;
        };
        newest = newest.max(st.mtime_ns);
        if st.kind == Kind::Dir {
            match dir.child(name) {
                Ok(below) => newest = newest.max(newest_mtime_ns(&below, looks)),
                Err(_) => return u64::MAX,
            }
        }
    }
    newest
}

/// How deep [`remove_tree`] goes: a kept folder holds a file, a folder or two.
#[cfg(not(windows))]
const TREE_DEPTH: usize = 16;

/// Removes the folder `name` of `parent` and everything in it, through folders it holds: a link in
/// it is removed as a link and never followed, and a name swapped for something else between the
/// look and the removal is not followed either. A name that is not text, or a tree deeper than
/// [`TREE_DEPTH`], stays (an error).
#[cfg(not(windows))]
fn remove_tree(parent: &Dir, name: &str, depth: usize) -> io::Result<()> {
    if depth >= TREE_DEPTH {
        return Err(io::Error::other("the folder is too deep to be scratch"));
    }
    let dir = parent.child(name).map_err(io_of)?;
    for entry in dir.names()? {
        let Some(entry) = entry.to_str() else {
            return Err(io::Error::other("a name that is not text"));
        };
        if dir.lstat(entry)?.kind == Kind::Dir {
            remove_tree(&dir, entry, depth + 1)?;
        } else {
            dir.unlink(entry).map_err(io_of)?;
        }
    }
    parent.rmdir(name).map_err(io_of)
}

/// Windows: the standard library removes a tree without following a link and clears the read-only
/// attribute of the copies of cloud files on the way.
#[cfg(windows)]
fn remove_tree(parent: &Dir, name: &str, _depth: usize) -> io::Result<()> {
    fs::remove_dir_all(parent.path().join(name))
}

/// What became of a file a program noted in a kept folder ([`Scratch::made_state`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MadeState {
    /// It is the file that was written: the same size, time and identity.
    Untouched,
    /// It is not: saved over by an editor, written to, replaced, or gone.
    Changed,
    /// Nothing is known: it was never noted, or its record cannot be read.
    Unknown,
}

/// The copy of a drive's file [`Scratch::open_copy`] makes.
#[derive(Clone, Copy, Debug)]
pub struct CopyOf<'a> {
    /// What names the file: its key on the drive, with whatever tells two drives apart when one
    /// scratch serves several (a copy opened from another drive is another copy).
    pub key: &'a str,
    /// The copy's file name: one name, no folders (the folder of the copy is its version's). A
    /// name that is not one usable file name, or that is the record's own, is refused.
    pub name: &'a str,
    /// What tells one version of the file from the next (the drive's etag and date, the content's
    /// hash): another version is another folder and the first stays as it was. A caller whose drive
    /// cannot tell versions apart gives a fresh value per call, and its copy is made again.
    pub version: &'a str,
    /// The version's size, when the drive says: a copy of another size, whose record is lost, is
    /// not this version's.
    pub size: Option<u64>,
}

/// A copy handed to an app, and the user's work found where the copy goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedCopy {
    /// The copy: read-only, in a folder of its own.
    pub path: PathBuf,
    /// A file that stood at the copy's place and was not the bytes that were written - an editor
    /// that saves by renaming a new file over the read-only copy, "Save As" in its folder, an edit
    /// that kept the size - put aside under this path, never deleted
    /// (X-DELTA-AZDRIVE-FILES-7). The caller says so.
    pub kept_edit: Option<PathBuf>,
}

/// What a program noted of a file it made in a kept folder.
#[derive(Debug, Serialize, Deserialize)]
struct MadeFile {
    size: u64,
    mtime_ns: u64,
    dev: u64,
    ino: u64,
}

impl MadeFile {
    fn of(st: &Stat) -> MadeFile {
        MadeFile {
            size: st.size,
            mtime_ns: st.mtime_ns,
            dev: st.dev,
            ino: st.ino,
        }
    }

    /// Whether `st` is the file that was noted: a regular file of the size and the time, and of
    /// the identity where the system gives one (a file saved over by an editor is another inode).
    /// An identity known on one side only is no match: the careful side.
    fn is(&self, st: &Stat) -> bool {
        let same_identity = match (self.ino, st.ino) {
            (0, 0) => true,
            (0, _) | (_, 0) => false,
            (was, now) => was == now && self.dev == st.dev,
        };
        st.kind == Kind::File
            && st.size == self.size
            && st.mtime_ns == self.mtime_ns
            && same_identity
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MadeRecord {
    files: BTreeMap<String, MadeFile>,
}

/// The record of `folder`; `None` when it has none, an error when it cannot be read (not the same
/// answer: an unreadable record is no absent one).
fn read_record(folder: &Path) -> io::Result<Option<MadeRecord>> {
    match fs::read(folder.join(MADE_RECORD)) {
        Ok(text) => serde_json::from_slice(&text)
            .map(Some)
            .map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Whether the kept folder `folder` holds anything its owner did not write: an entry its record
/// does not name, or a named file that is no longer what was written. A folder with no record is
/// not tracked and holds nobody's work; one whose record or listing cannot be read is not swept
/// (the careful side).
fn holds_other_work(folder: &Path) -> bool {
    let record = match read_record(folder) {
        Ok(Some(record)) => record,
        Ok(None) => return false,
        Err(_) => return true,
    };
    let (Ok(dir), Ok(names)) = (Dir::open_following(folder), fs::read_dir(folder)) else {
        return true;
    };
    for entry in names {
        let Ok(entry) = entry else {
            return true;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return true;
        };
        if name == MADE_RECORD {
            continue;
        }
        let ours = record
            .files
            .get(name)
            .is_some_and(|made| dir.lstat(name).is_ok_and(|st| made.is(&st)));
        if !ours {
            return true;
        }
    }
    false
}

/// Whether `name` can be the file name of a copy: one name (no folder, no NUL), not `.` or `..`,
/// not the record's own (a drive's file called `.made.json` would be written over by the note of
/// what was made, on a file system that does not tell the case of a name).
fn is_copy_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.eq_ignore_ascii_case(MADE_RECORD)
}

/// Whether the copy `name` of `folder` is the version's and as it was written: a regular file (not
/// a link) that [`Scratch::made_state`] does not say was changed and, where the record is lost,
/// that has the version's size. What cannot be told is not usable: the careful side puts it aside.
fn copy_is_usable(folder: &Path, name: &str, size: Option<u64>) -> bool {
    let Ok(meta) = fs::symlink_metadata(folder.join(name)) else {
        return false;
    };
    if !meta.file_type().is_file() {
        return false;
    }
    match (Scratch::made_state(folder, name), size) {
        (MadeState::Untouched, _) => true,
        (MadeState::Unknown, Some(size)) => meta.len() == size,
        (MadeState::Changed | MadeState::Unknown, _) => false,
    }
}

/// Puts the file `name` of `folder` aside as `<stem> (edited)<ext>` (then `(edited 2)`, ...),
/// never over a name that is taken: the user's own work, found where a copy goes. Its path, or
/// `None` when the file was gone by then.
fn put_aside(folder: &Path, name: &str) -> Result<Option<PathBuf>, DriveError> {
    let io = |e: &dyn fmt::Display| DriveError::Io(format!("{name}: {e}"));
    let dir = Dir::open_following(folder).map_err(|e| io(&e))?;
    let (stem, extension) = crate::xfer::names::split_extension(name);
    for n in 1..=ASIDE_MAX {
        let aside = if n == 1 {
            format!("{stem} (edited){extension}")
        } else {
            format!("{stem} (edited {n}){extension}")
        };
        match dir.rename_noreplace(name, &dir, &aside) {
            Ok(()) => return Ok(Some(folder.join(aside))),
            Err(e) if e.is_already_exists() => {}
            Err(e) if e.is_not_found() => return Ok(None),
            Err(e) => return Err(io(&e)),
        }
    }
    Err(io(&"a thousand edited copies are kept beside it already"))
}

/// [`Scratch::sweep`] of every purpose below [`root`].
pub fn sweep_all(older_than: Duration) -> usize {
    sweep_all_in(&root(), older_than)
}

/// [`Scratch::sweep`] of every purpose below `root`.
pub fn sweep_all_in(root: &Path, older_than: Duration) -> usize {
    // A root that is a link, a file or another user's folder is not walked.
    if check_private(root).is_err() {
        return 0;
    }
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
    if !owned_by_me(&meta) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} belongs to another user", path.display()),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
