//! Algol 68 ALIEN PROC declarations for the C-ABI surface.
//!
//! Each surviving IR `FunctionDef` becomes a single line of the form:
//!
//! ```text
//! PROC az app create = (REF AZAPPCONFIG config) REF AZAPP: ALIEN "AzApp_create" ! "azul";
//! ```
//!
//! The `! "azul"` suffix tells a68g which shared object to load the
//! symbol from at runtime; a68g resolves this against the platform
//! lookup chain (`libazul.so` / `libazul.dylib` / `azul.dll`).
//!
//! ## Why a long Algol-side name?
//!
//! a68g identifiers are case-sensitive and conventionally use lowercase
//! letters with optional embedded spaces. We translate
//! `AzApp_create` -> `az app create` for the Algol 68 procedure name
//! while preserving the verbatim `AzApp_create` in the `ALIEN "..."`
//! string literal — the latter must match the C symbol exactly.
//!
//! ## ALIEN form vs. PROC body
//!
//! Standard Algol 68 PROCs have a body. a68g lets you replace the body
//! with `ALIEN "<symbol>" ! "<library>"` to delegate the call to a
//! native function. This is the only mechanism a68g exposes for FFI.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{
    algol_proc_name, map_type_to_algol, ptr_type, sanitize_comment, sanitize_identifier, LIB_NAME,
};

pub fn generate_aliens(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.line("# ALIEN PROC declarations: every C-ABI symbol imported from libazul.            #");
    builder.line("# Symbol names match the C bindings verbatim.                                   #");
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_alien(builder, func, ir);
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

fn emit_alien(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("# {} #", sanitize_comment(d)));
        }
    }

    let proc_name = algol_proc_name(&func.class_name, &func.method_name);

    // Build the argument list. We emit `MODE name` per argument; Algol
    // 68 PROC type lists are comma-separated and parenthesised.
    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let mode = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_algol(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type(&a.type_name, ir),
            };
            format!("{} {}", mode, sanitize_identifier(&a.name))
        })
        .collect();

    let arg_list = if args.is_empty() {
        String::new()
    } else {
        format!(" ({})", args.join(", "))
    };

    let ret_mode = match &func.return_type {
        Some(ret) => map_type_to_algol(ret, ir),
        None => "VOID".to_string(),
    };

    // Final form:
    //   PROC <name> ={args} <ret>: ALIEN "<c_symbol>" ! "<lib>";
    builder.line(&format!(
        "PROC {} ={} {}: ALIEN \"{}\" ! \"{}\";",
        proc_name, arg_list, ret_mode, func.c_name, LIB_NAME
    ));
}
