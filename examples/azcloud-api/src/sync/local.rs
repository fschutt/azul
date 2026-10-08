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
//! `endpoints`), keys sorted, pretty-printed: its hash is that blob's, so a
//! change of this computer's endpoints is no change for the sync, and a
//! downloaded copy gets this computer's own keys back. One that does not
//! parse as an object is skipped (its keys cannot be held back).

use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{bail, Context, Result};
use azul_appkit::azlin_config::MACHINE_LOCAL_KEYS;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::rules::Rules;
use crate::state::{read_json, write_json};

/// The `format` of a local index.
pub const LOCAL_FORMAT: &str = "azcloud.sync-local";
/// A file whose time lies this close to the scan that recorded it is read
/// again by the next scan.
pub const RACY_NS: u64 = 2_000_000_000;

/// What both sides agreed on for one file at the last sync.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseEntry {
    /// The BLAKE3 of the blob.
    pub hash: String,
    /// The file's bytes on this device.
    pub size: u64,
    /// The file's modification time on this device (nanoseconds).
    pub mtime_ns: u64,
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
}

impl LocalIndex {
    /// The index of a folder never synced.
    #[must_use]
    pub fn new(root: &Path, bucket: &str, prefix: &str) -> LocalIndex {
        LocalIndex {
            format: LOCAL_FORMAT.to_string(),
            version: 1,
            root: root.display().to_string(),
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
            generation: 0,
            scanned_at_ns: 0,
            remote_etag: None,
            files: BTreeMap::new(),
        }
    }

    /// Reads the index at `path`; `None` when the folder was never synced.
    ///
    /// # Errors
    ///
    /// When it cannot be read or is no local index.
    pub fn load(path: &Path) -> Result<Option<LocalIndex>> {
        let Some(index) = read_json::<LocalIndex>(path)? else {
            return Ok(None);
        };
        if index.format != LOCAL_FORMAT {
            bail!("{} is no azcloud local index", path.display());
        }
        Ok(Some(index))
    }

    /// Writes it.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Content {
    /// Its bytes.
    Raw,
    /// Its JSON object without the machine-local keys.
    JsonMerge,
}

/// One file of the folder as the scan found it.
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Walks `root` (symbolic links are never followed) and finds every file the
/// `rules` let through, reusing the `base`'s hash of a file whose size and
/// time did not move since the scan at `base_scanned_ns`. A key of
/// `json_merge` is synced as JSON without its machine-local keys. A folder
/// that does not exist is empty.
///
/// # Errors
///
/// When the folder itself cannot be read; a file that cannot be read is
/// skipped and reported.
pub fn scan(
    root: &Path,
    rules: &Rules,
    base: &BTreeMap<String, BaseEntry>,
    base_scanned_ns: u64,
    json_merge: &[String],
    max_file_bytes: u64,
) -> Result<Scan> {
    let mut out = Scan {
        started_ns: crate::now_ns(),
        ..Scan::default()
    };
    if !root.exists() {
        return Ok(out);
    }
    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if prefix.is_empty() => {
                return Err(e).with_context(|| format!("the folder {}", dir.display()));
            }
            Err(e) => {
                out.skipped
                    .push((prefix.clone(), format!("cannot be read ({e})")));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    out.skipped
                        .push((prefix.clone(), format!("cannot be listed ({e})")));
                    continue;
                }
            };
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(raw) => {
                    out.skipped.push((
                        format!("{prefix}{}", raw.to_string_lossy()),
                        String::from("its name is not UTF-8"),
                    ));
                    continue;
                }
            };
            let key = format!("{prefix}{name}");
            // file_type() does not follow a symbolic link.
            let Ok(file_type) = entry.file_type() else {
                out.skipped
                    .push((key, String::from("vanished while scanned")));
                continue;
            };
            if file_type.is_symlink() {
                out.skipped
                    .push((key, String::from("a symbolic link (never followed)")));
                continue;
            }
            if file_type.is_dir() {
                if rules.excluded_dir(&key).is_some() {
                    out.excluded += 1;
                } else {
                    stack.push((entry.path(), format!("{key}/")));
                }
                continue;
            }
            if !file_type.is_file() {
                out.skipped.push((key, String::from("not a regular file")));
                continue;
            }
            if rules.excluded(&key).is_some() {
                out.excluded += 1;
                continue;
            }
            if let Err(e) = super::remote::check_key(&key) {
                out.skipped
                    .push((key, format!("its name cannot be a key ({e})")));
                continue;
            }
            match scan_file(
                &entry.path(),
                &key,
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
                Err(why) => out.skipped.push((key, why)),
            }
        }
    }
    Ok(out)
}

