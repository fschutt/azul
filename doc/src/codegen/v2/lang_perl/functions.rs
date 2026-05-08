//! Perl `$ffi->attach(...)` emission.
//!
//! For every C-ABI function in the IR we emit a single line of the form
//!
//! ```perl
//!     $Azul::ffi->attach('AzApp_create' => ['opaque', 'opaque'] => 'opaque');
//! ```
//!
//! That binds a Perl sub `Azul::FFI::AzApp_create($data, $config)` (FFI::Platypus
//! installs the symbol into the current `package`, which we set to `Azul::FFI`
//! in `mod.rs` before this module is invoked).
//!
//! Type translation lives in `types.rs` (`type_with_ref_to_perl`,
//! `primitive_to_ffi_name`); we re-use it here so attach types stay
//! consistent with record layouts.
//!
//! Skipped functions get a `# SKIPPED:` marker line so the output
//! self-documents what didn't translate. References to types whose
//! definition was skipped (generic templates, recursive types, etc.)
//! collapse to `'opaque'` — the C ABI is still pointer-sized; callers just
//! lose the field accessors on the Perl side.

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FieldRefKind, FunctionDef, FunctionKind};
use super::types::{should_emit_enum, should_emit_struct, type_with_ref_to_perl};

/// Emit `$Azul::ffi->attach(...)` lines for every IR function.
pub fn emit_attach_functions(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("# --- $ffi->attach declarations --------------------------------");
    builder.line("# (Subs are installed into the current package, Azul::FFI.)");

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            emit_skip_marker(builder, &func.c_name, &skip_reason(func, ir, config));
            continue;
        }
        emit_attach_function(builder, func, ir, config);
    }

    builder.blank();
}

// ============================================================================
// Filtering
// ============================================================================

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !class_is_emitted(&func.class_name, ir, config) {
        return false;
    }
    true
}

fn class_is_emitted(name: &str, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if let Some(s) = ir.structs.iter().find(|s| s.name == name) {
        return should_emit_struct(s, config);
    }
    if let Some(e) = ir.enums.iter().find(|e| e.name == name) {
        return should_emit_enum(e, config);
    }
    // Free functions / orphan c_names: emit anyway.
    true
}

fn skip_reason(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> String {
    if let Some(s) = ir.structs.iter().find(|s| s.name == func.class_name) {
        if !should_emit_struct(s, config) {
            return format!(
                "owning struct {} skipped (category={})",
                s.name,
                s.category.description()
            );
        }
    }
    if let Some(e) = ir.enums.iter().find(|e| e.name == func.class_name) {
        if !should_emit_enum(e, config) {
            return format!(
                "owning enum {} skipped (category={})",
                e.name,
                e.category.description()
            );
        }
    }
    "unknown".to_string()
}

fn emit_skip_marker(builder: &mut CodeBuilder, c_name: &str, reason: &str) {
    builder.line(&format!("# SKIPPED: {} ({})", c_name, reason));
}

// ============================================================================
// Emission
// ============================================================================

fn emit_attach_function(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let c_name = &func.c_name;

    let mut arg_types: Vec<String> = func
        .args
        .iter()
        .map(|a| arg_to_perl_ffi(&a.type_name, a.ref_kind, config, ir))
        .collect();

    // Auto-generated trait functions (`_delete`, `_partialEq`, ...) always
    // take a `*mut Self`. If the IR happens to emit them with empty args,
    // synthesise a single `opaque` so attach works at runtime.
    if matches!(
        func.kind,
        FunctionKind::Delete
            | FunctionKind::DeepCopy
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::DebugToString
    ) && arg_types.is_empty()
    {
        arg_types.push("opaque".to_string());
    }

    let ret = match &func.return_type {
        None => "void".to_string(),
        Some(t) => return_to_perl_ffi(t, config, ir),
    };

    let arg_list = arg_types
        .iter()
        .map(|t| format!("'{}'", t))
        .collect::<Vec<_>>()
        .join(", ");

    builder.line(&format!(
        "$Azul::ffi->attach('{}' => [{}] => '{}');",
        c_name, arg_list, ret
    ));
}

// ============================================================================
// Type translation helpers (delegate to types.rs)
// ============================================================================

fn arg_to_perl_ffi(
    type_name: &str,
    ref_kind: ArgRefKind,
    config: &CodegenConfig,
    ir: &CodegenIR,
) -> String {
    let field_ref = arg_ref_to_field_ref(ref_kind);
    type_with_ref_to_perl(type_name, field_ref, config, ir)
}

fn return_to_perl_ffi(type_name: &str, config: &CodegenConfig, ir: &CodegenIR) -> String {
    type_with_ref_to_perl(type_name, FieldRefKind::Owned, config, ir)
}

fn arg_ref_to_field_ref(k: ArgRefKind) -> FieldRefKind {
    match k {
        ArgRefKind::Owned => FieldRefKind::Owned,
        ArgRefKind::Ref => FieldRefKind::Ref,
        ArgRefKind::RefMut => FieldRefKind::RefMut,
        ArgRefKind::Ptr => FieldRefKind::Ptr,
        ArgRefKind::PtrMut => FieldRefKind::PtrMut,
    }
}
