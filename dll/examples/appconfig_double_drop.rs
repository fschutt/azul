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
//! STATUS: with the IconProviderHandle fix, this now gets PAST `icon_provider` and
//! aborts later in `AzSystemStyle_delete` (`free(): double free detected`) — i.e.
//! `AzAppConfig` nests MULTIPLE ungated double-drop fields (SystemStyle next, likely
//! more). This is whack-a-mole for deeply-nested structs; the real fix is the systemic
//! codegen change (don't double-drop Az mirror fields — `ptr::read`+forget in `_delete`,
//! or `ManuallyDrop` mirror fields). This example is the regression target for that
//! work: it should run clean only once the whole class is closed. It is built (compiled)
//! but intentionally NOT run in CI until then.
//!
//! Run (currently aborts at SystemStyle — see above):
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
