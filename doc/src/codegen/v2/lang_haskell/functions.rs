//! Raw `foreign import ccall` declarations for the C-ABI symbols.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single Haskell binding of the shape:
//!
//! ```haskell
//! foreign import ccall unsafe "AzApp_create"
//!   c_AzApp_create :: Ptr AppCreateOptions -> IO (Ptr App)
//! ```
//!
//! Conventions:
//! - The Haskell-side identifier is `c_<C symbol>` so the FFI bindings
//!   are textually distinct from the idiomatic surface.
//! - We use `unsafe` for non-callback-invoking functions (the common
//!   case): faster call, no callback re-entry. We reserve `safe` for
//!   any function whose argument type list contains a callback typedef
//!   pointer.
//! - Every C function is treated as living in `IO`, since calls have
//!   side effects from Haskell's perspective even when the Rust side
//!   is morally pure (e.g. construction of a `Dom`).
//! - Argument and return types use the Haskell representation chosen
//!   in `types.rs` for the matching IR type. Pointers to FFI types
//!   become `Ptr <Name>`; primitives become their `Foreign.C.Types`
//!   equivalent.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::sanitize_doc;
use super::types::haskell_field_type;

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_foreign_imports(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_one(builder, func, ir);
    }

    // Callback wrappers: emit `foreign import ccall "wrapper"` for each
    // callback typedef so users can pass Haskell functions across the
    // FFI as `FunPtr`s.
    builder.blank();
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Callback wrappers: turn a Haskell function into a C function pointer.");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        emit_callback_wrapper(builder, cb, ir);
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

// ============================================================================
// Function emission
// ============================================================================

fn emit_one(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    let safety = if function_takes_callback(func, ir) {
        "safe"
    } else {
        "unsafe"
    };

    let hs_binding = format!("c_{}", func.c_name);

    // Build the type signature: arg1 -> arg2 -> ... -> IO Ret
    let mut atoms: Vec<String> = Vec::new();
    for a in &func.args {
        let ty = match a.ref_kind {
            ArgRefKind::Owned => map_arg_owned(&a.type_name, ir),
            ArgRefKind::Ref
            | ArgRefKind::RefMut
            | ArgRefKind::Ptr
            | ArgRefKind::PtrMut => format!("Ptr {}", map_arg_owned(&a.type_name, ir)),
        };
        atoms.push(ty);
    }

    let returns_void = func
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);

    let return_ty = if returns_void {
        "()".to_string()
    } else {
        let r = func.return_type.as_deref().unwrap_or("()");
        map_arg_owned(r, ir)
    };

    let sig = if atoms.is_empty() {
        format!("IO {}", paren_if_needed(&return_ty))
    } else {
        format!(
            "{} -> IO {}",
            atoms
                .iter()
                .map(|a| paren_if_needed(a))
                .collect::<Vec<_>>()
                .join(" -> "),
            paren_if_needed(&return_ty)
        )
    };

    builder.line(&format!(
        "foreign import ccall {} \"{}\"",
        safety, func.c_name
    ));
    builder.indent();
    builder.line(&format!("{} :: {}", hs_binding, sig));
    builder.dedent();
}

// ============================================================================
// Callback wrappers (`foreign import ccall "wrapper"`)
// ============================================================================

fn emit_callback_wrapper(
    builder: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    // Build the Haskell function type for the callback.
    let mut atoms: Vec<String> = Vec::new();
    for a in &cb.args {
        let ty = match a.ref_kind {
            ArgRefKind::Owned => map_arg_owned(&a.type_name, ir),
            ArgRefKind::Ref
            | ArgRefKind::RefMut
            | ArgRefKind::Ptr
            | ArgRefKind::PtrMut => format!("Ptr {}", map_arg_owned(&a.type_name, ir)),
        };
        atoms.push(ty);
    }

    let returns_void = cb
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);
    let ret_ty = if returns_void {
        "()".to_string()
    } else {
        map_arg_owned(cb.return_type.as_deref().unwrap_or("()"), ir)
    };

    let func_ty = if atoms.is_empty() {
        format!("IO {}", paren_if_needed(&ret_ty))
    } else {
        format!(
            "{} -> IO {}",
            atoms
                .iter()
                .map(|a| paren_if_needed(a))
                .collect::<Vec<_>>()
                .join(" -> "),
            paren_if_needed(&ret_ty)
        )
    };

    let mk_name = format!("mk_{}", cb.name);
    builder.line("foreign import ccall \"wrapper\"");
    builder.indent();
    builder.line(&format!(
        "{} :: ({}) -> IO (FunPtr ({}))",
        mk_name, func_ty, func_ty
    ));
    builder.dedent();
}

// ============================================================================
// Helpers
// ============================================================================

fn function_takes_callback(func: &FunctionDef, ir: &CodegenIR) -> bool {
    func.args.iter().any(|a| {
        a.callback_info.is_some()
            || ir
                .callback_typedefs
                .iter()
                .any(|c| c.name == a.type_name.trim())
    })
}

/// Map an argument's IR type (without ref-kind decoration) to the
/// matching Haskell type. We share with `types::haskell_field_type`
/// for the leaf-type mapping by faking an Owned ref-kind.
fn map_arg_owned(type_name: &str, ir: &CodegenIR) -> String {
    haskell_field_type(type_name, super::super::ir::FieldRefKind::Owned, ir)
}

/// Wrap a multi-token type expression in parens so the surrounding
/// signature parses unambiguously (`Ptr Foo` would otherwise bind
/// `Ptr` only).
fn paren_if_needed(s: &str) -> String {
    let needs = s.contains(' ') && !(s.starts_with('(') && s.ends_with(')'));
    if needs {
        format!("({})", s)
    } else {
        s.to_string()
    }
}
