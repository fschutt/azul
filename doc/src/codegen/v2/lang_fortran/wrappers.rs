//! Idiomatic Fortran wrapper types with `final` finalizers (F2003+).
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit a Fortran derived type whose name drops the `Az`
//! prefix (e.g. `App` instead of `AzApp`):
//!
//! ```fortran
//! type :: App
//!   private
//!   type(AzApp) :: raw
//!   logical :: owned = .true.
//! contains
//!   final :: app_finalizer
//!   procedure :: run => app_run
//! end type App
//! ```
//!
//! - The `final ::` binding registers a F2003 finalizer subroutine.
//!   Fortran calls it automatically when the wrapper goes out of scope
//!   (or when an `allocatable :: App` is deallocated). The finalizer
//!   body forwards to `<Type>_delete` exactly the way Pascal
//!   destructors and Ada's `Finalize` overrides do.
//! - The `procedure :: run => app_run` line is a *type-bound procedure*
//!   (TBP). Users call it OO-style: `call my_app%run(window)`. The TBP
//!   body always takes the wrapper as its first argument (`self`).
//! - Static factory functions (constructors) are plain module
//!   procedures named `App_create`, returning a wrapper instance.
//!
//! Plain POD structs without a matching `_delete` get *no* wrapper —
//! users construct the underlying `type(AzFoo)` directly. Tagged-union
//! enums similarly aren't wrapped (they have no clear OO equivalent in
//! Fortran).

use std::collections::BTreeSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, StructDef, TypeCategory};
use super::functions::fortran_alias_for;
use super::{
    ffi_type_name, instance_method_prefix, map_type_to_fortran, sanitize_identifier,
    truncate_identifier, wrapper_type_name,
};

// ============================================================================
// Public entry points
// ============================================================================

pub fn generate_wrapper_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }

    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Idiomatic wrapper types with `final` finalizers.");
    builder.line("! Going out of scope releases the underlying native resources.");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    for s in &targets {
        emit_wrapper_type_decl(builder, s, ir);
    }
    Ok(())
}

pub fn generate_wrapper_bodies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    for s in &targets {
        emit_wrapper_bodies(builder, s, ir);
    }
    Ok(())
}

// ============================================================================
// Discovery
// ============================================================================

fn collect_wrapper_targets<'a>(
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Vec<&'a StructDef> {
    let delete_set: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    ir.structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config) && delete_set.contains(s.name.as_str()))
        .collect()
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
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
// Wrapper type declaration
// ============================================================================

fn emit_wrapper_type_decl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let wrapper = truncate_identifier(&wrapper_type_name(&s.name));
    let raw_ty = truncate_identifier(&ffi_type_name(&s.name));
    let prefix = instance_method_prefix(&s.name);
    let finalizer = truncate_identifier(&format!("{}_finalizer", prefix));

    if !s.doc.is_empty() {
        for d in &s.doc {
            let safe = d.replace('\r', " ").replace('\n', " ");
            builder.line(&format!("! {}", safe));
        }
    }

    builder.line(&format!("type :: {}", wrapper));
    builder.indent();
    builder.line("private");
    // The `target` attribute is illegal on a derived-type component
    // in gfortran (raises "Attribute at (1) is not allowed in a TYPE
    // definition"). We instead apply `target` to each method's
    // `self` dummy argument so `c_loc(self%raw)` is well-formed.
    builder.line(&format!("type({}) :: raw", raw_ty));
    builder.line("logical :: owned = .true.");
    builder.dedent();
    builder.line("contains");
    builder.indent();
    builder.line(&format!("final :: {}", finalizer));

    // One TBP per surviving instance / static method. Constructors
    // are emitted as free module procedures (factories), not TBPs.
    for func in ir.functions_for_class(&s.name) {
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
        ) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        let tbp_name = sanitize_identifier(&func.method_name);
        let proc_name =
            truncate_identifier(&format!("{}_{}", prefix, sanitize_identifier(&func.method_name)));
        // Static methods (no self arg in the IR) must use `nopass` so
        // Fortran doesn't try to inject the wrapper as the first arg.
        // Instance methods use the default pass-by-self semantics.
        let takes_self = matches!(
            func.kind,
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
        );
        let pass_attr = if takes_self { "" } else { ", nopass" };
        builder.line(&format!(
            "procedure{} :: {} => {}",
            pass_attr, tbp_name, proc_name
        ));
    }

    builder.dedent();
    builder.line(&format!("end type {}", wrapper));
    builder.line(&format!("public :: {}", wrapper));
    builder.blank();
}

