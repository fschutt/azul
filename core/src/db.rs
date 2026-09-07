//! POD types for the SQL database surface (SUPER_PLAN_2 §4 P4.3).
//!
//! Engine-agnostic: the public API is SQL strings plus typed value arrays,
//! so the engine (bundled SQLite via `rusqlite`) stays fully hidden behind
//! the `db-sqlite` feature in `azul-dll`. The handle type (`Db`, wrapping a
//! `rusqlite::Connection`) lives in the dll — like `App` — because it
//! carries an engine resource; these param/result *data* types live here in
//! `azul-core` (no engine dep) so they're always present and codegen-able.
//!
//! Shape: `db.execute(sql, params: DbValueVec) -> rows_affected` and
//! `db.query(sql, params) -> DbRows`. `DbValue` maps onto SQLite's five
//! storage classes.

use alloc::vec::Vec;

use azul_css::{corety::OptionString, AzString, StringVec, U8Vec};

use crate::refany::{OptionRefAny, RefAny};

/// A single SQL value — a bound statement parameter or a result cell.
/// Mirrors `SQLite`'s storage classes (Null / Integer / Real / Text / Blob)
/// but names nothing engine-specific.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum DbValue {
    /// SQL `NULL`.
    Null,
    /// 64-bit signed integer.
    Integer(i64),
    /// 64-bit IEEE float.
    Real(f64),
    /// UTF-8 text.
    Text(AzString),
    /// Raw bytes.
    Blob(U8Vec),
}

impl DbValue {
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    #[must_use]
    pub const fn as_integer(&self) -> Option<i64> {
        if let Self::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn as_real(&self) -> Option<f64> {
        if let Self::Real(r) = self {
            Some(*r)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn as_text(&self) -> Option<&AzString> {
        if let Self::Text(t) = self {
            Some(t)
        } else {
            None
        }
    }
}

impl_vec!(
    DbValue,
    DbValueVec,
    DbValueVecDestructor,
    DbValueVecDestructorType,
    DbValueVecSlice,
    OptionDbValue
);
impl_vec_debug!(DbValue, DbValueVec);
impl_vec_clone!(DbValue, DbValueVec, DbValueVecDestructor);
impl_vec_partialeq!(DbValue, DbValueVec);
impl_option!(
    DbValue,
    OptionDbValue,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The result of `db.query(...)` — a column-named, row-major value grid.
/// Flat (not nested vectors) for a simple FFI shape: cell `(row, col)` is
/// `values[row * num_columns + col]`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbRows {
    /// Column names; `len()` is the number of columns.
    pub columns: StringVec,
    /// All cells, row-major. `len()` is `num_rows * num_columns`.
    pub values: DbValueVec,
}

impl DbRows {
    /// Number of result columns.
    #[must_use]
    pub fn num_columns(&self) -> usize {
        self.columns.as_ref().len()
    }
    /// Number of result rows (`0` when there are no columns).
    #[must_use]
    pub fn num_rows(&self) -> usize {
        let cols = self.num_columns();
        if cols == 0 {
            0
        } else {
            self.values.as_ref().len() / cols
        }
    }
    /// The cell at `(row, col)`, or `None` if out of range.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<&DbValue> {
        let cols = self.num_columns();
        if col >= cols {
            return None;
        }
        // Checked so an out-of-range `row` (whose `row * cols + col` overflows
        // usize) resolves to None instead of panicking (debug) / wrapping to a
        // real cell (release).
        let idx = row.checked_mul(cols)?.checked_add(col)?;
        self.values.as_ref().get(idx)
    }
}

// ============================================================================
// Local-first key/value store: configuration, schema, scope, sync, results
// ============================================================================
//
// The portable `Db` surface is a key/value + index store with working-set
// replication: the local store (turso-backed SQLite on desktop, IndexedDB on
// web) holds a scoped working set, writes are queued in a row-level oplog,
// and `sync_now` pushes dirty rows to an optional backup endpoint and pulls
// the remote changes back. Raw SQL is not part of the API on any target.
// These are the POD halves; the handle (`Db`) lives in the dll with the
// engine.

/// How a sync conflict (the same key changed locally and remotely since the
/// last sync) is resolved for a store.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbConflictPolicy {
    /// The newer modification wins (the default).
    LastWriteWins,
    /// The remote value always wins.
    ServerWins,
    /// The local value always wins.
    ClientWins,
    /// The store's `DbMergeCallback` (see `Db::set_on_conflict`) computes the
    /// merged value; falls back to `LastWriteWins` when none is registered.
    Merge,
}

/// A secondary index over a store.
///
/// Values are indexed when they are JSON
/// objects: `key_path` names the top-level field whose value becomes the
/// index key; values that are not JSON objects, or lack the field, are
/// simply not indexed.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIndexSchema {
    pub name: AzString,
    pub key_path: AzString,
    pub unique: bool,
}

