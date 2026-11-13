//! The journal: every multi-step change a pass makes here is written down before it starts,
//! and marked done after. A crash (kill -9, a power cut) in between leaves the step in the
//! journal, and the next pass finishes it or rolls it back before it scans, so a crash never
//! leaves a temp file behind or makes a second conflict copy of the same version.
//!
//! The steps, and what recovery does with one left open:
//!
//! | step                                  | left open                                         |
//! |---------------------------------------|---------------------------------------------------|
//! | a download into a temp file           | the temp file is removed; the old version is whole|
//! | a conflict: this side's file renamed  | the rename is finished when the file still holds  |
//! | to its copy (the copy is committed)   | the version that was committed as the copy        |
//! | a move into the trash                 | nothing: the rename happened whole or not at all  |
//! | a case-only rename through a third    | the second rename is finished                     |
//! | name                                  |                                                   |
//! | the read-back's replace of a damaged  | an empty name gets the new version (whole, beside)|
//! | copy: the old file to the trash, the  | or, when that is gone, the old file back from the |
//! | new version into the name (two renames| trash; a name that is held is left as it is       |
//! | where the volume has no hard links)   |                                                   |
//! | "Free up space": the file taken aside | the file is put back under its name (beside it,   |
//! | under a name of its own before it is  | as `name (kept)`, when a newer file holds the     |
//! | removed                               | name): the free-up is undone, never finished      |
//!
//! The journal is `journal.jsonl` in the pairing's state folder (0600), one JSON line per
//! begin and per done, each flushed to the disk before the step goes on. Recovery empties it.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, PoisonError,
    },
};

use azul_storage::{
    beneath::{Dir, Existing, Kind, Root},
    checked::hash_file,
};
use serde::{Deserialize, Serialize};

use crate::error::{CloudResult, Context};

/// The journal's file in a pairing's state folder.
pub const JOURNAL_FILE: &str = "journal.jsonl";

/// A step that takes more than one change on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Intent {
    /// The drive's version of `key` (BLAKE3 `hash`) comes down through a temp file beside it.
    Download { key: String, hash: String },
    /// This side's version of `key` (BLAKE3 `hash`) is renamed to `copy`, which the commit
    /// named already.
    Conflict {
        key: String,
        copy: String,
        hash: String,
    },
    /// `key` moves into the trash at `to` (relative to the root).
    Trash { key: String, to: String },
    /// `from` is renamed to `to` (the same name to this disk) through `via`.
    CaseRename {
        from: String,
        via: String,
        to: String,
    },
    /// The read-back puts the whole new version `staged` over `key` and the file that is there
    /// goes to `slot` (the trash). Where the volume makes no hard links that is two renames
    /// ([`azul_storage::durable::replace_keeping`]), and between them the name is EMPTY - which a
    /// pass reads as the user's delete and takes to the drive (X-DELTA-SYNC2-6). `staged` and
    /// `slot` are paths from the folder's root.
    Replace {
        key: String,
        staged: String,
        slot: String,
    },
    /// "Free up space" takes the file `key` aside under `aside` (a path from the folder's root, in
    /// the file's own folder) before it removes it. A kill between the rename and the unlink leaves
    /// the file there under a name nobody lists; recovery puts it back (X-DELTA-SYNC6-5).
    Evict { key: String, aside: String },
}

impl Intent {
    /// The file it is about.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Intent::Download { key, .. }
            | Intent::Conflict { key, .. }
            | Intent::Trash { key, .. }
            | Intent::Replace { key, .. }
            | Intent::Evict { key, .. } => key,
            Intent::CaseRename { from, .. } => from,
        }
    }
}

/// One line of the journal: a step begun (`intent`) or one done.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Line {
    id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intent: Option<Intent>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    done: bool,
}

/// A step begun, to be marked done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(pub u64);

/// What recovery did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Recovery {
    /// Steps it finished (`key: what`).
    pub finished: Vec<String>,
    /// Steps it rolled back (temp files removed).
    pub rolled_back: Vec<String>,
    /// Steps it left as they are, and why (a folder that is a link now, a file that changed).
    pub left: Vec<String>,
}

/// The journal of one pairing; shared by a pass's threads.
pub struct Journal {
    path: PathBuf,
    next: AtomicU64,
    /// Lines are written whole and in order.
    lock: Mutex<()>,
}

