//! Sync: one folder on this device against one prefix of the drive (unencrypted;
//! [`remote`] has the layout).
//!
//! A run:
//!
//! 1. scans the folder ([`local::scan`]: the rules, the base's hashes for
//!    files that did not move);
//! 2. reads the index (a conditional GET with the ETag of the last run: a
//!    304 means the drive did not change, and the cached copy is used);
//! 3. plans every file with the three-way merge ([`merge::plan`]); a plan
//!    that would delete most of a folder, or every file of a folder that
//!    stays, stops (an emptied or unmounted folder, a ransomware-like burst;
//!    `--allow-mass-delete`); a burst of
//!    rewrites and deletes, or files rewritten into what looks like
//!    encrypted data, pauses the folder's uploads until the user answers
//!    ([`guard`]; `--allow-burst`);
//! 4. uploads the blobs the new index will name - content-addressed, so
//!    whatever happens next nothing is lost or overwritten; each file is
//!    hashed again as it is read, and one that changed since the scan waits
//!    for the next run;
//! 5. writes the new index with `If-Match` (compare-and-swap). Another
//!    device that committed in between makes it fail (412): the run reads
//!    the index again, merges again and retries (up to eight times) - the
//!    blobs already up stay up;
//! 6. applies the committed index here: downloads (each blob checked against
//!    its BLAKE3), deletes, conflict copies, merged JSON - each only when the
//!    file is still as the scan found it, so an edit made during the run is
//!    never overwritten;
//! 7. records the new base.
//!
//! A run that changes nothing sends one GET (a 304 after the first) and no
//! PUT. Two devices that sync in turn converge on the same files; see the
//! table in [`merge`] for what a concurrent edit does.
//!
//! The Azlin tree ([`azlin_roots`]) is two such folders: the data root of
//! every app to `azlin/data/`, the `.azlin` folder to `azlin/config/`, whose
//! `config.json` syncs key by key with the look (`currentTheme`, `mode`) only; the rest of it is this computer's.
//!
//! Every call blocks (call it from an azul `Thread`); small blobs travel several at once on
//! threads of the run's own, against any [`RemoteStore`].
//!
//! Where a folder syncs to is a [`Target`]: the plain index and blobs above (the command
//! line, a plain drive), or an encrypted drive's own files under their names ([`named`]: the
//! drive's index names them; no index of the sync lies in the bucket). The loop is the same.
//!
//! An app adds [`RunHooks`] to a run ([`sync_to`]): files it keeps in the cloud only (a
//! base entry's `cloud_only`: unchanged here at its version, never a delete; the `fetch`
//! policy for files new on the drive), conflicts held for the user instead of copied
//! (`hold_conflicts`, D52: the report's `held`), each file told as it moves (`progress`), and
//! a stop (`cancel`: nothing half-done is committed). [`evict_file`] frees a synced file's
//! bytes here ("Free up space"), [`fetch_file`] brings a cloud-only file down (opening it);
//! [`session`] keeps an app's pairing, its settings and its files' states.

// Extended attributes and BSD flags: the sync does not carry them, a pass says so per folder.
pub mod attrs;
#[cfg(feature = "state-db")]
pub mod base;
// The clock an expiry is judged by: steady for a day, and not behind the drive.
pub mod clock;
// The user's answers to the guards' questions, bound to what was shown.
pub mod consent;
pub mod drive_store;
pub mod engine;
// "Free up space" only once the drive holds the version whole.
pub mod evict;
// Where a test harness crashes or pauses a pass (through SyncSession::with_points), and the
// clock and disk a pass sees.
pub mod faults;
pub mod folder_remote;
// Folders as entries of their own: empty ones made, deleted and renamed everywhere.
pub mod folders;
pub mod guard;
pub mod journal;
pub mod local;
pub mod loops;
pub mod merge;
// How much memory the process took at most (tests bound a pass by it).
pub mod memory;
// A move into the synced folder: the source goes once the drive holds the copy.
pub mod movein;
pub mod named;
pub mod names;
pub mod objects;
// A backup pairing's plan: one way, from the folder to the drive.
mod oneway;
pub mod oplog;
pub mod problems;
pub mod quarantine;
// What a drive synced by name cannot keep in its own index: times, executable bits, deletes.
pub mod record;
pub mod remote;
pub mod retry;
pub mod root;
pub mod rules;
pub mod session;
// Undoing a delete from this device: the trashed files put back.
pub mod trash;
// Whether a tombstone's "the user confirmed this delete" may be believed: one pure judge.
pub mod trust;
// The read-back of both sides against the hashes the pairing knows.
pub mod verify;
pub mod watcher;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Mutex, MutexGuard, PoisonError,
    },
    time::Duration,
};

use azul_storage::{
    beneath::{self, BeneathError, Kind, Stat},
    cap::{Cap, Posture},
    checked::{FileCheck, HashCheck},
    ctl::Quiet,
    durable::{self, Expect, Mode, Publish, PublishError},
    restore::{self, Standing},
    LocalDrive,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use self::{
    folders::FolderAction,
    local::{BaseEntry, BaseFolder, Content, LocalFile, LocalIndex, Scan},
    merge::Action,
    remote::{
        normalize_prefix, FolderTombstone, RemoteFile, RemoteFolder, RemoteIndex, Tombstone,
    },
    rules::Rules,
};
pub use crate::store::{Conditional, RemoteObject, RemoteStore};
use crate::{
    error::{fail, CloudError, CloudResult, Context},
    state::{read_json, write_json},
    store::BIG_BLOB,
    user_errors::{Behaviour, Class, UserError},
};
/// A restore of the drive announced by the token server: the storage crate's, where the
/// decision about it is ([`azul_storage::restore`]).
pub use azul_storage::restore::{RestoreKind, RestoreNotice};

/// The data root of every Azlin app, in the drive.
pub const DATA_PREFIX: &str = "azlin/data/";
/// The `.azlin` folder, in the drive.
pub const HOME_PREFIX: &str = "azlin/config/";
/// How long a tombstone stays in the index.
pub const TOMBSTONE_DAYS: i64 = 90;
/// How often a run tries again after another device won the race.
pub const MAX_ATTEMPTS: u32 = 8;
/// The longest a run waits for a folder to settle ([`SyncOptions::settle`]): a build that
/// writes all the time is synced as it is after this.
pub const SETTLE_MAX: Duration = Duration::from_secs(30);
/// Below this many deletes a plan is never stopped as a mass delete - unless it deletes every
/// file of the folder ([`mass_delete`]).
pub const MASS_DELETE_MIN: usize = 10;
/// The space a download leaves free on the folder's disk by default ([`SyncOptions::disk_reserve`]).
pub const DISK_RESERVE: u64 = 1 << 30;
/// How long a file must be quiet before an app's pass takes it ([`SyncOptions::file_settle`]).
pub const FILE_SETTLE: Duration = Duration::from_secs(2);
/// The biggest file a sync takes by default: S3's largest object (5 TiB). A big file travels
/// in parts - up from its file (an upload cut off goes on from the parts the bucket holds, and
/// its BLAKE3 is checked again before it completes), down into a file - never whole in memory.
pub const MAX_FILE_BYTES: u64 = 5 * (1 << 40);

/// How a run behaves.
#[derive(Clone, Debug)]
pub struct SyncOptions {
    /// The drive prefix ([`normalize_prefix`]).
    pub prefix: String,
    /// The bucket (recorded in the local index).
    pub bucket: String,
    /// This device's name: conflict copies, the index's `device`.
    pub device: String,
    /// Small blobs in flight at once.
    pub parallel: usize,
    pub max_attempts: u32,
    /// The command line's `--allow-mass-delete`: this run confirms every delete it plans. An
    /// app's pairing leaves it off and passes the user's answer in [`SyncOptions::consents`].
    pub allow_mass_delete: bool,
    /// The user's "these changes are mine": the run sends what the burst
    /// guard held back, and its window starts afresh ([`guard`]).
    pub allow_burst: bool,
    /// The user's answers to the questions this pairing asked, each for the keys (or the pause) it
    /// was given for ([`consent::Consent`]): a plan that deletes another set, or a pause that is
    /// another, asks again.
    pub consents: Vec<consent::Consent>,
    /// The account's other devices by name (this one always counts): files they wrote need no
    /// quarantine mark ([`quarantine::Writers`]). A name only spares a warning; it is never
    /// proof of who wrote a tombstone - the breaker counts a tombstone's delete whatever the
    /// tombstone says ([`trust::trust_tombstone`], SYNC-1).
    pub own_devices: Vec<String>,
    /// Plan only: nothing is uploaded, written or deleted.
    pub dry_run: bool,
    pub max_file_bytes: u64,
    pub tombstone_days: i64,
    /// Seconds this computer's clock is off the true time, as a test harness plays a wrong clock
    /// ([`session::SyncSession::with_clock`], the command line's `AZSYNC_CLOCK_OFFSET`): the
    /// expiries read the wall clock through it ([`clock::TrustedClock`]). Zero in every real run.
    pub clock_offset: i64,
    /// A run first waits until nothing in the folder changed for this long (at most
    /// [`SETTLE_MAX`]): a burst of changes - a checkout, an unzip, a build - goes in one run
    /// as a whole, in batches, and the burst guard sees all of it. Zero (the default): the run
    /// starts at once.
    pub settle: Duration,
    /// A file written within this long is left for a later run (listed as settling, never a
    /// delete meanwhile): a file still being written is never uploaded half. Zero (the default
    /// of the command line): every file is taken as it is; an app's session uses
    /// [`FILE_SETTLE`].
    pub file_settle: Duration,
    /// A download that would leave less than this many bytes free on the folder's disk is not
    /// started (listed): another person's files never fill this disk ([`DISK_RESERVE`]).
    pub disk_reserve: u64,
    /// The drive's newest restore announcement, as the token server hands it out: what the
    /// drive serves is taken as that restore (and healed from here) when its epoch is above the
    /// one the base saw - whatever the drive's counter says ([`restore::judge`]); without one an
    /// index older than the last one seen is refused.
    pub restore: Option<RestoreNotice>,
}

impl SyncOptions {
    /// The defaults for `prefix` of `bucket` on the device `device`.
    ///
    /// # Errors
    ///
    /// When `prefix` is no prefix a sync may take.
    pub fn new(prefix: &str, bucket: &str, device: &str) -> CloudResult<SyncOptions> {
        Ok(SyncOptions {
            prefix: normalize_prefix(prefix)?,
            bucket: bucket.to_string(),
            device: device.to_string(),
            parallel: 4,
            max_attempts: MAX_ATTEMPTS,
            allow_mass_delete: false,
            allow_burst: false,
            consents: Vec::new(),
            own_devices: Vec::new(),
            dry_run: false,
            max_file_bytes: MAX_FILE_BYTES,
            tombstone_days: TOMBSTONE_DAYS,
            clock_offset: 0,
            settle: Duration::ZERO,
            file_settle: Duration::ZERO,
            disk_reserve: DISK_RESERVE,
            restore: None,
        })
    }
}

/// A folder on this device.
#[derive(Clone, Debug)]
pub struct LocalRoot {
    pub path: PathBuf,
    /// The Azlin data root: written through azul-storage's `LocalDrive::new`,
    /// which keeps its `.azlin/cache` manifest right (every app's write goes
    /// through it too).
    pub data_tree: bool,
    /// Keys synced as JSON objects without their machine-local keys.
    pub json_merge: Vec<String>,
    pub rules: Rules,
}

impl LocalRoot {
    /// Any folder: the base rules and its `.azcloudignore`.
    #[must_use]
    pub fn folder(path: &Path) -> LocalRoot {
        LocalRoot {
            path: path.to_path_buf(),
            data_tree: false,
            json_merge: Vec::new(),
            rules: Rules::base(),
        }
        .with_ignore_file()
    }

    /// Adds the lines of the folder's `.azcloudignore`, if it has one.
    #[must_use]
    pub fn with_ignore_file(mut self) -> LocalRoot {
        if let Ok(text) = fs::read_to_string(self.path.join(rules::IGNORE_FILE)) {
            self.rules.add_ignore_file(&text);
        }
        self
    }

    fn drive(&self) -> LocalDrive {
        if self.data_tree {
            LocalDrive::new(&self.path)
        } else {
            LocalDrive::without_manifest(&self.path)
        }
    }
}

/// The Azlin tree: the data root (to [`DATA_PREFIX`], the data tree's rules)
/// and the `.azlin` folder (to [`HOME_PREFIX`], its `config.json` merged per
/// key without the machine-local keys). When one lies inside the other, the
/// outer one leaves it to its own sync.
#[must_use]
pub fn azlin_roots(data_root: &Path, azlin_home: &Path) -> Vec<(LocalRoot, &'static str)> {
    let mut data = LocalRoot {
        path: data_root.to_path_buf(),
        data_tree: true,
        json_merge: Vec::new(),
        rules: Rules::azlin_data(),
    }
    .with_ignore_file();
    let mut home = LocalRoot {
        path: azlin_home.to_path_buf(),
        data_tree: false,
        json_merge: vec![azul_appkit::azlin_config::CONFIG_FILE.to_string()],
        rules: Rules::azlin_home(),
    }
    .with_ignore_file();
    if let Some(inner) = relative_key(data_root, azlin_home) {
        home.rules
            .add(&format!("/{inner}/"), "the data root, synced on its own");
    }
    if let Some(inner) = relative_key(azlin_home, data_root) {
        data.rules.add(
            &format!("/{inner}/"),
            "the .azlin folder, synced on its own",
        );
        // The .azlin folder is the data root's bookkeeping folder: its sync/ holds the data
        // root's marker and trash, which no other device may get.
        if inner == names::BOOKKEEPING {
            home.rules.add(
                "/sync/",
                "the data root's sync bookkeeping (its marker, its trash)",
            );
        }
    }
    vec![(data, DATA_PREFIX), (home, HOME_PREFIX)]
}

/// `inner` relative to `outer` as a key (`a/b`), when it lies inside it.
fn relative_key(inner: &Path, outer: &Path) -> Option<String> {
    let rel = inner.strip_prefix(outer).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// What a run did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncReport {
    pub root: String,
    pub prefix: String,
    pub dry_run: bool,
    /// Files found, hashed, taken by a rule, skipped (and why).
    pub scanned: usize,
    pub hashed: usize,
    pub excluded: usize,
    pub skipped: Vec<(String, String)>,
    /// Files whose version here went to the drive; blobs and bytes that
    /// travelled; blobs the drive already had.
    pub files_up: usize,
    pub blobs_up: usize,
    pub bytes_up: u64,
    pub blobs_reused: usize,
    /// Files written here from the drive, and their bytes.
    pub files_down: usize,
    pub bytes_down: u64,
    pub deleted_here: usize,
    pub deleted_there: usize,
    pub unchanged: usize,
    /// Folders made here because the drive has them, and removed here (empty) because the drive
    /// deleted them.
    pub folders_made: usize,
    pub folders_removed: usize,
    /// `key -> copy` of every conflict.
    pub conflicts: Vec<String>,
    /// JSON files merged key by key, and the keys both sides changed (the
    /// drive's value won).
    pub merged: Vec<String>,
    pub merge_clashes: Vec<String>,
    /// Whether the index was written, its generation, and how often another
    /// device won the race first.
    pub index_written: bool,
    pub generation: u64,
    pub cas_retries: u32,
    /// Files left for the next run: they changed while this one ran.
    pub changed_during_sync: Vec<String>,
    /// What went wrong for single files (the rest of the run went on).
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    /// `--dry-run`: what a run would do.
    pub planned: Vec<String>,
    /// The files of the drive this device keeps in the cloud only after the run (an app's
    /// on-demand files, [`RunHooks`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cloud_only: Vec<String>,
    /// The conflicts held for the user ([`RunHooks::hold_conflicts`]): nothing of them moved.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<HeldConflict>,
    /// The run was stopped ([`RunHooks::cancel`]): what it did not do waits for the next one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cancelled: bool,
    /// The files whose transfer failed, and why (`errors` says the same in sentences).
    #[serde(skip)]
    pub file_errors: Vec<(String, String)>,
    /// The drive's files after the run (what an app shows); `None` after a dry or stopped run.
    #[serde(skip)]
    pub remote: Option<RemoteIndex>,
    /// The folder's uploads, paused by the burst guard until the user answers ([`guard`]):
    /// nothing went up, what the drive changed came here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused: Option<guard::Pause>,
    /// What the drive's index uses that this version does not know (D43,
    /// [`RemoteIndex::unknown_features`]): the run changed nothing on either side - the drive
    /// is read-only here until the app is updated.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub newer_format: Vec<String>,
    /// A mass delete held for the user ([`RunHooks::hold_mass_delete`]): nothing changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mass_delete: Option<MassDelete>,
    /// The files whose transfer [`RunHooks::transfer_limit`] held back, either way: their
    /// versions stay where they are, and the next run without the limit moves them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held_back: Vec<String>,
    /// The files' problems the run recorded with their codes (the problem list's source;
    /// [`problems::from_report`] adds what the sentences above say): the first
    /// [`problems::LISTED_PER_CODE`] of each code, and one row of it that says how many more there
    /// were ([`problems::Problem::more`]; X-DELTA-EDGE-3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<problems::Problem>,
    /// How many problems of each code the run told, kept or not (the scan's counted in).
    #[serde(skip)]
    pub(crate) told: problems::Tally,
    /// The files a delete from the drive moved into the folder's trash here, and where to
    /// (from the folder's root).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trashed: Vec<(String, String)>,
    /// The files the drive moved that were renamed here (`old -> new`), not fetched again.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<String>,
    /// Why the run changed nothing anywhere (the folder is missing or not the paired one, the
    /// drive's index went back): its problem says it too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

impl SyncReport {
    /// Records the problem `code` of `key`, while its code has room in the list
    /// ([`problems::Tally`]): a hostile index with a hundred thousand bad keys is no hundred
    /// thousand problems in memory (X-DELTA-EDGE-3).
    fn tell(&mut self, code: problems::Code, key: &str, detail: impl Into<String>) {
        if self.told.admit(code) {
            self.problems
                .push(problems::Problem::new(code, key, detail));
        }
    }

    /// Puts into the problems the one row per code that says how many it left out.
    fn close_problems(&mut self) {
        let told = self.told.clone();
        told.close(&mut self.problems);
    }

    /// The run changes nothing anywhere, and says why.
    fn refuse(&mut self, code: problems::Code, detail: String) {
        self.tell(code, "", detail.clone());
        self.errors.push(detail.clone());
        self.refused = Some(detail);
    }

    /// One line for people.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(why) = &self.refused {
            return format!(
                "{} <-> {}: nothing synced, nothing deleted - {why}",
                self.root, self.prefix
            );
        }
        if !self.newer_format.is_empty() {
            return format!(
                "{} <-> {}: nothing synced - the drive's sync index uses what this version does \
                 not know ({}): update it",
                self.root,
                self.prefix,
                self.newer_format.join(", ")
            );
        }
        if self.dry_run {
            return format!(
                "{} <-> {}: dry run, {} changes planned",
                self.root,
                self.prefix,
                self.planned.len()
            );
        }
        let paused = self.paused.as_ref().map_or_else(String::new, |p| {
            format!(
                "; uploads paused: {} - run again with --allow-burst if the changes are yours",
                p.describe()
            )
        });
        format!(
            "{} <-> {}: {} up ({} bytes in {} blobs), {} down ({} bytes), {} deleted here, {} \
             deleted on the drive, {} unchanged, {} conflicts, {} merged; index {} (generation \
             {}, {} retries){paused}",
            self.root,
            self.prefix,
            self.files_up,
            self.bytes_up,
            self.blobs_up,
            self.files_down,
            self.bytes_down,
            self.deleted_here,
            self.deleted_there,
            self.unchanged,
            self.conflicts.len(),
            self.merged.len(),
            if self.index_written {
                "written"
            } else {
                "unchanged"
            },
            self.generation,
            self.cas_retries
        )
    }
}

/// One file moving, as a run tells an app ([`RunHooks::progress`]; from the run's threads).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncEvent {
    /// What the run is about to move: files and bytes up to the drive and down from it.
    Planned {
        up_files: usize,
        up_bytes: u64,
        down_files: usize,
        down_bytes: u64,
    },
    /// A file starts to travel (`up`: to the drive).
    Started { key: String, up: bool, bytes: u64 },
    /// It arrived, or why it did not.
    Finished {
        key: String,
        up: bool,
        bytes: u64,
        error: Option<String>,
    },
}

/// A file changed here and on the drive since the last sync, held for the user
/// ([`RunHooks::hold_conflicts`], D52): keep mine, take theirs, or keep both.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldConflict {
    pub key: String,
    /// This device's version (BLAKE3) and the drive's, its size and the device that wrote it.
    pub here: String,
    pub there: String,
    pub there_size: u64,
    pub there_device: String,
    /// The drive's executable bit: "keep mine" takes the drive's version as the base, and the
    /// bit is part of a file's content.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub there_exec: bool,
}

