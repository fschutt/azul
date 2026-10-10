//! The drive index's local query cache (feature `index-cache`): the questions
//! git is bad at - find by name, the largest files, the recent ones, how much
//! the drive holds - answered from a SQLite table of every file and folder.
//!
//! - **A cache, not a source of truth:** it is rebuilt from the device's copy whenever the
//!   copy's head moved ([`QueryCache::refresh`]), and it can be deleted at any time.
//! - **The engine** is the SQLite azul's own `Db` runs on: turso (pure Rust), with a
//!   busy-polling `block_on` - turso's IO is an in-process synchronous backend, so its futures
//!   complete without a reactor (as in the dll).
//! - **Plaintext on the device:** the cache holds names, sizes and dates, like the device's
//!   own folder index; it belongs in the user's private app data (it never leaves the device).
//!   One cache is used from one thread.

use std::{
    future::Future,
    path::Path,
    pin::pin,
    task::{Context, Poll, Waker},
};

use turso::{params::Params, Builder, Connection, Database, Statement, Value};

use super::{
    bucket::Bucket, objects::Mode, pointer, repo::MetaRepo, seal::Sealer, tree::walk, MetaError,
    ObjectId, Objects,
};
use crate::DriveError;

/// Runs one of turso's futures to its end on this thread.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut context) {
            return out;
        }
        std::hint::spin_loop();
    }
}

fn db_error(e: impl std::fmt::Display) -> MetaError {
    MetaError::Drive(DriveError::Io(format!("the drive's query cache: {e}")))
}

/// One file or folder of the drive as the cache has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedEntry {
    /// From the root, `/`-separated (a folder without a trailing `/`).
    pub path: String,
    /// The folder it is in (`""`: the root).
    pub folder: String,
    pub name: String,
    pub is_folder: bool,
    /// The plaintext's bytes (0 for a folder).
    pub size: u64,
    /// Seconds since 1970, when known.
    pub modified: Option<u64>,
}

/// The rows of the drive under `root`: every file (from its pointer) and folder,
/// not the drive's own `.azlin`.
fn entries_of(objects: &Objects, root: &ObjectId) -> Result<Vec<CachedEntry>, MetaError> {
    let mut out = Vec::new();
    for (path, (mode, id)) in walk(objects, root)? {
        if path == ".azlin" || path.starts_with(".azlin/") {
            continue;
        }
        let (folder, name) = match path.rsplit_once('/') {
            Some((folder, name)) => (folder.to_string(), name.to_string()),
            None => (String::new(), path.clone()),
        };
        let entry = match mode {
            Mode::Tree => CachedEntry {
                path,
                folder,
                name,
                is_folder: true,
                size: 0,
                modified: None,
            },
            Mode::File => {
                let pointer = pointer::decode(objects.blob(&id)?).map_err(|reason| {
                    MetaError::Corrupt {
                        key: path.clone(),
                        reason,
                    }
                })?;
                CachedEntry {
                    path,
                    folder,
                    name,
                    is_folder: false,
                    size: pointer.size,
                    modified: pointer.modified,
                }
            }
        };
        out.push(entry);
    }
    Ok(out)
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(n)) => Some(*n),
        _ => None,
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// The columns every query selects, in this order.
const COLUMNS: &str = "path, folder, name, is_folder, size, modified";

fn entry_of(row: &[Value]) -> CachedEntry {
    CachedEntry {
        path: text(row.first()),
        folder: text(row.get(1)),
        name: text(row.get(2)),
        is_folder: integer(row.get(3)) == Some(1),
        size: integer(row.get(4)).map_or(0, |n| u64::try_from(n).unwrap_or(0)),
        modified: integer(row.get(5)).and_then(|n| u64::try_from(n).ok()),
    }
}

/// The local query cache of one drive.
pub struct QueryCache {
    _database: Database,
    connection: Connection,
}

impl QueryCache {
    /// Opens (or makes) the cache in the file `path`.
    pub fn open(path: &Path) -> Result<QueryCache, MetaError> {
        let name = path
            .to_str()
            .ok_or_else(|| db_error("the cache's path is not UTF-8"))?;
        let database = block_on(Builder::new_local(name).build()).map_err(db_error)?;
        let connection = database.connect().map_err(db_error)?;
        let cache = QueryCache {
            _database: database,
            connection,
        };
        cache.execute(
            "CREATE TABLE IF NOT EXISTS entries (path TEXT NOT NULL, folder TEXT NOT NULL, \
             name TEXT NOT NULL, lname TEXT NOT NULL, is_folder INTEGER NOT NULL, \
             size INTEGER NOT NULL, modified INTEGER)",
            Vec::new(),
        )?;
        cache.execute(
            "CREATE TABLE IF NOT EXISTS cache_meta (key TEXT PRIMARY KEY, value TEXT)",
            Vec::new(),
        )?;
        Ok(cache)
    }

    fn execute(&self, sql: &str, params: Vec<Value>) -> Result<u64, MetaError> {
        block_on(self.connection.execute(sql, Params::Positional(params))).map_err(db_error)
    }

    fn prepare(&self, sql: &str) -> Result<Statement, MetaError> {
        block_on(self.connection.prepare(sql)).map_err(db_error)
    }

