//! This device's side of a synced folder.
//!
//! The local index (`<state>/sync/<id>.json`, never in the folder) is the
//! BASE of the three-way merge: for every file, the content both sides agreed
//! on at the last sync. It is also the scan's cache: a file whose size and
//! modification time (nanoseconds) are what the base recorded keeps the
//! base's hash without being read - unless its time lies within two seconds
//! of that scan, when a second edit in the same clock tick could hide behind
//! the same time (git's "racy" files); those are read again.
//!
//! A JSON-merge file (`config.json` of the `.azlin` folder) is synced as its
//! object WITHOUT the machine-local keys (azul-appkit's `MACHINE_LOCAL_KEYS`:
//! `endpoints`, `updates`), keys sorted, pretty-printed: its hash is that blob's, so a
//! change of this computer's endpoints or update cohort is no change for the sync, and a
//! downloaded copy gets this computer's own keys back. One that does not
//! parse as an object is skipped (its keys cannot be held back).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use azul_appkit::azlin_config::MACHINE_LOCAL_KEYS;
use azul_storage::beneath::{self, BeneathError, Kind, Stat};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    guard::Guard,
    names::{self, System, Volume},
    problems::{Code, Problem, Severity},
    rules::Rules,
};
use crate::{
    error::{fail, CloudResult, Context},
    state::{read_json, write_json},
};

/// The `format` of a local index.
pub const LOCAL_FORMAT: &str = "azcloud.sync-local";
/// The newest format version of it this version reads (and the one it writes).
pub const LOCAL_VERSION: u32 = 1;
/// A file whose time lies this close to the scan that recorded it is read
/// again by the next scan.
pub const RACY_NS: u64 = 2_000_000_000;

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// What both sides agreed on for one file at the last sync.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseEntry {
    /// The BLAKE3 of the blob.
    pub hash: String,
    /// The file's bytes on this device.
    pub size: u64,
    /// The file's modification time on this device (nanoseconds; 0 for a
    /// cloud-only file).
    pub mtime_ns: u64,
    /// This device keeps no bytes of it (an app's on-demand file: left in the
    /// cloud, or freed): the next run takes it as unchanged here at `hash`, so
    /// it is never a delete; deleted on the drive it is forgotten.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cloud_only: bool,
    /// The user's executable bit: part of the file's content for the merge.
    #[serde(default, skip_serializing_if = "is_false")]
    pub exec: bool,
    /// The status-change time, inode and device the file had (0: not known - a base an older
    /// version wrote). With size and time they say the file was not touched since, so its hash
    /// is kept; `touch -r` can set the time back, never the status-change time.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ctime_ns: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ino: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dev: u64,
}

/// The local index of one folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIndex {
    pub format: String,
    pub version: u32,
    /// The folder, the bucket and the prefix it was synced with.
    pub root: String,
    pub bucket: String,
    pub prefix: String,
    /// The index generation of the last sync.
    pub generation: u64,
    /// When the last scan started (nanoseconds): the "racy" bound.
    pub scanned_at_ns: u64,
    /// The ETag of the index as last read or written (the next poll's
    /// `If-None-Match`).
    #[serde(default)]
    pub remote_etag: Option<String>,
    /// The base.
    #[serde(default)]
    pub files: BTreeMap<String, BaseEntry>,
    /// The burst guard's memory, and its pause ([`super::guard`]).
    #[serde(default)]
    pub guard: Guard,
    /// The folder's identity ([`super::root`]'s marker): a pass whose folder lacks it changes
    /// nothing. `None` until the first pass that checked the folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    /// The drive's restore epoch at the last sync: an index older than the last one seen is
    /// refused unless its restore epoch is higher (the drive was restored and announced it).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restore_epoch: u64,
    /// What the disk under the folder does with names, probed at the first pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,
    /// Files here the drive deleted before this pairing knew them (D5): kept, listed and not
    /// sent until the user chooses ([`super::session::SyncSession::send_kept`]).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub kept: BTreeSet<String>,
}

impl LocalIndex {
    /// The index of a folder never synced.
    #[must_use]
    pub fn new(root: &Path, bucket: &str, prefix: &str) -> LocalIndex {
        LocalIndex {
            format: LOCAL_FORMAT.to_string(),
            version: LOCAL_VERSION,
            root: root.display().to_string(),
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
            generation: 0,
            scanned_at_ns: 0,
            remote_etag: None,
            files: BTreeMap::new(),
            guard: Guard::default(),
            root_id: None,
            restore_epoch: 0,
            volume: None,
            kept: BTreeSet::new(),
        }
    }