/// What an app adds to a run; the command line's run has none of it ([`sync_folder`]).
#[derive(Clone, Copy, Default)]
pub struct RunHooks<'a> {
    /// Whether a file the drive has and this folder has not (new there; deleted here and
    /// changed there) comes down now: (its key, its bytes). `false` keeps it in the cloud
    /// only. `None`: everything comes down.
    pub fetch: Option<&'a (dyn Fn(&str, u64) -> bool + Sync)>,
    /// A file changed on both sides waits for the user (the report's `held`) instead of
    /// becoming a conflict copy - unless its key is in `keep_both`.
    pub hold_conflicts: bool,
    pub keep_both: Option<&'a BTreeSet<String>>,
    /// Hears each file as it moves.
    pub progress: Option<&'a (dyn Fn(SyncEvent) + Sync)>,
    /// Set: the run stops before its next file; nothing half-done is committed.
    pub cancel: Option<&'a AtomicBool>,
    /// A plan that would delete most of the folder here or there changes nothing and waits for
    /// the user (the report's `mass_delete`) instead of failing.
    pub hold_mass_delete: bool,
    /// Files over this many bytes wait for a run without the limit, either way (an app on a
    /// metered network): the report's `held_back`. A rename of such a file waits whole: its new
    /// name's transfer and the old name's delete. `None`: no limit.
    pub transfer_limit: Option<u64>,
    /// The engine around the run ([`engine::Engine`]): the journal, the operations log, the
    /// loop detector and quarantine marks hear every local step. `None`: none of them.
    pub observer: Option<&'a dyn engine::Observer>,
    /// A backup pairing: one way, from the folder to the drive ([`oneway`]). Nothing on the
    /// drive changes the folder, and nothing deleted here is deleted there.
    pub backup: bool,
}

impl RunHooks<'_> {
    fn tell(&self, event: SyncEvent) {
        if let Some(progress) = self.progress {
            progress(event);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::SeqCst))
    }

    fn holds(&self, key: &str) -> bool {
        self.hold_conflicts && !self.keep_both.is_some_and(|keys| keys.contains(key))
    }
}

/// Where a folder syncs to.
#[derive(Clone, Copy)]
pub enum Target<'a> {
    /// The plain JSON index and the BLAKE3 blobs under the prefix ([`remote`]): the command
    /// line's, a plain drive's.
    Index(&'a dyn RemoteStore),
    /// The drive's files under their own names, its own index naming them ([`named`]): an
    /// encrypted drive (its names, sizes and BLAKE3s in its metadata repository).
    Named(&'a dyn azul_storage::Drive),
}

/// Today, `2026-10-08` (UTC), for conflict copies.
fn today() -> String {
    let stamp = crate::rfc3339(crate::now());
    stamp.get(..10).unwrap_or(&stamp).to_string()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `work` of every job on up to `parallel` threads of this run; the results in the jobs'
/// order (every job runs, whatever the others did).
fn each_in_parallel<J: Sync, R: Send>(
    jobs: &[J],
    parallel: usize,
    work: impl Fn(&J) -> R + Sync,
) -> Vec<R> {
    if jobs.is_empty() {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..parallel.max(1).min(jobs.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else {
                    break;
                };
                let result = work(job);
                lock(&results)[i] = Some(result);
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .flatten()
        .collect()
}

/// The last index a run saw, kept beside the local index: what a 304 stands
/// for.
#[derive(Serialize, Deserialize)]
struct CachedIndex {
    etag: String,
    /// The index as the drive holds it, its damaged entries too ([`RemoteIndex::to_bytes`]),
    /// read back as the drive's own bytes are ([`RemoteIndex::parse`]): an entry a pass keeps
    /// aside as unreadable is never missing from the copy the next pass works from.
    index: serde_json::Value,
}

fn cache_path(index_path: &Path) -> PathBuf {
    index_path.with_extension("remote.json")
}

fn load_cache(index_path: &Path, local: &LocalIndex) -> Option<(String, RemoteIndex)> {
    let cached: CachedIndex = read_json(&cache_path(index_path)).ok().flatten()?;
    if local.remote_etag.as_deref() != Some(cached.etag.as_str()) {
        return None;
    }
    let bytes = serde_json::to_vec(&cached.index).ok()?;
    let index = RemoteIndex::parse(&bytes).ok()?;
    Some((cached.etag, index))
}

fn save_cache(index_path: &Path, etag: Option<&str>, index: &RemoteIndex) {
    let path = cache_path(index_path);
    match etag.filter(|e| !e.is_empty()) {
        Some(etag) => {
            let Ok(whole) = serde_json::from_slice(&index.to_bytes()) else {
                let _ = fs::remove_file(path);
                return;
            };
            let cached = CachedIndex {
                etag: etag.to_string(),
                index: whole,
            };
            let _ = write_json(&path, &cached, true);
        }
        None => {
            let _ = fs::remove_file(path);
        }
    }
}

/// What a pass sends, kept beside its base from before its first upload until the base is
/// saved. After a pass that stopped in between, the next one takes what of it is on the drive
/// into the base ([`adopt_sent`]).
#[derive(Default, Serialize, Deserialize)]
struct Sending {
    /// Each key and the content the pass sends under it (`None`: its delete).
    files: BTreeMap<String, Option<String>>,
}

fn sending_path(index_path: &Path) -> PathBuf {
    index_path.with_extension("sending.json")
}

/// Notes what `actions` send (best effort: without it a pass that stops short only makes the
/// next one compare once more).
///
/// A conflict's copy is NOT noted (AZ-S10): it goes up under a name of its own, but until the pass
/// has renamed this side's file to that name the copy is not a file of this folder, and a base
/// that already held it would plan its delete on the drive - and a second copy - at the next
/// pass. Left out of the base, the next pass finds the copy on the drive with this side's bytes
/// and finishes the conflict ([`merge::plan`], `cut_conflicts`).
fn note_sending(
    index_path: &Path,
    actions: &[Action],
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
) {
    let mut files = BTreeMap::new();
    for action in actions {
        let key = match action {
            Action::DeleteRemote { key } => {
                files.insert(key.clone(), None);
                continue;
            }
            Action::Conflict { .. } => continue,
            other => other.key(),
        };
        if let Some(hash) = uploaded_hash(action, scan, merged) {
            files.insert(key.to_string(), Some(hash.to_string()));
        }
    }
    let path = sending_path(index_path);
    if files.is_empty() {
        let _ = fs::remove_file(path);
    } else {
        let _ = write_json(&path, &Sending { files }, true);
    }
}

/// Takes into `base` what the last pass sent (its note beside the base) as far as `remote`
/// holds it: a version the drive names with the content sent, a delete the drive has made.
/// What did not land stays as the base had it.
fn adopt_sent(index_path: &Path, remote: &RemoteIndex, base: &mut BTreeMap<String, BaseEntry>) {
    let Ok(Some(sent)) = read_json::<Sending>(&sending_path(index_path)) else {
        return;
    };
    for (key, hash) in sent.files {
        match (hash, remote.files.get(&key)) {
            (Some(hash), Some(file)) if file.hash == hash => {
                if base.get(&key).is_some_and(|b| b.hash == hash) {
                    continue;
                }
                base.insert(
                    key,
                    BaseEntry {
                        hash,
                        size: file.size,
                        exec: file.exec,
                        ..BaseEntry::default()
                    },
                );
            }
            (None, None) => {
                base.remove(&key);
            }
            _ => {}
        }
    }
}

/// The index and its ETag (`None`: there is none yet). With `cached`, a
/// conditional GET: a 304 answers with the cached copy.
fn read_index<S: RemoteStore + ?Sized>(
    store: &S,
    key: &str,
    cached: Option<&(String, RemoteIndex)>,
) -> CloudResult<(RemoteIndex, Option<String>)> {
    let etag = cached.map(|(etag, _)| etag.as_str());
    match store.get_unless(key, etag)? {
        Conditional::NotFound => Ok((RemoteIndex::empty(), None)),
        Conditional::NotModified => match cached {
            Some((etag, index)) => Ok((index.clone(), Some(etag.clone()))),
            None => fail!("the drive answered 304 to an unconditional GET of {key}"),
        },
        Conditional::Found { body, etag } => {
            let etag = etag.filter(|e| !e.is_empty()).ok_or_else(|| {
                CloudError::failed(format!(
                    "the drive's S3 service sent no ETag for {key}; the sync needs conditional \
                     writes (If-Match)"
                ))
            })?;
            Ok((RemoteIndex::parse(&body)?, Some(etag)))
        }
    }
}

/// The blob `hash`, checked against its name.
fn fetch_blob<S: RemoteStore + ?Sized>(
    store: &S,
    prefix: &str,
    hash: &str,
    size: u64,
) -> CloudResult<Vec<u8>> {
    let key = remote::blob_key(prefix, hash);
    let bytes = store
        .fetch(&key, size)?
        .ok_or_else(|| CloudError::failed(format!("the blob {key} is missing from the drive")))?;
    if local::hash_bytes(&bytes) != hash {
        fail!("the blob {key} is damaged: its BLAKE3 is not its name");
    }
    Ok(bytes)
}

/// A plan that would delete most of the folder here or there, waiting for the user
/// ([`RunHooks::hold_mass_delete`]; the command line stops with `--allow-mass-delete` to say).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MassDelete {
    /// `true`: the drive deleted them elsewhere and this folder would lose its copies; `false`:
    /// they are gone from this folder and the drive would lose them.
    pub here: bool,
    /// How many, of how many files (the folder's now / at its last sync).
    pub count: usize,
    pub of: usize,
    /// Every file it would delete: those gone from this folder that the drive would lose, and
    /// those the drive deleted elsewhere that this folder still holds - both can be in one
    /// question ([`session::SyncSession::answer_mass_delete`] tells them apart by the disk).
    pub keys: Vec<String>,
}

impl MassDelete {
    /// What the user answers when they answer this question: its kind and the files it names
    /// ([`consent::keys_hash`]). A question that names other files is another question, whatever
    /// the dialog showed (X-DELTA-AZDRIVE-SYNC-14): an answer that carries the identity it was given
    /// for is refused when the one waiting differs
    /// ([`session::SyncSession::answer_mass_delete_shown`]).
    #[must_use]
    pub fn identity(&self) -> String {
        let keys: Vec<&str> = self.keys.iter().map(String::as_str).collect();
        consent::keys_hash(consent::ConsentKind::MassDelete, &keys)
    }
}

/// The mass delete a plan would make, if it would (D1): more than [`MASS_DELETE_MIN`] deletes
/// and more than half of the folder's files at its last sync, or every one of them (at least
/// two) - a folder of four tax files emptied is as suspect as one of four thousand - or every
/// file of a folder that stays ([`emptied_folder`], SYNC-F2). Deletes here and on the drive count
/// together. A delete whose content goes up under another name in the same plan is a move, not
/// counted; a delete here is counted whatever its tombstone says: the flag `confirmed` is a word
/// anyone who can write to the drive can put in a tombstone, and the drive authenticates no writer
/// ([`trust::trust_tombstone`], S7, SYNC-1), so the user's other computer asks once more.
fn mass_delete(
    actions: &[Action],
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    remote: &RemoteIndex,
    opts: &SyncOptions,
) -> Option<MassDelete> {
    let moved_here: BTreeSet<&str> = local_moves(actions, scan, remote).into_values().collect();
    let own: Vec<trust::MemberId> = opts
        .own_devices
        .iter()
        .map(|d| trust::MemberId::new(d))
        .collect();
    let here: Vec<String> = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteLocal { .. }))
        .filter(|a| {
            remote.deleted.get(a.key()).is_none_or(|t| {
                trust::trust_tombstone(t, trust::verified_writer(t), &own) == trust::Trust::Ask
            })
        })
        .filter(|a| !moved_here.contains(a.key()))
        .map(|a| a.key().to_string())
        .collect();
    let moved = moves(actions, scan, base);
    let there: Vec<String> = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteRemote { .. }))
        .filter(|a| !moved.contains(a.key()))
        .map(|a| a.key().to_string())
        .collect();
    let count = here.len() + there.len();
    let of = base.len();
    let mass = count > MASS_DELETE_MIN && count * 2 > of;
    let all = count >= 2 && count >= of;
    if count == 0 || !(mass || all || emptied_folder(&there, &here, base, scan, remote)) {
        return None;
    }
    let is_here = here.len() >= there.len();
    let mut keys = here;
    keys.extend(there);
    Some(MassDelete {
        here: is_here,
        count,
        of,
        keys,
    })
}

/// Whether a plan takes EVERY synced file (two or more) out of a folder that stays (SYNC-F2):
/// the files `there` were deleted here and the folder is still here, or the files `here` were
/// deleted on the drive and the drive still has the folder. Files left in a folder one by one
/// are what a tool, a failing disk or an attack does; a person who deletes a folder removes the
/// folder with it, and that goes through as before (a question for each would be a nag, and the
/// bytes are in the trash and on the drive's versions). Files kept in the cloud only are not
/// here to be taken out and do not count.
fn emptied_folder(
    there: &[String],
    here: &[String],
    base: &BTreeMap<String, BaseEntry>,
    scan: &Scan,
    remote: &RemoteIndex,
) -> bool {
    // The folders a key lies in, outermost first: `a/b/c.txt` -> `a`, `a/b`.
    fn folders_of(key: &str) -> impl Iterator<Item = &str> {
        key.match_indices('/').map(move |(i, _)| &key[..i])
    }
    if there.is_empty() && here.is_empty() {
        return false;
    }
    let mut synced: BTreeMap<&str, usize> = BTreeMap::new();
    for (key, entry) in base {
        if !entry.cloud_only {
            for folder in folders_of(key) {
                *synced.entry(folder).or_default() += 1;
            }
        }
    }
    let taken_out = |keys: &[String], stays: &dyn Fn(&str) -> bool| {
        let mut gone: BTreeMap<&str, usize> = BTreeMap::new();
        for key in keys {
            for folder in folders_of(key) {
                *gone.entry(folder).or_default() += 1;
            }
        }
        gone.into_iter()
            .any(|(folder, n)| n >= 2 && synced.get(folder) == Some(&n) && stays(folder))
    };
    taken_out(there, &|folder| scan.dirs.contains(folder))
        || taken_out(here, &|folder| remote.folders.contains_key(folder))
}

/// The deletes on the drive a plan makes whose bytes go up under another name in the same plan:
/// files renamed or moved here (a folder renamed). They are no delete and no change of a file -
/// neither the mass-delete guard nor the burst guard counts them.
fn moves(actions: &[Action], scan: &Scan, base: &BTreeMap<String, BaseEntry>) -> BTreeSet<String> {
    let mut arriving: BTreeMap<&str, usize> = BTreeMap::new();
    for action in actions {
        if let Action::Upload { key } = action {
            if let Some(file) = scan.files.get(key) {
                *arriving.entry(file.hash.as_str()).or_default() += 1;
            }
        }
    }
    let mut moved = BTreeSet::new();
    for action in actions {
        let Action::DeleteRemote { key } = action else {
            continue;
        };
        let Some(entry) = base.get(key) else {
            continue;
        };
        if let Some(left) = arriving
            .get_mut(entry.hash.as_str())
            .filter(|left| **left > 0)
        {
            *left -= 1;
            moved.insert(key.clone());
        }
    }
    moved
}

/// The command line's stop of a mass delete.
fn mass_delete_error(asked: &MassDelete) -> CloudError {
    if asked.here {
        CloudError::failed(format!(
            "the drive says {} of this folder's {} files were deleted elsewhere. That looks \
             like a mistake (or ransomware), so nothing was changed; run again with \
             --allow-mass-delete if it is right",
            asked.count, asked.of
        ))
    } else {
        CloudError::failed(format!(
            "{} of the {} files this folder held at its last sync are gone from it (an \
             emptied or unmounted folder?). Nothing was changed; run again with \
             --allow-mass-delete if they were deleted on purpose",
            asked.count, asked.of
        ))
    }
}

/// A JSON file merged key by key: its blob, the blob's hash, and the file to
/// write here (the blob with this computer's machine-local keys).
struct Merged {
    blob: Vec<u8>,
    hash: String,
    file: Vec<u8>,
}

/// The drive side of a run: where the index comes from, where the bytes go. [`BlobIndex`]
/// is the plain index and blobs of [`remote`]; [`named::NamedFiles`] an encrypted drive's own
/// files under their names.
trait SyncRemote: Sync {
    /// The index and the marker the next read is conditional on (`None`: there is none yet).
    /// With `cached` (the last index and its marker), a read that finds nothing new may answer
    /// with it.
    fn read(
        &self,
        cached: Option<&(String, RemoteIndex)>,
    ) -> CloudResult<(RemoteIndex, Option<String>)>;

    /// The bytes of `key`, the version whose BLAKE3 is `hash`, checked against it.
    fn fetch(&self, key: &str, hash: &str, size: u64) -> CloudResult<Vec<u8>>;

    /// The bytes of the content `hash` (a base's version, for a key-by-key merge); an error
    /// when the drive no longer has them.
    fn fetch_hash(&self, hash: &str, size: u64) -> CloudResult<Vec<u8>>;

    /// Reads the drive's copy of `key` (`file` as the listing named it; a big one into `spool`)
    /// back whole and says what came of it (SYNC-4, [`verify::ReadBack`]). By default by
    /// [`SyncRemote::fetch`]: every failure is a read that failed, never damage - a remote that
    /// cannot tell a damaged copy from another device's newer version says neither.
    fn read_back(&self, key: &str, file: &RemoteFile, spool: Option<&Path>) -> verify::ReadBack {
        verify::read_back_by_fetch(self, key, file, spool)
    }

    /// Sends the version `file` of `key` again from the file at `from` over the drive's damaged
    /// copy of it (SYNC-4). Only a write that is conditional on the version the listing carried
    /// and that completes only when `from` still holds the version's bytes: a copy that changed
    /// since it was hashed, or a drive file another device replaced, is left as it is and the
    /// error says so. By default a remote heals nothing.
    fn heal(&self, key: &str, file: &RemoteFile, from: &Path) -> CloudResult<()> {
        let _ = (file, from);
        fail!("the drive's copy of {key} is not sent again from here: this drive is not healed")
    }

    /// Whether the drive keeps a file's executable bit. A drive that cannot (one synced by name
    /// that keeps no folder record) leaves it out of every comparison: a file would otherwise
    /// look changed there on every run.
    fn tracks_exec(&self) -> bool {
        true
    }

    /// Whether the drive keeps folders as entries of their own ([`remote::RemoteIndex::folders`],
    /// [`folders`]). One that does not (a harness's layout, a drive synced by name without a
    /// record) syncs files only: a folder goes with its last file, as before folders were kept.
    fn keeps_folders(&self) -> bool {
        false
    }

    /// Whether the drive keeps links as entries of their own ([`remote::RemoteFile::link`]). One
    /// that does not gets none: every link here is skipped and listed, never followed.
    fn keeps_links(&self) -> bool {
        false
    }

    /// Whether an entry may name the drive's version of a file instead of its content
    /// ([`remote::version_stand_in`]): a drive synced by name knows a file's content only once
    /// a device read or wrote it. Anywhere else such an entry is a damaged one.
    fn names_versions(&self) -> bool {
        false
    }

    /// What the generation this side serves counts ([`restore::judge`]): every device's commits
    /// (a plain drive's index) or this device's own (a drive synced by name, whose generation
    /// says nothing about the drive and so never marks a rollback).
    fn clock(&self) -> restore::Clock {
        restore::Clock::Shared
    }

    /// [`SyncRemote::fetch`] into the file `dest` (a big file: in ranges, never whole in
    /// memory where the drive can), checked the same way; the content's hash for the base.
    fn fetch_to(&self, key: &str, hash: &str, size: u64, dest: &Path) -> CloudResult<String> {
        let bytes = self.fetch(key, hash, size)?;
        durable::replace(dest, &bytes, Mode::Shared)?;
        Ok(content_hash(hash, &bytes))
    }

    /// [`SyncRemote::fetch_to`] that calls `mid` once, when the first part of a download in
    /// parts is in the file (the `mid-download` point: [`RemoteStore::fetch_to_with`]). By
    /// default the content comes whole, without `mid`.
    fn fetch_to_with(
        &self,
        key: &str,
        hash: &str,
        size: u64,
        dest: &Path,
        mid: &dyn Fn(),
    ) -> CloudResult<String> {
        let _ = mid;
        self.fetch_to(key, hash, size, dest)
    }

    /// Puts what the plan sends to the drive where the commit can name it; the keys whose
    /// upload waits for the next run (their file changed, the run was stopped, ...).
    fn upload(&self, work: &Uploads<'_>, report: &mut SyncReport) -> CloudResult<BTreeSet<String>>;

    /// Makes `next` the drive's state in place of `remote` (read with `etag`): the new marker;
    /// `None` when another device committed first (the run reads, merges and tries again).
    fn commit(
        &self,
        remote: &RemoteIndex,
        next: &RemoteIndex,
        etag: Option<&str>,
        report: &mut SyncReport,
    ) -> CloudResult<Option<String>>;
}

/// What an upload looks at.
struct Uploads<'a> {
    root: &'a LocalRoot,
    actions: &'a [Action],
    scan: &'a Scan,
    merged: &'a BTreeMap<String, Merged>,
    remote: &'a RemoteIndex,
    opts: &'a SyncOptions,
    hooks: &'a RunHooks<'a>,
    /// The folders renamed here that the drive renames with this run, in order (`remote` and
    /// the plan have them at their new names already): a drive whose commit is its files moves
    /// them before anything is sent.
    folder_moves: &'a [(String, String)],
}

/// The plain JSON index and BLAKE3 blobs under a prefix ([`remote`]).
struct BlobIndex<'s, S: RemoteStore + ?Sized> {
    store: &'s S,
    prefix: String,
    index_key: String,
    /// The blobs a run put or found up already: a retry after a lost race does not send them
    /// again.
    known: Mutex<BTreeSet<String>>,
}

impl<'s, S: RemoteStore + ?Sized> BlobIndex<'s, S> {
    fn new(store: &'s S, prefix: &str) -> Self {
        BlobIndex {
            store,
            prefix: prefix.to_string(),
            index_key: remote::index_key(prefix),
            known: Mutex::new(BTreeSet::new()),
        }
    }
}

