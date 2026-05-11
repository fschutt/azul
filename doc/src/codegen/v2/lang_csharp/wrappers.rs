//! Idiomatic C# wrapper-class emission for the C# generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit a `public class TypeName : IDisposable` that:
//!
//! - Holds the raw FFI struct (`AzTypeName`) by value in a private field
//! - Exposes `Dispose()` / a finalizer / `Dispose(bool)` calling
//!   `NativeMethods.AzTypeName_delete(...)`
//! - Surfaces every non-trait method on `TypeName` as an idiomatic instance
//!   or static method that delegates to the underlying P/Invoke import
//!
//! Plain POD structs without a `_delete` and unit enums get *no* wrapper —
//! the user manipulates them through the FFI struct/enum directly.
//!
//! Tagged-union enums get a separate, very minimal "discriminator hierarchy"
//! emitted by [`generate_union_hierarchies`]; for now this is intentionally
//! narrow (we expose the tag enum and a static factory per unit variant).
//! Full pattern-matching support can be expanded later without breaking
//! the surface area.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef,
    TypeCategory,
};
use super::{ffi_type_name, map_type_to_csharp, sanitize_identifier, snake_to_pascal};

// ============================================================================
// Public entry points (called from mod.rs)
// ============================================================================

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Idiomatic IDisposable wrapper classes.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_wrapper_class(builder, s, ir);
    }

    Ok(())
}

pub fn generate_union_hierarchies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let mut emitted_header = false;

    for e in &ir.enums {
        if !should_emit_union_hierarchy(e, config) {
            continue;
        }

        if !emitted_header {
            builder.line(
                "// --------------------------------------------------------------------------",
            );
            builder.line("// Tagged-union convenience helpers (static factories per variant).");
            builder.line(
                "// --------------------------------------------------------------------------",
            );
            builder.blank();
            emitted_header = true;
        }

        emit_union_helper(builder, e);
    }

    Ok(())
}

// ============================================================================
// Wrapper inclusion filter
// ============================================================================

fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    match s.category {
        TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::DestructorOrClone
        | TypeCategory::GenericTemplate => return false,
        _ => {}
    }
    has_delete_function(&s.name, ir)
}

fn should_emit_union_hierarchy(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    if matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    ) {
        return false;
    }
    e.is_union
}

fn has_delete_function(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && f.kind == FunctionKind::Delete)
}

// ============================================================================
// Wrapper class emission
// ============================================================================

