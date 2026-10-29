//! A synced folder's own bookkeeping, in `<root>/.azlin/sync/` (no name from the drive can
//! point there):
//!
//! - `root-id`, the folder's marker: written when the folder is paired. A pass whose folder is
//!   missing, is no folder, or lacks the marker its base names changes nothing anywhere, so an
//!   unplugged disk (an empty mount point, or one that still holds a stale copy) never reads as
//!   "every file was deleted".
//! - `trash/<stamp>/`, where a delete here moves the file (a rename on the folder's own file
//!   system), so a delete that came from another device is always recoverable here.
//! - `tmp/`, where a big download is spooled on the folder's own disk (never the system's
//!   temporary folder, often a small tmpfs), then renamed into place.
//!
//! What the disk does with names ([`Volume`]) is probed in the same folder.

use std::{collections::BTreeMap, path::Path};

use azul_storage::beneath::{self, BeneathError, Kind, Root};
use serde::{Deserialize, Serialize};

use super::{
    local::BaseEntry,
    names::{System, Volume},
};

/// The bookkeeping folder of a synced folder, from its root.
pub const SYNC_DIR: &str = ".azlin/sync";
/// The marker's file in it.
pub const MARKER_FILE: &str = "root-id";
/// The trash, in it.
pub const TRASH_DIR: &str = ".azlin/sync/trash";
/// Where big downloads are spooled, in it.
pub const SPOOL_DIR: &str = ".azlin/sync/tmp";
/// How long a file stays in the trash here; the drive keeps its versions for longer.
pub const TRASH_DAYS: i64 = 30;

/// The marker in a paired folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// Random, 32 hex digits: the folder's identity, kept in the base too.
    pub id: String,
    /// When it was paired (seconds).
    pub created: i64,
}

/// What a pass found where its folder should be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootCheck {
    /// The paired folder, with its marker (or a folder paired now).
    Paired(Marker),
    /// Nothing is there (a disk not mounted, a folder deleted or moved).
    Missing,
    /// Something is there that is not the paired folder: a file, or a folder without the
    /// marker the base names (an empty mount point, a stale copy under it, another folder).
    Replaced(String),
    /// The folder is there but cannot be read (its permission taken away, a privacy setting
    /// that keeps this app out): nothing about its files is known.
    Unreadable(String),
}

