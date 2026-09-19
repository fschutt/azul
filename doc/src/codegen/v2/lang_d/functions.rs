//! The raw layer's functions: one `extern(C)` declaration per exported libazul
//! symbol, with the C name verbatim.
//!
//! A function taking a callback-wrapper argument is exported as a pair the
//! binding uses both halves of: `<c_name>` takes the bare function pointer
//! typedef (what a plain `extern(C)` function's address is passed to), and
//! `<c_name>Struct` takes the wrapper struct whole, which is how the idiomatic
//! layer hands libazul a trampoline plus the `ctx` RefAny naming the D function.
//!
//! Every declaration is `nothrow @nogc`: libazul never throws D exceptions and
//! never touches the D GC (callbacks re-enter D through trampolines that catch
//! everything).

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionDef},
        managed_host_invoker::{
            callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
        },
    },
    arg_type_for_ref_kind, map_type_to_d, raw_identifier, should_emit_function,
};

pub fn generate_extern_block(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// C ABI functions: every exported libazul symbol.");
    b.line("// ----------------------------------------------------------------------------");
    b.line("extern (C) nothrow @nogc");
    b.line("{");
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        emit_extern_proc(b, func, ir, false);
        if has_callback_wrapper_arg(func) {
            emit_extern_proc(b, func, ir, true);
        }
    }
    b.line("}");
    b.blank();
}

fn emit_extern_proc(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR, wrapper_struct: bool) {
    let cb_mode = has_callback_wrapper_arg(func) && !wrapper_struct;
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
            format!("{} {}", ty, raw_identifier(&a.name))
        })
        .collect();
    let ret = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_d(r, ir))
        .unwrap_or_else(|| "void".to_string());
    let name = if wrapper_struct {
        format!("{}Struct", func.c_name)
    } else {
        func.c_name.clone()
    };
    b.line(&format!("    {} {}({});", ret, name, args.join(", ")));
}
