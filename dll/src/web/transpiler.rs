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
#[derive(Debug, Clone, Default)]
pub struct WasmModule {
    /// The raw WASM binary bytes.
    pub bytes: Vec<u8>,
    /// Content hash for cache-busting URLs.
    pub content_hash: String,
    /// Functions exported by this module.
    pub exports: Vec<String>,
    /// Functions imported from azul-mini.wasm.
    pub imports_from_mini: Vec<String>,
    /// M10-D: canonical addresses of every [`FnClass::BoundaryImport`]
    /// the lift's BFS surfaced as a dependency. Empty in legacy
    /// bundled mode (when api.json `Framework` symbols classify as
    /// `Recursable`). The web-orchestrator unions these across every
    /// per-cb / per-layout / mini lift, then runs a second pass to
    /// lift each boundary into its own per-fn wasm shard.
    pub used_boundaries: Vec<usize>,
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
    /// - `kind`: Callback-typedef tag from api.json (e.g. `"Callback"`,
    ///   `"LayoutCallback"`, `"CheckBoxOnToggleCallback"`). Drives
    ///   wrapper signature synthesis via the implementation's
    ///   per-kind PCS table — picks how args land in registers and
    ///   whether the return uses a hidden destination buffer (X8
    ///   PCS for `>16B` aggregate returns like `LayoutCallback`'s
    ///   `AzDom`).
    fn lift_function(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
        kind: &str,
    ) -> Result<WasmModule, TranspileError>;

    /// Lift multiple framework functions and link them into a single module.
    ///
    /// This produces azul-mini.wasm — the framework core for the browser.
    fn lift_and_link_framework(
        &self,
        functions: &[(String, usize, usize)], // (name, addr, size)
    ) -> Result<WasmModule, TranspileError>;

    /// Lift the eventloop's `AzStartup_*` functions from libazul and link
    /// the resulting wasm32 objects into a single `azul-mini.wasm`.
    ///
    /// Each `(name, addr, size)` tuple selects a function whose body
    /// gets lifted; the per-symbol wrapper signature is resolved via
    /// `signature_for_eventloop_fn(name)` in the remill implementation.
    /// The final module exports every `name` so the browser-side
    /// loader can call them directly.
    ///
    /// Stub transpiler returns `Err`; remill transpiler runs the full
    /// M6/M7 pipeline per symbol and one final wasm-ld link.
    fn lift_and_link_eventloop(
        &self,
        symbols: &[(String, usize, usize)], // (name, addr, size)
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
        _kind: &str,
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

    fn lift_and_link_eventloop(
        &self,
        _symbols: &[(String, usize, usize)],
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
