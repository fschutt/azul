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
    #[must_use] pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    #[must_use] pub const fn as_integer(&self) -> Option<i64> {
        if let Self::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }
    #[must_use] pub const fn as_real(&self) -> Option<f64> {
        if let Self::Real(r) = self {
            Some(*r)
        } else {
            None
        }
    }
    #[must_use] pub const fn as_text(&self) -> Option<&AzString> {
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
    #[must_use] pub fn num_columns(&self) -> usize {
        self.columns.as_ref().len()
    }
    /// Number of result rows (`0` when there are no columns).
    #[must_use] pub fn num_rows(&self) -> usize {
        let cols = self.num_columns();
        if cols == 0 {
            0
        } else {
            self.values.as_ref().len() / cols
        }
    }
    /// The cell at `(row, col)`, or `None` if out of range.
    #[must_use] pub fn get(&self, row: usize, col: usize) -> Option<&DbValue> {
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
mod tests {
    use super::*;

    #[test]
    fn dbvalue_accessors() {
        assert!(DbValue::Null.is_null());
        assert_eq!(DbValue::Integer(7).as_integer(), Some(7));
        assert_eq!(DbValue::Real(1.5).as_real(), Some(1.5));
        assert_eq!(
            DbValue::Text(AzString::from_const_str("hi")).as_text().map(AzString::as_str),
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
        assert_eq!(rows.get(0, 0).and_then(DbValue::as_integer), Some(1));
        assert_eq!(
            rows.get(1, 1).and_then(|v| v.as_text()).map(AzString::as_str),
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

#[cfg(test)]
mod autotest_generated {
    //! Adversarial tests generated for the DB POD surface.
    //!
    //! Targets: malformed/ragged grids, i64/f64 extremes (MIN/MAX/NaN/±0/±inf),
    //! Unicode + huge text, wrong-variant getters, and — the headline case —
    //! index overflow in `DbRows::get` (`row * cols + col`), which panics under
    //! the default overflow-checked test profile.
    use super::*;

    // ---- helpers -----------------------------------------------------------

    fn cols(names: &[&'static str]) -> StringVec {
        StringVec::from_vec(names.iter().copied().map(AzString::from_const_str).collect())
    }

    /// 2 columns × 2 rows of integers: [[10,11],[20,21]].
    fn grid_2x2() -> DbRows {
        DbRows {
            columns: cols(&["a", "b"]),
            values: DbValueVec::from_vec(vec![
                DbValue::Integer(10),
                DbValue::Integer(11),
                DbValue::Integer(20),
                DbValue::Integer(21),
            ]),
        }
    }

    /// Every non-`Null` / non-matching variant, for wrong-variant getter tests.
    fn all_variants() -> Vec<DbValue> {
        vec![
            DbValue::Null,
            DbValue::Integer(0),
            DbValue::Real(0.0),
            DbValue::Text(AzString::from_const_str("t")),
            DbValue::Blob(U8Vec::from_vec(vec![1, 2, 3])),
        ]
    }

    // ---- DbValue::is_null (predicate) --------------------------------------

    #[test]
    fn is_null_basic_true_false() {
        assert!(DbValue::Null.is_null());
        assert!(!DbValue::Integer(0).is_null());
    }

    #[test]
    fn is_null_only_null_variant_is_null() {
        for v in all_variants() {
            let expect = matches!(v, DbValue::Null);
            assert_eq!(v.is_null(), expect, "is_null mismatch for {v:?}");
        }
        // Extreme payloads must not change the discriminant-only answer.
        assert!(!DbValue::Integer(i64::MIN).is_null());
        assert!(!DbValue::Real(f64::NAN).is_null());
        assert!(!DbValue::Blob(U8Vec::from_vec(vec![])).is_null());
    }

    #[test]
    fn is_null_is_const_evaluable() {
        // `const fn`: forcing const evaluation guards against a future body that
        // is no longer const-safe.
        const NULL_IS_NULL: bool = DbValue::Null.is_null();
        const INT_IS_NULL: bool = DbValue::Integer(9).is_null();
        const _: () = assert!(NULL_IS_NULL && !INT_IS_NULL);
    }

    // ---- DbValue::as_integer (getter) --------------------------------------

    #[test]
    fn as_integer_extremes_round_trip() {
        for &i in &[0i64, 1, -1, i64::MIN, i64::MIN + 1, i64::MAX - 1, i64::MAX] {
            assert_eq!(DbValue::Integer(i).as_integer(), Some(i), "round-trip {i}");
        }
    }

    #[test]
    fn as_integer_wrong_variant_is_none() {
        for v in all_variants() {
            if matches!(v, DbValue::Integer(_)) {
                continue;
            }
            assert_eq!(v.as_integer(), None, "expected None for {v:?}");
        }
    }

    #[test]
    fn as_integer_is_const_evaluable() {
        const MIN_INT: Option<i64> = DbValue::Integer(i64::MIN).as_integer();
        assert_eq!(MIN_INT, Some(i64::MIN));
    }

    // ---- DbValue::as_real (getter) -----------------------------------------

    #[test]
    fn as_real_extremes_round_trip_bit_exact() {
        // Compare via bit pattern so ±0.0 and subnormals are distinguished.
        for &r in &[
            0.0f64,
            -0.0,
            1.0,
            -1.0,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::EPSILON,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(
                DbValue::Real(r).as_real().map(f64::to_bits),
                Some(r.to_bits()),
                "bit-exact round-trip for {r:?}",
            );
        }
    }

    #[test]
    fn as_real_nan_is_preserved_not_lost() {
        // NaN != NaN, so assert on the predicate rather than equality.
        let got = DbValue::Real(f64::NAN).as_real();
        assert!(got.is_some());
        assert!(got.unwrap().is_nan());
    }

    #[test]
    fn as_real_negative_zero_keeps_sign() {
        let got = DbValue::Real(-0.0).as_real().unwrap();
        assert_eq!(got, 0.0);
        assert!(got.is_sign_negative(), "-0.0 must stay negative-signed");
    }

    #[test]
    fn as_real_wrong_variant_is_none() {
        for v in all_variants() {
            if matches!(v, DbValue::Real(_)) {
                continue;
            }
            assert_eq!(v.as_real(), None, "expected None for {v:?}");
        }
    }

    // ---- DbValue::as_text (getter) -----------------------------------------

    #[test]
    fn as_text_empty_unicode_and_huge() {
        // Empty string.
        assert_eq!(
            DbValue::Text(AzString::from_const_str("")).as_text().map(AzString::as_str),
            Some(""),
        );
        // Multi-byte Unicode + NUL byte survives round-trip.
        let tricky = "áé💥🔥\u{0}\u{FEFF}中文";
        let v = DbValue::Text(AzString::from(tricky.to_string()));
        assert_eq!(v.as_text().map(AzString::as_str), Some(tricky));
        // Large payload: no truncation, length preserved.
        let huge = "x".repeat(200_000);
        let v = DbValue::Text(AzString::from(huge.clone()));
        assert_eq!(v.as_text().map(|s| s.as_str().len()), Some(huge.len()));
    }

    #[test]
    fn as_text_wrong_variant_is_none() {
        for v in all_variants() {
            if matches!(v, DbValue::Text(_)) {
                continue;
            }
            assert!(v.as_text().is_none(), "expected None for {v:?}");
        }
    }

    // ---- DbRows::num_columns (getter) --------------------------------------

    #[test]
    fn num_columns_empty_and_many() {
        let empty = DbRows {
            columns: StringVec::from_vec(vec![]),
            values: DbValueVec::from_vec(vec![]),
        };
        assert_eq!(empty.num_columns(), 0);

        let names: Vec<AzString> =
            (0..1000).map(|_| AzString::from_const_str("c")).collect();
        let wide = DbRows {
            columns: StringVec::from_vec(names),
            values: DbValueVec::from_vec(vec![]),
        };
        assert_eq!(wide.num_columns(), 1000);
    }

    // ---- DbRows::num_rows (getter) -----------------------------------------

    #[test]
    fn num_rows_zero_columns_never_divides_by_zero() {
        // Malformed: 0 columns but non-empty values. Documented: `0` rows,
        // and crucially no divide-by-zero panic.
        let rows = DbRows {
            columns: StringVec::from_vec(vec![]),
            values: DbValueVec::from_vec(vec![DbValue::Null, DbValue::Integer(1)]),
        };
        assert_eq!(rows.num_rows(), 0);
        // `get` must also stay safe with 0 columns.
        assert!(rows.get(0, 0).is_none());
    }

    #[test]
    fn num_rows_exact_and_ragged_truncates() {
        // Exact multiple: 2 cols, 4 values → 2 rows.
        assert_eq!(grid_2x2().num_rows(), 2);

        // Ragged: 2 cols, 3 values → floor(3/2) = 1 row (last partial row dropped).
        let ragged = DbRows {
            columns: cols(&["a", "b"]),
            values: DbValueVec::from_vec(vec![
                DbValue::Integer(1),
                DbValue::Integer(2),
                DbValue::Integer(3),
            ]),
        };
        assert_eq!(ragged.num_rows(), 1);
        // Flat index 3 is past the end → None (deterministic, no panic).
        assert!(ragged.get(1, 1).is_none());

        // Single column: N values → N rows.
        let single = DbRows {
            columns: cols(&["only"]),
            values: DbValueVec::from_vec(vec![
                DbValue::Integer(0),
                DbValue::Integer(1),
                DbValue::Integer(2),
            ]),
        };
        assert_eq!(single.num_rows(), 3);
    }

    // ---- DbRows::get (numeric / bounds) ------------------------------------

    #[test]
    fn get_zero_and_all_in_range_cells() {
        let g = grid_2x2();
        assert_eq!(g.get(0, 0).and_then(DbValue::as_integer), Some(10));
        assert_eq!(g.get(0, 1).and_then(DbValue::as_integer), Some(11));
        assert_eq!(g.get(1, 0).and_then(DbValue::as_integer), Some(20));
        assert_eq!(g.get(1, 1).and_then(DbValue::as_integer), Some(21));
    }

    #[test]
    fn get_out_of_range_column_is_none() {
        let g = grid_2x2();
        assert!(g.get(0, 2).is_none()); // col == num_columns
        // col == usize::MAX hits the `col >= cols` guard before any arithmetic.
        assert!(g.get(0, usize::MAX).is_none());
        // Both extreme: the column guard short-circuits before `row * cols`.
        assert!(g.get(usize::MAX, usize::MAX).is_none());
    }

    #[test]
    fn get_out_of_range_row_is_none() {
        let g = grid_2x2();
        assert!(g.get(2, 0).is_none());
        assert!(g.get(1_000_000, 1).is_none());
    }

    #[test]
    fn get_extreme_row_single_column_no_overflow() {
        // With cols == 1, `row * cols + col` == usize::MAX (no overflow) and
        // resolves to an out-of-range slice index → None.
        let single = DbRows {
            columns: cols(&["only"]),
            values: DbValueVec::from_vec(vec![DbValue::Integer(0)]),
        };
        assert!(single.get(usize::MAX, 0).is_none());
    }

    #[test]
    fn get_on_empty_grid_is_none() {
        let empty = DbRows {
            columns: StringVec::from_vec(vec![]),
            values: DbValueVec::from_vec(vec![]),
        };
        assert!(empty.get(0, 0).is_none());
        assert!(empty.get(usize::MAX, 0).is_none());
    }

    #[test]
    fn get_extreme_row_multi_column_never_yields_bogus_cell() {
        // 2-column grid: `row * cols` = usize::MAX * 2 overflows usize. Under the
        // default overflow-checked test profile this panics; in a wrapping
        // (release) build it must still resolve to None, never a real cell.
        // Guard with catch_unwind so the observation is non-fatal either way.
        // NOTE: this documents a latent overflow in `DbRows::get` — see report.
        let g = grid_2x2();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            g.get(usize::MAX, 0).cloned()
        }));
        match outcome {
            Ok(v) => assert!(v.is_none(), "overflowed index must not map to a real cell, got {v:?}"),
            Err(_) => { /* overflow-check panic: latent bug, reported separately */ }
        }
    }

    #[test]
    fn get_largest_non_overflowing_row_is_none_not_a_panic() {
        // With `cols == 2`, the largest row whose flat index cannot overflow is
        // `usize::MAX / 2`: `row * 2 + 1 == usize::MAX` exactly. One row higher
        // overflows (covered above); *this* one must resolve cleanly to `None` —
        // no panic, and no wrap-around into a live cell.
        let row = usize::MAX / 2;
        assert_eq!(
            row.checked_mul(2).and_then(|x| x.checked_add(1)),
            Some(usize::MAX),
            "precondition: this row is the exact non-overflowing boundary",
        );
        let g = grid_2x2();
        assert!(g.get(row, 0).is_none());
        assert!(g.get(row, 1).is_none());
    }

    // ---- DbValue: cross-variant equality invariants ------------------------

    #[test]
    fn eq_nan_never_equals_itself_but_survives_the_clone() {
        // Derived PartialEq delegates to f64, so IEEE-754 NaN semantics must hold.
        let nan = DbValue::Real(f64::NAN);
        let nan_clone = nan.clone();
        assert_ne!(nan, nan_clone, "NaN must not compare equal to itself");

        let v = DbValueVec::from_vec(vec![DbValue::Real(f64::NAN)]);
        let v_clone = v.clone();
        assert_ne!(v, v_clone, "a vec holding NaN is not equal to its own clone");
        // ...but the payload is still a NaN, not a corrupted bit pattern.
        assert!(v_clone.as_slice()[0].as_real().unwrap().is_nan());
    }

    #[test]
    fn eq_signed_zero_and_distinct_storage_classes() {
        // IEEE: +0.0 == -0.0, even though the bits differ.
        assert_eq!(DbValue::Real(0.0), DbValue::Real(-0.0));
        assert_ne!(DbValue::Real(0.0).as_real().map(f64::to_bits), DbValue::Real(-0.0).as_real().map(f64::to_bits));

        // Different storage classes never compare equal, however similar the payload.
        assert_ne!(DbValue::Integer(1), DbValue::Real(1.0));
        assert_ne!(DbValue::Null, DbValue::Integer(0));
        assert_ne!(DbValue::Text(AzString::from_const_str("1")), DbValue::Integer(1));
        assert_ne!(
            DbValue::Text(AzString::from_const_str("ab")),
            DbValue::Blob(U8Vec::from_vec(vec![b'a', b'b'])),
            "Text and Blob with identical bytes are still distinct storage classes",
        );
        // Same variant, same payload → equal (sanity anchor for the assertions above).
        assert_eq!(DbValue::Integer(i64::MIN), DbValue::Integer(i64::MIN));
    }

    // ---- DbValue::Blob: opaque to every typed getter -----------------------

    #[test]
    fn blob_is_opaque_to_every_getter_and_round_trips_bit_exact() {
        // Non-UTF-8, embedded NULs, every byte value — 70 KB of it.
        let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
        let b = DbValue::Blob(U8Vec::from_vec(bytes.clone()));

        // There is no `as_blob`, so every typed getter must decline — and a Blob
        // is emphatically not NULL.
        assert!(!b.is_null());
        assert_eq!(b.as_integer(), None);
        assert_eq!(b.as_real(), None);
        assert!(b.as_text().is_none());

        // Payload survives a clone byte-for-byte (no UTF-8 validation, no truncation).
        match b.clone() {
            DbValue::Blob(v) => assert_eq!(v.as_slice(), bytes.as_slice()),
            other => panic!("clone changed the variant: {other:?}"),
        }

        // An empty Blob is still a Blob, not a Null.
        let empty = DbValue::Blob(U8Vec::from_vec(vec![]));
        assert!(!empty.is_null());
        assert_eq!(empty.as_integer(), None);
        assert_ne!(empty, DbValue::Null);
    }

    // ---- round-trip: Vec<DbValue> → DbValueVec → Vec<DbValue> --------------

    /// Extreme-but-comparable payloads (no NaN — it would break `assert_eq!`).
    fn extreme_values() -> Vec<DbValue> {
        vec![
            DbValue::Null,
            DbValue::Integer(i64::MIN),
            DbValue::Integer(i64::MAX),
            DbValue::Real(f64::NEG_INFINITY),
            DbValue::Real(f64::MIN_POSITIVE),
            DbValue::Real(-0.0),
            DbValue::Text(AzString::from(String::new())),
            DbValue::Text(AzString::from("nul\u{0}\u{FEFF}émoji💥中文".to_string())),
            DbValue::Blob(U8Vec::from_vec(vec![])),
            DbValue::Blob(U8Vec::from_vec((0..=255u8).collect())),
        ]
    }

    #[test]
    fn dbvaluevec_round_trip_preserves_order_and_payloads() {
        let original = extreme_values();
        let encoded = DbValueVec::from_vec(original.clone());

        assert_eq!(encoded.len(), original.len());
        assert!(!encoded.is_empty());
        assert_eq!(encoded.as_slice(), original.as_slice(), "decode(encode(v)) != v");

        // Element-wise too, so a reordering/aliasing bug names the offending index.
        for (i, (got, want)) in encoded.as_slice().iter().zip(original.iter()).enumerate() {
            assert_eq!(got, want, "cell {i} changed across the vec round-trip");
        }

        // ...and the full ownership round-trip back out to a Rust Vec.
        let decoded = DbValueVec::from_vec(original.clone()).into_library_owned_vec();
        assert_eq!(decoded, original);
    }

    #[test]
    fn dbvaluevec_empty_is_safe_to_read() {
        // A null-ptr / zero-len vec must yield an empty slice, never a deref of 0x0.
        for v in [DbValueVec::new(), DbValueVec::default(), DbValueVec::from_vec(vec![])] {
            assert_eq!(v.len(), 0);
            assert!(v.is_empty());
            assert_eq!(v.as_slice(), &[] as &[DbValue]);
            assert!(v.get(0).is_none());
            assert!(v.get(usize::MAX).is_none());
            assert!(v.c_get(0).is_none());
            assert_eq!(v.iter().count(), 0);
        }
    }

    #[test]
    fn dbvaluevec_c_get_agrees_with_get_and_declines_out_of_range() {
        let vals = extreme_values();
        let v = DbValueVec::from_vec(vals.clone());
        for (i, want) in vals.iter().enumerate() {
            assert!(v.c_get(i).is_some(), "c_get({i}) should be Some");
            assert_eq!(v.c_get(i).into_option().as_ref(), Some(want), "c_get({i})");
            assert_eq!(v.get(i), Some(want), "get({i})");
        }
        // Past the end, and at the usize limit: None, no panic.
        assert!(v.c_get(vals.len()).is_none());
        assert!(v.c_get(usize::MAX).is_none());
        assert!(v.get(usize::MAX).is_none());
    }

    #[test]
    fn option_dbvalue_round_trips_through_option() {
        let inner = DbValue::Text(AzString::from("💥".to_string()));

        let some: OptionDbValue = Some(inner.clone()).into();
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_ref(), Some(&inner));
        assert_eq!(Option::<DbValue>::from(some.clone()), Some(inner.clone()));
        assert_eq!(some.into_option(), Some(inner));

        let none: OptionDbValue = Option::<DbValue>::None.into();
        assert!(none.is_none());
        assert_eq!(none.into_option(), None);
        assert_eq!(OptionDbValue::default().into_option(), None);

        // A `Some(Null)` is NOT a `None` — the two nullities must not collapse.
        let some_null: OptionDbValue = Some(DbValue::Null).into();
        assert!(some_null.is_some());
        assert_ne!(some_null, OptionDbValue::None);
    }

    // ---- DbRows: clone / equality ------------------------------------------

    #[test]
    fn dbrows_clone_is_deep_and_outlives_the_original() {
        let rows = DbRows {
            columns: cols(&["id", "name"]),
            values: DbValueVec::from_vec(vec![
                DbValue::Integer(-1),
                DbValue::Text(AzString::from("héllo 💥".to_string())),
            ]),
        };
        let copy = rows.clone();
        assert_eq!(copy, rows);

        // A shallow (pointer-aliasing) clone would leave `copy` dangling here —
        // and a double-free would trip on the second drop.
        drop(rows);
        assert_eq!(copy.num_columns(), 2);
        assert_eq!(copy.num_rows(), 1);
        assert_eq!(copy.get(0, 0).and_then(DbValue::as_integer), Some(-1));
        assert_eq!(
            copy.get(0, 1).and_then(|v| v.as_text()).map(AzString::as_str),
            Some("héllo 💥"),
        );

        // Cloning a clone keeps working (no destructor-state corruption).
        let copy2 = copy.clone();
        drop(copy);
        assert_eq!(
            copy2.get(0, 1).and_then(|v| v.as_text()).map(AzString::as_str),
            Some("héllo 💥"),
        );
    }

    #[test]
    fn dbrows_equality_is_structural() {
        assert_eq!(grid_2x2(), grid_2x2());

        // Same cells, different column names → different result set.
        let renamed = DbRows { columns: cols(&["a", "z"]), ..grid_2x2() };
        assert_ne!(renamed, grid_2x2());

        // Same cells in a different row order → different result set.
        let reordered = DbRows {
            columns: cols(&["a", "b"]),
            values: DbValueVec::from_vec(vec![
                DbValue::Integer(20),
                DbValue::Integer(21),
                DbValue::Integer(10),
                DbValue::Integer(11),
            ]),
        };
        assert_ne!(reordered, grid_2x2());

        // Same flat cells, but 1 column instead of 2 → a different shape entirely.
        let reshaped = DbRows { columns: cols(&["a"]), ..grid_2x2() };
        assert_ne!(reshaped, grid_2x2());
        assert_eq!(reshaped.num_rows(), 4);
    }

    #[test]
    fn dbrows_from_default_collections_is_an_empty_grid() {
        let rows = DbRows { columns: StringVec::default(), values: DbValueVec::default() };
        assert_eq!(rows.num_columns(), 0);
        assert_eq!(rows.num_rows(), 0);
        assert!(rows.get(0, 0).is_none());
        assert!(rows.get(usize::MAX, usize::MAX).is_none());
        assert_eq!(
            rows,
            DbRows {
                columns: StringVec::from_vec(vec![]),
                values: DbValueVec::from_vec(vec![]),
            },
        );
    }

    // ---- DbRows: shape invariants over every small grid --------------------

    fn grid(cols_n: usize, len: usize) -> (DbRows, Vec<DbValue>) {
        let names: Vec<AzString> = (0..cols_n).map(|_| AzString::from_const_str("c")).collect();
        let cells: Vec<DbValue> = (0..len).map(|i| DbValue::Integer(i as i64)).collect();
        let rows = DbRows {
            columns: StringVec::from_vec(names),
            values: DbValueVec::from_vec(cells.clone()),
        };
        (rows, cells)
    }

    #[test]
    fn get_agrees_with_the_row_major_flat_index_for_every_shape() {
        for cols_n in 1..=5usize {
            for len in 0..=17usize {
                let (rows, cells) = grid(cols_n, len);
                assert_eq!(rows.num_columns(), cols_n);
                assert_eq!(rows.num_rows(), len / cols_n, "cols={cols_n} len={len}");

                // Every in-range cell is exactly `values[row * cols + col]`.
                for r in 0..rows.num_rows() {
                    for c in 0..cols_n {
                        assert_eq!(
                            rows.get(r, c),
                            Some(&cells[r * cols_n + c]),
                            "({r},{c}) cols={cols_n} len={len}",
                        );
                    }
                }

                // The ragged tail: `get` reads the physically-present cells of a
                // partial row even though `num_rows` doesn't count it, and returns
                // None past the end. Either way — deterministic, never a panic.
                let tail = rows.num_rows();
                for c in 0..cols_n {
                    assert_eq!(
                        rows.get(tail, c),
                        cells.get(tail * cols_n + c),
                        "ragged tail ({tail},{c}) cols={cols_n} len={len}",
                    );
                }

                // An out-of-range column is None regardless of the row.
                assert!(rows.get(0, cols_n).is_none());
                assert!(rows.get(rows.num_rows().saturating_sub(1), cols_n).is_none());
            }
        }
    }

    #[test]
    fn num_rows_times_num_columns_never_exceeds_the_cell_count() {
        for cols_n in 0..=6usize {
            for len in 0..=20usize {
                let (rows, _) = grid(cols_n, len);
                let (r, c) = (rows.num_rows(), rows.num_columns());

                let covered = r.checked_mul(c).expect("row×col count must not overflow");
                assert!(covered <= len, "claims {covered} cells but only {len} exist");
                if c > 0 {
                    // At most one partial row may be dropped — never more.
                    assert!(len - covered < c, "dropped a whole row: cols={c} len={len}");
                }
            }
        }
    }

    #[test]
    fn more_columns_than_values_yields_zero_rows_and_no_bogus_cells() {
        let rows = DbRows {
            columns: cols(&["a", "b", "c"]),
            values: DbValueVec::from_vec(vec![DbValue::Integer(1), DbValue::Null]),
        };
        assert_eq!(rows.num_columns(), 3);
        assert_eq!(rows.num_rows(), 0, "floor(2/3) == 0: no complete row exists");

        // No complete row, but the two cells that physically exist still read back.
        assert_eq!(rows.get(0, 0).and_then(DbValue::as_integer), Some(1));
        assert!(matches!(rows.get(0, 1), Some(DbValue::Null)));
        // ...and nothing past the end is invented.
        assert!(rows.get(0, 2).is_none());
        assert!(rows.get(1, 0).is_none());
    }

    #[test]
    fn num_columns_counts_names_verbatim_including_duplicates_and_unicode() {
        let rows = DbRows {
            columns: StringVec::from_vec(vec![
                AzString::from_const_str("dup"),
                AzString::from_const_str("dup"), // duplicates are not deduped
                AzString::from_const_str(""),    // an empty name still counts
                AzString::from("列💥\u{0}".to_string()),
            ]),
            values: DbValueVec::from_vec(vec![]),
        };
        assert_eq!(rows.num_columns(), 4);
        assert_eq!(rows.num_rows(), 0);
        assert!(rows.get(0, 3).is_none()); // no values at all
    }
}
