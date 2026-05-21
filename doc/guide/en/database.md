---
slug: database
title: SQL Database (Db)
language: en
canonical_slug: database
audience: external
maturity: beta
guide_order: 260
topic_only: false
short_desc: A bundled-SQLite handle - execute/query with typed value arrays
prerequisites: [hello-world, background-tasks]
tracked_files:
  - core/src/db.rs
  - dll/src/desktop/extra/sqlite/mod.rs
last_generated_rev: 730697e830000000000000000000000000000000
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - Db
  - DbValue
  - DbRows
  - DbValueVec
  - execute
  - query
---

# SQL Database (Db)

## Introduction

`Db` is a small SQL database handle, backed by bundled SQLite. It's the
AzulVault (P4) goal app's storage. The public API is engine-agnostic - SQL
strings plus typed value arrays - so the engine stays hidden: you bind
parameters as [`DbValue`]s and read results as a [`DbRows`] grid. Like `Pdf`
and `AudioSink` it's a C-ABI handle you keep in your own `State` (no globals).

Because SQLite calls are blocking, run heavier queries from a
[`Thread`](background-tasks.md), the same as any blocking I/O.

## Opening + querying

```rust
use azul::misc::{Db, DbValue};

let db = Db::open("app.sqlite".into());   // or ":memory:" for in-memory
assert!(db.is_open());

// CREATE / INSERT / UPDATE / DELETE -> rows affected
db.execute(
    "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT)".into(),
    DbValueVec::from_const_slice(&[]),
);
let affected = db.execute(
    "INSERT INTO notes (body) VALUES (?)".into(),
    vec![DbValue::Text("hello".into())].into(),
);

// SELECT -> a column-named, row-major grid
let rows = db.query("SELECT id, body FROM notes".into(), DbValueVec::from_const_slice(&[]));
for r in 0..rows.num_rows() {
    let id = rows.get(r, 0).and_then(|v| v.as_integer());
    let body = rows.get(r, 1).and_then(|v| v.as_text().map(|s| s.as_str().to_string()));
    // ...
}
```

## Values + results

[`DbValue`] maps onto SQLite's five storage classes:

- `Null`, `Integer(i64)`, `Real(f64)`, `Text(AzString)`, `Blob(U8Vec)`.
- Accessors: `is_null`, `as_integer`, `as_real`, `as_text`.

Bind parameters as a `DbValueVec` (positional `?` placeholders), in order.

[`DbRows`] is a flat, row-major grid: `columns` (the column names) and `values`
(all cells). Read it with `num_columns()`, `num_rows()`, and `get(row, col) ->
Option<&DbValue>` (cell `(row, col)` is `values[row * num_columns + col]`).

## Feature gating

The `Db` handle is always present (it codegen-exposes with no feature gating),
but the bundled-SQLite engine is opt-in via the `db-sqlite` Cargo feature.
Without it, `Db::open` returns an invalid handle (`is_open()` is false) and
`execute`/`query` no-op - so a build that doesn't need SQL doesn't compile the
SQLite amalgamation.

## See also

- [background-tasks](background-tasks.md) - run blocking queries off the UI thread.
- [architecture](architecture.md) - keeping the handle in your `State`.