impl<S: RemoteStore + ?Sized> SyncRemote for BlobIndex<'_, S> {
    fn read(
        &self,
        cached: Option<&(String, RemoteIndex)>,
    ) -> CloudResult<(RemoteIndex, Option<String>)> {
        read_index(self.store, &self.index_key, cached)
    }

    fn keeps_folders(&self) -> bool {
        true
    }

    fn keeps_links(&self) -> bool {
        true
    }

    fn fetch(&self, _key: &str, hash: &str, size: u64) -> CloudResult<Vec<u8>> {
        fetch_blob(self.store, &self.prefix, hash, size)
    }

    fn fetch_hash(&self, hash: &str, size: u64) -> CloudResult<Vec<u8>> {
        fetch_blob(self.store, &self.prefix, hash, size)
    }

    /// A blob is named by its BLAKE3: bytes that were read whole and are not their name are
    /// damaged, whatever the index says now (another version is another blob), and a blob the
    /// index names that is not there is lost - the same repair, which writes the version's bytes
    /// under the version's own name and so cannot overwrite anything. A read that failed is no
    /// finding about the blob.
    fn read_back(&self, _key: &str, file: &RemoteFile, spool: Option<&Path>) -> verify::ReadBack {
        let blob = remote::blob_key(&self.prefix, &file.hash);
        let read = match spool {
            Some(dest) => match self.store.fetch_to_with(&blob, file.size, dest, &|| {}) {
                Ok(true) => local::hash_file(dest).map_err(CloudError::from),
                Ok(false) => return verify::ReadBack::Damaged,
                Err(e) => Err(e),
            },
            None => match self.store.fetch(&blob, file.size) {
                Ok(Some(bytes)) => Ok(local::hash_bytes(&bytes)),
                Ok(None) => return verify::ReadBack::Damaged,
                Err(e) => Err(e),
            },
        };
        match read {
            Ok(got) if got == file.hash => verify::ReadBack::Whole,
            Ok(_) => verify::ReadBack::Damaged,
            Err(e) => verify::ReadBack::Unread(e),
        }
    }

    /// The blob goes back under its own name (it cannot revert anything: the name is the
    /// content), from a file that is hashed as it is read and fails the upload when it is not
    /// the version's.
    fn heal(&self, _key: &str, file: &RemoteFile, from: &Path) -> CloudResult<()> {
        let mut body = HashCheck::new(fs::File::open(from)?, &file.hash);
        self.store.put_from(
            &remote::blob_key(&self.prefix, &file.hash),
            &mut body,
            file.size,
        )?;
        Ok(())
    }

    fn fetch_to(&self, key: &str, hash: &str, size: u64, dest: &Path) -> CloudResult<String> {
        self.fetch_to_with(key, hash, size, dest, &|| {})
    }

    fn fetch_to_with(
        &self,
        _key: &str,
        hash: &str,
        size: u64,
        dest: &Path,
        mid: &dyn Fn(),
    ) -> CloudResult<String> {
        let key = remote::blob_key(&self.prefix, hash);
        if !self.store.fetch_to_with(&key, size, dest, mid)? {
            fail!("the blob {key} is missing from the drive");
        }
        if local::hash_file(dest)? != hash {
            let _ = fs::remove_file(dest);
            fail!("the blob {key} is damaged: its BLAKE3 is not its name");
        }
        Ok(hash.to_string())
    }

    fn upload(&self, work: &Uploads<'_>, report: &mut SyncReport) -> CloudResult<BTreeSet<String>> {
        let mut known = lock(&self.known);
        upload_blobs(self.store, work, &mut known, report)
    }

    fn commit(
        &self,
        _remote: &RemoteIndex,
        next: &RemoteIndex,
        etag: Option<&str>,
        _report: &mut SyncReport,
    ) -> CloudResult<Option<String>> {
        self.store.put_if(&self.index_key, &next.to_bytes(), etag)
    }
}

/// Merges every [`Action::MergeJson`] file: this side's object, the drive's
/// and the base's (fetched by its hash; a blob collected since merges as an
/// empty object), all with the synced keys only.
fn merge_json_files<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    actions: &[Action],
    base: &BTreeMap<String, BaseEntry>,
    remote: &RemoteIndex,
    report: &mut SyncReport,
) -> CloudResult<BTreeMap<String, Merged>> {
    let mut out = BTreeMap::new();
    for action in actions {
        let Action::MergeJson { key } = action else {
            continue;
        };
        let object = |bytes: &[u8]| {
            local::json_object(bytes)
                .map(local::without_local_keys)
                .unwrap_or_default()
        };
        // Read without following a link, as every file of the folder is.
        let current = read_here(&root.path, key).ok_or_else(|| {
            CloudError::failed(format!(
                "{key} cannot be read here (it changed or is a link)"
            ))
        })?;
        let theirs_file = remote
            .files
            .get(key)
            .ok_or_else(|| CloudError::failed(format!("{key} vanished from the index")))?;
        let theirs_blob = remote_side.fetch(key, &theirs_file.hash, theirs_file.size)?;
        let theirs = object(theirs_blob.as_slice());
        let base_object = match base.get(key) {
            Some(entry) => match remote_side.fetch_hash(&entry.hash, 0) {
                Ok(bytes) => object(bytes.as_slice()),
                Err(_) => Map::new(),
            },
            None => Map::new(),
        };
        let ours = object(current.as_slice());
        let (merged, clashes) = merge::merge_json(&base_object, &ours, &theirs);
        let blob = local::pretty(&Value::Object(merged));
        let file = local::json_with_local_keys(&blob, Some(current.as_slice()))
            .unwrap_or_else(|| blob.clone());
        report
            .merge_clashes
            .extend(clashes.into_iter().map(|k| format!("{key}: {k}")));
        out.insert(
            key.clone(),
            Merged {
                hash: local::hash_bytes(&blob),
                blob,
                file,
            },
        );
    }
    Ok(out)
}

/// Where a blob's bytes come from.
enum BlobSource {
    File(PathBuf),
    /// A JSON file merged key by key: read from the scanned file ([`BlobJob::found`]).
    JsonFile,
    Bytes(Vec<u8>),
}

/// The file a job reads, as the scan found it: opened again from the folder's root without a
/// link, and checked to be the same file, untouched.
struct Found {
    root: PathBuf,
    /// Its name here ([`Scan::here`]).
    key: String,
    file: LocalFile,
}

struct BlobJob {
    key: String,
    hash: String,
    size: u64,
    /// The file's modification time as the scan found it (nanoseconds; 0 for bytes made here):
    /// a big file's resumable upload checks it is still that file.
    mtime_ns: u64,
    source: BlobSource,
    /// The file it reads (`None` for bytes made here).
    found: Option<Found>,
}

/// Opens the file of a job without following a link and without blocking on a FIFO, and checks
/// it is the file the scan hashed (inode, size and times): a link or a FIFO swapped in since
/// is never read.
fn open_found(found: &Found) -> Result<(fs::File, Stat), String> {
    let changed = || String::from("changed while it was synced");
    let tree = beneath::Root::open(&found.root).map_err(|e| format!("cannot be read ({e})"))?;
    // The scan's entry: on this disk already, walked to and never made.
    let (dir, name) = match names::walk_entry(&tree, &found.key) {
        Ok(found) => found,
        Err(e) if e.is_not_found() => return Err(String::from("vanished while it was synced")),
        Err(_) => return Err(changed()),
    };
    match dir.open_read(&name) {
        Ok((file, st)) if found.file.matches(&st) => Ok((file, st)),
        Ok(_) | Err(BeneathError::ThroughLink(_)) => Err(changed()),
        Err(e) if e.is_not_found() => Err(String::from("vanished while it was synced")),
        Err(e) => Err(format!("cannot be read ({e})")),
    }
}

/// After a file was read for an upload: it must still be the file opened, untouched, and not
/// deleted meanwhile (an open file still reads after its last name is gone).
fn still_found(file: &fs::File, opened: &Stat) -> Result<(), String> {
    match beneath::stat_of(file) {
        Ok(now)
            if now.nlink > 0
                && now.size == opened.size
                && now.mtime_ns == opened.mtime_ns
                && now.ctime_ns == opened.ctime_ns =>
        {
            Ok(())
        }
        _ => Err(String::from("changed while it was synced")),
    }
}

/// The bytes of a job's file, read whole and checked.
fn read_found(found: &Found) -> Result<Vec<u8>, String> {
    let (mut file, opened) = open_found(found)?;
    let mut bytes = Vec::with_capacity(usize::try_from(opened.size).unwrap_or(0));
    std::io::Read::read_to_end(&mut file, &mut bytes)
        .map_err(|e| format!("cannot be read ({e})"))?;
    still_found(&file, &opened)?;
    Ok(bytes)
}

/// The file a job streams up (a big one), opened and checked as [`open_found`] does.
fn open_job_file(job: &BlobJob) -> Result<(fs::File, Stat), String> {
    match &job.found {
        Some(found) => open_found(found),
        None => Err(String::from("vanished while it was synced")),
    }
}

enum Outcome {
    Uploaded(u64),
    Existed,
    /// The file changed (or vanished) since the scan.
    Changed(String),
    /// The run was stopped before it.
    Cancelled,
}

/// A file read for a streamed upload, hashed as it is read: at its end a content that is not
/// the one expected fails the read - the upload stops before it is completed (no blob under a
/// name its content does not have) - and [`CheckedRead::changed`] says why.
struct CheckedRead {
    file: fs::File,
    hasher: blake3::Hasher,
    expected: String,
    read: u64,
    ended: bool,
    changed: bool,
}

impl CheckedRead {
    fn new(file: fs::File, expected: &str) -> CheckedRead {
        CheckedRead {
            file,
            hasher: blake3::Hasher::new(),
            expected: expected.to_string(),
            read: 0,
            ended: false,
            changed: false,
        }
    }

    /// The file's content was not the one expected.
    fn changed(&self) -> bool {
        self.changed
    }

    /// The bytes read so far.
    fn bytes(&self) -> u64 {
        self.read
    }
}

impl std::io::Read for CheckedRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let changed = || {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the file changed while it was synced",
            )
        };
        if self.changed {
            return Err(changed());
        }
        if self.ended {
            return Ok(0);
        }
        let n = std::io::Read::read(&mut self.file, buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
            self.read += n as u64;
            return Ok(n);
        }
        self.ended = true;
        if self.hasher.finalize().to_hex().as_str() != self.expected {
            self.changed = true;
            return Err(changed());
        }
        Ok(0)
    }
}

/// A file in the folder's spool (`.azlin/sync/tmp`, on the folder's own disk) a big download
/// comes into, named after what it holds (a download cut short resumes there next time);
/// removed when dropped, unless it was renamed into place.
struct TempBlob(PathBuf);

