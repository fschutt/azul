//! V extern declarations for the C-ABI surface: one top-level
//! `fn C.AzFoo(...)` per exported libazul symbol.
//!
//! V has no "foreign block" — each C function is declared individually as
//! a bodyless `fn C.<c_name>(...) <ret>`. The `C.` namespace is global, so
//! any module (including the `package main` driver) can call
//! `C.AzApp_create` once `azul.v` is compiled into the build; the `#flag`
//! lines in `mod.rs` provide the link flags. Symbol names match the C
//! bindings verbatim so the linker resolves the same exports as the C /
//! Zig / Odin bindings.
//!
//! Callback handling mirrors the RAW C variant (the one Zig/Odin see): for
//! a function that takes a callback-wrapper arg, the wrapper is replaced by
//! its bare fn-pointer typedef, and we bind the raw `<c_name>` symbol. That
//! is exactly the declaration a plain top-level V `fn` value can be passed
//! to — no host-invoker machinery.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::{arg_type_for_ref_kind, map_type_to_v, sanitize_identifier, should_emit_function};

pub fn generate_externs(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Extern declarations: every exported libazul C symbol. Call these through");
    b.line("// the `C.` namespace (C.AzApp_create). Callbacks bind the raw fn-pointer");
    b.line("// variant — a top-level V `fn` value is passed directly.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_extern(b, func, ir);
    }
    b.blank();

    emit_string_helpers(b);
}

/// Emit idiomatic String helpers into `module azul`. `AzString_fromUtf8`
/// COPIES the bytes into a refcounted AzString, so passing a temporary V
/// string is safe. These replace the hand-rolled `az_str` every driver
/// would otherwise copy out of the example.
fn emit_string_helpers(b: &mut CodeBuilder) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic String helper. AzString_fromUtf8 copies the bytes into a");
    b.line("// refcounted AzString, so passing a temporary V string is safe.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line("// Copy a V string into a refcounted AzString.");
    b.line("pub fn az_str(s string) AzString {");
    b.line("\treturn C.AzString_fromUtf8(s.str, usize(s.len))");
    b.line("}");
    b.blank();
}

fn emit_extern(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("// {}", d.replace('\n', " ").replace('\r', " ")));
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
            format!("{} {}", sanitize_identifier(&a.name), ty)
        })
        .collect();

    let ret = func
        .return_type
        .as_ref()
        .map(|r| format!(" {}", map_type_to_v(r, ir)))
        .unwrap_or_default();

    b.line(&format!(
        "fn C.{}({}){}",
        func.c_name,
        args.join(", "),
        ret
    ));
}
