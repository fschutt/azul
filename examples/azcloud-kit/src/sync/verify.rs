//! The read-back: a pairing checks both sides' bytes against the hashes it knows, so damage is
//! found before anyone needs the file.
//!
//! - **The drive's copies**: every file the drive's index names is read back and checked
//!   against its BLAKE3. A copy that does not read back whole is listed (`hash-mismatch`) and,
//!   when this device holds that very version, sent again from here - the drive heals from a
//!   good copy, never from a guess.
//! - **This device's copies**: a synced file whose size, times and identity are as its last
//!   sync left them but whose bytes are not (a failing disk, a raw write) is no edit: the
//!   drive's version is put back and the damaged bytes go into the folder's trash (and the
//!   operations log). A pass would have trusted the unchanged file and never read it. A file
//!   whose status-change time moved was written to: that is an edit, whatever it did to the
//!   size and the modification time, and the next pass sends it.
//!
//! The report is kept in the pairing's state folder ([`VERIFY_FILE`]): the window says when the
//! folder was last checked and what was found.

use std::{collections::BTreeSet, path::Path};

use azul_storage::{
    beneath::{self, BeneathError, Fingerprint, Kind},
    durable::{self, Expect, PublishError},
};
use serde::{Deserialize, Serialize};

use super::{
    hash_here, load_cache,
    local::{entry_now, path_of, stat_now},
    named, names,
    oplog::{Op, OpKind, OpLog},
    problems::{Code, Problem},
    remote::{self, RemoteFile},
    root, synced_index, synced_publish, BlobIndex, LocalRoot, SyncOptions, SyncRemote, Target,
    TempBlob,
};
use crate::{
    error::{fail, CloudError, CloudResult},
    store::BIG_BLOB,
};

/// The last read-back's report, in a pairing's state folder.
pub const VERIFY_FILE: &str = "verify.json";

/// What a read-back found and did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// When it ran (seconds).
    pub at: i64,
    /// The drive's files read back.
    pub checked: usize,
    /// The drive's copies that did not read back whole, and of those the ones sent again from
    /// this device's good copy.
    pub mismatches: Vec<String>,
    pub healed: Vec<String>,
    /// This device's copies whose bytes changed under the same size and time, and of those the
    /// ones whose synced version was put back.
    pub rotted: Vec<String>,
    pub restored: Vec<String>,
    /// What it lists, one `hash-mismatch` per file.
    pub problems: Vec<Problem>,
}

/// Sends `key`'s version `hash` again from the file at `path` (`size` bytes).
type Heal<'a> = &'a dyn Fn(&str, &str, &Path, u64) -> CloudResult<()>;

/// Reads both sides of the synced folder `root` back (its base in `index_path`), against the
/// drive behind `target`.
///
/// # Errors
///
/// When the folder was never synced or the drive's index cannot be read; a file that cannot
/// be read back is a finding, no error.
pub fn verify(
    target: Target<'_>,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
) -> CloudResult<VerifyReport> {
    match target {
        Target::Index(store) => {
            let heal = |_key: &str, hash: &str, path: &Path, size: u64| -> CloudResult<()> {
                let mut file = std::fs::File::open(path)?;
                store.put_from(&remote::blob_key(&opts.prefix, hash), &mut file, size)?;
                Ok(())
            };
            run(&BlobIndex::new(store, &opts.prefix), root, index_path, &heal)
        }
        Target::Named(drive) => {
            let heal = |key: &str, _hash: &str, path: &Path, _size: u64| -> CloudResult<()> {
                let mut file = std::fs::File::open(path)?;
                drive.put_from(&format!("{}{key}", opts.prefix), &mut file)?;
                Ok(())
            };
            run(
                &named::NamedFiles::new(drive, &opts.prefix, index_path),
                root,
                index_path,
                &heal,
            )
        }
    }
}