    /// Reads the index at `path`; `None` when the folder was never synced.
    ///
    /// # Errors
    ///
    /// When it cannot be read or is no local index.
    pub fn load(path: &Path) -> CloudResult<Option<LocalIndex>> {
        // A `.db` path is the base as a state database (an older JSON base beside it is
        // imported once).
        #[cfg(feature = "state-db")]
        if super::base::is_database(path) {
            return super::base::load(path);
        }
        let Some(index) = read_json::<LocalIndex>(path)? else {
            return Ok(None);
        };
        if index.format != LOCAL_FORMAT {
            fail!("{} is no azcloud local index", path.display());
        }
        // A newer version's base may mean things this one cannot read: never taken as one of
        // its own (a run pairs afresh and deletes nothing, D5).
        if index.version > LOCAL_VERSION {
            fail!(
                "{} was written by a newer version of the app (state format {}; this one reads up \
                 to {LOCAL_VERSION})",
                path.display(),
                index.version
            );
        }
        Ok(Some(index))
    }

    /// Writes it.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, path: &Path) -> CloudResult<()> {
        #[cfg(feature = "state-db")]
        if super::base::is_database(path) {
            return super::base::save(path, self);
        }
        write_json(path, self, false)
    }
}

/// Where the local index of `root` synced with `bucket` / `prefix` lives in
/// the state folder's `sync/` (`sync_dir`): one file per triple, named after
/// its hash, so two folders never share a base.
#[must_use]
pub fn index_path(sync_dir: &Path, bucket: &str, prefix: &str, root: &Path) -> PathBuf {
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    let id = blake3::hash(format!("{bucket}\n{prefix}\n{}", root.display()).as_bytes())
        .to_hex()
        .to_string();
    sync_dir.join(format!("{}.json", &id[..16]))
}

/// How a file is synced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Content {
    /// Its bytes.
    #[default]
    Raw,
    /// Its JSON object without the machine-local keys.
    JsonMerge,
}

/// One file of the folder as the scan found it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalFile {
    /// The BLAKE3 of its blob (for [`Content::JsonMerge`]: of the filtered
    /// object).
    pub hash: String,
    /// The file's bytes.
    pub size: u64,
    /// The file's modification time (nanoseconds).
    pub mtime_ns: u64,
    /// The blob's bytes.
    pub blob_size: u64,
    pub content: Content,
    /// The user's executable bit.
    pub exec: bool,
    /// The status-change time, inode and device as the scan saw them (0 where the system has
    /// none): an upload and a replace check the file is still this one.
    pub ctime_ns: u64,
    pub ino: u64,
    pub dev: u64,
}

impl LocalFile {
    /// What the base keeps of it.
    #[must_use]
    pub fn base_entry(&self) -> BaseEntry {
        BaseEntry {
            hash: self.hash.clone(),
            size: self.size,
            mtime_ns: self.mtime_ns,
            cloud_only: false,
            exec: self.exec,
            ctime_ns: self.ctime_ns,
            ino: self.ino,
            dev: self.dev,
        }
    }

    /// Whether `st` (an `lstat` now) is still this file, untouched.
    #[must_use]
    pub fn matches(&self, st: &Stat) -> bool {
        st.kind == Kind::File
            && st.size == self.size
            && st.mtime_ns == self.mtime_ns
            && (self.ctime_ns == 0 || st.ctime_ns == self.ctime_ns)
            && (self.ino == 0 || (st.ino == self.ino && st.dev == self.dev))
    }
}

/// What a scan found.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub files: BTreeMap<String, LocalFile>,
    /// Files and folders a rule took.
    pub excluded: usize,
    /// Files not synced, and why (symbolic links, unreadable, too big, ...).
    pub skipped: Vec<(String, String)>,
    /// Files read and hashed (the rest kept their base's hash).
    pub hashed: usize,
    /// When it started (nanoseconds).
    pub started_ns: u64,
    /// The folders found (keys, no trailing slash): a file of the drive cannot come down under
    /// such a name, nor one whose folder is a file here.
    pub dirs: BTreeSet<String>,
    /// The links found, with their targets: never followed. What a link now hides where a
    /// synced folder was is unknown, never deleted, and nothing is written through one.
    pub links: BTreeMap<String, String>,
    /// The folders that are another disk mounted inside the folder: never walked into, and
    /// unmounting one deletes nothing.
    pub mounts: BTreeSet<String>,
    /// Files and folders that could not be read, or changed while read, or are still being
    /// written: unknown this pass, never a delete (nothing below such a folder either).
    pub unknown: BTreeSet<String>,
    /// The files the rules leave out, with their sizes: an app that saves a file writes it
    /// aside and renames the old one to such a name for a moment, which is no delete.
    pub temps: BTreeMap<String, u64>,
    /// What the scan has to tell, file by file (links, special files, names, unreadable).
    pub problems: Vec<Problem>,
    /// The local spelling of a key the drive spells otherwise (decomposed here, composed
    /// there, on a disk that takes both for one name): paths here use it.
    pub spelled: BTreeMap<String, String>,
}

