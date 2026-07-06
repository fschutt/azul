//! Crystal `fun` bindings for the C-ABI surface (inside `lib LibAzul`),
//! plus idiomatic `Az`-stripped type aliases in a `module Azul`.
//!
//! Every surviving IR `FunctionDef` becomes one
//! `fun <crystal_name> = <CName>(...) : Ret` line inside the `lib` block,
//! keeping the C symbol name verbatim on the right so the linker matches
//! the same exports as the C / Zig / Odin bindings. The Crystal-side name
//! lowercases the first letter of the C name (a method name must not be
//! capitalized).
//!
//! Callback handling mirrors the RAW C variant (the one Zig sees through
//! `azul.h`): for a function that takes a callback-wrapper arg, the
//! wrapper is replaced by its bare proc-typedef, and we bind the raw
//! `<c_name>` symbol. That is exactly the declaration whose argument a
//! plain non-capturing Crystal proc can be passed to — no host-invoker
//! machinery.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::{
    arg_type_for_ref_kind, crystal_fun_name, ffi_type_name, map_type_to_crystal,
    sanitize_identifier, should_emit_function,
};

pub fn generate_funs(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("  # --------------------------------------------------------------------------");
    b.line("  # C-ABI functions: every exported libazul symbol. The Crystal-side name");
    b.line("  # lowercases the first letter (azApp_create); the C symbol is kept verbatim");
    b.line("  # after `=`. Callbacks bind the raw fn-pointer variant.");
    b.line("  # --------------------------------------------------------------------------");
    b.blank();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_fun(b, func, ir);
    }
    b.blank();
}

fn emit_fun(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("  # {}", d.replace('\n', " ").replace('\r', " ")));
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
            format!("{} : {}", sanitize_identifier(&a.name), ty)
        })
        .collect();

    let ret = func
        .return_type
        .as_ref()
        .map(|r| format!(" : {}", map_type_to_crystal(r, ir)))
        .unwrap_or_default();

    b.line(&format!(
        "  fun {} = {}({}){}",
        crystal_fun_name(&func.c_name),
        func.c_name,
        args.join(", "),
        ret
    ));
}

/// Emit idiomatic type aliases dropping the `Az` prefix inside a
/// `module Azul` (`Azul::Dom = LibAzul::AzDom`). The aliases are
/// namespaced so they never shadow Crystal core types (e.g. `String`).
/// The raw `LibAzul::Az*` names — and every `fun` binding — remain the
/// canonical entry points.
pub fn generate_type_aliases(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("# ----------------------------------------------------------------------------");
    b.line("# Idiomatic aliases: the same types without the `Az` prefix, namespaced under");
    b.line("# `Azul::` so they never shadow Crystal core types. Functions stay on LibAzul.");
    b.line("# ----------------------------------------------------------------------------");
    b.line("module Azul");

    let mut seen: BTreeSet<String> = BTreeSet::new();

    let mut emit_alias = |b: &mut CodeBuilder, seen: &mut BTreeSet<String>, short: &str| {
        if short.is_empty() {
            return;
        }
        if !seen.insert(short.to_string()) {
            return;
        }
        b.line(&format!(
            "  alias {} = {}::{}",
            short,
            super::LIB_MODULE,
            ffi_type_name(short)
        ));
    };

    for s in &ir.structs {
        if config.should_include_type(&s.name) {
            emit_alias(b, &mut seen, &s.name);
        }
    }
    for e in &ir.enums {
        if config.should_include_type(&e.name) {
            emit_alias(b, &mut seen, &e.name);
        }
    }
    for ta in &ir.type_aliases {
        if config.should_include_type(&ta.name) {
            emit_alias(b, &mut seen, &ta.name);
        }
    }
    for cb in &ir.callback_typedefs {
        emit_alias(b, &mut seen, &cb.name);
    }

    b.line("end");
    b.blank();
}
