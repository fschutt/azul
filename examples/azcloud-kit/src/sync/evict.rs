//! "Free up space" that never loses a file: a synced file's bytes leave this device only once
//! the drive holds the very version this device synced, whole. The drive's index must name
//! that version, and its copy is read back and checked against its BLAKE3 first; a copy the
//! drive lost or damaged keeps the file here (and the problem list names it), so the only good
//! copy is never the one that goes.

use std::path::Path;

use super::{
    evict_file, load_cache, named,
    problems::{Problem, ProblemList},
    synced_index, BlobIndex, LocalRoot, SyncOptions, SyncRemote, Target, TempBlob,
};
use crate::{
    error::{fail, CloudResult},
    store::BIG_BLOB,
};

/// Frees the bytes of the synced file `key` here ([`evict_file`]) once the drive behind
/// `target` holds its version whole.
///
/// # Errors
///
/// When the file is not synced (yet), the drive's index names another version or none, the
/// drive's copy is missing or damaged, or the file cannot be removed here.
pub fn evict_checked(
    target: Target<'_>,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    key: &str,
) -> CloudResult<()> {
    match target {
        Target::Index(store) => checked(&BlobIndex::new(store, &opts.prefix), root, index_path, key),
        Target::Named(drive) => checked(
            &named::NamedFiles::new(drive, &opts.prefix, index_path),
            root,
            index_path,
            key,
        ),
    }
}

fn checked<R: SyncRemote + ?Sized>(
    remote_side: &R,
    root: &LocalRoot,
    index_path: &Path,
    key: &str,
) -> CloudResult<()> {
    let index = synced_index(root, index_path)?;
    let Some(entry) = index.files.get(key) else {
        fail!("{key} is not synced yet: its only copy is on this device");
    };
    if entry.cloud_only {
        return Ok(());
    }
    let cached = load_cache(index_path, &index);
    let (drive_index, _) = remote_side.read(cached.as_ref())?;
    let Some(file) = drive_index.files.get(key) else {
        fail!("the drive does not hold {key} (any more): it stays on this device");
    };
    if file.hash != entry.hash {
        fail!(
            "the drive holds another version of {key} than the one here: it stays on this device \
             until a pass brings the two together"
        );
    }
    // Read back whole and checked against its BLAKE3; a big file through the folder's spool,
    // never whole in memory.
    if file.size > BIG_BLOB {
        let spool = TempBlob::for_download(&root.path, key, &file.hash)?;
        remote_side.fetch_to(key, &file.hash, file.size, spool.path())?;
    } else {
        remote_side.fetch(key, &file.hash, file.size)?;
    }
    evict_file(root, index_path, key)
}

/// Adds `problem` to the pairing's problem list in `state_dir` now (a check outside a pass
/// found it); the next pass publishes the list afresh.
pub fn note_problem(state_dir: &Path, mut problem: Problem) {
    let mut list = ProblemList::load(state_dir).unwrap_or_default();
    if list
        .problems
        .iter()
        .any(|p| p.code == problem.code && p.key == problem.key)
    {
        return;
    }
    let now = crate::now();
    problem.since = now;
    list.problems.push(problem);
    list.updated_at = now;
    let _ = list.save(state_dir);
}