impl Scan {
    /// The name the file of `key` has here.
    #[must_use]
    pub fn here<'a>(&'a self, key: &'a str) -> &'a str {
        self.spelled.get(key).map_or(key, String::as_str)
    }

    /// Whether `key`, or a folder of it, is unknown this pass (unreadable, settling, behind a
    /// link or on another disk): nothing of it is deleted or written.
    #[must_use]
    pub fn hides(&self, key: &str) -> bool {
        let covers = |p: &str| key == p || key.strip_prefix(p).is_some_and(|r| r.starts_with('/'));
        self.unknown.iter().any(|p| covers(p))
            || self.links.keys().any(|p| covers(p))
            || self.mounts.iter().any(|p| covers(p))
    }

    fn tell(&mut self, code: Code, key: &str, detail: impl Into<String>) {
        self.problems.push(Problem::new(code, key, detail));
    }
}

/// Waits until nothing under `root` changed for `quiet` - the newest modification time of its
/// files and folders (a rule's leave out) is that old, or it stayed the same while this looked
/// for `quiet` - or until `most` passed, or `stop` says so: a burst (a checkout, an unzip, a
/// build) is then synced by one run as a whole, and the burst guard sees all of it. A folder's
/// time moves when an entry is made, renamed or removed, so an unzip whose files keep their old
/// dates still counts. A time in the future (a file from a computer whose clock runs ahead, a
/// clock that jumped back) cannot tell its age, so only the watching decides then. How long it
/// waited.
pub fn settle(
    root: &Path,
    rules: &Rules,
    quiet: Duration,
    most: Duration,
    stop: &dyn Fn() -> bool,
) -> Duration {
    let started = Instant::now();
    if quiet.is_zero() {
        return Duration::ZERO;
    }
    // The newest time seen, and since when it has not moved.
    let mut seen: Option<(SystemTime, Instant)> = None;
    loop {
        let Some(newest) = newest_change(root, rules) else {
            return started.elapsed();
        };
        let age = SystemTime::now()
            .duration_since(newest)
            .unwrap_or(Duration::ZERO);
        let still = match seen {
            Some((time, since)) if time == newest => since.elapsed(),
            _ => {
                seen = Some((newest, Instant::now()));
                Duration::ZERO
            }
        };
        let waited = started.elapsed();
        if age >= quiet || still >= quiet || waited >= most || stop() {
            return waited;
        }
        // Short steps, so a stop is heard and a burst that ends is noticed soon.
        let wait = (quiet - age.max(still))
            .min(most - waited)
            .clamp(Duration::from_millis(10), Duration::from_millis(250));
        std::thread::sleep(wait);
    }
}

/// `read` (a [`scan`]) of a folder that did not change while it was read. A change made during
/// the read - a folder deleted midway, an unzip still writing - leaves a torn scan, half of the
/// change, and a run planned from it takes half a delete for what the user did (the burst guard
/// asked about the files a `rm -rf` had removed when the scan passed, the mass-delete question
/// about all of them only after). So the newest time under `root` is read before and after;
/// when it moved, the folder is let settle and read anew, for at most `most` in all - a folder
/// that never stops changing is taken as the last read saw it, as [`settle`] takes it. With
/// `quiet` zero (no settling) or once `stop` says so, the first read stands.
///
/// # Errors
///
/// `read`'s.
pub fn scan_unchanged(
    root: &Path,
    rules: &Rules,
    quiet: Duration,
    most: Duration,
    stop: &dyn Fn() -> bool,
    mut read: impl FnMut() -> CloudResult<Scan>,
) -> CloudResult<Scan> {
    let started = Instant::now();
    loop {
        let before = newest_change(root, rules);
        let scan = read()?;
        if quiet.is_zero()
            || stop()
            || started.elapsed() >= most
            || newest_change(root, rules) == before
        {
            return Ok(scan);
        }
        settle(
            root,
            rules,
            quiet,
            most.saturating_sub(started.elapsed()),
            stop,
        );
    }
}

