//! Pascal `cdecl; external 'azul';` declarations for the C-ABI surface.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single Pascal `function` or `procedure` import. The C-ABI symbol name
//! is preserved verbatim (`AzApp_create`, `AzDom_addChild`, ...) so the
//! linker matches the same exported symbols as the C / C++ bindings.
//!
//! Idiomatic, namespaced wrappers (`TApp.Create(...)`) live in
//! `wrappers.rs` and call into these externals.
//!
//! Pascal calling-convention notes:
//!
//! - `cdecl` matches Rust's `extern "C"` ABI.
//! - `external AzulLib` causes the FPC linker to import the symbol from
//!   `azul.dll` / `libazul.so` / `libazul.dylib` at runtime.
//! - For arguments the IR marks as references / pointers, we emit a typed
//!   pointer (`PAzApp`) so callers get compile-time pointer-type checking
//!   (passing a `PAzWindow` where a `PAzApp` is expected is rejected).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::types::ptr_type_for_arg;
use super::{map_type_to_pascal, sanitize_identifier};

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ External cdecl declarations: every C-ABI function imported from      }");
    builder.line("{ libazul. Symbol names match the C bindings verbatim.                  }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    // Pascal is case-insensitive, so two C symbols differing only in
    // case (e.g. `AzImageRef_getRawImage` vs `AzImageRef_getRawimage`)
    // collide as duplicate `external` declarations. Dedup by lowercased
    // C name so the lookup table on the C side still resolves both
    // symbols (we keep the first declaration's casing).
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !emitted.insert(func.c_name.to_ascii_lowercase()) {
            builder.line(&format!(
                "{{ SKIPPED duplicate external (case-only collision): {} }}",
                func.c_name
            ));
            continue;
        }
        emit_external(builder, func, ir);
    }
    builder.blank();

    Ok(())
}

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !s.generic_params.is_empty() {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&func.class_name) {
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !e.generic_params.is_empty() {
            return false;
        }
    }
    true
}

fn emit_external(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            // Pascal uses `{ ... }` for block comments. Embedded `{`
            // opens a nested level and trailing `}` closes the outer
            // comment, so swap BOTH braces for parens.
            let safe = d
                .replace('{', "(")
                .replace('}', ")")
                .replace('\n', " ")
                .replace('\r', " ");
            builder.line(&format!("{{ {} }}", safe));
        }
    }

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let pas_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type_for_arg(&a.type_name, ir),
            };
            format!("{}: {}", sanitize_identifier(&a.name), pas_ty)
        })
        .collect();

    let args_str = if args.is_empty() {
        String::new()
    } else {
        format!("({})", args.join("; "))
    };

    match &func.return_type {
        Some(ret) => {
            let pas_ret = map_type_to_pascal(ret, ir);
            builder.line(&format!(
                "function {}{}: {}; cdecl; external AzulLib;",
                func.c_name, args_str, pas_ret
            ));
        }
        None => {
            builder.line(&format!(
                "procedure {}{}; cdecl; external AzulLib;",
                func.c_name, args_str
            ));
        }
    }
}
