//! Documents with an owner (ARCH28 RF47 / AR5a-4): a JSON file that several writers change - two
//! windows, a window and a command line, the app and a background job - without one dropping
//! the other's change.
//!
//! A document read into memory at the start and written whole later takes every change another
//! writer made since with it ("File > Open new window" is a second PROCESS with its own copy). A
//! [`Doc`] is changed with [`Doc::update`]: the writers take the document's lock, read the file
//! FRESH, apply their change to what is there, and put the result in place whole
//! ([`crate::durable::replace`]: the old version or the new one survives a crash, never half). Two
//! writers on two threads, in two processes, interleave: both changes survive.
//!
//! - A file that is not the document is never turned into an empty one by the next write: a cut
//!   file ([`DocRead::Damaged`]) is renamed aside first (its bytes stay), a file of another format
//!   or a newer version ([`DocRead::Foreign`]) is not written at all.
//! - A field this build does not know, in the file's top level or inside the entries of a list,
//!   is carried through the write: a newer build's additions survive an older build's edit
//!   (AZ-CN19: they were dropped on the next save).
//! - A change that leaves the document as it was writes nothing.
//! - A window that keeps the WHOLE document in memory and saves it whole ([`Doc::save_merged`]) writes
//!   only what it changed since it last read or wrote the file: a field another process changed,
//!   an entry of a list another process added, stay (AZ-U7: a second window dropped the first one's
//!   sync pairings and a trusted contact's share).

