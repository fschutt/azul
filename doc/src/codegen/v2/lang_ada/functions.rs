//! `pragma Import (C, Ada_Name, "c_symbol")` emission for the Ada
//! generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! free `procedure` or `function` declaration with a matching
//! `pragma Import` that pins the C link name to the exact symbol exported
//! by `libazul`.
//!
//! The Ada subprogram name follows the form `Az_<Class>_<Method>` so
//! that:
//!
//! - It does not collide with the user-facing wrapper-type primitives
//!   (which drop the `Az_` prefix).
//! - It is a valid Ada identifier (case-insensitive, no quoting needed).
//! - Reading a stack trace gives a clear correspondence to the C symbol.
//!
//! All `pragma Import` link names use the **exact** C symbol from
//! `func.c_name`, never an Ada-mangled form. This is critical: Ada is
//! case-insensitive but the dynamic linker is not.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, TypeCategory};

// Re-export the kind enum for convenience to the helper below.
use super::{ada_ffi_type_name, map_type_to_ada, sanitize_identifier};

pub fn emit_imports(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Raw FFI subprogram declarations (pragma Import (C, ..., \"...\"))");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    // Ada is case-insensitive, so two functions that lower to the
    // same identifier collide (e.g. `deep_copy` and `clone` both map
    // to `Deep_Copy` via the FunctionKind::DeepCopy renaming). Skip
    // the second occurrence; the first wins.
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        let name = ada_subprogram_name(func);
        if !emitted.insert(name.to_ascii_lowercase()) {
            builder.line(&format!(
                "-- SKIPPED duplicate (collides with prior Ada name): {} ({})",
                func.c_name,
                name
            ));
            continue;
        }
        emit_one(builder, func, ir);
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

fn emit_one(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    // Ada subprogram name: keep it close to the C symbol (which is the
    // link name anyway) so the spec reads naturally.
    let ada_name = ada_subprogram_name(func);

    let returns_void = func
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);

    // Build parameter list. All non-owned modes (Ref/Mut/Ptr/PtrMut)
    // collapse to `System.Address` because `pragma Convention (C)` does
    // not give us a typed access for arbitrary pointers without first
    // declaring a per-type access — which the FFI struct already exposes
    // separately.
    let mut params: Vec<(String, String)> = Vec::new();
    for a in &func.args {
        let pname = sanitize_identifier(&pascalize_arg_name(&a.name));
        let ada_ty = match a.ref_kind {
            ArgRefKind::Owned => map_type_to_ada(&a.type_name, ir),
            ArgRefKind::Ref
            | ArgRefKind::RefMut
            | ArgRefKind::Ptr
            | ArgRefKind::PtrMut => "System.Address".to_string(),
        };
        params.push((pname, ada_ty));
    }

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("-- {}", d.replace('\n', " ").trim()));
        }
    }

    // Build the prototype across multiple lines for readability.
    let header = if returns_void {
        format!("procedure {}", ada_name)
    } else {
        // Note: the actual `return ...` clause is appended after params.
        format!("function {}", ada_name)
    };

    if params.is_empty() {
        if returns_void {
            builder.line(&format!("{};", header));
        } else {
            let ret = func
                .return_type
                .as_deref()
                .map(|r| map_type_to_ada(r, ir))
                .unwrap_or_else(|| "System.Address".to_string());
            builder.line(&format!("{} return {};", header, ret));
        }
    } else {
        builder.line(&format!("{} ", header));
        builder.line("  (");
        for (i, (pname, pty)) in params.iter().enumerate() {
            let sep = if i + 1 == params.len() { "" } else { ";" };
            // All FFI parameters travel by value (or by-address-as-System.Address);
            // mode `in` is correct for all of them on the C side.
            builder.line(&format!("   {} : {}{}", pname, pty, sep));
        }
        if returns_void {
            builder.line("  );");
        } else {
            let ret = func
                .return_type
                .as_deref()
                .map(|r| map_type_to_ada(r, ir))
                .unwrap_or_else(|| "System.Address".to_string());
            builder.line(&format!("  ) return {};", ret));
        }
    }

    builder.line(&format!(
        "pragma Import (C, {}, \"{}\");",
        ada_name, func.c_name
    ));
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Build the Ada subprogram name from an IR function.
///
/// We use `Az_<Class>_<Method>` (Pascal_Snake) so the spec reads as a
/// natural Ada identifier even though it shadows the C symbol form
/// closely. `pragma Import` provides the exact C link name separately.
pub fn ada_subprogram_name(func: &FunctionDef) -> String {
    let class_part = ada_ffi_type_name(&func.class_name);
    let method_part = ada_method_part(&func.method_name, func.kind);
    format!("{}_{}", class_part, method_part)
}

fn ada_method_part(method_name: &str, kind: FunctionKind) -> String {
    let raw = match kind {
        FunctionKind::Delete => "Delete".to_string(),
        FunctionKind::DeepCopy => "Deep_Copy".to_string(),
        FunctionKind::PartialEq => "Partial_Eq".to_string(),
        FunctionKind::PartialCmp => "Partial_Cmp".to_string(),
        FunctionKind::Cmp => "Cmp".to_string(),
        FunctionKind::Hash => "Hash".to_string(),
        FunctionKind::Default => "Default".to_string(),
        FunctionKind::DebugToString => "To_Dbg_String".to_string(),
        _ => pascalize_method_name(method_name),
    };
    sanitize_identifier(&raw)
}

fn pascalize_method_name(name: &str) -> String {
    // Insert underscores at camelCase boundaries.
    let mut split = String::with_capacity(name.len() + 4);
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        split.push(c);
        if c.is_ascii_lowercase()
            && chars.peek().map(|n| n.is_ascii_uppercase()).unwrap_or(false)
        {
            split.push('_');
        }
    }
    // Title-case each word.
    let mut out = String::with_capacity(split.len());
    let mut upper_next = true;
    for c in split.chars() {
        if c == '_' {
            out.push('_');
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.extend(c.to_ascii_lowercase().to_string().chars());
        }
    }
    out
}

fn pascalize_arg_name(name: &str) -> String {
    pascalize_method_name(name)
}