/// One file: `Ok(Some((file, hashed)))`, `Ok(None)` when it vanished, `Err`
/// with the reason it is skipped.
fn scan_file(
    path: &Path,
    key: &str,
    base: &BTreeMap<String, BaseEntry>,
    base_scanned_ns: u64,
    json_merge: &[String],
    max_file_bytes: u64,
) -> Result<Option<(LocalFile, bool)>, String> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot be read ({e})")),
    };
    let size = meta.len();
    if size > max_file_bytes {
        return Err(format!(
            "{size} bytes, over the {max_file_bytes} a sync takes (--max-file-mb)"
        ));
    }
    let mtime_ns = mtime_ns(&meta);
    if json_merge.iter().any(|k| k == key) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("cannot be read ({e})")),
        };
        let blob = json_blob(&bytes).ok_or_else(|| {
            String::from(
                "not a JSON object, so its machine-local keys cannot be held back; not synced \
                 until it is one",
            )
        })?;
        let file = LocalFile {
            hash: hash_bytes(&blob),
            size,
            mtime_ns,
            blob_size: blob.len() as u64,
            content: Content::JsonMerge,
        };
        return Ok(Some((file, true)));
    }
    let unchanged = base.get(key).filter(|b| {
        b.size == size
            && b.mtime_ns == mtime_ns
            && mtime_ns.saturating_add(RACY_NS) < base_scanned_ns
    });
    if let Some(b) = unchanged {
        let file = LocalFile {
            hash: b.hash.clone(),
            size,
            mtime_ns,
            blob_size: size,
            content: Content::Raw,
        };
        return Ok(Some((file, false)));
    }
    match hash_file(path) {
        Ok(hash) => Ok(Some((
            LocalFile {
                hash,
                size,
                mtime_ns,
                blob_size: size,
                content: Content::Raw,
            },
            true,
        ))),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot be read ({e})")),
    }
}

/// Whether the file of `key` is still as the scan found it (`None`: still
/// absent) - checked right before a download overwrites or a delete removes
/// it, so an edit made during the sync is never lost; it waits for the next
/// run.
#[must_use]
pub fn still_as_scanned(root: &Path, key: &str, scanned: Option<&LocalFile>) -> bool {
    match (fs::symlink_metadata(path_of(root, key)), scanned) {
        (Err(e), None) => e.kind() == ErrorKind::NotFound,
        (Ok(meta), Some(file)) => {
            meta.is_file() && meta.len() == file.size && mtime_ns(&meta) == file.mtime_ns
        }
        _ => false,
    }
}

/// Removes the folders of `key` under `root` that a delete left empty, up to
/// (not including) `root`.
pub fn prune_empty_parents(root: &Path, key: &str) {
    let mut path = path_of(root, key);
    while path.pop() {
        if path == root || !path.starts_with(root) {
            break;
        }
        // remove_dir only removes an empty folder; anything in it stops the walk.
        if fs::remove_dir(&path).is_err() {
            break;
        }
    }
}