impl Journal {
    /// The journal in `state_dir`.
    #[must_use]
    pub fn new(state_dir: &Path) -> Journal {
        let path = state_dir.join(JOURNAL_FILE);
        let next = read_lines(&path).iter().map(|l| l.id).max().unwrap_or(0) + 1;
        Journal {
            path,
            next: AtomicU64::new(next),
            lock: Mutex::new(()),
        }
    }

    /// Writes `intent` down, on the disk, before the step starts.
    ///
    /// # Errors
    ///
    /// When it cannot be written: the step must not start then.
    pub fn begin(&self, intent: Intent) -> CloudResult<Ticket> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.append(&Line {
            id,
            intent: Some(intent),
            done: false,
        })?;
        Ok(Ticket(id))
    }

    /// The step of `ticket` is done.
    ///
    /// # Errors
    ///
    /// When it cannot be written (recovery then finds the step done on disk and leaves it).
    pub fn done(&self, ticket: Ticket) -> CloudResult<()> {
        self.append(&Line {
            id: ticket.0,
            intent: None,
            done: true,
        })
    }

    fn append(&self, line: &Line) -> CloudResult<()> {
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut text = serde_json::to_string(line).unwrap_or_default();
        text.push('\n');
        (|| {
            if let Some(dir) = self.path.parent() {
                crate::state::create_private_dir(dir)?;
            }
            let mut file = crate::state::open_private_append(&self.path)?;
            file.write_all(text.as_bytes())?;
            file.sync_data()
        })()
        .with_context(|| self.path.display())
    }

    /// The steps begun and not done, oldest first.
    #[must_use]
    pub fn open_steps(&self) -> Vec<(Ticket, Intent)> {
        let lines = read_lines(&self.path);
        let done: std::collections::BTreeSet<u64> =
            lines.iter().filter(|l| l.done).map(|l| l.id).collect();
        lines
            .into_iter()
            .filter(|l| !done.contains(&l.id))
            .filter_map(|l| l.intent.map(|i| (Ticket(l.id), i)))
            .collect()
    }

    /// Finishes or rolls back every open step in the folder `root`, then empties the journal.
    ///
    /// # Errors
    ///
    /// When the emptied journal cannot be written (the steps are done then; the next recovery
    /// finds them done on disk).
    pub fn recover(&self, root: &Path) -> CloudResult<Recovery> {
        self.recover_with(root, &|_| {})
    }

    /// [`Journal::recover`] that tells `point` the moments a test plays a window in: right before the
    /// rename that finishes a conflict copy (`recover-conflict-before-rename`) or a case rename
    /// (`recover-case-before-rename`), after the look at the name that must still be free
    /// (X-DELTA-SYNC4-22: the recoveries' renames never replace a file that took the name since that
    /// look, and until now no test could enter the window).
    ///
    /// # Errors
    ///
    /// As [`Journal::recover`].
    pub fn recover_with(&self, root: &Path, point: &dyn Fn(&str)) -> CloudResult<Recovery> {
        let mut out = Recovery::default();
        // Every name is reached from the folder's root without a link: recovery finishes no
        // step in a folder that a link below the synced folder points at.
        let tree = Root::open(root);
        for (_, intent) in self.open_steps() {
            match &tree {
                Ok(tree) => recover_one(tree, &intent, &mut out, point),
                Err(e) => out.left.push(format!(
                    "{}: the folder cannot be opened ({e})",
                    intent.key()
                )),
            }
        }
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| self.path.display()),
        }
        Ok(out)
    }
}

fn read_lines(path: &Path) -> Vec<Line> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    // A line cut off by the crash is no step: it never began on disk.
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

/// The folder of a name recovery LOOKS at, removes or moves OUT of its folder - a half download's temp files, the file a
/// conflict keeps, the staged version, the old file in the trash's slot, an aside, the file a case rename moves - and its
/// name, reached from the root one folder at a time without a link; `None` when a folder on the way is a link, another disk,
/// or gone. The key is a record this device wrote (a line of the journal): walked to, never made, and the folder is an
/// [`Existing`] one, which makes no name in itself (ARCH29 C6c; X-DELTA-STRUCT4-7). The slot is in the trash, under
/// `.azlin`: the rows of a synced name would refuse it, and it is only ever moved out of.
fn looked_at(tree: &Root, key: &str) -> Option<(Existing, String)> {
    super::names::walk_record(tree, key).ok()
}

