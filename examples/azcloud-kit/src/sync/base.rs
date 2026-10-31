//! The base as a state database (feature `state-db`): what both sides agreed on at a folder's
//! last sync, kept in SQLite in the device's state folder, never inside the folder itself.
//!
//! - **What it holds:** one row per file (its key, its content hash and the whole base entry:
//!   size, times, inode, device, executable bit - whatever [`BaseEntry`] carries), and the
//!   pairing's own record (the root, the bucket and prefix, the generation last seen, the
//!   restore epoch, the burst guard's memory): a [`LocalIndex`] split into rows.
//! - **Why a database:** a save writes the rows that changed in one transaction, so a crash
//!   leaves the old base or the new one, and a folder of 20,000 files does not rewrite a
//!   20,000-entry JSON file after every edit.
//! - **Private:** the state folder is 0700 and the database and its journal files 0600 (they
//!   name every file of the folder). A database path inside the synced folder is refused: the
//!   base would sync and every device would read another's.
//! - **Upgrades:** a base written as JSON by an older version (`index.json` beside the
//!   database) is imported once, then kept as `index.json.migrated`.
//!
//! [`LocalIndex::load`] and [`LocalIndex::save`] use it for a path ending in `.db`. The engine
//! is turso (pure Rust SQLite, the engine azul's own `Db` runs on) with a busy-polling
//! `block_on`: its IO is synchronous in-process, so the futures complete without a reactor.

use std::{
    collections::BTreeMap,
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::pin,
    task::{Context, Poll, Waker},
};

use turso::{params::Params, Builder, Connection, Database, Value};

use super::local::{BaseEntry, LocalIndex};
use crate::error::{fail, CloudError, CloudResult};

/// The format the database's meta row names.
pub const BASE_FORMAT: &str = "azcloud.sync-base 1";

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

fn db_error(path: &Path, e: impl std::fmt::Display) -> CloudError {
    CloudError::Io(format!("the sync's base {}: {e}", path.display()))
}

/// Whether `path` names a base database (the `.db` paths [`LocalIndex`] keeps here).
#[must_use]
pub fn is_database(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "db")
}

/// The JSON base an older version kept where `db` is now (`index.db` -> `index.json`).
#[must_use]
pub fn legacy_json(db: &Path) -> PathBuf {
    db.with_extension("json")
}

/// One base database, open.
pub struct BaseDb {
    path: PathBuf,
    _database: Database,
    connection: Connection,
}

