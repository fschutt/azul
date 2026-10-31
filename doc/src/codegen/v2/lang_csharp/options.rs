//! Native optionals for C#.
//!
//! Every `Option*` mirror crosses the wrapper surface as a C# nullable, in
//! both directions: `string?` for text, the class (`FilePath?`) for a type
//! the binding owns, `T?` (`AzColorU?`, `uint?`) for a value type. A
//! parameter takes the nullable (`null` is None), a field property reads
//! and writes it, and a return value already arrives as one
//! (`wrappers::classify_return`). `?.`, `??` and `is not { } x` then work
//! on everything the API hands out.
//!
//! The conversion lives in one generated class, `__AzOpt`, with an
//! `In<Option>` (nullable to option, a class argument moved in) and a
//! `Take<Option>` (option to nullable, the option consumed) per type.
//! Options that carry callbacks or the type-erased handle are plumbing for
//! the callback machinery and keep their raw struct.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{CodegenIR, EnumVariantKind, FieldRefKind, TypeCategory},
        managed_lang_helpers::has_delete_function,
    },
    ffi_type_name,
    types::ref_kind_field_type,
    wrappers::{field_type_carries_callback, is_owning_wrapper, sanitize_class_name},
};

/// What an Option's `Some` holds, as C# sees it.
pub(super) enum Payload {
    /// The API's string: a C# `string`, copied in and out.
    Str,
    /// A class that owns native memory: moved in with `__Take()`, moved out
    /// into a new wrapper.
    Class(String),
    /// A value type (`AzColorU`, `uint`, a C enum): copied.
    Value(String),
}

/// One Option type the C# surface converts.
pub(super) struct CsOption {
    /// The api.json name (`OptionString`), which also names the bridge
    /// methods (`__AzOpt.InOptionString`).
    pub name: String,
    pub payload: Payload,
}

impl CsOption {
    /// The nullable type the surface shows (`string?`, `FilePath?`).
    pub(super) fn native(&self) -> String {
        match &self.payload {
            Payload::Str => "string?".to_string(),
            // Qualified: inside a class with a `Dom()` method a bare `Dom`
            // names the method.
            Payload::Class(c) => format!("global::Azul.{}?", c),
            Payload::Value(v) => format!("{}?", v),
        }
    }

    /// `__AzOpt.In<Option>(<expr>)`: the option for a nullable.
    pub(super) fn into_raw(&self, expr: &str) -> String {
        format!("__AzOpt.In{}({})", self.name, expr)
    }

    /// `__AzOpt.Take<Option>(<expr>)`: the nullable for an option, which it consumes.
    pub(super) fn take(&self, expr: &str) -> String {
        format!("__AzOpt.Take{}({})", self.name, expr)
    }
}

/// The conversion for `type_name` when it is an Option mirror the surface
/// shows as a nullable (see the module docs for which ones).
pub(super) fn cs_option(type_name: &str, ir: &CodegenIR) -> Option<CsOption> {
    let t = type_name.trim();
    let payload = ir.option_payload(t)?;
    let e = ir.find_enum(t)?;
    let some = e.variants.iter().find(|v| v.name == "Some")?;
    if !matches!(&some.kind, EnumVariantKind::Tuple(f) if f.len() == 1 && f[0].1 == FieldRefKind::Owned) {
        return None;
    }
    if field_type_carries_callback(t, ir) {
        return None;
    }
    ir.variant_constructor(t, "None")?;
    ir.variant_constructor(t, "Some")?;
    let payload = if ir
        .find_struct(payload)
        .is_some_and(|s| matches!(s.category, TypeCategory::String))
    {
        Payload::Str
    } else if is_owning_wrapper(payload, ir) {
        Payload::Class(sanitize_class_name(payload))
    } else {
        let ty = ref_kind_field_type(payload, &FieldRefKind::Owned, ir);
        // A recursive or unknown type the mirror only holds as a pointer.
        if ty == "IntPtr" {
            return None;
        }
        Payload::Value(ty)
    };
    Some(CsOption {
        name: t.to_string(),
        payload,
    })
}

/// `internal static class __AzOpt`: the `In` / `Take` pair of every
/// converted Option type.
pub(super) fn emit_option_bridge(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.line(
        "/// <summary>The C# nullable of every Option type, in both directions. `In*` builds \
         the native option (null is None, a class argument is moved in); `Take*` takes one \
         apart (the option is consumed, None is null).</summary>",
    );
    builder.line("internal static class __AzOpt");
    builder.line("{");
    builder.indent();
    for e in &ir.enums {
        let Some(opt) = cs_option(&e.name, ir) else {
            continue;
        };
        let ffi = ffi_type_name(&e.name);
        let none = format!("NativeMethods.{}_none()", ffi);
        let some = |v: &str| format!("NativeMethods.{}_some({})", ffi, v);
        let native = opt.native();
        match &opt.payload {
            Payload::Str => {
                builder.line(&format!(
                    "internal static {ffi} In{name}(string? v) => v is null ? {none} : {};",
                    some("__AzField.MakeString(v)"),
                    name = opt.name,
                ));
                builder.line(&format!("internal static string? Take{}({} o)", opt.name, ffi));
                builder.line("{");
                builder.indent();
                builder.line("var __n = o.AsNullable();");
                builder.line("if (__n == null) return null;");
                builder.line("var __s = __AzField.ReadString(__n.Value);");
                if has_delete_function(&e.name, ir) {
                    builder.line(&format!("__AzDerive.Consume(o, NativeMethods.{}_delete);", ffi));
                }
                builder.line("return __s;");
                builder.dedent();
                builder.line("}");
            }
            Payload::Class(class) => {
                builder.line(&format!(
                    "internal static {ffi} In{name}({native} v) => v is null ? {none} : {};",
                    some("v.__Take()"),
                    name = opt.name,
                ));
                // The payload moves into the new wrapper: the option holds
                // nothing else, so it is not freed.
                builder.line(&format!(
                    "internal static {native} Take{name}({ffi} o) {{ var __n = o.AsNullable(); \
                     return __n == null ? null : new global::Azul.{class}(__n.Value); }}",
                    name = opt.name,
                ));
            }
            Payload::Value(_) => {
                builder.line(&format!(
                    "internal static {ffi} In{name}({native} v) => v.HasValue ? {} : {none};",
                    some("v.Value"),
                    name = opt.name,
                ));
                builder.line(&format!(
                    "internal static {native} Take{name}({ffi} o) => o.AsNullable();",
                    name = opt.name,
                ));
            }
        }
    }
    builder.dedent();
    builder.line("}");
    builder.blank();
}
