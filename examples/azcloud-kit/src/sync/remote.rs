//! The drive side of a synced folder: under its prefix `P`,
//!
//! - `P.azlin/index.json`: the index - every file's path, BLAKE3, size,
//!   modification time, the generation that wrote it and the device; every
//!   folder with its identity (empty ones too); the tombstones of deleted
//!   files and folders. It is the commit point: a device writes it
//!   with `If-Match: <the ETag it read>` (`If-None-Match: *` for the first),
//!   so of two devices exactly one wins and the other re-reads, merges and
//!   tries again (the drive's S3 service makes conditional PUTs
//!   linearizable).
//! - `P.azlin/blobs/<first two hex>/<BLAKE3>`: the contents, one object per
//!   distinct content. A blob is uploaded before the index names it and
//!   never changes, so a device that loses the race wastes nothing, a reader
//!   never sees half a file, and two devices writing the same path never
//!   overwrite each other's bytes - the index alone says which one the path
//!   has.
//!
//! The index is plain, readable JSON (the bucket stays debuggable: follow a
//! path to its blob). An encrypted form may replace both once the drive has
//! a key; the CAS-and-merge loop stays the same. `.azlin/` under the prefix
//! is bookkeeping, as at a drive's root.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::{fail, CloudError, CloudResult};

/// The `format` of the index.
pub const INDEX_FORMAT: &str = "azcloud.sync-index";
/// The index version this code reads and writes.
pub const INDEX_VERSION: u32 = 1;
/// The format features this code reads (D43): a newer writer names the features its index
/// uses, and a device that meets one it does not know - or a newer version - leaves the drive
/// as it is (read-only there) and asks to be updated.
pub const KNOWN_FEATURES: &[&str] = &[FOLDERS_FEATURE, LINKS_FEATURE];
/// The feature of an index with folder entries ([`RemoteIndex::folders`]): a version that does
/// not know them would drop them with every commit, so it leaves such a drive as it is.
pub const FOLDERS_FEATURE: &str = "folders";
/// The feature of an index with link entries ([`RemoteFile::link`]): a version that does not
/// know them would write each as a file holding its target.
pub const LINKS_FEATURE: &str = "links";
/// The bookkeeping folder under a prefix.
pub const META_DIR: &str = ".azlin";

/// A prefix as the sync uses it: no leading slash, a trailing one unless it
/// is the bucket's root (empty).
///
/// # Errors
///
/// When it climbs out (`..`), holds a backslash or NUL, or names `.azlin`.
pub fn normalize_prefix(prefix: &str) -> CloudResult<String> {
    let p = prefix.trim().trim_start_matches('/');
    if p.is_empty() {
        return Ok(String::new());
    }
    let p = if p.ends_with('/') {
        p.to_string()
    } else {
        format!("{p}/")
    };
    azul_storage::key::check_prefix(&p)
        .map_err(|e| CloudError::failed(format!("the prefix {prefix:?}: {e}")))?;
    if p.split('/').any(|segment| segment == META_DIR) {
        fail!("the prefix {prefix:?} names {META_DIR}, the bookkeeping folder");
    }
    Ok(p)
}

/// The index of the folder synced to `prefix`.
#[must_use]
pub fn index_key(prefix: &str) -> String {
    format!("{prefix}{META_DIR}/index.json")
}

/// Where the blobs of `prefix` are.
#[must_use]
pub fn blobs_prefix(prefix: &str) -> String {
    format!("{prefix}{META_DIR}/blobs/")
}

/// The blob of the content with BLAKE3 `hash` (64 hex) under `prefix`.
#[must_use]
pub fn blob_key(prefix: &str, hash: &str) -> String {
    let fan = hash.get(..2).unwrap_or("00");
    format!("{}{fan}/{hash}", blobs_prefix(prefix))
}

/// Whether `s` is a BLAKE3 in lowercase hex.
#[must_use]
pub fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// What a drive synced by name puts in an entry whose content it does not know yet: the
/// version the drive gave the file (its entity tag), until a device has read or written it.
#[must_use]
pub fn version_stand_in(etag: &str) -> String {
    format!("{VERSION_STAND_IN}{etag}")
}

