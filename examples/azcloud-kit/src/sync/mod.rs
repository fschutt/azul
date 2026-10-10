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
//!    that would delete most of a folder stops (an emptied or unmounted
//!    folder, a ransomware-like burst; `--allow-mass-delete`);
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
//! `config.json` syncs key by key without the machine-local `endpoints`.
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

pub mod drive_store;
pub mod local;
pub mod merge;
pub mod named;
pub mod objects;
pub mod remote;
pub mod rules;
pub mod session;

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

use azul_storage::{Drive as _, LocalDrive};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use self::{
    local::{BaseEntry, Content, LocalFile, LocalIndex, Scan},
    merge::Action,
    remote::{normalize_prefix, RemoteFile, RemoteIndex, Tombstone},
    rules::Rules,
};
pub use crate::store::{Conditional, RemoteObject, RemoteStore};
use crate::{
    error::{fail, CloudError, CloudResult, Context},
    state::{read_json, write_json},
    store::BIG_BLOB,
};

/// The data root of every Azlin app, in the drive.
pub const DATA_PREFIX: &str = "azlin/data/";
/// The `.azlin` folder, in the drive.
pub const HOME_PREFIX: &str = "azlin/config/";
/// How long a tombstone stays in the index.
pub const TOMBSTONE_DAYS: i64 = 90;
/// How often a run tries again after another device won the race.
pub const MAX_ATTEMPTS: u32 = 8;
/// Below this many deletes a plan is never stopped as a mass delete.
pub const MASS_DELETE_MIN: usize = 10;
/// The biggest file a sync takes by default (each is held in memory while
/// it travels).
pub const MAX_FILE_BYTES: u64 = 1 << 30;

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
    pub allow_mass_delete: bool,
    /// Plan only: nothing is uploaded, written or deleted.
    pub dry_run: bool,
    pub max_file_bytes: u64,
    pub tombstone_days: i64,
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
            dry_run: false,
            max_file_bytes: MAX_FILE_BYTES,
            tombstone_days: TOMBSTONE_DAYS,
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
}

impl SyncReport {
    /// One line for people.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.dry_run {
            return format!(
                "{} <-> {}: dry run, {} changes planned",
                self.root,
                self.prefix,
                self.planned.len()
            );
        }
        format!(
            "{} <-> {}: {} up ({} bytes in {} blobs), {} down ({} bytes), {} deleted here, {} \
             deleted on the drive, {} unchanged, {} conflicts, {} merged; index {} (generation \
             {}, {} retries)",
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
    index: RemoteIndex,
}

fn cache_path(index_path: &Path) -> PathBuf {
    index_path.with_extension("remote.json")
}

fn load_cache(index_path: &Path, local: &LocalIndex) -> Option<(String, RemoteIndex)> {
    let cached: CachedIndex = read_json(&cache_path(index_path)).ok().flatten()?;
    (local.remote_etag.as_deref() == Some(cached.etag.as_str()))
        .then_some((cached.etag, cached.index))
}

