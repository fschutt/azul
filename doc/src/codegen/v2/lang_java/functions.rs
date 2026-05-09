//! Native FFI interface emission for the Java JNA generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! method on the `interface AzulNative extends Library` declaration.
//! JNA dispatches each call through `libffi` to the matching exported
//! C symbol; the C-ABI symbol name is preserved verbatim
//! (`AzApp_create`, `AzDom_addChild`, ...) so the same prebuilt
//! `azul.{dll,so,dylib}` artefact serves every binding.
//!
//! The interface owns a single `INSTANCE` static field — JNA uses it
//! as the entry point for every native call:
//!
//! ```java
//! AzulNative.INSTANCE.AzApp_create(...);
//! ```

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{map_jvm_type, sanitize_identifier, LIBRARY_NAME};

/// Generate the `interface AzulNative extends Library { ... }`
/// declaration. Caller-owns the surrounding `package` + import block.
pub fn generate_native_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("/**");
    builder.line(" * JNA-bound interface to the Azul C ABI.");
    builder.line(" * <p>");
    builder.line(" * Each method maps 1:1 to an exported C symbol of the same name. JNA");
    builder.line(" * resolves every call through libffi at runtime; the {@code INSTANCE}");
    builder.line(" * field is the singleton bridge to the loaded native library.");
    builder.line(" */");
    builder.line("public interface AzulNative extends Library {");
    builder.indent();

    builder.line(&format!(
        "AzulNative INSTANCE = Native.load(\"{}\", AzulNative.class);",
        LIBRARY_NAME
    ));
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_native_method(builder, func, ir);
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

fn emit_native_method(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let return_type = func
        .return_type
        .as_ref()
        .map(|r| map_jvm_type_for_return(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let jt = match a.ref_kind {
                ArgRefKind::Owned => map_jvm_type_for_owned_arg(&a.type_name, ir),
                // Pass-by-pointer at the C level → JNA Pointer.
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "Pointer".to_string(),
            };
            format!("{} {}", jt, sanitize_identifier(&a.name))
        })
        .collect();

    if !func.doc.is_empty() {
        builder.line("/**");
        for d in &func.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }
    builder.line(&format!(
        "{} {}({});",
        return_type,
        func.c_name,
        args.join(", ")
    ));
    builder.blank();
}

/// For an "Owned" argument (pass-by-value), use `<Type>.ByValue` if
/// the target is a generated `Structure`, otherwise fall back to the
/// primitive mapping.
fn map_jvm_type_for_owned_arg(type_name: &str, ir: &CodegenIR) -> String {
    let raw = map_jvm_type(type_name, ir);
    if raw.starts_with("Az") {
        format!("{}.ByValue", raw)
    } else {
        raw
    }
}

/// Returns are passed by value for Structures.
fn map_jvm_type_for_return(type_name: &str, ir: &CodegenIR) -> String {
    map_jvm_type_for_owned_arg(type_name, ir)
}

/// Escape characters that are illegal in a Javadoc comment body.
///
/// Java's javadoc parser interprets `\u` / `\U` as Unicode escapes
/// (even inside comments — see JLS §3.3). Doc strings like
/// `C:\Users\username` contain `\U` which is parsed as the start of an
/// invalid Unicode escape sequence and rejected. Double the
/// backslashes so the literal text survives.
fn javadoc_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace("*/", "*&#47;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('&', "&amp;")
}

