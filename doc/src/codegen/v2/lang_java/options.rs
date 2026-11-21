//! Native optionals for Java.
//!
//! Every `Option*` mirror crosses the wrapper surface as Java's own
//! optional: a parameter takes the plain value or `null` (None), a field
//! getter and a return value give a `java.util.Optional<T>`. `T` is
//! `java.lang.String` for text, the wrapper class (`FilePath`) for a type
//! the binding owns, and what the mirror's `toNullable()` gives for
//! anything else (a boxed primitive, a JNA struct).
//!
//! The conversion lives in one generated class, `AzulOptions`, with an
//! `in<Option>` (value or null to the native option, a wrapper moved in)
//! and a `take<Option>` (native option to `Optional`, the option consumed)
//! per type. Options that carry callbacks stay raw: they are plumbing for
//! the callback machinery. Scala uses these classes as they are
//! (`Option(x)` / `.toScala` on an `Optional`).

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, EnumVariantKind, FieldRefKind, FunctionKind, TypeCategory},
        managed_host_invoker::is_callback_wrapper,
        managed_lang_helpers::{has_delete_function, has_wrapper_class, is_refany_type},
    },
    emit_file, ffi_type_name,
    functions::native_class_for_class,
    types::{java_boxed, ref_kind_field_type},
    wrappers::wrapper_class_name,
};

/// What an Option's `Some` holds, as Java sees it.
pub(crate) enum Payload {
    /// The API's string: a `java.lang.String`, copied in and out.
    Str,
    /// A type with a wrapper class (the class name): moved in, moved out.
    Class(String),
    /// Anything else, as the mirror's `toNullable()` type (a boxed
    /// primitive or a JNA struct): copied.
    Value(String),
}

/// One Option type the Java surface converts.
pub(crate) struct JvmOption {
    /// The api.json name (`OptionString`): it names the bridge methods.
    pub name: String,
    pub payload: Payload,
}

impl JvmOption {
    /// The Java type of the value inside (`java.lang.String`, `FilePath`, `Integer`).
    pub(crate) fn value_type(&self) -> String {
        match &self.payload {
            Payload::Str => "java.lang.String".to_string(),
            Payload::Class(c) | Payload::Value(c) => c.clone(),
        }
    }

    /// `AzulOptions.in<Option>(<expr>)`: the native option for a value or null.
    pub(crate) fn into_raw(&self, expr: &str) -> String {
        format!("AzulOptions.in{}({})", self.name, expr)
    }

    /// `AzulOptions.take<Option>(<expr>)`: the `Optional` for a native option, which it consumes.
    pub(crate) fn take(&self, expr: &str) -> String {
        format!("AzulOptions.take{}({})", self.name, expr)
    }
}

/// Does `t`, or the payload of `t`, carry a callback or the type-erased
/// handle? Such options are wired by the callback machinery.
fn carries_callback(t: &str, ir: &CodegenIR) -> bool {
    is_callback_wrapper(ir, t)
        || is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
}

/// The conversion for `type_name` when it is an Option mirror the surface
/// shows as Java's own optional (see the module docs for which ones).
pub(crate) fn jvm_option(type_name: &str, ir: &CodegenIR) -> Option<JvmOption> {
    let t = type_name.trim();
    let payload = ir.option_payload(t)?;
    let e = ir.find_enum(t)?;
    let some = e.variants.iter().find(|v| v.name == "Some")?;
    if !matches!(&some.kind, EnumVariantKind::Tuple(f) if f.len() == 1 && f[0].1 == FieldRefKind::Owned) {
        return None;
    }
    if carries_callback(payload, ir) {
        return None;
    }
    ir.variant_constructor(t, "None")?;
    ir.variant_constructor(t, "Some")?;
    let payload = if ir
        .find_struct(payload)
        .is_some_and(|s| matches!(s.category, TypeCategory::String))
    {
        Payload::Str
    } else if has_wrapper_class(payload, ir) {
        // Moving a wrapper out needs it to own the bytes; moving one in needs
        // `__takeRaw`, which every class with a `_delete` has.
        if !has_delete_function(payload, ir) {
            return None;
        }
        Payload::Class(wrapper_class_name(payload))
    } else {
        // A heap-owning value without a class would be freed with the option.
        if has_delete_function(t, ir) {
            return None;
        }
        let ty = java_boxed(&ref_kind_field_type(payload, &FieldRefKind::Owned, ir));
        if ty == "Pointer" {
            return None;
        }
        Payload::Value(ty)
    };
    Some(JvmOption {
        name: t.to_string(),
        payload,
    })
}

/// `AzulOptions.java`: the `in` / `take` pair of every converted Option.
pub(super) fn emit_options_file(out: &mut String, ir: &CodegenIR, config: &CodegenConfig) -> anyhow::Result<()> {
    let string_class = ir
        .structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))
        .map(|s| s.name.clone());
    let chunk = emit_file(
        "AzulOptions.java",
        |b| {
            emit_options_class(b, ir, string_class.as_deref());
            Ok(())
        },
        config,
    )?;
    out.push_str(&chunk);
    Ok(())
}