/// The folder of a name recovery PUTS something under - the copy a conflict keeps, the name a replaced or a freed file goes
/// back to, a case rename's new name - and its name: a folder that is there already and the name passed the rows of a synced
/// name again before one is made (X-DELTA-STRUCT4-7: it took them as `OnDisk` keys and renamed into any folder that was there,
/// so a journal that names `proj/.git/hooks/post-commit` as the slot to fill got a hook). A step the engine could make passes
/// them; a record a person wrote does not, and is left (`out.left`: "its folder is a link or gone").
fn put_into(tree: &Root, key: &str) -> Option<(Dir, String)> {
    let policy = super::names::Policy::here(super::names::Volume::default());
    super::names::walk_admitted(tree, &policy, key).ok()
}

/// Whether `name` is a temp file azul-storage's local writes leave beside `target_name`.
#[must_use]
pub fn is_download_temp(name: &str, target_name: &str) -> bool {
    name.starts_with(&format!(".{target_name}.azul-storage-")) && name.ends_with(".tmp")
}

/// Removes the temp files of a download of `key`.
fn remove_temps(tree: &Root, key: &str, out: &mut Recovery) {
    let Some((dir, name)) = looked_at(tree, key) else {
        out.left
            .push(format!("{key}: its folder is a link or gone"));
        return;
    };
    let Ok(entries) = dir.names() else {
        return;
    };
    for other in entries {
        let Some(other) = other.to_str() else {
            continue;
        };
        let is_file = dir.lstat(other).is_ok_and(|st| st.kind == Kind::File);
        if is_file && is_download_temp(other, &name) && dir.unlink(other).is_ok() {
            out.rolled_back.push(format!("{key}: removed the half download {other}"));
        }
    }
}

fn recover_one(tree: &Root, intent: &Intent, out: &mut Recovery, point: &dyn Fn(&str)) {
    match intent {
        Intent::Download { key, .. } => remove_temps(tree, key, out),
        Intent::Conflict { key, copy, hash } => {
            remove_temps(tree, key, out);
            let (Some((from_dir, from)), Some((to_dir, to))) =
                (looked_at(tree, key), put_into(tree, copy))
            else {
                out.left.push(format!("{key}: its folder is a link or gone"));
                return;
            };
            if to_dir.lstat(&to).is_ok() {
                return;
            }
            // Opened without following a link or blocking on a FIFO, hashed from that handle.
            let holds = from_dir
                .open_read(&from)
                .ok()
                .and_then(|(file, _)| hash_file(file).ok())
                .is_some_and(|h| h == *hash);
            if holds {
                // Never over a file that took the copy's name since the look above (X-DELTA-SYNC4-22).
                point("recover-conflict-before-rename");
                match from_dir.rename_noreplace(&from, &to_dir, &to) {
                    Ok(()) => {
                        to_dir.sync();
                        out.finished.push(format!("{key}: kept as {copy}"));
                    }
                    Err(e) if e.is_already_exists() => {}
                    Err(e) => out.left.push(format!("{key}: {e}")),
                }
            } else {
                out.left.push(format!(
                    "{key}: changed since its conflict; the next pass looks at it again"
                ));
            }
        }
        Intent::Trash { .. } => {}
        Intent::Replace { key, staged, slot } => recover_replace(tree, key, staged, slot, out),
        Intent::Evict { key, aside } => recover_evict(tree, key, aside, out),
        Intent::CaseRename { from, via, to } => {
            let (Some((via_dir, via_name)), Some((to_dir, to_name))) =
                (looked_at(tree, via), put_into(tree, to))
            else {
                out.left.push(format!("{from}: its folder is a link or gone"));
                return;
            };
            if via_dir.lstat(&via_name).is_ok() && to_dir.lstat(&to_name).is_err() {
                // Never over a file that took the name since the look above (X-DELTA-SYNC4-22).
                point("recover-case-before-rename");
                match via_dir.rename_noreplace(&via_name, &to_dir, &to_name) {
                    Ok(()) => {
                        to_dir.sync();
                        out.finished.push(format!("{from}: renamed to {to}"));
                    }
                    Err(e) if e.is_already_exists() => {}
                    Err(e) => out.left.push(format!("{from}: {e}")),
                }
            }
        }
    }
}