/// The prefix of [`version_stand_in`].
const VERSION_STAND_IN: &str = "version:";

/// Whether `s` is a [`version_stand_in`].
#[must_use]
pub fn is_version_stand_in(s: &str) -> bool {
    s.strip_prefix(VERSION_STAND_IN).is_some()
}

/// One file of the index.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    /// The BLAKE3 of the blob.
    pub hash: String,
    /// The blob's bytes.
    pub size: u64,
    /// The file's modification time where it was uploaded (seconds).
    pub mtime: i64,
    /// The same time to the nanosecond (0: an older writer's entry, `mtime` says it): a
    /// download sets it, so a photo's time comes back as it was taken.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mtime_ns: u64,
    /// The generation of the index that wrote it.
    pub gen: u64,
    /// The device that wrote it.
    pub device: String,
    /// The user's executable bit, part of the file's content: a download sets it, never any
    /// other permission bit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exec: bool,
    /// A link, not a file: its target (relative, inside the folder), which is its content -
    /// `hash` and `size` are the target's ([`LINKS_FEATURE`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// A deleted file: kept for [`super::SyncOptions::tombstone_days`], so a
/// device that still holds the old copy and never synced it deletes it
/// instead of bringing it back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    /// The BLAKE3 of the content that was deleted.
    pub hash: String,
    pub gen: u64,
    /// When (seconds).
    pub at: i64,
    pub device: String,
    /// The user confirmed this delete on the device that made it (a mass delete they were
    /// asked about): other devices move the file into their trash without asking again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirmed: bool,
}

/// A folder of the synced tree, empty or not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFolder {
    /// Its identity: made with the folder, kept when the folder is renamed (32 hex digits).
    pub id: String,
    /// The generation of the index that wrote it.
    pub gen: u64,
    /// The device that wrote it.
    pub device: String,
}

/// A deleted folder. Only a recorded delete removes a folder elsewhere: a folder an older
/// writer left out of the index is no delete, and comes back from a device that has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderTombstone {
    /// The identity the folder had.
    pub id: String,
    pub gen: u64,
    /// When (seconds).
    pub at: i64,
    pub device: String,
}

/// The identity of a folder no device has named yet (a drive synced by name lists a folder
/// another tool made): the same on every device, made from its key, so a later rename of it by
/// a device carries this identity.
#[must_use]
pub fn derived_folder_id(key: &str) -> String {
    let hash = blake3::hash(format!("azcloud folder\n{key}").as_bytes());
    hash.to_hex().as_str()[..32].to_string()
}

/// Whether `id` is a folder identity as [`RemoteFolder::id`] names one: 32 lowercase hex digits.
#[must_use]
pub fn is_folder_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIndex {
    pub format: String,
    pub version: u32,
    /// The format features its writer used beyond the version ([`KNOWN_FEATURES`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Counts the commits: one more with every write.
    pub generation: u64,
    pub updated_at: i64,
    pub updated_by: String,
    #[serde(default)]
    pub files: BTreeMap<String, RemoteFile>,
    #[serde(default)]
    pub deleted: BTreeMap<String, Tombstone>,
    /// Every folder by its key (no trailing slash), empty ones too ([`FOLDERS_FEATURE`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folders: BTreeMap<String, RemoteFolder>,
    /// The deleted folders.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deleted_folders: BTreeMap<String, FolderTombstone>,
    /// Entries whose fields cannot be read at all (a size of -1, no object): kept exactly as
    /// they were written - a commit writes them back - and named by
    /// [`RemoteIndex::unreadable`], with why.
    #[serde(skip)]
    pub damaged: BTreeMap<String, DamagedEntry>,
}

/// An entry of the index whose fields cannot be read, kept as the drive holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamagedEntry {
    pub raw: serde_json::Value,
    /// Why it cannot be read.
    pub why: String,
    /// A record of a delete (the index's `deleted`), not a file.
    pub tombstone: bool,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl RemoteFile {
    /// Its modification time in nanoseconds since 1970 (0: none known).
    #[must_use]
    pub fn modified_ns(&self) -> u64 {
        if self.mtime_ns > 0 {
            self.mtime_ns
        } else {
            u64::try_from(self.mtime)
                .unwrap_or(0)
                .saturating_mul(1_000_000_000)
        }
    }
}

