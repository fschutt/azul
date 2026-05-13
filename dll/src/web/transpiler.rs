//! Transpiler trait and stub implementation.
//!
//! The `Transpiler` trait abstracts over the x86-64 → WASM lifting pipeline.
//! The trait is the *only* surface this module exposes — callers feed in
//! `(fn_name, fn_addr, fn_size)` and get back `WasmModule` bytes, which
//! keeps the lift step decoupled from windowing, the event loop, and the
//! `run_web` orchestrator. That isolation is what lets the web.md flow
//! choose *when* lifting runs (build-time, first-request, lazy-per-callback)
//! without touching this file.
//!
//! Implementations:
//! - [`StubTranspiler`] — always available pure-Rust fallback. Returns
//!   `TranspileError` from both lift methods so the web backend falls back
//!   to server-side callback execution (POST → run natively → return HTML).
//! - [`RemillTranspiler`] — opt-in via the `web-transpiler` Cargo feature
//!   (pulls in the `third_party/remill-rs` submodule). Lifts x86-64 → LLVM
//!   IR → WASM. Lives in a sibling file (`transpiler_remill.rs`) so the
//!   remill toolchain only links when the feature is on.

/// Error returned when a function cannot be transpiled.
#[derive(Debug, Clone)]
pub struct TranspileError {
    pub fn_name: String,
    pub reason: String,
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot transpile '{}': {}", self.fn_name, self.reason)
    }
}

/// A lifted WASM module (bytes + metadata).
#[derive(Debug, Clone)]
pub struct WasmModule {
    /// The raw WASM binary bytes.
    pub bytes: Vec<u8>,
    /// Content hash for cache-busting URLs.
    pub content_hash: String,
    /// Functions exported by this module.
    pub exports: Vec<String>,
    /// Functions imported from azul-mini.wasm.
    pub imports_from_mini: Vec<String>,
}

/// Trait for transpiling native functions to WASM.
///
/// Implementations:
/// - `StubTranspiler`: Phase 0 — returns errors, callbacks run server-side
/// - (future): Real transpiler using remill to lift x86-64 → LLVM IR → WASM
pub trait Transpiler {
    /// Lift a single function from the running binary into a WASM module.
    ///
    /// # Arguments
    /// - `fn_name`: The symbol name (e.g., "on_click", "AzDom_addChild")
    /// - `fn_addr`: The function's address in the running process
    /// - `fn_size`: Estimated size in bytes (from dladdr or DWARF)
    fn lift_function(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
    ) -> Result<WasmModule, TranspileError>;

    /// Lift multiple framework functions and link them into a single module.
    ///
    /// This produces azul-mini.wasm — the framework core for the browser.
    fn lift_and_link_framework(
        &self,
        functions: &[(String, usize, usize)], // (name, addr, size)
    ) -> Result<WasmModule, TranspileError>;

    /// Whether this transpiler is functional (vs. a stub).
    fn is_available(&self) -> bool;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Phase 0 stub transpiler. Returns errors for all operations.
///
/// When this transpiler is active, the web backend falls back to
/// server-side callback execution (POST → run natively → return HTML).
pub struct StubTranspiler;

impl Transpiler for StubTranspiler {
    fn lift_function(
        &self,
        fn_name: &str,
        _fn_addr: usize,
        _fn_size: usize,
    ) -> Result<WasmModule, TranspileError> {
        Err(TranspileError {
            fn_name: fn_name.to_string(),
            reason: "transpiler not yet implemented (Phase 0 stub — callbacks run server-side)".into(),
        })
    }

    fn lift_and_link_framework(
        &self,
        _functions: &[(String, usize, usize)],
    ) -> Result<WasmModule, TranspileError> {
        Err(TranspileError {
            fn_name: "azul-mini".into(),
            reason: "transpiler not yet implemented (Phase 0 stub)".into(),
        })
    }

    fn is_available(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "StubTranspiler (Phase 0)"
    }
}

/// Get the default transpiler for the current build.
///
/// With the `web-transpiler` feature ON, returns [`RemillTranspiler`].
/// Otherwise returns the pure-Rust [`StubTranspiler`] fallback.
pub fn default_transpiler() -> Box<dyn Transpiler> {
    #[cfg(feature = "web-transpiler")]
    {
        Box::new(crate::web::transpiler_remill::RemillTranspiler::new())
    }
    #[cfg(not(feature = "web-transpiler"))]
    {
        Box::new(StubTranspiler)
    }
}