fn new_id() -> String {
    let mut bytes = [0u8; 16];
    // The OS random source; a failure leaves a time-based id, still unique enough to tell
    // two folders apart.
    if getrandom::getrandom(&mut bytes).is_err() {
        let now = crate::now_ns().to_le_bytes();
        bytes[..8].copy_from_slice(&now);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The marker of the folder `root`, if it has one.
#[must_use]
pub fn read_marker(root: &Path) -> Option<Marker> {
    let root = Root::open(root).ok()?;
    let dir = root.dir(SYNC_DIR, false).ok()?;
    let (mut file, _) = dir.open_read(MARKER_FILE).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).ok()?;
    serde_json::from_str::<Marker>(&text)
        .ok()
        .filter(|m| !m.id.is_empty())
}

/// Writes a new marker into the folder `root` (which must exist).
///
/// # Errors
///
/// When the bookkeeping folder cannot be made or written.
pub fn write_marker(root: &Path) -> Result<Marker, BeneathError> {
    let marker = Marker {
        id: new_id(),
        created: crate::now(),
    };
    let root = Root::open(root)?;
    let dir = root.dir(SYNC_DIR, true)?;
    let text = serde_json::to_string_pretty(&marker).unwrap_or_default();
    dir.write_atomically(MARKER_FILE, text.as_bytes(), 0o644)?;
    Ok(marker)
}

/// Checks the folder at `root` against the marker its base names (`expected`; `None`: a
/// pairing without one yet - a new pairing, or one made before markers existed). `base`: the
/// files the pairing synced, `None` while nothing is paired (the base knows no file and no
/// generation). A folder that is missing when nothing is paired yet is made; a folder without
/// a marker and without an expected one gets one - when the pairing is older than markers,
/// only while it holds one of its files as it was synced (its size and time).
///
/// # Errors
///
/// When the folder or its marker cannot be made.
pub fn check(
    root: &Path,
    expected: Option<&str>,
    base: Option<&BTreeMap<String, BaseEntry>>,
) -> Result<RootCheck, BeneathError> {
    let meta = match std::fs::metadata(root) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if expected.is_some() || base.is_some() {
                return Ok(RootCheck::Missing);
            }
            std::fs::create_dir_all(root)?;
            std::fs::metadata(root)?
        }
        Err(e) => return Err(e.into()),
    };
    if !meta.is_dir() {
        return Ok(RootCheck::Replaced(format!(
            "{} is no folder",
            root.display()
        )));
    }
    // A folder this app may not read shows no marker either: it is no other folder, it is
    // this one closed to us.
    if let Err(e) = std::fs::read_dir(root) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return Ok(RootCheck::Unreadable(format!(
                "{} cannot be read ({e})",
                root.display()
            )));
        }
    }
    match (read_marker(root), expected) {
        (Some(marker), Some(id)) if marker.id == id => Ok(RootCheck::Paired(marker)),
        (Some(_), Some(_)) => Ok(RootCheck::Replaced(format!(
            "{} holds the marker of another synced folder",
            root.display()
        ))),
        (None, Some(_)) => Ok(RootCheck::Replaced(format!(
            "{} lacks the marker it was paired with (a disk not mounted, or another folder)",
            root.display()
        ))),
        // A pairing from before markers: only its files tell its folder. An empty mount point,
        // or a stale copy written under it, holds none of them as synced.
        (_, None) if base.is_some_and(|files| !holds_synced(root, files)) => {
            Ok(RootCheck::Replaced(format!(
                "{} holds none of the files synced from it as they were, and no marker (a disk \
                 not mounted, or another folder)",
                root.display()
            )))
        }
        // A folder synced elsewhere before (moved here, plugged in again) keeps its identity.
        (Some(marker), None) => Ok(RootCheck::Paired(marker)),
        (None, None) => Ok(RootCheck::Paired(write_marker(root)?)),
    }
}

/// Whether the folder `root` holds one of the files `base` synced from it as it was synced (its
/// size and time), or `base` synced none here (all in the cloud only): the evidence a pairing
/// without a marker has that this is its folder. Inode numbers are not compared: FAT and
/// exFAT give a file another one after a remount.
fn holds_synced(root: &Path, base: &BTreeMap<String, BaseEntry>) -> bool {
    let mut here = base.iter().filter(|(_, e)| !e.cloud_only).peekable();
    if here.peek().is_none() {
        return true;
    }
    let Ok(tree) = Root::open(root) else {
        return false;
    };
    here.any(|(key, entry)| {
        let (folder, name) = beneath::split_key(key);
        tree.dir(folder, false)
            .ok()
            .and_then(|dir| dir.lstat(name).ok())
            .is_some_and(|st| {
                st.kind == Kind::File && st.size == entry.size && st.mtime_ns == entry.mtime_ns
            })
    })
}

/// The trash folder of one pass: `20261024-101500-a1b2c3`, so passes never mix their files.
#[must_use]
pub fn trash_stamp(now: i64) -> String {
    let stamp = crate::rfc3339(now);
    let digits: String = stamp
        .chars()
        .take(19)
        .filter(char::is_ascii_digit)
        .collect();
    let date = digits.get(..8).unwrap_or("00000000");
    let time = digits.get(8..14).unwrap_or("000000");
    format!("{date}-{time}-{}", &new_id()[..6])
}

/// Where the file of `key` goes in the trash of the pass `stamp`: under its path from the root,
/// so a rename on the folder's own file system takes it there. The trash folder (made when
/// missing), the name in it (a second file of that name in one pass gets ` (2)`), and that path
/// from the root.
///
/// # Errors
///
/// When the trash folder cannot be made.
pub fn trash_target(
    root: &Root,
    key: &str,
    stamp: &str,
) -> Result<(beneath::Dir, String, String), BeneathError> {
    let (folder, name) = beneath::split_key(key);
    let rel = if folder.is_empty() {
        format!("{TRASH_DIR}/{stamp}")
    } else {
        format!("{TRASH_DIR}/{stamp}/{folder}")
    };
    let trash = root.dir(&rel, true)?;
    let mut target = name.to_string();
    let mut n = 2;
    while trash.lstat(&target).is_ok() {
        target = format!("{name} ({n})");
        n += 1;
    }
    let path = format!("{rel}/{target}");
    Ok((trash, target, path))
}

