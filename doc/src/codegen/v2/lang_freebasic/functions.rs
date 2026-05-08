//! FreeBASIC `Declare Function` / `Declare Sub` declarations for the
//! C-ABI surface.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single FB import inside the surrounding `Extern "C" Lib "azul"`
//! block (which is opened/closed by the parent `mod.rs`). The C-ABI
//! symbol name is preserved verbatim via the `Alias "..."` clause:
//! FreeBASIC is case-insensitive for identifiers and uppercases them
//! by default at link-time, but the string passed to `Alias` is
//! treated as the literal symbol the dynamic loader looks up.
//!
//! Idiomatic, namespaced wrappers (`Azul.App.Create(...)`) live in
//! `wrappers.rs` and call into these declarations.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::types::ptr_type_for_arg;
use super::{map_type_to_fb, sanitize_comment, sanitize_identifier};

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("' --------------------------------------------------------------------");
    builder.line("' Extern \"C\" declarations: every C-ABI function imported from libazul.");
    builder.line("' Symbol names match the C bindings verbatim via Alias \"...\".");
    builder.line("' --------------------------------------------------------------------");

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_external(builder, func, ir);
    }

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
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let fb_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_fb(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type_for_arg(&a.type_name, ir),
            };
            // Always pass ByVal: even structs-by-value go ByVal in C ABI,
            // and pointer types are already explicit `... Ptr`.
            format!("ByVal {} As {}", sanitize_identifier(&a.name), fb_ty)
        })
        .collect();

    let arg_str = args.join(", ");

    let alias = &func.c_name;

    match &func.return_type {
        Some(ret) => {
            let fb_ret = map_type_to_fb(ret, ir);
            builder.line(&format!(
                "Declare Function {} Alias \"{}\" ({}) As {}",
                func.c_name, alias, arg_str, fb_ret
            ));
        }
        None => {
            builder.line(&format!(
                "Declare Sub {} Alias \"{}\" ({})",
                func.c_name, alias, arg_str
            ));
        }
    }
}
