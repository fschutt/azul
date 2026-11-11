//! The read-back: a pairing checks both sides' bytes against the hashes it knows, so damage is
//! found before anyone needs the file.
//!
//! - **The drive's copies**: every file the drive's index names is read back and checked
//!   against its BLAKE3. A copy is DAMAGED when its bytes were read whole and are not the
//!   version listed while the drive still names that version ([`ReadBack`]); it is listed
//!   (`hash-mismatch`) and, when this device holds that very version, sent again from here -
//!   only as a write that is conditional on the version the listing carried, so another
//!   device's newer file is never overwritten. A read that FAILED (a dropped connection, a 404,
//!   a copy that does not authenticate, a timeout) or that met a version another device wrote
//!   meanwhile says nothing about the copy: it is listed as not checked and nothing is written
//!   (SYNC-4; the drive heals from a good copy, never from a guess).
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
    DriveError,
};
use serde::{Deserialize, Serialize};

use super::{
    hash_here,
    journal::{Intent, Journal},
    load_cache,
    local::{entry_now, path_of, stat_now},
    named, names,
    oplog::{Op, OpKind, OpLog},
    problems::{Code, Problem},
    remote::RemoteFile,
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
    /// The drive's copies whose read failed: not checked, not touched (SYNC-4).
    #[serde(default)]
    pub unread: Vec<String>,
    /// The drive's copies another device changed or deleted while the check ran: left as they
    /// are, the next pass takes the new version (SYNC-4).
    #[serde(default)]
    pub changed: Vec<String>,
    /// This device's copies whose bytes changed under the same size and time, and of those the
    /// ones whose synced version was put back.
    pub rotted: Vec<String>,
    pub restored: Vec<String>,
    /// What it lists: a `hash-mismatch` per damaged copy, a `changed-during-sync` per drive copy
    /// another device changed meanwhile, an `unchecked` per read that failed.
    pub problems: Vec<Problem>,
}

/// What reading one of the drive's copies back found (SYNC-4). Damage is ONE finding: the bytes
/// were read whole, they are not the version the listing named, and the drive - asked again -
/// still names that very version. Everything else says nothing about the copy and is never healed:
/// the read-back used to take any failed read for damage and write this device's copy over the
/// drive's file, which overwrote a newer version another device had just saved.
#[derive(Debug)]
pub(super) enum ReadBack {
    /// The bytes are the version's.
    Whole,
    /// Read whole, not the listed version's, and the drive still names the version the check
    /// began with: the drive's copy is damaged.
    Damaged,
    /// Bytes that are not the listed version's because the drive names another version now (or
    /// none): another device wrote or deleted the file since the listing. Not damage.
    Changed,
    /// The read itself failed - a dropped connection, a timeout, a copy that does not
    /// authenticate, a drive that cannot say which version it holds - so there are no bytes to
    /// judge. Not damage.
    Unread(CloudError),
}

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
    verify_with(target, root, index_path, opts, &|_| {})
}

/// [`verify`] that tells `point` where it stands (`before-replace`: a damaged copy's replace is
/// written down in the journal and has moved nothing yet; [`super::faults::POINTS`], heard
/// through `SyncSession::with_points`).
///
/// # Errors
///
/// As [`verify`].
pub(crate) fn verify_with(
    target: Target<'_>,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    point: &dyn Fn(&str),
) -> CloudResult<VerifyReport> {
    match target {
        Target::Index(store) => run(
            &BlobIndex::new(store, &opts.prefix),
            root,
            index_path,
            point,
        ),
        Target::Named(drive) => run(
            &named::NamedFiles::new(drive, &opts.prefix, index_path),
            root,
            index_path,
            point,
        ),
    }
}

/// Which problem a read that failed is listed as: a copy that does not authenticate is the
/// drive's damage or a change by someone without the key (`hash-mismatch`, an error); any other
/// failure is a check this device could not make (`unchecked`, a warning).
fn unread_code(e: &CloudError) -> Code {
    match e.root() {
        CloudError::Drive(DriveError::Corrupt { .. }) => Code::HashMismatch,
        _ => Code::Unchecked,
    }
}

