//! `define-azul` emission for the Racket generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes one
//! `(define-azul AzApp_create (_fun _AzRefAny _AzAppConfig -> _AzApp))`
//! form. The Racket identifier is kept verbatim as the C-ABI symbol
//! (`AzApp_create`, `AzDom_addChild`, ...) so these bindings hit the same
//! exported symbols as the C / C++ / CFFI bindings.
//!
//! Idiomatic non-prefixed wrappers (`(dom-add-child dom child)`) live in
//! `wrappers.rs` and call into these raw `Az*` bindings.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::super::managed_host_invoker::managed_c_symbol;
use super::{kebab, map_type_to_racket};

pub fn generate_defines(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Raw FFI bindings (define-azul forms).");
    builder.line(";;");
    builder.line(";; Each binding maps a verbatim C-ABI symbol to a same-named Racket value.");
    builder.line(";; The `_fun` type turns a Racket closure passed for a fn-ptr arg into a real");
    builder.line(";; C callback (libffi), so no separate trampoline layer is needed.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_define(builder, func, ir);
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

fn emit_define(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    // The LINKED C symbol may differ from func.c_name: a function taking a
    // callback-wrapper struct by value must bind the `<c_name>Struct`
    // export (whole wrapper struct → preserves the host-handle ctx). The
    // Racket identifier stays `func.c_name` so wrapper call sites don't
    // churn; `#:c-id` retargets the linked symbol when they differ.
    let racket_id = &func.c_name;
    let c_symbol = managed_c_symbol(func);

    let ret = func
        .return_type
        .as_deref()
        .map(|r| map_type_to_racket(r, ir))
        .unwrap_or_else(|| "_void".to_string());

    for d in &func.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }

    let arg_types: Vec<String> = func
        .args
        .iter()
        .map(|a| match a.ref_kind {
            ArgRefKind::Owned => map_type_to_racket(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "_pointer".to_string()
            }
        })
        .collect();

    // Annotate each arg with its kebab name as a comment-free label via the
    // labelled `_fun` form so the signature reads well.
    let sig = if arg_types.is_empty() {
        format!("(_fun -> {})", ret)
    } else {
        let labelled: Vec<String> = func
            .args
            .iter()
            .zip(arg_types.iter())
            .map(|(a, t)| format!("[{} : {}]", kebab(&a.name), t))
            .collect();
        format!("(_fun {} -> {})", labelled.join(" "), ret)
    };

    if c_symbol == *racket_id {
        builder.line(&format!("(define-azul {} {})", racket_id, sig));
    } else {
        builder.line(&format!(
            "(define-azul {} {} #:c-id {})",
            racket_id, sig, c_symbol
        ));
    }
}

fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