impl RemoteIndex {
    /// The index before the first commit.
    #[must_use]
    pub fn empty() -> RemoteIndex {
        RemoteIndex {
            format: INDEX_FORMAT.to_string(),
            version: INDEX_VERSION,
            features: Vec::new(),
            generation: 0,
            updated_at: 0,
            updated_by: String::new(),
            files: BTreeMap::new(),
            deleted: BTreeMap::new(),
            folders: BTreeMap::new(),
            deleted_folders: BTreeMap::new(),
            damaged: BTreeMap::new(),
        }
    }

    /// Reads an index. An entry whose path no folder can hold (`..`, absolute, `.azlin/` at the
    /// root, a control character) or whose hash is no BLAKE3 is kept as it is - another device
    /// or a share participant wrote it - and named by [`RemoteIndex::unreadable`]: a run never
    /// acts on it and never takes it for deleted. A newer version, features and fields this
    /// code does not know are read ([`RemoteIndex::unknown_features`] names them; D43).
    ///
    /// # Errors
    ///
    /// When the bytes are no index.
    pub fn parse(bytes: &[u8]) -> CloudResult<RemoteIndex> {
        let unreadable =
            |e: serde_json::Error| CloudError::failed(format!("the drive's sync index cannot be read: {e}"));
        let mut whole: serde_json::Value = serde_json::from_slice(bytes).map_err(unreadable)?;
        // An entry is read on its own: one whose fields cannot be read is kept aside as it
        // is, never the reason the whole index is refused (M1).
        let mut damaged = BTreeMap::new();
        if let Some(serde_json::Value::Object(files)) = whole.get_mut("files") {
            for (key, entry) in std::mem::take(files) {
                match serde_json::from_value::<RemoteFile>(entry.clone()) {
                    Ok(_) => {
                        files.insert(key, entry);
                    }
                    Err(e) => {
                        let why = format!("the drive's entry for it cannot be read ({e})");
                        damaged.insert(key, DamagedEntry { raw: entry, why, tombstone: false });
                    }
                }
            }
        }
        if let Some(serde_json::Value::Object(deleted)) = whole.get_mut("deleted") {
            deleted.retain(|key, tomb| {
                let readable = serde_json::from_value::<Tombstone>(tomb.clone()).is_ok();
                if !readable {
                    let why = String::from("the drive's record of its delete cannot be read");
                    damaged.insert(key.clone(), DamagedEntry { raw: tomb.clone(), why, tombstone: true });
                }
                readable
            });
        }
        // A folder entry that cannot be read is left out: a folder holds no bytes, and only a
        // readable record of its delete removes it anywhere.
        if let Some(fields) = whole.as_object_mut() {
            for list in ["folders", "deleted_folders"] {
                if fields.get(list).is_some_and(|entries| !entries.is_object()) {
                    fields.remove(list);
                }
                if let Some(serde_json::Value::Object(entries)) = fields.get_mut(list) {
                    entries.retain(|_, entry| {
                        if list == "folders" {
                            serde_json::from_value::<RemoteFolder>(entry.clone()).is_ok()
                        } else {
                            serde_json::from_value::<FolderTombstone>(entry.clone()).is_ok()
                        }
                    });
                }
            }
        }
        let mut index: RemoteIndex = serde_json::from_value(whole).map_err(unreadable)?;
        index.damaged = damaged;
        if index.format != INDEX_FORMAT {
            fail!(
                "the drive's sync index is {:?}, not {INDEX_FORMAT:?}",
                index.format
            );
        }
        if index.version == 0 {
            fail!("the drive's sync index has no version");
        }
        Ok(index)
    }