    fn rows(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Vec<Value>>, MetaError> {
        let mut rows =
            block_on(self.connection.query(sql, Params::Positional(params))).map_err(db_error)?;
        let mut out = Vec::new();
        while let Some(row) = block_on(rows.next()).map_err(db_error)? {
            let cells = (0..row.column_count())
                .map(|i| row.get_value(i).unwrap_or(Value::Null))
                .collect();
            out.push(cells);
        }
        Ok(out)
    }

    fn entries(&self, sql: &str, params: Vec<Value>) -> Result<Vec<CachedEntry>, MetaError> {
        Ok(self.rows(sql, params)?.iter().map(|row| entry_of(row)).collect())
    }

    /// The head commit the cache was last built from.
    pub fn built_from(&self) -> Result<Option<String>, MetaError> {
        let rows = self.rows(
            "SELECT value FROM cache_meta WHERE key = ?",
            vec![Value::Text("head".to_string())],
        )?;
        Ok(rows.first().map(|row| text(row.first())).filter(|t| !t.is_empty()))
    }

    /// Rebuilds the cache from the device's copy when the copy's head moved
    /// since (a lazy copy reads every pack first). Whether it was rebuilt.
    pub fn refresh<B: Bucket, S: Sealer>(&self, repo: &mut MetaRepo<B, S>) -> Result<bool, MetaError> {
        let head = repo.head().map(|h| h.to_hex());
        if self.built_from()? == head {
            return Ok(false);
        }
        repo.fetch_all()?;
        let entries = match repo.root()? {
            Some(root) => entries_of(repo.objects(), &root)?,
            None => Vec::new(),
        };
        self.execute("BEGIN", Vec::new())?;
        match self.replace_all(&entries, head.as_deref()) {
            Ok(()) => {
                self.execute("COMMIT", Vec::new())?;
                Ok(true)
            }
            Err(e) => {
                let _ = self.execute("ROLLBACK", Vec::new());
                Err(e)
            }
        }
    }

    fn replace_all(&self, entries: &[CachedEntry], head: Option<&str>) -> Result<(), MetaError> {
        self.execute("DELETE FROM entries", Vec::new())?;
        let mut insert = self.prepare(
            "INSERT INTO entries (path, folder, name, lname, is_folder, size, modified) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )?;
        for entry in entries {
            let params = vec![
                Value::Text(entry.path.clone()),
                Value::Text(entry.folder.clone()),
                Value::Text(entry.name.clone()),
                Value::Text(entry.name.to_lowercase()),
                Value::Integer(i64::from(entry.is_folder)),
                Value::Integer(to_i64(entry.size)),
                entry.modified.map_or(Value::Null, |m| Value::Integer(to_i64(m))),
            ];
            block_on(insert.execute(Params::Positional(params))).map_err(db_error)?;
        }
        self.execute("DELETE FROM cache_meta WHERE key = ?", vec![Value::Text("head".to_string())])?;
        self.execute(
            "INSERT INTO cache_meta (key, value) VALUES (?, ?)",
            vec![
                Value::Text("head".to_string()),
                Value::Text(head.unwrap_or_default().to_string()),
            ],
        )?;
        Ok(())
    }

    /// Files and folders whose name holds `text` (letters compared without case),
    /// by path, `limit` at most.
    pub fn search(&self, text: &str, limit: usize) -> Result<Vec<CachedEntry>, MetaError> {
        self.entries(
            &format!(
                "SELECT {COLUMNS} FROM entries WHERE instr(lname, ?) > 0 ORDER BY path LIMIT ?"
            ),
            vec![
                Value::Text(text.to_lowercase()),
                Value::Integer(to_i64(limit as u64)),
            ],
        )
    }

    /// The largest files, `limit` at most.
    pub fn largest(&self, limit: usize) -> Result<Vec<CachedEntry>, MetaError> {
        self.entries(
            &format!(
                "SELECT {COLUMNS} FROM entries WHERE is_folder = 0 ORDER BY size DESC, path LIMIT ?"
            ),
            vec![Value::Integer(to_i64(limit as u64))],
        )
    }

    /// The files changed last, `limit` at most.
    pub fn recent(&self, limit: usize) -> Result<Vec<CachedEntry>, MetaError> {
        self.entries(
            &format!(
                "SELECT {COLUMNS} FROM entries WHERE is_folder = 0 AND modified IS NOT NULL \
                 ORDER BY modified DESC, path LIMIT ?"
            ),
            vec![Value::Integer(to_i64(limit as u64))],
        )
    }

    /// How many files the drive holds and their plaintext bytes: their size before
    /// compression (the quota counts the stored bytes of their objects instead - what
    /// arrives at the storage nodes, compressed and encrypted).
    pub fn totals(&self) -> Result<(u64, u64), MetaError> {
        let rows = self.rows(
            "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM entries WHERE is_folder = 0",
            Vec::new(),
        )?;
        let row = rows.first().cloned().unwrap_or_default();
        let count = integer(row.first()).map_or(0, |n| u64::try_from(n).unwrap_or(0));
        let bytes = integer(row.get(1)).map_or(0, |n| u64::try_from(n).unwrap_or(0));
        Ok((count, bytes))
    }
}