// ============================================================================
// Wrapper procedure bodies
// ============================================================================

fn emit_wrapper_bodies(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let wrapper = truncate_identifier(&wrapper_type_name(&s.name));
    let raw_ty = truncate_identifier(&ffi_type_name(&s.name));
    let prefix = instance_method_prefix(&s.name);

    // Finalizer. The C `Foo_delete(...)` signature is conventionally
    // by-pointer (`Foo* self`), so the Fortran interface emits
    // `type(c_ptr), value :: instance` and we must wrap with
    // c_loc(self%raw). Look up the actual delete function in the IR
    // to handle the rare by-value-self case.
    let finalizer = truncate_identifier(&format!("{}_finalizer", prefix));
    let delete_alias = fortran_alias_for(&format!("{}_delete", ffi_type_name(&s.name)));
    let delete_by_value = ir
        .functions_for_class(&s.name)
        .find(|f| matches!(f.kind, FunctionKind::Delete))
        .and_then(|f| f.args.first())
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let delete_self_expr = if delete_by_value {
        "self%raw".to_string()
    } else {
        "c_loc(self%raw)".to_string()
    };
    builder.line(&format!("subroutine {}(self)", finalizer));
    builder.indent();
    // `target` lets `c_loc(self%raw)` work without declaring `raw`
    // as a target component (gfortran rejects that).
    builder.line(&format!("type({}), intent(inout), target :: self", wrapper));
    builder.line("if (self%owned) then");
    builder.indent();
    builder.line(&format!("call {}({})", delete_alias, delete_self_expr));
    builder.line("self%owned = .false.");
    builder.dedent();
    builder.line("end if");
    builder.dedent();
    builder.line(&format!("end subroutine {}", finalizer));
    builder.blank();

    // Static factory functions (constructors). Skip any factory whose
    // return type isn't Self — Fortran can't treat a function as a
    // subroutine, and we'd otherwise emit `call foo()` against a
    // function-typed interface (gfortran rejects the type mismatch).
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        let returns_self = func
            .return_type
            .as_deref()
            .map(|r| r.trim() == func.class_name)
            .unwrap_or(false);
        if !returns_self {
            builder.line(&format!(
                "! SKIPPED: factory {} returns {:?}, not {}",
                fortran_alias_for(&func.c_name),
                func.return_type,
                func.class_name
            ));
            builder.blank();
            continue;
        }
        emit_factory_body(builder, &wrapper, &raw_ty, &prefix, func, ir);
    }

    // Instance / static method TBP bodies.
    for func in ir.functions_for_class(&s.name) {
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
        ) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        emit_method_body(builder, &wrapper, &raw_ty, &prefix, func, ir);
    }
}

/// Emit `function <Wrapper>_<method>(...) result(r)` where `r` is a
/// new wrapper instance whose `raw` field is set from the C return
/// value. F2003 calls the wrapper's `final` subroutine when `r` (a
/// local) goes out of scope, but assigning the result to a caller
/// variable transfers ownership.
fn emit_factory_body(
    builder: &mut CodeBuilder,
    wrapper: &str,
    _raw_ty: &str,
    _prefix: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let factory_suffix = if func.method_name == "new" || func.method_name == "create" {
        // Both `new` and `create` normally map to the idiomatic `_create`,
        // but Fortran has no overloading: when a class declares BOTH
        // constructors (Accordion/ComboBox: `new(sections)` + `create()`),
        // two identical specific names are a compile error. On collision the
        // literal `create` keeps the idiomatic name and `new` stays `_new`.
        let collides = func.method_name == "new"
            && ir
                .functions_for_class(&func.class_name)
                .any(|f| f.kind == FunctionKind::Constructor && f.method_name == "create");
        if collides {
            "new".to_string()
        } else {
            "create".to_string()
        }
    } else {
        sanitize_identifier(&func.method_name)
    };
    let factory =
        truncate_identifier(&format!("{}_{}", wrapper_type_name(&func.class_name), factory_suffix));
    let alias = fortran_alias_for(&func.c_name);

    let arg_names: Vec<String> = func
        .args
        .iter()
        .map(|a| sanitize_identifier(&a.name))
        .collect();
    let arg_list = arg_names.join(", ");

    builder.line(&format!(
        "function {}({}) result(r)",
        factory,
        arg_list
    ));
    builder.indent();

    for arg in &func.args {
        let f_ty = match arg.ref_kind {
            ArgRefKind::Owned => map_type_to_fortran(&arg.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "type(c_ptr)".to_string()
            }
        };
        let nm = sanitize_identifier(&arg.name);
        builder.line(&format!("{}, intent(in), value :: {}", f_ty, nm));
    }
    builder.line(&format!("type({}) :: r", wrapper));

    // Callers above filter out non-self-returning factories so this
    // path is always Self-returning. Use the function-call syntax.
    builder.line(&format!("r%raw = {}({})", alias, arg_list));
    builder.line("r%owned = .true.");

    builder.dedent();
    builder.line(&format!("end function {}", factory));
    builder.blank();
}

