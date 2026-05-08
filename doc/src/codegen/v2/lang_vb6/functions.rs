//! VB6 `Public Declare Function/Sub` extern declarations and
//! `Public Function` module-level wrappers.
//!
//! Two layers:
//!
//! 1. **Externs** — `Public Declare Function az_dom_create Lib "azul"
//!    Alias "AzDom_create" (...) As Long`. The `Alias` clause is what
//!    VB6 actually sends to the dynamic loader; the
//!    case-insensitive identifier on the left is just a VB6
//!    convenience name. We keep them identical so call sites
//!    look natural.
//! 2. **Module-level wrappers** — `Public Function`s in the
//!    `Azul.bas` module that hide the `Az` prefix from user code
//!    where it makes sense. For free functions (functions whose
//!    `class_name` doesn't have a corresponding `_delete`) we emit
//!    a thin pass-through (`Public Function App_create(...) As Long :
//!    App_create = AzApp_create(...) : End Function`) that drops the
//!    `Az` prefix at the call site.
//!
//! VB6 calling-convention quirks:
//!
//! - VB6 cannot pass user-defined types by value to a `Declare`. UDT
//!   arguments must always be passed `ByRef` (i.e. as a pointer).
//!   Functions whose C signature takes a struct by value (i.e.
//!   `ArgRefKind::Owned` over a struct type) get flagged with
//!   `' SKIPPED: cannot pass UDT ByVal — workaround required`.
//! - Pointer arguments are passed `ByVal ... As Long` (Long-as-pointer).
//! - `String` arguments default to `ByVal ... As String` so VB6 auto-marshals
//!   to ANSI. For UTF-8-correct paths the user must use `Long`-as-pointer
//!   plus `StrPtr` / `CopyMemory` — but we keep the simpler `String`
//!   shape for the generated declares because most strings are ASCII.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, TypeCategory};
use super::{
    idiomatic_method_name, map_type_to_vb6, sanitize_comment, sanitize_identifier, LIB_NAME,
};

// ============================================================================
// Extern declarations
// ============================================================================

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("' --------------------------------------------------------------------");
    builder.line("' Public Declare Function/Sub: every C-ABI symbol imported from azul.dll.");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

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

    let mut skipped_udt_byval = false;
    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let (clause, vb_ty) = arg_clause_and_type(&a.ref_kind, &a.type_name, ir);
            // Detect UDT-by-value (the one shape VB6 cannot Declare).
            if a.ref_kind == ArgRefKind::Owned
                && (ir.find_struct(&a.type_name).is_some() || ir.find_enum(&a.type_name).is_some())
            {
                skipped_udt_byval = true;
            }
            format!("{} {} As {}", clause, sanitize_identifier(&a.name), vb_ty)
        })
        .collect();

    let arg_str = args.join(", ");
    let alias = &func.c_name;

    if skipped_udt_byval {
        builder.line(&format!(
            "' SKIPPED: {} takes a UDT ByVal — VB6 Declare cannot pass user-defined types ByVal.",
            func.c_name
        ));
        builder.line("' Workaround: write a C-side shim that takes the struct ByRef, or copy");
        builder.line("' the struct into a Byte() buffer and pass StrPtr-style.");
    }

    match &func.return_type {
        Some(ret) => {
            let vb_ret = map_type_to_vb6(ret, ir);
            // SKIPPED: returning a UDT by value from a Declare is also forbidden in VB6.
            if ir.find_struct(ret.trim()).is_some() || ir.find_enum(ret.trim()).is_some() {
                builder.line(&format!(
                    "' SKIPPED: {} returns a UDT ByVal — VB6 Declare cannot return UDTs.",
                    func.c_name
                ));
                builder.line("' Workaround: write a C-side shim that writes the struct via an out-pointer.");
            }
            builder.line(&format!(
                "Public Declare Function {} Lib \"{}\" Alias \"{}\" ({}) As {}",
                func.c_name, LIB_NAME, alias, arg_str, vb_ret
            ));
        }
        None => {
            builder.line(&format!(
                "Public Declare Sub {} Lib \"{}\" Alias \"{}\" ({})",
                func.c_name, LIB_NAME, alias, arg_str
            ));
        }
    }
}