impl_vec!(
    DbIndexSchema,
    DbIndexSchemaVec,
    DbIndexSchemaVecDestructor,
    DbIndexSchemaVecDestructorType,
    DbIndexSchemaVecSlice,
    OptionDbIndexSchema
);
impl_vec_debug!(DbIndexSchema, DbIndexSchemaVec);
impl_vec_clone!(DbIndexSchema, DbIndexSchemaVec, DbIndexSchemaVecDestructor);
impl_vec_partialeq!(DbIndexSchema, DbIndexSchemaVec);
impl_option!(
    DbIndexSchema,
    OptionDbIndexSchema,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

/// One store (collection) of a `DbSchema`: its name, indexes and the
/// conflict policy applied when syncing it.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbStoreSchema {
    pub name: AzString,
    pub indexes: DbIndexSchemaVec,
    pub conflict_policy: DbConflictPolicy,
}

impl_vec!(
    DbStoreSchema,
    DbStoreSchemaVec,
    DbStoreSchemaVecDestructor,
    DbStoreSchemaVecDestructorType,
    DbStoreSchemaVecSlice,
    OptionDbStoreSchema
);
impl_vec_debug!(DbStoreSchema, DbStoreSchemaVec);
impl_vec_clone!(DbStoreSchema, DbStoreSchemaVec, DbStoreSchemaVecDestructor);
impl_vec_partialeq!(DbStoreSchema, DbStoreSchemaVec);
impl_option!(
    DbStoreSchema,
    OptionDbStoreSchema,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The declarative schema of a database: its stores with their indexes and
/// conflict policies.
///
/// No DDL strings - the engine derives its tables from
/// this on every target. Stores not declared here are created on first
/// write, without indexes and with `LastWriteWins`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbSchema {
    pub stores: DbStoreSchemaVec,
}

impl DbSchema {
    /// A schema with no declared stores.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            stores: DbStoreSchemaVec::from_vec(Vec::new()),
        }
    }

    /// The declared schema of `store`, if any.
    #[must_use]
    pub fn store(&self, store: &str) -> Option<&DbStoreSchema> {
        self.stores.as_ref().iter().find(|s| s.name.as_str() == store)
    }
}

impl Default for DbSchema {
    fn default() -> Self {
        Self::empty()
    }
}

/// A key range: `None` bounds are unbounded, `*_open` excludes the bound
/// itself. Keys order the way `SQLite` orders values (Null < numbers < text <
/// blobs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbKeyRange {
    pub lower: OptionDbValue,
    pub upper: OptionDbValue,
    pub lower_open: bool,
    pub upper_open: bool,
}

impl DbKeyRange {
    /// Every key.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            lower: OptionDbValue::None,
            upper: OptionDbValue::None,
            lower_open: false,
            upper_open: false,
        }
    }

    /// `lower <= key <= upper`.
    #[must_use]
    pub const fn between(lower: DbValue, upper: DbValue) -> Self {
        Self {
            lower: OptionDbValue::Some(lower),
            upper: OptionDbValue::Some(upper),
            lower_open: false,
            upper_open: false,
        }
    }

    /// Exactly `key`.
    #[must_use]
    pub fn only(key: DbValue) -> Self {
        Self::between(key.clone(), key)
    }
}

impl Default for DbKeyRange {
    fn default() -> Self {
        Self::all()
    }
}

impl_option!(
    DbKeyRange,
    OptionDbKeyRange,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// A predicate over a store's secondary index: the rows whose index key
/// falls in `range`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbIndexPredicate {
    pub index: AzString,
    pub range: DbKeyRange,
}

impl_option!(
    DbIndexPredicate,
    OptionDbIndexPredicate,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The part of one store a scope covers: a key range and / or an index
/// predicate; both `None` means the whole store.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbCollectionScope {
    pub store: AzString,
    pub range: OptionDbKeyRange,
    pub index_predicate: OptionDbIndexPredicate,
}

impl DbCollectionScope {
    /// The whole `store`.
    #[must_use]
    pub const fn whole_store(store: AzString) -> Self {
        Self {
            store,
            range: OptionDbKeyRange::None,
            index_predicate: OptionDbIndexPredicate::None,
        }
    }

