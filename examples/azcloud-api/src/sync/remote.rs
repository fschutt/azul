//! The drive side of a synced folder: under its prefix `P`,
//!
//! - `P.azlin/index.json`: the index - every file's path, BLAKE3, size,
//!   modification time, the generation that wrote it and the device; the
//!   tombstones of deleted files. It is the commit point: a device writes it
//!   with `If-Match: <the ETag it read>` (`If-None-Match: *` for the first),
//!   so of two devices exactly one wins and the other re-reads, merges and
//!   tries again (PLAN §12.3; Azlin's S3 makes conditional PUTs linearizable
//!   through Raft).
//! - `P.azlin/blobs/<first two hex>/<BLAKE3>`: the contents, one object per
//!   distinct content. A blob is uploaded before the index names it and
//!   never changes, so a device that loses the race wastes nothing, a reader
//!   never sees half a file, and two devices writing the same path never
//!   overwrite each other's bytes - the index alone says which one the path
//!   has.
//!
//! The index is plain, readable JSON (the bucket stays debuggable: follow a
//! path to its blob). PLAN §12.1 / §12.3's encrypted git repository and AZL1
//! objects replace both once the drive key exists; the CAS-and-merge loop
//! stays the same. `.azlin/` under the prefix is bookkeeping, as at a
//! drive's root.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

/// The `format` of the index.
pub const INDEX_FORMAT: &str = "azcloud.sync-index";
/// The index version this code reads and writes.
pub const INDEX_VERSION: u32 = 1;
/// The bookkeeping folder under a prefix.
pub const META_DIR: &str = ".azlin";

/// A prefix as the sync uses it: no leading slash, a trailing one unless it
/// is the bucket's root (empty).
///
/// # Errors
///
/// When it climbs out (`..`), holds a backslash or NUL, or names `.azlin`.
pub fn normalize_prefix(prefix: &str) -> Result<String> {
    let p = prefix.trim().trim_start_matches('/');
    if p.is_empty() {
        return Ok(String::new());
    }
    let p = if p.ends_with('/') {
        p.to_string()
    } else {
        format!("{p}/")
    };
    azul_storage::key::check_path_prefix(&p).map_err(|e| anyhow!("the prefix {prefix:?}: {e}"))?;
    if p.split('/').any(|segment| segment == META_DIR) {
        bail!("the prefix {prefix:?} names {META_DIR}, the bookkeeping folder");
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

/// One file of the index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    /// The BLAKE3 of the blob.
    pub hash: String,
    /// The blob's bytes.
    pub size: u64,
    /// The file's modification time where it was uploaded (seconds).
    pub mtime: i64,
    /// The generation of the index that wrote it.
    pub gen: u64,
    /// The device that wrote it.
    pub device: String,
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
}

/// The index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIndex {
    pub format: String,
    pub version: u32,
    /// Counts the commits: one more with every write.
    pub generation: u64,
    pub updated_at: i64,
    pub updated_by: String,
    #[serde(default)]
    pub files: BTreeMap<String, RemoteFile>,
    #[serde(default)]
    pub deleted: BTreeMap<String, Tombstone>,
}

impl RemoteIndex {
    /// The index before the first commit.
    #[must_use]
    pub fn empty() -> RemoteIndex {
        RemoteIndex {
            format: INDEX_FORMAT.to_string(),
            version: INDEX_VERSION,
            generation: 0,
            updated_at: 0,
            updated_by: String::new(),
            files: BTreeMap::new(),
            deleted: BTreeMap::new(),
        }
    }

