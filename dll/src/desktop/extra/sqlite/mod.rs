//! SQLite engine backend for the `Db` API (SUPER_PLAN_2 §4 P4.3), behind
//! the `db-sqlite` feature.
//!
//! Bundled SQLite via `rusqlite` (statically compiled from C source, no
//! system libsqlite) — approach A. The public surface stays
//! engine-agnostic: SQL strings + `azul_core::db::{DbValue, DbRows}`.
//! [`Db`] is an opaque handle (a boxed `rusqlite::Connection`) that lives
//! here in the dll — like `App` — because it carries an engine resource;
//! the api.json layer (a later tick) exposes `open` / `execute` / `query`
//! against it.

use core::ffi::c_void;

use azul_core::db::{DbRows, DbValue, DbValueVec};
use azul_css::{AzString, StringVec, U8Vec};
use rusqlite::{types::Value, Connection};

/// The bundled SQLite library version (e.g. `"3.50.0"`). Smoke-tests that
/// `rusqlite`'s bundled engine linked; handy for diagnostics. Not part of
/// the public api.json surface.
pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}

/// An open SQLite database — an opaque, FFI-safe handle (`repr(C)`, single
/// pointer) wrapping a boxed [`rusqlite::Connection`]. `ptr` is null when
/// [`Db::open`] failed; methods then degrade safely (`execute` → 0,
/// `query` → empty). The boxed connection is freed on `Drop` (the api.json
/// destructor maps to it).
#[repr(C)]
pub struct Db {
    ptr: *mut c_void,
}

impl Db {
    /// Open (or create) a database at `path`. Use `":memory:"` for an
    /// in-memory database. On failure returns a handle whose `is_open()`
    /// is false rather than erroring across the FFI.
    pub fn open(path: AzString) -> Db {
        match Connection::open(path.as_str()) {
            Ok(conn) => Db {
                ptr: Box::into_raw(Box::new(conn)) as *mut c_void,
            },
            Err(_) => Db {
                ptr: core::ptr::null_mut(),
            },
        }
    }

    /// `true` if the database opened successfully.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    fn conn(&self) -> Option<&Connection> {
        if self.ptr.is_null() {
            None
        } else {
            // Safe: `ptr` is a `Box<Connection>` we own for the lifetime of
            // `self`; the returned ref is bounded by `&self`.
            Some(unsafe { &*(self.ptr as *const Connection) })
        }
    }

    /// Run a non-query statement (INSERT / UPDATE / DELETE / CREATE …) with
    /// positional `params` (`?` placeholders). Returns the number of rows
    /// affected (`0` on error or a closed handle).
    pub fn execute(&self, sql: AzString, params: DbValueVec) -> usize {
        let conn = match self.conn() {
            Some(c) => c,
            None => return 0,
        };
        let values = to_values(&params);
        conn.execute(sql.as_str(), rusqlite::params_from_iter(values))
            .unwrap_or(0)
    }

    /// Run a query (SELECT) with positional `params` and collect the result
    /// grid. Returns an empty [`DbRows`] on error or a closed handle.
    pub fn query(&self, sql: AzString, params: DbValueVec) -> DbRows {
        let conn = match self.conn() {
            Some(c) => c,
            None => return empty_rows(),
        };
        let values = to_values(&params);

        let mut stmt = match conn.prepare(sql.as_str()) {
            Ok(s) => s,
            Err(_) => return empty_rows(),
        };
        // Column names must be snapshotted before the mutable `query` borrow.
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
}

impl Drop for Db {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // Reclaim and drop the boxed Connection (closes the database).
            drop(unsafe { Box::from_raw(self.ptr as *mut Connection) });
            self.ptr = core::ptr::null_mut();
        }
    }
}

fn empty_rows() -> DbRows {
    DbRows {
        columns: StringVec::from_vec(Vec::new()),
        values: DbValueVec::from_vec(Vec::new()),
    }
}

/// `DbValue` → rusqlite (for binding params). `rusqlite::types::Value` is a
/// 1:1 match for `DbValue`.
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

/// rusqlite → `DbValue` (for result cells).
fn value_to_db(v: Value) -> DbValue {
    match v {
        Value::Null => DbValue::Null,
        Value::Integer(i) => DbValue::Integer(i),
        Value::Real(r) => DbValue::Real(r),
        Value::Text(s) => DbValue::Text(AzString::from(s)),
        Value::Blob(b) => DbValue::Blob(U8Vec::from_vec(b)),
    }
}
