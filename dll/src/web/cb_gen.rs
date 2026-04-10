//! Discover and transpile user callbacks to WASM.
//!
//! In the full pipeline, this:
//! 1. Walks the DOM tree for all routes
//! 2. Collects registered callback function pointers
//! 3. Lifts each via remill → LLVM IR → WASM
//! 4. Relinks calls to Az* functions as imports from azul-mini.wasm
//!
//! Not yet implemented — all callbacks currently execute server-side.


/// A discovered callback and its WASM module (if transpiled).
#[derive(Debug, Clone)]
pub struct CallbackWasm {
    /// Callback name (derived from symbol name via dladdr).
    pub name: String,
    /// Content hash for cache-busting.
    pub content_hash: String,
    /// WASM bytes. Empty if transpilation failed / stubbed.
    pub wasm_bytes: Vec<u8>,
    /// Whether this callback can run client-side (transpiled to WASM)
    /// or must fall back to server-side execution.
    pub is_client_side: bool,
}

/// Discover all user callbacks and attempt transpilation.
///
/// Not yet implemented — returns an empty vec; all callbacks
/// execute server-side via POST requests.
pub fn discover_and_transpile_callbacks() -> Vec<CallbackWasm> {
    // TODO: walk DOM tree, collect fn pointers, lift via remill.
    Vec::new()
}
