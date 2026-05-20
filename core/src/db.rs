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
/// Mirrors SQLite's storage classes (Null / Integer / Real / Text / Blob)
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
    pub fn is_null(&self) -> bool {
        matches!(self, DbValue::Null)
    }
    pub fn as_integer(&self) -> Option<i64> {
        if let DbValue::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }
    pub fn as_real(&self) -> Option<f64> {
        if let DbValue::Real(r) = self {
            Some(*r)
        } else {
            None
        }
    }
    pub fn as_text(&self) -> Option<&AzString> {
        if let DbValue::Text(t) = self {
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
impl_option!(DbValue, OptionDbValue, copy = false, [Debug, Clone, PartialEq]);

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
    pub fn num_columns(&self) -> usize {
        self.columns.as_ref().len()
    }
    /// Number of result rows (`0` when there are no columns).
    pub fn num_rows(&self) -> usize {
        let cols = self.num_columns();
        if cols == 0 {
            0
        } else {
            self.values.as_ref().len() / cols
        }
    }
    /// The cell at `(row, col)`, or `None` if out of range.
    pub fn get(&self, row: usize, col: usize) -> Option<&DbValue> {
        let cols = self.num_columns();
        if col >= cols {
            return None;
        }
        self.values.as_ref().get(row * cols + col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbvalue_accessors() {
        assert!(DbValue::Null.is_null());
        assert_eq!(DbValue::Integer(7).as_integer(), Some(7));
        assert_eq!(DbValue::Real(1.5).as_real(), Some(1.5));
        assert_eq!(
            DbValue::Text(AzString::from_const_str("hi")).as_text().map(|s| s.as_str()),
            Some("hi")
        );
        // Wrong-variant accessors return None.
        assert_eq!(DbValue::Null.as_integer(), None);
        assert!(!DbValue::Integer(0).is_null());
    }

    #[test]
    fn dbrows_indexing() {
        // 2 columns × 2 rows.
        let columns = StringVec::from_vec(vec![
            AzString::from_const_str("id"),
            AzString::from_const_str("name"),
        ]);
        let values = DbValueVec::from_vec(vec![
            DbValue::Integer(1),
            DbValue::Text(AzString::from_const_str("alice")),
            DbValue::Integer(2),
            DbValue::Text(AzString::from_const_str("bob")),
        ]);
        let rows = DbRows { columns, values };

        assert_eq!(rows.num_columns(), 2);
        assert_eq!(rows.num_rows(), 2);
        assert_eq!(rows.get(0, 0).and_then(|v| v.as_integer()), Some(1));
        assert_eq!(
            rows.get(1, 1).and_then(|v| v.as_text()).map(|s| s.as_str()),
            Some("bob")
        );
        // Out-of-range column / row → None.
        assert!(rows.get(0, 2).is_none());
        assert!(rows.get(2, 0).is_none());
    }

    #[test]
    fn dbrows_empty() {
        let rows = DbRows {
            columns: StringVec::from_vec(vec![]),
            values: DbValueVec::from_vec(vec![]),
        };
        assert_eq!(rows.num_columns(), 0);
        assert_eq!(rows.num_rows(), 0);
        assert!(rows.get(0, 0).is_none());
    }
}