/// The newest modification time of `root`, its folders and its files (symbolic links and what
/// `rules` leave out are not looked at); `None` when `root` cannot be read.
fn newest_change(root: &Path, rules: &Rules) -> Option<SystemTime> {
    let mut newest = fs::metadata(root).and_then(|m| m.modified()).ok()?;
    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
            let skipped = if kind.is_dir() {
                rules.excluded_dir(&key).is_some()
            } else {
                kind.is_symlink() || rules.excluded(&key).is_some()
            };
            if skipped {
                continue;
            }
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                newest = newest.max(modified);
            }
            if kind.is_dir() {
                stack.push((entry.path(), format!("{key}/")));
            }
        }
    }
    Some(newest)
}

/// The modification time of `meta` in nanoseconds since 1970 (0 when the
/// file system has none).
#[must_use]
pub fn mtime_ns(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// The user's executable bit of `meta` (never set where the system has none).
#[must_use]
pub fn is_executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o100 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        false
    }
}

/// The BLAKE3 of a file, read in pieces.
///
/// # Errors
///
/// When it cannot be read.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// The BLAKE3 of `bytes`, lowercase hex.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// `v` with every object's keys in order (whatever map serde_json is built
/// with), so equal content always gives equal bytes.
#[must_use]
pub fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = Map::new();
            for (key, value) in entries {
                out.insert(key.clone(), canonical(value));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// The bytes of a JSON-merge blob or file: canonical, pretty-printed, with a
/// final newline (the shape azul-appkit writes the shared config in).
#[must_use]
pub fn pretty(v: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(&canonical(v)).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text.into_bytes()
}

/// The JSON object in `bytes`; `None` when they are none.
#[must_use]
pub fn json_object(bytes: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(bytes).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// `map` without the machine-local keys.
#[must_use]
pub fn without_local_keys(mut map: Map<String, Value>) -> Map<String, Value> {
    for key in MACHINE_LOCAL_KEYS {
        map.remove(*key);
    }
    map
}

/// The blob of a JSON-merge file: its object without the machine-local keys;
/// `None` when the bytes are no JSON object.
#[must_use]
pub fn json_blob(bytes: &[u8]) -> Option<Vec<u8>> {
    json_object(bytes).map(|map| pretty(&Value::Object(without_local_keys(map))))
}

/// The file to write for a JSON-merge blob from the drive: its object (any
/// machine-local key in it dropped - one from the bucket is never taken) with
/// this computer's own machine-local keys from `current` (the file as it is
/// now) put back. `None` when the blob is no JSON object.
#[must_use]
pub fn json_with_local_keys(blob: &[u8], current: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut map = without_local_keys(json_object(blob)?);
    if let Some(current) = current.and_then(json_object) {
        for key in MACHINE_LOCAL_KEYS {
            if let Some(value) = current.get(*key) {
                map.insert((*key).to_string(), value.clone());
            }
        }
    }
    Some(pretty(&Value::Object(map)))
}

/// The file of `key` under `root`.
#[must_use]
pub fn path_of(root: &Path, key: &str) -> PathBuf {
    azul_appkit::data::local_path(root, key)
}

/// Walks `root` and finds every file the `rules` let through, reusing the `base`'s hash of a
/// file whose size, times and inode did not move since the scan at `base_scanned_ns`. A key
/// of `json_merge` is synced as JSON without its machine-local keys.
///
/// Every folder is opened from the one above it without following a link
/// ([`azul_storage::beneath`]), and every file is opened without following a link and without
/// blocking on a FIFO, then checked to be the file the folder listed: a link or a FIFO swapped
/// in between is never read. Links, special files, other disks mounted inside and files that
/// cannot be read are listed ([`Scan::problems`]) and never taken for deleted.
///
/// # Errors
///
/// When the folder itself is missing or cannot be read: a missing folder is no empty folder
/// (an unplugged disk would read as "every file deleted").
pub fn scan(
    root: &Path,
    rules: &Rules,
    base: &BTreeMap<String, BaseEntry>,
    base_scanned_ns: u64,
    json_merge: &[String],
    max_file_bytes: u64,
) -> CloudResult<Scan> {
    scan_settled(
        root,
        rules,
        base,
        base_scanned_ns,
        json_merge,
        max_file_bytes,
        Duration::ZERO,
    )
}

/// [`scan`], leaving the files written within the last `settle` for a later pass: a file still
/// being written (an export, a copy, a download) is taken once it is quiet, and is never a
/// delete meanwhile.
///
/// # Errors
///
/// As [`scan`].
pub fn scan_settled(
    root: &Path,
    rules: &Rules,
    base: &BTreeMap<String, BaseEntry>,
    base_scanned_ns: u64,
    json_merge: &[String],
    max_file_bytes: u64,
    settle: Duration,
) -> CloudResult<Scan> {
    let mut out = Scan {
        started_ns: crate::now_ns(),
        ..Scan::default()
    };
    let tree =
        beneath::Root::open(root).with_context(|| format!("the folder {}", root.display()))?;
    let top = tree
        .top()
        .with_context(|| format!("the folder {}", root.display()))?;
    let settle_ns = u64::try_from(settle.as_nanos()).unwrap_or(u64::MAX);
    let here = System::here();
    // A test harness may play a folder that cannot be listed in this scan (the fault seam).
    super::faults::scan_started(root);
    // The folders whose items carry attributes the sync does not carry: said once each.
    let mut noted: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<(beneath::Dir, String)> = vec![(top, String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let folder = prefix.trim_end_matches('/').to_string();
        if !folder.is_empty() {
            note_attributes(&mut out, &mut noted, root, &folder, &folder);
        }
        let listed = match super::faults::readdir_error(root, &folder) {
            Some(e) => Err(e),
            None => dir.names(),
        };
        let mut entries = match listed {
            Ok(entries) => entries,
            Err(e) if prefix.is_empty() => {
                return Err(e).with_context(|| format!("the folder {}", root.display()));
            }
            Err(e) => {
                out.unknown.insert(folder.clone());
                out.skipped
                    .push((folder.clone(), format!("cannot be read ({e})")));
                out.tell(
                    Code::Unreadable,
                    &folder,
                    format!("the folder cannot be read ({e}); nothing in it is taken for deleted"),
                );
                continue;
            }
        };
        entries.sort();
        for raw in entries {
            let name = match raw.into_string() {
                Ok(name) => name,
                Err(raw) => {
                    let key = format!("{prefix}{}", raw.to_string_lossy());
                    out.skipped
                        .push((key.clone(), String::from("its name is not UTF-8")));
                    out.tell(
                        Code::BadName,
                        &key,
                        "its name is not UTF-8, so no other device could name it",
                    );
                    continue;
                }
            };
            let key = format!("{prefix}{name}");
            let st = match dir.lstat(&name) {
                Ok(st) => st,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => {
                    out.unknown.insert(key.clone());
                    out.skipped
                        .push((key.clone(), format!("cannot be read ({e})")));
                    out.tell(
                        Code::Unreadable,
                        &key,
                        "it cannot be read; it is not taken for deleted",
                    );
                    continue;
                }
            };
            match st.kind {
                Kind::Link => {
                    let target = dir
                        .read_link(&name)
                        .map(|t| t.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    out.skipped.push((
                        key.clone(),
                        String::from("a symbolic link (never followed)"),
                    ));
                    out.tell(
                        Code::SymlinkSkipped,
                        &key,
                        format!("a link to {target:?} is never followed"),
                    );
                    out.links.insert(key, target);
                    continue;
                }
                Kind::Dir => {
                    if rules.excluded_dir(&key).is_some() {
                        out.excluded += 1;
                        continue;
                    }
                    match dir.child(&name, false) {
                        Ok(child) if !tree.is_mount_point(child.rel()) => {
                            out.dirs.insert(key.clone());
                            stack.push((child, format!("{key}/")));
                        }
                        Ok(_) | Err(BeneathError::CrossesMount(_)) => {
                            out.skipped.push((
                                key.clone(),
                                String::from("another disk mounted here (never walked into)"),
                            ));
                            out.tell(
                                Code::MountPoint,
                                &key,
                                "another disk is mounted here; it is not synced, and unmounting \
                                 it deletes nothing",
                            );
                            out.mounts.insert(key);
                        }
                        // Swapped for a link since it was listed.
                        Err(BeneathError::ThroughLink(_)) => {
                            out.skipped.push((
                                key.clone(),
                                String::from("a symbolic link (never followed)"),
                            ));
                            out.links.insert(key, String::new());
                        }
                        Err(e) => {
                            out.skipped
                                .push((key.clone(), format!("cannot be read ({e})")));
                            out.tell(
                                Code::Unreadable,
                                &key,
                                format!(
                                    "the folder cannot be opened ({e}); nothing in it is taken \
                                     for deleted"
                                ),
                            );
                            out.unknown.insert(key);
                        }
                    }
                    continue;
                }
                Kind::File => {}
                Kind::Fifo | Kind::Socket | Kind::Device | Kind::Other => {
                    let what = match st.kind {
                        Kind::Fifo => "a FIFO",
                        Kind::Socket => "a socket",
                        Kind::Device => "a device file",
                        _ => "not a regular file",
                    };
                    out.skipped
                        .push((key.clone(), String::from("not a regular file")));
                    out.tell(
                        Code::SpecialFile,
                        &key,
                        format!("{what} is never opened or synced"),
                    );
                    continue;
                }
            }
            if rules.excluded(&key).is_some() {
                out.excluded += 1;
                out.temps.insert(key, st.size);
                continue;
            }
            if let Err(e) = super::remote::check_key(&key) {
                out.skipped
                    .push((key.clone(), format!("its name cannot be a key ({e})")));
                out.tell(Code::BadName, &key, format!("not synced: {e}"));
                continue;
            }
            if let Some(why) = names::key_problem(&key) {
                out.skipped
                    .push((key.clone(), format!("its name cannot be a key ({why})")));
                out.tell(Code::BadName, &key, format!("not synced: {why}"));
                continue;
            }
            // A write sets the time to now: a time in the future (a file from a computer whose
            // clock runs ahead) is no write in progress, and would otherwise wait for ever.
            let now_ns = crate::now_ns();
            if settle_ns > 0
                && st.mtime_ns <= now_ns
                && st.mtime_ns.saturating_add(settle_ns) > now_ns
            {
                out.unknown.insert(key.clone());
                out.tell(
                    Code::Settling,
                    &key,
                    "it changed moments ago; it is taken once it is quiet",
                );
                continue;
            }
            for (system, why) in names::portability(&key, here) {
                out.tell(
                    Code::Portability,
                    &key,
                    format!("{} cannot hold it: {why}", system.label()),
                );
            }
            if st.nlink > 1 {
                // For the record only: it syncs as any file does.
                let mut note = Problem::new(
                    Code::Hardlink,
                    &key,
                    "it has other names (hard links); it syncs as its own copy",
                );
                note.severity = Severity::Warning;
                out.problems.push(note);
            }
            note_attributes(&mut out, &mut noted, root, &folder, &key);
            match scan_file(
                &dir,
                &name,
                &key,
                &st,
                base,
                base_scanned_ns,
                json_merge,
                max_file_bytes,
            ) {
                Ok(Some((file, hashed))) => {
                    if hashed {
                        out.hashed += 1;
                    }
                    out.files.insert(key, file);
                }
                Ok(None) => {}
                Err(Skip::Changed) => {
                    out.unknown.insert(key.clone());
                    out.tell(
                        Code::ChangedDuringSync,
                        &key,
                        "it changed while it was read; the next pass takes it",
                    );
                }
                Err(Skip::Unreadable(why)) => {
                    out.unknown.insert(key.clone());
                    out.skipped.push((key.clone(), why));
                    out.tell(
                        Code::Unreadable,
                        &key,
                        "it cannot be read; it is not taken for deleted",
                    );
                }
                Err(Skip::Refused(code, why)) => {
                    out.skipped.push((key.clone(), why.clone()));
                    out.tell(code, &key, why);
                }
                Err(Skip::Locked) => {
                    out.unknown.insert(key.clone());
                    out.tell(
                        Code::Locked,
                        &key,
                        "another app holds it locked; it goes up once that app lets it go",
                    );
                }
            }
        }
    }
    Ok(out)
}

/// Says once for `folder` that its items' extended attributes and flags are not synced, when
/// the item `key` (in it, or the folder itself) carries any (rule P6: their content syncs).
fn note_attributes(
    out: &mut Scan,
    noted: &mut BTreeSet<String>,
    root: &Path,
    folder: &str,
    key: &str,
) {
    if noted.contains(folder) || !super::attrs::has_user_attributes(&path_of(root, key)) {
        return;
    }
    noted.insert(folder.to_string());
    out.tell(
        Code::XattrsNotSynced,
        folder,
        "items here carry extended attributes, a resource fork or flags (tags, comments, \
         hidden or locked): their content syncs, these do not",
    );
}

/// Why a file the scan found is not taken.
enum Skip {
    /// It changed (or was swapped for something else) while it was read: the next pass.
    Changed,
    /// It cannot be read: unknown, never a delete.
    Unreadable(String),
    /// It is never synced (too big, not a JSON object): the code and why.
    Refused(Code, String),
    /// Another app holds it locked (an exclusive lock, a share-deny open): it is mid-write, so
    /// it waits for a pass after the lock is let go - unknown meanwhile, never a delete.
    Locked,
}

/// One regular file of `dir`, as its `lstat` `st` found it: `Ok(None)` when it vanished.
#[allow(clippy::too_many_arguments)]
fn scan_file(
    dir: &beneath::Dir,
    name: &str,
    key: &str,
    st: &Stat,
    base: &BTreeMap<String, BaseEntry>,
    base_scanned_ns: u64,
    json_merge: &[String],
    max_file_bytes: u64,
) -> Result<Option<(LocalFile, bool)>, Skip> {
    if st.size > max_file_bytes {
        return Err(Skip::Refused(
            Code::TooLarge,
            format!(
                "{} bytes, over the {max_file_bytes} a sync takes (--max-file-mb)",
                st.size
            ),
        ));
    }
    let file_of = |hash: String, blob_size: u64, content: Content, at: &Stat| LocalFile {
        hash,
        size: at.size,
        mtime_ns: at.mtime_ns,
        blob_size,
        content,
        exec: at.executable(),
        ctime_ns: at.ctime_ns,
        ino: at.ino,
        dev: at.dev,
    };
    if json_merge.iter().any(|k| k == key) {
        let (bytes, at) = match read_checked(dir, name, st) {
            Ok(Some(read)) => read,
            Ok(None) => return Ok(None),
            Err(skip) => return Err(skip),
        };
        let blob = json_blob(&bytes).ok_or_else(|| {
            Skip::Refused(
                Code::Unreadable,
                String::from(
                    "not a JSON object, so its machine-local keys cannot be held back; not \
                     synced until it is one",
                ),
            )
        })?;
        let file = file_of(
            hash_bytes(&blob),
            blob.len() as u64,
            Content::JsonMerge,
            &at,
        );
        return Ok(Some((file, true)));
    }
    let unchanged = base.get(key).filter(|b| {
        b.size == st.size
            && b.mtime_ns == st.mtime_ns
            && (b.ctime_ns == 0 || b.ctime_ns == st.ctime_ns)
            && (b.ino == 0 || (b.ino == st.ino && b.dev == st.dev))
            && b.exec == st.executable()
            && st.mtime_ns.saturating_add(RACY_NS) < base_scanned_ns
    });
    if let Some(b) = unchanged {
        return Ok(Some((
            file_of(b.hash.clone(), st.size, Content::Raw, st),
            false,
        )));
    }
    match hash_checked(dir, name, st) {
        Ok(Some((hash, at))) => Ok(Some((file_of(hash, at.size, Content::Raw, &at), true))),
        Ok(None) => Ok(None),
        Err(skip) => Err(skip),
    }
}

/// Opens the file `name` of `dir` without following a link and checks it is the file `st`
/// describes; `Ok(None)` when it is gone.
fn open_checked(
    dir: &beneath::Dir,
    name: &str,
    st: &Stat,
) -> Result<Option<(fs::File, Stat)>, Skip> {
    match dir.open_read(name) {
        Ok((file, at)) if at.same_file(st) => {
            // An exclusive lock of another app's (flock, LockFileEx) means it is being written:
            // a shared lock of ours, held while it is read, says so without waiting. A disk
            // that keeps no locks answers an error, which says nothing about the file.
            if let Err(std::fs::TryLockError::WouldBlock) = file.try_lock_shared() {
                return Err(Skip::Locked);
            }
            Ok(Some((file, at)))
        }
        Ok(_) | Err(BeneathError::ThroughLink(_)) => Err(Skip::Changed),
        Err(e) if e.is_not_found() => Ok(None),
        Err(BeneathError::Io(io)) if held_open_elsewhere(&io) => Err(Skip::Locked),
        Err(e) => Err(Skip::Unreadable(format!("cannot be read ({e})"))),
    }
}

/// Whether opening a file failed because another app holds it open without sharing it
/// (Windows' sharing and lock violations); other systems keep no such opens.
fn held_open_elsewhere(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33))
}

/// After a read: the file's `fstat` must still be what it was when it was opened.
fn still_same(file: &fs::File, at: &Stat) -> Result<(), Skip> {
    match beneath::stat_of(file) {
        Ok(now)
            if now.size == at.size
                && now.mtime_ns == at.mtime_ns
                && now.ctime_ns == at.ctime_ns =>
        {
            Ok(())
        }
        _ => Err(Skip::Changed),
    }
}

/// The bytes of the file `name` of `dir` (the file `st` describes), read whole.
fn read_checked(
    dir: &beneath::Dir,
    name: &str,
    st: &Stat,
) -> Result<Option<(Vec<u8>, Stat)>, Skip> {
    let Some((mut file, at)) = open_checked(dir, name, st)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| Skip::Unreadable(format!("cannot be read ({e})")))?;
    still_same(&file, &at)?;
    Ok(Some((bytes, at)))
}

/// The BLAKE3 of the file `name` of `dir` (the file `st` describes), read in pieces.
fn hash_checked(dir: &beneath::Dir, name: &str, st: &Stat) -> Result<Option<(String, Stat)>, Skip> {
    let Some((mut file, at)) = open_checked(dir, name, st)? else {
        return Ok(None);
    };
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| Skip::Unreadable(format!("cannot be read ({e})")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    still_same(&file, &at)?;
    Ok(Some((hasher.finalize().to_hex().to_string(), at)))
}

/// The `lstat` of the file of `key` under `root` now, reached without a link; `Ok(None)` when
/// nothing is there.
///
/// # Errors
///
/// When a folder of its path is a link or another disk, or cannot be read.
pub fn stat_now(root: &Path, key: &str) -> Result<Option<Stat>, BeneathError> {
    let tree = beneath::Root::open(root)?;
    let (dir, name) = match tree.parent_of(key, false) {
        Ok(found) => found,
        Err(e) if e.is_not_found() => return Ok(None),
        Err(e) => return Err(e),
    };
    match dir.lstat(name) {
        Ok(st) => Ok(Some(st)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Whether the file of `key` is still as the scan found it (`None`: still absent) - checked
/// right before a download replaces or a delete removes it, so an edit made during the sync is
/// never lost; it waits for the next run. A folder of its path that became a link or another
/// disk is never as scanned.
#[must_use]
pub fn still_as_scanned(root: &Path, key: &str, scanned: Option<&LocalFile>) -> bool {
    match (stat_now(root, key), scanned) {
        (Ok(None), None) => true,
        (Ok(Some(st)), Some(file)) => file.matches(&st),
        _ => false,
    }
}

/// The name a file here answers to when `key`'s own name is asked for but is not in its folder:
/// a disk that folds case or accents (macOS, Windows) finds `Notes.txt` for `notes.txt` and
/// `Café` written one way for `Café` written the other (NFC / NFD). Such a disk cannot hold both
/// files of the drive; `None` when nothing answers or the name itself is there.
#[must_use]
pub fn other_spelling(root: &Path, key: &str) -> Option<String> {
    let path = path_of(root, key);
    let found = fs::symlink_metadata(&path).ok()?;
    let wanted = path.file_name()?;
    let folder = path.parent()?;
    let mut other = None;
    for entry in fs::read_dir(folder).ok()?.flatten() {
        let name = entry.file_name();
        if name.as_os_str() == wanted {
            return None;
        }
        if other.is_none() && same_file(&found, &entry) {
            other = Some(name.to_string_lossy().into_owned());
        }
    }
    // Something answers to the name and no entry has it: another spelling (unnamed when this
    // platform cannot tell which entry it is).
    Some(other.unwrap_or_else(|| String::from("another spelling of the name")))
}

/// Whether `entry` is the file `found` describes.
#[cfg(unix)]
fn same_file(found: &fs::Metadata, entry: &fs::DirEntry) -> bool {
    use std::os::unix::fs::MetadataExt;
    entry
        .metadata()
        .is_ok_and(|m| m.ino() == found.ino() && m.dev() == found.dev())
}

/// Whether `entry` is the file `found` describes (no inode numbers here: by size and time).
#[cfg(not(unix))]
fn same_file(found: &fs::Metadata, entry: &fs::DirEntry) -> bool {
    entry
        .metadata()
        .is_ok_and(|m| m.len() == found.len() && m.modified().ok() == found.modified().ok())
}

/// Removes the folders of `key` under `root` that a delete left empty, up to (not including)
/// `root`; never through a link.
pub fn prune_empty_parents(root: &Path, key: &str) {
    let Ok(tree) = beneath::Root::open(root) else {
        return;
    };
    let mut folder = beneath::split_key(key).0;
    while !folder.is_empty() {
        let Ok((dir, name)) = tree.parent_of(folder, false) else {
            break;
        };
        // rmdir only removes an empty folder; anything in it stops the walk.
        if dir.rmdir(name).is_err() {
            break;
        }
        folder = beneath::split_key(folder).0;
    }
}

/// The base entry of a file as it is on disk now (after a download wrote it): `hash` and the
/// file's size, times and inode.
#[must_use]
pub fn entry_now(root: &Path, key: &str, hash: &str) -> Option<BaseEntry> {
    let st = stat_now(root, key).ok()??;
    (st.kind == Kind::File).then(|| BaseEntry {
        hash: hash.to_string(),
        size: st.size,
        mtime_ns: st.mtime_ns,
        cloud_only: false,
        exec: st.executable(),
        ctime_ns: st.ctime_ns,
        ino: st.ino,
        dev: st.dev,
    })
}