impl TempBlob {
    /// The spool file for `key` at `hash` under the folder `root` (the spool is made when
    /// missing).
    fn for_download(root: &Path, key: &str, hash: &str) -> CloudResult<TempBlob> {
        let tree = beneath::Root::open(root)?;
        root::spool(&tree)?;
        let name = blake3::hash(format!("{key}\n{hash}").as_bytes()).to_hex();
        let mut path = root.to_path_buf();
        for segment in root::SPOOL_DIR.split('/') {
            path.push(segment);
        }
        Ok(TempBlob(path.join(format!(
            "azcloud-sync-{}.blob",
            &name.as_str()[..32]
        ))))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Its name in the spool.
    fn name(&self) -> String {
        self.0
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

impl Drop for TempBlob {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// What came down for a file: its bytes, or (a big one) a file holding them and their hash.
enum Blob {
    Bytes(Vec<u8>),
    File { file: TempBlob, hash: String },
}

/// The bytes a job uploads, unless its file changed since the scan.
fn job_bytes(job: &BlobJob) -> Result<Vec<u8>, String> {
    let bytes = match &job.source {
        BlobSource::File(_) => match &job.found {
            Some(found) => read_found(found)?,
            None => return Err(String::from("vanished while it was synced")),
        },
        BlobSource::JsonFile => {
            let raw = match &job.found {
                Some(found) => read_found(found)?,
                None => return Err(String::from("vanished while it was synced")),
            };
            match local::json_blob(&raw) {
                Some(bytes) => bytes,
                None => return Err(String::from("no longer a JSON object")),
            }
        }
        BlobSource::Bytes(bytes) => bytes.clone(),
    };
    if local::hash_bytes(&bytes) != job.hash {
        return Err(String::from("changed while it was synced"));
    }
    Ok(bytes)
}

fn upload_one<S: RemoteStore + ?Sized>(
    store: &S,
    prefix: &str,
    job: &BlobJob,
) -> CloudResult<Outcome> {
    // A big file goes up from its file, part by part: an upload a dropped connection or a
    // killed app cut off goes on next time with the parts it lacks, and it never completes
    // unless the file still holds the bytes its name says (hashed whole once more).
    if let BlobSource::File(path) = &job.source {
        if job.size > BIG_BLOB {
            let key = remote::blob_key(prefix, &job.hash);
            if blob_is_whole(store, &key, job.size)? {
                return Ok(Outcome::Existed);
            }
            // Held open for the whole upload: it is the scanned file (no link, no FIFO), and
            // still that file, undeleted, when the upload is done.
            let (held, opened) = match open_job_file(job) {
                Ok(open) => open,
                Err(why) => return Ok(Outcome::Changed(why)),
            };
            // The file held open above: the upload below opens it again by its name and
            // refuses any other file there (another device or inode).
            let check = FileCheck {
                size: job.size,
                modified_ns: job.mtime_ns,
                blake3: job.hash.clone(),
                dev: opened.dev,
                ino: opened.ino,
            };
            let sent = match store.put_file(&key, path, &check) {
                Ok(Some(sent)) => sent,
                Ok(None) => {
                    return Ok(Outcome::Changed(String::from(
                        "changed while it was synced",
                    )))
                }
                // A file that changed or vanished since the scan waits for the next run; any
                // other failure is the upload's.
                Err(e) => {
                    return match still_found(&held, &opened) {
                        Ok(()) => Err(e),
                        Err(why) => Ok(Outcome::Changed(why)),
                    }
                }
            };
            return match still_found(&held, &opened) {
                Ok(()) => Ok(Outcome::Uploaded(sent)),
                Err(why) => Ok(Outcome::Changed(why)),
            };
        }
    }
    let bytes = match job_bytes(job) {
        Ok(bytes) => bytes,
        Err(why) => return Ok(Outcome::Changed(why)),
    };
    let key = remote::blob_key(prefix, &job.hash);
    let n = bytes.len() as u64;
    if n > BIG_BLOB && blob_is_whole(store, &key, n)? {
        return Ok(Outcome::Existed);
    }
    store.put(&key, &bytes)?;
    Ok(Outcome::Uploaded(n))
}

/// Whether the drive holds the blob `key` WHOLE: an object of the size the blob has. A blob is
/// named by its BLAKE3, so its name proves nothing of the bytes the drive holds under it: one
/// that was cut short or damaged on the drive is there, the wrong size, and is sent again
/// (AZ-S22, X-AR5a-16); counting its presence as "uploaded" put an index entry on a blob that
/// reads back as an error.
fn blob_is_whole<S: RemoteStore + ?Sized>(store: &S, key: &str, size: u64) -> CloudResult<bool> {
    Ok(store.head(key)? == Some(size))
}

/// [`upload_one`], told as it moves and skipped once the run is stopped.
fn upload_told<S: RemoteStore + ?Sized>(
    store: &S,
    prefix: &str,
    job: &BlobJob,
    hooks: &RunHooks<'_>,
) -> CloudResult<Outcome> {
    if hooks.cancelled() {
        return Ok(Outcome::Cancelled);
    }
    hooks.tell(SyncEvent::Started {
        key: job.key.clone(),
        up: true,
        bytes: job.size,
    });
    hooks.point("mid-read");
    let outcome = upload_one(store, prefix, job);
    if matches!(outcome, Ok(Outcome::Uploaded(_))) {
        hooks.point("after-blob");
    }
    let error = match &outcome {
        Ok(Outcome::Changed(why)) => Some(why.clone()),
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };
    hooks.tell(SyncEvent::Finished {
        key: job.key.clone(),
        up: true,
        bytes: job.size,
        error,
    });
    outcome
}

/// The hash an action puts into the index, if it puts one.
fn uploaded_hash<'a>(
    action: &'a Action,
    scan: &'a Scan,
    merged: &'a BTreeMap<String, Merged>,
) -> Option<&'a str> {
    match action {
        Action::Upload { key } | Action::Conflict { key, .. } => {
            scan.files.get(key).map(|f| f.hash.as_str())
        }
        Action::MergeJson { key } => merged.get(key).map(|m| m.hash.as_str()),
        _ => None,
    }
}

/// The job that uploads what `action` sends to the drive (its key is the action's).
fn blob_job(action: &Action, work: &Uploads<'_>, hash: &str) -> BlobJob {
    let key = action.key();
    let (size, source, found) = match action {
        Action::MergeJson { .. } => {
            let m = &work.merged[key];
            (m.blob.len() as u64, BlobSource::Bytes(m.blob.clone()), None)
        }
        // A link's content is its target, as the scan read it: the link is never opened.
        _ if work.scan.files[key].link.is_some() => {
            let target = work.scan.files[key].link.clone().unwrap_or_default();
            (target.len() as u64, BlobSource::Bytes(target.into_bytes()), None)
        }
        _ => {
            let file = &work.scan.files[key];
            let here = work.scan.here(key);
            let path = local::path_of(&work.root.path, here);
            let source = match file.content {
                Content::Raw => BlobSource::File(path),
                Content::JsonMerge => BlobSource::JsonFile,
            };
            let found = Found {
                root: work.root.path.clone(),
                key: here.to_string(),
                file: file.clone(),
            };
            (file.blob_size, source, Some(found))
        }
    };
    let mtime_ns = work.scan.files.get(key).map_or(0, |f| f.mtime_ns);
    BlobJob {
        key: key.to_string(),
        hash: hash.to_string(),
        size,
        mtime_ns,
        source,
        found,
    }
}

/// Uploads every blob the plan needs that the drive may not hold; the keys
/// whose upload has to wait for the next run (their file changed, the run was stopped).
fn upload_blobs<S: RemoteStore + ?Sized>(
    store: &S,
    work: &Uploads<'_>,
    known: &mut BTreeSet<String>,
    report: &mut SyncReport,
) -> CloudResult<BTreeSet<String>> {
    let (actions, scan, merged, remote) = (work.actions, work.scan, work.merged, work.remote);
    let mut jobs = Vec::new();
    let mut queued: BTreeSet<String> = BTreeSet::new();
    for action in actions {
        let Some(hash) = uploaded_hash(action, scan, merged) else {
            continue;
        };
        if remote.has_blob(hash) || known.contains(hash) {
            report.blobs_reused += 1;
            continue;
        }
        // The same content under two names travels once.
        if !queued.insert(hash.to_string()) {
            continue;
        }
        jobs.push(blob_job(action, work, hash));
    }
    let (small, big): (Vec<BlobJob>, Vec<BlobJob>) =
        jobs.into_iter().partition(|j| j.size <= BIG_BLOB);
    let prefix = work.opts.prefix.as_str();
    let hooks = work.hooks;
    let mut outcomes: Vec<(String, String, CloudResult<Outcome>)> =
        each_in_parallel(&small, work.opts.parallel, |job| {
            (
                job.key.clone(),
                job.hash.clone(),
                upload_told(store, prefix, job, hooks),
            )
        });
    // A big blob goes alone: its parts travel in parallel already.
    for job in &big {
        let outcome = upload_told(store, prefix, job, hooks);
        outcomes.push((job.key.clone(), job.hash.clone(), outcome));
    }
    let mut abandoned = BTreeSet::new();
    // The first upload that failed by itself (no connection, a busy node), and whether any
    // went through.
    let mut passing: Option<CloudError> = None;
    let mut any_sent = false;
    for (key, hash, outcome) in outcomes {
        let outcome = match outcome {
            Ok(outcome) => outcome,
            // A dropped connection, a busy node, a server that fails this one object, a file
            // the drive refuses for itself (too large), a full or unpaid drive that takes no
            // more files (its reads and deletes still sync): this file waits for the next run,
            // listed with its reason, and the others reach the drive now. What the user must
            // act on before anything works (sign in again, unlock) fails the run, which says it.
            Err(e) if !stops_the_run(&e) => {
                listed_upload_failure(report, &key, &e);
                abandoned.insert(key);
                if passes(&e) {
                    passing.get_or_insert(e);
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        match outcome {
            Outcome::Uploaded(bytes) => {
                known.insert(hash);
                report.blobs_up += 1;
                report.bytes_up += bytes;
                any_sent = true;
            }
            Outcome::Existed => {
                known.insert(hash);
                report.blobs_reused += 1;
                any_sent = true;
            }
            Outcome::Changed(why) => {
                report.changed_during_sync.push(format!("{key}: {why}"));
                abandoned.insert(key);
            }
            Outcome::Cancelled => {
                abandoned.insert(key);
            }
        }
    }
    // Nothing went through at all: the run fails with it, and the app says the drive cannot be
    // reached (a node down, no connection) instead of "files cannot sync".
    if let Some(e) = passing {
        if !any_sent {
            return Err(e);
        }
    }
    // An action whose content no upload confirmed (its twin under another
    // name changed during the run) waits for the next run too.
    for action in actions {
        if let Some(hash) = uploaded_hash(action, scan, merged) {
            if !remote.has_blob(hash) && !known.contains(hash) {
                abandoned.insert(action.key().to_string());
            }
        }
    }
    Ok(abandoned)
}

/// Whether `e` passes by itself (the errors table's retry class: no connection, a busy or
/// updating node): one file's such failure is no failed run.
fn passes(e: &CloudError) -> bool {
    UserError::from_cloud_error(e).is_some_and(|user| user.class() == Class::Retry)
}

/// The upload of `key` failed with `e` and the file waits: said in the report, listed with its
/// code.
fn listed_upload_failure(report: &mut SyncReport, key: &str, e: &CloudError) {
    report.errors.push(format!("upload {key}: {e}"));
    report.file_errors.push((key.to_string(), e.to_string()));
    report.problems.push(problems::Problem::of_error(key, e));
}

/// Whether an upload error stops the whole run: this device's access to the drive is gone (it
/// must sign in again or unlock the drive), or the error is no answer of the drive at all. Any
/// other is about the file (too large, a server failing this object), the moment (a busy
/// node) or the drive's writes (full, unpaid, locked down: reads and deletes go on): the file
/// waits, listed, and the rest of the run goes on.
fn stops_the_run(e: &CloudError) -> bool {
    UserError::from_cloud_error(e).is_none_or(|user| {
        user.class() == Class::ReAuth
            || matches!(
                user.behaviour(),
                Behaviour::SignInAgain | Behaviour::RefreshSession | Behaviour::Unlock
            )
    })
}

/// The index after `actions`: `remote` itself when they change nothing in it
/// (so an unchanged folder writes no index). `moved`: `remote` has folders this run renamed
/// here moved already, a change of its own.
#[allow(clippy::too_many_arguments)]
fn next_index(
    remote: &RemoteIndex,
    moved: bool,
    actions: &[Action],
    folders: &[FolderAction],
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
    device: &str,
    now: i64,
    expire_before: Option<i64>,
    confirmed: &dyn Fn(&str) -> bool,
) -> RemoteIndex {
    let mut next = remote.clone();
    let generation = remote.generation + 1;
    let entry = |hash: &str, size: u64, mtime_ns: u64, exec: bool| RemoteFile {
        hash: hash.to_string(),
        size,
        mtime: i64::try_from(mtime_ns / 1_000_000_000).unwrap_or(i64::MAX),
        mtime_ns,
        gen: generation,
        device: device.to_string(),
        exec,
        link: None,
    };
    // A file of the scan as the index keeps it: a link with its target.
    let scanned = |f: &LocalFile| RemoteFile {
        link: f.link.clone(),
        ..entry(&f.hash, f.blob_size, f.mtime_ns, f.exec)
    };
    let mut changed = moved;
    for action in actions {
        match action {
            Action::Upload { key } => {
                if let Some(f) = scan.files.get(key) {
                    next.files.insert(key.clone(), scanned(f));
                    next.deleted.remove(key);
                    changed = true;
                }
            }
            Action::Conflict { key, copy } => {
                if let Some(f) = scan.files.get(key) {
                    next.files.insert(copy.clone(), scanned(f));
                    next.deleted.remove(copy);
                    changed = true;
                }
            }
            Action::MergeJson { key } => {
                if let Some(m) = merged.get(key) {
                    let same = remote.files.get(key).is_some_and(|f| f.hash == m.hash);
                    if !same {
                        let now_ns = u64::try_from(now)
                            .unwrap_or(0)
                            .saturating_mul(1_000_000_000);
                        let exec = scan.files.get(key).is_some_and(|f| f.exec);
                        next.files.insert(
                            key.clone(),
                            entry(&m.hash, m.blob.len() as u64, now_ns, exec),
                        );
                        changed = true;
                    }
                }
            }
            Action::DeleteRemote { key } => {
                if let Some(old) = next.files.remove(key) {
                    next.deleted.insert(
                        key.clone(),
                        Tombstone {
                            hash: old.hash,
                            gen: generation,
                            at: now,
                            device: device.to_string(),
                            confirmed: confirmed(key),
                        },
                    );
                    changed = true;
                }
            }
            Action::Agree { .. }
            | Action::Download { .. }
            | Action::DeleteLocal { .. }
            | Action::Forget { .. }
            | Action::CloudOnly { .. } => {}
        }
    }
    for action in folders {
        match action {
            FolderAction::Send { key, id } => {
                let entry = RemoteFolder {
                    id: id.clone(),
                    gen: generation,
                    device: device.to_string(),
                };
                next.folders.insert(key.clone(), entry);
                next.deleted_folders.remove(key);
                changed = true;
            }
            FolderAction::RemoveThere { key, id } => {
                if next.folders.remove(key).is_some() {
                    let tomb = FolderTombstone {
                        id: id.clone(),
                        gen: generation,
                        at: now,
                        device: device.to_string(),
                    };
                    next.deleted_folders.insert(key.clone(), tomb);
                    changed = true;
                }
            }
            FolderAction::Agree { .. }
            | FolderAction::Make { .. }
            | FolderAction::RemoveHere { .. }
            | FolderAction::Forget { .. } => {}
        }
    }
    if changed {
        // The tombstones that are older than the clock a believed reading gave are dropped; with
        // no such reading none is (AZ-S12: a clock a year ahead dropped them all at its commit).
        if let Some(horizon) = expire_before {
            next.deleted.retain(|_, t| t.at >= horizon);
            next.deleted_folders.retain(|_, t| t.at >= horizon);
        }
        // An index with folders says so: a version that does not know them would drop them.
        let has_folders = !next.folders.is_empty() || !next.deleted_folders.is_empty();
        let has_links = next.files.values().any(|f| f.link.is_some());
        for (has, feature) in [
            (has_folders, remote::FOLDERS_FEATURE),
            (has_links, remote::LINKS_FEATURE),
        ] {
            if has && !next.features.iter().any(|f| f == feature) {
                next.features.push(feature.to_string());
            }
        }
        next.generation = generation;
        next.updated_at = now;
        next.updated_by = device.to_string();
    }
    next
}

/// A download to apply: the key, the blob, and what the file must still be
/// (`None`: absent) for the write to go ahead.
struct Download {
    key: String,
    hash: String,
    size: u64,
    expect: Option<LocalFile>,
    /// The device that wrote the drive's version (quarantine marks on other people's files).
    writer: String,
    /// The drive's executable bit and modification time (nanoseconds; 0: none) for the new
    /// file.
    exec: bool,
    mtime_ns: u64,
}

/// The problem a failed step on this disk is.
pub(crate) fn problem_of(key: &str, e: &BeneathError) -> problems::Problem {
    use problems::{Code, Problem};
    match e {
        BeneathError::ThroughLink(at) => Problem::new(
            Code::ThroughLink,
            key,
            format!("\"{at}\" here is a link or no folder; nothing is written through it"),
        ),
        BeneathError::CrossesMount(at) => Problem::new(
            Code::MountPoint,
            key,
            format!("\"{at}\" is another disk; nothing is written into it"),
        ),
        e if e.is_disk_full() => Problem::new(
            Code::DiskFull,
            key,
            "the disk is full; the old version stays as it was",
        ),
        // Another app has the file or the name open (Windows' sharing and lock violations,
        // which std files under no kind of their own - AZ-W11 - and a refused access).
        BeneathError::Io(io)
            if io.kind() == std::io::ErrorKind::PermissionDenied || beneath::is_held_open(io) =>
        {
            Problem::new(
                Code::Locked,
                key,
                format!("it cannot be replaced ({io}); the next pass tries again"),
            )
        }
        other => Problem::new(Code::IoError, key, other.to_string()),
    }
}

/// A step on this disk that failed for `key`: said in the report's sentences and its problems.
fn failed_here(report: &mut SyncReport, verb: &str, key: &str, e: &BeneathError) {
    report.errors.push(format!("{verb} {key}: {e}"));
    report.file_errors.push((key.to_string(), e.to_string()));
    report.problems.push(problem_of(key, e));
}

/// The bytes of the file of `key` under `root`, read without following a link.
fn read_here(root: &Path, key: &str) -> Option<Vec<u8>> {
    let tree = beneath::Root::open(root).ok()?;
    let (dir, name) = names::walk_record(&tree, key).ok()?;
    let (mut file, _) = dir.open_read(&name).ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes).ok()?;
    Some(bytes)
}

/// Why a new version was not renamed in.
enum Replace {
    /// The file here changed since the scan (or a file turned up): the next run takes it.
    Changed,
    Failed(BeneathError),
}

impl From<PublishError> for Replace {
    fn from(e: PublishError) -> Replace {
        match e {
            PublishError::Changed => Replace::Changed,
            PublishError::Walk(e) => Replace::Failed(e),
            other => Replace::Failed(BeneathError::Io(other.into())),
        }
    }
}

/// Whether the file `name` of `dir` is still what the scan found (`None`: still absent).
fn still_there(dir: &beneath::Dir, name: &str, expect: Option<&LocalFile>) -> bool {
    match (dir.lstat(name), expect) {
        (Err(e), None) => e.kind() == std::io::ErrorKind::NotFound,
        (Ok(st), Some(file)) => file.matches(&st),
        _ => false,
    }
}

/// How the sync puts a new version in place ([`azul_storage::durable`]): the name must still hold
/// what `expect` says, the permission bits follow the sync's rule (the old file's read and write
/// bits, the executable bit when the drive's version has it, never setuid, setgid or sticky: a
/// remote entry brings no permission of its own), the drive's modification time when it has one
/// (the merge never decides by it, people and other tools do).
fn synced_publish(expect: Expect, exec: bool, mtime_ns: u64) -> Publish<'static> {
    let publish = Publish::new(
        expect,
        Mode::Synced {
            exec: exec && merge::TRACKS_EXEC,
        },
        &Quiet,
    );
    if mtime_ns > 0 {
        publish.with_mtime(std::time::UNIX_EPOCH + Duration::from_nanos(mtime_ns))
    } else {
        publish
    }
}

/// [`synced_publish`] for a name that must still hold the file the scan found (`None`: nothing).
fn publish_of(expect: Option<&LocalFile>, exec: bool, mtime_ns: u64) -> Publish<'static> {
    let expect = match expect {
        Some(file) => Expect::Is(file.fingerprint()),
        None => Expect::Absent,
    };
    synced_publish(expect, exec, mtime_ns)
}

/// Writes a new version of `name` in `dir` from `body`: into a new temporary file in the same
/// folder (the same disk, never the system's temporary folder), with its permission bits and
/// time, synced to disk; then, if the file is still what the scan found, renamed over it and
/// the folder synced. The old version is whole until the rename, a crash leaves the old or the
/// new one, and a failure (a full disk) leaves no temporary file. The bytes written.
fn replace_checked(
    dir: &beneath::Dir,
    name: &str,
    expect: Option<&LocalFile>,
    body: &mut dyn std::io::Read,
    exec: bool,
    mtime_ns: u64,
) -> Result<u64, Replace> {
    durable::publish(dir, name, body, publish_of(expect, exec, mtime_ns))
        .map(|landed| landed.len)
        .map_err(Replace::from)
}

/// [`replace_checked`] for a big download already in the spool (`spool`, its file `blob`):
/// its permission bits and time set, synced, then renamed straight over `name` if that is
/// still what the scan found. The bytes it holds.
fn replace_from_spool(
    tree: &beneath::Root,
    blob: &TempBlob,
    dir: &beneath::Dir,
    name: &str,
    expect: Option<&LocalFile>,
    exec: bool,
    mtime_ns: u64,
) -> Result<u64, Replace> {
    let spool = root::spool(tree).map_err(Replace::Failed)?;
    let spooled = blob.name();
    let blob = durable::Blob {
        dir: &spool,
        name: &spooled,
    };
    durable::publish_staged(dir, name, blob, publish_of(expect, exec, mtime_ns))
        .map(|landed| landed.len)
        .map_err(Replace::from)
}

/// The files the drive moved, as this plan sees them: a delete here whose content the plan
/// downloads under a name that is free here. The download's key and the delete's.
fn local_moves<'a>(
    actions: &'a [Action],
    scan: &Scan,
    remote: &RemoteIndex,
) -> BTreeMap<&'a str, &'a str> {
    let mut leaving: BTreeMap<(&str, bool), Vec<&'a str>> = BTreeMap::new();
    for action in actions {
        if let Action::DeleteLocal { key } = action {
            if let Some(file) = scan.files.get(key) {
                leaving
                    .entry((file.hash.as_str(), file.exec))
                    .or_default()
                    .push(key.as_str());
            }
        }
    }
    let mut out = BTreeMap::new();
    for action in actions {
        let Action::Download { key } = action else {
            continue;
        };
        if scan.files.contains_key(key) {
            continue;
        }
        let Some(file) = remote.files.get(key) else {
            continue;
        };
        if let Some(old) = leaving
            .get_mut(&(file.hash.as_str(), file.exec))
            .and_then(Vec::pop)
        {
            out.insert(key.as_str(), old);
        }
    }
    out
}

/// Renames the file of `from` here to `to` (the drive moved it), never through a link: the
/// source still as scanned, the target's name still free (or, on a disk that ignores case, the
/// same file under another case - renamed through a third name). Its base entry under the new
/// name; `None` when it changed meanwhile (the next run takes it).
fn move_here(
    tree: &beneath::Root,
    drive: &LocalDrive,
    root: &LocalRoot,
    scan: &Scan,
    policy: &names::Policy,
    from: &str,
    to: &str,
    hooks: &RunHooks<'_>,
) -> Result<Option<BaseEntry>, BeneathError> {
    let from_here = scan.here(from);
    let Some(expect) = scan.files.get(from) else {
        return Ok(None);
    };
    let (source, source_name) = names::walk_entry(tree, from_here)?;
    let source_name = source_name.as_str();
    if !still_there(&source, source_name, Some(expect)) {
        return Ok(None);
    }
    // `to` is the DRIVE's new name: it passes the rows of a synced name before a folder of it is
    // made (X-DELTA-AZDRIVE-FILES-1).
    let (target, target_name) = names::make_parent_of(tree, policy, to)?;
    let target_name = target_name.as_str();
    // Another name for the same file: a case-only rename on a disk that ignores case. Only a
    // known identity says it is the same file; another file, or one whose identity cannot be
    // told, is a name that is taken (the next run takes it from there).
    let same_file = match target.lstat(target_name) {
        Ok(st) => {
            if !beneath::FileId::new(expect.dev, expect.ino)
                .same(&st.id())
                .is_same()
            {
                return Ok(None);
            }
            true
        }
        Err(_) => false,
    };
    let via = same_file.then(|| beneath::temp_name(target_name));
    let step = engine::Step::Move {
        from,
        to,
        via: via.as_deref(),
    };
    let ticket = hooks.begin(&step).map_err(|e| {
        BeneathError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    // A move is a rename into place as a download's is: a crash on either side of it is one
    // the next run must converge from.
    hooks.point("before-rename");
    let moved = match &via {
        Some(via) => source
            .rename(source_name, &target, via)
            .and_then(|()| target.rename(via, &target, target_name)),
        None => source.rename(source_name, &target, target_name),
    };
    if moved.is_ok() {
        target.sync();
        source.sync();
    }
    hooks.finished(&step, ticket, moved.is_ok(), None);
    hooks.point("after-rename");
    moved?;
    drive.note_renamed(from_here, to);
    Ok(local::entry_now(&root.path, to, &expect.hash))
}

/// What the disk reserve does when the disk cannot say its free space: the downloads go on and the
/// report says that the reserve was not enforced. Every system has a probe now (Windows'
/// `GetDiskFreeSpaceExW`, RF49), so this is a disk that will not say - a network share, a sandbox
/// -, and failing closed would lock a user out of a drive on a share that never answers; the files a
/// full disk cannot take still fail one by one, and the report lists them.
const FREE_SPACE_POSTURE: Posture = Posture::Degrade(
    "the free space of this disk cannot be read, so the reserve it keeps free is not enforced for \
     these downloads",
);

/// The free bytes the disk reserve is checked against for a pass over `root` (E3), and the
/// sentence for the report when the disk could not say: the disk of `disk_of` as a test harness
/// may play it (armed under `fault_root`, [`faults`]); else its own answer under
/// [`FREE_SPACE_POSTURE`].
pub fn free_for_reserve(fault_root: &Path, disk_of: &Path) -> (u64, Option<String>) {
    match faults::free_override(fault_root) {
        Some(bytes) => (bytes, None),
        None => free_under(beneath::free_space(disk_of), FREE_SPACE_POSTURE),
    }
}

/// `probe` as the bytes a reserve check uses, with what to tell the report when it was unknown:
/// as many as can ever be needed under [`Posture::Degrade`], none under [`Posture::FailClosed`]
/// (then no download fits).
pub(crate) fn free_under(probe: Cap<u64>, posture: Posture) -> (u64, Option<String>) {
    match probe {
        Cap::Known(bytes) => (bytes, None),
        Cap::Unknown(why) => match posture {
            Posture::Degrade(note) => (u64::MAX, Some(format!("{note} ({why})"))),
            Posture::FailClosed => (
                0,
                Some(format!(
                    "{why}: nothing is downloaded here, because the room it needs cannot be told"
                )),
            ),
        },
    }
}

/// Applies the committed index here and updates `base`.
#[allow(clippy::too_many_arguments)]
fn apply_local<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    actions: &[Action],
    committed: &RemoteIndex,
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
    volume: &names::Volume,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    use problems::Code;
    let drive = root.drive();
    // What a name from the drive must pass before a file or a folder is made under it here.
    let policy = names::Policy::here(*volume);
    // The folder, reached from here on without a link (it was checked at the start of the run).
    let tree = match beneath::Root::open(&root.path) {
        Ok(tree) => tree,
        Err(e) => {
            report.errors.push(format!(
                "{}: the folder cannot be opened ({e}); nothing was written here",
                root.path.display()
            ));
            return;
        }
    };
    let scanned_entry = |key: &str| scan.files.get(key).map(LocalFile::base_entry);
    let stamp = root::trash_stamp(crate::now());
    // E3: what the downloads planned so far take of the disk's free space.
    let (free, unread) = free_for_reserve(&root.path, &root.path);
    // The reserve was not enforced because the disk would not say its room: a problem of the pass
    // (a warning), not a note nothing shows (X-DELTA-AZDRIVE-FILES-33).
    if let Some(note) = unread {
        report.tell(Code::Unchecked, "", note);
    }
    let mut pending: u64 = 0;
    let mut fits = |key: &str, size: u64, report: &mut SyncReport| -> bool {
        if free.saturating_sub(pending) < size.saturating_add(opts.disk_reserve) {
            report.tell(
                Code::DiskReserve,
                key,
                format!(
                    "{size} bytes would leave less than the {} bytes this disk keeps free; it \
                     stays on the drive",
                    opts.disk_reserve
                ),
            );
            report.file_errors.push((
                key.to_string(),
                String::from("not enough free space on this disk"),
            ));
            return false;
        }
        pending = pending.saturating_add(size);
        true
    };
    let mut downloads = Vec::new();
    // A3: what the drive moved (gone under one name, the same content new under another) is
    // renamed here, never trashed and fetched again.
    let moved_here = local_moves(actions, scan, committed);
    let moving: BTreeSet<&str> = moved_here.values().copied().collect();
    // What this run deletes here before its downloads are written: a file the drive renamed by
    // case only comes down under its new name once the old one is gone.
    let leaving: BTreeSet<&str> = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteLocal { .. }))
        .map(Action::key)
        .collect();
    for action in actions {
        let key = action.key();
        let here = scan.here(key);
        let moved_on = || format!("{key}: changed during the run; the next run takes it");
        match action {
            Action::Agree { .. } | Action::Upload { .. } => {
                if let Some(entry) = scanned_entry(key) {
                    base.insert(key.to_string(), entry);
                }
                if matches!(action, Action::Agree { .. }) {
                    report.unchanged += 1;
                } else {
                    report.files_up += 1;
                }
            }
            Action::DeleteRemote { .. } => {
                base.remove(key);
                report.deleted_there += 1;
            }
            Action::Forget { .. } => {
                base.remove(key);
            }
            Action::CloudOnly { .. } => {
                if let Some(f) = committed.files.get(key) {
                    base.insert(
                        key.to_string(),
                        BaseEntry {
                            hash: f.hash.clone(),
                            size: f.size,
                            cloud_only: true,
                            exec: f.exec,
                            ..BaseEntry::default()
                        },
                    );
                }
            }
            // Moved here by the download of its new name.
            Action::DeleteLocal { .. } if moving.contains(key) => {}
            Action::Download { .. } if moved_here.contains_key(key) => {
                let from = moved_here[key];
                match move_here(&tree, &drive, root, scan, &policy, from, key, hooks) {
                    Ok(Some(entry)) => {
                        // The folder it left goes with it only where folders are no entries.
                        if !remote_side.keeps_folders() {
                            local::prune_empty_parents(&root.path, scan.here(from));
                        }
                        base.remove(from);
                        base.insert(key.to_string(), entry);
                        report.moved.push(format!("{from} -> {key}"));
                    }
                    Ok(None) => report.changed_during_sync.push(moved_on()),
                    Err(e) => failed_here(report, "move", key, &e),
                }
            }
            Action::DeleteLocal { .. } => {
                // D2: a delete here is a move into the folder's trash, never an unlink.
                let expect = scan.files.get(key);
                let (dir, name) = match names::walk_entry(&tree, here) {
                    Ok(found) => found,
                    Err(e) => {
                        failed_here(report, "delete", key, &e);
                        continue;
                    }
                };
                let name = name.as_str();
                if !(expect.is_some() && still_there(&dir, name, expect)) {
                    report.changed_during_sync.push(moved_on());
                    continue;
                }
                let (trash, target, rel) = match root::trash_target(&tree, here, &stamp) {
                    Ok(target) => target,
                    Err(e) => {
                        failed_here(report, "delete", key, &e);
                        continue;
                    }
                };
                let step = engine::Step::Delete {
                    key,
                    hash: expect.map(|f| f.hash.as_str()),
                    trash: Some(rel.as_str()),
                };
                hooks.point("before-trash");
                let ticket = match hooks.begin(&step) {
                    Ok(ticket) => ticket,
                    Err(e) => {
                        report.errors.push(format!("delete {key}: {e}"));
                        report.file_errors.push((key.to_string(), e.to_string()));
                        continue;
                    }
                };
                let moved = dir.rename(name, &trash, &target);
                if moved.is_ok() {
                    trash.sync();
                    dir.sync();
                }
                hooks.finished(&step, ticket, moved.is_ok(), None);
                match moved {
                    Ok(()) => {
                        drive.note_deleted(here);
                        // A drive that keeps folders deletes one only by its own delete: the
                        // folder of a deleted file stays, empty.
                        if !remote_side.keeps_folders() {
                            local::prune_empty_parents(&root.path, here);
                        }
                        base.remove(key);
                        report.deleted_here += 1;
                        report.trashed.push((key.to_string(), rel));
                    }
                    Err(e) => failed_here(report, "delete", key, &e),
                }
            }
            Action::Download { .. } => {
                let expect = scan.files.get(key).cloned();
                if expect.is_none() {
                    // A file new here whose name this disk already gives another file that
                    // stays: it stays on the drive, said, and is not fetched again every run.
                    let folder = key.rfind('/').map_or("", |i| &key[..=i]);
                    let twin = local::other_spelling(&root.path, here)
                        .filter(|other| !leaving.contains(format!("{folder}{other}").as_str()));
                    if let Some(other) = twin {
                        let why = same_name(&other);
                        report.errors.push(format!("download {key}: {why}"));
                        report.file_errors.push((key.to_string(), why.clone()));
                        report.tell(Code::NameCollision, key, why);
                        continue;
                    }
                }
                if let Some(f) = committed.files.get(key) {
                    // A link is made from its entry: its target is all of it.
                    if let Some(target) = &f.link {
                        let made = Made {
                            tree: &tree,
                            drive: &drive,
                            root,
                            scan,
                            policy: &policy,
                        };
                        write_link(&made, key, target, &f.hash, expect.as_ref(), base, report);
                        continue;
                    }
                    if !fits(key, f.size, report) {
                        continue;
                    }
                    downloads.push(Download {
                        key: key.to_string(),
                        hash: f.hash.clone(),
                        size: f.size,
                        expect,
                        writer: f.device.clone(),
                        exec: f.exec,
                        mtime_ns: f.modified_ns(),
                    });
                }
            }
            Action::Conflict { copy, .. } => {
                hooks.point("conflict-start");
                let expect = scan.files.get(key);
                let (dir, name) = match names::walk_entry(&tree, here) {
                    Ok(found) => found,
                    Err(e) => {
                        failed_here(report, "keep", key, &e);
                        continue;
                    }
                };
                let name = name.as_str();
                let copy_name = beneath::split_key(copy).1;
                // The copy's name must still be free; this side's version still as scanned.
                if !(expect.is_some() && still_there(&dir, name, expect))
                    || dir.lstat(copy_name).is_ok()
                {
                    report.changed_during_sync.push(moved_on());
                    continue;
                }
                let here_hash = expect.map_or("", |f| f.hash.as_str());
                let step = engine::Step::Conflict {
                    key,
                    copy,
                    hash: here_hash,
                };
                let ticket = match hooks.begin(&step) {
                    Ok(ticket) => ticket,
                    Err(e) => {
                        report.errors.push(format!("keep {key} as {copy}: {e}"));
                        report.file_errors.push((key.to_string(), e.to_string()));
                        continue;
                    }
                };
                let renamed = dir.rename(name, &dir, copy_name);
                if renamed.is_ok() {
                    dir.sync();
                }
                hooks.finished(&step, ticket, renamed.is_ok(), None);
                hooks.point("conflict-renamed");
                match renamed {
                    Ok(()) => {
                        let (folder, _) = beneath::split_key(here);
                        let copy_here = if folder.is_empty() {
                            copy_name.to_string()
                        } else {
                            format!("{folder}/{copy_name}")
                        };
                        drive.note_renamed(here, &copy_here);
                        let entry = local::entry_now(&root.path, &copy_here, here_hash)
                            .or_else(|| scanned_entry(key));
                        if let Some(entry) = entry {
                            base.insert(copy.clone(), entry);
                        }
                        report.conflicts.push(format!("{key} -> {copy}"));
                        report.files_up += 1;
                        if let Some(f) = committed.files.get(key) {
                            if let Some(target) = &f.link {
                                let made = Made {
                                    tree: &tree,
                                    drive: &drive,
                                    root,
                                    scan,
                                    policy: &policy,
                                };
                                write_link(&made, key, target, &f.hash, None, base, report);
                            } else if fits(key, f.size, report) {
                                downloads.push(Download {
                                    key: key.to_string(),
                                    hash: f.hash.clone(),
                                    size: f.size,
                                    expect: None,
                                    writer: f.device.clone(),
                                    exec: f.exec,
                                    mtime_ns: f.modified_ns(),
                                });
                            }
                        }
                    }
                    Err(e) => failed_here(report, "keep", key, &e),
                }
            }
            Action::MergeJson { .. } => {
                let Some(m) = merged.get(key) else { continue };
                let expect = scan.files.get(key);
                let written = faults::check_write(&root.path)
                    .map_err(|e| Replace::Failed(e.into()))
                    .and_then(|()| names::walk_record(&tree, here).map_err(Replace::Failed))
                    .and_then(|(dir, name)| {
                        let exec = expect.is_some_and(|f| f.exec);
                        replace_checked(&dir, &name, expect, &mut &m.file[..], exec, 0)
                    });
                match written {
                    Ok(_) => {
                        drive.note_written(here);
                        if let Some(entry) = local::entry_now(&root.path, here, &m.hash) {
                            base.insert(key.to_string(), entry);
                        }
                        report.merged.push(key.to_string());
                        report.files_up += 1;
                    }
                    Err(Replace::Changed) => report.changed_during_sync.push(moved_on()),
                    Err(Replace::Failed(e)) => failed_here(report, "write", key, &e),
                }
            }
        }
    }
    let (small, big): (Vec<Download>, Vec<Download>) =
        downloads.into_iter().partition(|d| d.size <= BIG_BLOB);
    // A download starts before its first byte is asked for: what a pause there lets happen
    // (the connection cut, the sign-in revoked) is what the fetch meets.
    let fetch = |d: &Download| {
        hooks.point("download-start");
        hooks.tell(SyncEvent::Started {
            key: d.key.clone(),
            up: false,
            bytes: d.size,
        });
        remote_side.fetch(&d.key, &d.hash, d.size)
    };
    if !small.is_empty() {
        // The blobs come in on threads of their own, at most `parallel` waiting to be
        // written; the files are written here, one at a time, as they arrive.
        let next = AtomicUsize::new(0);
        let (small, next, fetch) = (&small, &next, &fetch);
        std::thread::scope(|scope| {
            let (sender, fetched) = mpsc::sync_channel(opts.parallel.max(1));
            for _ in 0..opts.parallel.max(1).min(small.len()) {
                let sender = sender.clone();
                scope.spawn(move || loop {
                    // A stopped run fetches nothing more: what is left waits for the next one.
                    if hooks.cancelled() {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(d) = small.get(i) else {
                        break;
                    };
                    let blob = fetch(d);
                    if sender.send((i, blob)).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            for (i, blob) in fetched {
                let blob = blob.map(Blob::Bytes);
                write_download(
                    &tree, &drive, root, scan, &policy, &small[i], blob, hooks, base, report,
                );
            }
        });
    }
    for d in &big {
        if hooks.cancelled() {
            break;
        }
        // A big file comes down into the folder's spool on its own disk, never whole in memory
        // (a JSON-merge file is small).
        let blob = if root.json_merge.iter().any(|k| *k == d.key) {
            fetch(d).map(Blob::Bytes)
        } else {
            hooks.point("download-start");
            hooks.tell(SyncEvent::Started {
                key: d.key.clone(),
                up: false,
                bytes: d.size,
            });
            TempBlob::for_download(&root.path, &d.key, &d.hash).and_then(|file| {
                let mid = || hooks.point("mid-download");
                remote_side
                    .fetch_to_with(&d.key, &d.hash, d.size, file.path(), &mid)
                    .map(|hash| Blob::File { file, hash })
            })
        };
        write_download(
            &tree, &drive, root, scan, &policy, d, blob, hooks, base, report,
        );
    }
}

/// Writes one fetched blob to its file (a JSON-merge file with this computer's machine-local
/// keys put back) through a new file renamed in ([`replace_checked`]), unless the file changed
/// since the scan.
#[allow(clippy::too_many_arguments)]
fn write_download(
    tree: &beneath::Root,
    drive: &LocalDrive,
    root: &LocalRoot,
    scan: &Scan,
    policy: &names::Policy,
    d: &Download,
    blob: CloudResult<Blob>,
    hooks: &RunHooks<'_>,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    let finished = |error: Option<String>| {
        hooks.tell(SyncEvent::Finished {
            key: d.key.clone(),
            up: false,
            bytes: d.size,
            error,
        });
    };
    let here = scan.here(&d.key).to_string();
    let blob = match blob {
        Ok(blob) => blob,
        Err(e) => {
            report.errors.push(format!("download {}: {e}", d.key));
            report.file_errors.push((d.key.clone(), e.to_string()));
            report
                .problems
                .push(problems::Problem::of_error(&d.key, &e));
            finished(Some(e.to_string()));
            return;
        }
    };
    let hash = match &blob {
        Blob::Bytes(bytes) => content_hash(&d.hash, bytes),
        Blob::File { hash, .. } => hash.clone(),
    };
    let blob = match blob {
        Blob::Bytes(bytes) if root.json_merge.iter().any(|k| *k == d.key) => {
            let current = read_here(&root.path, &here);
            Blob::Bytes(local::json_with_local_keys(&bytes, current.as_deref()).unwrap_or(bytes))
        }
        other => other,
    };
    // The folder of the file, made where missing, never through a link or into another disk - for a
    // name that passed the rows of a name from the drive (X-DELTA-AZDRIVE-FILES-1).
    let (dir, name) = match names::make_parent_of(tree, policy, &here) {
        Ok(found) => found,
        Err(e) => {
            failed_here(report, "write", &d.key, &e);
            finished(Some(e.to_string()));
            return;
        }
    };
    let name = name.as_str();
    let step = engine::Step::Download {
        key: &d.key,
        hash: &hash,
        replaces: d.expect.is_some(),
        writer: &d.writer,
    };
    let ticket = match hooks.begin(&step) {
        Ok(ticket) => ticket,
        Err(e) => {
            report.errors.push(format!("write {}: {e}", d.key));
            report.file_errors.push((d.key.clone(), e.to_string()));
            finished(Some(e.to_string()));
            return;
        }
    };
    // The last look before the rename (in replace_checked): what a pause here lets change is
    // caught there.
    hooks.point("before-rename");
    // A disk played full by a test harness fails the write as a full disk does.
    let written = match faults::check_write(&root.path) {
        Err(e) => Err(Replace::Failed(e.into())),
        Ok(()) => match &blob {
            Blob::Bytes(bytes) => replace_checked(
                &dir,
                name,
                d.expect.as_ref(),
                &mut &bytes[..],
                d.exec,
                d.mtime_ns,
            ),
            Blob::File { file, .. } => replace_from_spool(
                tree,
                file,
                &dir,
                name,
                d.expect.as_ref(),
                d.exec,
                d.mtime_ns,
            ),
        },
    };
    let path = local::path_of(&root.path, &here);
    hooks.finished(&step, ticket, written.is_ok(), Some(&path));
    hooks.point("after-rename");
    match written {
        Ok(written) => {
            drive.note_written(&here);
            if let Some(entry) = local::entry_now(&root.path, &here, &hash) {
                base.insert(d.key.clone(), entry);
            }
            report.files_down += 1;
            report.bytes_down += written;
            finished(None);
        }
        Err(Replace::Changed) => {
            // Another file of the drive came down under a name this disk takes for this one's.
            let other = d
                .expect
                .is_none()
                .then(|| local::other_spelling(&root.path, &here))
                .flatten();
            if let Some(other) = other {
                let why = same_name(&other);
                report.errors.push(format!("write {}: {why}", d.key));
                report.file_errors.push((d.key.clone(), why.clone()));
                report.tell(problems::Code::NameCollision, &d.key, why.clone());
                finished(Some(why));
                return;
            }
            report.changed_during_sync.push(format!(
                "{}: changed during the run; the next run takes it",
                d.key
            ));
            finished(Some(String::from("changed here during the run")));
        }
        Err(Replace::Failed(e)) => {
            failed_here(report, "write", &d.key, &e);
            finished(Some(e.to_string()));
        }
    }
}

/// Where a run writes here: the folder's tree (reached without a link), its drive (told of the
/// run's own writes), the folder and the scan.
struct Made<'a> {
    tree: &'a beneath::Root,
    drive: &'a LocalDrive,
    root: &'a LocalRoot,
    scan: &'a Scan,
    /// What a name from the drive must pass before a link is made under it.
    policy: &'a names::Policy,
}

/// Makes the link `key` here holding `target`, in place of what the scan found there (`expect`;
/// `None`: nothing): never through another link, never over a folder, never over a file that
/// changed since the scan, and only when its target stays inside the folder and is the content
/// (`hash`) the entry names.
fn write_link(
    made: &Made<'_>,
    key: &str,
    target: &str,
    hash: &str,
    expect: Option<&LocalFile>,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    let here = made.scan.here(key);
    if !local::link_stays_inside(key, target) || local::hash_bytes(target.as_bytes()) != hash {
        report.tell(
            problems::Code::LinkRefused,
            key,
            format!("a link to {target:?} is never made here"),
        );
        return;
    }
    let (dir, name) = match names::make_parent_of(made.tree, made.policy, here) {
        Ok(found) => found,
        Err(e) => {
            failed_here(report, "link", key, &e);
            return;
        }
    };
    let name = name.as_str();
    if !still_there(&dir, name, expect) {
        report
            .changed_during_sync
            .push(format!("{key}: changed during the run; the next run takes it"));
        return;
    }
    // Made under a name of its own, then renamed into place: what had the name before is
    // replaced in one step, and a folder of that name is never replaced.
    let temp = beneath::temp_name(name);
    match dir
        .symlink(target, &temp)
        .and_then(|()| dir.rename(&temp, &dir, name))
    {
        Ok(()) => {
            dir.sync();
            made.drive.note_written(here);
            if let Some(entry) = local::entry_now(&made.root.path, here, hash) {
                base.insert(key.to_string(), entry);
            }
            report.files_down += 1;
        }
        Err(e) => {
            let _ = dir.unlink(&temp);
            failed_here(report, "link", key, &e);
        }
    }
}

/// Why a file of the drive stays there: this disk gives its name to `other` already.
fn same_name(other: &str) -> String {
    format!(
        "\"{other}\" here has the same name to this disk, which ignores case and accents; \
         this one stays on the drive"
    )
}

/// What an app's hooks make of a plan: a file kept in the cloud only is forgotten when the
/// drive deleted it and stays in the cloud when the drive changed it; a file new to this
/// device comes down when `fetch` says so; a conflict the app holds is taken out (and
/// returned). The command line's run (no hooks) keeps the plan as it is.
fn on_demand(
    actions: &mut Vec<Action>,
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    remote: &RemoteIndex,
    hooks: &RunHooks<'_>,
) -> Vec<HeldConflict> {
    let cloud_only =
        |key: &str| !scan.files.contains_key(key) && base.get(key).is_some_and(|e| e.cloud_only);
    let mut held = Vec::new();
    let mut out = Vec::with_capacity(actions.len());
    for action in actions.drain(..) {
        let action = match action {
            Action::DeleteLocal { key } if cloud_only(&key) => Action::Forget { key },
            Action::Download { key } if cloud_only(&key) => Action::CloudOnly { key },
            // A link always comes down: it holds no bytes to keep in the cloud.
            Action::Download { key }
                if remote.files.get(&key).is_some_and(|f| f.link.is_some()) =>
            {
                Action::Download { key }
            }
            Action::Download { key } if !scan.files.contains_key(&key) => {
                let size = remote.files.get(&key).map_or(0, |f| f.size);
                match hooks.fetch {
                    Some(fetch) if !fetch(&key, size) => Action::CloudOnly { key },
                    _ => Action::Download { key },
                }
            }
            Action::Conflict { key, copy } => {
                if !hooks.holds(&key) {
                    out.push(Action::Conflict { key, copy });
                    continue;
                }
                if let (Some(here), Some(there)) = (scan.files.get(&key), remote.files.get(&key)) {
                    held.push(HeldConflict {
                        key: key.clone(),
                        here: here.hash.clone(),
                        there: there.hash.clone(),
                        there_size: there.size,
                        there_device: there.device.clone(),
                        there_exec: there.exec,
                    });
                }
                continue;
            }
            other => other,
        };
        out.push(action);
    }
    *actions = out;
    held
}

/// Takes the transfers over `limit` bytes out of the plan ([`RunHooks::transfer_limit`]): the
/// upload of a file bigger than that here, the download of one bigger than that on the drive.
/// Both sides keep their versions and the base its last synced one, so the next run without the
/// limit plans them again - as the burst guard's pause holds uploads back. Their keys.
///
/// A rename is one change in two halves (the new name's bytes, the old name's delete) and is held
/// as one (AZ-S11). The upload of a big file under its new name waits, and so does the delete of
/// its old name on the drive - else the drive has neither name until the free network, and the
/// other devices trash their copy. A rename the drive made moves no bytes (the old name here is
/// renamed, [`local_moves`]): it is no transfer, and the limit does not hold it.
fn hold_big(
    actions: &mut Vec<Action>,
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    remote: &RemoteIndex,
    limit: u64,
) -> Vec<String> {
    // The deletes that are the old name of a file arriving under another one, as the plan reads
    // them for the guards; the downloads that are no transfer at all.
    let renamed_there = moves(actions, scan, base);
    let renamed_here: BTreeSet<String> = local_moves(actions, scan, remote)
        .into_keys()
        .map(str::to_string)
        .collect();
    let mut held = Vec::new();
    // The content of each held upload.
    let mut held_up: BTreeMap<String, usize> = BTreeMap::new();
    actions.retain(|action| {
        let bytes = match action {
            Action::Upload { key } => scan.files.get(key).map(|f| f.blob_size),
            Action::Download { key } if !renamed_here.contains(key) => {
                remote.files.get(key).map(|f| f.size)
            }
            _ => None,
        };
        let waits = bytes.is_some_and(|bytes| bytes > limit);
        if waits {
            held.push(action.key().to_string());
            if let Action::Upload { key } = action {
                if let Some(file) = scan.files.get(key) {
                    *held_up.entry(file.hash.clone()).or_default() += 1;
                }
            }
        }
        !waits
    });
    // One old name per held upload of its content, as `moves` pairs them.
    actions.retain(|action| match action {
        Action::DeleteRemote { key } if renamed_there.contains(key) => {
            let hash = base.get(key).map(|e| e.hash.as_str()).unwrap_or_default();
            match held_up.get_mut(hash).filter(|left| **left > 0) {
                Some(left) => {
                    *left -= 1;
                    false
                }
                None => true,
            }
        }
        _ => true,
    });
    held
}

/// How the restored drive `remote` relates to the base's `entry` of `key`, for
/// [`restore::rebase`]: the files it lost (no entry, no recorded delete OF THIS VERSION), the ones
/// it holds in another version than the base's (the edit may exist nowhere else; an entry that
/// holds no bytes here is no edit - a new version of it on the drive is an ordinary download), and
/// the ones it agrees with or recorded the delete of. A tombstone is the delete of one version
/// (its hash): a name deleted at generation 2, made again at generation 3 and restored to
/// generation 2 holds the OLD tombstone, and the new bytes are lost to the restore, not deleted -
/// they used to read as "deleted elsewhere, unchanged here" and went to the trash on every device
/// that had pulled them (X-DELTA-AZDRIVE-FILES-8).
fn restore_relation(remote: &RemoteIndex, key: &str, entry: &BaseEntry) -> restore::Remote {
    match remote.files.get(key) {
        None if remote
            .deleted
            .get(key)
            .is_some_and(|tomb| tomb.hash == entry.hash) =>
        {
            restore::Remote::Deleted
        }
        None => restore::Remote::Lost,
        Some(file) if !entry.cloud_only && !merge::same_version(entry, file) => {
            restore::Remote::Other
        }
        Some(_) => restore::Remote::Same,
    }
}

/// The base of a pairing made now (a folder never synced, or whose base was lost or damaged):
/// it starts at the current restore epoch, so an announcement made before it existed never
/// excuses a later unannounced rollback ([`restore::baseline`]).
fn fresh_index(root: &LocalRoot, opts: &SyncOptions) -> LocalIndex {
    let mut index = LocalIndex::new(&root.path, &opts.bucket, &opts.prefix);
    index.restore_epoch = restore::baseline(opts.restore.as_ref());
    index
}

/// `map` with every key under a folder of `finals` (base key -> final key) at its final key.
fn rebase_keys<V: Clone>(
    map: &BTreeMap<String, V>,
    finals: &BTreeMap<String, String>,
) -> BTreeMap<String, V> {
    map.iter()
        .map(|(key, value)| (folders::rebased(key, finals), value.clone()))
        .collect()
}

/// Every name the scan found here: files, folders, and what it does not sync.
fn present_here(scan: &Scan) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = scan.files.keys().cloned().collect();
    out.extend(scan.dirs.iter().cloned());
    out.extend(scan.unknown.keys().cloned());
    out.extend(scan.temps.keys().cloned());
    out.extend(scan.links.keys().cloned());
    out.extend(scan.mounts.iter().cloned());
    out.extend(scan.skipped.iter().map(|(key, _)| key.clone()));
    out
}

/// Whether `moves` can be made one after the other where `present` names everything: each moves
/// a folder that is there onto a name nothing has, never into itself.
fn moves_fit(present: &BTreeSet<String>, moves: &[(String, String)]) -> bool {
    let mut present = present.clone();
    for (from, to) in moves {
        let inside = format!("{to}/");
        let taken = present.contains(to)
            || present
                .range(inside.clone()..)
                .next()
                .is_some_and(|k| k.starts_with(&inside));
        if !present.contains(from) || taken || to.starts_with(&format!("{from}/")) {
            return false;
        }
        let one = [(from.clone(), to.clone())];
        present = present
            .into_iter()
            .map(|key| folders::moved(&key, &one))
            .collect();
    }
    true
}

/// The scan with the folders of `moves` moved, as they were just moved here.
fn moved_scan(scan: &mut Scan, moves: &[(String, String)]) {
    if moves.is_empty() {
        return;
    }
    let m = |key: &str| folders::moved(key, moves);
    scan.files = std::mem::take(&mut scan.files)
        .into_iter()
        .map(|(key, file)| (m(key.as_str()), file))
        .collect();
    scan.folder_ids = std::mem::take(&mut scan.folder_ids)
        .into_iter()
        .map(|(key, ino)| (m(key.as_str()), ino))
        .collect();
    scan.temps = std::mem::take(&mut scan.temps)
        .into_iter()
        .map(|(key, size)| (m(key.as_str()), size))
        .collect();
    scan.links = std::mem::take(&mut scan.links)
        .into_iter()
        .map(|(key, target)| (m(key.as_str()), target))
        .collect();
    scan.spelled = std::mem::take(&mut scan.spelled)
        .into_iter()
        .map(|(key, spelling)| (m(key.as_str()), m(spelling.as_str())))
        .collect();
    scan.unknown = std::mem::take(&mut scan.unknown)
        .into_iter()
        .map(|(key, why)| (m(key.as_str()), why))
        .collect();
    for set in [&mut scan.dirs, &mut scan.mounts] {
        *set = std::mem::take(set)
            .into_iter()
            .map(|key| m(key.as_str()))
            .collect();
    }
    for (key, _) in &mut scan.skipped {
        *key = m(key.as_str());
    }
}

/// The drive's index with the folders of `moves` moved, as this device moved them here: every
/// entry under one at its new key, and the old names' deletes recorded, so a device that lost
/// its state keeps an old copy instead of sending it again.
fn moved_index(
    index: &RemoteIndex,
    moves: &[(String, String)],
    gen: u64,
    now: i64,
    device: &str,
) -> RemoteIndex {
    let mut out = index.clone();
    if moves.is_empty() {
        return out;
    }
    out.files = BTreeMap::new();
    for (key, file) in &index.files {
        let to = folders::moved(key, moves);
        if to != *key && remote::is_hash(&file.hash) {
            let tomb = Tombstone {
                hash: file.hash.clone(),
                gen,
                at: now,
                device: device.to_string(),
                confirmed: false,
            };
            out.deleted.insert(key.clone(), tomb);
        }
        out.deleted.remove(&to);
        out.files.insert(to, file.clone());
    }
    out.folders = BTreeMap::new();
    for (key, folder) in &index.folders {
        let to = folders::moved(key, moves);
        if to != *key {
            let tomb = FolderTombstone {
                id: folder.id.clone(),
                gen,
                at: now,
                device: device.to_string(),
            };
            out.deleted_folders.insert(key.clone(), tomb);
        }
        out.deleted_folders.remove(&to);
        out.folders.insert(to, folder.clone());
    }
    out
}

/// Moves the folders of `moves` here, one after the other, without following a link: how many
/// were moved (a move that fails stops the rest). `from` is a folder of this disk (the base's
/// or the scan's key); `to` is the DRIVE's new name for it, so it passes the whole policy of a
/// name from the drive - the protected names in any spelling, this system's rows and this
/// disk's (`volume`) - before a folder of it is made or the folder is renamed to it: a drive
/// that names a folder `.git` must not make a repository here (X-DELTA-AZDRIVE-FILES-1).
fn move_folders_here(
    root: &LocalRoot,
    moves: &[(String, String)],
    volume: &names::Volume,
    report: &mut SyncReport,
) -> usize {
    let Ok(tree) = beneath::Root::open(&root.path) else {
        return 0;
    };
    let policy = names::Policy::here(*volume);
    for (made, (from, to)) in moves.iter().enumerate() {
        let moved = names::walk_record(&tree, from).and_then(|(source, name)| {
            let (target, target_name) = names::make_parent_of(&tree, &policy, to)?;
            source.rename(&name, &target, &target_name)?;
            target.sync();
            source.sync();
            Ok(())
        });
        match moved {
            Ok(()) => report.moved.push(format!("{from}/ -> {to}/")),
            Err(e) => {
                report.notes.push(format!(
                    "{from}: renamed on the drive to {to}, and not renamed here ({e}); its files \
                     follow one by one"
                ));
                return made;
            }
        }
    }
    moves.len()
}

/// The folders the drive renamed to a name this computer refuses, as (the name the base has,
/// the name the drive gives): a folder keeps its identity on the drive through a rename, so the
/// base's folder of that identity under another name is the one that was renamed. `align_folders`
/// leaves them alone (a refused name is no rename here), which is right - but the files are listed
/// under the refused name (frozen) and absent under the old one, and every unchanged copy here
/// would plan "the drive deleted it" (X-DELTA-AZDRIVE-SYNC-11).
fn refused_renames(index: &LocalIndex, remote: &RemoteIndex) -> Vec<(String, String)> {
    remote
        .folders
        .iter()
        .filter(|(key, folder)| {
            remote::is_folder_id(&folder.id)
                && (remote::check_key(key).is_err() || names::folder_problem(key).is_some())
        })
        .filter_map(|(new, folder)| {
            index
                .folders
                .iter()
                .find(|(old, base)| base.id == folder.id && *old != new)
                .map(|(old, _)| (old.clone(), new.clone()))
        })
        .collect()
}

/// The folders renamed since the base, on either side (A3, C1): what the drive renamed is
/// renamed here now and taken into the scan; a rename both sides have made is taken into the
/// base; what was renamed here comes back as the moves the drive gets with this run's commit,
/// with the key every folder of the base ends at. A rename that cannot be made as a whole (its
/// name taken, a folder that cannot be moved, a backup) is left to the files, each a delete and
/// a new file as without folder identities.
fn align_folders(
    root: &LocalRoot,
    index: &mut LocalIndex,
    scan: &mut Scan,
    remote: &RemoteIndex,
    volume: &names::Volume,
    backup: bool,
    report: &mut SyncReport,
) -> (Vec<(String, String)>, BTreeMap<String, String>) {
    let none = || (Vec::new(), BTreeMap::new());
    // The drive's folder entries as they stand BEFORE `RemoteIndex::unreadable` looked at them:
    // a folder is named by the same table a file is (a last name `.git` included), so the one
    // the drive renamed to such a name is no rename here (X-DELTA-AZDRIVE-FILES-1).
    let there: BTreeMap<String, RemoteFolder> = remote
        .folders
        .iter()
        .filter(|(key, folder)| {
            remote::check_key(key).is_ok()
                && names::folder_problem(key).is_none()
                && remote::is_folder_id(&folder.id)
        })
        .map(|(key, folder)| (key.clone(), folder.clone()))
        .collect();
    let mut found = folders::renames(&index.folders, &scan.folder_ids, &there);
    if found.is_empty() {
        return none();
    }
    for key in &found.clashes {
        report.notes.push(format!(
            "{key}: renamed here and on the drive to different names; it takes the drive's"
        ));
    }
    if !found.here.is_empty() {
        if backup || !moves_fit(&present_here(scan), &found.here) {
            report.notes.push(String::from(
                "a folder renamed on the drive cannot be renamed here (its new name is taken); \
                 its files follow one by one",
            ));
            return none();
        }
        let made = move_folders_here(root, &found.here, volume, report);
        moved_scan(scan, &found.here[..made]);
        if made < found.here.len() {
            return none();
        }
        found = folders::renames(&index.folders, &scan.folder_ids, &there);
    }
    if !found.agreed.is_empty() {
        index.files = rebase_keys(&index.files, &found.agreed);
        index.folders = rebase_keys(&index.folders, &found.agreed);
        index.kept = index
            .kept
            .iter()
            .map(|key| folders::rebased(key, &found.agreed))
            .collect();
        found = folders::renames(&index.folders, &scan.folder_ids, &there);
    }
    if found.there.is_empty() || backup {
        return none();
    }
    let present: BTreeSet<String> = remote
        .files
        .keys()
        .chain(remote.folders.keys())
        .chain(remote.damaged.keys())
        .cloned()
        .collect();
    if !moves_fit(&present, &found.there) {
        report.notes.push(String::from(
            "a folder renamed here cannot be renamed on the drive (its new name is taken there); \
             its files go one by one",
        ));
        return none();
    }
    (found.there, found.finals)
}

/// What is here and on the drive once `actions` are done: the files, and here also what the
/// scan saw and does not sync (unknown, settling, left out, links), there what the run could not
/// read. A folder holding any of it is not removed on that side.
fn files_after(
    actions: &[Action],
    scan: &Scan,
    remote: &RemoteIndex,
    unreadable: &BTreeMap<String, String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut here: BTreeSet<String> = scan.files.keys().cloned().collect();
    here.extend(scan.unknown.keys().cloned());
    here.extend(scan.temps.keys().cloned());
    here.extend(scan.links.keys().cloned());
    here.extend(scan.mounts.iter().cloned());
    here.extend(scan.skipped.iter().map(|(key, _)| key.clone()));
    let mut there: BTreeSet<String> = remote.files.keys().cloned().collect();
    there.extend(unreadable.keys().cloned());
    for action in actions {
        match action {
            Action::Download { key } => {
                here.insert(key.clone());
            }
            Action::DeleteLocal { key } => {
                here.remove(key);
            }
            Action::Upload { key } | Action::MergeJson { key } => {
                there.insert(key.clone());
            }
            Action::DeleteRemote { key } => {
                there.remove(key);
            }
            Action::Conflict { key, copy } => {
                here.insert(key.clone());
                here.insert(copy.clone());
                there.insert(copy.clone());
            }
            Action::Agree { .. } | Action::Forget { .. } | Action::CloudOnly { .. } => {}
        }
    }
    (here, there)
}

/// The folders' plan of a run ([`folders::plan`]): the folders here whose names may be keys,
/// what stays on each side after `actions`, and the drive's folders as the run plans with them.
fn plan_folders(
    actions: &[Action],
    scan: &Scan,
    readable: &RemoteIndex,
    unreadable: &BTreeMap<String, String>,
    base: &BTreeMap<String, BaseFolder>,
    excluded: &dyn Fn(&str) -> bool,
    rules: &Rules,
) -> Vec<FolderAction> {
    let here: BTreeSet<String> = scan
        .dirs
        .iter()
        .filter(|key| remote::check_key(key).is_ok() && names::folder_problem(key).is_none())
        .cloned()
        .collect();
    let (files_here, files_there) = files_after(actions, scan, readable, unreadable);
    let left_out = |key: &str| excluded(key) || rules.excluded_dir(key).is_some();
    folders::plan(
        &folders::FolderInput {
            base,
            here: &here,
            remote: readable,
            excluded: &left_out,
            files_here: &files_here,
            files_there: &files_there,
        },
        &root::new_id,
    )
}

/// Makes here the folders the drive has and this side has not, removes the ones the drive
/// deleted (empty ones only: a folder that still holds something stays), and keeps in `base`
/// the folders both sides have now, with their inode here. A folder is made under a name from
/// the DRIVE: it passes the policy of such a name (`volume` is this disk's) before a folder of it
/// exists.
fn apply_folders(
    root: &LocalRoot,
    scan: &Scan,
    actions: &[FolderAction],
    volume: &names::Volume,
    base: &mut BTreeMap<String, BaseFolder>,
    report: &mut SyncReport,
) {
    if actions.is_empty() {
        return;
    }
    let tree = match beneath::Root::open(&root.path) {
        Ok(tree) => tree,
        Err(e) => {
            report.errors.push(format!(
                "{}: the folder cannot be opened ({e}); no folder was made or removed here",
                root.path.display()
            ));
            return;
        }
    };
    let here = |key: &str| scan.folder_ids.get(key).copied().unwrap_or((0, 0));
    let policy = names::Policy::here(*volume);
    for action in actions {
        match action {
            FolderAction::Agree { key, id } | FolderAction::Send { key, id } => {
                let (ino, dev) = here(key.as_str());
                let entry = BaseFolder {
                    id: id.clone(),
                    ino,
                    dev,
                };
                base.insert(key.clone(), entry);
            }
            FolderAction::Make { key, id } => {
                let made = policy
                    .admit(key, names::Purpose::Sync)
                    .map_err(BeneathError::from)
                    .and_then(|admitted| tree.dir(&admitted, true))
                    .and_then(|dir| dir.stat().map_err(BeneathError::from));
                match made {
                    Ok(st) => {
                        let entry = BaseFolder {
                            id: id.clone(),
                            ino: st.ino,
                            dev: st.dev,
                        };
                        base.insert(key.clone(), entry);
                        report.folders_made += 1;
                    }
                    Err(e) => failed_here(report, "make", key, &e),
                }
            }
            FolderAction::RemoveThere { key, .. } | FolderAction::Forget { key } => {
                base.remove(key);
            }
            FolderAction::RemoveHere { .. } => {}
        }
    }
    // Deepest first: a folder inside goes before the one it is in.
    let mut removals: Vec<&str> = actions
        .iter()
        .filter_map(|a| match a {
            FolderAction::RemoveHere { key } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    removals.sort_by_key(|key| std::cmp::Reverse(key.matches('/').count()));
    for key in removals {
        match names::walk_record(&tree, key).and_then(|(dir, name)| dir.rmdir(&name)) {
            Ok(()) => {
                base.remove(key);
                report.folders_removed += 1;
            }
            Err(e) if e.is_not_found() => {
                base.remove(key);
            }
            // Something the sync does not take is still in it (a file left out by a rule).
            Err(e) => report.notes.push(format!(
                "{key}: deleted on the drive, and kept here because it is not empty ({e})"
            )),
        }
    }
}

/// What the plan moves: files and bytes up and down.
fn planned(actions: &[Action], scan: &Scan, remote: &RemoteIndex) -> SyncEvent {
    let (mut up_files, mut up_bytes, mut down_files, mut down_bytes) = (0usize, 0u64, 0usize, 0u64);
    for action in actions {
        let key = action.key();
        let up = matches!(
            action,
            Action::Upload { .. } | Action::Conflict { .. } | Action::MergeJson { .. }
        );
        let down = matches!(action, Action::Download { .. } | Action::Conflict { .. });
        if up {
            if let Some(f) = scan.files.get(key) {
                up_files += 1;
                up_bytes += f.blob_size;
            }
        }
        if down {
            if let Some(f) = remote.files.get(key) {
                down_files += 1;
                down_bytes += f.size;
            }
        }
    }
    SyncEvent::Planned {
        up_files,
        up_bytes,
        down_files,
        down_bytes,
    }
}

/// Syncs the folder `root` with `opts.prefix` of the bucket behind `store`;
/// its local index lives at `index_path` (in the state folder).
///
/// # Errors
///
/// When the folder or the index cannot be read, a plan is stopped as a mass
/// delete, the race is lost [`MAX_ATTEMPTS`] times, or the drive refuses a
/// request. Nothing is changed here before the index was committed; errors
/// of single files after that are in the report's `errors`.
pub fn sync_folder<S: RemoteStore + ?Sized>(
    store: &S,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
) -> CloudResult<SyncReport> {
    run(
        &BlobIndex::new(store, &opts.prefix),
        root,
        index_path,
        opts,
        &RunHooks::default(),
    )
}

/// Syncs the folder `root` with `opts.prefix` of `target`, with an app's `hooks`; its local
/// index lives at `index_path`.
///
/// # Errors
///
/// As [`sync_folder`]. A stopped run is no error: the report says `cancelled`.
pub fn sync_to(
    target: Target<'_>,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
) -> CloudResult<SyncReport> {
    match target {
        Target::Index(store) => run(
            &BlobIndex::new(store, &opts.prefix),
            root,
            index_path,
            opts,
            hooks,
        ),
        Target::Named(drive) => run(
            &named::NamedFiles::new(drive, &opts.prefix, index_path),
            root,
            index_path,
            opts,
            hooks,
        ),
    }
}

/// The BLAKE3 of the file of `key` under `root`, read in pieces without following a link.
fn hash_here(root: &Path, key: &str) -> Option<String> {
    let tree = beneath::Root::open(root).ok()?;
    let (dir, name) = names::walk_record(&tree, key).ok()?;
    let (mut file, _) = dir.open_read(&name).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hasher.finalize().to_hex().to_string())
}

/// P10, D6: an app that saves a file writes the new version aside and renames the old one to
/// an ignored temporary name in the same folder for a moment, then renames the new one in. A
/// run in that moment sees the file gone; a delete on the drive then would lose it on every
/// device. A delete whose old bytes sit in such a temporary file next to it waits for the next
/// run.
fn keep_saves(
    actions: &mut Vec<Action>,
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    root: &Path,
    report: &mut SyncReport,
) {
    let mut hashes: BTreeMap<String, Option<String>> = BTreeMap::new();
    actions.retain(|action| {
        let Action::DeleteRemote { key } = action else {
            return true;
        };
        let Some(entry) = base.get(key) else {
            return true;
        };
        let folder = beneath::split_key(key).0;
        for (temp, size) in &scan.temps {
            let (temp_folder, temp_name) = beneath::split_key(temp);
            if temp_folder != folder || *size != entry.size {
                continue;
            }
            let hash = hashes
                .entry(temp.clone())
                .or_insert_with(|| hash_here(root, temp));
            if hash.as_deref() == Some(entry.hash.as_str()) {
                report.tell(
                    problems::Code::SaveInProgress,
                    key,
                    format!(
                        "an app is saving it (its old version is in {temp_name}); the next pass \
                         takes the result"
                    ),
                );
                return false;
            }
        }
        true
    });
}

/// Checks every file of the drive before the plan (M1, S2, P1-P5): a name this system or disk
/// cannot hold, two names this disk takes for one, a file whose folder here is a link, another
/// disk or a file, a folder here where the drive has a file. Each is listed and frozen: the
/// plan neither writes nor deletes it, on either side. A name the drive spells otherwise than
/// this folder - decomposed here, composed there, on a disk that takes both for one name - is
/// the same file: the scan takes the drive's spelling ([`Scan::spelled`]).
/// The folders of the drive this disk takes for another spelling of one of them, each with the
/// spelling that stays (the one the base or this folder holds files under, else the one
/// committed first, else the composed (NFC) one).
fn losing_folders(
    remote: &RemoteIndex,
    base: &BTreeMap<String, BaseEntry>,
    scan: &Scan,
    volume: &names::Volume,
    frozen: &BTreeSet<String>,
) -> Vec<(String, String)> {
    // Per folded folder, each spelling with the first generation that wrote under it.
    let mut spellings: BTreeMap<String, BTreeMap<&str, u64>> = BTreeMap::new();
    for (key, file) in remote.files.iter().filter(|(k, _)| !frozen.contains(*k)) {
        for (i, _) in key.match_indices('/') {
            let folder = &key[..i];
            let first = spellings
                .entry(names::fold(folder, volume))
                .or_default()
                .entry(folder)
                .or_insert(u64::MAX);
            *first = (*first).min(file.gen);
        }
    }
    let under = |folder: &str| format!("{folder}/");
    let mut out = Vec::new();
    for found in spellings.values().filter(|found| found.len() > 1) {
        let winner = found
            .keys()
            .find(|f| base.keys().any(|k| k.starts_with(&under(f))))
            .or_else(|| {
                found
                    .keys()
                    .find(|f| scan.files.keys().any(|k| k.starts_with(&under(f))))
            })
            .or_else(|| {
                found
                    .iter()
                    .min_by_key(|(f, gen)| (**gen, names::nfc(f) != **f, **f))
                    .map(|(f, _)| f)
            })
            .copied()
            .unwrap_or_default();
        for folder in found.keys().filter(|f| **f != winner) {
            out.push(((*folder).to_string(), winner.to_string()));
        }
    }
    out
}

fn check_remote(
    scan: &mut Scan,
    remote: &RemoteIndex,
    base: &BTreeMap<String, BaseEntry>,
    volume: &names::Volume,
    max_file_bytes: u64,
    report: &mut SyncReport,
) -> BTreeSet<String> {
    use problems::Code;
    let system = names::System::here();
    let mut frozen = BTreeSet::new();
    for (key, file) in &remote.files {
        // P8, P14: more than this device takes, or than the disk can hold in one file (FAT32's
        // 4 GiB - 1): listed, never written in part.
        let limit = volume
            .max_file
            .map_or(max_file_bytes, |m| m.min(max_file_bytes));
        if file.size > limit && !scan.files.contains_key(key) {
            report.tell(
                Code::TooLarge,
                key,
                format!(
                    "{} bytes is more than this device or disk takes ({limit})",
                    file.size
                ),
            );
            frozen.insert(key.clone());
            continue;
        }
        if let Some(why) = names::here_problem(key, system, volume) {
            report.tell(
                Code::InvalidHere,
                key,
                format!("this system cannot hold it ({why}); it stays on the drive, not renamed"),
            );
            frozen.insert(key.clone());
            continue;
        }
        // S8: a link that would point out of the folder (or at its bookkeeping, an absolute
        // path, a drive) is never made here; what has its name here stays as it is.
        if let Some(target) = &file.link {
            if !local::link_stays_inside(key, target) || !cfg!(unix) {
                let why = if cfg!(unix) {
                    format!("a link to {target:?} would point out of the folder; it is never made")
                } else {
                    format!("a link to {target:?} is not made on this system")
                };
                report.tell(Code::LinkRefused, key, why);
                frozen.insert(key.clone());
                continue;
            }
        }
        for risk in names::display_risks(key) {
            report.tell(Code::DisplayRisk, key, format!("its name has {risk}"));
        }
        // A repository's own configuration (`.gitmodules`, `.gitattributes`) that arrives from a
        // drive other people write to is warned of, not refused: git reads it where it lies
        // (X-DELTA-AZDRIVE-FILES-32).
        if let Some(note) = names::vcs_metadata_note(key) {
            report.tell(Code::DisplayRisk, key, note);
        }
        if !scan.files.contains_key(key) {
            for (other, why) in names::portability(key, system) {
                report.tell(
                    Code::Portability,
                    key,
                    format!("{} cannot hold it: {why}", other.label()),
                );
            }
        }
        let mut blocked = None;
        let mut folder = key.as_str();
        while let Some(i) = folder.rfind('/') {
            folder = &folder[..i];
            if scan.links.contains_key(folder) {
                blocked = Some((
                    Code::ThroughLink,
                    format!("\"{folder}\" here is a link; nothing is written through it"),
                ));
                break;
            }
            if scan.mounts.contains(folder) {
                blocked = Some((
                    Code::MountPoint,
                    format!("\"{folder}\" here is another disk; nothing is written into it"),
                ));
                break;
            }
            if scan.files.contains_key(folder) {
                blocked = Some((
                    Code::TypeConflict,
                    format!(
                        "\"{folder}\" is a file here and a folder on the drive; both are kept as \
                         they are"
                    ),
                ));
                break;
            }
        }
        if blocked.is_none() && scan.dirs.contains(key.as_str()) {
            blocked = Some((
                Code::TypeConflict,
                String::from(
                    "it is a folder here and a file on the drive; both are kept as they are",
                ),
            ));
        }
        if let Some((code, why)) = blocked {
            report.tell(code, key, why);
            frozen.insert(key.clone());
        }
    }
    if !(volume.case_insensitive || volume.norm_insensitive) {
        return frozen;
    }
    // P1 for folders: two spellings of one folder on the drive ("Applause Samples/",
    // "applause samples/") are one folder to this disk, where the files of both would land
    // under names the drive does not know. The files of the spelling in the base or here,
    // else of the one committed first, come down; the other's stay on the drive, listed.
    for (folder, winner) in losing_folders(remote, base, scan, volume, &frozen) {
        let under = format!("{folder}/");
        let keys: Vec<String> = remote
            .files
            .keys()
            .filter(|k| k.starts_with(&under) && !frozen.contains(*k))
            .cloned()
            .collect();
        for key in keys {
            report.tell(
                Code::NameCollision,
                &key,
                format!(
                    "this disk takes its folder \"{}\" for \"{}\", which stays; it is not \
                     downloaded here",
                    names::escaped(&folder),
                    names::escaped(&winner)
                ),
            );
            frozen.insert(key);
        }
    }
    // P1: of two names of the drive this disk takes for one, the one in the base or here, else
    // the one committed first, comes down; the other stays on the drive, listed.
    let mut groups: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for key in remote.files.keys().filter(|k| !frozen.contains(*k)) {
        groups
            .entry(names::fold(key, volume))
            .or_default()
            .push(key);
    }
    let mut winners: BTreeMap<String, String> = BTreeMap::new();
    let mut losers: Vec<(String, String)> = Vec::new();
    for (fold, keys) in &groups {
        let winner = if keys.len() == 1 {
            keys[0].clone()
        } else {
            keys.iter()
                .find(|k| base.contains_key(k.as_str()))
                .or_else(|| keys.iter().find(|k| scan.files.contains_key(k.as_str())))
                .or_else(|| {
                    // The one committed first; where the drive does not say (a drive synced by
                    // name gives every file the same generation), the composed (NFC) spelling,
                    // the form every system reads the same.
                    keys.iter().min_by_key(|k| {
                        (
                            remote.files.get(k.as_str()).map_or(0, |f| f.gen),
                            names::nfc(k) != ***k,
                            (**k).clone(),
                        )
                    })
                })
                .map(|k| (**k).clone())
                .unwrap_or_default()
        };
        for key in keys {
            if **key != winner {
                losers.push(((*key).clone(), winner.clone()));
            }
        }
        winners.insert(fold.clone(), winner);
    }
    for (loser, winner) in losers {
        report.tell(
            Code::NameCollision,
            &loser,
            format!(
                "this disk takes it for \"{}\", which stays; it is not downloaded here",
                names::escaped(&winner)
            ),
        );
        frozen.insert(loser);
    }
    // P2: a name here the drive spells in another normalization is that file.
    if volume.norm_insensitive {
        let aliases: Vec<(String, String)> = scan
            .files
            .keys()
            .filter(|l| !remote.files.contains_key(*l) && !base.contains_key(*l))
            .filter_map(|l| {
                let w = winners.get(&names::fold(l, volume))?;
                (w != l && names::nfc(w) == names::nfc(l) && !scan.files.contains_key(w))
                    .then(|| (l.clone(), w.clone()))
            })
            .collect();
        for (here, there) in aliases {
            if let Some(file) = scan.files.remove(&here) {
                scan.files.insert(there.clone(), file);
                scan.spelled.insert(there, here);
            }
        }
    }
    frozen
}

/// The loop of a run against any drive side: [`run_pass`], and the problems it left out said in
/// one row per code.
fn run<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
) -> CloudResult<SyncReport> {
    run_pass(remote_side, root, index_path, opts, hooks).map(|mut report| {
        report.close_problems();
        report
    })
}

fn run_pass<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
) -> CloudResult<SyncReport> {
    use problems::Code;
    let mut report = SyncReport {
        root: root.path.display().to_string(),
        prefix: opts.prefix.clone(),
        dry_run: opts.dry_run,
        ..SyncReport::default()
    };
    // The quiet folder before the base is read: a change to it made during the wait (an
    // answer to a question) is in what this run works from.
    local::settle(&root.path, &root.rules, opts.settle, SETTLE_MAX, &|| {
        hooks.cancelled()
    });
    // D5: a base that cannot be read (damaged, from a newer version) is no reason to delete
    // anything: the run pairs afresh, a union of both sides, and says so.
    let mut index = match LocalIndex::load(index_path) {
        Ok(Some(index)) => index,
        Ok(None) => {
            // No base, in a folder that holds the marker of a pairing: it was synced before and
            // its sync state is gone (deleted, a lost cache, paired again). Said, as for a
            // damaged base: the run matches both sides by content and deletes nothing.
            if root::read_marker(&root.path).is_some() {
                report.tell(
                    Code::StateReset,
                    "",
                    "this folder was synced before and its sync state is gone; this run matches \
                     it with the drive by content and deletes nothing",
                );
            }
            fresh_index(root, opts)
        }
        Err(e) => {
            report.tell(
                Code::StateReset,
                "",
                format!(
                    "this folder's sync state cannot be read ({e}); this run pairs it afresh and \
                     deletes nothing"
                ),
            );
            // The new pairing is saved in a new base, the unreadable one kept beside it.
            if !opts.dry_run {
                LocalIndex::set_aside(index_path).map_err(|why| {
                    CloudError::Io(format!(
                        "the unreadable sync state {} cannot be moved aside: {why}",
                        index_path.display()
                    ))
                })?;
            }
            fresh_index(root, opts)
        }
    };
    // D3: the paired folder, or nothing changes anywhere. A missing folder, or one without the
    // marker it was paired with (an unplugged disk's empty mount point, a stale copy under it),
    // is never "every file deleted".
    let paired = index.generation > 0 || !index.files.is_empty() || index.root_id.is_some();
    if paired || !opts.dry_run {
        match root::check(
            &root.path,
            index.root_id.as_deref(),
            paired.then_some(&index.files),
        ) {
            Ok(root::RootCheck::Paired(marker)) => index.root_id = Some(marker.id),
            Ok(root::RootCheck::Missing) => {
                report.refuse(
                    Code::RootMissing,
                    format!(
                        "{} is gone (a disk not mounted, a folder moved or deleted): nothing was \
                         synced and nothing deleted",
                        root.path.display()
                    ),
                );
                return Ok(report);
            }
            Ok(root::RootCheck::Replaced(why)) => {
                report.refuse(
                    Code::RootReplaced,
                    format!("{why}: nothing was synced and nothing deleted"),
                );
                return Ok(report);
            }
            Ok(root::RootCheck::Unreadable(why)) => {
                report.refuse(
                    Code::Unreadable,
                    format!(
                        "{why}: nothing was synced and nothing deleted (give this app access to \
                         the folder again)"
                    ),
                );
                return Ok(report);
            }
            Err(e) => {
                return Err(
                    CloudError::from(e).context(format!("the folder {}", root.path.display()))
                )
            }
        }
        if !opts.dry_run {
            // What the disk does with names: probed at the first pass, and again when the folder
            // is on another disk than the one it was probed on (it keeps its marker, and so its
            // identity, when it is moved). A probe that could not answer is no answer: nothing is
            // kept, this pass judges names by the strict side, the next one tries again (X-AR5a-3).
            let disk = root::device_of(&root.path);
            let moved = disk.is_some_and(|disk| index.volume_dev != disk);
            if index.volume.is_none() || moved {
                match root::probe_volume(&root.path) {
                    Cap::Known(volume) => {
                        index.volume = Some(volume);
                        index.volume_dev = disk.unwrap_or(0);
                    }
                    Cap::Unknown(why) => {
                        index.volume = None;
                        index.volume_dev = 0;
                        report.notes.push(format!(
                            "{why}; this pass judges names as strictly as any disk could, and \
                             the next one tries again"
                        ));
                    }
                }
            }
            if let Ok(tree) = beneath::Root::open(&root.path) {
                root::purge_spool(&tree);
            }
        }
    }
    let volume = index.volume.unwrap_or_else(root::unprobed_volume);
    // The folder as it was at one moment: a change made while it is read is read again whole.
    let scanned = local::scan_unchanged(
        &root.path,
        &root.rules,
        opts.settle,
        SETTLE_MAX,
        &|| hooks.cancelled(),
        || {
            local::scan_settled(
                &root.path,
                &root.rules,
                &index.files,
                index.scanned_at_ns,
                &root.json_merge,
                opts.max_file_bytes,
                opts.file_settle,
            )
        },
    )?;
    hooks.point("after-scan");
    let mut scanned = scanned;
    // A drive that keeps no links gets none: each link is skipped and listed, never followed.
    if !remote_side.keeps_links() {
        local::skip_links(&mut scanned);
    }
    if !remote_side.tracks_exec() {
        for file in scanned.files.values_mut() {
            file.exec = false;
        }
    } else if volume.no_exec_bits {
        // The disk shows every file with one mode: a file keeps the bit of the version last
        // synced (a new one has none), so the mode alone never makes it look changed.
        for (key, file) in &mut scanned.files {
            file.exec = index.files.get(key).is_some_and(|b| b.exec);
        }
    }
    report.scanned = scanned.files.len();
    report.hashed = scanned.hashed;
    report.excluded = scanned.excluded;
    report.skipped = scanned.skipped.clone();
    // The scan's rows that name a file; the ones that say "and N more" are made again at the end
    // from the count the report keeps.
    report
        .problems
        .extend(scanned.problems.iter().filter(|p| p.more == 0).cloned());
    report.told.absorb(&scanned.told);
    // What an attempt tells is told again by the next one (another device committed first).
    let told = report.problems.len();
    let told_counts = report.told.clone();
    let date = today();
    let mut cached = load_cache(index_path, &index);
    let mut pause = None;
    // The clock the expiries of this pass are judged by, looked at once the drive has answered.
    let mut trusted: Option<clock::TrustedClock> = None;
    let mut attempt = 0u32;
    // The files D5 keeps (the last attempt's), remembered in the base.
    let mut kept: BTreeSet<String>;
    // Where the base's folders end, as the last attempt renamed them.
    let mut finals_done: BTreeMap<String, String> = BTreeMap::new();
    let (committed, actions, folder_actions, merged, etag, scan, base_after, restore_epoch) = loop {
        attempt += 1;
        report.problems.truncate(told);
        report.told.clone_from(&told_counts);
        let (remote, etag) = remote_side.read(cached.as_ref())?;
        // D43: an index this version does not read whole is left as it is (read-only here).
        let newer = remote.unknown_features();
        if !newer.is_empty() {
            report.tell(
                Code::UpdateNeeded,
                "",
                format!(
                    "the drive uses what this version does not know ({}); it is left as it is \
                     until the app is updated",
                    newer.join(", ")
                ),
            );
            report.newer_format = newer;
            report.generation = remote.generation;
            return Ok(report);
        }
        // The trash and the tombstones expire by age, and only by a clock that has proved itself
        // (AZ-S12): one that stayed steady for a day and is not behind what the drive has seen. The
        // look comes after the drive has answered - against a storage node a clock that is far off
        // has been refused by now - and before any expiry of the pass.
        if trusted.is_none() && !opts.dry_run {
            let clock = clock::TrustedClock::look(
                &mut index.clock,
                crate::now().saturating_add(opts.clock_offset),
                remote.updated_at,
            );
            if let (Some(before), Ok(tree)) = (
                clock.purge_before(root::TRASH_DAYS),
                beneath::Root::open(&root.path),
            ) {
                root::purge_trash(&tree, before);
            }
            trusted = Some(clock);
        }
        // A4: what the last pass of this folder sent before it stopped short of saving its
        // base (a crash or a kill after its commit) and that is on the drive now is this
        // device's own last change, never another device's: the base takes it first.
        if attempt == 1 && !opts.dry_run && remote.generation >= index.generation {
            adopt_sent(index_path, &remote, &mut index.files);
        }
        // A3, C1: folders renamed since the base, here or on the drive, move as folders: the
        // drive's renames are made here now, this side's go to the drive with this commit, and
        // the files are planned under the names both sides end with.
        let (there_moves, finals) = if remote_side.keeps_folders()
            && !opts.dry_run
            && remote.generation >= index.generation
        {
            align_folders(
                root,
                &mut index,
                &mut scanned,
                &remote,
                &volume,
                hooks.backup,
                &mut report,
            )
        } else {
            (Vec::new(), BTreeMap::new())
        };
        finals_done.clone_from(&finals);
        // The drive as this commit will leave it, and as it was read (to tell a change by).
        let (remote, read) = if there_moves.is_empty() {
            (remote, None)
        } else {
            let gen = remote.generation + 1;
            let moved = moved_index(&remote, &there_moves, gen, crate::now(), &opts.device);
            (moved, Some(remote))
        };
        // S15, V3: an index older than the last one seen is a rollback (a replayed or
        // rolled-back index would resurrect deletes and turn every edit since into a conflict)
        // unless the drive announced a restore since. A restore heals from here: the files
        // the restored index lacks are sent again, never deleted; and the files it has in
        // another version than the base are not taken for "the drive changed" - this side's
        // edit may exist nowhere else.
        let mut restore_epoch = index.restore_epoch;
        let healed;
        let renamed;
        // One decision for plain and encrypted drives ([`restore::judge`]): the announcement is
        // looked at whatever the generation says (S3 - another device that synced first and
        // committed on top of the restored index makes the generations equal, and the files
        // committed after the copy would read as deleted elsewhere), and the generation of a
        // drive synced by name counts this device's commits only, so it decides nothing (S5).
        let seen = restore::Seen {
            epoch: index.restore_epoch,
            clock: index.generation,
        };
        let clock = remote_side.clock();
        let standing = restore::judge(
            &seen,
            remote.generation,
            clock,
            opts.restore.as_ref(),
            &opts.prefix,
        );
        let base: &BTreeMap<String, BaseEntry> = match standing {
            Standing::Rollback {
                seen: newest,
                served,
            } => {
                report.refuse(
                    Code::RemoteRollback,
                    format!(
                        "the drive answered generation {served}, older than the {newest} this \
                         folder saw: a rolled-back or replayed index; nothing was changed (if \
                         you replaced the drive's folder yourself, pair this folder again)"
                    ),
                );
                report.generation = remote.generation;
                return Ok(report);
            }
            Standing::Restored(notice) => {
                // The base is untrusted where the restored drive disagrees ([`restore::rebase`],
                // pure): a file it lacks is sent again, a file it holds in another version than
                // the base's is kept - this side's edit may exist nowhere else.
                let rebased = restore::rebase(&index.files, &|key, entry| {
                    restore_relation(&remote, key, entry)
                });
                if restore::went_back(&seen, remote.generation, clock)
                    || !rebased.lost.is_empty()
                    || !rebased.edited.is_empty()
                {
                    report.tell(
                        Code::ServerRestored,
                        "",
                        format!(
                            "the drive was restored from its backup; {} file(s) newer than the \
                             backup are sent again from here, {} file(s) in another version \
                             than the backup's are kept (a conflict copy where both differ)",
                            rebased.lost.len(),
                            rebased.edited.len()
                        ),
                    );
                }
                restore_epoch = notice.epoch;
                healed = rebased.trusted;
                &healed
            }
            Standing::InSync | Standing::Behind => {
                if finals.is_empty() {
                    &index.files
                } else {
                    renamed = rebase_keys(&index.files, &finals);
                    &renamed
                }
            }
        };
        // The base's folders under the names both sides end with.
        let folders_renamed = (!finals.is_empty()).then(|| rebase_keys(&index.folders, &finals));
        let base_folders = folders_renamed.as_ref().unwrap_or(&index.folders);
        // M1: entries no run acts on stay on the drive as they are.
        let unreadable = remote.unreadable(remote_side.names_versions());
        for (key, why) in &unreadable {
            let code = if remote::check_key(key).is_err() || names::key_problem(key).is_some() {
                Code::BadRemoteName
            } else {
                Code::RemoteEntryUnreadable
            };
            report.tell(code, key, format!("never written here: {why}"));
        }
        let mut readable = remote.readable(&unreadable);
        let mut scan = scanned.clone();
        let mut frozen = check_remote(
            &mut scan,
            &readable,
            base,
            &volume,
            opts.max_file_bytes,
            &mut report,
        );
        // What the drive says of them cannot be read: unknown, so never a delete here either.
        frozen.extend(unreadable.keys().cloned());
        // A folder the drive renamed to a name this computer refuses: its files are absent under
        // the old name and unreadable under the new one. The old names are unknown for this pass -
        // neither deleted nor overwritten - and the report says a rename is why.
        for (old, new) in refused_renames(&index, &remote) {
            report.tell(
                Code::BadRemoteName,
                &old,
                format!(
                    "the drive renamed this folder to \"{new}\", which this computer refuses: its \
                     files here are left as they are"
                ),
            );
            let under = format!("{old}/");
            frozen.extend(
                base.keys()
                    .chain(scan.files.keys())
                    .filter(|key| key.starts_with(&under))
                    .cloned(),
            );
        }
        // D5 keeps a file the drive deleted meanwhile at a fresh pairing (a re-login, a lost
        // base). On a folder paired before, a file made here with the bytes of one deleted
        // earlier is the user's (restored from a trash, made again): it goes up.
        if paired {
            readable.deleted.retain(|key, _| {
                index.kept.contains(key)
                    || !(scan.files.contains_key(key) && !base.contains_key(key))
            });
            // So with a folder made here again under a deleted folder's name.
            readable
                .deleted_folders
                .retain(|key, _| !(scan.dirs.contains(key) && !base_folders.contains_key(key)));
        }
        let excluded = |key: &str| {
            root.rules.excluded(key).is_some() || frozen.contains(key) || scan.hides(key)
        };
        // S16: a synced file the folder's ignore file now excludes (someone edited it, here or
        // on another device) stops syncing. It is listed, never quietly left behind; both
        // copies stay where they are.
        for key in base.keys() {
            if root.rules.excluded(key) == Some(rules::IGNORED) {
                report.tell(
                    Code::IgnoreChange,
                    key,
                    format!(
                        "no longer synced: the folder's {} excludes it now; it stays here and on \
                         the drive",
                        rules::IGNORE_FILE
                    ),
                );
            }
        }
        // The folder as the merge sees it: what the scan could not tell is unknown, never absent;
        // the files this device keeps in the cloud only count as here, unchanged.
        let here = scan.view().with_cloud_only(base);
        let input = merge::PlanInput {
            base,
            local: &here,
            remote: &readable,
            excluded: &excluded,
            device: &opts.device,
            date: &date,
        };
        // D5: a file the drive deleted before this device knew it is kept, not sent again.
        kept = merge::deleted_elsewhere(&input).into_iter().collect();
        for key in &kept {
            report.tell(
                Code::DeletedElsewhereKept,
                key,
                "the drive deleted this file before this device was paired; it is kept here and \
                 not sent again until you choose",
            );
        }
        let mut actions = merge::plan(&input);
        keep_saves(&mut actions, &scan, base, &root.path, &mut report);
        // A backup's one-way answers first: a conflict there is no question for the user.
        if hooks.backup {
            oneway::backup_plan(&mut actions, &scan, base);
        }
        report.held = on_demand(&mut actions, &scan, base, &readable, hooks);
        hooks.observe_plan(&mut actions, &readable, base, &mut report.problems);
        // The deletes the user confirmed in this pass: the keys the question named, when the
        // answer given is for exactly them (S8 - an answer is for the question it was given to,
        // not for whatever a later pass finds).
        let mut confirmed_keys: BTreeSet<String> = BTreeSet::new();
        if !opts.allow_mass_delete {
            if let Some(asked) = mass_delete(&actions, &scan, base, &readable, opts) {
                let shown: Vec<&str> = asked.keys.iter().map(String::as_str).collect();
                let answered = opts
                    .consents
                    .iter()
                    .any(|c| c.covers(consent::ConsentKind::MassDelete, &shown));
                if answered {
                    confirmed_keys.extend(asked.keys.iter().cloned());
                } else {
                    report.tell(
                        Code::MassDeleteHeld,
                        "",
                        format!(
                            "{} of {} files would be deleted {}; nothing changed until you \
                             confirm",
                            asked.count,
                            asked.of,
                            if asked.here { "here" } else { "on the drive" }
                        ),
                    );
                    // An app asks the user (nothing changed); the command line stops.
                    if hooks.hold_mass_delete {
                        report.mass_delete = Some(asked);
                        report.generation = remote.generation;
                        return Ok(report);
                    }
                    return Err(mass_delete_error(&asked));
                }
            }
        }
        let deletes_confirmed = |key: &str| opts.allow_mass_delete || confirmed_keys.contains(key);
        if opts.dry_run {
            report.planned = actions
                .iter()
                .filter(|a| !matches!(a, Action::Agree { .. }))
                .map(Action::describe)
                .collect();
            report.generation = remote.generation;
            return Ok(report);
        }
        if hooks.cancelled() {
            report.cancelled = true;
            return Ok(report);
        }
        if attempt == 1 {
            // A file renamed or moved here is no change of a file, and deletes the user just
            // confirmed in the mass-delete question were asked about: the guard sees neither.
            let moved = moves(&actions, &scan, base);
            let confirmed =
                |a: &Action| matches!(a, Action::DeleteRemote { .. }) && deletes_confirmed(a.key());
            let judged: Vec<Action> = actions
                .iter()
                .filter(|a| !moved.contains(a.key()) && !confirmed(*a))
                .cloned()
                .collect();
            // "These changes are mine" is the answer to the pause the user was shown, and to no
            // later one (S8): a burst that paused again is another question.
            let shown_pause = index.guard.paused.as_ref().map(guard::Pause::identity);
            let burst_answered = shown_pause.as_deref().is_some_and(|shown| {
                opts.consents
                    .iter()
                    .any(|c| c.covers(consent::ConsentKind::Burst, &[shown]))
            });
            pause = guard::check_burst(
                &mut index.guard,
                &judged,
                base,
                &root.path,
                crate::now(),
                opts.allow_burst || burst_answered,
            );
        }
        if pause.is_some() {
            // Only what the drive changed comes here; nothing goes up.
            actions.retain(|a| !guard::sends(a));
        }
        if let Some(limit) = hooks.transfer_limit {
            report.held_back = hold_big(&mut actions, &scan, base, &readable, limit);
        }
        // The folders after the files: a folder stays wherever something stays in it.
        let mut folder_actions = if remote_side.keeps_folders() {
            plan_folders(
                &actions,
                &scan,
                &readable,
                &unreadable,
                base_folders,
                &excluded,
                &root.rules,
            )
        } else {
            Vec::new()
        };
        if hooks.backup {
            // One way: nothing of the drive changes the folder, nothing deleted here goes.
            folder_actions.retain(|a| {
                matches!(
                    a,
                    FolderAction::Agree { .. }
                        | FolderAction::Send { .. }
                        | FolderAction::Forget { .. }
                )
            });
        }
        if pause.is_some() {
            // Only what the drive changed comes here.
            folder_actions.retain(|a| {
                !matches!(
                    a,
                    FolderAction::Send { .. } | FolderAction::RemoveThere { .. }
                )
            });
        }
        if attempt == 1 {
            hooks.tell(planned(&actions, &scan, &readable));
        }
        let merged = merge_json_files(remote_side, root, &actions, base, &readable, &mut report)?;
        if !opts.dry_run {
            note_sending(index_path, &actions, &scan, &merged);
        }
        hooks.point("before-upload");
        let abandoned = remote_side.upload(
            &Uploads {
                root,
                actions: &actions,
                scan: &scan,
                merged: &merged,
                remote: &readable,
                opts,
                hooks,
                folder_moves: &there_moves,
            },
            &mut report,
        )?;
        actions.retain(|a| !abandoned.contains(a.key()));
        // From the drive's whole index: the entries no run acts on stay in it as they are.
        let next = next_index(
            &remote,
            read.is_some(),
            &actions,
            &folder_actions,
            &scan,
            &merged,
            &opts.device,
            crate::now(),
            trusted.and_then(|clock| clock.purge_before(opts.tombstone_days)),
            &deletes_confirmed,
        );
        if next == *read.as_ref().unwrap_or(&remote) {
            let base_after = base.clone();
            break (
                remote,
                actions,
                folder_actions,
                merged,
                etag,
                scan,
                base_after,
                restore_epoch,
            );
        }
        if hooks.cancelled() {
            // The blobs that went up wait for the next run (or the garbage collection).
            report.cancelled = true;
            return Ok(report);
        }
        match remote_side.commit(&remote, &next, etag.as_deref(), &mut report)? {
            Some(new_etag) => {
                report.index_written = true;
                hooks.point("after-commit");
                let base_after = base.clone();
                break (
                    next,
                    actions,
                    folder_actions,
                    merged,
                    Some(new_etag),
                    scan,
                    base_after,
                    restore_epoch,
                );
            }
            None => {
                report.cas_retries += 1;
                if attempt >= opts.max_attempts {
                    fail!(
                        "other devices committed {attempt} times while this one tried; nothing \
                         was changed here - run again"
                    );
                }
                // The cached copy is stale: read the index whole again.
                cached = None;
                std::thread::sleep(Duration::from_millis(50 * u64::from(attempt)));
            }
        }
    };
    report.generation = committed.generation;
    let mut base = base_after;
    apply_local(
        remote_side,
        root,
        &actions,
        &committed,
        &scan,
        &merged,
        &volume,
        opts,
        hooks,
        &mut base,
        &mut report,
    );
    // The folders this run renamed on the drive are the base's under their new names now.
    if !finals_done.is_empty() {
        index.folders = rebase_keys(&index.folders, &finals_done);
    }
    // The folders once the files are in place: a folder is removed only when it is empty.
    if !hooks.cancelled() {
        apply_folders(
            root,
            &scan,
            &folder_actions,
            &volume,
            &mut index.folders,
            &mut report,
        );
    }
    guard::learn(&mut index.guard, &actions, &root.path, &scan);
    report.paused = pause;
    report.cancelled = hooks.cancelled();
    if volume.no_exec_bits {
        // What a download left here shows the disk's one mode: the base takes the bit of the
        // version it is, as the drive names it.
        for (key, entry) in &mut base {
            if let Some(f) = committed.files.get(key).filter(|f| f.hash == entry.hash) {
                entry.exec = f.exec;
            }
        }
    }
    index.files = base;
    index.generation = committed.generation;
    index.restore_epoch = restore_epoch;
    index.kept = kept;
    index.scanned_at_ns = scan.started_ns;
    index.remote_etag = etag.clone().filter(|e| !e.is_empty());
    hooks.point("before-base-save");
    index.save(index_path)?;
    // The base knows what this pass sent now.
    let _ = fs::remove_file(sending_path(index_path));
    save_cache(index_path, index.remote_etag.as_deref(), &committed);
    report.cloud_only = index
        .files
        .iter()
        .filter(|(_, e)| e.cloud_only)
        .map(|(key, _)| key.clone())
        .collect();
    report.remote = Some(committed);
    Ok(report)
}