impl BaseDb {
    /// Opens (or makes) the base at `path`. `root`, when given, is the synced folder: a path
    /// inside it is refused.
    ///
    /// # Errors
    ///
    /// When the path lies in the synced folder, is not UTF-8, or the database cannot be opened.
    pub fn open(path: &Path, root: Option<&Path>) -> CloudResult<BaseDb> {
        if let Some(root) = root {
            let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
            if abs(path).starts_with(abs(root)) {
                fail!(
                    "the sync's base {} lies inside the folder it syncs: it must stay in the \
                     device's state folder",
                    path.display()
                );
            }
        }
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            make_private_dir(dir).map_err(|e| db_error(path, e))?;
        }
        let name = path
            .to_str()
            .ok_or_else(|| db_error(path, "the path is not UTF-8"))?;
        let database =
            block_on(Builder::new_local(name).build()).map_err(|e| db_error(path, e))?;
        let connection = database.connect().map_err(|e| db_error(path, e))?;
        let db = BaseDb {
            path: path.to_path_buf(),
            _database: database,
            connection,
        };
        db.execute(
            "CREATE TABLE IF NOT EXISTS files (key TEXT PRIMARY KEY, hash TEXT NOT NULL, \
             entry TEXT NOT NULL)",
            Vec::new(),
        )?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            Vec::new(),
        )?;
        db.make_files_private();
        Ok(db)
    }

    fn execute(&self, sql: &str, params: Vec<Value>) -> CloudResult<u64> {
        block_on(self.connection.execute(sql, Params::Positional(params)))
            .map_err(|e| db_error(&self.path, e))
    }

    fn rows(&self, sql: &str, params: Vec<Value>) -> CloudResult<Vec<Vec<Value>>> {
        let mut rows = block_on(self.connection.query(sql, Params::Positional(params)))
            .map_err(|e| db_error(&self.path, e))?;
        let mut out = Vec::new();
        while let Some(row) = block_on(rows.next()).map_err(|e| db_error(&self.path, e))? {
            out.push(
                (0..row.column_count())
                    .map(|i| row.get_value(i).unwrap_or(Value::Null))
                    .collect(),
            );
        }
        Ok(out)
    }

    fn meta(&self, key: &str) -> CloudResult<Option<String>> {
        let rows = self.rows(
            "SELECT value FROM meta WHERE key = ?",
            vec![Value::Text(key.to_string())],
        )?;
        Ok(rows.into_iter().next().and_then(|row| match row.into_iter().next() {
            Some(Value::Text(text)) => Some(text),
            _ => None,
        }))
    }

    fn set_meta(&self, key: &str, value: &str) -> CloudResult<()> {
        self.execute("DELETE FROM meta WHERE key = ?", vec![Value::Text(key.to_string())])?;
        self.execute(
            "INSERT INTO meta (key, value) VALUES (?, ?)",
            vec![Value::Text(key.to_string()), Value::Text(value.to_string())],
        )?;
        Ok(())
    }

    /// Every file row: key -> the entry's JSON.
    fn file_rows(&self) -> CloudResult<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        for row in self.rows("SELECT key, entry FROM files", Vec::new())? {
            let mut cells = row.into_iter();
            if let (Some(Value::Text(key)), Some(Value::Text(entry))) = (cells.next(), cells.next())
            {
                out.insert(key, entry);
            }
        }
        Ok(out)
    }

    /// The base, `None` when the folder was never synced.
    ///
    /// # Errors
    ///
    /// When the database cannot be read, or holds no base of this format (a damaged base is
    /// an error, never an empty one: an empty base would take every file for new).
    pub fn load(&self) -> CloudResult<Option<LocalIndex>> {
        let Some(format) = self.meta("format")? else {
            return Ok(None);
        };
        if format != BASE_FORMAT {
            fail!("{} holds a base of format {format:?}", self.path.display());
        }
        let Some(record) = self.meta("pairing")? else {
            fail!("{} holds files but no pairing record", self.path.display());
        };
        let mut index: LocalIndex = serde_json::from_str(&record)
            .map_err(|e| db_error(&self.path, format!("the pairing record: {e}")))?;
        for (key, entry) in self.file_rows()? {
            let entry: BaseEntry = serde_json::from_str(&entry)
                .map_err(|e| db_error(&self.path, format!("the entry of {key:?}: {e}")))?;
            index.files.insert(key, entry);
        }
        // The pairing record names the base's format as the JSON base does: a newer one is
        // refused, an older one brought up to this version's.
        index.upgraded(&self.path).map(Some)
    }

    /// Makes `index` the base: the rows that changed, the pairing record, in one transaction.
    ///
    /// # Errors
    ///
    /// When it cannot be written (the old base stays whole then).
    pub fn save(&self, index: &LocalIndex) -> CloudResult<()> {
        let before = self.file_rows()?;
        let mut record = index.clone();
        record.files = BTreeMap::new();
        let record = serde_json::to_string(&record).map_err(|e| db_error(&self.path, e))?;
        self.execute("BEGIN", Vec::new())?;
        let written = (|| {
            for key in before.keys().filter(|k| !index.files.contains_key(*k)) {
                self.execute("DELETE FROM files WHERE key = ?", vec![Value::Text(key.clone())])?;
            }
            for (key, entry) in &index.files {
                let json = serde_json::to_string(entry).map_err(|e| db_error(&self.path, e))?;
                if before.get(key) == Some(&json) {
                    continue;
                }
                if before.contains_key(key) {
                    self.execute(
                        "DELETE FROM files WHERE key = ?",
                        vec![Value::Text(key.clone())],
                    )?;
                }
                self.execute(
                    "INSERT INTO files (key, hash, entry) VALUES (?, ?, ?)",
                    vec![
                        Value::Text(key.clone()),
                        Value::Text(entry.hash.clone()),
                        Value::Text(json),
                    ],
                )?;
            }
            self.set_meta("pairing", &record)?;
            self.set_meta("format", BASE_FORMAT)
        })();
        match written {
            Ok(()) => {
                self.execute("COMMIT", Vec::new())?;
                self.make_files_private();
                Ok(())
            }
            Err(e) => {
                let _ = self.execute("ROLLBACK", Vec::new());
                Err(e)
            }
        }
    }

    /// The database's own side files (its write-ahead log) owner-only too.
    fn make_files_private(&self) {
        for name in files_of(&self.path) {
            let _ = set_private(&name);
        }
    }
}

/// The database at `path` and its side files (the write-ahead log and its index, a rollback
/// journal), whether they exist or not.
#[must_use]
pub fn files_of(path: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm", "-journal"]
        .into_iter()
        .map(|suffix| {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            PathBuf::from(name)
        })
        .collect()
}

/// Reads the base at the database `path`, importing an older JSON base beside it once (kept
/// as `.migrated`).
///
/// # Errors
///
/// When the database or the JSON base cannot be read.
pub fn load(path: &Path) -> CloudResult<Option<LocalIndex>> {
    let db = BaseDb::open(path, None)?;
    if let Some(index) = db.load()? {
        return Ok(Some(index));
    }
    let json = legacy_json(path);
    let Some(index) = crate::state::read_json::<LocalIndex>(&json)? else {
        return Ok(None);
    };
    let index = index.upgraded(&json)?;
    db.save(&index)?;
    let mut done = json.as_os_str().to_os_string();
    done.push(".migrated");
    fs::rename(&json, &done).map_err(|e| db_error(path, e))?;
    Ok(Some(index))
}

/// Writes `index` as the base at the database `path`.
///
/// # Errors
///
/// When it cannot be written.
pub fn save(path: &Path, index: &LocalIndex) -> CloudResult<()> {
    let root = PathBuf::from(&index.root);
    let root = (!index.root.is_empty()).then_some(root.as_path());
    BaseDb::open(path, root)?.save(index)
}

fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    set_private_dir(dir)
}

#[cfg(unix)]
fn set_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(dir)?.permissions().mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(mode & 0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if !path.exists() {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
