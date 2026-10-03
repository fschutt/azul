//! The data tree's manifest: `<data root>/.azlin/cache` records what is in
//! the tree, so the later S3 / database sync only diffs (user ruling
//! 2026-10-02: no sync yet, every durable write goes into the data tree).
//!
//! The [`LocalDrive`](crate::LocalDrive) of the data root keeps it on every
//! `put` / `delete` / `rename` / `copy` / folder operation: key -> size,
//! modification time, content hash. The apps call nothing for it. `.azlin/`
//! at a drive's root is the drive's own bookkeeping, never content: it is not
//! listed, and no key may name anything in it ([`is_reserved_key`]).
//!
//! # The file format
//!
//! UTF-8 text, one record per line, `\n`-terminated, fields separated by one
//! space. Cheap to APPEND (a drive call adds one line in one write) and to
//! REWRITE atomically (a snapshot written next to the file and renamed over
//! it). The lines are applied in order, the later one wins:
//!
//! ```text
//! azlin-cache 1 0000000000000116          the header: version 1, the size of the
//!                                         last snapshot (16 digits; when to compact)
//! + 5 1759480000 2cf24d...9824 notes/a.md the object is there: size in bytes,
//!                                         modified (seconds since 1970 UTC, `-` =
//!                                         unknown), lowercase hex SHA-256, key
//! - notes/a.md                            the object is gone
//! - notes/old/                            a key ending in `/`: the folder is gone,
//!                                         with everything under it
//! > notes/a.md notes/b.md                 renamed (a key or, ending in `/`, a folder)
//! ```
//!
//! Keys are written percent-encoded as SigV4 encodes a path
//! ([`crate::sigv4::uri_encode`] with `/` kept), so a key never holds a space
//! or a line break in the file. A line without its `\n` (a crash in the
//! middle of an append) and a record this version does not know are
//! skipped. The hash is the one SigV4 already computes for an upload
//! (`x-amz-content-sha256`), so the sync compares without reading files.
//!
//! The manifest is a CACHE of the tree, never the truth: [`diff`] compares it
//! with what a drive really holds, so a record lost to a crash or to two
//! processes compacting at once costs the sync some work, never a change.
//! When the file has grown past twice its last snapshot (and a minimum), the
//! drive rewrites it as a snapshot: one `+` line per object.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};

use crate::{
    ops::list_all,
    sigv4::{sha256_hex, sha256_hex_of, uri_decode, uri_encode},
    Drive, DriveError,
};

/// The folder of the drive's own bookkeeping, at its root.
pub const MANIFEST_DIR: &str = ".azlin";

/// The manifest's file in [`MANIFEST_DIR`].
pub const CACHE_FILE: &str = "cache";

/// The first word of the header line.
const MAGIC: &str = "azlin-cache";

/// The format version this code writes and reads.
const VERSION: &str = "1";

/// The digits of the snapshot size in the header (fixed, so the header's
/// length does not depend on it).
const SNAPSHOT_DIGITS: usize = 16;

/// Below this size the log is never compacted.
#[cfg(not(test))]
pub(crate) const COMPACT_MIN_BYTES: u64 = 256 * 1024;
#[cfg(test)]
pub(crate) const COMPACT_MIN_BYTES: u64 = 4 * 1024;

/// Whether `key` names the drive's own bookkeeping: `.azlin` or anything in
/// `.azlin/`, at the drive's root.
#[must_use]
pub fn is_reserved_key(key: &str) -> bool {
    key == MANIFEST_DIR
        || key
            .strip_prefix(MANIFEST_DIR)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// What the manifest knows about one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Bytes.
    pub size: u64,
    /// Last modified, in seconds since 1970-01-01 UTC, if known.
    pub modified: Option<u64>,
    /// The lowercase hex SHA-256 of the content.
    pub hash: String,
}

/// One line of the manifest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// The object `key` is there.
    Put { key: String, entry: ManifestEntry },
    /// The object `key` is gone; a key ending in `/` is a folder with
    /// everything under it.
    Delete { key: String },
    /// The object (or, ending in `/`, the folder) `from` is now `to`.
    Rename { from: String, to: String },
}

impl Record {
    /// The record as one line of the file, without its `\n`.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Record::Put { key, entry } => format!(
                "+ {} {} {} {}",
                entry.size,
                entry
                    .modified
                    .map_or_else(|| String::from("-"), |m| m.to_string()),
                entry.hash,
                uri_encode(key, false)
            ),
            Record::Delete { key } => format!("- {}", uri_encode(key, false)),
            Record::Rename { from, to } => {
                format!("> {} {}", uri_encode(from, false), uri_encode(to, false))
            }
        }
    }

    /// The record of one line (without its `\n`); `None` for a header, an
    /// empty line, a record this version does not know or a damaged one.
    #[must_use]
    pub fn parse_line(line: &str) -> Option<Record> {
        let mut fields = line.split(' ');
        let record = match fields.next()? {
            "+" => {
                let size = fields.next()?.parse().ok()?;
                let modified = match fields.next()? {
                    "-" => None,
                    m => Some(m.parse().ok()?),
                };
                let hash = fields.next()?;
                if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return None;
                }
                Record::Put {
                    key: decode_key(fields.next()?)?,
                    entry: ManifestEntry {
                        size,
                        modified,
                        hash: hash.to_string(),
                    },
                }
            }
            "-" => Record::Delete {
                key: decode_key(fields.next()?)?,
            },
            ">" => Record::Rename {
                from: decode_key(fields.next()?)?,
                to: decode_key(fields.next()?)?,
            },
            _ => return None,
        };
        if fields.next().is_some() {
            return None;
        }
        Some(record)
    }
}

