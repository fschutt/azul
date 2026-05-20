//! SQLite engine for the `Db` API (SUPER_PLAN_2 §4 P4.3).
//!
//! [`Db`] is an **always-present** FFI handle (a POD `repr(C)` struct, like
//! `App`); the bundled-SQLite `rusqlite` engine sits behind the `db-sqlite`
//! feature. Without the feature the public API still compiles — `open`
//! returns an invalid handle (`is_open()` false), and `execute`/`query`
//! degrade to `0` / empty — so `Db` flows through the normal api.json
//! codegen with **no feature-gating** (the engine, not the type, is gated).
//! Engine-agnostic surface: SQL strings + `azul_core::db::{DbValue, DbRows}`.

use core::ffi::c_void;

use azul_core::db::{DbRows, DbValue, DbValueVec};
use azul_css::{AzString, StringVec};

#[cfg(feature = "db-sqlite")]
use rusqlite::{types::Value, Connection};

/// The bundled SQLite library version (only with `db-sqlite`). Internal
/// smoke-test / diagnostic; not part of the public api.json surface.
#[cfg(feature = "db-sqlite")]
pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}

/// An (optionally) open SQLite database — an FFI-safe opaque handle.
/// `ptr` is a boxed `rusqlite::Connection`, or null when closed / open
/// failed / the `db-sqlite` feature is off. Mirrors the `App` handle
/// pattern (`run_destructor` + custom `Drop` for C-ABI ownership).
#[repr(C)]
#[derive(Debug)]
pub struct Db {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for Db {
    fn clone(&self) -> Self {
        // Non-owning shallow handle copy — only the original frees the
        // connection (the FFI handle convention; the demo never clones a Db).
        Db {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for Db {
    fn default() -> Self {
        Db {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Db {
    /// Open (or create) a database at `path` (`":memory:"` for in-memory).
    /// Returns an invalid handle (`is_open()` false) on failure or when the
    /// `db-sqlite` feature is disabled.
    pub fn open(path: AzString) -> Db {
        #[cfg(feature = "db-sqlite")]
        {
            match Connection::open(path.as_str()) {
                Ok(conn) => Db {
                    ptr: Box::into_raw(Box::new(conn)) as *mut c_void,
                    run_destructor: true,
                },
                Err(_) => Db::default(),
            }
        }
        #[cfg(not(feature = "db-sqlite"))]
        {
            let _ = path;
            Db::default()
        }
    }

    /// `true` if the database is open (the `db-sqlite` engine is present
    /// and `open` succeeded).
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Run a non-query statement (INSERT/UPDATE/DELETE/CREATE …) with
    /// positional `params` (`?` placeholders). Returns rows affected (`0`
    /// on error, a closed handle, or no engine).
    pub fn execute(&self, sql: AzString, params: DbValueVec) -> usize {
        #[cfg(feature = "db-sqlite")]
        {
            engine::execute(self, sql, params)
        }
        #[cfg(not(feature = "db-sqlite"))]
        {
            let _ = (sql, params);
            0
        }
    }

    /// Run a query (SELECT) with positional `params`. Returns an empty
    /// [`DbRows`] on error, a closed handle, or no engine.
    pub fn query(&self, sql: AzString, params: DbValueVec) -> DbRows {
        #[cfg(feature = "db-sqlite")]
        {
            engine::query(self, sql, params)
        }
        #[cfg(not(feature = "db-sqlite"))]
        {
            let _ = (sql, params);
            empty_rows()
        }
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        #[cfg(feature = "db-sqlite")]
        {
            if self.run_destructor && !self.ptr.is_null() {
                // Reclaim and drop the boxed Connection (closes the db).
                drop(unsafe { Box::from_raw(self.ptr as *mut Connection) });
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

fn empty_rows() -> DbRows {
    DbRows {
        columns: StringVec::from_vec(Vec::new()),
        values: DbValueVec::from_vec(Vec::new()),
    }
}

// ───────── rusqlite engine (only with `db-sqlite`) ─────────────────────
#[cfg(feature = "db-sqlite")]
mod engine {
    use super::*;
    use azul_css::U8Vec;

    fn conn(db: &Db) -> Option<&Connection> {
        if db.ptr.is_null() {
            None
        } else {
            // Safe: `ptr` is a `Box<Connection>` owned for the lifetime of
            // `db`; the ref is bounded by `&db`.
            Some(unsafe { &*(db.ptr as *const Connection) })
        }
    }

    pub fn execute(db: &Db, sql: AzString, params: DbValueVec) -> usize {
        let conn = match conn(db) {
            Some(c) => c,
            None => return 0,
        };
        conn.execute(sql.as_str(), rusqlite::params_from_iter(to_values(&params)))
            .unwrap_or(0)
    }

    pub fn query(db: &Db, sql: AzString, params: DbValueVec) -> DbRows {
        let conn = match conn(db) {
            Some(c) => c,
            None => return empty_rows(),
        };
        let values = to_values(&params);
        let mut stmt = match conn.prepare(sql.as_str()) {
            Ok(s) => s,
            Err(_) => return empty_rows(),
        };
        let columns: Vec<AzString> = stmt
            .column_names()
            .iter()
            .map(|s| AzString::from(s.to_string()))
            .collect();
        let col_count = columns.len();
        let mut rows = match stmt.query(rusqlite::params_from_iter(values)) {
            Ok(r) => r,
            Err(_) => return empty_rows(),
        };
        let mut cells: Vec<DbValue> = Vec::new();
        while let Ok(Some(row)) = rows.next() {
            for i in 0..col_count {
                let v: Value = row.get(i).unwrap_or(Value::Null);
                cells.push(value_to_db(v));
            }
        }
        DbRows {
            columns: StringVec::from_vec(columns),
            values: DbValueVec::from_vec(cells),
        }
    }

    fn to_values(params: &DbValueVec) -> Vec<Value> {
        params.as_ref().iter().map(db_to_value).collect()
    }

    fn db_to_value(v: &DbValue) -> Value {
        match v {
            DbValue::Null => Value::Null,
            DbValue::Integer(i) => Value::Integer(*i),
            DbValue::Real(r) => Value::Real(*r),
            DbValue::Text(s) => Value::Text(s.as_str().to_string()),
            DbValue::Blob(b) => Value::Blob(b.as_ref().to_vec()),
        }
    }

    fn value_to_db(v: Value) -> DbValue {
        match v {
            Value::Null => DbValue::Null,
            Value::Integer(i) => DbValue::Integer(i),
            Value::Real(r) => DbValue::Real(r),
            Value::Text(s) => DbValue::Text(AzString::from(s)),
            Value::Blob(b) => DbValue::Blob(U8Vec::from_vec(b)),
        }
    }
}