/// Removes the trash folders of passes older than [`TRASH_DAYS`] (their names start with the
/// date). The bytes stay on the drive as its versions.
pub fn purge_trash(root: &Root, now: i64) {
    let Ok(trash) = root.dir(TRASH_DIR, false) else {
        return;
    };
    let horizon = crate::rfc3339(now - TRASH_DAYS * 86_400);
    let horizon: String = horizon
        .chars()
        .take(10)
        .filter(char::is_ascii_digit)
        .collect();
    let Ok(names) = trash.names() else {
        return;
    };
    for name in names {
        let Some(name) = name.to_str() else { continue };
        let date = name.get(..8).unwrap_or("");
        if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) && date < horizon.as_str() {
            remove_tree(&trash, name);
        }
    }
}

/// Removes the folder `name` of `dir` with everything in it, never through a link (a link in
/// it is removed as the link).
fn remove_tree(dir: &beneath::Dir, name: &str) {
    match dir.lstat(name) {
        Ok(st) if st.kind == Kind::Dir => {
            if let Ok(child) = dir.child(name, false) {
                if let Ok(names) = child.names() {
                    for inner in names {
                        if let Some(inner) = inner.to_str() {
                            remove_tree(&child, inner);
                        }
                    }
                }
            }
            let _ = dir.rmdir(name);
        }
        Ok(_) => {
            let _ = dir.unlink(name);
        }
        Err(_) => {}
    }
}

/// The spool folder for big downloads, made when missing.
///
/// # Errors
///
/// When it cannot be made.
pub fn spool(root: &Root) -> Result<beneath::Dir, BeneathError> {
    root.dir(SPOOL_DIR, true)
}

/// What the disk under `root` does with names, probed with empty files in the bookkeeping
/// folder (removed again). Windows refuses its own names anyway, and creating `a:b` there
/// would write a stream of `a`, so that probe runs elsewhere only.
#[must_use]
pub fn probe_volume(root: &Path) -> Volume {
    // A system without permission bits (Windows) keeps none on any disk: a file there keeps the
    // bit its synced version has, as on a FAT disk elsewhere.
    let mut volume = Volume {
        max_file: beneath::file_size_limit(root),
        no_exec_bits: !cfg!(unix),
        ..Volume::default()
    };
    let Ok(root) = Root::open(root) else {
        return volume;
    };
    let Ok(dir) = root.dir(SYNC_DIR, true) else {
        return volume;
    };
    let touch = |name: &str| dir.create_new(name, 0o600).map(drop).is_ok();
    if touch("probe-Aa") {
        volume.case_insensitive = dir.lstat("probe-aA").is_ok();
        let _ = dir.unlink("probe-Aa");
    }
    // The decomposed name, looked up composed.
    if touch("probe-e\u{301}") {
        volume.norm_insensitive = dir.lstat("probe-\u{e9}").is_ok();
        let _ = dir.unlink("probe-e\u{301}");
    } else {
        volume.norm_insensitive = true;
    }
    // A file made without the executable bit that shows it, or one given the bit that does
    // not: the disk keeps no permission bits.
    #[cfg(unix)]
    if let Ok(file) = dir.create_new("probe-mode", 0o600) {
        use std::os::unix::fs::PermissionsExt;
        let executable = |file: &std::fs::File| {
            file.metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
        };
        let shown_plain = executable(&file).unwrap_or(false);
        let shown_set = file
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .and_then(|()| executable(&file));
        volume.no_exec_bits = shown_plain || matches!(shown_set, Ok(false));
        drop(file);
        let _ = dir.unlink("probe-mode");
    }
    if System::here() != System::Windows {
        for name in ["probe-a:b", "probe-q?", "probe-dot."] {
            if !touch(name) {
                volume.windows_names = true;
                break;
            }
            let listed = dir
                .names()
                .map(|names| names.iter().any(|n| n.to_str() == Some(name)))
                .unwrap_or(false);
            let _ = dir.unlink(name);
            if !listed {
                volume.windows_names = true;
                break;
            }
        }
    }
    volume
}
