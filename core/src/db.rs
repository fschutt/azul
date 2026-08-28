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

use azul_css::{AzString, StringVec, U8Vec};

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


#[cfg(test)]
#[path = "db_test.rs"]
mod db_test;
