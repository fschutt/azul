//! Ruby `attach_function` emission.
//!
//! For every C-ABI function in the IR we emit a single line of the form
//!
//! ```ruby
//!     attach_function :az_app_create, :AzApp_create, [:pointer, AzAppConfig.by_value], AzApp.by_value
//! ```
//!
//! The two-argument form binds a snake_case Ruby method (`az_app_create`)
//! to the actual C symbol (`AzApp_create`); higher-level wrappers in
//! `wrappers.rs` then call `Native.az_app_create(...)`.
//!
//! Type translation lives in `types.rs` (`type_with_ref_to_ruby`,
//! `primitive_to_ffi_symbol`); we re-use it here to keep the two emitters
//! consistent.
//!
//! Skipped functions get a `# SKIPPED:` marker line so the output
//! self-documents what didn't translate.
//!
//! References to types whose definition was skipped (generic templates,
//! recursive types, etc.) collapse to `:pointer` — the C ABI is still
//! pointer-sized, callers just lose the field accessors.

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FieldRefKind, FunctionDef, FunctionKind,
};
use super::types::{should_emit_enum, should_emit_struct, type_with_ref_to_ruby};

/// Emit `attach_function` lines for every IR function.
pub fn emit_attach_functions(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("# --- attach_function declarations -----------------------------");

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
    // The owning class must itself be emitted.
    if !class_is_emitted(&func.class_name, ir, config) {
        return false;
    }

    // Skip enum-variant constructors of non-emitted enums (already covered
    // above) and functions referencing skipped argument types are still
    // emitted as `:pointer` — we don't lose the symbol, the user just can't
    // peek into the struct from Ruby. The C ABI works either way.
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
    let ruby_name = ruby_attach_name(&func.c_name);
    let c_name = &func.c_name;

    // Build the argument list. Methods receive an implicit `self` pointer
    // first; the IR's argument list always includes it explicitly already
    // when the kind is Method/MethodMut, so we just translate as-is.
    let arg_types: Vec<String> = func
        .args
        .iter()
        .map(|a| arg_to_ruby_ffi(&a.type_name, a.ref_kind, config, ir))
        .collect();

    let ret = match &func.return_type {
        None => ":void".to_string(),
        Some(t) => return_to_ruby_ffi(t, config, ir),
    };

    // Special-case the auto-generated trait functions: they always take a
    // `*mut Self` and (for delete) return :void. The IR usually models them
    // correctly, but if the args happened to be empty we synthesise a
    // pointer arg so attach_function works.
    let arg_types = if matches!(
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
        vec![":pointer".to_string()]
    } else {
        arg_types
    };

    builder.line(&format!(
        "attach_function :{}, :{}, [{}], {}",
        ruby_name,
        c_name,
        arg_types.join(", "),
        ret
    ));
}

// ============================================================================
// Type translation helpers (delegate to types.rs)
// ============================================================================

/// Map an argument's (type, ref_kind) to a Ruby FFI type token.
///
/// References / pointers always become `:pointer`. Owned struct values
/// become `Foo.by_value`. Primitives go through `primitive_to_ffi_symbol`
/// in `types.rs`.
fn arg_to_ruby_ffi(
    type_name: &str,
    ref_kind: ArgRefKind,
    config: &CodegenConfig,
    ir: &CodegenIR,
) -> String {
    let field_ref = arg_ref_to_field_ref(ref_kind);
    type_with_ref_to_ruby(type_name, field_ref, config, ir, false)
}

/// Map a return type string to a Ruby FFI type token.
///
/// We don't have a `ref_kind` for return values; the api.json convention
/// is that complex returns are owned values (`Foo`) or raw pointers
/// (`*mut Foo`). `type_with_ref_to_ruby` handles the pointer-prefix case
/// already.
fn return_to_ruby_ffi(type_name: &str, config: &CodegenConfig, ir: &CodegenIR) -> String {
    type_with_ref_to_ruby(type_name, FieldRefKind::Owned, config, ir, false)
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

// ============================================================================
// Naming
// ============================================================================

/// Convert a C-ABI symbol like `AzApp_create` or `AzAppConfig_default` into
/// the snake_case Ruby method name we attach (e.g. `az_app_create`).
///
/// We preserve the existing `_` separators (so `App_create` stays as a
/// boundary) and lowercase each CamelCase token.
fn ruby_attach_name(c_name: &str) -> String {
    let mut out = String::with_capacity(c_name.len() + 4);
    let mut prev_was_lower = false;
    let mut prev_was_underscore = false;
    for (i, c) in c_name.chars().enumerate() {
        if c == '_' {
            out.push('_');
            prev_was_lower = false;
            prev_was_underscore = true;
            continue;
        }
        if c.is_ascii_uppercase() {
            // Insert a separator before an uppercase that follows a
            // lowercase letter (CamelCase boundary). Don't insert one
            // right after an existing underscore or at the very start.
            if i != 0 && prev_was_lower && !prev_was_underscore {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            out.push(c);
            prev_was_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
        prev_was_underscore = false;
    }
    out
}

