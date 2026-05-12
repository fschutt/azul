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
use super::{map_jvm_type_byvalue, sanitize_identifier, LIBRARY_NAME};

/// Generate the `interface AzulNative extends Library { ... }`
/// declaration. Caller-owns the surrounding `package` + import block.
pub fn generate_native_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("/**");
    builder.line(" * JNA-bound class exposing the Azul C ABI.");
    builder.line(" * <p>");
    builder.line(" * Each method maps 1:1 to an exported C symbol of the same name.");
    builder.line(" * <p>");
    builder.line(" * Uses {@code Native.register(\"azul\")} (direct mapping) instead of");
    builder.line(" * {@code Native.load(...)} (Proxy mode). Proxy mode generates a single");
    builder.line(" * dynamic class with a {@code <clinit>} that calls {@code Method.getMethod}");
    builder.line(" * for every interface method; with ~1700 FFI exports that bytecode");
    builder.line(" * exceeds the JVM's 64KB per-method limit and throws at class init.");
    builder.line(" * Direct mapping binds each {@code native} method individually via JNI,");
    builder.line(" * with no Proxy class to generate.");
    builder.line(" * <p>");
    builder.line(" * Legacy callers may still write {@code AzulNative.INSTANCE.foo(...)};");
    builder.line(" * the Java compiler resolves that to the static {@code AzulNative.foo(...)}");
    builder.line(" * call (with a hint warning). The wrapper classes use both forms.");
    builder.line(" */");
    builder.line("public final class AzulNative {");
    builder.indent();

    // `static {}` block fires when the class is first loaded — equivalent
    // to the old `INSTANCE = Native.load(...)` initialiser but binds every
    // `public static native` method below to its libazul export.
    builder.line(&format!(
        "static {{ Native.register(\"{}\"); }}",
        LIBRARY_NAME
    ));
    // INSTANCE kept as a marker so legacy `AzulNative.INSTANCE.foo(...)`
    // call sites still compile. `final null` is fine: Java resolves
    // static-method calls through any expression of the class's type,
    // including null, with only a hint warning.
    builder.line("public static final AzulNative INSTANCE = null;");
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
        "public static native {} {}({});",
        return_type,
        func.c_name,
        args.join(", ")
    ));
    builder.blank();
}

/// For an "Owned" argument (pass-by-value), use `<Type>.ByValue` when
/// the target is a generated `Structure` (POD struct or tagged-union
/// enum). Java `public enum` types and JNA `Callback` interfaces have
/// no `.ByValue` inner class, so they pass through unchanged.
/// Delegates to the shared `map_jvm_type_byvalue`.
fn map_jvm_type_for_owned_arg(type_name: &str, ir: &CodegenIR) -> String {
    map_jvm_type_byvalue(type_name, ir)
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