fn emit_wrapper_class(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = sanitize_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    builder.line(&format!("public sealed class {} : IDisposable", class_name));
    builder.line("{");
    builder.indent();

    // Storage and disposal flag.
    builder.line(&format!("private {} _inner;", ffi_name));
    builder.line("private bool _disposed;");
    builder.blank();

    // Internal accessors used by sibling wrapper classes.
    builder.line(&format!(
        "/// <summary>Returns the underlying FFI struct by value (use with care).</summary>"
    ));
    builder.line(&format!("internal {} Raw => _inner;", ffi_name));
    builder.blank();
    builder.line("/// <summary>Wrap an existing raw FFI struct (takes ownership).</summary>");
    builder.line(&format!(
        "internal {}({} inner) {{ _inner = inner; }}",
        class_name, ffi_name
    ));
    builder.blank();

    // Emit methods for each non-trait function on this class.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            // Skip Delete/PartialEq/Cmp/Hash/Debug — Dispose() and overrides cover these.
            continue;
        }
        emit_wrapper_method(builder, &class_name, func, ir);
    }

    // IDisposable boilerplate.
    emit_dispose_methods(builder, &class_name, &s.name);

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_dispose_methods(builder: &mut CodeBuilder, class_name: &str, raw_type_name: &str) {
    builder.line("/// <summary>Frees the underlying native resources.</summary>");
    builder.line("public void Dispose()");
    builder.line("{");
    builder.indent();
    builder.line("Dispose(true);");
    builder.line("GC.SuppressFinalize(this);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("private void Dispose(bool disposing)");
    builder.line("{");
    builder.indent();
    builder.line("if (_disposed) return;");
    builder.line("// `disposing` is false when called from the finalizer; native");
    builder.line("// cleanup is still safe because the FFI struct is value-typed.");
    // Use Marshal.AllocHGlobal/StructureToPtr instead of `fixed` so the
    // emit is compatible with PowerShell's Add-Type (PS 7's Roslyn
    // wrapper has no /unsafe option). Slight overhead — one extra alloc
    // — but the call is at Dispose time only.
    builder.line(&format!(
        "var __p = System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.InteropServices.Marshal.SizeOf<{}>());",
        ffi_type_name(raw_type_name)
    ));
    builder.line(&format!(
        "System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, __p, false);",
    ));
    builder.line(&format!(
        "NativeMethods.Az{}_delete(__p);",
        raw_type_name
    ));
    builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__p);");
    builder.line("_disposed = true;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line(&format!("~{}() {{ Dispose(false); }}", class_name));
    builder.blank();
}

// ============================================================================
// Method emission
// ============================================================================

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let ffi_class_name = ffi_type_name(&func.class_name);

    // Skip auto-generated default constructors with no body — there's
    // nothing meaningful to surface beyond `new T()`.
    let _ = func.is_const;

    let return_cs = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_csharp(r, ir))
        .unwrap_or_else(|| "void".to_string());

    // Arguments: skip the implicit `self` (named after the lowercased class
    // name) — we forward `this` through the inner FFI struct.
    let class_lower = func.class_name.to_lowercase();
    let user_args: Vec<_> = func
        .args
        .iter()
        .filter(|a| a.name != class_lower && a.name != "self")
        .collect();

    // Build argument signature.
    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let cs_type = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_csharp(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "IntPtr".to_string(),
            };
            format!("{} {}", cs_type, sanitize_identifier(&a.name))
        })
        .collect();

    // Build call arguments. The first parameter of an instance method is
    // the implicit self; emit `ref _inner` for it. User arguments pass
    // through unchanged.
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        // The FFI signature accepts `IntPtr` for &self / &mut self; we
        // bridge to it through a `fixed` block in the method body.
        call_args.push("(IntPtr)__self".to_string());
    }
    for a in &user_args {
        let raw_name = sanitize_identifier(&a.name);
        // Auto-route callback args through HostInvoker.Register<Wrapper>(...)
        // so the user can pass a plain delegate. Only kinds with
        // `impl_managed_callback!` applied are substituted.
        if let Some(cb) = a.callback_info.as_ref() {
            let wrapper = cb.callback_wrapper_name.as_str();
            if super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper) {
                call_args.push(format!(
                    "HostInvoker.Register{}({})",
                    wrapper, raw_name
                ));
                continue;
            }
        }
        call_args.push(raw_name);
    }

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Emit doc comment.
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    // Constructors and static factories return a wrapper around the
    // returned FFI struct; instance methods simply delegate.
    //
    // Methods inherited from `System.Object` (ToString, GetHashCode,
    // Equals, GetType) need `new` (or `override`) when our signature
    // SHADOWS the inherited one. Only fires for exact-signature
    // collisions — `GetType()` shadows, but `GetType(arg)` doesn't.
    // The common collider in practice is `ToString()` returning
    // `Az*` instead of `string`.
    let arg_count = arg_sig.len();
    let needs_new = !is_static
        && match method_name.as_str() {
            "ToString" => arg_count == 0,
            "GetHashCode" => arg_count == 0,
            "GetType" => arg_count == 0,
            "Equals" => arg_count == 1,
            "MemberwiseClone" => arg_count == 0,
            _ => false,
        };
    let modifiers = if is_static {
        "public static".to_string()
    } else if needs_new {
        "public new".to_string()
    } else {
        "public".to_string()
    };

    // Decide whether the return type should be wrapped. If the FFI
    // function returns the same struct as the wrapping class, wrap it.
    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let displayed_return = if returns_self {
        class_name.to_string()
    } else {
        return_cs.clone()
    };

    builder.line(&format!(
        "{} {} {}({})",
        modifiers.as_str(),
        displayed_return,
        method_name,
        arg_sig.join(", ")
    ));
    builder.line("{");
    builder.indent();

    if !is_static {
        builder.line("if (_disposed) throw new ObjectDisposedException(nameof(_inner));");
    }

    let call = format!(
        "NativeMethods.{}_{}({})",
        ffi_class_name,
        func.method_name,
        call_args.join(", ")
    );

    // If the method receives `self`, produce an IntPtr to a heap-copy of
    // the FFI struct. Avoid `fixed`/`unsafe` so the same emit works under
    // PowerShell's Add-Type (no /unsafe option in PS 7's Roslyn wrapper).
    // Slight alloc cost per call; we copy back on return to mirror
    // mutation through `out` semantics.
    if takes_self {
        builder.line(&format!(
            "var __self = System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.InteropServices.Marshal.SizeOf<{}>());",
            ffi_class_name
        ));
        builder.line("try");
        builder.line("{");
        builder.indent();
        builder.line(&format!(
            "System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, __self, false);",
        ));
        if return_cs == "void" {
            builder.line(&format!("{};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
        } else if returns_self {
            builder.line(&format!("var __raw = {};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            builder.line(&format!("return new {}(__raw);", class_name));
        } else {
            builder.line(&format!("var __ret = {};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            builder.line("return __ret;");
        }
        builder.dedent();
        builder.line("}");
        builder.line("finally");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__self);");
        builder.dedent();
        builder.line("}");
    } else if return_cs == "void" {
        builder.line(&format!("{};", call));
    } else if returns_self {
        builder.line(&format!("var __raw = {};", call));
        builder.line(&format!("return new {}(__raw);", class_name));
    } else {
        builder.line(&format!("return {};", call));
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Tagged-union helper class
// ============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class_name = sanitize_class_name(&e.name);
    let ffi_name = ffi_type_name(&e.name);
    let tag_name = format!("{}_Tag", ffi_name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    builder.line(&format!("public static class {}Helpers", class_name));
    builder.line("{");
    builder.indent();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let pascal = snake_to_pascal(&v.name);
                builder.line(&format!(
                    "/// <summary>Construct the {}.{} variant.</summary>",
                    e.name, v.name
                ));
                builder.line(&format!("public static {} {}()", ffi_name, pascal));
                builder.line("{");
                builder.indent();
                builder.line(&format!("var u = new {}();", ffi_name));
                // Set tag on the variant's payload struct slot.
                let variant_field = sanitize_identifier(&v.name);
                builder.line(&format!(
                    "u.{}.tag = {}.{};",
                    variant_field,
                    tag_name,
                    sanitize_identifier(&v.name)
                ));
                builder.line("return u;");
                builder.dedent();
                builder.line("}");
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                // SKIPPED: payload-bearing variants need per-payload
                // overloads which depend on the FFI struct layout. The
                // user can construct these via the public FFI struct
                // fields directly.
                builder.line(&format!(
                    "// SKIPPED: variant {}.{} has payload — set fields directly on the FFI struct.",
                    e.name, v.name
                ));
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Pick a safe C# class name. We use the IR type name verbatim (no `Az`
/// prefix) because the wrapper lives inside `namespace Azul`.
fn sanitize_class_name(raw: &str) -> String {
    sanitize_identifier(raw)
}

/// Convert an api.json method name (typically already camelCase) to a
/// PascalCase C# method name, with a few special-casings.
fn idiomatic_method_name(method_name: &str) -> String {
    // Treat `new` specially — C# `new` is a keyword, surface it as
    // `Create` on the wrapper class.
    if method_name == "new" {
        return "Create".to_string();
    }

    // If it's already in lowerCamelCase or snake_case, normalise.
    if method_name.contains('_') {
        snake_to_pascal(method_name)
    } else {
        // Capitalise the first character.
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