/// The local index of `root`, which must have been synced.
fn synced_index(root: &LocalRoot, index_path: &Path) -> CloudResult<LocalIndex> {
    LocalIndex::load(index_path)?.ok_or_else(|| {
        CloudError::failed(format!(
            "{} was never synced: nothing of it is known yet",
            root.path.display()
        ))
    })
}

/// Brings the cloud-only file `key` of the synced folder `root` down (opening it, pinning
/// it): the drive's version as it is now, written where the folder keeps it; its base entry
/// is a file on this device again. A file that is here already is left as it is. The path.
///
/// # Errors
///
/// When the folder was never synced, the drive has no such file (any more), or it cannot be
/// fetched or written - or a file of that name turned up here meanwhile (it is never
/// overwritten).
pub fn fetch_file(
    target: Target<'_>,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    key: &str,
) -> CloudResult<PathBuf> {
    match target {
        Target::Index(store) => fetch_one(
            &BlobIndex::new(store, &opts.prefix),
            root,
            index_path,
            opts,
            key,
        ),
        Target::Named(drive) => fetch_one(
            &named::NamedFiles::new(drive, &opts.prefix, index_path),
            root,
            index_path,
            opts,
            key,
        ),
    }
}

/// The base's hash of a downloaded file: the drive's name for its content - or, where the drive
/// could not name it (a version of unknown content), the content's own.
fn content_hash(named: &str, blob: &[u8]) -> String {
    if remote::is_hash(named) {
        named.to_string()
    } else {
        local::hash_bytes(blob)
    }
}

