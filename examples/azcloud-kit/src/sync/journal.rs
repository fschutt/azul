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
    beneath::{Dir, Kind, Root},
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
}

impl Intent {
    /// The file it is about.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Intent::Download { key, .. }
            | Intent::Conflict { key, .. }
            | Intent::Trash { key, .. } => key,
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
                fs::create_dir_all(dir)?;
            }
            let mut options = fs::OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&self.path)?;
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
        let mut out = Recovery::default();
        // Every name is reached from the folder's root without a link: recovery finishes no
        // step in a folder that a link below the synced folder points at.
        let tree = Root::open(root);
        for (_, intent) in self.open_steps() {
            match &tree {
                Ok(tree) => recover_one(tree, &intent, &mut out),
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

/// The folder of `key` and its name in it, reached from the root one folder at a time without
/// a link; `None` when a folder on the way is a link, another disk, or gone.
fn inside<'k>(tree: &Root, key: &'k str) -> Option<(Dir, &'k str)> {
    tree.parent_of(key, false).ok()
}

/// Whether `name` is a temp file azul-storage's local writes leave beside `target_name`.
#[must_use]
pub fn is_download_temp(name: &str, target_name: &str) -> bool {
    name.starts_with(&format!(".{target_name}.azul-storage-")) && name.ends_with(".tmp")
}

/// Removes the temp files of a download of `key`.
fn remove_temps(tree: &Root, key: &str, out: &mut Recovery) {
    let Some((dir, name)) = inside(tree, key) else {
        out.left.push(format!("{key}: its folder is a link or gone"));
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
        if is_file && is_download_temp(other, name) && dir.unlink(other).is_ok() {
            out.rolled_back.push(format!("{key}: removed the half download {other}"));
        }
    }
}

fn recover_one(tree: &Root, intent: &Intent, out: &mut Recovery) {
    match intent {
        Intent::Download { key, .. } => remove_temps(tree, key, out),
        Intent::Conflict { key, copy, hash } => {
            remove_temps(tree, key, out);
            let (Some((from_dir, from)), Some((to_dir, to))) =
                (inside(tree, key), inside(tree, copy))
            else {
                out.left.push(format!("{key}: its folder is a link or gone"));
                return;
            };
            if to_dir.lstat(to).is_ok() {
                return;
            }
            // Opened without following a link or blocking on a FIFO, hashed from that handle.
            let holds = from_dir
                .open_read(from)
                .ok()
                .and_then(|(file, _)| hash_file(file).ok())
                .is_some_and(|h| h == *hash);
            if holds {
                match from_dir.rename(from, &to_dir, to) {
                    Ok(()) => {
                        to_dir.sync();
                        out.finished.push(format!("{key}: kept as {copy}"));
                    }
                    Err(e) => out.left.push(format!("{key}: {e}")),
                }
            } else {
                out.left.push(format!(
                    "{key}: changed since its conflict; the next pass looks at it again"
                ));
            }
        }
        Intent::Trash { .. } => {}
        Intent::CaseRename { from, via, to } => {
            let (Some((via_dir, via_name)), Some((to_dir, to_name))) =
                (inside(tree, via), inside(tree, to))
            else {
                out.left.push(format!("{from}: its folder is a link or gone"));
                return;
            };
            if via_dir.lstat(via_name).is_ok() && to_dir.lstat(to_name).is_err() {
                match via_dir.rename(via_name, &to_dir, to_name) {
                    Ok(()) => {
                        to_dir.sync();
                        out.finished.push(format!("{from}: renamed to {to}"));
                    }
                    Err(e) => out.left.push(format!("{from}: {e}")),
                }
            }
        }
    }
}
