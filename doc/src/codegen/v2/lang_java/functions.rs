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
use std::collections::BTreeMap;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{emit_file, map_jvm_type_byvalue, sanitize_identifier, LIBRARY_NAME};

/// Look up the api.json module that owns `class_name`. Falls back to
/// `"misc"` when the IR has no mapping — that bucket exists so
/// generated trait functions (`_delete`, `_clone`, …) on types like
/// `MonitorId` (no module entry) still land somewhere.
pub fn module_for_class(class_name: &str, ir: &CodegenIR) -> String {
    ir.type_to_module
        .get(class_name)
        .cloned()
        .unwrap_or_else(|| "misc".to_string())
}

/// Map an api.json module name (`app`, `dom`, …) to the per-module
/// JNA class that owns its native methods (`AzulNativeApp`,
/// `AzulNativeDom`, …). The class is `public final class AzulNative<X>`
/// in `lang_java`'s emitted source.
pub fn module_native_class(module: &str) -> String {
    let mut chars = module.chars();
    match chars.next() {
        Some(c) => format!("AzulNative{}{}", c.to_uppercase().collect::<String>(), chars.as_str()),
        None => "AzulNativeMisc".to_string(),
    }
}

/// Convenience: native-class for a function's owning class.
pub fn native_class_for_func(func: &FunctionDef, ir: &CodegenIR) -> String {
    module_native_class(&module_for_class(&func.class_name, ir))
}

/// Convenience: native-class for a (typically wrapper) class name.
pub fn native_class_for_class(class_name: &str, ir: &CodegenIR) -> String {
    module_native_class(&module_for_class(class_name, ir))
}

/// Emit one `AzulNative<Module>.java` file per api.json module.
///
/// Background: a single monolithic `AzulNative` class with ~1700
/// `public static native` methods is well under the JVM's per-class
/// limit, BUT it pushes against JNA tooling assumptions and gives a
/// poor library-author experience (one 35k-line file to navigate).
/// More importantly, the same code shape is the only thing that lets
/// Kotlin's JNA Proxy mode fit — the Proxy `<clinit>` bytecode
/// scales with method count and breaks past 64KB. Split into modules
/// (matches the structure already present in api.json) and every JVM
/// binding gets a natural, idiomatic shape.
///
/// Each per-module class still uses `Native.register("azul")` direct
/// mapping (proven by commit 6e7cc7bd2) so behaviour is unchanged
/// modulo where the methods now live.
pub fn generate_native_module_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    // Group functions by module.
    let mut by_module: BTreeMap<String, Vec<&FunctionDef>> = BTreeMap::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        by_module
            .entry(module_for_class(&func.class_name, ir))
            .or_default()
            .push(func);
    }

    for (module, funcs) in &by_module {
        let class_name = module_native_class(module);
        let filename = format!("{}.java", class_name);
        let class_name = class_name.clone();
        let funcs = funcs.clone();
        out.push_str(&emit_file(
            &filename,
            |builder| {
                builder.line("/**");
                builder.line(&format!(
                    " * JNA-bound static class for the {} module of the Azul C ABI.",
                    module
                ));
                builder.line(" * <p>");
                builder.line(" * One {@code public static native} method per exported C symbol");
                builder.line(" * in this module. Bound via {@code Native.register(\"azul\")}");
                builder.line(" * direct mapping — no JNA Proxy class, no 64KB &lt;clinit&gt; limit.");
                builder.line(" * <p>");
                builder.line(" * Callers: write {@code AzulNative<Module>.foo(...)} directly,");
                builder.line(" * or {@code AzulNative<Module>.INSTANCE.foo(...)} for the legacy");
                builder.line(" * INSTANCE-style invocation shape (Java resolves static-method");
                builder.line(" * calls through any expression of the class's type, so the");
                builder.line(" * INSTANCE marker exists purely for syntactic compatibility).");
                builder.line(" */");
                builder.line(&format!("public final class {} {{", class_name));
                builder.indent();
                builder.line(&format!(
                    "static {{ Native.register(\"{}\"); }}",
                    LIBRARY_NAME
                ));
                builder.line(&format!(
                    "public static final {} INSTANCE = null;",
                    class_name
                ));
                builder.line(&format!("private {}() {{}}", class_name));
                builder.blank();
                for func in &funcs {
                    emit_native_method(builder, func, ir);
                }
                builder.dedent();
                builder.line("}");
                Ok(())
            },
            config,
        )?);
    }

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
    // Functions with a callback-wrapper arg bind the `<c_name>Struct`
    // C symbol (whole wrapper struct by value — matches the ByValue
    // signature declared here). The raw `<c_name>` takes a bare fn ptr
    // at the C ABI; declaring it with these args is an ABI mismatch.
    builder.line(&format!(
        "public static native {} {}({});",
        return_type,
        super::super::managed_host_invoker::managed_c_symbol(func),
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

