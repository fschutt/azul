//! Regression repro for the `CssPropertyCachePtr` codegen double-drop (issue #15).
//!
//! `drop_in_place::<AzStyledDom>` runs the wrapper's `Drop` (-> `AzStyledDom_delete`
//! -> `drop_in_place` of the *real* `StyledDom`, which frees the `CssPropertyCache`
//! `Box`) AND THEN the compiler-generated drop-glue drops the wrapper's own
//! `css_property_cache: AzCssPropertyCachePtr` field, whose `Drop` re-runs
//! `_delete` -> `drop_in_place::<CssPropertyCachePtr>` on the SAME bytes.
//!
//! Before the `ManuallyDrop` + `run_destructor`-gating fix in
//! `core/src/prop_cache.rs`, that freed the owned `Box<CssPropertyCache>` twice ->
//! double free. Looping many times means a double-free corrupts the allocator and
//! aborts (`free(): double free detected` / SIGSEGV) almost immediately; a clean
//! run to completion proves the fix holds. This mirrors the already-fixed
//! `GlContextPtr` (resize) and `InstantPtr` (timer) crashes of the same class.
//!
//! Run:
//!     cargo run --release -p azul-dll --example css_double_drop --features link-static

use azul::prelude::StyledDom;

fn main() {
    let n: usize = 200_000;
    for _ in 0..n {
        // `StyledDom::default()` allocates a `CssPropertyCachePtr` (a boxed
        // `CssPropertyCache`). Dropping the `AzStyledDom` wrapper exercises the
        // double-drop path described above.
        let s = StyledDom::default();
        core::hint::black_box(&s);
        drop(s);
    }
    println!(
        "OK: created + dropped {} StyledDom (AzStyledDom) with no double-free",
        n
    );
}