    /// Reads an index. Every path must be a key a folder can hold (no `..`,
    /// not absolute, no `.azlin/` at the root) and every hash a BLAKE3: an
    /// index that breaks the rules is refused whole, never acted on in part.
    ///
    /// # Errors
    ///
    /// When the bytes are no index of this version, or break a rule.
    pub fn parse(bytes: &[u8]) -> Result<RemoteIndex> {
        let index: RemoteIndex = serde_json::from_slice(bytes)
            .map_err(|e| anyhow!("the drive's sync index cannot be read: {e}"))?;
        if index.format != INDEX_FORMAT {
            bail!(
                "the drive's sync index is {:?}, not {INDEX_FORMAT:?}",
                index.format
            );
        }
        if index.version == 0 || index.version > INDEX_VERSION {
            bail!(
                "the drive's sync index is version {}; this azcloud reads up to {INDEX_VERSION} - \
                 update it",
                index.version
            );
        }
        let paths = index
            .files
            .iter()
            .map(|(k, f)| (k, &f.hash))
            .chain(index.deleted.iter().map(|(k, t)| (k, &t.hash)));
        for (key, hash) in paths {
            check_key(key)?;
            if !is_hash(hash) {
                bail!("the drive's sync index names {key:?} with a damaged hash");
            }
        }
        Ok(index)
    }

    /// The index as stored: pretty JSON (keys in order) and a final newline.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
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
pub fn check_key(key: &str) -> Result<()> {
    azul_storage::key::check_path_key(key).map_err(|e| anyhow!("{e}"))?;
    if azul_storage::manifest::is_reserved_key(key) {
        bail!("{key:?} lies in the bookkeeping folder {META_DIR}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(c: char) -> String {
        std::iter::repeat(c).take(64).collect()
    }

    #[test]
    fn a_prefix_has_one_trailing_slash_and_never_climbs_out() {
        assert_eq!(normalize_prefix("").unwrap(), "");
        assert_eq!(normalize_prefix("/").unwrap(), "");
        assert_eq!(normalize_prefix("azlin/data").unwrap(), "azlin/data/");
        assert_eq!(normalize_prefix("/azlin/data/").unwrap(), "azlin/data/");
        for bad in ["../x", "a/../b", "a\\b", "x/.azlin/", ".azlin"] {
            assert!(normalize_prefix(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_index_and_the_blobs_live_in_the_prefixes_bookkeeping_folder() {
        assert_eq!(index_key("azlin/data/"), "azlin/data/.azlin/index.json");
        assert_eq!(index_key(""), ".azlin/index.json");
        let hash = h('a');
        assert_eq!(blob_key("p/", &hash), format!("p/.azlin/blobs/aa/{hash}"));
        assert!(is_hash(&hash));
        assert!(!is_hash(&h('A')));
        assert!(!is_hash("abc"));
    }

    #[test]
    fn an_index_round_trips_and_one_that_breaks_a_rule_is_refused_whole() {
        let mut index = RemoteIndex::empty();
        index.generation = 3;
        index.files.insert(
            String::from("notes/a.md"),
            RemoteFile {
                hash: h('1'),
                size: 5,
                mtime: 1,
                gen: 3,
                device: String::from("dev-a"),
            },
        );
        index.deleted.insert(
            String::from("notes/old.md"),
            Tombstone {
                hash: h('2'),
                gen: 2,
                at: 9,
                device: String::from("dev-b"),
            },
        );
        let back = RemoteIndex::parse(&index.to_bytes()).unwrap();
        assert_eq!(back, index);
        assert!(back.has_blob(&h('1')));
        assert!(back.has_blob(&h('2')), "a tombstone keeps its blob");
        assert!(!back.has_blob(&h('3')));

        for bad in [
            "../../etc/passwd",
            "/etc/passwd",
            ".azlin/index.json",
            "a\\b",
        ] {
            let mut evil = index.clone();
            evil.files
                .insert(bad.to_string(), evil.files["notes/a.md"].clone());
            assert!(RemoteIndex::parse(&evil.to_bytes()).is_err(), "{bad:?}");
        }
        let mut damaged = index.clone();
        damaged.files.get_mut("notes/a.md").unwrap().hash = String::from("zz");
        assert!(RemoteIndex::parse(&damaged.to_bytes()).is_err());
        let mut newer = index;
        newer.version = INDEX_VERSION + 1;
        let err = RemoteIndex::parse(&newer.to_bytes())
            .unwrap_err()
            .to_string();
        assert!(err.contains("update"), "{err}");
    }
}
