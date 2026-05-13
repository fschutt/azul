//! `RemillTranspiler` — remill-rs backed implementation of `Transpiler`.
//!
//! Only compiled when the `web-transpiler` Cargo feature is enabled. The
//! feature pulls in the `third_party/remill-rs` git submodule and routes
//! `default_transpiler()` here. Without the feature, this file is excluded
//! from the build and the stub transpiler is the only implementation.
//!
//! Isolation requirement: this module sees only `(fn_name, fn_addr, fn_size)`
//! and returns `WasmModule` bytes. It must not depend on any GUI,
//! event-loop, or window types — the caller decides when in the web.md
//! flow lifting happens.
//!
//! Status: scaffolded. The minimum viable `lift_function` will lift a leaf
//! function such as `fn add(i32, i32) -> i32` through x86-64 → LLVM IR →
//! WASM using remill-rs. Until the submodule's Rust bindings land, every
//! method is a compiling `todo!()` that documents the blocker; the
//! `web-transpiler` feature still gates this file out of default builds so
//! the rest of the crate keeps building.

use super::transpiler::{Transpiler, TranspileError, WasmModule};

/// remill-rs backed transpiler.
///
/// Constructed by `default_transpiler()` when the `web-transpiler` feature
/// is enabled. Holds whatever long-lived state the remill toolchain needs
/// (LLVM context, lifting options); currently empty until the submodule's
/// Rust API surface is wired up.
pub struct RemillTranspiler {
    _private: (),
}

impl RemillTranspiler {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for RemillTranspiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Transpiler for RemillTranspiler {
    fn lift_function(
        &self,
        _fn_name: &str,
        _fn_addr: usize,
        _fn_size: usize,
    ) -> Result<WasmModule, TranspileError> {
        todo!(
            "RemillTranspiler::lift_function — requires third_party/remill-rs bindings; \
             see doc/guide/en/internals/web.md Phase C"
        )
    }

    fn lift_and_link_framework(
        &self,
        _functions: &[(String, usize, usize)],
    ) -> Result<WasmModule, TranspileError> {
        todo!(
            "RemillTranspiler::lift_and_link_framework — requires third_party/remill-rs bindings"
        )
    }

    fn is_available(&self) -> bool {
        // Feature is compiled in, but the lift methods are not implemented yet.
        // Callers should still treat this as "transpilation is wired in"; the
        // first lift call will panic via todo!() until the submodule is ready.
        false
    }

    fn name(&self) -> &str {
        "RemillTranspiler (web-transpiler feature, submodule pending)"
    }
}
