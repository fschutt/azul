//! P/Invoke `[DllImport]` extern emission for the C# generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single `static extern` declaration inside an `internal static class
//! NativeMethods`. The C-ABI symbol name is preserved verbatim
//! (`AzApp_create`, `AzDom_addChild`, etc.) so that the resulting
//! P/Invoke calls hit the same exported symbols as the C / C++
//! bindings.
//!
//! Idiomatic, namespaced wrappers (`Azul.App.Create(...)`) live in
//! `wrappers.rs` and call into `NativeMethods.AzApp_create(...)`.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{map_type_to_csharp, sanitize_identifier, DLL_NAME};

pub fn generate_native_methods(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// NativeMethods: every DLL-exported C-ABI function as a P/Invoke import.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    // `public` (not `internal`) so PowerShell scripts using Add-Type
    // can call the FFI helpers directly. C# consumers should prefer
    // the idiomatic wrapper classes (App, Dom, Button, ...) and treat
    // these as low-level escape hatches.
    builder.line("public static class NativeMethods");
    builder.line("{");
    builder.indent();
    builder.line(&format!("public const string DllName = \"{}\";", DLL_NAME));
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_dll_import(builder, func, ir);
    }

    builder.dedent();
    builder.line("}");
    builder.blank();

    Ok(())
}

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&func.class_name) {
        return false;
    }
    // Skip categories that are not exposed in C# at all.
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

fn emit_dll_import(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let return_type = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_csharp(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let cs_type = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_csharp(&a.type_name, ir),
                // For references and pointers we use IntPtr in the
                // P/Invoke signature. Wrappers can pass `ref` / `out`
                // for blittable structs when convenient.
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "IntPtr".to_string(),
            };
            format!("{} {}", cs_type, sanitize_identifier(&a.name))
        })
        .collect();

    builder.line(
        "[DllImport(DllName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]",
    );
    if return_type == "bool" {
        // .NET Marshal default is BOOL (4 bytes) which does not match the
        // Rust C ABI's `bool` (1 byte). Force I1.
        builder.line("[return: MarshalAs(UnmanagedType.I1)]");
    }
    builder.line(&format!(
        "public static extern {} {}({});",
        return_type,
        func.c_name,
        args.join(", ")
    ));
    builder.blank();
}