fn decode_key(field: &str) -> Option<String> {
    let key = uri_decode(field)?;
    (!key.is_empty()).then_some(key)
}

/// What the manifest records: every object of the tree by key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    entries: BTreeMap<String, ManifestEntry>,
}

impl Manifest {
    /// The entry of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ManifestEntry> {
        self.entries.get(key)
    }

    /// Every entry, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ManifestEntry)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), e))
    }

    /// How many objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records `key` (replacing what was there).
    pub fn insert(&mut self, key: &str, entry: ManifestEntry) {
        self.entries.insert(key.to_string(), entry);
    }

    /// Forgets `key`; a key ending in `/` forgets the folder with everything
    /// under it.
    pub fn remove(&mut self, key: &str) {
        if key.ends_with('/') {
            self.entries.retain(|k, _| !k.starts_with(key));
        } else {
            self.entries.remove(key);
        }
    }

    /// Moves `from` to `to`: one key, or (both ending in `/`) every key
    /// under the folder `from`.
    pub fn rename(&mut self, from: &str, to: &str) {
        if from.ends_with('/') {
            let moved: Vec<String> = self
                .entries
                .keys()
                .filter(|k| k.starts_with(from))
                .cloned()
                .collect();
            for key in moved {
                if let Some(entry) = self.entries.remove(&key) {
                    self.entries
                        .insert(format!("{to}{}", &key[from.len()..]), entry);
                }
            }
        } else if let Some(entry) = self.entries.remove(from) {
            self.entries.insert(to.to_string(), entry);
        }
    }

    /// Applies one record.
    pub fn apply(&mut self, record: Record) {
        match record {
            Record::Put { key, entry } => {
                self.entries.insert(key, entry);
            }
            Record::Delete { key } => self.remove(&key),
            Record::Rename { from, to } => self.rename(&from, &to),
        }
    }

    /// The manifest a file's text describes: its complete lines applied in
    /// order (a torn last line, unknown records and damaged ones skipped).
    #[must_use]
    pub fn parse(text: &str) -> Manifest {
        let mut manifest = Manifest::default();
        manifest.apply_text(text);
        manifest
    }

    /// Applies the complete lines of `text` (see [`Manifest::parse`]).
    pub fn apply_text(&mut self, text: &str) {
        let Some(end) = text.rfind('\n') else {
            return;
        };
        for line in text[..end].split('\n') {
            if let Some(record) = Record::parse_line(line.trim_end_matches('\r')) {
                self.apply(record);
            }
        }
    }

    /// The manifest as a snapshot file: the header, then one `+` line per
    /// object in key order.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut body = String::new();
        for (key, entry) in &self.entries {
            body.push_str(
                &Record::Put {
                    key: key.clone(),
                    entry: entry.clone(),
                }
                .line(),
            );
            body.push('\n');
        }
        let mut text = header(header(0).len() + body.len());
        text.push_str(&body);
        text
    }
}

/// The header line (with its `\n`) of a file whose last snapshot was
/// `snapshot` bytes.
fn header(snapshot: usize) -> String {
    format!("{MAGIC} {VERSION} {snapshot:0width$}\n", width = SNAPSHOT_DIGITS)
}

