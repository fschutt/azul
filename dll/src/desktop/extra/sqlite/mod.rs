//! The `Db` handle: a local-first key/value + index store with working-set
//! replication (SUPER_PLAN_2 §4 P4.3, remodeled for the single-surface API).
//!
//! The surface is identical on every target and raw SQL is not part of it:
//!
//! * `Db::open(config, data, on_open)` resumes with a `DbOpenResult`.
//! * `get` / `iterate` / `query_index` are requests that resume with a
//!   `DbValueResult` / `DbRowsResult`; `set` / `remove` are fire-and-forget
//!   writes that land in the local store immediately and mark the row dirty
//!   in the oplog.
//! * `subscribe` delivers a `DbChangeResult` for every change inside a scope,
//!   local or pulled; `sync_now` pushes the dirty oplog rows to the backup
//!   endpoint and pulls the remote changes back, resuming with a
//!   `DbSyncStatusResult`. Offline is `Queued`, never an error.
//!
//! Desktop engine: **turso** (pure-Rust SQLite, no C dependency), hidden
//! behind the `db-sqlite` feature. Every store is one table
//! `(k, v, dirty, modified, deleted)`, every declared index a side table
//! `<store>__idx__<name>(ik, k)`, and the oplog / cursor live in two `__az_*`
//! tables. turso's futures complete without a reactor, so they are driven
//! by an in-crate `block_on`. On web the same surface is served by IndexedDB
//! from the host; no engine is ever shipped to the browser.
//!
//! Sync speaks a row-level oplog over HTTPS (JSON, `POST <url>/push`,
//! `GET <url>/pull?db=..&since=..`), the same protocol the web host uses via
//! `fetch()`; conflicts are resolved per store (`DbConflictPolicy`).

use core::ffi::c_void;
use std::sync::{Arc, Mutex};

use azul_core::{
    db::{
        DbChangeResult, DbConfig, DbConflict, DbConflictPolicy, DbError, DbErrorKind, DbKeyRange,
        DbMergeCallback, DbRows, DbRowsResult, DbScope, DbSyncState, DbSyncStatus,
        DbSyncStatusResult, DbValue, DbValueResult, DbValueVec, OptionDbScope, OptionDbValue,
    },
    refany::RefAny,
    task::RequestId,
};
use azul_css::{
    corety::OptionString, impl_option, impl_option_inner, impl_result_inner, AzString, StringVec,
    U8Vec,
};
use azul_layout::{callbacks::ResumeCallback, request};

/// The engine name, for diagnostics.
#[cfg(feature = "db-sqlite")]
pub fn sqlite_version() -> &'static str {
    "turso 0.7"
}

/// One `subscribe` registration.
struct Subscription {
    id: RequestId,
    scope: DbScope,
    data: RefAny,
    callback: ResumeCallback,
}

/// One `set_on_sync_status` registration.
struct StatusSubscription {
    data: RefAny,
    callback: ResumeCallback,
}

/// One `set_on_conflict` registration.
struct MergeHook {
    store: AzString,
    data: RefAny,
    callback: DbMergeCallback,
}

/// Everything behind a live `Db`; shared by every clone of the handle.
struct DbState {
    config: DbConfig,
    #[cfg(feature = "db-sqlite")]
    engine: Option<engine::Handle>,
    subscriptions: Vec<Subscription>,
    status_subscriptions: Vec<StatusSubscription>,
    merge_hooks: Vec<MergeHook>,
    sync_state: DbSyncState,
    last_synced_ms: u64,
    sync_error: Option<AzString>,
    /// `true` once a pull has completed for the configured scope and no
    /// clean row has been evicted since.
    covered: bool,
    closed: bool,
    /// Where the local store lives (`":memory:"` for the throwaway store);
    /// the free-space query runs against its directory.
    store_path: String,
    /// When the last local write happened, for `DbAutoSync::on_idle`.
    last_write_ms: u64,
    /// When the last sync attempt started, for `DbAutoSync::interval`.
    last_sync_attempt_ms: u64,
}

/// Every open store, so the shells' per-frame pump can run the automatic
/// syncs (`DbAutoSync`). Entries are weak: a dropped handle disappears on
/// the next tick.
static OPEN_DBS: Mutex<Vec<std::sync::Weak<Inner>>> = Mutex::new(Vec::new());

/// Milliseconds of a `Duration` (ticks count as milliseconds on targets
/// without a system clock).
fn duration_ms(d: &azul_core::task::Duration) -> u64 {
    match d {
        azul_core::task::Duration::System(t) => {
            t.secs.saturating_mul(1000).saturating_add(u64::from(t.nanos) / 1_000_000)
        }
        azul_core::task::Duration::Tick(t) => t.tick_diff,
    }
}

/// How long a store may sit unwritten before `on_idle` syncs it.
const IDLE_SYNC_MS: u64 = 1500;

/// Free bytes on the volume holding `path` (`0` = unknown / in-memory).
fn free_bytes_at(path: &str) -> u64 {
    if path == ":memory:" {
        return 0;
    }
    let dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    #[cfg(all(unix, feature = "libc"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_dir) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
            return 0;
        };
        // SAFETY: `statvfs` is zero-initialised and only read after the call
        // reported success; `c_dir` is a valid NUL-terminated path.
        unsafe {
            let mut stats: libc::statvfs = core::mem::zeroed();
            if libc::statvfs(c_dir.as_ptr(), &mut stats) == 0 {
                return u64::from(stats.f_bavail).saturating_mul(u64::from(stats.f_frsize));
            }
        }
        0
    }
    #[cfg(all(windows, feature = "winapi"))]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(core::iter::once(0)).collect();
        let mut free_to_caller: winapi::shared::ntdef::ULARGE_INTEGER = unsafe { core::mem::zeroed() };
        // SAFETY: `wide` is NUL-terminated; the out-pointer is a valid, writable
        // ULARGE_INTEGER; the two other out-pointers may be null.
        let ok = unsafe {
            winapi::um::fileapi::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_to_caller,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        if ok != 0 {
            return unsafe { *free_to_caller.QuadPart() };
        }
        0
    }
    #[cfg(not(any(all(unix, feature = "libc"), all(windows, feature = "winapi"))))]
    {
        let _ = dir;
        0
    }
}

/// Run the automatic syncs that are due (`DbAutoSync::interval` elapsed, or
/// `on_idle` with unpushed writes older than `IDLE_SYNC_MS`). Called by the
/// shells' per-frame pump before completed requests are delivered, so the
/// sync's status callbacks resume in the same frame.
pub fn tick_auto_sync() {
    let now = now_ms();
    let live: Vec<Arc<Inner>> = {
        let mut open = OPEN_DBS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        open.retain(|w| w.strong_count() > 0);
        open.iter().filter_map(std::sync::Weak::upgrade).collect()
    };
    for inner in live {
        let due = {
            let s = inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            auto_sync_due(&s, now)
        };
        if !due {
            continue;
        }
        // SAFETY: `inner` is a live Arc; the temporary handle below balances
        // this increment in its `Drop`.
        unsafe { Arc::increment_strong_count(Arc::as_ptr(&inner)) };
        let db = Db {
            ptr: Arc::as_ptr(&inner) as *mut c_void,
            run_destructor: true,
        };
        let status = db.sync_now_blocking(OptionDbScope::None);
        db.notify_status(&status);
    }
}