/// The base entry of a file as it is on disk now (after a download wrote
/// it): `hash` and the file's size and time.
#[must_use]
pub fn entry_now(root: &Path, key: &str, hash: &str) -> Option<BaseEntry> {
    let meta = fs::metadata(path_of(root, key)).ok()?;
    Some(BaseEntry {
        hash: hash.to_string(),
        size: meta.len(),
        mtime_ns: mtime_ns(&meta),
    })
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    fn write(root: &Path, key: &str, bytes: &[u8]) {
        let path = path_of(root, key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_scan_finds_the_files_the_rules_let_through_and_never_follows_a_link() {
        let dir = TempDir::new("azcloud-scan");
        let root = dir.path();
        write(root, "notes/a.md", b"a");
        write(root, "notes/sub/b.md", b"bb");
        write(root, ".azlin/cache", b"manifest");
        write(root, "notes/.a.md.azul-storage-1-0.tmp", b"half");
        write(root, "music/cache/c.jpg", b"jpg");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", root.join("notes/etc")).unwrap();
        let scan = scan(
            root,
            &Rules::azlin_data(),
            &BTreeMap::new(),
            0,
            &[],
            u64::MAX,
        )
        .unwrap();
        let keys: Vec<&str> = scan.files.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["notes/a.md", "notes/sub/b.md"]);
        assert_eq!(scan.files["notes/sub/b.md"].hash, hash_bytes(b"bb"));
        assert_eq!(scan.hashed, 2);
        assert_eq!(
            scan.excluded, 3,
            ".azlin/, the temporary file, music/cache/"
        );
        #[cfg(unix)]
        assert!(scan
            .skipped
            .iter()
            .any(|(k, why)| k == "notes/etc" && why.contains("link")));
        let missing = super::scan(
            &root.join("nope"),
            &Rules::base(),
            &BTreeMap::new(),
            0,
            &[],
            1,
        )
        .unwrap();
        assert!(missing.files.is_empty(), "a folder not there yet is empty");
    }

    #[test]
    fn an_unchanged_file_keeps_its_base_hash_unless_its_time_is_racy() {
        let dir = TempDir::new("azcloud-racy");
        let root = dir.path();
        write(root, "a.txt", b"one");
        let meta = fs::metadata(root.join("a.txt")).unwrap();
        let entry = BaseEntry {
            hash: String::from("cached"),
            size: 3,
            mtime_ns: mtime_ns(&meta),
        };
        let base: BTreeMap<String, BaseEntry> = [(String::from("a.txt"), entry.clone())].into();
        let long_after = entry.mtime_ns + 10 * RACY_NS;
        let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
        assert_eq!(
            s.files["a.txt"].hash, "cached",
            "size and time say unchanged"
        );
        assert_eq!(s.hashed, 0);
        let s = scan(
            root,
            &Rules::none(),
            &base,
            entry.mtime_ns + 1,
            &[],
            u64::MAX,
        )
        .unwrap();
        assert_eq!(
            s.files["a.txt"].hash,
            hash_bytes(b"one"),
            "racy: read again"
        );
        let s = scan(root, &Rules::none(), &base, long_after, &[], 2).unwrap();
        assert!(s.files.is_empty());
        assert!(s.skipped[0].1.contains("--max-file-mb"));
    }

    #[test]
    fn the_shared_config_syncs_without_its_endpoints_and_gets_this_computers_back() {
        let ours =
            br#"{"mode": "dark", "endpoints": {"profile": "local"}, "currentTheme": "flat"}"#;
        let blob = json_blob(ours).unwrap();
        let text = String::from_utf8(blob.clone()).unwrap();
        assert!(!text.contains("endpoints"), "{text}");
        assert!(
            text.find("currentTheme") < text.find("mode"),
            "keys in order: {text}"
        );
        assert_eq!(
            json_blob(br#"{"currentTheme":"flat","mode":"dark","endpoints":{"token":"x"}}"#)
                .unwrap(),
            blob,
            "another computer's endpoints make no other blob"
        );
        // A blob from the bucket that carries endpoints never brings them here.
        let evil = br#"{"mode": "light", "endpoints": {"token": "https://evil.test"}}"#;
        let written = json_with_local_keys(evil, Some(&ours[..])).unwrap();
        let written: Value = serde_json::from_slice(&written).unwrap();
        assert_eq!(written["mode"], "light");
        assert_eq!(written["endpoints"]["profile"], "local");
        assert!(written["endpoints"]["token"].is_null());
        let fresh: Value =
            serde_json::from_slice(&json_with_local_keys(evil, None).unwrap()).unwrap();
        assert!(fresh.get("endpoints").is_none());
        assert_eq!(json_blob(b"not json"), None);
        assert_eq!(json_blob(b"[1, 2]"), None);
    }

    #[test]
    fn a_json_merge_file_that_is_no_object_is_skipped_not_uploaded_raw() {
        let dir = TempDir::new("azcloud-json-skip");
        write(dir.path(), "config.json", br#"{"endpoints": {"token": "#);
        let s = scan(
            dir.path(),
            &Rules::none(),
            &BTreeMap::new(),
            0,
            &[String::from("config.json")],
            u64::MAX,
        )
        .unwrap();
        assert!(s.files.is_empty());
        assert!(s.skipped[0].1.contains("JSON object"), "{:?}", s.skipped);
    }

    #[test]
    fn a_change_after_the_scan_is_noticed_before_a_download_overwrites_it() {
        let dir = TempDir::new("azcloud-still");
        let root = dir.path();
        write(root, "a.txt", b"one");
        let s = scan(root, &Rules::none(), &BTreeMap::new(), 0, &[], u64::MAX).unwrap();
        assert!(still_as_scanned(root, "a.txt", s.files.get("a.txt")));
        assert!(still_as_scanned(root, "b.txt", None));
        write(root, "a.txt", b"one, edited");
        assert!(!still_as_scanned(root, "a.txt", s.files.get("a.txt")));
        write(root, "b.txt", b"new");
        assert!(!still_as_scanned(root, "b.txt", None));
    }

    #[test]
    fn a_delete_prunes_the_folders_it_left_empty_and_nothing_else() {
        let dir = TempDir::new("azcloud-prune");
        let root = dir.path();
        write(root, "a/b/c/d.txt", b"x");
        write(root, "a/keep.txt", b"y");
        fs::remove_file(root.join("a/b/c/d.txt")).unwrap();
        prune_empty_parents(root, "a/b/c/d.txt");
        assert!(!root.join("a/b").exists());
        assert!(root.join("a/keep.txt").exists());
        assert!(root.exists());
    }

    #[test]
    fn the_local_index_lives_in_the_state_folder_one_file_per_folder_bucket_and_prefix() {
        let sync = Path::new("/state/sync");
        let a = index_path(sync, "d-1", "azlin/data/", Path::new("/data"));
        assert!(a.starts_with(sync));
        assert_ne!(
            a,
            index_path(sync, "d-1", "azlin/config/", Path::new("/data"))
        );
        assert_ne!(
            a,
            index_path(sync, "d-2", "azlin/data/", Path::new("/data"))
        );
        assert_eq!(
            a,
            index_path(sync, "d-1", "azlin/data/", Path::new("/data"))
        );
    }
}
