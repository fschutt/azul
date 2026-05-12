//! Kotlin-idiomatic wrapper-class emission.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit:
//!
//! ```kotlin
//! class App private constructor(private val ptr: Pointer) : AutoCloseable {
//!     companion object {
//!         fun create(...): App = App(AzulNative.INSTANCE.AzApp_create(...).pointer!!)
//!     }
//!     override fun close() = AzulNative.INSTANCE.AzApp_delete(ptr)
//!     fun run(...) = AzulNative.INSTANCE.AzApp_run(ptr, ...)
//! }
//! ```
//!
//! Kotlin's stdlib already provides `AutoCloseable.use { }` so call
//! sites get `App.create(...).use { app -> app.run(...) }` for free.
//!
//! Tagged-union enums get a tiny helper `object` with static factory
//! methods per unit variant; payload-bearing variants are left to the
//! user (`Az<Foo>` Union + `Az<Foo>Variant_<Variant>` payload struct).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef,
    TypeCategory,
};
use super::{ffi_type_name, map_kt_owned, map_kt_return, sanitize_kt_identifier};

pub fn emit_all(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Idiomatic Kotlin wrappers (AutoCloseable + companion-object factories).");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_wrapper(builder, s, ir);
    }

    for e in &ir.enums {
        if !should_emit_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e);
    }

    Ok(())
}

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
    has_delete(&s.name, ir)
}

fn should_emit_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
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

fn has_delete(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && f.kind == FunctionKind::Delete)
}

fn emit_wrapper(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = sanitize_kt_identifier(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &s.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
        
    }

    builder.line(&format!(
        "class {} private constructor(private val ptr: Pointer) : AutoCloseable {{",
        class_name
    ));
    builder.indent();

    builder.line("private var closed: Boolean = false");
    builder.blank();

    // Internal pointer access for sibling wrappers.
    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("fun rawPointer(): Pointer = ptr");
    builder.blank();

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a `kotlin.String`. AzString's C-side layout is
    // `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`,
    // so offset 0 is `vec.ptr` (the UTF-8 byte buffer) and offset 8 is
    // `vec.len` (byte length).
    if s.name == "String" {
        builder.line("/**");
        builder.line(" * Decode the wrapped UTF-8 bytes into a `kotlin.String`.");
        builder.line(" * Reads `vec.ptr` (offset 0) and `vec.len` (offset 8) from");
        builder.line(" * the AzString struct directly via JNA.");
        builder.line(" */");
        builder.line("override fun toString(): kotlin.String {");
        builder.indent();
        builder.line("if (closed) return \"\"");
        builder.line("val vecPtr: Pointer? = ptr.getPointer(0)");
        builder.line("val vecLen: Long = ptr.getLong(8)");
        builder.line("if (vecPtr == null || vecLen <= 0) return \"\"");
        builder.line("val bytes = vecPtr.getByteArray(0, vecLen.toInt())");
        builder.line("return kotlin.String(bytes, Charsets.UTF_8)");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // companion object holding the static factories.
    let static_funcs: Vec<&FunctionDef> = ir
        .functions_for_class(&s.name)
        .filter(|f| {
            !f.kind.is_trait_function()
                && matches!(
                    f.kind,
                    FunctionKind::Constructor
                        | FunctionKind::StaticMethod
                        | FunctionKind::Default
                )
        })
        .collect();

    if !static_funcs.is_empty() {
        builder.line("companion object {");
        builder.indent();
        for func in static_funcs {
            emit_static_factory(builder, &class_name, &ffi_name, func, ir);
        }
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Instance methods.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
        ) {
            continue;
        }
        emit_instance_method(builder, &class_name, &ffi_name, func, ir);
    }

    // close()
    builder.line("/** Frees the underlying native resources. Idempotent. */");
    builder.line("override fun close() {");
    builder.indent();
    builder.line("if (closed) return");
    builder.line(&format!(
        "{}.INSTANCE.{}_delete(ptr)",
        super::super::lang_java::functions::native_class_for_class(&s.name, ir),
        ffi_name
    ));
    builder.line("closed = true");
    builder.dedent();
    builder.line("}");

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_static_factory(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);

    let return_kt = func
        .return_type
        .as_ref()
        .map(|r| map_kt_return(r, ir))
        .unwrap_or_else(|| "Unit".to_string());

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let arg_sig: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let kt = match a.ref_kind {
                ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "Pointer?".to_string(),
            };
            format!("{}: {}", sanitize_kt_identifier(&a.name), kt)
        })
        .collect();

    // No wrapper-boundary callback substitution. The wrapper signature
    // carries the C ABI type unchanged; users call AzulHostInvoker.register*
    // themselves to construct the wrapper struct. Matches C# / Java / Lua.
    let call_args: Vec<String> = func
        .args
        .iter()
        .map(|a| sanitize_kt_identifier(&a.name))
        .collect();

    if !func.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &func.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
        
    }

    let displayed_return = if returns_self {
        class_name.to_string()
    } else {
        return_kt.clone()
    };

    builder.line(&format!(
        "@JvmStatic fun {}({}): {} {{",
        method_name,
        arg_sig.join(", "),
        displayed_return
    ));
    builder.indent();

    // Use `func.c_name` to match the AzulNative interface (where
    // functions are declared by their C ABI symbol with camelCase
    // method portion) rather than `func.method_name` (raw snake-case
    // from api.json) which produces e.g. `AzFoo_with_resolver` instead
    // of the actual `AzFoo_withResolver`.
    let call = format!(
        "{}.INSTANCE.{}({})",
        super::super::lang_java::functions::native_class_for_func(func, ir),
        func.c_name,
        call_args.join(", ")
    );

    if return_kt == "Unit" {
        builder.line(&format!("{}", call));
    } else if returns_self {
        // ByValue → adopt its underlying Pointer.
        builder.line(&format!("val raw = {}", call));
        builder.line(&format!("return {}(raw.pointer)", class_name));
    } else {
        builder.line(&format!("return {}", call));
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_instance_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);

    let return_kt = func
        .return_type
        .as_ref()
        .map(|r| map_kt_return(r, ir))
        .unwrap_or_else(|| "Unit".to_string());

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // Drop the implicit self argument. For instance methods (caller path
    // is `emit_instance_method` so takes_self is always true here) the
    // first arg in func.args IS the self regardless of how api.json names
    // it (`instance`, lowercased class name, `icon_provider_handle`,
    // etc.). Skip args[0] unconditionally — matches the Java/C# fix.
    let user_args: Vec<_> = func.args.iter().skip(1).collect();

    // Some C ABIs take self by VALUE (`AzRibbon_renderDom(AzRibbon)`)
    // rather than by pointer. Detect via args[0].ref_kind = Owned and
    // build a `.ByValue` overlay via JNA's Structure.newInstance(...).
    let self_by_value = func
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);

    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let kt = match a.ref_kind {
                ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "Pointer?".to_string(),
            };
            format!("{}: {}", sanitize_kt_identifier(&a.name), kt)
        })
        .collect();

    let mut pre_call_lines: Vec<String> = Vec::new();
    let self_arg = if self_by_value {
        let self_ty = format!("Az{}", func.class_name);
        pre_call_lines.push(format!(
            "val __self = Structure.newInstance({}.ByValue::class.java, this.ptr) as {}.ByValue",
            self_ty, self_ty
        ));
        pre_call_lines.push("__self.read()".to_string());
        "__self".to_string()
    } else {
        "this.ptr".to_string()
    };

    let mut call_args: Vec<String> = vec![self_arg];
    // Callback args: no wrapper-boundary substitution. The wrapper
    // signature already matches the C ABI type; users construct the
    // AzCallback struct themselves via AzulHostInvoker.register*.
    for a in &user_args {
        let raw_name = sanitize_kt_identifier(&a.name);
        call_args.push(raw_name);
    }

    if !func.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &func.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
        
    }

    let displayed_return = if returns_self {
        class_name.to_string()
    } else {
        return_kt.clone()
    };

    builder.line(&format!(
        "fun {}({}): {} {{",
        method_name,
        arg_sig.join(", "),
        displayed_return
    ));
    builder.indent();
    builder.line("check(!closed) { \"closed\" }");

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `func.c_name` to match the AzulNative interface (where
    // functions are declared by their C ABI symbol with camelCase
    // method portion) rather than `func.method_name` (raw snake-case
    // from api.json) which produces e.g. `AzFoo_with_resolver` instead
    // of the actual `AzFoo_withResolver`.
    let call = format!(
        "{}.INSTANCE.{}({})",
        super::super::lang_java::functions::native_class_for_func(func, ir),
        func.c_name,
        call_args.join(", ")
    );

    if return_kt == "Unit" {
        builder.line(&format!("{}", call));
    } else if returns_self {
        builder.line(&format!("val raw = {}", call));
        builder.line(&format!("return {}(raw.pointer)", class_name));
    } else {
        builder.line(&format!("return {}", call));
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class_name = sanitize_kt_identifier(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &e.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
        
    }

    builder.line(&format!("object {}Helpers {{", class_name));
    builder.indent();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let mname = idiomatic_method_name(&v.name);
                let variant_ident = sanitize_kt_identifier(&v.name);
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "@JvmStatic fun {}(): {} {{",
                    mname, ffi_name
                ));
                builder.indent();
                builder.line(&format!("val u = {}()", ffi_name));
                builder.line(&format!(
                    "u.{}.tag = {}_Tag.{}.value",
                    variant_ident, ffi_name, variant_ident
                ));
                builder.line(&format!("u.setType(\"{}\")", v.name));
                builder.line("return u");
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
    builder.blank();
}

fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    let camel = if method_name.contains('_') {
        let mut out = String::new();
        let mut upper = false;
        for c in method_name.chars() {
            if c == '_' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // Kotlin hard keywords (`object`, `class`, `interface`, etc.) cannot
    // be method names without backticks; emit them backticked. (Backticks
    // are valid inside method-name position in Kotlin source.)
    if super::is_kotlin_hard_keyword(&camel) {
        format!("`{}`", camel)
    } else if camel == "close" {
        // Every wrapper implements AutoCloseable with its own `close()`
        // for resource cleanup. A user-API method also named `close`
        // would collide; rename it. (SvgPath has both — the path's "close
        // path" segment plus the AutoCloseable.close() lifecycle method.)
        "closeInner".to_string()
    } else if matches!(camel.as_str(), "toString" | "hashCode" | "equals") {
        // Methods on Any/Object require an `override` modifier and
        // a compatible return type. The Azul wrappers' `toString` returns
        // AzString.ByValue, not java.lang.String, so it can't override
        // Any.toString. Suffix to avoid the collision.
        format!("{}_", camel)
    } else {
        camel
    }
}

/// Escape doc-comment text for KDoc emission. Several characters in
/// the raw Rust docs would otherwise confuse Kotlin's parser:
///
/// - `*/` inside paths like `/users/*/name` is read as the doc-comment
///   terminator, prematurely closing the KDoc and surfacing as
///   "Missing '}" / "Unclosed comment" errors on later lines.
/// - `{` / `}` are KDoc inline-tag delimiters. Unbalanced braces from
///   inline code samples (`r#"{"users":...}"#`) trip the doc parser.
pub(crate) fn kdoc_escape(s: &str) -> String {
    s.replace("*/", "*&#47;")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
}