fn run<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    heal: Heal<'_>,
) -> CloudResult<VerifyReport> {
    let mut index = synced_index(root, index_path)?;
    let cached = load_cache(index_path, &index);
    let (drive, _) = remote_side.read(cached.as_ref())?;
    let mut report = VerifyReport {
        at: crate::now(),
        ..VerifyReport::default()
    };
    // The drive's copies.
    let mut whole: BTreeSet<&str> = BTreeSet::new();
    for (key, file) in &drive.files {
        report.checked += 1;
        // A link's content is its target, which its entry holds: nothing to read back.
        if file.link.is_some() {
            whole.insert(key.as_str());
            continue;
        }
        let Err(e) = read_back(remote_side, root, key, file) else {
            whole.insert(key.as_str());
            continue;
        };
        report.mismatches.push(key.clone());
        let here = path_of(&root.path, key);
        let good_here = hash_here(&root.path, key).as_deref() == Some(file.hash.as_str());
        let healed = good_here && heal(key, &file.hash, &here, file.size).is_ok();
        if healed {
            report.healed.push(key.clone());
        }
        report.problems.push(Problem::new(
            Code::HashMismatch,
            key,
            format!(
                "the drive's copy did not read back whole ({e}); {}",
                if healed {
                    "it was sent again from this device"
                } else {
                    "no good copy of this version is on this device"
                }
            ),
        ));
    }
    // This device's copies: the ones a pass would trust by their size and time.
    let tree = beneath::Root::open(&root.path)?;
    let stamp = root::trash_stamp(crate::now());
    let log = OpLog::new(index_path.parent().unwrap_or_else(|| Path::new(".")));
    let synced: Vec<(String, super::BaseEntry)> = index
        .files
        .iter()
        .filter(|(_, e)| !e.cloud_only)
        .map(|(k, e)| (k.clone(), e.clone()))
        .collect();
    let mut changed_base = false;
    for (key, entry) in synced {
        let Ok(Some(st)) = stat_now(&root.path, &key) else {
            continue;
        };
        // As it was when it was hashed (AZ-S13 - size and time alone also pass an edit that keeps
        // both, `touch -r`, a volume with whole-second times; an edit cannot set the status-change
        // time back): the question the scan asks before it trusts a hash. Anything else is an
        // edit, the next pass's.
        if st.kind != Kind::File || !entry.holds(&st, index.scanned_at_ns) {
            continue;
        }
        let Some(hash) = hash_here(&root.path, &key) else {
            continue;
        };
        if hash == entry.hash {
            continue;
        }
        report.rotted.push(key.clone());
        let there = drive.files.get(&key).filter(|f| f.hash == entry.hash);
        let restored = match there {
            Some(file) if whole.contains(key.as_str()) => {
                restore(
                    remote_side,
                    &root.path,
                    &tree,
                    &log,
                    &stamp,
                    &key,
                    &hash,
                    Fingerprint::of(&st),
                    file,
                )
                .is_ok()
            }
            _ => false,
        };
        if restored {
            report.restored.push(key.clone());
            if let Some(now) = entry_now(&root.path, &key, &entry.hash) {
                index.files.insert(key.clone(), now);
                changed_base = true;
            }
        }
        report.problems.push(Problem::new(
            Code::HashMismatch,
            &key,
            if restored {
                "this device's copy changed under the same size and time (a failing disk?); the \
                 synced version was put back and the damaged bytes are in the trash"
            } else {
                "this device's copy changed under the same size and time (a failing disk?); no \
                 good copy of the synced version is on the drive to put back"
            },
        ));
    }
    if changed_base {
        index.save(index_path)?;
    }
    Ok(report)
}

/// Reads the drive's copy of `key` back whole, checked against its BLAKE3 (a big file through
/// the folder's spool, never whole in memory).
fn read_back<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    key: &str,
    file: &RemoteFile,
) -> CloudResult<()> {
    if file.size > BIG_BLOB {
        let spool = TempBlob::for_download(&root.path, key, &file.hash)?;
        remote_side.fetch_to(key, &file.hash, file.size, spool.path())?;
    } else {
        remote_side.fetch(key, &file.hash, file.size)?;
    }
    Ok(())
}

/// The drive's version of a file, fetched and checked against its BLAKE3: in memory when small,
/// in the folder's spool when big.
enum Replacement {
    Bytes(Vec<u8>),
    Spool(TempBlob),
}

/// Puts the drive's version `file` of `key` back over this device's damaged copy (whose bytes
/// hash to `damaged`, and which was `seen` when they were hashed), which goes into the trash of
/// `stamp`, logged.
///
/// The replacement is fetched and checked FIRST (AZ-S4) and staged whole and synced beside the
/// damaged file; only then does [`durable::replace_keeping`] move the damaged file into the trash
/// and the replacement into its place, and put the damaged file back when that fails. The name is
/// never empty while the replacement is written - a pass that met a gap read "gone here, unchanged
/// on the drive" as a delete, wrote a tombstone and trashed the good copy on every device - and a
/// damaged file that was written to since it was hashed (`seen` no longer matches) is left as it is.
#[allow(clippy::too_many_arguments)]
fn restore<R: SyncRemote + ?Sized>(
    remote_side: &R,
    folder: &Path,
    tree: &beneath::Root,
    log: &OpLog,
    stamp: &str,
    key: &str,
    damaged: &str,
    seen: Fingerprint,
    file: &RemoteFile,
) -> CloudResult<()> {
    let (dir, name) = names::walk_local(tree, key, false)?;
    let name = name.as_str();
    let replacement = if file.size > BIG_BLOB {
        let blob = TempBlob::for_download(folder, key, &file.hash)?;
        remote_side.fetch_to(key, &file.hash, file.size, blob.path())?;
        Replacement::Spool(blob)
    } else {
        Replacement::Bytes(remote_side.fetch(key, &file.hash, file.size)?)
    };
    let (trash, target, rel) = root::trash_target(tree, key, stamp)?;
    let publish = synced_publish(Expect::Is(seen), file.exec, file.modified_ns());
    let old = dir.lstat(name).ok();
    let spool;
    let spooled;
    let staged = match &replacement {
        Replacement::Spool(blob) => {
            spool = root::spool(tree)?;
            spooled = blob.name();
            durable::stage_blob(
                durable::Blob {
                    dir: &spool,
                    name: &spooled,
                },
                old.as_ref(),
                &publish,
            )
        }
        Replacement::Bytes(bytes) => durable::stage(&dir, name, &mut &bytes[..], &publish),
    }
    .map_err(BeneathError::from)?;
    let slot = durable::TrashSlot {
        dir: &trash,
        name: &target,
    };
    match durable::replace_keeping(&dir, name, staged, &slot, Expect::Is(seen)) {
        Ok(_) => {
            let _ = log.record(&Op {
                t: crate::now() as f64,
                op: OpKind::Trash,
                key: key.to_string(),
                to: Some(rel),
                hash: Some(damaged.to_string()),
            });
            Ok(())
        }
        Err(PublishError::Changed) => fail!("{key} turned up again meanwhile; it is left as it is"),
        // The damaged copy is back in its place (or still is): no pass takes a gap for a delete.
        Err(e) => Err(CloudError::from(BeneathError::from(e))),
    }
}
