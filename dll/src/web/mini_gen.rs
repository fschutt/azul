//! Generate azul-mini.wasm from framework functions.
//!
//! In the full pipeline, this lifts ~200 Azul C API functions from the
//! native binary → LLVM IR → wasm32, producing a single linked module
//! that the browser uses for layout, DOM diffing, hit-testing, etc.
//!
//! Phase 0: Returns a minimal stub WASM module (valid but empty).

use super::classify::ApiClassification;

/// Generate azul-mini.wasm.
///
/// Phase 0: Returns a minimal valid WASM module (~8 bytes).
/// The module has no functions — it exists so the HTTP server has
/// something to serve at `/az/mini.{hash}.wasm`.
pub fn generate_mini_wasm(_classification: &ApiClassification) -> Vec<u8> {
    // Minimal valid WASM module:
    // magic: \0asm
    // version: 1
    minimal_wasm_module()
}

/// Produce the smallest valid WASM module (8 bytes).
fn minimal_wasm_module() -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6D, // \0asm magic
        0x01, 0x00, 0x00, 0x00, // version 1
    ]
}