fn fetch_one<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    key: &str,
) -> CloudResult<PathBuf> {
    let mut index = synced_index(root, index_path)?;
    // The name policy of a pass holds for a file opened from the cloud too (AZ-W1, X2): a
    // `.GIT/...` entry of the base is not written into `.git` because the user opened it.
    if let Err(e) = remote::check_key(key) {
        fail!("{key} cannot be written here ({e}); it stays on the drive");
    }
    if let Some(why) = names::here_problem(
        key,
        names::System::here(),
        &index.volume.unwrap_or_else(root::unprobed_volume),
    )
    .or_else(|| names::key_problem(key))
    {
        fail!("{key} cannot be written here ({why}); it stays on the drive");
    }
    // The rules of the folder hold for a file opened from the cloud as they do for a pass: what
    // they leave out is neither downloaded nor written because someone asked for it by name (a
    // cloud-only `apps/..` entry of the `.azlin` folder is not installed by opening it).
    if let Some(why) = root.rules.excluded(key) {
        fail!("{key} is left out of this folder's sync ({why}); it stays on the drive");
    }
    let path = local::path_of(&root.path, key);
    let kept_in_cloud = index.files.get(key).is_some_and(|e| e.cloud_only);
    let now = local::stat_now(&root.path, key)?;
    if !kept_in_cloud && now.is_some_and(|st| st.kind == Kind::File) {
        return Ok(path);
    }
    let cached = load_cache(index_path, &index);
    let (remote, _) = remote_side.read(cached.as_ref())?;
    let file = remote
        .files
        .get(key)
        .ok_or_else(|| CloudError::failed(format!("{key} is not on the drive (any more)")))?;
    // The file's folder, made where missing, never through a link or into another disk.
    let tree = beneath::Root::open(&root.path)?;
    let policy = names::Policy::here(index.volume.unwrap_or_else(root::unprobed_volume));
    let (dir, name) = names::make_parent_of(&tree, &policy, key)?;
    let name = name.as_str();
    let turned_up = || {
        CloudError::failed(format!(
            "{key} turned up on this device meanwhile; it is left as it is"
        ))
    };
    faults::check_write(&root.path)?;
    let hash = if file.size > BIG_BLOB {
        // Into the folder's spool first, never whole in memory.
        let blob = TempBlob::for_download(&root.path, key, &file.hash)?;
        let hash = remote_side.fetch_to(key, &file.hash, file.size, blob.path())?;
        match replace_from_spool(
            &tree,
            &blob,
            &dir,
            name,
            None,
            file.exec,
            file.modified_ns(),
        ) {
            Ok(_) => hash,
            Err(Replace::Changed) => return Err(turned_up()),
            Err(Replace::Failed(e)) => return Err(e.into()),
        }
    } else {
        let bytes = remote_side.fetch(key, &file.hash, file.size)?;
        match replace_checked(
            &dir,
            name,
            None,
            &mut &bytes[..],
            file.exec,
            file.modified_ns(),
        ) {
            Ok(_) => content_hash(&file.hash, &bytes),
            Err(Replace::Changed) => return Err(turned_up()),
            Err(Replace::Failed(e)) => return Err(e.into()),
        }
    };
    // The file came down from the drive: as in a pass, another person's file carries the system's
    // "downloaded from elsewhere" mark, so the system looks at an `.exe`, a script or a shortcut
    // before it runs it, and the user's own devices' files do not (AZ-W20; X-DELTA-AZDRIVE-FILES-
    // 19). Before the base entry is taken: setting the mark changes the file's change time.
    if !quarantine::Writers::is_own(opts, &file.device) {
        quarantine::mark_downloaded(&path);
    }
    root.drive().note_written(key);
    if let Some(entry) = local::entry_now(&root.path, key, &hash) {
        index.files.insert(key.to_string(), entry);
    }
    index.save(index_path)?;
    Ok(path)
}

