//! AutoCloseable wrapper-class emission for the Java JNA generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit a `public final class <TypeName> implements
//! AutoCloseable` that:
//!
//! - Holds the underlying JNA `Pointer` in a private field
//! - Provides `close()` calling `AzulNative.INSTANCE.Az<Type>_delete(ptr)`
//! - Surfaces every non-trait method on the IR class as either an
//!   instance method (`fn(self, ...)`) or a `public static` factory
//!   (`fn() -> Self`)
//!
//! Tagged-union enums get a separate, very minimal helper class with
//! static factories per unit variant. (Payload-bearing variants are
//! left for the user to construct via the generated `Az<Type>` Union
//! plus the matching `<Type>Variant_<Variant>` payload struct — see
//! `types.rs`.)

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef,
    TypeCategory,
};
use super::{emit_file, ffi_type_name, map_jvm_type, map_jvm_type_byvalue, sanitize_identifier, snake_to_lower_camel};

// ============================================================================
// Top-level driver
// ============================================================================

pub fn emit_all_wrapper_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        let class_name = wrapper_class_name(&s.name);
        let chunk = emit_file(
            &format!("{}.java", class_name),
            |b| {
                emit_wrapper_class(b, s, ir);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        let helper_name = format!("{}Helpers", wrapper_class_name(&e.name));
        let chunk = emit_file(
            &format!("{}.java", helper_name),
            |b| {
                emit_union_helper(b, e);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    Ok(())
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    has_delete_function(&s.name, ir)
}

fn should_emit_union_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
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
// Wrapper emission
// ============================================================================

fn emit_wrapper_class(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = wrapper_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        builder.line("/**");
        for d in &s.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!(
        "public final class {} implements AutoCloseable {{",
        class_name
    ));
    builder.indent();

    builder.line("private Pointer ptr;");
    builder.line("private boolean closed;");
    builder.blank();

    // Internal pointer-wrapping constructor (package-private).
    builder.line(&format!(
        "/** Wrap an existing native {} pointer; takes ownership. */",
        ffi_name
    ));
    builder.line(&format!(
        "{}(Pointer ptr) {{ this.ptr = ptr; this.closed = false; }}",
        class_name
    ));
    builder.blank();

    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("public Pointer rawPointer() { return ptr; }");
    builder.blank();

    // Methods.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        emit_wrapper_method(builder, &class_name, func, ir);
    }

    // close() / AutoCloseable.
    emit_close_method(builder, &s.name, &class_name);

    builder.dedent();
    builder.line("}");
}

fn emit_close_method(builder: &mut CodeBuilder, raw_type_name: &str, class_name: &str) {
    builder.line("/** Frees the underlying native resources. Idempotent. */");
    builder.line("@Override");
    builder.line("public void close() {");
    builder.indent();
    builder.line("if (closed || ptr == null) return;");
    builder.line(&format!(
        "AzulNative.INSTANCE.Az{}_delete(ptr);",
        raw_type_name
    ));
    builder.line("ptr = null;");
    builder.line("closed = true;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Defensive finalizer in case the user forgets try-with-resources.
    builder.line("@Override");
    builder.line("@SuppressWarnings(\"deprecation\")");
    builder.line("protected void finalize() throws Throwable {");
    builder.indent();
    builder.line("try { close(); } finally { super.finalize(); }");
    builder.dedent();
    builder.line("}");
    let _ = class_name;
}

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let _ = ffi_type_name(&func.class_name);

    let return_jvm = func
        .return_type
        .as_ref()
        .map(|r| map_jvm_type_byvalue(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // For takes_self methods the first arg in `func.args` IS the
    // implicit self, regardless of the name the api.json gave it
    // (`instance`, lowercased class name, etc.). Skip args[0] unconditionally
    // when takes_self; otherwise filter by conventional self names so a
    // legitimate user arg named after the class still passes through.
    let user_args: Vec<_> = if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        let class_lower = func.class_name.to_lowercase();
        func.args
            .iter()
            .filter(|a| a.name != class_lower && a.name != "self")
            .collect()
    };

    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let jt = match a.ref_kind {
                ArgRefKind::Owned => map_jvm_type_byvalue(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "Pointer".to_string(),
            };
            format!("{} {}", jt, sanitize_identifier(&a.name))
        })
        .collect();

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Some C ABIs take self by VALUE (e.g. `AzRibbon_renderDom(AzRibbon r)`)
    // rather than by pointer (`AzFoo_*(IntPtr instance, ...)`). Detect via
    // the first arg's ref_kind (Owned = by value). When self-by-value, we
    // construct a `Az<Type>.ByValue` whose Pointer points at our heap-held
    // instance and pass it in.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);

    let mut pre_call_lines: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        if self_by_value {
            // Build a JNA `.ByValue` Structure overlaying our pointer
            // via the public Structure.newInstance(Class, Pointer)
            // factory. `useMemory` is protected and not callable from
            // here; `newInstance` is the canonical replacement.
            let self_ty = ffi_type_name(&func.class_name);
            pre_call_lines.push(format!(
                "{}.ByValue __self = Structure.newInstance({}.ByValue.class, this.ptr);",
                self_ty, self_ty
            ));
            pre_call_lines.push("__self.read();".to_string());
            call_args.push("__self".to_string());
        } else {
            call_args.push("this.ptr".to_string());
        }
    }
    // Callback args: do NOT auto-substitute at the wrapper-method
    // boundary. The wrapper signature carries the C ABI type (e.g.
    // `AzCallback.ByValue` or `AzCallbackType`) and is passed through
    // unchanged. Users construct the wrapper struct via
    // `AzulHostInvoker.register*(handler)` themselves and pass that.
    // (Same conclusion C# / Lua reached.)
    for a in &user_args {
        let raw_name = sanitize_identifier(&a.name);
        call_args.push(raw_name);
    }

    if !func.doc.is_empty() {
        builder.line("/**");
        for d in &func.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let displayed_return = if returns_self {
        class_name.to_string()
    } else {
        return_jvm.clone()
    };

    let modifiers = if is_static { "public static" } else { "public" };

    builder.line(&format!(
        "{} {} {}({}) {{",
        modifiers,
        displayed_return,
        method_name,
        arg_sig.join(", ")
    ));
    builder.indent();

    if !is_static {
        builder.line("if (closed) throw new IllegalStateException(\"closed\");");
    }

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `func.c_name` directly — it is already the camelCase native
    // symbol name (`AzFoo_withCapacity`) that matches the AzulNative
    // interface declarations. Reconstructing it from method_name yields
    // snake_case (`AzFoo_with_capacity`) which drifts from the actual
    // C ABI symbol the codegen registered.
    let call = format!(
        "AzulNative.INSTANCE.{}({})",
        func.c_name,
        call_args.join(", ")
    );

    if return_jvm == "void" {
        builder.line(&format!("{};", call));
    } else if returns_self {
        // The C ABI returned a struct-by-value; the JNA shim returned a
        // ByValue Structure. We adopt its `Pointer` for the wrapper.
        builder.line(&format!("{} __raw = {};", return_jvm, call));
        builder.line(&format!(
            "return new {}(__raw.getPointer());",
            class_name
        ));
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
    let class_name = wrapper_class_name(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        builder.line("/**");
        for d in &e.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!("public final class {}Helpers {{", class_name));
    builder.indent();
    builder.line(&format!("private {}Helpers() {{}}", class_name));
    builder.blank();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let mname = idiomatic_method_name(&v.name);
                let variant_ident = sanitize_identifier(&v.name);
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "public static {} {}() {{",
                    ffi_name, mname
                ));
                builder.indent();
                builder.line(&format!("{} u = new {}();", ffi_name, ffi_name));
                builder.line(&format!(
                    "u.{}.tag = (byte) {}_Tag.{}.value;",
                    variant_ident, ffi_name, variant_ident
                ));
                builder.line(&format!("u.setType(\"{}\");", v.name));
                builder.line("return u;");
                builder.dedent();
                builder.line("}");
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                builder.line(&format!(
                    "// SKIPPED: variant {}.{} carries a payload — set the variant",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "// via {0}.{1} fields directly (see {0}Variant_{1}).",
                    ffi_name, v.name
                ));
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
}