/// A "Free up space" cut between its rename and its unlink (X-DELTA-SYNC6-5): the file is under the
/// aside name, the name is empty and the base says "in the cloud only". The aside is nothing a pass
/// lists - the rules leave it out - so it would never be freed, never swept, and the space the user
/// asked to free would stay used under a name the app hides. The free-up is UNDONE, not finished: the
/// file goes back under its name (a newer file that holds the name is never overwritten, the file is
/// kept beside it as `name (kept)`), where the pass reads it as present and unchanged and makes the
/// base entry whole again; the user presses "Free up space" once more. Finishing it would need the
/// base to be known here, and an unlink on a guess is the one thing this must not do.
fn recover_evict(tree: &Root, key: &str, aside: &str, out: &mut Recovery) {
    let (Some((aside_dir, aside_name)), Some((dir, name))) =
        (looked_at(tree, aside), put_into(tree, key))
    else {
        out.left
            .push(format!("{key}: its folder is a link or gone"));
        return;
    };
    match aside_dir.lstat(&aside_name) {
        Ok(st) if st.kind == Kind::File => {}
        Ok(_) => {
            out.left.push(format!(
                "{key}: {aside_name} is no file; it is left as it is"
            ));
            return;
        }
        // Not taken aside (the cut came before the rename), or removed: nothing to put back.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.left.push(format!("{key}: {e}"));
            return;
        }
    }
    match aside_dir.rename_noreplace(&aside_name, &dir, &name) {
        Ok(()) => {
            dir.sync();
            aside_dir.sync();
            out.rolled_back.push(format!(
                "{key}: the file a stopped Free up space took aside is back"
            ));
        }
        Err(e) if e.is_already_exists() => match super::keep_beside(&dir, &aside_name, &name) {
            Some(kept) => out.finished.push(format!(
                "{key}: a newer file holds the name, the file a stopped Free up space took aside \
                 is kept as {kept}"
            )),
            None => out.left.push(format!(
                "{key}: a newer file holds the name and the file a stopped Free up space took \
                 aside stays as {aside_name}"
            )),
        },
        Err(e) => out.left.push(format!("{key}: {e}")),
    }
}

/// A replace cut between its two renames (X-DELTA-SYNC2-6): the name is empty, the old file is in
/// the slot and the new version, whole and synced, is beside. The name is the thing a pass must
/// never find empty (gone here, unchanged on the drive: a delete), so the replace is finished - the
/// new version takes the name - or, when the new version is gone, rolled back: the old file comes
/// back from the slot. A name that is held is nothing to finish: the old file is still there (the
/// replace was cut before it moved anything) or the new one already is.
fn recover_replace(tree: &Root, key: &str, staged: &str, slot: &str, out: &mut Recovery) {
    let Some((dir, name)) = put_into(tree, key) else {
        out.left
            .push(format!("{key}: its folder is a link or gone"));
        return;
    };
    match dir.lstat(&name) {
        Ok(_) => return,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            out.left.push(format!("{key}: {e}"));
            return;
        }
    }
    if let Some((from_dir, from)) = looked_at(tree, staged) {
        if from_dir.lstat(&from).is_ok_and(|st| st.kind == Kind::File) {
            match from_dir.rename_noreplace(&from, &dir, &name) {
                Ok(()) => {
                    dir.sync();
                    from_dir.sync();
                    out.finished
                        .push(format!("{key}: the new version was put in its place"));
                }
                Err(e) => out.left.push(format!("{key}: {e}")),
            }
            return;
        }
    }
    if let Some((slot_dir, slot_name)) = looked_at(tree, slot) {
        if slot_dir
            .lstat(&slot_name)
            .is_ok_and(|st| st.kind == Kind::File)
        {
            match slot_dir.rename_noreplace(&slot_name, &dir, &name) {
                Ok(()) => {
                    dir.sync();
                    out.rolled_back
                        .push(format!("{key}: the old file was put back from the trash"));
                }
                Err(e) => out.left.push(format!("{key}: {e}")),
            }
            return;
        }
    }
    out.left.push(format!(
        "{key}: cut while it was replaced; neither the new version nor the old file is where the \
         journal says"
    ));
}