/// The snapshot size a header line names (0 when it is not a header).
fn snapshot_size(first_line: &str) -> u64 {
    let mut fields = first_line.split(' ');
    match (fields.next(), fields.next(), fields.next()) {
        (Some(MAGIC), Some(_), Some(size)) => size.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// What changed between two states of a tree, each list in key order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changes {
    /// There now, not before.
    pub added: Vec<String>,
    /// There before and now, with other content.
    pub modified: Vec<String>,
    /// There before, not now.
    pub deleted: Vec<String>,
}

impl Changes {
    /// Nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }
}

/// What changed from the manifest `base` to the manifest `now`, by size and
/// hash, without touching a drive (a new modification time alone is no
/// change). The sync compares the manifest of its last run with today's.
#[must_use]
pub fn changes(base: &Manifest, now: &Manifest) -> Changes {
    let mut out = Changes::default();
    for (key, entry) in now.iter() {
        match base.get(key) {
            None => out.added.push(key.to_string()),
            Some(old) if old.size != entry.size || old.hash != entry.hash => {
                out.modified.push(key.to_string());
            }
            Some(_) => {}
        }
    }
    for (key, _) in base.iter() {
        if now.get(key).is_none() {
            out.deleted.push(key.to_string());
        }
    }
    out
}

/// The SHA-256 of the object `key`: read from its file in pieces when the
/// drive is a folder on disk, else fetched whole.
pub(crate) fn hash_object<D: Drive + ?Sized>(drive: &D, key: &str) -> Result<String, DriveError> {
    if let Some(path) = drive.local_path(key) {
        if path.is_file() {
            return File::open(&path)
                .and_then(sha256_hex_of)
                .map_err(|e| DriveError::Io(format!("{key}: {e}")));
        }
    }
    drive.get(key).map(|bytes| sha256_hex(&bytes))
}

/// An object whose time moved but whose bytes did not, with its fresh
/// entry (a refresh records the new time without hashing it again).
pub(crate) struct Touched {
    pub(crate) key: String,
    pub(crate) entry: ManifestEntry,
}

/// The scan behind [`diff`]: the changes, and the touched objects.
pub(crate) fn scan<D: Drive + ?Sized>(
    manifest: &Manifest,
    drive: &D,
) -> Result<(Changes, Vec<Touched>), DriveError> {
    let mut out = Changes::default();
    let mut touched = Vec::new();
    let mut seen = BTreeSet::new();
    for object in list_all(drive, "")? {
        // The bookkeeping and folder markers are not content.
        if is_reserved_key(&object.key) || object.key.ends_with('/') {
            continue;
        }
        seen.insert(object.key.clone());
        let Some(known) = manifest.get(&object.key) else {
            out.added.push(object.key);
            continue;
        };
        if known.size != object.size {
            out.modified.push(object.key);
            continue;
        }
        if object.modified.is_some() && object.modified == known.modified {
            continue; // same size, same time: unchanged, without reading it
        }
        // Same size, another (or no) time: only the bytes can tell.
        let hash = hash_object(drive, &object.key)?;
        if hash == known.hash {
            touched.push(Touched {
                key: object.key,
                entry: ManifestEntry {
                    size: object.size,
                    modified: object.modified,
                    hash,
                },
            });
        } else {
            out.modified.push(object.key);
        }
    }
    for (key, _) in manifest.iter() {
        if !seen.contains(key) {
            out.deleted.push(key.to_string());
        }
    }
    Ok((out, touched))
}

/// What changed in `drive` since `manifest`: objects added, modified (another
/// size, or another time AND other bytes) and deleted, each in key order.
/// Only the objects whose size matches but whose time moved are read. The
/// drive can be the data tree itself (what changed outside the drive's own
/// calls) or any other drive (the bucket the sync compares with).
pub fn diff<D: Drive + ?Sized>(manifest: &Manifest, drive: &D) -> Result<Changes, DriveError> {
    scan(manifest, drive).map(|(changes, _)| changes)
}

// ---------------------------------------------------------------------------
// The file (kept by the LocalDrive)
// ---------------------------------------------------------------------------

/// Reads the manifest file at `path`, with the bytes it was read from; a
/// missing file is an empty manifest.
pub(crate) fn load(path: &Path) -> io::Result<(Manifest, Vec<u8>)> {
    match fs::read(path) {
        Ok(bytes) => Ok((Manifest::parse(&String::from_utf8_lossy(&bytes)), bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok((Manifest::default(), Vec::new())),
        Err(e) => Err(e),
    }
}

/// Appends `record` to the manifest file at `path` in ONE write (creating
/// the folder, and the header when the file is new), then compacts when the
/// file has grown past twice its last snapshot.
pub(crate) fn append(path: &Path, record: &Record) -> Result<(), DriveError> {
    let open = || fs::OpenOptions::new().create(true).append(true).open(path);
    let mut file = match open() {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            open()?
        }
        Err(e) => return Err(e.into()),
    };
    let mut line = String::new();
    if file.metadata()?.len() == 0 {
        line.push_str(&header(0));
    }
    line.push_str(&record.line());
    line.push('\n');
    file.write_all(line.as_bytes())?;
    let len = file.metadata()?.len();
    drop(file);
    if len > COMPACT_MIN_BYTES && len > 2 * read_snapshot_size(path) {
        rewrite(path, |_| Ok(()))?;
    }
    Ok(())
}

/// The snapshot size the header of the file at `path` names.
fn read_snapshot_size(path: &Path) -> u64 {
    let mut first = [0u8; 64];
    let Ok(mut file) = File::open(path) else {
        return 0;
    };
    let n = file.read(&mut first).unwrap_or(0);
    let text = String::from_utf8_lossy(&first[..n]);
    snapshot_size(text.split('\n').next().unwrap_or(""))
}

/// Rewrites the manifest file at `path` as a snapshot of what it records
/// after `change` (when `change` fails, nothing is written). Records another
/// writer appended while `change` ran are applied on top before the rename.
pub(crate) fn rewrite(
    path: &Path,
    change: impl FnOnce(&mut Manifest) -> Result<(), DriveError>,
) -> Result<(), DriveError> {
    let (mut manifest, read) = load(path)?;
    change(&mut manifest)?;
    if let Ok(now) = fs::read(path) {
        if now.len() > read.len() && now.starts_with(&read) {
            manifest.apply_text(&String::from_utf8_lossy(&now[read.len()..]));
        }
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    crate::local::write_atomically(path, manifest.to_text().as_bytes())?;
    Ok(())
}