fn save_cache(index_path: &Path, etag: Option<&str>, index: &RemoteIndex) {
    let path = cache_path(index_path);
    match etag.filter(|e| !e.is_empty()) {
        Some(etag) => {
            let cached = CachedIndex {
                etag: etag.to_string(),
                index: index.clone(),
            };
            let _ = write_json(&path, &cached, false);
        }
        None => {
            let _ = fs::remove_file(path);
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

/// Stops a plan that would delete most of the folder here or there.
fn guard_mass_delete(
    actions: &[Action],
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    opts: &SyncOptions,
) -> CloudResult<()> {
    if opts.allow_mass_delete {
        return Ok(());
    }
    let here = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteLocal { .. }))
        .count();
    if here > MASS_DELETE_MIN && here * 2 > scan.files.len() {
        fail!(
            "the drive says {here} of this folder's {} files were deleted elsewhere. That looks \
             like a mistake (or ransomware), so nothing was changed; run again with \
             --allow-mass-delete if it is right",
            scan.files.len()
        );
    }
    let there = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteRemote { .. }))
        .count();
    if there > MASS_DELETE_MIN && there * 2 > base.len() {
        fail!(
            "{there} of the {} files this folder held at its last sync are gone from it (an \
             emptied or unmounted folder?). Nothing was changed; run again with \
             --allow-mass-delete if they were deleted on purpose",
            base.len()
        );
    }
    Ok(())
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

    /// Puts what the plan sends to the drive where the commit can name it; the keys whose
    /// upload waits for the next run (their file changed, the run was stopped, ...).
    fn upload(&self, work: &Uploads<'_>, report: &mut SyncReport)
        -> CloudResult<BTreeSet<String>>;

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

    fn fetch(&self, _key: &str, hash: &str, size: u64) -> CloudResult<Vec<u8>> {
        fetch_blob(self.store, &self.prefix, hash, size)
    }

    fn fetch_hash(&self, hash: &str, size: u64) -> CloudResult<Vec<u8>> {
        fetch_blob(self.store, &self.prefix, hash, size)
    }

    fn upload(
        &self,
        work: &Uploads<'_>,
        report: &mut SyncReport,
    ) -> CloudResult<BTreeSet<String>> {
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
/// empty object), all without machine-local keys.
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
        let path = local::path_of(&root.path, key);
        let current = fs::read(&path).with_context(|| format!("{}", path.display()))?;
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
    JsonFile(PathBuf),
    Bytes(Vec<u8>),
}

struct BlobJob {
    key: String,
    hash: String,
    size: u64,
    source: BlobSource,
}

enum Outcome {
    Uploaded(u64),
    Existed,
    /// The file changed (or vanished) since the scan.
    Changed(String),
    /// The run was stopped before it.
    Cancelled,
}

/// The bytes a job uploads, unless its file changed since the scan.
fn job_bytes(job: &BlobJob) -> Result<Vec<u8>, String> {
    let bytes = match &job.source {
        BlobSource::File(path) => match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => return Err(format!("cannot be read ({e})")),
        },
        BlobSource::JsonFile(path) => {
            match fs::read(path).ok().and_then(|b| local::json_blob(&b)) {
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
    let bytes = match job_bytes(job) {
        Ok(bytes) => bytes,
        Err(why) => return Ok(Outcome::Changed(why)),
    };
    let key = remote::blob_key(prefix, &job.hash);
    if bytes.len() as u64 > BIG_BLOB && store.head(&key)?.is_some() {
        return Ok(Outcome::Existed);
    }
    let n = bytes.len() as u64;
    store.put(&key, &bytes)?;
    Ok(Outcome::Uploaded(n))
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
    let outcome = upload_one(store, prefix, job);
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
    let (size, source) = match action {
        Action::MergeJson { .. } => {
            let m = &work.merged[key];
            (m.blob.len() as u64, BlobSource::Bytes(m.blob.clone()))
        }
        _ => {
            let file = &work.scan.files[key];
            let path = local::path_of(&work.root.path, key);
            let source = match file.content {
                Content::Raw => BlobSource::File(path),
                Content::JsonMerge => BlobSource::JsonFile(path),
            };
            (file.blob_size, source)
        }
    };
    BlobJob {
        key: key.to_string(),
        hash: hash.to_string(),
        size,
        source,
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
    for (key, hash, outcome) in outcomes {
        match outcome? {
            Outcome::Uploaded(bytes) => {
                known.insert(hash);
                report.blobs_up += 1;
                report.bytes_up += bytes;
            }
            Outcome::Existed => {
                known.insert(hash);
                report.blobs_reused += 1;
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

/// The index after `actions`: `remote` itself when they change nothing in it
/// (so an unchanged folder writes no index).
fn next_index(
    remote: &RemoteIndex,
    actions: &[Action],
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
    device: &str,
    now: i64,
    tombstone_days: i64,
) -> RemoteIndex {
    let mut next = remote.clone();
    let generation = remote.generation + 1;
    let entry = |hash: &str, size: u64, mtime: i64| RemoteFile {
        hash: hash.to_string(),
        size,
        mtime,
        gen: generation,
        device: device.to_string(),
    };
    let mut changed = false;
    for action in actions {
        match action {
            Action::Upload { key } => {
                if let Some(f) = scan.files.get(key) {
                    let mtime = (f.mtime_ns / 1_000_000_000) as i64;
                    next.files
                        .insert(key.clone(), entry(&f.hash, f.blob_size, mtime));
                    next.deleted.remove(key);
                    changed = true;
                }
            }
            Action::Conflict { key, copy } => {
                if let Some(f) = scan.files.get(key) {
                    let mtime = (f.mtime_ns / 1_000_000_000) as i64;
                    next.files
                        .insert(copy.clone(), entry(&f.hash, f.blob_size, mtime));
                    next.deleted.remove(copy);
                    changed = true;
                }
            }
            Action::MergeJson { key } => {
                if let Some(m) = merged.get(key) {
                    let same = remote.files.get(key).is_some_and(|f| f.hash == m.hash);
                    if !same {
                        next.files
                            .insert(key.clone(), entry(&m.hash, m.blob.len() as u64, now));
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
    if changed {
        let horizon = now - tombstone_days * 86_400;
        next.deleted.retain(|_, t| t.at >= horizon);
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
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    let drive = root.drive();
    let scanned_entry = |key: &str| {
        scan.files.get(key).map(|f| BaseEntry {
            hash: f.hash.clone(),
            size: f.size,
            mtime_ns: f.mtime_ns,
            cloud_only: false,
        })
    };
    let mut downloads = Vec::new();
    for action in actions {
        let key = action.key();
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
                            mtime_ns: 0,
                            cloud_only: true,
                        },
                    );
                }
            }
            Action::DeleteLocal { .. } => {
                if !local::still_as_scanned(&root.path, key, scan.files.get(key)) {
                    report.changed_during_sync.push(moved_on());
                    continue;
                }
                match drive.delete(key) {
                    Ok(()) => {
                        local::prune_empty_parents(&root.path, key);
                        base.remove(key);
                        report.deleted_here += 1;
                    }
                    Err(e) => {
                        report.errors.push(format!("delete {key}: {e}"));
                        report.file_errors.push((key.to_string(), e.to_string()));
                    }
                }
            }
            Action::Download { .. } => {
                if let Some(f) = committed.files.get(key) {
                    downloads.push(Download {
                        key: key.to_string(),
                        hash: f.hash.clone(),
                        size: f.size,
                        expect: scan.files.get(key).cloned(),
                    });
                }
            }
            Action::Conflict { copy, .. } => {
                if !local::still_as_scanned(&root.path, key, scan.files.get(key)) {
                    report.changed_during_sync.push(moved_on());
                    continue;
                }
                match drive.rename(key, copy) {
                    Ok(()) => {
                        // A rename keeps the time: the copy is as scanned.
                        if let Some(entry) = scanned_entry(key) {
                            base.insert(copy.clone(), entry);
                        }
                        report.conflicts.push(format!("{key} -> {copy}"));
                        report.files_up += 1;
                        if let Some(f) = committed.files.get(key) {
                            downloads.push(Download {
                                key: key.to_string(),
                                hash: f.hash.clone(),
                                size: f.size,
                                expect: None,
                            });
                        }
                    }
                    Err(e) => {
                        report.errors.push(format!("keep {key} as {copy}: {e}"));
                        report.file_errors.push((key.to_string(), e.to_string()));
                    }
                }
            }
            Action::MergeJson { .. } => {
                if !local::still_as_scanned(&root.path, key, scan.files.get(key)) {
                    report.changed_during_sync.push(moved_on());
                    continue;
                }
                let Some(m) = merged.get(key) else { continue };
                match drive.put(key, &m.file) {
                    Ok(()) => {
                        if let Some(entry) = local::entry_now(&root.path, key, &m.hash) {
                            base.insert(key.to_string(), entry);
                        }
                        report.merged.push(key.to_string());
                        report.files_up += 1;
                    }
                    Err(e) => {
                        report.errors.push(format!("write {key}: {e}"));
                        report.file_errors.push((key.to_string(), e.to_string()));
                    }
                }
            }
        }
    }
    let (small, big): (Vec<Download>, Vec<Download>) = downloads
        .into_iter()
        .partition(|d| d.size <= BIG_BLOB);
    let fetch = |d: &Download| {
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
                write_download(&drive, root, &small[i], blob, hooks, base, report);
            }
        });
    }
    for d in &big {
        if hooks.cancelled() {
            break;
        }
        let blob = fetch(d);
        write_download(&drive, root, d, blob, hooks, base, report);
    }
}

/// Writes one fetched blob to its file (a JSON-merge file with this
/// computer's machine-local keys put back), unless the file changed since
/// the scan.
fn write_download(
    drive: &LocalDrive,
    root: &LocalRoot,
    d: &Download,
    blob: CloudResult<Vec<u8>>,
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
    let blob = match blob {
        Ok(blob) => blob,
        Err(e) => {
            report.errors.push(format!("download {}: {e}", d.key));
            report.file_errors.push((d.key.clone(), e.to_string()));
            finished(Some(e.to_string()));
            return;
        }
    };
    let hash = content_hash(&d.hash, &blob);
    let bytes = if root.json_merge.iter().any(|k| *k == d.key) {
        let current = fs::read(local::path_of(&root.path, &d.key)).ok();
        local::json_with_local_keys(&blob, current.as_deref()).unwrap_or(blob)
    } else {
        blob
    };
    if !local::still_as_scanned(&root.path, &d.key, d.expect.as_ref()) {
        report.changed_during_sync.push(format!(
            "{}: changed during the run; the next run takes it",
            d.key
        ));
        finished(Some(String::from("changed here during the run")));
        return;
    }
    match drive.put(&d.key, &bytes) {
        Ok(()) => {
            if let Some(entry) = local::entry_now(&root.path, &d.key, &hash) {
                base.insert(d.key.clone(), entry);
            }
            report.files_down += 1;
            report.bytes_down += bytes.len() as u64;
            finished(None);
        }
        Err(e) => {
            report.errors.push(format!("write {}: {e}", d.key));
            report.file_errors.push((d.key.clone(), e.to_string()));
            finished(Some(e.to_string()));
        }
    }
}

/// This device's files as the plan sees them: the ones on disk, and the ones it keeps in the
/// cloud only - unchanged here, at their base's version.
fn with_cloud_only(
    files: &BTreeMap<String, LocalFile>,
    base: &BTreeMap<String, BaseEntry>,
) -> BTreeMap<String, LocalFile> {
    let mut here = files.clone();
    for (key, entry) in base.iter().filter(|(_, e)| e.cloud_only) {
        here.entry(key.clone()).or_insert_with(|| LocalFile {
            hash: entry.hash.clone(),
            size: entry.size,
            mtime_ns: 0,
            blob_size: entry.size,
            content: Content::Raw,
        });
    }
    here
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
                if let (Some(here), Some(there)) = (scan.files.get(&key), remote.files.get(&key))
                {
                    held.push(HeldConflict {
                        key: key.clone(),
                        here: here.hash.clone(),
                        there: there.hash.clone(),
                        there_size: there.size,
                        there_device: there.device.clone(),
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

/// What the plan moves: files and bytes up and down.
fn planned(
    actions: &[Action],
    scan: &Scan,
    remote: &RemoteIndex,
) -> SyncEvent {
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

/// The loop of a run against any drive side.
fn run<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
) -> CloudResult<SyncReport> {
    let mut report = SyncReport {
        root: root.path.display().to_string(),
        prefix: opts.prefix.clone(),
        dry_run: opts.dry_run,
        ..SyncReport::default()
    };
    let mut index = LocalIndex::load(index_path)?
        .unwrap_or_else(|| LocalIndex::new(&root.path, &opts.bucket, &opts.prefix));
    let scan = local::scan(
        &root.path,
        &root.rules,
        &index.files,
        index.scanned_at_ns,
        &root.json_merge,
        opts.max_file_bytes,
    )?;
    report.scanned = scan.files.len();
    report.hashed = scan.hashed;
    report.excluded = scan.excluded;
    report.skipped = scan.skipped.clone();
    let excluded = |key: &str| root.rules.excluded(key).is_some();
    let date = today();
    let empty_base: BTreeMap<String, BaseEntry> = BTreeMap::new();
    let mut cached = load_cache(index_path, &index);
    let mut attempt = 0u32;
    let (committed, actions, merged, reset, etag) = loop {
        attempt += 1;
        let (remote, etag) = remote_side.read(cached.as_ref())?;
        // The index went back (deleted, replaced): this side's base no longer
        // describes it, so nothing is deleted on either side this run.
        let reset = remote.generation < index.generation;
        if reset && attempt == 1 {
            report.notes.push(format!(
                "the drive's index (generation {}) is older than this folder's last sync \
                 (generation {}): this run deletes nothing on either side",
                remote.generation, index.generation
            ));
        }
        let base = if reset { &empty_base } else { &index.files };
        let here = with_cloud_only(&scan.files, base);
        let mut actions = merge::plan(&merge::PlanInput {
            base,
            local: &here,
            remote: &remote,
            excluded: &excluded,
            device: &opts.device,
            date: &date,
        });
        report.held = on_demand(&mut actions, &scan, base, &remote, hooks);
        guard_mass_delete(&actions, &scan, base, opts)?;
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
            hooks.tell(planned(&actions, &scan, &remote));
        }
        let merged = merge_json_files(remote_side, root, &actions, base, &remote, &mut report)?;
        let abandoned = remote_side.upload(
            &Uploads {
                root,
                actions: &actions,
                scan: &scan,
                merged: &merged,
                remote: &remote,
                opts,
                hooks,
            },
            &mut report,
        )?;
        actions.retain(|a| !abandoned.contains(a.key()));
        let next = next_index(
            &remote,
            &actions,
            &scan,
            &merged,
            &opts.device,
            crate::now(),
            opts.tombstone_days,
        );
        if next == remote {
            break (remote, actions, merged, reset, etag);
        }
        if hooks.cancelled() {
            // The blobs that went up wait for the next run (or the garbage collection).
            report.cancelled = true;
            return Ok(report);
        }
        match remote_side.commit(&remote, &next, etag.as_deref(), &mut report)? {
            Some(new_etag) => {
                report.index_written = true;
                break (next, actions, merged, reset, Some(new_etag));
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
    let mut base = if reset {
        BTreeMap::new()
    } else {
        index.files.clone()
    };
    apply_local(
        remote_side,
        root,
        &actions,
        &committed,
        &scan,
        &merged,
        opts,
        hooks,
        &mut base,
        &mut report,
    );
    report.cancelled = hooks.cancelled();
    index.files = base;
    index.generation = committed.generation;
    index.scanned_at_ns = scan.started_ns;
    index.remote_etag = etag.clone().filter(|e| !e.is_empty());
    index.save(index_path)?;
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
        Target::Index(store) => fetch_one(&BlobIndex::new(store, &opts.prefix), root, index_path, key),
        Target::Named(drive) => fetch_one(
            &named::NamedFiles::new(drive, &opts.prefix, index_path),
            root,
            index_path,
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
    key: &str,
) -> CloudResult<PathBuf> {
    let mut index = synced_index(root, index_path)?;
    let path = local::path_of(&root.path, key);
    let kept_in_cloud = index.files.get(key).is_some_and(|e| e.cloud_only);
    if !kept_in_cloud && path.is_file() {
        return Ok(path);
    }
    let cached = load_cache(index_path, &index);
    let (remote, _) = remote_side.read(cached.as_ref())?;
    let file = remote.files.get(key).ok_or_else(|| {
        CloudError::failed(format!("{key} is not on the drive (any more)"))
    })?;
    let bytes = remote_side.fetch(key, &file.hash, file.size)?;
    if fs::symlink_metadata(&path).is_ok() {
        fail!("{key} turned up on this device meanwhile; it is left as it is");
    }
    root.drive().put(key, &bytes)?;
    if let Some(entry) = local::entry_now(&root.path, key, &content_hash(&file.hash, &bytes)) {
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
    let mut index = synced_index(root, index_path)?;
    let Some(entry) = index.files.get(key).cloned() else {
        fail!("{key} is not synced yet: its only copy is on this device");
    };
    if entry.cloud_only {
        return Ok(());
    }
    let path = local::path_of(&root.path, key);
    let meta = fs::metadata(&path).with_context(|| format!("{}", path.display()))?;
    let unmoved = meta.len() == entry.size && local::mtime_ns(&meta) == entry.mtime_ns;
    if !unmoved && local::hash_file(&path).ok().as_deref() != Some(entry.hash.as_str()) {
        fail!("{key} changed since its last sync and is not synced yet: it stays on this device");
    }
    root.drive().delete(key)?;
    index.files.insert(
        key.to_string(),
        BaseEntry {
            mtime_ns: 0,
            cloud_only: true,
            ..entry
        },
    );
    index.save(index_path)
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