    /// Whether `key` in `store` falls inside this collection scope. Index
    /// predicates cannot be evaluated from the key alone and count as a
    /// match; the engine narrows them where it has the index.
    #[must_use]
    pub fn covers(&self, store: &str, key: &DbValue) -> bool {
        if self.store.as_str() != store {
            return false;
        }
        self.range
            .as_ref()
            .is_none_or(|range| range_contains(range, key))
    }
}

impl_vec!(
    DbCollectionScope,
    DbCollectionScopeVec,
    DbCollectionScopeVecDestructor,
    DbCollectionScopeVecDestructorType,
    DbCollectionScopeVecSlice,
    OptionDbCollectionScope
);
impl_vec_debug!(DbCollectionScope, DbCollectionScopeVec);
impl_vec_clone!(
    DbCollectionScope,
    DbCollectionScopeVec,
    DbCollectionScopeVecDestructor
);
impl_vec_partialeq!(DbCollectionScope, DbCollectionScopeVec);
impl_option!(
    DbCollectionScope,
    OptionDbCollectionScope,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The working set a database replicates locally: a set of collection
/// scopes. An empty scope means "everything the remote has".
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbScope {
    pub collections: DbCollectionScopeVec,
}

impl DbScope {
    /// Everything.
    #[must_use]
    pub const fn everything() -> Self {
        Self {
            collections: DbCollectionScopeVec::from_vec(Vec::new()),
        }
    }

    /// Whether `key` in `store` falls inside this scope.
    #[must_use]
    pub fn covers(&self, store: &str, key: &DbValue) -> bool {
        let collections = self.collections.as_ref();
        collections.is_empty() || collections.iter().any(|c| c.covers(store, key))
    }
}

impl Default for DbScope {
    fn default() -> Self {
        Self::everything()
    }
}

impl_option!(DbScope, OptionDbScope, copy = false, [Debug, Clone, PartialEq]);

/// Compares two values the way the store orders keys: Null < numbers < text
/// < blobs, numbers by value, text and blobs bytewise.
#[must_use]
/// Order an integer against a real the way `SQLite` does
/// (`sqlite3IntFloatCompare`), without first rounding the integer to `f64`.
///
/// `i64 as f64` is exact only up to 2^53; above that, integers that differ
/// round to the same double and would compare `Equal` — 9007199254740993 vs
/// 9007199254740992.0, say — and the DB would treat two distinct keys as one.
/// So: decide by the real's magnitude first (outside i64's range the answer
/// needs no conversion), then compare the integer against the real's
/// truncation, and only if THOSE are equal look at the real's fraction.
fn compare_int_real(i: i64, r: f64) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    // `total_cmp` semantics for NaN, matching the Real/Real arm: NaN sorts
    // after every finite value.
    if r.is_nan() {
        return Ordering::Less;
    }
    // 2^63 is exactly representable; everything at or beyond it is above
    // any i64, everything below -2^63 is below any i64.
    if r >= 9_223_372_036_854_775_808.0 {
        return Ordering::Less;
    }
    if r < -9_223_372_036_854_775_808.0 {
        return Ordering::Greater;
    }
    // In range: truncation toward zero is exact for a double this size.
    #[allow(clippy::cast_possible_truncation)] // bounds checked just above
    let r_trunc = r as i64;
    match i.cmp(&r_trunc) {
        Ordering::Equal => {
            // Same integer part. The real is bigger iff it has a positive
            // fraction, smaller iff negative; `r - trunc` is exact here
            // because both share an exponent.
            #[allow(clippy::cast_precision_loss)] // |r_trunc| == |trunc(r)| <= 2^63, and r_trunc == trunc(r) exactly, so this cast reproduces r's own integer part
            let frac = r - (r_trunc as f64);
            if frac > 0.0 {
                Ordering::Less
            } else if frac < 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        other => other,
    }
}

#[must_use]
pub fn compare_db_values(a: &DbValue, b: &DbValue) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    const fn class(v: &DbValue) -> u8 {
        match v {
            DbValue::Null => 0,
            DbValue::Integer(_) | DbValue::Real(_) => 1,
            DbValue::Text(_) => 2,
            DbValue::Blob(_) => 3,
        }
    }
    match class(a).cmp(&class(b)) {
        Ordering::Equal => {}
        other => return other,
    }
    match (a, b) {
        (DbValue::Integer(x), DbValue::Integer(y)) => x.cmp(y),
        (DbValue::Integer(x), DbValue::Real(y)) => compare_int_real(*x, *y),
        (DbValue::Real(x), DbValue::Integer(y)) => compare_int_real(*y, *x).reverse(),
        (DbValue::Real(x), DbValue::Real(y)) => x.total_cmp(y),
        (DbValue::Text(x), DbValue::Text(y)) => x.as_str().as_bytes().cmp(y.as_str().as_bytes()),
        (DbValue::Blob(x), DbValue::Blob(y)) => x.as_ref().cmp(y.as_ref()),
        _ => Ordering::Equal,
    }
}