fn emit_method_body(
    builder: &mut CodeBuilder,
    wrapper: &str,
    _raw_ty: &str,
    prefix: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let proc_name = truncate_identifier(&format!(
        "{}_{}",
        prefix,
        sanitize_identifier(&func.method_name)
    ));
    let alias = fortran_alias_for(&func.c_name);

    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // Drop the implicit self argument. For instance methods
    // (takes_self == true) args[0] IS the self irrespective of
    // how api.json named it (`instance`, lowercased class, snake-
    // cased class like `icon_provider_handle`, etc.). Skipping
    // args[0] directly matches the Java/C#/Kotlin fix and avoids
    // wrapper-vs-native arg drift. For non-instance functions all
    // args are user-facing.
    let visible: Vec<&super::super::ir::FunctionArg> = if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        func.args.iter().collect()
    };

    let mut decl_arg_names: Vec<String> = Vec::with_capacity(visible.len() + 1);
    if takes_self {
        decl_arg_names.push("self".to_string());
    }
    for a in &visible {
        decl_arg_names.push(sanitize_identifier(&a.name));
    }
    let decl_arg_list = decl_arg_names.join(", ");

    let is_function = func.return_type.is_some();

    if is_function {
        builder.line(&format!(
            "function {}({}) result(r)",
            proc_name, decl_arg_list
        ));
    } else {
        builder.line(&format!("subroutine {}({})", proc_name, decl_arg_list));
    }
    builder.indent();

    if takes_self {
        // `class(...)` polymorphic-self isn't strictly required for a
        // non-extending wrapper type, but it lets users override later
        // without changing call sites. Use `class()` for instance
        // methods so the binding is forward-compatible with `extends`.
        // `target` lets `c_loc(self%raw)` work; can't declare `raw`
        // as a target component (gfortran rejects).
        builder.line(&format!(
            "class({}), intent(inout), target :: self",
            wrapper
        ));
    }

    for a in &visible {
        let f_ty = match a.ref_kind {
            ArgRefKind::Owned => map_type_to_fortran(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "type(c_ptr)".to_string()
            }
        };
        let nm = sanitize_identifier(&a.name);
        builder.line(&format!("{}, intent(in), value :: {}", f_ty, nm));
    }

    if let Some(ret) = &func.return_type {
        let ret_ty = map_type_to_fortran(ret, ir);
        builder.line(&format!("{} :: r", ret_ty));
    }

    // Build the C-side call argument list. If `takes_self`, the C
    // function's first param is the self-record. Its declared
    // interface type is `type(...)` for by-value self (Owned) or
    // `type(c_ptr)` for by-ref self (Ref/Ptr) — match exactly,
    // gfortran refuses to widen `type(AzFoo)` to `type(c_ptr)`.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
    let mut call_args: Vec<String> = Vec::with_capacity(visible.len() + 1);
    if takes_self {
        if self_by_value {
            call_args.push("self%raw".to_string());
        } else {
            call_args.push("c_loc(self%raw)".to_string());
        }
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }
    let call_arg_list = call_args.join(", ");

    if is_function {
        builder.line(&format!("r = {}({})", alias, call_arg_list));
    } else {
        builder.line(&format!("call {}({})", alias, call_arg_list));
    }

    // Consume-after-by-value: when the C ABI took `self%raw` by value
    // (DeepCopy / consuming-self method) Rust now owns those bytes.
    // Flip `self%owned = .false.` so the F2003 `final ::` subroutine
    // short-circuits on cleanup and we don't double-drop. `self` is
    // declared `intent(inout)` (line 391) so the mutation is legal.
    // Mirrors the Pascal `FOwned := False;` and JVM `__consume()`
    // pattern from commits dbc7d82b9 + 62094b885.
    if self_by_value {
        builder.line("self%owned = .false.");
    }

    builder.dedent();
    if is_function {
        builder.line(&format!("end function {}", proc_name));
    } else {
        builder.line(&format!("end subroutine {}", proc_name));
    }
    builder.blank();
}