/// Frees the bytes of the synced file `key` here ("Free up space"): the file goes from this
/// device, its base entry stays as a cloud-only one - no delete for the next run; the drive
/// keeps it. Only a file as its last sync left it is freed; one already in the cloud only is
/// fine.
///
/// # Errors
///
/// When the folder was never synced, the file is not synced yet (new, or changed since its
/// last sync), or it cannot be removed.
pub fn evict_file(root: &LocalRoot, index_path: &Path, key: &str) -> CloudResult<()> {
    evict_file_with(root, index_path, key, &|_| {})
}

/// [`evict_file`] that tells `point` where it stands (`before-evict-unlink`, `after-evict-unlink`:
/// [`faults`], heard through `SyncSession::with_points`).
///
/// The order is the one that keeps the file whatever stops it (X-DELTA-SYNC2-1): the base says
/// "in the cloud only" FIRST, the file leaves after. Once the name is empty a base that still says
/// "synced here" reads the empty name as the user's delete - the next pass would delete the file
/// ON THE DRIVE and on every device, after a "Free up space" that was meant to keep it there. Cut
/// between the two steps (a quit, a power cut, a disk that fills) the file is here beside a
/// cloud-only entry, which the merge reads as present and unchanged: it is hashed, equals the
/// drive's, and the entry is a whole one again.
pub(crate) fn evict_file_with(
    root: &LocalRoot,
    index_path: &Path,
    key: &str,
    point: &dyn Fn(&str),
) -> CloudResult<()> {
    let mut index = synced_index(root, index_path)?;
    let Some(entry) = index.files.get(key).cloned() else {
        fail!("{key} is not synced yet: its only copy is on this device");
    };
    if entry.cloud_only {
        return Ok(());
    }
    // Reached without a link: a folder swapped for one never makes this free a file elsewhere.
    let tree = beneath::Root::open(&root.path)?;
    let (dir, name) = names::walk_record(&tree, key)?;
    let name = name.as_str();
    let st = dir.lstat(name).with_context(|| format!("{key}"))?;
    if st.kind != Kind::File {
        fail!("{key} is no file here: nothing is freed");
    }
    // Unread only when the file is, for certain, the one that was hashed (AZ-S13: size and
    // modification time also pass an edit that kept both); else its bytes are read first.
    let unmoved = entry.holds(&st, index.scanned_at_ns);
    if !unmoved {
        if hash_here(&root.path, key).as_deref() != Some(entry.hash.as_str()) {
            fail!(
                "{key} changed since its last sync and is not synced yet: it stays on this device"
            );
        }
        // The hash took as long as the file is big. An editor that saved by rename meanwhile left
        // the old file to the hasher - whose bytes are the base's - and the NEW file, which exists
        // nowhere else, must not be the one removed below (X-DELTA-AZDRIVE-SYNC-16). Looked at
        // again it is the file that was hashed, or it stays.
        let again = dir.lstat(name).with_context(|| format!("{key}"))?;
        if !st.unchanged(&again) {
            fail!("{key} was changed while it was read: it stays on this device");
        }
    }
    // The record first. A base that cannot be written changes nothing here: the file stays.
    faults::check_write(&root.path)?;
    index.files.insert(
        key.to_string(),
        BaseEntry {
            mtime_ns: 0,
            ctime_ns: 0,
            ino: 0,
            dev: 0,
            cloud_only: true,
            ..entry.clone()
        },
    );
    index.save(index_path)?;
    point("before-evict-unlink");
    // Whatever keeps the file here, the base is put back as it was: a whole entry beside a file.
    let put_back_base = |index: &mut LocalIndex, why: String| -> CloudError {
        index.files.insert(key.to_string(), entry.clone());
        match index.save(index_path) {
            Ok(()) => CloudError::failed(why),
            // The cloud-only entry stays beside the file that is here: the next pass reads the
            // file, finds it the drive's (or an edit, and sends it), and writes the whole entry.
            Err(e) => CloudError::failed(format!(
                "{why} (and the base could not be put back: {e}; the next sync does)"
            )),
        }
    };
    match take_away(&dir, name, &st) {
        Ok(TakenAway::Gone) => {
            dir.sync();
            root.drive().note_deleted(key);
            point("after-evict-unlink");
            Ok(())
        }
        Ok(TakenAway::Replaced) => Err(put_back_base(
            &mut index,
            format!("{key} was changed while it was freed: it stays on this device"),
        )),
        Ok(TakenAway::Kept(aside)) => Err(put_back_base(
            &mut index,
            format!(
                "{key} stays on this device as {aside}: a newer file took its name while it was \
                 freed"
            ),
        )),
        Err(e) => Err(put_back_base(&mut index, e.to_string())),
    }
}