/// Determine `(ByVal/ByRef, vb_type)` for a single argument.
///
/// VB6 rules:
///   - Pointer args (`*const`/`*mut`/`&`/`&mut`)  → `ByVal ... As Long`.
///   - Primitives by value (Long, Single, etc.)   → `ByVal ... As <T>`.
///   - UDT by value                                → `ByRef ... As <T>`
///     (VB6 cannot pass UDTs ByVal in Declare; the C side must accept
///     them ByRef — the externals layer flags this as SKIPPED if the
///     C side really does want by-value).
fn arg_clause_and_type(ref_kind: &ArgRefKind, type_name: &str, ir: &CodegenIR) -> (&'static str, String) {
    match ref_kind {
        ArgRefKind::Owned => {
            let vb_ty = map_type_to_vb6(type_name, ir);
            // VB6 cannot pass UDTs ByVal in Declare. Pass ByRef for UDTs
            // (this changes the C ABI shape — caller must verify) and
            // ByVal for primitives.
            let is_udt = ir.find_struct(type_name.trim()).is_some()
                || ir.find_enum(type_name.trim()).is_some();
            if is_udt {
                ("ByRef", vb_ty)
            } else {
                ("ByVal", vb_ty)
            }
        }
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            // Pointer in/out — Long-as-pointer.
            ("ByVal", "Long".to_string())
        }
    }
}

// ============================================================================
// Module-level wrapper functions (for free functions and prefix-stripping)
// ============================================================================
//
// We emit thin wrappers in Azul.bas that drop the `Az` prefix where it
// is unambiguous. These wrappers exist purely for naming — they don't
// add ownership semantics (that is the job of the .cls class modules).
// We only emit a wrapper when the raw method does NOT belong to a
// disposable class (because disposable classes already get wrappers
// via .cls files).

pub fn generate_module_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("' --------------------------------------------------------------------");
    builder.line("' Module-level wrappers: idiomatic name-mangling around the externals.");
    builder.line("' Disposable types live in their own .cls Class modules; this section");
    builder.line("' only re-exports free functions / static methods on POD types.");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

    // Compute the set of class names that have a `_delete`. Functions
    // whose class_name is in this set are emitted via .cls files, NOT
    // via this BAS module.
    use std::collections::BTreeSet;
    let disposable: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if disposable.contains(func.class_name.as_str()) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        if matches!(func.kind, FunctionKind::Delete) {
            continue;
        }
        emit_module_wrapper(builder, func, ir);
    }

    Ok(())
}

fn emit_module_wrapper(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let pretty_class = func.class_name.as_str();
    let pretty_method = idiomatic_method_name(&func.method_name);
    let wrapper_name = format!("{}_{}", pretty_class, pretty_method);

    let mut skipped = false;
    let args: Vec<(String, String, String)> = func
        .args
        .iter()
        .map(|a| {
            let (clause, vb_ty) = match a.ref_kind {
                ArgRefKind::Owned => {
                    let vb = map_type_to_vb6(&a.type_name, ir);
                    let is_udt = ir.find_struct(a.type_name.trim()).is_some()
                        || ir.find_enum(a.type_name.trim()).is_some();
                    if is_udt {
                        skipped = true;
                        ("ByRef", vb)
                    } else {
                        ("ByVal", vb)
                    }
                }
                _ => ("ByVal", "Long".to_string()),
            };
            let nm = sanitize_identifier(&a.name);
            (clause.to_string(), nm, vb_ty)
        })
        .collect();

    let sig_args: Vec<String> = args
        .iter()
        .map(|(c, n, t)| format!("{} {} As {}", c, n, t))
        .collect();
    let call_args: Vec<String> = args.iter().map(|(_, n, _)| n.clone()).collect();
    let sig_args_str = sig_args.join(", ");
    let call_args_str = call_args.join(", ");

    if skipped {
        builder.line(&format!(
            "' SKIPPED: wrapper {} — argument is UDT ByVal, see Declare for details",
            wrapper_name
        ));
    }

    match &func.return_type {
        Some(ret) => {
            let vb_ret = map_type_to_vb6(ret, ir);
            builder.line(&format!(
                "Public Function {} ({}) As {}",
                wrapper_name, sig_args_str, vb_ret
            ));
            builder.indent();
            builder.line(&format!(
                "{} = {}({})",
                wrapper_name, func.c_name, call_args_str
            ));
            builder.dedent();
            builder.line("End Function");
        }
        None => {
            builder.line(&format!("Public Sub {} ({})", wrapper_name, sig_args_str));
            builder.indent();
            builder.line(&format!("{} {}", func.c_name, call_args_str));
            builder.dedent();
            builder.line("End Sub");
        }
    }
    builder.blank();
}
