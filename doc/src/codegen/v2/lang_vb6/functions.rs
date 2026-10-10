//! VB6 `Public Declare Function/Sub` extern declarations and
//! `Public Function` module-level wrappers.
//!
//! Two layers:
//!
//! 1. **Externs** — `Public Declare Function az_dom_create Lib "azul" Alias "AzDom_create" (...) As
//!    Long`. The `Alias` clause is what VB6 actually sends to the dynamic loader; the
//!    case-insensitive identifier on the left is just a VB6 convenience name. We keep them
//!    identical so call sites look natural.
//! 2. **Module-level wrappers** — `Public Function`s in the `Azul.bas` module that hide the `Az`
//!    prefix from user code where it makes sense. For free functions (functions whose `class_name`
//!    doesn't have a corresponding `_delete`) we emit a thin pass-through (`Public Function
//!    App_create(...) As Long : App_create = AzApp_create(...) : End Function`) that drops the `Az`
//!    prefix at the call site.
//!
//! VB6 calling-convention quirks:
//!
//! - VB6 cannot pass or return user-defined types by value in a `Declare`. A function that does
//!   either is declared and called through the `<symbol>Byref` twin libazul exports for exactly
//!   such FFIs (aggregates by pointer - VB6's `ByRef` - and the result through a leading
//!   out-pointer): [`uses_byref_twin`], [`declared_symbol`], [`call_lines`].
//! - VB6 `Declare` is stdcall; libazul's exports are cdecl. On 32-bit x86 they differ in who pops
//!   the arguments, so a call with arguments raises "Bad DLL calling convention" (error 49) unless
//!   the 32-bit libazul exports stdcall entry points - see the F1 report.
//! - Pointer arguments are passed `ByVal ... As Long` (Long-as-pointer).
//! - `String` arguments default to `ByVal ... As String` so VB6 auto-marshals to ANSI. For
//!   UTF-8-correct paths the user must use `Long`-as-pointer plus `StrPtr` / `CopyMemory` — but we
//!   keep the simpler `String` shape for the generated declares because most strings are ASCII.

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, TypeCategory},
        managed_host_invoker::managed_c_symbol,
    },
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
    // A trait entry point an api.json `derive` declares is not what the
    // `DestructorOrClone` exclusion below is for. That category is excluded
    // because those types' ordinary methods traffic in callback function
    // pointers this binding cannot marshal; `Az{T}_toDbgString(ptr) ->
    // AzString` traffics in neither, and is the same shape as the ~2800
    // `_toDbgString` declarations this binding already emits. Excluding it
    // wholesale is why every `*VecDestructor` declared `Debug` and named it
    // nowhere. (`*VecDestructor` is a tagged union, hence `find_enum`.)
    // RECURSIVE types are here for the same reason. `XmlNodeChild` and friends
    // are excluded below because their ORDINARY methods traffic in a shape
    // this binding cannot express by value - but `Az{T}_partialEq(a, b) ->
    // bool` and `Az{T}_toDbgString(ptr) -> AzString` take a pointer and return
    // a scalar, so the exclusion never applied to them. That is why the same
    // four types - `Xml`, `XmlNodeChild`, `XmlNodeChildVec`,
    // `ResultXmlXmlError` - showed up as the residue in fourteen bindings at
    // once: one cause, not fourteen.
    if func.kind.is_declared_capability()
        && (ir.find_enum(&func.class_name).is_some_and(|e| {
            matches!(
                e.category,
                TypeCategory::DestructorOrClone | TypeCategory::Recursive
            )
        }) || ir
            .find_struct(&func.class_name)
            .is_some_and(|s| s.category == TypeCategory::Recursive))
    {
        return config.should_include_type(&func.class_name);
    }

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

/// Does `func` pass an aggregate by value or return one? A VB6 `Declare` can
/// do neither, so such a function is declared and called through the
/// `<symbol>Byref` twin libazul exports (the one azul.h declares): owned
/// aggregates by pointer (still CONSUMED, like the by-value call), the
/// result through a leading out-pointer. "Aggregate" is
/// [`CodegenIR::is_value_aggregate`] - the predicate the twins are emitted
/// by; a unit enum is an int and stays ByVal.
pub(super) fn uses_byref_twin(func: &FunctionDef, ir: &CodegenIR) -> bool {
    func.args
        .iter()
        .any(|a| a.ref_kind == ArgRefKind::Owned && ir.is_value_aggregate(&a.type_name))
        || func
            .return_type
            .as_deref()
            .is_some_and(|r| ir.is_value_aggregate(r))
}

