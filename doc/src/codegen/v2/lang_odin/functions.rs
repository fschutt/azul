//! Odin `foreign azul { ... }` declarations for the C-ABI surface, plus
//! the idiomatic `Az`-stripped procedure aliases.
//!
//! Every surviving IR `FunctionDef` becomes one `proc(...) ---` line
//! inside a `@(default_calling_convention="c")` foreign block, keeping
//! the C symbol name verbatim so the linker matches the same exports as
//! the C / Zig bindings.
//!
//! Callback handling mirrors the RAW C variant (the one Zig sees through
//! `azul.h`): for a function that takes a callback-wrapper arg, the
//! wrapper is replaced by its bare fn-pointer typedef proc-type, and we
//! bind the raw `<c_name>` symbol. That is exactly the declaration whose
//! argument a plain `proc "c"` value can be passed to — no host-invoker
//! machinery.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::{arg_type_for_ref_kind, map_type_to_odin, sanitize_identifier, should_emit_function};

pub fn generate_foreign_block(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Foreign block: every exported libazul C symbol. Callbacks bind the raw");
    b.line("// fn-pointer variant (a `proc \"c\"` value is passed directly).");
    b.line("// ----------------------------------------------------------------------------");
    b.line("@(default_calling_convention=\"c\")");
    b.line("foreign azul {");

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_foreign_proc(b, func, ir);
    }

    b.line("}");
    b.blank();
}

fn emit_foreign_proc(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("\t// {}", d.replace('\n', " ").replace('\r', " ")));
    }

    // Functions with a callback-wrapper arg export a triple in the DLL;
    // we bind the RAW `<c_name>` variant, which takes the bare fn-pointer
    // typedef in place of the wrapper struct.
    let cb_mode = has_callback_wrapper_arg(func);

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let effective = if cb_mode && is_callback_wrapper(&a.type_name) {
                callback_typedef_for(a.type_name.trim())
            } else {
                a.type_name.clone()
            };
            let ty = arg_type_for_ref_kind(&effective, &a.ref_kind, ir);
            format!("{}: {}", sanitize_identifier(&a.name), ty)
        })
        .collect();

    let ret = func
        .return_type
        .as_ref()
        .map(|r| format!(" -> {}", map_type_to_odin(r, ir)))
        .unwrap_or_default();

    b.line(&format!(
        "\t{} :: proc({}){} ---",
        func.c_name,
        args.join(", "),
        ret
    ));
}

/// Emit idiomatic aliases dropping the `Az` prefix
/// (`App_create :: AzApp_create`). Odin binds these as compile-time
/// procedure-value aliases; the raw `Az*` symbols remain available.
pub fn generate_aliases(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic aliases: the same procedures without the `Az` prefix.");
    b.line("// ----------------------------------------------------------------------------");

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        if let Some(short) = func.c_name.strip_prefix("Az") {
            if short.is_empty() {
                continue;
            }
            b.line(&format!("{} :: {}", short, func.c_name));
        }
    }
    b.blank();
}
