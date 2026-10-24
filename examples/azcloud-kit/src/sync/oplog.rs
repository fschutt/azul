//! The operations log of a synced folder: every destructive step a pass takes here, one JSON
//! line each, so a user (or support) can see what the sync did to a file and undo it. A move
//! into the trash, a delete, a download over an existing file, a conflict rename and a move
//! are logged; reads and uploads are not (they destroy nothing here).
//!
//! The log lives in the pairing's state folder (`ops.jsonl`, 0600: it names files). It is
//! appended to and flushed per line; when it grows past [`MAX_BYTES`] the older half is
//! dropped, so a folder that churns for years keeps a bounded log.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::error::{CloudResult, Context};

/// The log's file in a pairing's state folder.
pub const OPS_FILE: &str = "ops.jsonl";
/// Past this size the older half of the log is dropped.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// What a step did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpKind {
    /// The file moved into the folder's trash (`to` names where).
    Trash,
    /// The file was removed for good (no trash on this disk).
    Delete,
    /// A download replaced the file's bytes here.
    Overwrite,
    /// This side's version was renamed to its conflict copy (`to`).
    Conflict,
    /// The file moved to `to` (a rename the drive made).
    Move,
}

/// One line of the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    /// When (seconds since 1970, with fractions), on the pass's clock.
    pub t: f64,
    pub op: OpKind,
    /// The file's key.
    pub key: String,
    /// Where it went: the trash path, the conflict copy, the new key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// The content it had before the step (BLAKE3), so the version can be found again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// The log of one pairing. Shared by a pass's threads: lines are written whole.
pub struct OpLog {
    path: PathBuf,
    /// Serializes appends: two threads never interleave their lines.
    lock: Mutex<()>,
}

impl OpLog {
    /// The log in `state_dir`.
    #[must_use]
    pub fn new(state_dir: &Path) -> OpLog {
        OpLog {
            path: state_dir.join(OPS_FILE),
            lock: Mutex::new(()),
        }
    }

    /// Appends `op`; trims the log when it grew too big.
    ///
    /// # Errors
    ///
    /// When the line cannot be written (the step itself is done then: the caller says so).
    pub fn record(&self, op: &Op) -> CloudResult<()> {
        let _held = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut line = serde_json::to_string(op).unwrap_or_default();
        line.push('\n');
        let mut file = open_append(&self.path).with_context(|| self.path.display())?;
        file.write_all(line.as_bytes())
            .and_then(|()| file.flush())
            .with_context(|| self.path.display())?;
        if file.metadata().map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
            drop(file);
            self.trim().with_context(|| self.path.display())?;
        }
        Ok(())
    }

    /// Every line of the log, oldest first (a damaged line is skipped).
    #[must_use]
    pub fn read(&self) -> Vec<Op> {
        let Ok(file) = fs::File::open(&self.path) else {
            return Vec::new();
        };
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect()
    }

    /// The lines about `key`.
    #[must_use]
    pub fn of_key(&self, key: &str) -> Vec<Op> {
        self.read().into_iter().filter(|op| op.key == key).collect()
    }

    /// Keeps the newer half of the lines.
    fn trim(&self) -> std::io::Result<()> {
        let text = fs::read_to_string(&self.path)?;
        let lines: Vec<&str> = text.lines().collect();
        let keep = &lines[lines.len() / 2..];
        let mut out = keep.join("\n");
        out.push('\n');
        crate::state::write_atomic(&self.path, out.as_bytes(), true)
    }
}

/// Opens `path` to append, made owner-only when it is new.
fn open_append(path: &Path) -> std::io::Result<fs::File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
