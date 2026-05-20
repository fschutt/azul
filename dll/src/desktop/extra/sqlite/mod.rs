//! SQLite engine backend for the `Db` API (SUPER_PLAN_2 §4 P4.3), behind
//! the `db-sqlite` feature.
//!
//! Bundled SQLite via `rusqlite` (statically compiled from C source, no
//! system libsqlite) — approach A. The public `Db` handle + `open` /
//! `execute` / `query` land in a later tick (the handle wraps a
//! `rusqlite::Connection` and lives here in the dll, like `App`, because it
//! carries an engine resource). The public surface stays engine-agnostic:
//! SQL strings + `azul_core::db::{DbValue, DbRows}`.
//!
//! This module is the engine seam — nothing here is exposed directly.

/// The bundled SQLite library version (e.g. `"3.50.0"`). Smoke-tests that
/// `rusqlite`'s bundled engine compiled + linked, and is handy for
/// diagnostics. Not part of the public api.json surface.
pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}
