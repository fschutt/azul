//! SQLite engine for the `Db` API (SUPER_PLAN_2 §4 P4.3).
//!
//! [`Db`] is an **always-present** FFI handle (a POD `repr(C)` struct, like
//! `App`); the bundled-SQLite engine sits behind the `db-sqlite` feature.
//! The engine is [`turso`](https://crates.io/crates/turso) — a pure-Rust,
//! SQLite-compatible database (formerly "Limbo") with **no C dependency**,
//! so it cross-compiles to mobile targets without a C toolchain. Without
//! the feature the public API still compiles — `open` returns an invalid
//! handle (`is_open()` false), and `execute`/`query` degrade to `0` /
//! empty — so `Db` flows through the normal api.json codegen with **no
//! feature-gating** (the engine, not the type, is gated). Engine-agnostic
//! surface: SQL strings + `azul_core::db::{DbValue, DbRows}`.
//!
//! turso's API is **async** (every query/execute returns a `Future`). The
//! `Db` surface is sync, so each engine call blocks on the future with a
//! minimal in-crate executor (see [`engine::block_on`]) — no extra runtime
//! dependency needed (turso uses an in-process synchronous IO backend, so
//! the futures resolve without ever yielding to a reactor).

use core::ffi::c_void;

use azul_core::db::{DbRows, DbValue, DbValueVec};
use azul_css::{AzString, StringVec};

#[cfg(feature = "db-sqlite")]
use self::engine::Handle;

/// The bundled SQLite-engine version (only with `db-sqlite`). Internal
/// smoke-test / diagnostic; not part of the public api.json surface.
#[cfg(feature = "db-sqlite")]
pub fn sqlite_version() -> &'static str {
    // turso does not expose a runtime version string; report the crate
    // semver it was built against.
    "turso 0.7"
}

/// An (optionally) open SQLite database — an FFI-safe opaque handle.
/// `ptr` is a boxed engine [`Handle`] (a turso `Database` + `Connection`),
/// or null when closed / open failed / the `db-sqlite` feature is off.
/// Mirrors the `App` handle pattern (`run_destructor` + custom `Drop` for
/// C-ABI ownership).
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