fn emit_options_class(b: &mut CodeBuilder, ir: &CodegenIR, string_class: Option<&str>) {
    b.line("/**");
    b.line(" * Java's own optional for every Option type, in both directions: {@code in*}");
    b.line(" * builds the native option (null is None, a wrapper is moved in), {@code take*}");
    b.line(" * takes one apart into a {@code java.util.Optional} (the option is consumed).");
    b.line(" */");
    b.line("final class AzulOptions {");
    b.indent();
    b.line("private AzulOptions() {}");
    b.blank();
    if let Some(s) = string_class {
        let native = native_class_for_class(s, ir);
        let ffi = ffi_type_name(s);
        b.line("/** A fresh native string holding a copy of {@code v}. */");
        b.line(&format!("static {}.ByValue str(java.lang.String v) {{", ffi));
        b.indent();
        b.line("byte[] __b = v.getBytes(java.nio.charset.StandardCharsets.UTF_8);");
        b.line("com.sun.jna.Memory __m = new com.sun.jna.Memory(Math.max(1, __b.length));");
        b.line("__m.write(0, __b, 0, __b.length);");
        b.line(&format!("return {}.{}_fromUtf8(__m, __b.length);", native, ffi));
        b.dedent();
        b.line("}");
        b.blank();
        b.line("/** The text of a native string; the string itself is left alone. */");
        b.line(&format!("static java.lang.String readStr({} s) {{", ffi));
        b.indent();
        b.line("s.write();");
        b.line("Pointer __vp = s.getPointer().getPointer(0);");
        b.line("long __vl = s.getPointer().getLong(8);");
        b.line("if (__vp == null || __vl <= 0) return \"\";");
        b.line(
            "return new java.lang.String(__vp.getByteArray(0, (int) __vl), \
             java.nio.charset.StandardCharsets.UTF_8);",
        );
        b.dedent();
        b.line("}");
        b.blank();
    }
    for e in &ir.enums {
        let Some(opt) = jvm_option(&e.name, ir) else {
            continue;
        };
        let ffi = ffi_type_name(&e.name);
        let native = native_class_for_class(&e.name, ir);
        let none = format!("{}.{}_none()", native, ffi);
        let some = |v: &str| format!("{}.{}_some({})", native, ffi, v);
        let delete = ir
            .functions
            .iter()
            .any(|f| f.class_name == e.name && matches!(f.kind, FunctionKind::Delete))
            .then(|| format!("{}.{}_delete(o.getPointer());", native, ffi));
        let vt = opt.value_type();
        b.line(&format!("static {}.ByValue in{}({} v) {{", ffi, opt.name, vt));
        b.indent();
        b.line(&format!("if (v == null) return {};", none));
        match &opt.payload {
            Payload::Str => b.line(&format!("return {};", some("str(v)"))),
            Payload::Class(_) => b.line(&format!("return {};", some("v.__takeRaw()"))),
            Payload::Value(ty) => {
                // A JNA struct is passed by value through its `.ByValue`
                // twin, a boxed primitive unboxes on its own.
                if ty.starts_with("Az") {
                    b.line("v.write();");
                    b.line(&format!(
                        "{t}.ByValue __v = Structure.newInstance({t}.ByValue.class, v.getPointer());",
                        t = ty
                    ));
                    b.line("__v.read();");
                    b.line(&format!("return {};", some("__v")));
                } else {
                    b.line(&format!("return {};", some("v")));
                }
            }
        }
        b.dedent();
        b.line("}");
        b.line(&format!(
            "static java.util.Optional<{}> take{}({} o) {{",
            vt, opt.name, ffi
        ));
        b.indent();
        match &opt.payload {
            Payload::Str => {
                b.line(&format!("{} __n = o.toNullable();", string_class.map(ffi_type_name).unwrap_or_default()));
                b.line("java.lang.String __s = __n == null ? null : readStr(__n);");
                if let Some(d) = &delete {
                    b.line(d);
                }
                b.line("return java.util.Optional.ofNullable(__s);");
            }
            Payload::Class(class) => {
                // The payload's bytes move into a fresh wrapper's memory; the
                // option holds nothing else, so it is not freed.
                let pffi = ffi_type_name(ir.option_payload(&e.name).unwrap_or_default());
                b.line(&format!("{} __n = o.toNullable();", pffi));
                b.line("if (__n == null) return java.util.Optional.empty();");
                b.line("com.sun.jna.Memory __m = new com.sun.jna.Memory(__n.size());");
                b.line("__m.write(0, __n.getPointer().getByteArray(0, __n.size()), 0, __n.size());");
                b.line(&format!("return java.util.Optional.of(new {}(__m));", class));
            }
            Payload::Value(_) => {
                b.line("return java.util.Optional.ofNullable(o.toNullable());");
            }
        }
        b.dedent();
        b.line("}");
        b.blank();
    }
    b.dedent();
    b.line("}");
}
