//! Sync: one folder on this device against one prefix of the drive (PLAN
//! §12.3 in its first, unencrypted form; [`remote`] has the layout).
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

pub mod local;
pub mod merge;
pub mod remote;
pub mod rules;
pub mod store;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use azul_storage::{Drive as _, LocalDrive};
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use self::{
    local::{BaseEntry, Content, LocalFile, LocalIndex, Scan},
    merge::Action,
    remote::{normalize_prefix, RemoteFile, RemoteIndex, Tombstone},
    rules::Rules,
    store::{MemStore, RemoteObject, RemoteStore},
};
use crate::{
    drive::Conditional,
    state::{read_json, write_json},
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
    pub fn new(prefix: &str, bucket: &str, device: &str) -> Result<SyncOptions> {
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

/// Today, `2026-10-08` (UTC), for conflict copies.
fn today() -> String {
    let stamp = azlin_proto::time::rfc3339(crate::now());
    stamp.get(..10).unwrap_or(&stamp).to_string()
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
async fn read_index<S: RemoteStore>(
    store: &S,
    key: &str,
    cached: Option<&(String, RemoteIndex)>,
) -> Result<(RemoteIndex, Option<String>)> {
    let etag = cached.map(|(etag, _)| etag.as_str());
    match store.get_unless(key, etag).await? {
        Conditional::NotFound => Ok((RemoteIndex::empty(), None)),
        Conditional::NotModified => match cached {
            Some((etag, index)) => Ok((index.clone(), Some(etag.clone()))),
            None => bail!("the drive answered 304 to an unconditional GET of {key}"),
        },
        Conditional::Found { body, etag } => {
            let etag = etag.filter(|e| !e.is_empty()).ok_or_else(|| {
                anyhow!(
                    "the drive's S3 service sent no ETag for {key}; the sync needs conditional \
                     writes (If-Match)"
                )
            })?;
            Ok((RemoteIndex::parse(&body)?, Some(etag)))
        }
    }
}

/// The blob `hash`, checked against its name.
async fn fetch_blob<S: RemoteStore>(
    store: &S,
    prefix: &str,
    hash: &str,
    size: u64,
) -> Result<Vec<u8>> {
    let key = remote::blob_key(prefix, hash);
    let bytes = store
        .fetch(&key, size)
        .await?
        .ok_or_else(|| anyhow!("the blob {key} is missing from the drive"))?;
    if local::hash_bytes(&bytes) != hash {
        bail!("the blob {key} is damaged: its BLAKE3 is not its name");
    }
    Ok(bytes)
}

/// Stops a plan that would delete most of the folder here or there.
fn guard_mass_delete(
    actions: &[Action],
    scan: &Scan,
    base: &BTreeMap<String, BaseEntry>,
    opts: &SyncOptions,
) -> Result<()> {
    if opts.allow_mass_delete {
        return Ok(());
    }
    let here = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteLocal { .. }))
        .count();
    if here > MASS_DELETE_MIN && here * 2 > scan.files.len() {
        bail!(
            "the drive says {here} of this folder's {} files were deleted elsewhere. That looks \
             like a mistake (or ransomware, PLAN C12), so nothing was changed; run again with \
             --allow-mass-delete if it is right",
            scan.files.len()
        );
    }
    let there = actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteRemote { .. }))
        .count();
    if there > MASS_DELETE_MIN && there * 2 > base.len() {
        bail!(
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

/// Merges every [`Action::MergeJson`] file: this side's object, the drive's
/// and the base's (fetched by its hash; a blob collected since merges as an
/// empty object), all without machine-local keys.
#[allow(clippy::too_many_arguments)]
async fn merge_json_files<S: RemoteStore>(
    store: &S,
    root: &LocalRoot,
    actions: &[Action],
    base: &BTreeMap<String, BaseEntry>,
    remote: &RemoteIndex,
    prefix: &str,
    report: &mut SyncReport,
) -> Result<BTreeMap<String, Merged>> {
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
            .ok_or_else(|| anyhow!("{key} vanished from the index"))?;
        let theirs_blob = fetch_blob(store, prefix, &theirs_file.hash, theirs_file.size).await?;
        let theirs = object(theirs_blob.as_slice());
        let base_object = match base.get(key) {
            Some(entry) => match fetch_blob(store, prefix, &entry.hash, 0).await {
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
}

async fn upload_one<S: RemoteStore>(store: &S, prefix: &str, job: &BlobJob) -> Result<Outcome> {
    let bytes = match &job.source {
        BlobSource::File(path) => match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => return Ok(Outcome::Changed(format!("cannot be read ({e})"))),
        },
        BlobSource::JsonFile(path) => {
            match fs::read(path).ok().and_then(|b| local::json_blob(&b)) {
                Some(bytes) => bytes,
                None => return Ok(Outcome::Changed(String::from("no longer a JSON object"))),
            }
        }
        BlobSource::Bytes(bytes) => bytes.clone(),
    };
    if local::hash_bytes(&bytes) != job.hash {
        return Ok(Outcome::Changed(String::from(
            "changed while it was synced",
        )));
    }
    let key = remote::blob_key(prefix, &job.hash);
    if bytes.len() as u64 > store::BIG_BLOB && store.head(&key).await?.is_some() {
        return Ok(Outcome::Existed);
    }
    let n = bytes.len() as u64;
    store.put(&key, bytes).await?;
    Ok(Outcome::Uploaded(n))
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

/// Uploads every blob the plan needs that the drive may not hold; the keys
/// whose upload has to wait for the next run (their file changed).
#[allow(clippy::too_many_arguments)]
async fn upload_blobs<S: RemoteStore>(
    store: &S,
    root: &LocalRoot,
    actions: &[Action],
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
    remote: &RemoteIndex,
    known: &mut BTreeSet<String>,
    opts: &SyncOptions,
    report: &mut SyncReport,
) -> Result<BTreeSet<String>> {
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
        let key = action.key();
        let (size, source) = match action {
            Action::MergeJson { .. } => {
                let m = &merged[key];
                (m.blob.len() as u64, BlobSource::Bytes(m.blob.clone()))
            }
            _ => {
                let file = &scan.files[key];
                let path = local::path_of(&root.path, key);
                let source = match file.content {
                    Content::Raw => BlobSource::File(path),
                    Content::JsonMerge => BlobSource::JsonFile(path),
                };
                (file.blob_size, source)
            }
        };
        jobs.push(BlobJob {
            key: key.to_string(),
            hash: hash.to_string(),
            size,
            source,
        });
    }
    let (small, big): (Vec<BlobJob>, Vec<BlobJob>) =
        jobs.into_iter().partition(|j| j.size <= store::BIG_BLOB);
    let prefix = opts.prefix.as_str();
    let mut outcomes: Vec<(String, String, Result<Outcome>)> =
        stream::iter(small.into_iter().map(|job| async move {
            let outcome = upload_one(store, prefix, &job).await;
            (job.key, job.hash, outcome)
        }))
        .buffer_unordered(opts.parallel.max(1))
        .collect()
        .await;
    // A big blob goes alone: its parts travel in parallel already.
    for job in big {
        let outcome = upload_one(store, prefix, &job).await;
        outcomes.push((job.key, job.hash, outcome));
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
            | Action::Forget { .. } => {}
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
async fn apply_local<S: RemoteStore>(
    store: &S,
    root: &LocalRoot,
    actions: &[Action],
    committed: &RemoteIndex,
    scan: &Scan,
    merged: &BTreeMap<String, Merged>,
    opts: &SyncOptions,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    let drive = root.drive();
    let scanned_entry = |key: &str| {
        scan.files.get(key).map(|f| BaseEntry {
            hash: f.hash.clone(),
            size: f.size,
            mtime_ns: f.mtime_ns,
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
                    Err(e) => report.errors.push(format!("delete {key}: {e}")),
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
                    Err(e) => report.errors.push(format!("keep {key} as {copy}: {e}")),
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
                    Err(e) => report.errors.push(format!("write {key}: {e}")),
                }
            }
        }
    }
    let (small, big): (Vec<Download>, Vec<Download>) = downloads
        .into_iter()
        .partition(|d| d.size <= store::BIG_BLOB);
    let prefix = opts.prefix.as_str();
    let fetches = stream::iter(small.into_iter().map(|d| async move {
        let blob = fetch_blob(store, prefix, &d.hash, d.size).await;
        (d, blob)
    }))
    .buffer_unordered(opts.parallel.max(1));
    let mut fetches = std::pin::pin!(fetches);
    while let Some((d, blob)) = fetches.next().await {
        write_download(&drive, root, d, blob, base, report);
    }
    for d in big {
        let blob = fetch_blob(store, prefix, &d.hash, d.size).await;
        write_download(&drive, root, d, blob, base, report);
    }
}

/// Writes one fetched blob to its file (a JSON-merge file with this
/// computer's machine-local keys put back), unless the file changed since
/// the scan.
fn write_download(
    drive: &LocalDrive,
    root: &LocalRoot,
    d: Download,
    blob: Result<Vec<u8>>,
    base: &mut BTreeMap<String, BaseEntry>,
    report: &mut SyncReport,
) {
    let blob = match blob {
        Ok(blob) => blob,
        Err(e) => {
            report.errors.push(format!("download {}: {e:#}", d.key));
            return;
        }
    };
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
        return;
    }
    match drive.put(&d.key, &bytes) {
        Ok(()) => {
            if let Some(entry) = local::entry_now(&root.path, &d.key, &d.hash) {
                base.insert(d.key.clone(), entry);
            }
            report.files_down += 1;
            report.bytes_down += bytes.len() as u64;
        }
        Err(e) => report.errors.push(format!("write {}: {e}", d.key)),
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
pub async fn sync_folder<S: RemoteStore>(
    store: &S,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
) -> Result<SyncReport> {
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
    let index_key = remote::index_key(&opts.prefix);
    let excluded = |key: &str| root.rules.excluded(key).is_some();
    let date = today();
    let empty_base: BTreeMap<String, BaseEntry> = BTreeMap::new();
    let mut cached = load_cache(index_path, &index);
    let mut known: BTreeSet<String> = BTreeSet::new();
    let mut attempt = 0u32;
    let (committed, actions, merged, reset, etag) = loop {
        attempt += 1;
        let (remote, etag) = read_index(store, &index_key, cached.as_ref()).await?;
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
        let mut actions = merge::plan(&merge::PlanInput {
            base,
            local: &scan.files,
            remote: &remote,
            excluded: &excluded,
            device: &opts.device,
            date: &date,
        });
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
        let merged = merge_json_files(
            store,
            root,
            &actions,
            base,
            &remote,
            &opts.prefix,
            &mut report,
        )
        .await?;
        let abandoned = upload_blobs(
            store,
            root,
            &actions,
            &scan,
            &merged,
            &remote,
            &mut known,
            opts,
            &mut report,
        )
        .await?;
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
        match store
            .put_if(&index_key, next.to_bytes(), etag.as_deref())
            .await?
        {
            Some(new_etag) => {
                report.index_written = true;
                break (next, actions, merged, reset, Some(new_etag));
            }
            None => {
                report.cas_retries += 1;
                if attempt >= opts.max_attempts {
                    bail!(
                        "other devices committed {attempt} times while this one tried; nothing \
                         was changed here - run again"
                    );
                }
                // The cached copy is stale: read the index whole again.
                cached = None;
                tokio::time::sleep(Duration::from_millis(50 * u64::from(attempt))).await;
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
        store,
        root,
        &actions,
        &committed,
        &scan,
        &merged,
        opts,
        &mut base,
        &mut report,
    )
    .await;
    index.files = base;
    index.generation = committed.generation;
    index.scanned_at_ns = scan.started_ns;
    index.remote_etag = etag.clone().filter(|e| !e.is_empty());
    index.save(index_path)?;
    save_cache(index_path, index.remote_etag.as_deref(), &committed);
    Ok(report)
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
pub async fn collect_garbage<S: RemoteStore>(
    store: &S,
    prefix: &str,
    grace_secs: i64,
    dry_run: bool,
) -> Result<GcReport> {
    let prefix = normalize_prefix(prefix)?;
    let (index, _) = read_index(store, &remote::index_key(&prefix), None).await?;
    let referenced = index.referenced();
    let mut report = GcReport {
        prefix: prefix.clone(),
        dry_run,
        ..GcReport::default()
    };
    let now = crate::now();
    for blob in store.list(&remote::blobs_prefix(&prefix)).await? {
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
            store.delete(&blob.key).await?;
        }
        report.deleted += 1;
        report.bytes_freed += blob.size;
    }
    Ok(report)
}