/// Whether `key` falls inside `range` under [`compare_db_values`].
#[must_use]
pub fn range_contains(range: &DbKeyRange, key: &DbValue) -> bool {
    use core::cmp::Ordering;
    if let Some(lower) = range.lower.as_ref() {
        match compare_db_values(key, lower) {
            Ordering::Less => return false,
            Ordering::Equal if range.lower_open => return false,
            _ => {}
        }
    }
    if let Some(upper) = range.upper.as_ref() {
        match compare_db_values(key, upper) {
            Ordering::Greater => return false,
            Ordering::Equal if range.upper_open => return false,
            _ => {}
        }
    }
    true
}

/// When the runtime syncs on its own: every `interval`, and / or when the
/// app has not written to the store for a moment (`on_idle`, about 1.5 s
/// after the last write, when unpushed writes exist).
///
/// Both are driven by the runtime's per-frame pump on desktop and by the
/// host on web; every automatic sync reports through
/// `Db::set_on_sync_status` like an explicit `sync_now`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbAutoSync {
    pub interval: crate::task::OptionDuration,
    pub on_idle: bool,
}

impl DbAutoSync {
    /// No automatic sync: only explicit `sync_now` calls.
    #[must_use]
    pub const fn manual() -> Self {
        Self {
            interval: crate::task::OptionDuration::None,
            on_idle: false,
        }
    }
}

impl Default for DbAutoSync {
    fn default() -> Self {
        Self::manual()
    }
}

/// How to open a database.
///
/// Builder-style: `DbConfig::new(local_name)` plus
/// the `with_*` setters. Identical on every target; the local store is a
/// file under the app's data directory on desktop and `IndexedDB` on web.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbConfig {
    /// Name of the local store (a file name stem on desktop, the `IndexedDB`
    /// database name on web). `":memory:"` opens a throwaway in-memory store.
    pub local_name: AzString,
    /// Remote backup / sync endpoint (HTTPS). `None` = local only.
    pub backup_sync_url: OptionString,
    /// Bearer token sent with every sync request.
    pub auth_token: OptionString,
    pub schema: DbSchema,
    /// The working set replicated locally; `None` = everything.
    pub scope: OptionDbScope,
    /// Local size budget in bytes; `0` = unlimited. Clean rows are evicted
    /// under pressure, dirty (unpushed) rows never.
    pub local_budget_bytes: u64,
    pub auto_sync: DbAutoSync,
}

impl DbConfig {
    /// A local-only database called `local_name` with no schema, no scope,
    /// no budget and manual sync.
    #[must_use]
    pub const fn new(local_name: AzString) -> Self {
        Self {
            local_name,
            backup_sync_url: OptionString::None,
            auth_token: OptionString::None,
            schema: DbSchema::empty(),
            scope: OptionDbScope::None,
            local_budget_bytes: 0,
            auto_sync: DbAutoSync::manual(),
        }
    }

    #[must_use]
    pub fn with_backup_sync_url(mut self, url: AzString) -> Self {
        self.backup_sync_url = OptionString::Some(url);
        self
    }

    #[must_use]
    pub fn with_auth_token(mut self, token: AzString) -> Self {
        self.auth_token = OptionString::Some(token);
        self
    }

    #[must_use]
    pub fn with_schema(mut self, schema: DbSchema) -> Self {
        self.schema = schema;
        self
    }

    #[must_use]
    pub fn with_scope(mut self, scope: DbScope) -> Self {
        self.scope = OptionDbScope::Some(scope);
        self
    }

    #[must_use]
    pub const fn with_local_budget_bytes(mut self, bytes: u64) -> Self {
        self.local_budget_bytes = bytes;
        self
    }

    #[must_use]
    pub const fn with_auto_sync(mut self, auto: DbAutoSync) -> Self {
        self.auto_sync = auto;
        self
    }
}

/// Where a database stands with its backup endpoint.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbSyncState {
    /// No `backup_sync_url` configured, or the handle is closed.
    Disconnected,
    /// Everything pushed and pulled.
    Idle,
    Pushing,
    Pulling,
    /// Offline: local writes are queued in the oplog and pushed on the next
    /// successful sync. Never an error.
    Queued,
    /// The last sync failed for a reason other than being offline (`error`
    /// says why).
    Error,
}