fn run<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    point: &dyn Fn(&str),
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
        match read_back(remote_side, root, key, file) {
            ReadBack::Whole => {
                whole.insert(key.as_str());
            }
            ReadBack::Changed => {
                report.changed.push(key.clone());
                report.problems.push(Problem::new(
                    Code::ChangedDuringSync,
                    key,
                    "another device changed or deleted the drive's copy while it was read back; \
                     it was left as it is and the next pass takes the new version",
                ));
            }
            ReadBack::Unread(e) => {
                report.unread.push(key.clone());
                report.problems.push(Problem::new(
                    unread_code(&e),
                    key,
                    format!(
                        "the drive's copy could not be read back ({e}); it was not checked and \
                         nothing was changed"
                    ),
                ));
            }
            ReadBack::Damaged => {
                report.mismatches.push(key.clone());
                let here = path_of(&root.path, key);
                let good_here = hash_here(&root.path, key).as_deref() == Some(file.hash.as_str());
                let outcome = if good_here {
                    remote_side.heal(key, file, &here)
                } else {
                    Err(CloudError::failed(
                        "no good copy of this version is on this device",
                    ))
                };
                let healed = outcome.is_ok();
                if healed {
                    report.healed.push(key.clone());
                }
                report.problems.push(Problem::new(
                    Code::HashMismatch,
                    key,
                    match outcome {
                        Ok(()) => String::from(
                            "the drive's copy is damaged (its bytes are not this version's, or it \
                             is gone); it was sent again from this device",
                        ),
                        Err(e) => format!(
                            "the drive's copy is damaged (its bytes are not this version's, or it \
                             is gone); it was not sent again from this device ({e}) and was left \
                             as it is"
                        ),
                    },
                ));
            }
        }
    }
    // This device's copies: the ones a pass would trust by their size and time.
    let tree = beneath::Root::open(&root.path)?;
    let stamp = root::trash_stamp(crate::now());
    let state_dir = index_path.parent().unwrap_or_else(|| Path::new("."));
    let log = OpLog::new(state_dir);
    let journal = Journal::new(state_dir);
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
            Some(file) if whole.contains(key.as_str()) => restore(
                remote_side,
                &root.path,
                &tree,
                (&log, &journal),
                &stamp,
                &key,
                &hash,
                Fingerprint::of(&st),
                file,
                point,
            )
            .is_ok(),
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
            } else if there.is_some() {
                // The drive names that version, but its copy was not read back whole just now:
                // nothing is put over this device's bytes from a copy nobody checked.
                "this device's copy changed under the same size and time (a failing disk?); the \
                 drive's copy of the synced version was not read back whole, so nothing was put \
                 back"
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
/// the folder's spool, never whole in memory), and says what came of it ([`ReadBack`]).
fn read_back<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    key: &str,
    file: &RemoteFile,
) -> ReadBack {
    if file.size > BIG_BLOB {
        match TempBlob::for_download(&root.path, key, &file.hash) {
            Ok(spool) => remote_side.read_back(key, file, Some(spool.path())),
            Err(e) => ReadBack::Unread(e),
        }
    } else {
        remote_side.read_back(key, file, None)
    }
}

/// [`SyncRemote::read_back`] by the remote's own checked fetch: any failure is a read that
/// failed, never damage (a remote that cannot tell a damaged copy from a changed one or from a
/// dropped connection has no way to say it).
pub(super) fn read_back_by_fetch<R: SyncRemote + ?Sized>(
    remote_side: &R,
    key: &str,
    file: &RemoteFile,
    spool: Option<&Path>,
) -> ReadBack {
    let read = match spool {
        Some(dest) => remote_side
            .fetch_to(key, &file.hash, file.size, dest)
            .map(drop),
        None => remote_side.fetch(key, &file.hash, file.size).map(drop),
    };
    match read {
        Ok(()) => ReadBack::Whole,
        Err(e) => ReadBack::Unread(e),
    }
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
///
/// The replace is two renames where the volume makes no hard links (FAT, exFAT, some shares, FUSE),
/// and the name is empty between them: a pass that found it so would read a delete. It is written
/// down in the journal BEFORE the first rename, so that the next pass's recovery finishes it - the
/// new version takes the name - or puts the old file back (X-DELTA-SYNC2-6).
#[allow(clippy::too_many_arguments)]
fn restore<R: SyncRemote + ?Sized>(
    remote_side: &R,
    folder: &Path,
    tree: &beneath::Root,
    (log, journal): (&OpLog, &Journal),
    stamp: &str,
    key: &str,
    damaged: &str,
    seen: Fingerprint,
    file: &RemoteFile,
    point: &dyn Fn(&str),
) -> CloudResult<()> {
    // The base's key of a damaged copy: a record of this device's, walked to and never made.
    let (dir, name) = names::walk_record(tree, key)?;
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
    // Nothing moves before this is on the disk: a journal that cannot be written stops the replace.
    let ticket = journal.begin(Intent::Replace {
        key: key.to_string(),
        staged: staged.rel(),
        slot: rel.clone(),
    })?;
    point("before-replace");
    let replaced = durable::replace_keeping(&dir, name, staged, &slot, Expect::Is(seen));
    // Done once the name is held again by a file the replace left whole: it went in, or it never moved
    // anything. Any other end stays open, and the next recovery looks at the name.
    if matches!(replaced, Ok(_) | Err(PublishError::Changed)) {
        // collapse-ok: a step left open that has its new version in place is a no-op for recovery
        // (the name is held), as the journal's `done` says of itself.
        let _ = journal.done(ticket);
    }
    match replaced {
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