/// What [`take_away`] did with a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TakenAway {
    /// It was the file looked at (or the user had removed it already), and it is gone.
    Gone,
    /// It was another file - an editor saved a new one under the name meanwhile - and it is back
    /// where it was, untouched.
    Replaced,
    /// Another file, and its name was taken again before it could go back: it stays beside, under
    /// this name.
    Kept(String),
}

/// Removes the file `name` of `dir` only when it is still the file `seen` was the `lstat` of.
/// An unlink by name would take whatever has the name by then - an editor that saves by rename
/// makes a NEW file there, which exists nowhere else (X-DELTA-AZDRIVE-SYNC-16). So the file is
/// first renamed to a name of its own, in the one step no editor can come between; what was taken
/// is looked at (the same file, size and modification time: a rename moves the status-change
/// time) and only then removed - or put back, where it was.
///
/// # Errors
///
/// When the file cannot be taken aside, or taken aside and neither removed nor put back (it is
/// said where it is).
pub(crate) fn take_away(dir: &beneath::Dir, name: &str, seen: &Stat) -> CloudResult<TakenAway> {
    use std::sync::atomic::AtomicU32;
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    // A hidden name of its own, short whatever the file's name (a name is at most 255 bytes).
    let aside = format!(
        ".azul-freeing-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    match dir.rename_noreplace(name, dir, &aside) {
        Ok(()) => {}
        // Removed by the user meanwhile: nothing is left to take, and the cloud keeps it.
        Err(e) if e.is_not_found() => return Ok(TakenAway::Gone),
        Err(e) => return Err(e.into()),
    }
    let put_back = || -> CloudResult<TakenAway> {
        match dir.rename_noreplace(&aside, dir, name) {
            Ok(()) => Ok(TakenAway::Replaced),
            Err(e) if e.is_already_exists() => {
                // The name is a newer file's again: this one is kept beside it, visibly.
                for n in 1..=9 {
                    let kept = if n == 1 {
                        format!("{name} (kept)")
                    } else {
                        format!("{name} (kept {n})")
                    };
                    if beneath::check_file_name(&kept).is_ok()
                        && dir.rename_noreplace(&aside, dir, &kept).is_ok()
                    {
                        return Ok(TakenAway::Kept(kept));
                    }
                }
                Err(CloudError::failed(format!(
                    "{name} was changed while it was freed and is kept as {aside}"
                )))
            }
            Err(e) => Err(CloudError::failed(format!(
                "{name} was taken aside as {aside} and could not be put back: {e}"
            ))),
        }
    };
    // Back where it was, and the word for why it was not removed. A file left under the name of
    // its own is the one thing this must not do.
    let stays = |why: &str| -> CloudResult<TakenAway> {
        match put_back()? {
            TakenAway::Replaced => Err(CloudError::failed(format!("{name} stays: {why}"))),
            kept => Ok(kept),
        }
    };
    let taken = match dir.lstat(&aside) {
        Ok(taken) => taken,
        // Not looked at, not removed: it goes back.
        Err(e) => return stays(&format!("it could not be looked at once taken aside ({e})")),
    };
    // The one question "is it that file, untouched" (`Fingerprint::matches`), without the
    // status-change time (0 is not compared): the rename itself moved it.
    let mut look = seen.fingerprint();
    look.ctime_ns = 0;
    if taken.kind != Kind::File || !look.matches(&taken).is_same() {
        // Another file (an editor's save), whole and untouched: where it was.
        return put_back();
    }
    match dir.unlink(&aside) {
        Ok(()) => Ok(TakenAway::Gone),
        Err(e) => stays(&format!("it could not be removed ({e})")),
    }
}

/// What a garbage collection found and did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct GcReport {
    pub prefix: String,
    pub blobs: usize,
    pub referenced: usize,
    /// Unreferenced, but younger than the grace period (a run may be about
    /// to commit them).
    pub kept_young: usize,
    pub deleted: usize,
    pub bytes_freed: u64,
    pub dry_run: bool,
}

/// Deletes the blobs of `prefix` that the index names nowhere (no file, no
/// tombstone) and that are older than `grace_secs` - a blob a running sync
/// uploaded but did not commit yet is younger.
///
/// # Errors
///
/// When the index or the listing cannot be read, or a delete is refused.
pub fn collect_garbage<S: RemoteStore + ?Sized>(
    store: &S,
    prefix: &str,
    grace_secs: i64,
    dry_run: bool,
) -> CloudResult<GcReport> {
    let prefix = normalize_prefix(prefix)?;
    let (index, _) = read_index(store, &remote::index_key(&prefix), None)?;
    let newer = index.unknown_features();
    if !newer.is_empty() {
        fail!(
            "the drive's sync index of {prefix:?} uses what this version does not know ({}): \
             nothing was collected - update it",
            newer.join(", ")
        );
    }
    let referenced = index.referenced();
    let mut report = GcReport {
        prefix: prefix.clone(),
        dry_run,
        ..GcReport::default()
    };
    let now = crate::now();
    for blob in store.list(&remote::blobs_prefix(&prefix))? {
        report.blobs += 1;
        let hash = blob.key.rsplit('/').next().unwrap_or("");
        if referenced.contains(hash) {
            report.referenced += 1;
            continue;
        }
        if blob.modified.is_none_or(|at| now - at < grace_secs) {
            report.kept_young += 1;
            continue;
        }
        if !dry_run {
            store.delete(&blob.key)?;
        }
        report.deleted += 1;
        report.bytes_freed += blob.size;
    }
    Ok(report)
}
