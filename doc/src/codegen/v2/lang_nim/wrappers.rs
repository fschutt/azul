//! Idiomatic, non-`Az`-prefixed Nim wrapper procs.
//!
//! These forward to the raw `importc` layer in `functions.rs`. They exist
//! so users can write `button.setButtonType(...)` / `domCreateBody()`
//! instead of `AzButton_setButtonType(addr button, ...)`. The raw layer
//! stays fully available for the paths the wrappers don't cover.
//!
//! Naming (Nim is style-insensitive, so we keep names globally distinct):
//!
//! - **Instance methods** (`Method` / `MethodMut`) become
//!   `proc <method>*(self: var AzClass, …)`. Overloading on the distinct
//!   `self` type keeps a shared method name (e.g. `dom`, `withChild`)
//!   unambiguous across types.
//! - **Constructors / static methods** become
//!   `proc <class><Method>*(…)` (e.g. `domCreateBody`, `buttonCreate`) —
//!   the class-name prefix guarantees a unique symbol, since Nim can't
//!   overload argument-less statics purely on return type.
//!
//! Trait functions (`Delete`, `DeepCopy`, `Default`, comparisons, hash,
//! debug) and enum-variant constructors are intentionally NOT wrapped —
//! the raw layer covers them and skipping keeps the wrapper surface
//! collision-free and easy to compile.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::types::nim_arg_type;
use super::ProcDedup;
use super::{ffi_type_name, map_type_to_nim, sanitize_identifier, to_lower_camel, to_pascal_case};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    procs: &mut ProcDedup,
) -> Result<()> {
    builder.line("# ============================================================================");
    builder.line("# Idiomatic wrappers — drop the `Az` prefix, forward to the raw layer.");
    builder.line("#   Dom.createBody()  ->  domCreateBody()");
    builder.line("#   button.dom()      ->  proc dom(self: var AzButton): AzDom");
    builder.line("# ============================================================================");
    builder.blank();

    for func in &ir.functions {
        if !should_wrap(func, ir, config) {
            continue;
        }
        match func.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_method_wrapper(builder, func, ir, procs)
            }
            FunctionKind::Constructor | FunctionKind::StaticMethod => {
                emit_static_wrapper(builder, func, ir, procs)
            }
            _ => {}
        }
    }

    Ok(())
}

fn should_wrap(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !matches!(
        func.kind,
        FunctionKind::Method
            | FunctionKind::MethodMut
            | FunctionKind::Constructor
            | FunctionKind::StaticMethod
    ) {
        return false;
    }
    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if !wrappable_struct(s) {
            return false;
        }
    }
    // Only wrap functions whose class is a struct we actually emit; enums
    // and unknown classes keep the raw layer only.
    ir.find_struct(&func.class_name).is_some()
}

fn wrappable_struct(s: &StructDef) -> bool {
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

// ============================================================================
// Instance-method wrapper
// ============================================================================

fn emit_method_wrapper(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    procs: &mut ProcDedup,
) {
    let class_ffi = ffi_type_name(&func.class_name);
    let self_idx = self_arg_index(func);
    let desired = sanitize_identifier(&to_lower_camel(&func.method_name));

    // Build the wrapper parameter list (self first, then user args in
    // their original positions) and the forwarded call argument list.
    let mut params: Vec<String> = Vec::new();
    let mut param_types: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();

    // `self` param — mutable so we can take `addr` when the C fn wants a
    // pointer.
    let self_by_ptr = matches!(
        self_idx.and_then(|i| func.args.get(i)).map(|a| a.ref_kind),
        Some(ArgRefKind::Ref)
            | Some(ArgRefKind::RefMut)
            | Some(ArgRefKind::Ptr)
            | Some(ArgRefKind::PtrMut)
    );
    if self_by_ptr {
        params.push(format!("self: var {}", class_ffi));
        param_types.push(format!("var {}", class_ffi));
    } else {
        params.push(format!("self: {}", class_ffi));
        param_types.push(class_ffi.clone());
    }

    for (i, a) in func.args.iter().enumerate() {
        if Some(i) == self_idx {
            call_args.push(if self_by_ptr {
                "addr self".to_string()
            } else {
                "self".to_string()
            });
            continue;
        }
        // Non-self args keep the raw layer's type (already `ptr AzFoo` for
        // pointer kinds) and forward straight through — no `addr`, which
        // would be illegal on an immutable proc parameter.
        let nim_ty = nim_arg_type(a, ir);
        let pname = sanitize_identifier(&a.name);
        params.push(format!("{}: {}", pname, nim_ty));
        param_types.push(nim_ty);
        call_args.push(pname);
    }

    let name = procs.unique(&desired, &param_types.join(","));
    emit_forwarder(builder, &name, &params, func, ir, &call_args, procs);
}

// ============================================================================
// Constructor / static-method wrapper
// ============================================================================

fn emit_static_wrapper(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    procs: &mut ProcDedup,
) {
    // `<classLowerCamel><MethodPascal>` — globally unique.
    let desired = sanitize_identifier(&format!(
        "{}{}",
        to_lower_camel(&func.class_name),
        to_pascal_case(&func.method_name)
    ));

    let mut params: Vec<String> = Vec::new();
    let mut param_types: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();
    for a in &func.args {
        let nim_ty = nim_arg_type(a, ir);
        let pname = sanitize_identifier(&a.name);
        params.push(format!("{}: {}", pname, nim_ty));
        param_types.push(nim_ty);
        call_args.push(pname);
    }

    let name = procs.unique(&desired, &param_types.join(","));
    emit_forwarder(builder, &name, &params, func, ir, &call_args, procs);
}

// ============================================================================
// Shared forwarder emission
// ============================================================================

fn emit_forwarder(
    builder: &mut CodeBuilder,
    name: &str,
    params: &[String],
    func: &FunctionDef,
    ir: &CodegenIR,
    call_args: &[String],
    procs: &ProcDedup,
) {
    let param_str = params.join(", ");
    // Forward to the raw external under the exact name it was emitted as —
    // which may be a de-collided alias, not the bare C symbol.
    let call = format!("{}({})", procs.external_name(&func.c_name), call_args.join(", "));

    match &func.return_type {
        Some(ret) if ret.trim() != "void" && ret.trim() != "()" => {
            let nim_ret = map_type_to_nim(ret, ir);
            builder.line(&format!(
                "proc {}*({}): {} {{.inline.}} = {}",
                name, param_str, nim_ret, call
            ));
        }
        _ => {
            builder.line(&format!(
                "proc {}*({}) {{.inline.}} = {}",
                name, param_str, call
            ));
        }
    }
}

// ============================================================================
// Argument helpers
// ============================================================================

/// The index of the implicit `self` argument (named `self` or the
/// lower-cased class name), if any.
fn self_arg_index(func: &FunctionDef) -> Option<usize> {
    let class_lower = func.class_name.to_lowercase();
    func.args
        .iter()
        .position(|a| a.name == "self" || a.name.to_lowercase() == class_lower)
}