/// The C symbol VB6 declares for `func`: the `Byref` twin (of the managed
/// symbol, the literal api.json signature) or the function itself.
pub(super) fn declared_symbol(func: &FunctionDef, ir: &CodegenIR) -> String {
    if uses_byref_twin(func, ir) {
        format!("{}Byref", managed_c_symbol(func))
    } else {
        func.c_name.clone()
    }
}

/// VB6 statements that call `func` with `args` (VB6 expressions, one per
/// `func.args` entry, in order) and, if it returns a value, store it in
/// `result`. Through the Byref twin the result variable is its first
/// argument (a UDT can be passed ByRef); otherwise it is an assignment.
pub(super) fn call_lines(
    func: &FunctionDef,
    ir: &CodegenIR,
    args: &[String],
    result: Option<&str>,
) -> Vec<String> {
    let symbol = declared_symbol(func, ir);
    let returns = func.return_type.is_some();
    if uses_byref_twin(func, ir) {
        let mut all: Vec<String> = Vec::with_capacity(args.len() + 1);
        if returns {
            all.push(result.unwrap_or("ret_").to_string());
        }
        all.extend(args.iter().cloned());
        return vec![format!("{} {}", symbol, all.join(", "))];
    }
    match (returns, result) {
        (true, Some(r)) => vec![format!("{} = {}({})", r, symbol, args.join(", "))],
        _ => vec![format!("{} {}", symbol, args.join(", "))],
    }
}

fn emit_external(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let byref = uses_byref_twin(func, ir);
    let symbol = declared_symbol(func, ir);
    let mut args: Vec<String> = Vec::with_capacity(func.args.len() + 1);
    if let (true, Some(ret)) = (byref, &func.return_type) {
        // The twin's out-pointer for the result.
        args.push(format!("ByRef ret_ As {}", map_type_to_vb6(ret, ir)));
    }
    for a in &func.args {
        let (clause, vb_ty) = arg_clause_and_type(&a.ref_kind, &a.type_name, ir);
        args.push(format!("{} {} As {}", clause, sanitize_identifier(&a.name), vb_ty));
    }
    let arg_str = args.join(", ");

    match (&func.return_type, byref) {
        (Some(ret), false) => {
            let vb_ret = map_type_to_vb6(ret, ir);
            builder.line(&format!(
                "Public Declare Function {} Lib \"{}\" Alias \"{}\" ({}) As {}",
                symbol, LIB_NAME, symbol, arg_str, vb_ret
            ));
        }
        _ => {
            builder.line(&format!(
                "Public Declare Sub {} Lib \"{}\" Alias \"{}\" ({})",
                symbol, LIB_NAME, symbol, arg_str
            ));
        }
    }
}

/// Determine `(ByVal/ByRef, vb_type)` for a single argument.
///
/// VB6 rules:
///   - Pointer args (`*const`/`*mut`/`&`/`&mut`)  → `ByVal ... As Long`.
///   - Primitives and unit enums by value          → `ByVal ... As <T>`.
///   - An aggregate by value                       → `ByRef ... As <T>`: VB6 passes it as a
///     pointer, which is what the `Byref` twin this binding declares for such a function takes
///     (see [`uses_byref_twin`]).
pub(super) fn arg_clause_and_type(
    ref_kind: &ArgRefKind,
    type_name: &str,
    ir: &CodegenIR,
) -> (&'static str, String) {
    match ref_kind {
        ArgRefKind::Owned => {
            let vb_ty = map_type_to_vb6(type_name, ir);
            if ir.is_value_aggregate(type_name) {
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

    let args: Vec<(String, String, String)> = func
        .args
        .iter()
        .map(|a| {
            let (clause, vb_ty) = arg_clause_and_type(&a.ref_kind, &a.type_name, ir);
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

    match &func.return_type {
        Some(ret) => {
            // A VB6 Function may return a UDT (only a Declare cannot); the
            // result lands in a local first, which the Byref twin fills.
            let vb_ret = map_type_to_vb6(ret, ir);
            builder.line(&format!(
                "Public Function {} ({}) As {}",
                wrapper_name, sig_args_str, vb_ret
            ));
            builder.indent();
            builder.line(&format!("Dim r_ As {}", vb_ret));
            for l in call_lines(func, ir, &call_args, Some("r_")) {
                builder.line(&l);
            }
            builder.line(&format!("{} = r_", wrapper_name));
            builder.dedent();
            builder.line("End Function");
        }
        None => {
            builder.line(&format!("Public Sub {} ({})", wrapper_name, sig_args_str));
            builder.indent();
            for l in call_lines(func, ir, &call_args, None) {
                builder.line(&l);
            }
            builder.dedent();
            builder.line("End Sub");
        }
    }
    builder.blank();
}
