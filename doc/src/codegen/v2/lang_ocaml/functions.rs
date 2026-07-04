//! `foreign "<C symbol>" (...)` emission for the OCaml generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! `let <ocaml_name> = foreign "<C symbol>" (<sig>)` value-binding at
//! the top level of `azul.ml`. The Ctypes-Foreign DSL builds the
//! libffi cif at runtime, so there is no compile step on the
//! consumer's machine.
//!
//! Naming:
//! - The OCaml-side identifier is `lower_snake_case` and corresponds
//!   to `func.c_name` lower-snaked: e.g. `AzApp_create` becomes
//!   `az_app_create`. This keeps the FFI bindings textually distinct
//!   from the idiomatic surface (which lives inside nested modules
//!   like `Azul.App.create`).
//! - The `foreign "..."` link name uses the **exact** C symbol from
//!   `func.c_name`. The dynamic linker is case-sensitive even though
//!   OCaml itself is.
//!
//! Argument and return-type coercion:
//! - Owned types: pass-by-value. Maps to the corresponding Ctypes view
//!   (`uint32_t`, `az_app`, etc.).
//! - References / mutable references / pointers: collapse to `(ptr T)`
//!   (typed pointer when `T` is known) or `(ptr void)` (opaque).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{
    inner_pointer_form, map_type_to_ocaml, sanitize_doc, sanitize_identifier, to_snake_case,
};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_foreign_bindings(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Raw FFI value bindings (`foreign \"<symbol>\" (...)`).                       *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
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
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    let ocaml_name = ocaml_binding_name(&func.c_name);

    let mut atoms: Vec<String> = Vec::new();
    if func.args.is_empty() {
        // `void` arg list: signature is `void @-> returning T` per Ctypes.
        atoms.push("void".to_string());
    } else {
        for a in &func.args {
            let view = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_ocaml(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => inner_pointer_form(a.type_name.trim(), ir),
            };
            // Sanitize the (otherwise unused) argument name for the
            // accompanying comment so reader can still see what's
            // what.
            let _ = sanitize_identifier(&to_snake_case(&a.name));
            atoms.push(view);
        }
    }

    let returns_void = func
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);

    let return_view = if returns_void {
        "void".to_string()
    } else {
        let r = func.return_type.as_deref().unwrap_or("void");
        map_type_to_ocaml(r, ir)
    };

    let signature = format!("{} @-> returning {}", atoms.join(" @-> "), return_view);

    // Functions with a callback-wrapper arg bind the `<c_name>Struct`
    // C symbol (whole wrapper struct by value — matches the ctypes
    // struct view built above). The raw `<c_name>` takes a bare fn ptr
    // at the C ABI; binding it with a struct view crashed on click.
    // The OCaml-side value name stays derived from the original c_name.
    builder.line(&format!(
        "let {} = foreign \"{}\" ({})",
        ocaml_name,
        super::super::managed_host_invoker::managed_c_symbol(func),
        signature
    ));
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a C symbol like `AzApp_create` to the OCaml binding
/// identifier `az_app_create`. We snake-case the entire symbol so
/// that the `_` separator between the class and the method matches
/// the snake-case of the class itself.
pub fn ocaml_binding_name(c_name: &str) -> String {
    // Snake-case the C name then prefix with `ffi_` so foreign-function
    // bindings never collide with struct typ values. Without the
    // prefix `AzShape_circle` (factory) and `AzShapeCircle` (struct)
    // both lowercase to `az_shape_circle`, and OCaml binds the later
    // emit, shadowing the typ. The `ffi_` prefix keeps the two
    // namespaces apart while remaining ergonomic
    // (`Azul.ffi_az_shape_circle args`).
    let snaked = to_snake_case(c_name);
    let sanitized = sanitize_identifier(&snaked);
    format!("ffi_{}", sanitized)
}
