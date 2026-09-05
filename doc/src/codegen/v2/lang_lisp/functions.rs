//! `cffi:defcfun` emission for the Common Lisp generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single `(defcfun ("AzApp_create" %az-app-create) :pointer ...)` form
//! inside the `:azul-internal` package. The C-ABI symbol name is kept
//! verbatim (`AzApp_create`, `AzDom_addChild`, ...) so the resulting
//! Lisp calls hit the same exported symbols as the C / C++ bindings.
//!
//! Idiomatic CLOS-class wrappers (`(make-app ...)`, `(app-run app
//! window)`) live in `wrappers.rs` and call into these `%az-*` raw
//! functions.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::super::managed_host_invoker::managed_c_symbol;
use super::{ident_to_kebab, map_type_to_cffi, raw_fn_name};

pub fn generate_defcfuns(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Raw FFI bindings (defcfun forms).");
    builder.line(";;");
    builder.line(";; Each binding maps a verbatim C-ABI symbol to a `%`-prefixed Lisp symbol.");
    builder.line(";; The `%` prefix is the convention for \"internal raw FFI\" in the Lisp world.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_defcfun(builder, func, ir);
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

fn emit_defcfun(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let lisp_name = raw_fn_name(&func.c_name);
    // The LINKED C symbol may differ from func.c_name: functions taking a
    // callback-wrapper struct by value must bind the `<c_name>Struct`
    // export (whole wrapper struct → preserves the host-handle ctx).
    // Binding the bare `<c_name>` (which takes a fn-ptr `...Type`) makes
    // the DLL store a heap pointer as the callback and clicking jumps into
    // heap memory (EXC_BAD_ACCESS). The Lisp-side symbol name (lisp_name)
    // stays derived from c_name so wrapper call sites don't churn.
    let c_symbol = managed_c_symbol(func);

    let return_cffi = func
        .return_type
        .as_deref()
        .map(|r| map_type_to_cffi(r, ir))
        .unwrap_or_else(|| ":void".to_string());

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    if func.args.is_empty() {
        builder.line(&format!(
            "(defcfun (\"{}\" {}) {})",
            c_symbol, lisp_name, return_cffi
        ));
        builder.blank();
        return;
    }

    // Multi-line form:
    //   (defcfun ("AzApp_create" %az-app-create) :pointer
    //     (data :pointer)
    //     (config :pointer))
    builder.line(&format!(
        "(defcfun (\"{}\" {}) {}",
        c_symbol, lisp_name, return_cffi
    ));
    builder.indent();

    let last_idx = func.args.len() - 1;
    for (i, a) in func.args.iter().enumerate() {
        let cffi_ty = match a.ref_kind {
            ArgRefKind::Owned => map_type_to_cffi(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                ":pointer".to_string()
            }
        };
        let suffix = if i == last_idx { ")" } else { "" };
        builder.line(&format!(
            "({} {}){}",
            ident_to_kebab(&a.name),
            cffi_ty,
            suffix
        ));
    }
    builder.dedent();
    builder.blank();
}

fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