// ============================================================================
// Helpers
// ============================================================================

fn wrapper_class_name(raw: &str) -> String {
    sanitize_identifier(raw)
}

fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    let camel = if method_name.contains('_') {
        snake_to_lower_camel(method_name)
    } else {
        // Already lowerCamel (most common in api.json) or PascalCase;
        // ensure first letter is lowercase for Java methods.
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // `default`, `class`, `case`, etc. can't be method names. Java has
    // no verbatim-identifier syntax, so append `_`.
    // Also rename `close` because every wrapper implements AutoCloseable
    // with its own `close()` for resource cleanup — a user-API method
    // also named `close` would be a duplicate-definition error. The
    // SvgPath bug (an SVG path's "close path" segment vs the lifecycle
    // close) showed up first; the rule generalises.
    if super::is_java_reserved(&camel) {
        format!("{}_", camel)
    } else if camel == "close" {
        "closeInner".to_string()
    } else if matches!(camel.as_str(), "toString" | "hashCode" | "equals" | "getClass" | "clone" | "finalize") {
        // Methods declared on java.lang.Object have fixed signatures.
        // A user-API method called `toString` returning `AzString.ByValue`
        // cannot legally override `Object.toString()` (which returns
        // `java.lang.String`), so suffix it. Same family covers
        // hashCode / equals / clone / finalize / getClass.
        format!("{}_", camel)
    } else {
        camel
    }
}

fn javadoc_escape(s: &str) -> String {
    // Java's javadoc parser interprets `\u` / `\U` as Unicode escapes
    // (even inside comments — see JLS §3.3). Doc strings like
    // `C:\Users\username` contain `\U` which is parsed as the start of
    // an invalid Unicode escape sequence and rejected. Double the
    // backslashes so the literal text survives.
    s.replace('\\', "\\\\")
        .replace("*/", "*&#47;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('&', "&amp;")
}
