//! D `extern(C) { ... }` declarations for the C-ABI surface, plus the
//! idiomatic `Az`-stripped function aliases.
//!
//! Every surviving IR `FunctionDef` becomes one `Ret Cname(args);` line
//! inside an `extern(C)` block, keeping the C symbol name verbatim so the
//! linker matches the same exports as the C / Zig / Odin bindings.
//!
//! Callback handling mirrors the RAW C variant (the one Zig sees through
//! `azul.h`): for a function that takes a callback-wrapper arg, the
//! wrapper is replaced by its bare fn-pointer typedef, and we bind the raw
//! `<c_name>` symbol. That is exactly the declaration whose argument a
//! plain `extern(C)` function's address can be passed to — no host-invoker
//! machinery.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::{arg_type_for_ref_kind, map_type_to_d, sanitize_identifier, should_emit_function};

pub fn generate_extern_block(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// extern(C) block: every exported libazul C symbol. Callbacks bind the raw");
    b.line("// fn-pointer variant (an `extern(C)` function's address is passed directly).");
    b.line("// ----------------------------------------------------------------------------");
    b.line("extern(C) {");

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_extern_proc(b, func, ir);
    }

    b.line("}");
    b.blank();
}

fn emit_extern_proc(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("\t// {}", d.replace('\n', " ").replace('\r', " ")));
    }

    // Functions with a callback-wrapper arg export a triple in the DLL; we
    // bind the RAW `<c_name>` variant, which takes the bare fn-pointer
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
            format!("{} {}", ty, sanitize_identifier(&a.name))
        })
        .collect();

    let ret = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_d(r, ir))
        .unwrap_or_else(|| "void".to_string());

    b.line(&format!(
        "\t{} {}({});",
        ret,
        func.c_name,
        args.join(", ")
    ));
}

/// Emit idiomatic aliases dropping the `Az` prefix
/// (`alias App_create = AzApp_create;`). D binds these as symbol aliases;
/// the raw `Az*` symbols remain available.
pub fn generate_aliases(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic aliases: the same functions without the `Az` prefix.");
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
            b.line(&format!("alias {} = {};", short, func.c_name));
        }
    }
    b.blank();
}