/// A snapshot of a database's sync state, delivered by `Db::sync_status`,
/// `Db::sync_now` and `Db::set_on_sync_status`. Timestamps are milliseconds
/// since the Unix epoch (`0` = never).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSyncStatus {
    pub state: DbSyncState,
    /// How much of the configured scope the local working set holds, in
    /// thousandths (`1000` = complete).
    pub working_set_coverage_x1000: u32,
    /// Local operations not yet acknowledged by the backup endpoint.
    pub pending_push_ops: u64,
    pub last_synced_ms: u64,
    pub local_bytes_used: u64,
    pub local_bytes_budget: u64,
    /// Storage the platform reports as available for the local store
    /// (`navigator.storage.estimate()` on web; the data volume on desktop;
    /// `0` = unknown).
    pub quota_bytes_available: u64,
    pub error: OptionString,
}

impl DbSyncStatus {
    /// The status of a closed or engine-less handle.
    #[must_use]
    pub const fn disconnected() -> Self {
        Self {
            state: DbSyncState::Disconnected,
            working_set_coverage_x1000: 0,
            pending_push_ops: 0,
            last_synced_ms: 0,
            local_bytes_used: 0,
            local_bytes_budget: 0,
            quota_bytes_available: 0,
            error: OptionString::None,
        }
    }
}

impl_option!(
    DbSyncStatus,
    OptionDbSyncStatus,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

/// A sync conflict handed to a store's `DbMergeCallback`: the same key was
/// modified locally and remotely since the last sync. Timestamps are
/// milliseconds since the Unix epoch.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbConflict {
    pub store: AzString,
    pub key: DbValue,
    pub local: DbValue,
    pub remote: DbValue,
    pub local_modified_ms: u64,
    pub remote_modified_ms: u64,
}

/// Computes the merged value for a `DbConflict`. A pure function over the
/// two versions, invoked synchronously while a sync is applied - it must not
/// touch the UI, which is why it takes no `CallbackInfo`.
pub type DbMergeCallbackType = extern "C" fn(RefAny, DbConflict) -> DbValue;

/// Function pointer + host context for a store's merge policy
/// (`Db::set_on_conflict`).
#[repr(C)]
pub struct DbMergeCallback {
    pub cb: DbMergeCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

crate::impl_callback!(DbMergeCallback, DbMergeCallbackType);

impl DbMergeCallback {
    /// Create a merge callback from a raw function pointer (ctx = None).
    #[must_use]
    pub const fn create(cb: DbMergeCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Why a database operation failed.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorKind {
    /// The handle is closed (or `open` never succeeded).
    NotOpen,
    /// The store name is empty or not usable as a store.
    InvalidStore,
    /// The key is not usable as a key (`Null` keys are rejected).
    InvalidKey,
    /// The local store could not be read or written.
    Io,
    /// The local engine reported an error.
    Engine,
    /// This build has no local engine (`db-sqlite` feature off).
    NoEngine,
    /// The backup endpoint answered with an error.
    Sync,
    Other,
}

/// A database error: a kind plus a message.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbError {
    pub kind: DbErrorKind,
    pub message: AzString,
}

impl DbError {
    #[must_use]
    pub fn new(kind: DbErrorKind, message: impl Into<AzString>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl core::fmt::Display for DbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message.as_str())
    }
}

// ---- result structs delivered to `ResumeCallback`s ----------------------

/// Result of `Db::get`: the value (`None` when absent) or an error message.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbValueResult {
    pub value: OptionDbValue,
    pub error: OptionString,
}

impl_option!(
    DbValueResult,
    OptionDbValueResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl DbValueResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionDbValueResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of `Db::iterate` / `Db::query_index`: the matching rows as a
/// two-column grid (`key`, `value`) or an error message.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbRowsResult {
    pub rows: DbRows,
    pub error: OptionString,
}

impl_option!(
    DbRowsResult,
    OptionDbRowsResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl DbRowsResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionDbRowsResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// One change delivered to a `Db::subscribe` callback: `value` is `None`
/// when the key was deleted.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbChangeResult {
    pub store: AzString,
    pub key: DbValue,
    pub value: OptionDbValue,
}

impl_option!(
    DbChangeResult,
    OptionDbChangeResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl DbChangeResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionDbChangeResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of `Db::sync_now` and every `Db::set_on_sync_status` delivery.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSyncStatusResult {
    pub status: DbSyncStatus,
}

impl_option!(
    DbSyncStatusResult,
    OptionDbSyncStatusResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl DbSyncStatusResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionDbSyncStatusResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

#[cfg(test)]
#[path = "db_test.rs"]
mod db_test;
