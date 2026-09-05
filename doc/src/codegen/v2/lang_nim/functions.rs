//! Nim `{.importc, cdecl, dynlib.}` proc declarations for the C-ABI
//! surface.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single Nim `proc` whose body is an `importc` pragma. The Nim proc name
//! is the exact C symbol (`AzDom_addChild`); the `importc: "..."` string
//! restates it so Nim's identifier style-insensitivity can never mangle
//! the symbol the dynamic loader looks up. `dynlib: azulLib` dlopens the
//! shared library at run time.
//!
//! This raw layer is what the shipped `hello-world.nim` (and the e2e
//! test) call directly — exactly as Zig's example uses `azul.C.*`. The
//! idiomatic wrappers in `wrappers.rs` forward to these procs.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef, TypeCategory};
use super::map_type_to_nim;
use super::sanitize_identifier;
use super::types::nim_arg_type;
use super::ProcDedup;

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    procs: &mut ProcDedup,
) -> Result<()> {
    builder.line("# ============================================================================");
    builder.line("# Raw C-ABI layer — every libazul function, imported verbatim.");
    builder.line("# ============================================================================");
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_external(builder, func, ir, procs);
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

fn emit_external(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    procs: &mut ProcDedup,
) {
    for d in &func.doc {
        builder.line(&format!("# {}", super::sanitize_comment(d)));
    }

    let arg_types: Vec<String> = func.args.iter().map(|a| nim_arg_type(a, ir)).collect();
    let args: Vec<String> = func
        .args
        .iter()
        .zip(&arg_types)
        .map(|(a, nim_ty)| format!("{}: {}", sanitize_identifier(&a.name), nim_ty))
        .collect();

    let arg_str = args.join(", ");
    // Nim proc name may differ from the C symbol when two symbols collide
    // under style-insensitivity (see `ProcDedup`); the `importc` string
    // below always carries the true, unchanged C symbol.
    let proc_name = procs.unique_external(&func.c_name, &arg_types.join(","));
    let pragma = format!("{{.importc: \"{}\", cdecl, dynlib: azulLib.}}", func.c_name);

    match &func.return_type {
        Some(ret) if ret.trim() != "void" && ret.trim() != "()" => {
            let nim_ret = map_type_to_nim(ret, ir);
            builder.line(&format!(
                "proc {}*({}): {} {}",
                proc_name, arg_str, nim_ret, pragma
            ));
        }
        _ => {
            builder.line(&format!("proc {}*({}) {}", proc_name, arg_str, pragma));
        }
    }
}
