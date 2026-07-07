//! Regression repro for the `IconProviderHandle` codegen double-drop (issue #16/#29).
//!
//! `AzAppConfig` nests an `AzIconProviderHandle` field (`icon_provider`). Dropping an
//! `AzAppConfig` by value runs `drop_in_place::<AzAppConfig>`, which first invokes the
//! wrapper's `Drop` (`AzAppConfig_delete` -> `drop_in_place` of the real `AppConfig`,
//! freeing `IconProviderHandle.inner`) and THEN the drop-glue drops the wrapper's own
//! `icon_provider: AzIconProviderHandle` field, whose `Drop` re-runs `_delete` ->
//! `drop_in_place::<IconProviderHandle>` on the SAME bytes.
//!
//! Before adding `run_destructor` + `ManuallyDrop` gating to `IconProviderHandle`
//! (core/src/icon.rs), that freed the inner `Box<IconProviderInner>` twice -> double
//! free. Same class as the SIGSEGV-proven CssPropertyCachePtr / GlContextPtr / InstantPtr
//! cases.
//!
//! STATUS: FIXED 2026-07-07 (commit 9e3a06ed9) via FIELD-GLUE DELEGATION — the codegen
//! emits a mirror `impl Drop` ONLY for true leaves (custom `Drop` or an owned raw
//! pointer); plain aggregates (`AppConfig`, the ~55 callback structs, `Dom`, `String`)
//! get NO mirror `Drop` and free each field exactly once via Rust field-glue, so the
//! double-drop cannot happen at any nesting depth. This example now runs CLEAN and is a
//! HARD CI regression gate (.github/workflows/rust.yml "Double-drop runtime gate"), paired
//! with the static invariant check (`azul-doc check`).
//!
//! Run:
//!     cargo run --release -p azul-dll --example appconfig_double_drop --features link-static

use azul::prelude::AppConfig;

fn main() {
    let n: usize = 200_000;
    for _ in 0..n {
        let c = AppConfig::default();
        core::hint::black_box(&c);
        drop(c);
    }
    println!(
        "OK: created + dropped {} AppConfig (AzAppConfig) with no double-free",
        n
    );
}