fn auto_sync_due(s: &DbState, now: u64) -> bool {
    if s.closed || s.config.backup_sync_url.is_none() {
        return false;
    }
    #[cfg(feature = "db-sqlite")]
    let pending = s.engine.is_some() && engine::pending_push_ops(s) > 0;
    #[cfg(not(feature = "db-sqlite"))]
    let pending = false;
    let auto = &s.config.auto_sync;
    let by_interval = auto
        .interval
        .as_ref()
        .map(duration_ms)
        .is_some_and(|ms| ms > 0 && now.saturating_sub(s.last_sync_attempt_ms) >= ms);
    let by_idle = auto.on_idle
        && pending
        && s.last_write_ms > s.last_sync_attempt_ms
        && now.saturating_sub(s.last_write_ms) >= IDLE_SYNC_MS;
    by_interval || by_idle
}

struct Inner {
    state: Mutex<DbState>,
}

/// A handle to an open local-first database. Reference counted: clones
/// share the store, and the store closes when the last clone is dropped or
/// `close` is called.
#[repr(C)]
#[derive(Debug)]
pub struct Db {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

// The engine handle is only ever used under the state mutex.
unsafe impl Send for Db {}
unsafe impl Sync for Db {}

impl Clone for Db {
    fn clone(&self) -> Self {
        if !self.ptr.is_null() {
            // SAFETY: `ptr` came from `Arc::into_raw` and is still owned by
            // at least this handle.
            unsafe { Arc::increment_strong_count(self.ptr as *const Inner) };
        }
        Db {
            ptr: self.ptr,
            run_destructor: !self.ptr.is_null(),
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

impl Drop for Db {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            // SAFETY: balances the `into_raw` / `increment_strong_count`
            // that produced this handle.
            unsafe { Arc::decrement_strong_count(self.ptr as *const Inner) };
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

azul_css::impl_result!(
    Db,
    DbError,
    ResultDbDbError,
    copy = false,
    clone = false,
    [Debug, Clone]
);

/// Result of [`Db::open`].
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DbOpenResult {
    pub result: ResultDbDbError,
}

impl_option!(DbOpenResult, OptionDbOpenResult, copy = false, [Debug, Clone]);

impl DbOpenResult {
    /// Downcast the `result` RefAny delivered to a `ResumeCallback`.
    pub fn downcast(mut result: RefAny) -> OptionDbOpenResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn valid_store(store: &str) -> bool {
    !store.is_empty() && !store.starts_with("__az_") && !store.contains('\0') && !store.contains("__idx__")
}

fn empty_rows() -> DbRows {
    DbRows {
        columns: StringVec::from_vec(Vec::new()),
        values: DbValueVec::from_vec(Vec::new()),
    }
}

fn rows_error(message: impl Into<AzString>) -> DbRowsResult {
    DbRowsResult {
        rows: empty_rows(),
        error: OptionString::Some(message.into()),
    }
}

fn value_error(message: impl Into<AzString>) -> DbValueResult {
    DbValueResult {
        value: OptionDbValue::None,
        error: OptionString::Some(message.into()),
    }
}

/// Where the local store of `local_name` lives on this machine.
fn local_store_path(local_name: &str) -> String {
    if local_name == ":memory:" {
        return String::from(":memory:");
    }
    let stem: String = local_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    let dir = azul_layout::file::FilePath::get_data_local_dir()
        .or_else(azul_layout::file::FilePath::get_data_dir)
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().into_owned());
    let dir = std::path::Path::new(&dir).join("azul-db");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{stem}.sqlite")).to_string_lossy().into_owned()
}

impl Db {
    fn inner(&self) -> Option<&Inner> {
        if self.ptr.is_null() {
            None
        } else {
            // SAFETY: a non-null `ptr` is a live `Arc<Inner>` owned by this handle.
            Some(unsafe { &*(self.ptr as *const Inner) })
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut DbState) -> R) -> Option<R> {
        let inner = self.inner()?;
        let mut guard = inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(f(&mut guard))
    }

    /// Open (or create) the local store described by `config` and resume
    /// `on_open` with a `DbOpenResult`. Local-only unless the config names a
    /// `backup_sync_url`; the stores declared in the schema are created
    /// (idempotently) before the callback runs.
    pub fn open(config: DbConfig, data: RefAny, on_open: ResumeCallback) -> RequestId {
        let result = match Self::open_blocking(config) {
            Ok(db) => ResultDbDbError::Ok(db),
            Err(e) => ResultDbDbError::Err(e),
        };
        request::complete(data, on_open, DbOpenResult { result })
    }

    /// The synchronous half of [`Self::open`] (Rust-internal).
    pub fn open_blocking(config: DbConfig) -> Result<Db, DbError> {
        #[cfg(not(feature = "db-sqlite"))]
        {
            announce_db_stub("Db::open");
            let _ = config;
            Err(DbError::new(
                DbErrorKind::NoEngine,
                "this build has no local db engine (rebuild with --features db-sqlite)",
            ))
        }
        #[cfg(feature = "db-sqlite")]
        {
            let path = local_store_path(config.local_name.as_str());
            let handle = engine::open(&path).ok_or_else(|| {
                DbError::new(DbErrorKind::Io, format!("could not open the local store at {path}"))
            })?;
            let mut state = DbState {
                config,
                engine: Some(handle),
                subscriptions: Vec::new(),
                status_subscriptions: Vec::new(),
                merge_hooks: Vec::new(),
                sync_state: DbSyncState::Disconnected,
                last_synced_ms: 0,
                sync_error: None,
                covered: false,
                closed: false,
                store_path: path.clone(),
                last_write_ms: 0,
                last_sync_attempt_ms: 0,
            };
            engine::ensure_meta_tables(&mut state)?;
            let declared: Vec<AzString> = state
                .config
                .schema
                .stores
                .as_ref()
                .iter()
                .map(|s| s.name.clone())
                .collect();
            for store in declared {
                engine::ensure_store(&mut state, store.as_str())?;
            }
            state.last_synced_ms = engine::meta_get_u64(&state, "last_synced_ms");
            state.sync_state = if state.config.backup_sync_url.is_some() {
                if engine::pending_push_ops(&state) > 0 {
                    DbSyncState::Queued
                } else {
                    DbSyncState::Idle
                }
            } else {
                DbSyncState::Disconnected
            };
            let inner = Arc::new(Inner {
                state: Mutex::new(state),
            });
            OPEN_DBS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(Arc::downgrade(&inner));
            Ok(Db {
                ptr: Arc::into_raw(inner) as *mut c_void,
                run_destructor: true,
            })
        }
    }

    /// `true` while the handle is open (the engine is present and `close`
    /// has not been called).
    pub fn is_open(&self) -> bool {
        self.with_state(|s| !s.closed).unwrap_or(false)
    }

    /// Read `key` from `store`, resuming `on_result` with a `DbValueResult`
    /// (`value: None` when the key is absent).
    pub fn get(
        &self,
        store: AzString,
        key: DbValue,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        let result = self.get_blocking(store, key);
        request::complete(data, on_result, result)
    }

    /// The synchronous half of [`Self::get`] (Rust-internal).
    pub fn get_blocking(&self, store: AzString, key: DbValue) -> DbValueResult {
        if !valid_store(store.as_str()) {
            return value_error("invalid store name");
        }
        if key.is_null() {
            return value_error("Null is not a valid key");
        }
        self.with_state(|s| {
            if s.closed {
                return value_error("the database is closed");
            }
            #[cfg(feature = "db-sqlite")]
            {
                match engine::get(s, store.as_str(), &key) {
                    Ok(value) => DbValueResult {
                        value: value.into(),
                        error: OptionString::None,
                    },
                    Err(e) => value_error(e.message),
                }
            }
            #[cfg(not(feature = "db-sqlite"))]
            {
                value_error("this build has no local db engine")
            }
        })
        .unwrap_or_else(|| value_error("the database is not open"))
    }

    /// Write `value` under `key` in `store`. Fire-and-forget: the row lands in
    /// the local store immediately (read-your-writes) and is marked dirty in
    /// the oplog until the next successful push. Returns `false` for an
    /// invalid store / key or a closed handle.
    pub fn set(&self, store: AzString, key: DbValue, value: DbValue) -> bool {
        if !valid_store(store.as_str()) || key.is_null() {
            return false;
        }
        let ok = self
            .with_state(|s| {
                if s.closed {
                    return false;
                }
                #[cfg(feature = "db-sqlite")]
                {
                    let ok = engine::put(s, store.as_str(), &key, Some(&value), now_ms()).is_ok();
                    if ok {
                        s.last_write_ms = now_ms();
                        engine::enforce_budget(s);
                        if s.config.backup_sync_url.is_some() && s.sync_state != DbSyncState::Error {
                            s.sync_state = DbSyncState::Queued;
                        }
                    }
                    ok
                }
                #[cfg(not(feature = "db-sqlite"))]
                {
                    let _ = &value;
                    false
                }
            })
            .unwrap_or(false);
        if ok {
            self.notify_change(store, key, Some(value));
        }
        ok
    }

    /// Delete `key` from `store` (a tombstone that syncs like any other
    /// write). Returns `false` for an invalid store / key or a closed handle.
    pub fn remove(&self, store: AzString, key: DbValue) -> bool {
        if !valid_store(store.as_str()) || key.is_null() {
            return false;
        }
        let ok = self
            .with_state(|s| {
                if s.closed {
                    return false;
                }
                #[cfg(feature = "db-sqlite")]
                {
                    let ok = engine::put(s, store.as_str(), &key, None, now_ms()).is_ok();
                    if ok {
                        s.last_write_ms = now_ms();
                    }
                    if ok && s.config.backup_sync_url.is_some() && s.sync_state != DbSyncState::Error {
                        s.sync_state = DbSyncState::Queued;
                    }
                    ok
                }
                #[cfg(not(feature = "db-sqlite"))]
                {
                    false
                }
            })
            .unwrap_or(false);
        if ok {
            self.notify_change(store, key, None);
        }
        ok
    }

    /// The rows of `store` whose key falls in `range`, in key order, at most
    /// `limit` of them (`0` = no limit), resuming `on_result` with a
    /// `DbRowsResult` whose grid has the two columns `key` and `value`.
    pub fn iterate(
        &self,
        store: AzString,
        range: DbKeyRange,
        limit: u32,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        let result = self.iterate_blocking(store, range, limit);
        request::complete(data, on_result, result)
    }

    /// The synchronous half of [`Self::iterate`] (Rust-internal).
    pub fn iterate_blocking(&self, store: AzString, range: DbKeyRange, limit: u32) -> DbRowsResult {
        if !valid_store(store.as_str()) {
            return rows_error("invalid store name");
        }
        self.with_state(|s| {
            if s.closed {
                return rows_error("the database is closed");
            }
            #[cfg(feature = "db-sqlite")]
            {
                match engine::iterate(s, store.as_str(), &range, limit) {
                    Ok(rows) => DbRowsResult {
                        rows,
                        error: OptionString::None,
                    },
                    Err(e) => rows_error(e.message),
                }
            }
            #[cfg(not(feature = "db-sqlite"))]
            {
                let _ = (&range, limit);
                rows_error("this build has no local db engine")
            }
        })
        .unwrap_or_else(|| rows_error("the database is not open"))
    }

    /// The rows of `store` whose `index` key falls in `range`, in index-key
    /// order, at most `limit` (`0` = no limit); same result shape as
    /// [`Self::iterate`].
    pub fn query_index(
        &self,
        store: AzString,
        index: AzString,
        range: DbKeyRange,
        limit: u32,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        let result = self.query_index_blocking(store, index, range, limit);
        request::complete(data, on_result, result)
    }

    /// The synchronous half of [`Self::query_index`] (Rust-internal).
    pub fn query_index_blocking(
        &self,
        store: AzString,
        index: AzString,
        range: DbKeyRange,
        limit: u32,
    ) -> DbRowsResult {
        if !valid_store(store.as_str()) || index.as_str().is_empty() {
            return rows_error("invalid store or index name");
        }
        self.with_state(|s| {
            if s.closed {
                return rows_error("the database is closed");
            }
            #[cfg(feature = "db-sqlite")]
            {
                match engine::query_index(s, store.as_str(), index.as_str(), &range, limit) {
                    Ok(rows) => DbRowsResult {
                        rows,
                        error: OptionString::None,
                    },
                    Err(e) => rows_error(e.message),
                }
            }
            #[cfg(not(feature = "db-sqlite"))]
            {
                let _ = (&range, limit);
                rows_error("this build has no local db engine")
            }
        })
        .unwrap_or_else(|| rows_error("the database is not open"))
    }

    /// Extend the local working set to cover `scope` and deliver a
    /// `DbChangeResult` to `on_change` for every change inside it, local or
    /// pulled, until the handle is closed. The returned id identifies the
    /// subscription.
    pub fn subscribe(&self, scope: DbScope, data: RefAny, on_change: ResumeCallback) -> RequestId {
        let id = RequestId::unique();
        self.with_state(|s| {
            s.subscriptions.push(Subscription {
                id,
                scope,
                data,
                callback: on_change,
            });
        });
        id
    }

    /// Explicit sync point: push every dirty row to the backup endpoint, then
    /// refresh the given (or the configured) scope from it, resuming
    /// `on_result` with a `DbSyncStatusResult`. Offline leaves the writes
    /// queued and reports `Queued`; it is not an error.
    pub fn sync_now(
        &self,
        scope: OptionDbScope,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        let status = self.sync_now_blocking(scope);
        self.notify_status(&status);
        request::complete(data, on_result, DbSyncStatusResult { status })
    }

    /// The synchronous half of [`Self::sync_now`] (Rust-internal).
    pub fn sync_now_blocking(&self, scope: OptionDbScope) -> DbSyncStatus {
        let pulled = self.with_state(|s| {
            if s.closed {
                return Vec::new();
            }
            #[cfg(feature = "db-sqlite")]
            {
                sync::run(s, scope.as_ref())
            }
            #[cfg(not(feature = "db-sqlite"))]
            {
                let _ = &scope;
                Vec::new()
            }
        });
        for (store, key, value) in pulled.unwrap_or_default() {
            self.notify_change(store, key, value);
        }
        self.sync_status()
    }

    /// A snapshot of the sync state (poll getter).
    pub fn sync_status(&self) -> DbSyncStatus {
        self.with_state(|s| {
            if s.closed {
                return DbSyncStatus::disconnected();
            }
            #[cfg(feature = "db-sqlite")]
            {
                let used = engine::local_bytes_used(s);
                DbSyncStatus {
                    state: s.sync_state,
                    working_set_coverage_x1000: if s.covered { 1000 } else { 0 },
                    pending_push_ops: engine::pending_push_ops(s),
                    last_synced_ms: s.last_synced_ms,
                    local_bytes_used: used,
                    local_bytes_budget: s.config.local_budget_bytes,
                    quota_bytes_available: free_bytes_at(&s.store_path),
                    error: s.sync_error.clone().into(),
                }
            }
            #[cfg(not(feature = "db-sqlite"))]
            {
                DbSyncStatus::disconnected()
            }
        })
        .unwrap_or_else(DbSyncStatus::disconnected)
    }

    /// Deliver a `DbSyncStatusResult` to `on_status` after every sync until
    /// the handle is closed.
    pub fn set_on_sync_status(&mut self, data: RefAny, on_status: ResumeCallback) {
        self.with_state(|s| {
            s.status_subscriptions.push(StatusSubscription {
                data,
                callback: on_status,
            });
        });
    }

    /// Install the merge function for `store` (used when its conflict policy
    /// is `Merge`).
    pub fn set_on_conflict(&mut self, store: AzString, data: RefAny, on_merge: DbMergeCallback) {
        self.with_state(|s| {
            s.merge_hooks.retain(|h| h.store.as_str() != store.as_str());
            s.merge_hooks.push(MergeHook {
                store,
                data,
                callback: on_merge,
            });
        });
    }

    /// Close the store. Every clone of the handle sees `is_open() == false`
    /// afterwards; subscriptions are dropped.
    pub fn close(&mut self) {
        self.with_state(|s| {
            s.closed = true;
            s.subscriptions.clear();
            s.status_subscriptions.clear();
            s.merge_hooks.clear();
            #[cfg(feature = "db-sqlite")]
            {
                s.engine = None;
            }
        });
    }

    fn notify_change(&self, store: AzString, key: DbValue, value: Option<DbValue>) {
        let targets: Vec<(RefAny, ResumeCallback)> = self
            .with_state(|s| {
                s.subscriptions
                    .iter()
                    .filter(|sub| sub.scope.covers(store.as_str(), &key))
                    .map(|sub| (sub.data.clone(), sub.callback.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (data, callback) in targets {
            let change = DbChangeResult {
                store: store.clone(),
                key: key.clone(),
                value: value.clone().into(),
            };
            let _ = request::complete(data, callback, change);
        }
    }

    fn notify_status(&self, status: &DbSyncStatus) {
        let targets: Vec<(RefAny, ResumeCallback)> = self
            .with_state(|s| {
                s.status_subscriptions
                    .iter()
                    .map(|sub| (sub.data.clone(), sub.callback.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (data, callback) in targets {
            let _ = request::complete(
                data,
                callback,
                DbSyncStatusResult {
                    status: status.clone(),
                },
            );
        }
    }
}

#[cfg(not(feature = "db-sqlite"))]
fn announce_db_stub(what: &str) {
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][db] {what} called, but this build has no `db-sqlite` feature: every Db \
             request resolves with DbErrorKind::NoEngine. Rebuild azul-dll with \
             --features build-dll,db-sqlite"
        );
    });
}

// ============================================================================
// turso engine
// ============================================================================

#[cfg(feature = "db-sqlite")]
mod engine {
    use core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    };

    use azul_core::db::{range_contains, DbIndexSchema};
    use turso::{params::Params, Builder, Connection, Database, Value};

    use super::*;

    /// The boxed engine state behind a live `Db`. The `Database` is kept
    /// alive alongside the `Connection` so the store outlives every query.
    pub struct Handle {
        #[allow(dead_code)]
        db: Database,
        conn: Connection,
    }

    /// Minimal `block_on` for turso's futures. turso runs its IO on an
    /// in-process synchronous backend, so its futures complete without
    /// yielding to any reactor - a no-op waker busy-poll terminates.
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

    pub fn open(path: &str) -> Option<Handle> {
        let db = block_on(Builder::new_local(path).build()).ok()?;
        let conn = db.connect().ok()?;
        Some(Handle { db, conn })
    }

    fn conn(state: &DbState) -> Result<&Connection, DbError> {
        state
            .engine
            .as_ref()
            .map(|h| &h.conn)
            .ok_or_else(|| DbError::new(DbErrorKind::NotOpen, "the database is closed"))
    }

    fn engine_err(e: impl core::fmt::Display) -> DbError {
        DbError::new(DbErrorKind::Engine, format!("{e}"))
    }

    fn quote(ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn idx_table(store: &str, index: &str) -> String {
        format!("{store}__idx__{index}")
    }

    fn exec(state: &DbState, sql: &str, params: Vec<Value>) -> Result<u64, DbError> {
        let c = conn(state)?;
        block_on(c.execute(sql, Params::Positional(params))).map_err(engine_err)
    }

    fn query(state: &DbState, sql: &str, params: Vec<Value>) -> Result<Vec<Vec<Value>>, DbError> {
        let c = conn(state)?;
        let mut stmt = block_on(c.prepare(sql)).map_err(engine_err)?;
        let mut rows = block_on(stmt.query(Params::Positional(params))).map_err(engine_err)?;
        let mut out = Vec::new();
        while let Some(row) = block_on(rows.next()).map_err(engine_err)? {
            let n = row.column_count();
            let mut cells = Vec::with_capacity(n);
            for i in 0..n {
                cells.push(row.get_value(i).unwrap_or(Value::Null));
            }
            out.push(cells);
        }
        Ok(out)
    }

    pub fn db_to_value(v: &DbValue) -> Value {
        match v {
            DbValue::Null => Value::Null,
            DbValue::Integer(i) => Value::Integer(*i),
            DbValue::Real(r) => Value::Real(*r),
            DbValue::Text(s) => Value::Text(s.as_str().to_string()),
            DbValue::Blob(b) => Value::Blob(b.as_ref().to_vec()),
        }
    }

    pub fn value_to_db(v: Value) -> DbValue {
        match v {
            Value::Null => DbValue::Null,
            Value::Integer(i) => DbValue::Integer(i),
            Value::Real(r) => DbValue::Real(r),
            Value::Text(s) => DbValue::Text(AzString::from(s)),
            Value::Blob(b) => DbValue::Blob(U8Vec::from_vec(b)),
        }
    }

    fn value_u64(v: Option<&Value>) -> u64 {
        match v {
            Some(Value::Integer(i)) => (*i).max(0) as u64,
            Some(Value::Real(r)) => r.max(0.0) as u64,
            Some(Value::Text(s)) => s.parse().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn ensure_meta_tables(state: &mut DbState) -> Result<(), DbError> {
        exec(
            state,
            "CREATE TABLE IF NOT EXISTS __az_oplog (seq INTEGER PRIMARY KEY, store TEXT NOT NULL, \
             k BLOB NOT NULL, v BLOB, deleted INTEGER NOT NULL, modified INTEGER NOT NULL)",
            Vec::new(),
        )?;
        exec(
            state,
            "CREATE TABLE IF NOT EXISTS __az_meta (key TEXT PRIMARY KEY, value TEXT)",
            Vec::new(),
        )?;
        Ok(())
    }

    pub fn meta_get(state: &DbState, key: &str) -> Option<String> {
        let rows = query(
            state,
            "SELECT value FROM __az_meta WHERE key = ?",
            vec![Value::Text(key.to_string())],
        )
        .ok()?;
        match rows.first().and_then(|r| r.first()) {
            Some(Value::Text(s)) => Some(s.clone()),
            Some(Value::Integer(i)) => Some(i.to_string()),
            _ => None,
        }
    }

    pub fn meta_get_u64(state: &DbState, key: &str) -> u64 {
        meta_get(state, key).and_then(|s| s.parse().ok()).unwrap_or(0)
    }

    pub fn meta_set(state: &DbState, key: &str, value: &str) -> Result<(), DbError> {
        exec(
            state,
            "INSERT OR REPLACE INTO __az_meta (key, value) VALUES (?, ?)",
            vec![Value::Text(key.to_string()), Value::Text(value.to_string())],
        )?;
        Ok(())
    }

    fn declared_indexes(state: &DbState, store: &str) -> Vec<DbIndexSchema> {
        state
            .config
            .schema
            .store(store)
            .map(|s| s.indexes.as_ref().to_vec())
            .unwrap_or_default()
    }

    pub fn ensure_store(state: &mut DbState, store: &str) -> Result<(), DbError> {
        exec(
            state,
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (k BLOB PRIMARY KEY NOT NULL, v BLOB, dirty INTEGER NOT \
                 NULL DEFAULT 0, modified INTEGER NOT NULL DEFAULT 0, deleted INTEGER NOT NULL DEFAULT 0)",
                quote(store)
            ),
            Vec::new(),
        )?;
        for index in declared_indexes(state, store) {
            let table = idx_table(store, index.name.as_str());
            exec(
                state,
                &format!(
                    "CREATE TABLE IF NOT EXISTS {} (ik BLOB, k BLOB NOT NULL, PRIMARY KEY (ik, k))",
                    quote(&table)
                ),
                Vec::new(),
            )?;
            if index.unique {
                exec(
                    state,
                    &format!(
                        "CREATE UNIQUE INDEX IF NOT EXISTS {} ON {} (ik)",
                        quote(&format!("{table}__unique")),
                        quote(&table)
                    ),
                    Vec::new(),
                )?;
            }
        }
        Ok(())
    }

    /// The index key of `value` under `key_path`: a top-level field of a
    /// JSON object value.
    fn index_key(value: &DbValue, key_path: &str) -> Option<DbValue> {
        let DbValue::Text(text) = value else {
            return None;
        };
        let json = azul_core::json::Json::parse(text.as_str()).ok()?;
        let field = json.get_key(key_path)?;
        if field.is_null() {
            None
        } else if let Some(s) = field.as_string().into_option() {
            Some(DbValue::Text(s))
        } else if let Some(i) = field.as_i64().into_option() {
            Some(DbValue::Integer(i))
        } else if let Some(n) = field.as_number().into_option() {
            Some(DbValue::Real(n))
        } else if let Some(b) = field.as_bool().into_option() {
            Some(DbValue::Integer(i64::from(b)))
        } else {
            Some(DbValue::Text(field.to_json_string()))
        }
    }

    fn update_indexes(state: &DbState, store: &str, key: &DbValue, value: Option<&DbValue>) -> Result<(), DbError> {
        for index in declared_indexes(state, store) {
            let table = quote(&idx_table(store, index.name.as_str()));
            exec(state, &format!("DELETE FROM {table} WHERE k = ?"), vec![db_to_value(key)])?;
            if let Some(ik) = value.and_then(|v| index_key(v, index.key_path.as_str())) {
                exec(
                    state,
                    &format!("INSERT OR REPLACE INTO {table} (ik, k) VALUES (?, ?)"),
                    vec![db_to_value(&ik), db_to_value(key)],
                )?;
            }
        }
        Ok(())
    }

    /// Write a local change: the row (or tombstone), its indexes, and an
    /// oplog entry marking it dirty.
    pub fn put(
        state: &mut DbState,
        store: &str,
        key: &DbValue,
        value: Option<&DbValue>,
        modified: u64,
    ) -> Result<(), DbError> {
        ensure_store(state, store)?;
        write_row(state, store, key, value, modified, true)?;
        let seq = meta_get_u64(state, "next_seq") + 1;
        exec(
            state,
            "INSERT INTO __az_oplog (seq, store, k, v, deleted, modified) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                Value::Integer(seq as i64),
                Value::Text(store.to_string()),
                db_to_value(key),
                value.map(db_to_value).unwrap_or(Value::Null),
                Value::Integer(i64::from(value.is_none())),
                Value::Integer(modified as i64),
            ],
        )?;
        meta_set(state, "next_seq", &seq.to_string())
    }

    /// Write a row without touching the oplog (used for pulled changes).
    pub fn write_row(
        state: &DbState,
        store: &str,
        key: &DbValue,
        value: Option<&DbValue>,
        modified: u64,
        dirty: bool,
    ) -> Result<(), DbError> {
        exec(
            state,
            &format!(
                "INSERT OR REPLACE INTO {} (k, v, dirty, modified, deleted) VALUES (?, ?, ?, ?, ?)",
                quote(store)
            ),
            vec![
                db_to_value(key),
                value.map(db_to_value).unwrap_or(Value::Null),
                Value::Integer(i64::from(dirty)),
                Value::Integer(modified as i64),
                Value::Integer(i64::from(value.is_none())),
            ],
        )?;
        update_indexes(state, store, key, value)
    }

    /// `(value, modified, dirty)` of a row, tombstones included.
    pub fn read_row(state: &DbState, store: &str, key: &DbValue) -> Result<Option<(Option<DbValue>, u64, bool)>, DbError> {
        let rows = query(
            state,
            &format!("SELECT v, modified, dirty, deleted FROM {} WHERE k = ?", quote(store)),
            vec![db_to_value(key)],
        )?;
        Ok(rows.into_iter().next().map(|mut r| {
            let deleted = value_u64(r.get(3)) != 0;
            let dirty = value_u64(r.get(2)) != 0;
            let modified = value_u64(r.get(1));
            let v = if deleted { None } else { Some(value_to_db(core::mem::replace(&mut r[0], Value::Null))) };
            (v, modified, dirty)
        }))
    }

    fn store_exists(state: &DbState, store: &str) -> bool {
        query(
            state,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
            vec![Value::Text(store.to_string())],
        )
        .map(|rows| !rows.is_empty())
        .unwrap_or(false)
    }

    pub fn get(state: &DbState, store: &str, key: &DbValue) -> Result<Option<DbValue>, DbError> {
        if !store_exists(state, store) {
            return Ok(None);
        }
        Ok(read_row(state, store, key)?.and_then(|(v, _, _)| v))
    }

    fn range_sql(column: &str, range: &DbKeyRange, params: &mut Vec<Value>) -> String {
        let mut clauses = Vec::new();
        if let Some(lower) = range.lower.as_ref() {
            clauses.push(format!("{column} {} ?", if range.lower_open { ">" } else { ">=" }));
            params.push(db_to_value(lower));
        }
        if let Some(upper) = range.upper.as_ref() {
            clauses.push(format!("{column} {} ?", if range.upper_open { "<" } else { "<=" }));
            params.push(db_to_value(upper));
        }
        clauses.join(" AND ")
    }

    fn limit_sql(limit: u32) -> String {
        if limit == 0 {
            String::new()
        } else {
            format!(" LIMIT {limit}")
        }
    }

    fn rows_from(cells: Vec<Vec<Value>>) -> DbRows {
        let mut values = Vec::with_capacity(cells.len() * 2);
        for mut row in cells {
            if row.len() < 2 {
                continue;
            }
            let v = core::mem::replace(&mut row[1], Value::Null);
            let k = core::mem::replace(&mut row[0], Value::Null);
            values.push(value_to_db(k));
            values.push(value_to_db(v));
        }
        DbRows {
            columns: StringVec::from_vec(vec![AzString::from("key"), AzString::from("value")]),
            values: DbValueVec::from_vec(values),
        }
    }

    pub fn iterate(state: &DbState, store: &str, range: &DbKeyRange, limit: u32) -> Result<DbRows, DbError> {
        if !store_exists(state, store) {
            return Ok(super::empty_rows());
        }
        let mut params = Vec::new();
        let mut sql = format!("SELECT k, v FROM {} WHERE deleted = 0", quote(store));
        let clause = range_sql("k", range, &mut params);
        if !clause.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&clause);
        }
        sql.push_str(" ORDER BY k");
        sql.push_str(&limit_sql(limit));
        Ok(rows_from(query(state, &sql, params)?))
    }

    pub fn query_index(
        state: &DbState,
        store: &str,
        index: &str,
        range: &DbKeyRange,
        limit: u32,
    ) -> Result<DbRows, DbError> {
        let table = idx_table(store, index);
        if !store_exists(state, store) || !store_exists(state, &table) {
            return Err(DbError::new(
                DbErrorKind::InvalidStore,
                format!("no index {index} on store {store} (declare it in the DbSchema)"),
            ));
        }
        let mut params = Vec::new();
        let mut sql = format!(
            "SELECT s.k, s.v FROM {} s JOIN {} i ON i.k = s.k WHERE s.deleted = 0",
            quote(store),
            quote(&table)
        );
        let clause = range_sql("i.ik", range, &mut params);
        if !clause.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&clause);
        }
        sql.push_str(" ORDER BY i.ik, s.k");
        sql.push_str(&limit_sql(limit));
        Ok(rows_from(query(state, &sql, params)?))
    }

    pub fn pending_push_ops(state: &DbState) -> u64 {
        let last = meta_get_u64(state, "last_pushed_seq");
        query(
            state,
            "SELECT COUNT(*) FROM __az_oplog WHERE seq > ?",
            vec![Value::Integer(last as i64)],
        )
        .ok()
        .and_then(|rows| rows.first().map(|r| value_u64(r.first())))
        .unwrap_or(0)
    }

    /// Every user store (not the `__az_*` bookkeeping, not the index tables).
    pub fn user_stores(state: &DbState) -> Vec<String> {
        query(
            state,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '\\_\\_az\\_%' ESCAPE '\\' AND name NOT LIKE '%\\_\\_idx\\_\\_%' ESCAPE '\\'",
            Vec::new(),
        )
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| match r.into_iter().next() {
            Some(Value::Text(s)) => Some(s),
            _ => None,
        })
        .collect()
    }

    pub fn local_bytes_used(state: &DbState) -> u64 {
        let mut total = 0u64;
        for store in user_stores(state) {
            let sql = format!(
                "SELECT COALESCE(SUM(LENGTH(k) + COALESCE(LENGTH(v), 0)), 0) FROM {}",
                quote(&store)
            );
            if let Ok(rows) = query(state, &sql, Vec::new()) {
                total += rows.first().map(|r| value_u64(r.first())).unwrap_or(0);
            }
        }
        total
    }

    /// Evict clean rows, oldest first, until the store fits its budget.
    /// Dirty (unpushed) rows are never evicted.
    pub fn enforce_budget(state: &mut DbState) {
        let budget = state.config.local_budget_bytes;
        if budget == 0 {
            return;
        }
        let mut rounds = 0;
        while local_bytes_used(state) > budget && rounds < 64 {
            rounds += 1;
            let mut evicted = 0u64;
            for store in user_stores(state) {
                let sql = format!(
                    "DELETE FROM {q} WHERE dirty = 0 AND k IN (SELECT k FROM {q} WHERE dirty = 0 ORDER BY modified ASC LIMIT 32)",
                    q = quote(&store)
                );
                evicted += exec(state, &sql, Vec::new()).unwrap_or(0);
            }
            if evicted == 0 {
                break;
            }
            state.covered = false;
        }
    }

    /// The dirty oplog rows after `last_pushed_seq`: `(seq, store, key, value, modified)`.
    pub fn unpushed_ops(state: &DbState) -> Result<Vec<(u64, String, DbValue, Option<DbValue>, u64)>, DbError> {
        let last = meta_get_u64(state, "last_pushed_seq");
        let rows = query(
            state,
            "SELECT seq, store, k, v, deleted, modified FROM __az_oplog WHERE seq > ? ORDER BY seq",
            vec![Value::Integer(last as i64)],
        )?;
        Ok(rows
            .into_iter()
            .filter_map(|mut r| {
                if r.len() < 6 {
                    return None;
                }
                let seq = value_u64(r.first());
                let store = match &r[1] {
                    Value::Text(s) => s.clone(),
                    _ => return None,
                };
                let deleted = value_u64(r.get(4)) != 0;
                let modified = value_u64(r.get(5));
                let v = core::mem::replace(&mut r[3], Value::Null);
                let k = core::mem::replace(&mut r[2], Value::Null);
                let value = if deleted { None } else { Some(value_to_db(v)) };
                Some((seq, store, value_to_db(k), value, modified))
            })
            .collect())
    }

    /// Acknowledge a push: forget the pushed oplog rows and clear the dirty
    /// flag of rows not modified since.
    pub fn mark_pushed(state: &DbState, upto_seq: u64, pushed: &[(u64, String, DbValue, Option<DbValue>, u64)]) -> Result<(), DbError> {
        for (_, store, key, _, modified) in pushed {
            exec(
                state,
                &format!("UPDATE {} SET dirty = 0 WHERE k = ? AND modified <= ?", quote(store)),
                vec![db_to_value(key), Value::Integer(*modified as i64)],
            )?;
        }
        exec(
            state,
            "DELETE FROM __az_oplog WHERE seq <= ?",
            vec![Value::Integer(upto_seq as i64)],
        )?;
        meta_set(state, "last_pushed_seq", &upto_seq.to_string())
    }

    /// Whether `key` in `store` is inside `scope`, evaluated locally.
    pub fn in_scope(scope: Option<&DbScope>, store: &str, key: &DbValue) -> bool {
        match scope {
            None => true,
            Some(s) => {
                s.collections.as_ref().is_empty()
                    || s.collections.as_ref().iter().any(|c| {
                        c.store.as_str() == store
                            && c.range.as_ref().map(|r| range_contains(r, key)).unwrap_or(true)
                    })
            }
        }
    }
}

// ============================================================================
// Row-level oplog sync over HTTPS
// ============================================================================

#[cfg(feature = "db-sqlite")]
mod sync {
    use azul_core::json::Json;

    use super::*;

    /// One pushed / pulled change on the wire.
    struct Op {
        store: String,
        key: DbValue,
        value: Option<DbValue>,
        modified: u64,
    }

    fn json_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(s: &str) -> Option<Vec<u8>> {
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    }

    fn value_json(v: Option<&DbValue>) -> String {
        match v {
            None | Some(DbValue::Null) => String::from("{\"t\":\"null\"}"),
            Some(DbValue::Integer(i)) => format!("{{\"t\":\"int\",\"v\":{i}}}"),
            Some(DbValue::Real(r)) => format!("{{\"t\":\"real\",\"v\":{r}}}"),
            Some(DbValue::Text(s)) => format!("{{\"t\":\"text\",\"v\":\"{}\"}}", json_escape(s.as_str())),
            Some(DbValue::Blob(b)) => format!("{{\"t\":\"blob\",\"v\":\"{}\"}}", hex(b.as_ref())),
        }
    }

    fn value_from_json(j: &Json) -> Option<DbValue> {
        let t = j.get_key("t")?.as_string().into_option()?;
        let v = j.get_key("v");
        Some(match t.as_str() {
            "null" => DbValue::Null,
            "int" => DbValue::Integer(v?.as_i64().into_option()?),
            "real" => DbValue::Real(v?.as_number().into_option()?),
            "text" => DbValue::Text(v?.as_string().into_option()?),
            "blob" => DbValue::Blob(U8Vec::from_vec(unhex(v?.as_string().into_option()?.as_str())?)),
            _ => return None,
        })
    }

    fn op_json(seq: u64, op: &Op) -> String {
        format!(
            "{{\"seq\":{seq},\"store\":\"{}\",\"key\":{},\"value\":{},\"deleted\":{},\"modified\":{}}}",
            json_escape(&op.store),
            value_json(Some(&op.key)),
            value_json(op.value.as_ref()),
            op.value.is_none(),
            op.modified
        )
    }

    fn op_from_json(j: &Json) -> Option<Op> {
        let store = j.get_key("store")?.as_string().into_option()?.as_str().to_string();
        let key = value_from_json(&j.get_key("key")?)?;
        let deleted = j.get_key("deleted").and_then(|d| d.as_bool().into_option()).unwrap_or(false);
        let value = if deleted {
            None
        } else {
            match j.get_key("value") {
                Some(v) if !v.is_null() => value_from_json(&v),
                _ => None,
            }
        };
        let modified = j.get_key("modified").and_then(|m| m.as_i64().into_option()).unwrap_or(0).max(0) as u64;
        Some(Op {
            store,
            key,
            value,
            modified,
        })
    }

    enum Transport {
        Ok(u16, Vec<u8>),
        /// The endpoint could not be reached: offline, DNS, TLS, timeout.
        Offline(String),
        /// The build has no HTTP client.
        Unavailable,
    }

    #[cfg(feature = "http")]
    fn http(state: &DbState, method_post: bool, url: &str, body: Vec<u8>) -> Transport {
        use azul_layout::http::{HttpError, HttpRequestConfig, ResultHttpResponseHttpError};
        let mut cfg = HttpRequestConfig::new().with_timeout(30);
        if let Some(token) = state.config.auth_token.as_ref() {
            cfg = cfg.with_header("Authorization", format!("Bearer {}", token.as_str()));
        }
        let url = AzString::from(url.to_string());
        let result = if method_post {
            cfg.http_post_blocking(url, U8Vec::from_vec(body), AzString::from("application/json"))
        } else {
            cfg.http_get_blocking(url)
        };
        match result {
            ResultHttpResponseHttpError::Ok(resp) => Transport::Ok(resp.status_code, resp.body.as_ref().to_vec()),
            ResultHttpResponseHttpError::Err(e) => match e {
                HttpError::ConnectionFailed(m) | HttpError::TlsError(m) | HttpError::IoError(m) => {
                    Transport::Offline(m.as_str().to_string())
                }
                HttpError::Timeout => Transport::Offline(String::from("timeout")),
                other => Transport::Offline(format!("{other}")),
            },
        }
    }

    #[cfg(not(feature = "http"))]
    fn http(_state: &DbState, _method_post: bool, _url: &str, _body: Vec<u8>) -> Transport {
        Transport::Unavailable
    }

    fn scope_stores(scope: Option<&DbScope>) -> String {
        scope
            .map(|s| {
                s.collections
                    .as_ref()
                    .iter()
                    .map(|c| c.store.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default()
    }

    fn fail(state: &mut DbState, st: DbSyncState, msg: String) -> Vec<(AzString, DbValue, Option<DbValue>)> {
        state.sync_state = st;
        state.sync_error = if st == DbSyncState::Error { Some(AzString::from(msg)) } else { None };
        Vec::new()
    }

    /// Push then pull. Returns the changes applied from the remote so the
    /// caller can notify subscribers once the state lock is released.
    pub fn run(state: &mut DbState, scope: Option<&DbScope>) -> Vec<(AzString, DbValue, Option<DbValue>)> {
        state.last_sync_attempt_ms = now_ms();
        let Some(url) = state.config.backup_sync_url.as_ref().map(|u| u.as_str().trim_end_matches('/').to_string()) else {
            state.sync_state = DbSyncState::Disconnected;
            state.sync_error = None;
            return Vec::new();
        };
        // Own the scope: `apply_remote` needs `state` mutably further down.
        let scope: Option<DbScope> = scope.cloned().or_else(|| state.config.scope.as_ref().cloned());
        let scope = scope.as_ref();

        // ---- push
        state.sync_state = DbSyncState::Pushing;
        let ops = match engine::unpushed_ops(state) {
            Ok(o) => o,
            Err(e) => return fail(state, DbSyncState::Error, e.message.as_str().to_string()),
        };
        if !ops.is_empty() {
            let body = format!(
                "{{\"db\":\"{}\",\"ops\":[{}]}}",
                json_escape(state.config.local_name.as_str()),
                ops.iter()
                    .map(|(seq, store, key, value, modified)| op_json(
                        *seq,
                        &Op {
                            store: store.clone(),
                            key: key.clone(),
                            value: value.clone(),
                            modified: *modified
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            match http(state, true, &format!("{url}/push"), body.into_bytes()) {
                Transport::Ok(status, _) if (200..300).contains(&status) => {
                    let upto = ops.last().map(|o| o.0).unwrap_or(0);
                    if let Err(e) = engine::mark_pushed(state, upto, &ops) {
                        return fail(state, DbSyncState::Error, e.message.as_str().to_string());
                    }
                }
                Transport::Ok(status, _) => {
                    return fail(state, DbSyncState::Error, format!("push rejected with HTTP {status}"))
                }
                Transport::Offline(_) => return fail(state, DbSyncState::Queued, String::new()),
                Transport::Unavailable => {
                    return fail(
                        state,
                        DbSyncState::Error,
                        String::from("built without the `http` feature: sync has no transport"),
                    )
                }
            }
        }

        // ---- pull
        state.sync_state = DbSyncState::Pulling;
        let cursor = engine::meta_get(state, "pull_cursor").unwrap_or_default();
        let stores = scope_stores(scope);
        let pull_url = format!(
            "{url}/pull?db={}&since={}&stores={}",
            state.config.local_name.as_str(),
            cursor,
            stores
        );
        let body = match http(state, false, &pull_url, Vec::new()) {
            Transport::Ok(status, body) if (200..300).contains(&status) => body,
            Transport::Ok(status, _) => {
                return fail(state, DbSyncState::Error, format!("pull rejected with HTTP {status}"))
            }
            Transport::Offline(_) => return fail(state, DbSyncState::Queued, String::new()),
            Transport::Unavailable => {
                return fail(
                    state,
                    DbSyncState::Error,
                    String::from("built without the `http` feature: sync has no transport"),
                )
            }
        };
        let json = match Json::parse_bytes(&body) {
            Ok(j) => j,
            Err(_) => return fail(state, DbSyncState::Error, String::from("pull answered with invalid JSON")),
        };
        let mut applied = Vec::new();
        if let Some(ops) = json.get_key("ops").and_then(|o| o.to_array()) {
            for j in ops.as_ref() {
                let Some(op) = op_from_json(j) else { continue };
                if !super::valid_store(&op.store) || !engine::in_scope(scope, &op.store, &op.key) {
                    continue;
                }
                match apply_remote(state, &op) {
                    Ok(true) => applied.push((AzString::from(op.store.clone()), op.key.clone(), op.value.clone())),
                    Ok(false) => {}
                    Err(e) => return fail(state, DbSyncState::Error, e.message.as_str().to_string()),
                }
            }
        }
        if let Some(cursor) = json.get_key("cursor") {
            let text = cursor
                .as_string()
                .into_option()
                .map(|s| s.as_str().to_string())
                .or_else(|| cursor.as_i64().into_option().map(|i| i.to_string()));
            if let Some(text) = text {
                let _ = engine::meta_set(state, "pull_cursor", &text);
            }
        }
        state.last_synced_ms = now_ms();
        let _ = engine::meta_set(state, "last_synced_ms", &state.last_synced_ms.to_string());
        state.covered = true;
        state.sync_state = DbSyncState::Idle;
        state.sync_error = None;
        engine::enforce_budget(state);
        applied
    }

    /// Apply one remote change under the store's conflict policy. `Ok(true)`
    /// when the local row changed.
    fn apply_remote(state: &mut DbState, op: &Op) -> Result<bool, DbError> {
        engine::ensure_store(state, &op.store)?;
        let local = engine::read_row(state, &op.store, &op.key)?;
        let (accept, value): (bool, Option<DbValue>) = match local {
            Some((local_value, local_modified, true)) => {
                // Local dirty row vs remote change: a conflict.
                let policy = state
                    .config
                    .schema
                    .store(&op.store)
                    .map(|s| s.conflict_policy)
                    .unwrap_or(DbConflictPolicy::LastWriteWins);
                match policy {
                    DbConflictPolicy::ServerWins => (true, op.value.clone()),
                    DbConflictPolicy::ClientWins => (false, None),
                    DbConflictPolicy::LastWriteWins => {
                        if op.modified >= local_modified {
                            (true, op.value.clone())
                        } else {
                            (false, None)
                        }
                    }
                    DbConflictPolicy::Merge => {
                        let hook = state.merge_hooks.iter().find(|h| h.store.as_str() == op.store);
                        match hook {
                            Some(h) => {
                                let conflict = DbConflict {
                                    store: AzString::from(op.store.clone()),
                                    key: op.key.clone(),
                                    local: local_value.clone().unwrap_or(DbValue::Null),
                                    remote: op.value.clone().unwrap_or(DbValue::Null),
                                    local_modified_ms: local_modified,
                                    remote_modified_ms: op.modified,
                                };
                                let merged = (h.callback.cb)(h.data.clone(), conflict);
                                let merged = if merged.is_null() { None } else { Some(merged) };
                                // The merged value is a new local write: it
                                // goes back through the oplog.
                                engine::put(state, &op.store, &op.key, merged.as_ref(), now_ms())?;
                                return Ok(true);
                            }
                            None => {
                                if op.modified >= local_modified {
                                    (true, op.value.clone())
                                } else {
                                    (false, None)
                                }
                            }
                        }
                    }
                }
            }
            Some((_, local_modified, false)) if local_modified > op.modified => (false, None),
            _ => (true, op.value.clone()),
        };
        if !accept {
            return Ok(false);
        }
        engine::write_row(state, &op.store, &op.key, value.as_ref(), op.modified, false)?;
        Ok(true)
    }
}

#[cfg(all(test, feature = "db-sqlite"))]
mod tests {
    use azul_core::db::{DbIndexSchema, DbIndexSchemaVec, DbSchema, DbStoreSchema, DbStoreSchemaVec};

    use super::*;

    fn memory_db() -> Db {
        let schema = DbSchema {
            stores: DbStoreSchemaVec::from_vec(vec![DbStoreSchema {
                name: AzString::from("notes"),
                indexes: DbIndexSchemaVec::from_vec(vec![DbIndexSchema {
                    name: AzString::from("by_tag"),
                    key_path: AzString::from("tag"),
                    unique: false,
                }]),
                conflict_policy: DbConflictPolicy::LastWriteWins,
            }]),
        };
        Db::open_blocking(DbConfig::new(AzString::from(":memory:")).with_schema(schema))
            .expect("in-memory store opens")
    }

    #[test]
    fn set_get_delete_roundtrip() {
        let db = memory_db();
        assert!(db.is_open());
        let key = DbValue::Text(AzString::from("a"));
        assert!(db.set(AzString::from("notes"), key.clone(), DbValue::Integer(7)));
        let got = db.get_blocking(AzString::from("notes"), key.clone());
        assert_eq!(got.value, OptionDbValue::Some(DbValue::Integer(7)));
        assert!(db.remove(AzString::from("notes"), key.clone()));
        let got = db.get_blocking(AzString::from("notes"), key);
        assert_eq!(got.value, OptionDbValue::None);
        assert!(got.error.is_none());
    }

    #[test]
    fn iterate_orders_by_key_and_honours_range_and_limit() {
        let db = memory_db();
        for i in [3i64, 1, 2, 5, 4] {
            assert!(db.set(AzString::from("notes"), DbValue::Integer(i), DbValue::Integer(i * 10)));
        }
        let rows = db.iterate_blocking(
            AzString::from("notes"),
            DbKeyRange::between(DbValue::Integer(2), DbValue::Integer(4)),
            2,
        );
        assert!(rows.error.is_none());
        assert_eq!(rows.rows.num_rows(), 2);
        assert_eq!(rows.rows.get(0, 0), Some(&DbValue::Integer(2)));
        assert_eq!(rows.rows.get(1, 1), Some(&DbValue::Integer(30)));
    }

    #[test]
    fn index_query_uses_json_field() {
        let db = memory_db();
        let store = AzString::from("notes");
        assert!(db.set(store.clone(), DbValue::Integer(1), DbValue::Text(AzString::from("{\"tag\":\"work\"}"))));
        assert!(db.set(store.clone(), DbValue::Integer(2), DbValue::Text(AzString::from("{\"tag\":\"home\"}"))));
        assert!(db.set(store.clone(), DbValue::Integer(3), DbValue::Text(AzString::from("plain text"))));
        let rows = db.query_index_blocking(
            store,
            AzString::from("by_tag"),
            DbKeyRange::only(DbValue::Text(AzString::from("work"))),
            0,
        );
        assert!(rows.error.is_none(), "{:?}", rows.error);
        assert_eq!(rows.rows.num_rows(), 1);
        assert_eq!(rows.rows.get(0, 0), Some(&DbValue::Integer(1)));
    }

    #[test]
    fn local_only_store_reports_disconnected_and_queues_nothing() {
        let db = memory_db();
        assert!(db.set(AzString::from("notes"), DbValue::Integer(1), DbValue::Null));
        let status = db.sync_now_blocking(OptionDbScope::None);
        assert_eq!(status.state, DbSyncState::Disconnected);
        assert_eq!(status.pending_push_ops, 1);
    }

    #[test]
    fn closed_handle_is_shared_by_clones() {
        let mut db = memory_db();
        let other = db.clone();
        db.close();
        assert!(!other.is_open());
        let got = other.get_blocking(AzString::from("notes"), DbValue::Integer(1));
        assert!(got.error.is_some());
    }
}
