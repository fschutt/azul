//! Julia `ccall` wrapper functions for the C-ABI surface, plus the
//! idiomatic `Az`-stripped aliases.
//!
//! Every surviving IR `FunctionDef` becomes one thin Julia function that
//! forwards its arguments to `ccall((:<c_name>, LIBAZUL), Ret, (Args...,),
//! args...)`, keeping the C symbol name verbatim so the loader matches the
//! same exports as the C / Odin bindings.
//!
//! Callback handling mirrors the RAW C variant (the one Odin binds): for a
//! function that takes a callback-wrapper arg, the wrapper is replaced by
//! its bare fn-pointer typedef (which maps to `Ptr{Cvoid}`), so a
//! `@cfunction(...)` value is passed straight through — no host-invoker.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::{arg_type_for_ref_kind, map_type_to_julia, sanitize_identifier, should_emit_function};

pub fn generate_functions(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("# ----------------------------------------------------------------------------");
    b.line("# ccall wrappers: every exported libazul C symbol. Callbacks bind the raw");
    b.line("# fn-pointer variant (a `@cfunction(...)` Ptr{Cvoid} is passed directly).");
    b.line("# ----------------------------------------------------------------------------");
    b.blank();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_ccall_wrapper(b, func, ir);
    }
    b.blank();
}

fn emit_ccall_wrapper(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("# {}", d.replace('\n', " ").replace('\r', " ")));
    }

    // Functions with a callback-wrapper arg export a triple in the DLL; we
    // bind the RAW `<c_name>` variant, which takes the bare fn-pointer
    // typedef (-> Ptr{Cvoid}) in place of the wrapper struct.
    let cb_mode = has_callback_wrapper_arg(func);

    let mut names: Vec<String> = Vec::new();
    let mut types: Vec<String> = Vec::new();
    for a in &func.args {
        let effective = if cb_mode && is_callback_wrapper(&a.type_name) {
            callback_typedef_for(a.type_name.trim())
        } else {
            a.type_name.clone()
        };
        names.push(sanitize_identifier(&a.name));
        types.push(arg_type_for_ref_kind(&effective, &a.ref_kind, ir));
    }

    let ret = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_julia(r, ir))
        .unwrap_or_else(|| "Cvoid".to_string());

    // Argument-type tuple: a single element needs a trailing comma so Julia
    // reads it as a 1-tuple, not a parenthesized expression.
    let arg_types = match types.len() {
        0 => "()".to_string(),
        1 => format!("({},)", types[0]),
        _ => format!("({})", types.join(", ")),
    };

    let params = names.join(", ");
    let call_args = if names.is_empty() {
        String::new()
    } else {
        format!(", {}", names.join(", "))
    };

    b.line(&format!("function {}({})", func.c_name, params));
    b.line(&format!(
        "    ccall((:{}, LIBAZUL), {}, {}{})",
        func.c_name, ret, arg_types, call_args
    ));
    b.line("end");
    b.blank();
}

/// Emit idiomatic aliases dropping the `Az` prefix
/// (`App_create = AzApp_create`). Julia binds these as `const` values
/// referencing the wrapper functions; the raw `Az*` names remain available.
pub fn generate_aliases(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("# ----------------------------------------------------------------------------");
    b.line("# Idiomatic aliases: the same functions without the `Az` prefix.");
    b.line("# ----------------------------------------------------------------------------");

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if let Some(short) = func.c_name.strip_prefix("Az") {
            if short.is_empty() {
                continue;
            }
            if !seen.insert(short.to_string()) {
                continue;
            }
            b.line(&format!("const {} = {}", short, func.c_name));
        }
    }
    b.blank();
}