use std::{
    collections::BTreeSet,
    fmt, fs, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::{
    durable::{self, Mode},
    lock::{LockDir, LockError},
};

/// The longest a writer waits for another writer of the same document.
pub const WAIT: Duration = Duration::from_secs(10);

/// A JSON document a [`Doc`] holds.
pub trait Document: Serialize + DeserializeOwned + Default {
    /// Whether this build may read what was read - and write it: `Err` with the sentence for a
    /// document of another format or a newer version, which is left exactly as it is.
    ///
    /// # Errors
    ///
    /// Why this build cannot take the document.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

/// What a [`Doc`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocRead<T> {
    /// The document.
    Found(T),
    /// There is no file: the document is empty so far.
    Absent,
    /// There is a file and it is not JSON at all (cut off, torn, something else): the next
    /// update renames it aside and starts a fresh document.
    Damaged(String),
    /// JSON that is not this build's document - another format, a newer version, a shape this
    /// build does not know: never written over, so a newer build's file survives an older one.
    Foreign(String),
}

/// Why a document was not changed.
#[derive(Debug)]
pub enum DocError {
    /// Another writer held the lock for longer than [`WAIT`].
    Busy(LockError),
    /// The file is not this build's document ([`DocRead::Foreign`]): nothing was written.
    Foreign(String),
    /// The file could not be read, renamed aside or written.
    Io(io::Error),
    /// The document could not be put into JSON.
    Encode(String),
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocError::Busy(e) => write!(f, "{e}"),
            DocError::Foreign(why) | DocError::Encode(why) => f.write_str(why),
            DocError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DocError {}

impl From<io::Error> for DocError {
    fn from(e: io::Error) -> Self {
        DocError::Io(e)
    }
}

/// A JSON file with an owner: a path, and the lock its writers take.
#[derive(Debug, Clone)]
pub struct Doc<T> {
    path: PathBuf,
    locks: LockDir,
    lock_name: String,
    document: PhantomData<fn() -> T>,
}

/// The file as it was read, before it is a `T`.
enum Raw<T> {
    Found(T, Value),
    Absent,
    Damaged(String),
    Foreign(String),
}

impl<T: Document> Doc<T> {
    /// The document at `path`. Its writers' lock is a file in `locks/` beside it.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Doc<T> {
        let path = path.into();
        let folder = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = path.file_name().map_or_else(
            || String::from("document"),
            |n| n.to_string_lossy().into_owned(),
        );
        Doc {
            locks: LockDir::new(folder.join("locks")),
            lock_name: format!("doc/{name}"),
            path,
            document: PhantomData,
        }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn raw(&self) -> Result<Raw<T>, io::Error> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Raw::Absent),
            Err(e) => return Err(e),
        };
        let shown = self.path.display();
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(e) => return Ok(Raw::Damaged(format!("{shown} cannot be read: {e}"))),
        };
        match serde_json::from_value::<T>(value.clone()) {
            Ok(document) => match document.check() {
                Ok(()) => Ok(Raw::Found(document, value)),
                Err(why) => Ok(Raw::Foreign(why)),
            },
            Err(e) => Ok(Raw::Foreign(format!(
                "{shown} is not a document this version reads: {e}"
            ))),
        }
    }

    /// The document as it is now, with nobody's lock: a read for showing it. Never a default for
    /// a file that is there and unreadable - [`DocRead::Damaged`] and [`DocRead::Foreign`] say
    /// so - and an error of the disk is [`DocRead::Damaged`] too.
    #[must_use]
    pub fn read(&self) -> DocRead<T> {
        match self.raw() {
            Ok(Raw::Found(document, _)) => DocRead::Found(document),
            Ok(Raw::Absent) => DocRead::Absent,
            Ok(Raw::Damaged(why)) => DocRead::Damaged(why),
            Ok(Raw::Foreign(why)) => DocRead::Foreign(why),
            Err(e) => DocRead::Damaged(format!("{}: {e}", self.path.display())),
        }
    }

    /// Changes the document: takes its lock, reads the file fresh (none: a default document),
    /// applies `change` to it, and puts the result in place whole - unless `change` left it as it
    /// was, which writes nothing. Returns the document as it is now and what `change` answered.
    /// Two writers never interleave inside this call, in this process or another.
    ///
    /// # Errors
    ///
    /// [`DocError::Busy`] when the lock is held for longer than [`WAIT`],
    /// [`DocError::Foreign`] when the file is not this build's document (nothing is written), the
    /// disk's error when the file cannot be read or written.
    pub fn update<R>(&self, change: impl FnOnce(&mut T) -> R) -> Result<(T, R), DocError> {
        let _held = self
            .locks
            .lock(&self.lock_name, WAIT)
            .map_err(DocError::Busy)?;
        let (mut document, disk) = match self.raw()? {
            Raw::Found(document, value) => (document, Some(value)),
            Raw::Absent => (T::default(), None),
            Raw::Damaged(_) => {
                self.set_aside()?;
                (T::default(), None)
            }
            Raw::Foreign(why) => return Err(DocError::Foreign(why)),
        };
        let encode =
            |e: serde_json::Error| DocError::Encode(format!("{}: {e}", self.path.display()));
        // What this build makes of the file before the change, and after it.
        let before = serde_json::to_value(&document).map_err(encode)?;
        let answer = change(&mut document);
        let after = serde_json::to_value(&document).map_err(encode)?;
        if before == after {
            return Ok((document, answer));
        }
        // What a newer build wrote that this build does not know goes through the write.
        let mut text = None;
        if let Some(disk) = &disk {
            let mut carried = after.clone();
            if carry_unknown(disk, &before, &mut carried) {
                text = Some(carried);
            }
        }
        let text = match text {
            Some(value) => serde_json::to_string_pretty(&value),
            None => serde_json::to_string_pretty(&document),
        }
        .map_err(encode)?;
        durable::replace(&self.path, format!("{text}\n").as_bytes(), Mode::Shared)?;
        Ok((document, answer))
    }

    /// Saves the document a window kept in memory: `mine` is what the window has now, `start` what
    /// it last read from the file or wrote to it. Under the lock the file is read fresh and the
    /// three are merged ([`merge_values`]): what `mine` changed since `start` is written, what
    /// another process changed in the meantime stays - a field `mine` did not touch is the
    /// file's, an entry of a list is kept or added by its key (`key_of`). Returns the merged
    /// document, which the window adopts. A file that is not this build's document is handled as
    /// by [`Doc::update`]: a cut file is set aside and `mine` written whole, a foreign one is not
    /// written.
    ///
    /// # Errors
    ///
    /// As [`Doc::update`].
    pub fn save_merged(&self, start: &T, mine: &T, key_of: KeyOf) -> Result<T, DocError> {
        let _held = self
            .locks
            .lock(&self.lock_name, WAIT)
            .map_err(DocError::Busy)?;
        let disk = match self.raw()? {
            Raw::Found(document, value) => Some((document, value)),
            Raw::Absent => None,
            Raw::Damaged(_) => {
                self.set_aside()?;
                None
            }
            Raw::Foreign(why) => return Err(DocError::Foreign(why)),
        };
        let encode =
            |e: serde_json::Error| DocError::Encode(format!("{}: {e}", self.path.display()));
        let start_value = serde_json::to_value(start).map_err(encode)?;
        let mine_value = serde_json::to_value(mine).map_err(encode)?;
        let Some((disk_document, disk_value)) = disk else {
            // Nothing there to keep: what the window has is the document.
            let text = serde_json::to_string_pretty(mine).map_err(encode)?;
            durable::replace(&self.path, format!("{text}\n").as_bytes(), Mode::Shared)?;
            return Ok(serde_json::from_value(mine_value).map_err(encode)?);
        };
        let round = serde_json::to_value(&disk_document).map_err(encode)?;
        let merged = merge_values(&start_value, &mine_value, &round, key_of);
        let document: T = serde_json::from_value(merged.clone()).map_err(encode)?;
        let mut final_value = merged.clone();
        let carried = carry_unknown(&disk_value, &round, &mut final_value);
        if merged == round && !carried {
            // The file already holds it: nothing to write.
            return Ok(document);
        }
        let text = if carried {
            serde_json::to_string_pretty(&final_value)
        } else {
            serde_json::to_string_pretty(&document)
        }
        .map_err(encode)?;
        durable::replace(&self.path, format!("{text}\n").as_bytes(), Mode::Shared)?;
        Ok(document)
    }

    /// The file that is no JSON, renamed next to itself with the time: its bytes stay.
    fn set_aside(&self) -> Result<(), io::Error> {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut aside = self.path.clone().into_os_string();
        aside.push(format!(".damaged-{at}"));
        match fs::rename(&self.path, PathBuf::from(aside)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Puts into `out` the fields of `disk` that `round` (the file as this build reads it) does not
/// have: what a newer build wrote. In a list, entries are matched by their `id`. Whether anything
/// was put in.
fn carry_unknown(disk: &Value, round: &Value, out: &mut Value) -> bool {
    match (disk, round, out) {
        (Value::Object(disk), Value::Object(round), Value::Object(out)) => {
            let mut added = false;
            for (key, disk_value) in disk {
                match (round.get(key), out.get_mut(key)) {
                    // Not this build's, and the change did not make it so: it stays.
                    (None, None) => {
                        out.insert(key.clone(), disk_value.clone());
                        added = true;
                    }
                    (Some(round_value), Some(out_value)) => {
                        added |= carry_unknown(disk_value, round_value, out_value);
                    }
                    // This build's field, changed or removed by the edit: the edit wins.
                    _ => {}
                }
            }
            added
        }
        (Value::Array(disk), Value::Array(round), Value::Array(out)) => {
            let mut added = false;
            for out_item in out.iter_mut() {
                let Some(id) = out_item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                else {
                    continue;
                };
                let same = |item: &&Value| item.get("id").and_then(Value::as_str) == Some(&id);
                if let (Some(disk_item), Some(round_item)) =
                    (disk.iter().find(same), round.iter().find(same))
                {
                    added |= carry_unknown(disk_item, round_item, out_item);
                }
            }
            added
        }
        _ => false,
    }
}

/// How the entries of a list are told apart: given the list's path (`.pinned`, `.recovery.held`)
/// and an entry, the key that names it - `None` for a list whose entries have none (then one side
/// wins as a whole).
pub type KeyOf = fn(&str, &Value) -> Option<String>;

/// The three-way merge of a document kept in memory: `start` is what the window last read or wrote,
/// `mine` what it has now, `theirs` what the file holds now. A field `mine` did not change is
/// `theirs`; one `theirs` did not change is `mine`; one both changed, in different ways, is
/// merged inside when both are objects or keyed lists, else `mine` (the change just made). In a
/// keyed list an entry added or changed by either side stays; one removed by `mine` and not
/// changed by `theirs` is gone; one `theirs` changed is kept even if `mine` removed it (a change
/// is not lost to a removal).
#[must_use]
pub fn merge_values(start: &Value, mine: &Value, theirs: &Value, key_of: KeyOf) -> Value {
    merge3("", Some(start), Some(mine), Some(theirs), key_of).unwrap_or(Value::Null)
}

fn merge3(
    path: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
    key_of: KeyOf,
) -> Option<Value> {
    if ours == base {
        return theirs.cloned();
    }
    if theirs == base || ours == theirs {
        return ours.cloned();
    }
    match (ours, theirs) {
        (Some(Value::Object(ours_map)), Some(Value::Object(theirs_map))) => {
            let base_map = base.and_then(Value::as_object);
            let keys: BTreeSet<&String> = ours_map
                .keys()
                .chain(theirs_map.keys())
                .chain(base_map.iter().flat_map(|map| map.keys()))
                .collect();
            let mut merged = serde_json::Map::new();
            for key in keys {
                let child = format!("{path}.{key}");
                if let Some(value) = merge3(
                    &child,
                    base_map.and_then(|map| map.get(key)),
                    ours_map.get(key),
                    theirs_map.get(key),
                    key_of,
                ) {
                    merged.insert(key.clone(), value);
                }
            }
            Some(Value::Object(merged))
        }
        (Some(Value::Array(ours_list)), Some(Value::Array(theirs_list))) => {
            let base_list: &[Value] = base.and_then(Value::as_array).map_or(&[], Vec::as_slice);
            merge_list(path, base_list, ours_list, theirs_list, key_of)
                .map(Value::Array)
                .or_else(|| ours.cloned())
        }
        _ => ours.cloned(),
    }
}

/// A list merged by the keys of its entries; `None` when an entry has no key.
fn merge_list(
    path: &str,
    base: &[Value],
    ours: &[Value],
    theirs: &[Value],
    key_of: KeyOf,
) -> Option<Vec<Value>> {
    let keyed = |list: &[Value]| -> Option<Vec<(String, Value)>> {
        list.iter()
            .map(|item| key_of(path, item).map(|key| (key, item.clone())))
            .collect()
    };
    let (base, ours, theirs) = (keyed(base)?, keyed(ours)?, keyed(theirs)?);
    let find = |list: &[(String, Value)], key: &str| -> Option<Value> {
        list.iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.clone())
    };
    let mut merged = Vec::new();
    // Their order first: the file's list is the one other windows read.
    for (key, theirs_item) in &theirs {
        let base_item = find(&base, key);
        match (find(&ours, key), base_item) {
            (Some(ours_item), base_item) => {
                if let Some(item) = merge3(
                    path,
                    base_item.as_ref(),
                    Some(&ours_item),
                    Some(theirs_item),
                    key_of,
                ) {
                    merged.push(item);
                }
            }
            // We had it and removed it: gone, unless they changed it meanwhile.
            (None, Some(base_item)) => {
                if *theirs_item != base_item {
                    merged.push(theirs_item.clone());
                }
            }
            // We never had it: another window added it.
            (None, None) => merged.push(theirs_item.clone()),
        }
    }
    // What only we have: added by us - or removed by them, which we changed (a change is kept).
    for (key, ours_item) in &ours {
        if find(&theirs, key).is_some() {
            continue;
        }
        match find(&base, key) {
            None => merged.push(ours_item.clone()),
            Some(base_item) if *ours_item != base_item => merged.push(ours_item.clone()),
            Some(_) => {}
        }
    }
    Some(merged)
}