    /// The entries no run acts on, and why: a path no folder can hold, a hash that is none.
    /// They stay in the index as they are (a run never removes them), so nothing is lost and
    /// nothing is deleted on their account. With `stand_ins` (a drive synced by name) an entry
    /// may name its version instead of its content ([`version_stand_in`]).
    #[must_use]
    pub fn unreadable(&self, stand_ins: bool) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let entries = self
            .files
            .iter()
            .map(|(k, f)| (k, &f.hash))
            .chain(self.deleted.iter().map(|(k, t)| (k, &t.hash)));
        for (key, entry) in &self.damaged {
            out.insert(key.clone(), entry.why.clone());
        }
        for (key, hash) in entries {
            if let Err(e) = check_key(key) {
                out.insert(key.clone(), e.to_string());
            } else if let Some(why) = super::names::key_problem(key) {
                out.insert(key.clone(), why);
            } else if !is_hash(hash) && !(stand_ins && is_version_stand_in(hash)) {
                out.insert(
                    key.clone(),
                    String::from("the drive's entry names no content (a damaged hash)"),
                );
            }
        }
        // A link's content is its target: an entry whose hash is another's is damaged.
        for (key, file) in &self.files {
            if let Some(target) = &file.link {
                if blake3::hash(target.as_bytes()).to_hex().as_str() != file.hash {
                    out.insert(
                        key.clone(),
                        String::from("the drive's link entry does not match its target"),
                    );
                }
            }
        }
        let folders = self
            .folders
            .iter()
            .map(|(k, f)| (k, &f.id))
            .chain(self.deleted_folders.iter().map(|(k, t)| (k, &t.id)));
        for (key, id) in folders {
            if let Err(e) = check_key(key) {
                out.insert(key.clone(), e.to_string());
            } else if let Some(why) = super::names::key_problem(key) {
                out.insert(key.clone(), why);
            } else if !is_folder_id(id) {
                out.insert(
                    key.clone(),
                    String::from("the drive's folder entry names no identity (a damaged id)"),
                );
            }
        }
        out
    }

    /// The index without the entries [`RemoteIndex::unreadable`] names: what a run plans with.
    #[must_use]
    pub fn readable(&self, unreadable: &BTreeMap<String, String>) -> RemoteIndex {
        let mut out = self.clone();
        out.files.retain(|k, _| !unreadable.contains_key(k));
        out.deleted.retain(|k, _| !unreadable.contains_key(k));
        out.folders.retain(|k, _| !unreadable.contains_key(k));
        out.deleted_folders.retain(|k, _| !unreadable.contains_key(k));
        out
    }

    /// What this code does not know of the index's format: a newer version (`version <n>`)
    /// and the features it does not know; empty when it reads the index whole.
    #[must_use]
    pub fn unknown_features(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.version > INDEX_VERSION {
            out.push(format!("version {}", self.version));
        }
        out.extend(
            self.features
                .iter()
                .filter(|feature| !KNOWN_FEATURES.contains(&feature.as_str()))
                .cloned(),
        );
        out
    }

    /// The index as stored: pretty JSON (keys in order) and a final newline.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut whole = serde_json::to_value(self).unwrap_or_default();
        // The damaged entries go back as they were read: a run never removes them.
        for (key, entry) in &self.damaged {
            let list = if entry.tombstone { "deleted" } else { "files" };
            if let Some(serde_json::Value::Object(map)) = whole.get_mut(list) {
                map.entry(key.clone()).or_insert_with(|| entry.raw.clone());
            }
        }
        let mut text = serde_json::to_string_pretty(&whole).unwrap_or_default();
        text.push('\n');
        text.into_bytes()
    }

    /// Whether the bucket holds the blob `hash` (a file or a tombstone of
    /// the index names it; the garbage collection keeps those).
    #[must_use]
    pub fn has_blob(&self, hash: &str) -> bool {
        self.files.values().any(|f| f.hash == hash) || self.deleted.values().any(|t| t.hash == hash)
    }

    /// Every hash the index names.
    #[must_use]
    pub fn referenced(&self) -> BTreeSet<&str> {
        self.files
            .values()
            .map(|f| f.hash.as_str())
            .chain(self.deleted.values().map(|t| t.hash.as_str()))
            .collect()
    }
}

/// Whether `key` may name a file of a synced folder.
///
/// # Errors
///
/// When it climbs out, is absolute, holds a backslash or NUL, or lies in the
/// bookkeeping folder at the root.
pub fn check_key(key: &str) -> CloudResult<()> {
    azul_storage::key::check_key(key)?;
    if azul_storage::manifest::is_reserved_key(key) {
        fail!("{key:?} lies in the bookkeeping folder {META_DIR}");
    }
    Ok(())
}
