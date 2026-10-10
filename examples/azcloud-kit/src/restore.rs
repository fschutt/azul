//! "Restore the drive as of Tuesday 10:00" (D42): the whole drive put back as its metadata
//! repository says it was at a time, as one new commit.
//!
//! An encrypted drive's index is a repository that keeps every state of the drive
//! (azul-storage's `meta`), and it releases no data object, so every file version within the
//! retention can come back. A restore writes the drive's files as they were at the time on top
//! of the current head: files changed or deleted since come back, files made since go. Nothing
//! in between is lost - the drive as it was just before the restore stays in the history and
//! comes back the same way.
//!
//! `.azlin` at the root - the drive's policy and the members' keys - is never restored: a
//! restore brings back no removed member and no old key.

use std::collections::BTreeMap;

use azul_storage::meta::{
    tree::walk, Bucket, Change, Commit, CommitOutcome, Conflict, MetaError, MetaRepo, Mode,
    ObjectId, Objects, Resolution, Sealer,
};

/// The root folder no restore touches (the drive's policy and keys).
const RESERVED: &str = ".azlin";

/// What a drive-wide restore did.
#[derive(Debug)]
pub struct DriveRestore {
    /// The commit that was the drive's state at the time.
    pub state: ObjectId,
    /// Files put back (changed or deleted since).
    pub restored: usize,
    /// Files taken away (made since).
    pub removed: usize,
    /// The restore's commit; `None` when the drive was as it was then already.
    pub outcome: Option<CommitOutcome>,
}

/// The drive's state at `as_of` (seconds since 1970): the newest commit of `history` (newest
/// first, as [`MetaRepo::history`] lists it) made at or before then; `None` when the drive is
/// younger.
#[must_use]
pub fn state_at(history: &[(ObjectId, Commit)], as_of: i64) -> Option<ObjectId> {
    history
        .iter()
        .find(|(_, commit)| commit.committer.time <= as_of)
        .map(|(id, _)| *id)
}

/// Puts the whole drive of `repo` back as it was at `as_of` (seconds since 1970), as one commit
/// "Restore the drive as of <time>" (`resolve` decides what another device's concurrent change
/// meets, as for every commit). `None` when the drive is younger than `as_of`.
///
/// # Errors
///
/// When the repository cannot be read or the commit cannot be published.
pub fn restore_drive_as_of<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    as_of: i64,
    resolve: &mut dyn FnMut(&Conflict) -> Resolution,
) -> Result<Option<DriveRestore>, MetaError> {
    // The newest head, and every commit of it (a lazy copy reads its packs whole).
    repo.pull()?;
    repo.fetch_all()?;
    let history = repo.history()?;
    let Some(state) = state_at(&history, as_of) else {
        return Ok(None);
    };
    let then = repo.objects().commit(&state)?.tree;
    let now = repo.root()?;
    let back = changes_back(repo.objects(), now.as_ref(), &then)?;
    let outcome = if back.changes.is_empty() {
        None
    } else {
        let message = format!("Restore the drive as of {}", crate::rfc3339(as_of));
        Some(repo.commit(&back.changes, &message, resolve)?)
    };
    Ok(Some(DriveRestore {
        state,
        restored: back.restored,
        removed: back.removed,
        outcome,
    }))
}

/// The changes that turn the drive's tree `now` (`None`: empty) into `then`.
struct Back {
    changes: Vec<Change>,
    restored: usize,
    removed: usize,
}

fn changes_back(
    objects: &Objects,
    now: Option<&ObjectId>,
    then: &ObjectId,
) -> Result<Back, MetaError> {
    let then = entries(objects, Some(then))?;
    let now = entries(objects, now)?;
    let mut changes = Vec::new();
    let mut removed = 0;
    // What is there now and was not then (or as something else): the top-most entry goes, with
    // everything in it.
    let gone: Vec<&String> = now
        .iter()
        .filter(|(path, (mode, _))| then.get(*path).is_none_or(|(was, _)| was != mode))
        .map(|(path, _)| path)
        .collect();
    for path in &gone {
        if now[*path].0 == Mode::File {
            removed += 1;
        }
        let inside_another = gone
            .iter()
            .any(|other| path.starts_with(&format!("{other}/")));
        if !inside_another {
            changes.push(Change::Delete {
                path: (*path).clone(),
            });
        }
    }
    // The folders and files of then, where they are not as they were.
    let mut puts = Vec::new();
    for (path, (mode, id)) in &then {
        if now.get(path) == Some(&(*mode, *id)) {
            continue;
        }
        match mode {
            Mode::Tree if now.get(path).is_none_or(|(is, _)| *is != Mode::Tree) => {
                changes.push(Change::Folder { path: path.clone() });
            }
            Mode::Tree => {}
            Mode::File => puts.push(Change::Put {
                path: path.clone(),
                id: *id,
            }),
        }
    }
    let restored = puts.len();
    changes.extend(puts);
    Ok(Back {
        changes,
        restored,
        removed,
    })
}

/// Every file and folder of the tree `root` (`None`: none) by path, without `.azlin`.
fn entries(
    objects: &Objects,
    root: Option<&ObjectId>,
) -> Result<BTreeMap<String, (Mode, ObjectId)>, MetaError> {
    let Some(root) = root else {
        return Ok(BTreeMap::new());
    };
    let mut all = walk(objects, root)?;
    all.retain(|path, _| path != RESERVED && !path.starts_with(&format!("{RESERVED}/")));
    Ok(all)
}