/// Say once per process that the `db-sqlite` feature is compiled out. The
/// stub handle otherwise degrades silently: `open` hands back an invalid
/// handle, `execute` returns 0 rows and `query` returns no rows — all
/// plausible answers from a real empty database.
#[cfg(not(feature = "db-sqlite"))]
fn announce_db_stub(what: &str) {
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][sqlite] {what} called, but this build has no `db-sqlite` \
             feature — every Db operation is a no-op (invalid handle, 0 rows). \
             Rebuild with: cargo build -p azul-dll --features build-dll,db-sqlite"
        );
    });
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
            match engine::open(path.as_str()) {
                Some(handle) => Db {
                    ptr: Box::into_raw(Box::new(handle)) as *mut c_void,
                    run_destructor: true,
                },
                None => Db::default(),
            }
        }
        #[cfg(not(feature = "db-sqlite"))]
        {
            announce_db_stub("Db::open");
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
            announce_db_stub("Db::execute");
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
            announce_db_stub("Db::query");
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
                // Reclaim and drop the boxed handle (closes the db).
                drop(unsafe { Box::from_raw(self.ptr as *mut Handle) });
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

// ───────── turso engine (only with `db-sqlite`) ────────────────────────
#[cfg(feature = "db-sqlite")]
mod engine {
    use core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    };

    use azul_css::U8Vec;
    use turso::{params::Params, Builder, Connection, Database, Value};

    use super::*;

    /// The boxed engine state behind a live `Db.ptr`. We keep the
    /// `Database` alive alongside the `Connection` so the on-disk / in-mem
    /// store outlives every query.
    pub struct Handle {
        // `Database` is retained to keep the backing store open; the
        // `Connection` is what we actually issue statements against.
        #[allow(dead_code)]
        db: Database,
        conn: Connection,
    }

    /// Minimal `block_on` for turso's futures. turso runs its IO on an
    /// in-process synchronous backend (`MemoryIO` / `PlatformIO`), so its
    /// futures complete without yielding to any reactor — a no-op waker
    /// busy-poll therefore terminates. Avoids pulling in a full async
    /// runtime (tokio) or requiring `pollster` to be feature-enabled.
    pub fn block_on<F: Future>(mut fut: F) -> F::Output {
        // SAFETY: `fut` lives on this stack frame for the whole loop and is
        // never moved after being pinned.
        let mut fut = unsafe { Pin::new_unchecked(&mut fut) };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
                return out;
            }
            // Pending only happens on IO; turso's sync backend resolves it
            // on the next poll, so we just spin (no thread parking needed).
            core::hint::spin_loop();
        }
    }

    fn noop_waker() -> Waker {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RAW, |_| {}, |_| {}, |_| {});
        const RAW: RawWaker = RawWaker::new(core::ptr::null(), &VTABLE);
        // SAFETY: the vtable's clone returns RAW and wake/drop are no-ops,
        // satisfying the Waker contract for a stateless waker.
        unsafe { Waker::from_raw(RAW) }
    }

    /// Open (or create) the database at `path` (`":memory:"` for in-mem).
    pub fn open(path: &str) -> Option<Handle> {
        let db = block_on(Builder::new_local(path).build()).ok()?;
        let conn = db.connect().ok()?;
        Some(Handle { db, conn })
    }

    fn handle(db: &Db) -> Option<&Handle> {
        if db.ptr.is_null() {
            None
        } else {
            // Safe: `ptr` is a `Box<Handle>` owned for the lifetime of
            // `db`; the ref is bounded by `&db`.
            Some(unsafe { &*(db.ptr as *const Handle) })
        }
    }

    pub fn execute(db: &Db, sql: AzString, params: DbValueVec) -> usize {
        let handle = match handle(db) {
            Some(h) => h,
            None => return 0,
        };
        let p = to_params(&params);
        match block_on(handle.conn.execute(sql.as_str(), p)) {
            Ok(n) => n as usize,
            Err(_) => 0,
        }
    }

    pub fn query(db: &Db, sql: AzString, params: DbValueVec) -> DbRows {
        let handle = match handle(db) {
            Some(h) => h,
            None => return empty_rows(),
        };

        // Prepare so we can read the column names; `query` then runs against
        // the same prepared statement.
        let mut stmt = match block_on(handle.conn.prepare(sql.as_str())) {
            Ok(s) => s,
            Err(_) => return empty_rows(),
        };
        let columns: Vec<AzString> = stmt
            .columns()
            .iter()
            .map(|c| AzString::from(c.name().to_string()))
            .collect();
        let col_count = columns.len();

        let mut rows = match block_on(stmt.query(to_params(&params))) {
            Ok(r) => r,
            Err(_) => return empty_rows(),
        };

        let mut cells: Vec<DbValue> = Vec::new();
        // turso reports its column count via the row itself; fall back to the
        // prepared-statement count if a row is shorter/longer.
        while let Ok(Some(row)) = block_on(rows.next()) {
            let n = if col_count == 0 {
                row.column_count()
            } else {
                col_count
            };
            for i in 0..n {
                let v = row.get_value(i).unwrap_or(Value::Null);
                cells.push(value_to_db(v));
            }
        }

        DbRows {
            columns: StringVec::from_vec(columns),
            values: DbValueVec::from_vec(cells),
        }
    }

    fn to_params(params: &DbValueVec) -> Params {
        Params::Positional(params.as_ref().iter().map(db_to_value).collect())
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

/// The turso engine behind the `Db` API, end to end — the pin that lets the
/// engine crate move (0.1 fork -> registry 0.7, 2026-08-31) with proof that
/// open / execute / prepare+query / row decoding still agree with the
/// in-crate `block_on` executor.
#[cfg(all(test, feature = "db-sqlite"))]
mod tests {
    use super::*;

    fn temp_db_path(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "azul-sqlite-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("t.db").to_string_lossy().into_owned()
    }

    #[test]
    fn open_execute_and_query_roundtrip_through_the_engine() {
        let path = temp_db_path("roundtrip");
        let db = Db::open(AzString::from(path.clone()));
        assert!(db.is_open(), "a fresh file-backed database opens");

        let n = db.execute(
            AzString::from(
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL, blob BLOB)",
            ),
            DbValueVec::from_vec(Vec::new()),
        );
        assert_eq!(n, 0, "DDL changes no rows");

        let inserted = db.execute(
            AzString::from("INSERT INTO t (id, name, score, blob) VALUES (?, ?, ?, ?)"),
            DbValueVec::from_vec(vec![
                DbValue::Integer(7),
                DbValue::Text(AzString::from("seven")),
                DbValue::Real(0.5),
                DbValue::Blob(azul_css::U8Vec::from_vec(vec![1u8, 2, 3])),
            ]),
        );
        assert_eq!(inserted, 1, "one row inserted");
        let inserted = db.execute(
            AzString::from("INSERT INTO t (id, name) VALUES (?, ?)"),
            DbValueVec::from_vec(vec![
                DbValue::Integer(8),
                DbValue::Text(AzString::from("eight")),
            ]),
        );
        assert_eq!(inserted, 1);

        let rows = db.query(
            AzString::from("SELECT id, name, score, blob FROM t WHERE id >= ? ORDER BY id"),
            DbValueVec::from_vec(vec![DbValue::Integer(7)]),
        );
        let columns: Vec<&str> = rows.columns.as_ref().iter().map(|c| c.as_str()).collect();
        assert_eq!(columns, ["id", "name", "score", "blob"]);
        let values = rows.values.as_ref();
        assert_eq!(values.len(), 8, "two rows x four columns, got {values:?}");
        assert!(matches!(values[0], DbValue::Integer(7)));
        assert!(matches!(&values[1], DbValue::Text(s) if s.as_str() == "seven"));
        assert!(matches!(values[2], DbValue::Real(r) if (r - 0.5).abs() < f64::EPSILON));
        assert!(matches!(&values[3], DbValue::Blob(b) if b.as_ref() == [1u8, 2, 3]));
        assert!(matches!(values[4], DbValue::Integer(8)));
        assert!(
            matches!(values[6], DbValue::Null),
            "unset REAL reads back as NULL"
        );
        assert!(
            matches!(values[7], DbValue::Null),
            "unset BLOB reads back as NULL"
        );

        // The engine reports what it is.
        assert!(sqlite_version().starts_with("turso 0."));
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn a_closed_handle_is_a_safe_no_op() {
        let db = Db::open(AzString::from(
            "/nonexistent-dir-azul/definitely/not/here.db",
        ));
        assert!(!db.is_open());
        assert_eq!(
            db.execute(AzString::from("SELECT 1"), DbValueVec::from_vec(Vec::new())),
            0
        );
        let rows = db.query(AzString::from("SELECT 1"), DbValueVec::from_vec(Vec::new()));
        assert!(rows.values.as_ref().is_empty());
    }
}
