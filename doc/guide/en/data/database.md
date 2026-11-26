---
slug: data/database
title: Local-first Database
language: en
canonical_slug: data/database
audience: external
maturity: beta
guide_order: 210
short_desc: A local-first key/value + index store that syncs to a backup endpoint
prerequisites: [hello-world, data/background-tasks]
tracked_files:
  - core/src/db.rs
  - dll/src/desktop/extra/sqlite/mod.rs
last_generated_rev: 730697e830000000000000000000000000000000
generated_at: 2026-09-07T00:00:00Z
default-search-keys:
  - Db
  - DbConfig
  - DbValue
  - DbRows
  - DbKeyRange
  - DbScope
  - sync_now
---

# Local-first Database

## Introduction

`Db` is a local-first key/value + index store. The local store holds a
*working set* of your data (a file under the app's data directory on desktop,
IndexedDB in the browser); every write lands there immediately and is queued
in a row-level oplog; `sync_now` pushes the queued rows to an optional backup
endpoint and pulls the remote changes back. Being offline is a normal state
(`Queued`), never an error.

The surface is identical on every target and it is not SQL: desktop keeps a
pure-Rust SQLite engine as a hidden implementation detail and the browser
uses its own database, so no engine is ever shipped to the web. Reads are
*requests* that resume a callback (the browser's storage is asynchronous),
writes are fire-and-forget, and every result struct has a static
`downcast(result)` accessor. Like `Pdf` and `AudioSink` the handle is a
C-ABI value you keep in your own `State`; it is reference counted, so clones
share the store.

## Opening + reading

```rust,ignore
use azul::db::{Db, DbConfig, DbKeyRange, DbOpenResult, DbRowsResult, DbValue, DbValueResult};
use azul::error::ResultDbDbError;

extern "C" fn on_start(data: RefAny, _info: CallbackInfo) -> Update {
    // A local-only store called "notes"; `:memory:` would be throwaway.
    let config = DbConfig::create("notes")
        .with_backup_sync_url("https://sync.example.org/notes") // optional
        .with_auth_token("...");
    let _request = Db::open(config, data, on_opened);
    Update::DoNothing
}

extern "C" fn on_opened(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(opened) = DbOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let db = match opened.result {
        ResultDbDbError::Ok(db) => db,
        ResultDbDbError::Err(e) => {
            eprintln!("open failed: {}", e.message.as_str());
            return Update::DoNothing;
        }
    };
    // Fire-and-forget writes: visible to the next read immediately.
    db.set("notes", DbValue::Integer(1), DbValue::Text("hello".into()));
    // Reads resume a callback with a result struct.
    let _request = db.iterate("notes", DbKeyRange::all(), 0, data.clone(), on_rows);
    if let Some(mut state) = data.downcast_mut::<MyState>() {
        state.db = Some(db);
    }
    Update::DoNothing
}

extern "C" fn on_rows(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(answer) = DbRowsResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    // `rows` is a two-column grid: `key`, `value`.
    for r in 0..answer.rows.num_rows() {
        let _key = answer.rows.get(r, 0);
        let _value = answer.rows.get(r, 1);
    }
    Update::RefreshDom
}
```

The request functions and the result struct each one resumes with:

| request | result struct |
|---|---|
| `Db::open(config, data, on_open)` | `DbOpenResult { result: ResultDbDbError }` |
| `db.get(store, key, data, on_result)` | `DbValueResult { value: OptionDbValue, error }` |
| `db.iterate(store, range, limit, data, on_result)` | `DbRowsResult { rows: DbRows, error }` |
| `db.query_index(store, index, range, limit, data, on_result)` | `DbRowsResult` |
| `db.subscribe(scope, data, on_change)` | `DbChangeResult { store, key, value }`, repeatedly |
| `db.sync_now(scope, data, on_result)` | `DbSyncStatusResult { status: DbSyncStatus }` |

`db.set(store, key, value)` and `db.remove(store, key)` return `bool` and
never block; `db.sync_status()` is a synchronous snapshot.

## Values, keys and ranges

[`DbValue`] is `Null`, `Integer(i64)`, `Real(f64)`, `Text(AzString)` or
`Blob(U8Vec)`, with `is_null` / `as_integer` / `as_real` / `as_text`
accessors. Keys are any non-`Null` value and order the way SQLite orders
values: `Null` < numbers < text < blobs. A [`DbKeyRange`] bounds a scan:
`DbKeyRange::all()`, `between(lower, upper)`, `only(key)`, or the struct
itself with `lower_open` / `upper_open` for exclusive bounds.

Results come back as a [`DbRows`] grid with the two columns `key` and
`value`: `num_rows()`, `num_columns()` and `get(row, col)`.

## Schema and indexes

Stores are created on first write. To get secondary indexes and per-store
conflict policies, declare them up front:

```rust,ignore
use azul::db::{DbConflictPolicy, DbIndexSchema, DbSchema, DbStoreSchema};

let schema = DbSchema {
    stores: vec![DbStoreSchema {
        name: "notes".into(),
        indexes: vec![DbIndexSchema {
            name: "by_tag".into(),
            key_path: "tag".into(),   // a top-level field of a JSON object value
            unique: false,
        }].into(),
        conflict_policy: DbConflictPolicy::LastWriteWins,
    }].into(),
};
let config = DbConfig::create("notes").with_schema(schema);
```

Values are indexed when they are JSON objects: `key_path` names the field
whose value becomes the index key. `db.query_index("notes", "by_tag",
DbKeyRange::only(DbValue::Text("work".into())), 0, data, on_rows)` then
returns the matching rows in index-key order.

## Sync, scope and budget

Sync is explicit: `sync_now` pushes every dirty row, then refreshes the
given (or the configured) scope from the endpoint. The [`DbSyncStatus`] it
resumes with reports `state` (`Disconnected` without an endpoint, `Idle`,
`Pushing`, `Pulling`, `Queued` while offline, `Error`), how many operations
still wait to be pushed, when the last sync completed, how many bytes the
local store uses against its budget, and how much of the scope the working
set covers. `set_on_sync_status` delivers the same struct after every sync.

Local stores are size-constrained (browser quotas), so the portable model is
working-set replication rather than full copies:

- `with_scope(DbScope { collections })` limits what is pulled to a set of
  [`DbCollectionScope`]s (a store, optionally a key range or an index
  predicate). `subscribe(scope, ...)` extends the working set and reports
  every change inside it, local or pulled.
- `with_local_budget_bytes(n)` caps the local store. Under pressure *clean*
  rows are evicted oldest-first; dirty (unpushed) rows are never evicted
  before a successful push.
- Conflicts (a key changed locally and remotely since the last sync) follow
  the store's [`DbConflictPolicy`]: `LastWriteWins` (default),
  `ServerWins`, `ClientWins`, or `Merge` with a merge function installed via
  `set_on_conflict(store, data, on_merge)`. The merge function is a pure
  `fn(RefAny, DbConflict) -> DbValue` (return `Null` to delete) invoked
  while the sync is applied; it must not touch the UI.

The wire protocol is a row-level oplog over HTTPS (`POST <url>/push` with
the queued operations as JSON, `GET <url>/pull?db=..&since=<cursor>` for
the remote changes), so a thin server in front of any store can serve it;
the browser host speaks the same protocol through `fetch()`.

## Feature gating

The `Db` handle is always present, but the desktop engine is opt-in via the
`db-sqlite` Cargo feature (part of `build-dll`) and sync needs the `http`
feature. Without an engine `Db::open` resumes with
`DbErrorKind::NoEngine`; without `http`, `sync_now` reports `Error` naming
the missing feature. `PlatformCapability::sql()` and `::sync()` answer these
questions at runtime.

## See also

- [background-tasks](background-tasks.md) - timers, for driving `sync_now`
  periodically on desktop.
- [architecture](../architecture.md) - keeping the handle in your `State`.
