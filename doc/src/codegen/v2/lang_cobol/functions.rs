//! COBOL function-symbol declarations.
//!
//! COBOL has no native FFI prototype declaration; it discovers external
//! C symbols at link time via the `CALL "literal"` form. To shield the
//! caller from typing the case-sensitive C identifier verbatim every
//! time, we emit one level-78 alphanumeric constant per surviving
//! function:
//!
//! ```cobol
//!     78  FN-AZ-APP-CREATE      VALUE "AzApp_create".
//!     78  FN-AZ-APP-DELETE      VALUE "AzApp_delete".
//! ```
//!
//! The user then writes:
//!
//! ```cobol
//!     CALL FN-AZ-APP-CREATE USING BY VALUE WS-DATA
//!                                  BY VALUE WS-CONFIG
//!                          RETURNING WS-APP.
//! ```
//!
//! and the COBOL preprocessor expands that to `CALL "AzApp_create"
//! USING ...`, preserving the exact identifier the C linker exports.
//!
//! In addition to the bare constants we emit a comment block above each
//! function describing the expected arguments and return type in a
//! quasi-prototype form. There is no executable code emitted in this
//! module — pure declarative content that fits inside `WORKING-STORAGE
//! SECTION`.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::types::pic_for_type;
use super::{
    cobol_identifier, emit_doc_comment, sanitize_cobol_identifier, sanitize_doc, to_cobol_case,
};

pub fn generate_function_constants(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("*> ============================================================");
    builder.line("*> FUNCTION-NAME CONSTANTS (level-78 strings)");
    builder.line("*> COBOL CALL forms accept a literal or a level-78 alphanumeric;");
    builder.line("*> we emit one constant per C symbol so callers write");
    builder.line("*>   CALL FN-AZ-APP-CREATE USING BY VALUE WS-DATA");
    builder.line("*>                                BY VALUE WS-CONFIG");
    builder.line("*>                       RETURNING WS-APP.");
    builder.line("*> The signature comment above each constant describes the C-ABI");
    builder.line("*> argument list. Pointers are passed BY VALUE; records BY REFERENCE.");
    builder.line("*> ============================================================");

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_function_constant(builder, func, ir);
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

fn emit_function_constant(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            emit_doc_comment(builder, d);
        }
    }

    // Quasi-prototype banner so callers know what to USE / RETURN.
    let arg_strs: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let nm = sanitize_cobol_identifier(&to_cobol_case(&a.name));
            let usage = match a.ref_kind {
                ArgRefKind::Owned => pic_for_type(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "USAGE POINTER".to_string(),
            };
            let pass = match a.ref_kind {
                ArgRefKind::Owned => "BY VALUE",
                _ => "BY VALUE", // pointer-shaped, pass the pointer by value
            };
            format!("{} {}: {}", pass, nm, usage)
        })
        .collect();
    let ret = match &func.return_type {
        Some(r) => pic_for_type(r, ir),
        None => "VOID".to_string(),
    };
    // Signatures with many args / very long type names can run past
    // free-format COBOL's 512-byte line limit. Wrap onto multiple
    // `*> SIGNATURE: ...` lines just like emit_doc_comment does.
    emit_doc_comment(
        builder,
        &format!("SIGNATURE: ({}) RETURNING {}", arg_strs.join(", "), ret),
    );

    // The level-78 constant. Its name is FN-<COBOL-CASE-OF-C-NAME>;
    // its value is the C symbol verbatim so the linker matches the
    // exact case it exports.
    let cobol_sym = cobol_identifier(&format!("FN-{}", to_cobol_case(&func.c_name)));
    builder.line(&format!(
        "       78  {:<28} VALUE \"{}\".",
        cobol_sym, func.c_name
    ));
    builder.blank();
}
