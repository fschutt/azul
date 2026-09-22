//! Crystal `fun` bindings for the C-ABI surface (inside `lib LibAzul`).
//!
//! Every surviving IR `FunctionDef` becomes one
//! `fun <crystal_name> = <CName>(...) : Ret` line inside the `lib` block,
//! keeping the C symbol name verbatim on the right so the linker matches
//! the same exports as the C / Zig / Odin bindings. The Crystal-side name
//! lowercases the first letter of the C name (a method name must not be
//! capitalized).
//!
//! A function that takes a callback-wrapper arg is exported as a triple
//! (see `managed_host_invoker::has_callback_wrapper_arg`). Both forms are
//! bound: `<c_name>` with the bare proc typedef in the wrapper's place (what
//! a non-capturing Crystal proc passes straight into), and `<c_name>Struct`
//! with the whole wrapper struct, which is how the idiomatic layer hands
//! libazul a trampoline plus the closure handle in the wrapper's ctx.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionDef},
        managed_host_invoker::{
            has_callback_wrapper_arg, shadow_callback_typedef,
        },
    },
    arg_type_for_ref_kind, crystal_fun_name, map_type_to_crystal, sanitize_identifier,
    should_emit_function,
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
        if has_callback_wrapper_arg(func) {
            emit_struct_variant(b, func, ir);
        }
    }
    b.blank();
}

/// `<c_name>Struct`: the api.json signature verbatim, wrapper struct by value.
fn emit_struct_variant(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let c_name = format!("{}Struct", func.c_name);
    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let ty = arg_type_for_ref_kind(&a.type_name, &a.ref_kind, ir);
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
        crystal_fun_name(&c_name),
        c_name,
        args.join(", "),
        ret
    ));
}

fn emit_fun(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    for d in &func.doc {
        b.line(&format!("  # {}", d.replace(['\n', '\r'], " ")));
    }

    // Functions with a callback-wrapper arg export a triple in the DLL; we
    // bind the RAW `<c_name>` variant, which takes the bare fn-pointer
    // typedef in place of the wrapper struct.
    let cb_mode = has_callback_wrapper_arg(func);

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let effective = match shadow_callback_typedef(func, a) {                Some(td) if cb_mode => td.to_string(),                _ => a.type_name.clone(),            };
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
