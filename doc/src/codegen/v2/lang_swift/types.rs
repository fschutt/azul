//! Swift "type" emission.
//!
//! Unlike the Odin backend, the Swift binding does **not** translate the
//! FFI record surface by hand: every `AzFoo` struct, enum and tagged
//! union is imported from the generated `azul.h` through the `CAzul`
//! Clang module (see [`super`]). Swift's C interop reproduces the
//! authoritative C layout, which pure Swift structs cannot guarantee and
//! which pure Swift cannot express at all for `#[repr(C)]` tagged unions.
//!
//! This module therefore only emits a documentation banner recording
//! which type categories the imported module covers — keeping the file
//! structure parallel to the Odin backend (`mod` / `types` / `functions`)
//! while making the "types come from C" decision explicit in the output.

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;

pub fn generate_types(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let n_structs = ir
        .structs
        .iter()
        .filter(|s| config.should_include_type(&s.name))
        .count();
    let n_enums = ir
        .enums
        .iter()
        .filter(|e| config.should_include_type(&e.name))
        .count();
    let n_callbacks = ir.callback_typedefs.len();

    b.line("// ----------------------------------------------------------------------------");
    b.line("// Types come from the imported `CAzul` module (the generated azul.h):");
    b.line(&format!(
        "//   ~{} structs / tagged unions, ~{} enums, {} callback fn-pointer typedefs.",
        n_structs, n_enums, n_callbacks
    ));
    b.line("//");
    b.line("// Swift's Clang importer maps them with their authoritative C layout:");
    b.line("//   - `struct AzFoo { ... };`      -> Swift struct `AzFoo` (memberwise init)");
    b.line("//   - `enum AzBar { AzBar_X ... };` -> Swift `AzBar` + global `AzBar_X` consts");
    b.line("//   - `union AzBaz { ... };`        -> Swift struct `AzBaz` (overlapping storage)");
    b.line("//   - `AzUpdate (*Cb)(...)`         -> `@convention(c) (...) -> AzUpdate` closure");
    b.line("//");
    b.line("// A plain Swift func with a matching signature converts implicitly to the");
    b.line("// `@convention(c)` fn-pointer typedef, so callbacks are passed C-direct.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
}
