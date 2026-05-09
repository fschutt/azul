//! Struct, enum, and callback delegate emission for the C# generator.
//!
//! Strategy:
//! - **Unit-only enums** -> `public enum AzFoo : uint { ... }`. We do not
//!   emit explicit numeric values; the C ABI uses sequential numbering
//!   from 0, which matches the C# default.
//! - **Tagged-union enums** (`is_union == true`) -> a tag enum
//!   `AzFoo_Tag : uint` plus per-variant `[StructLayout(Sequential)]`
//!   structs (`AzFooVariant_Bar`) plus an `[StructLayout(Explicit)]`
//!   `AzFoo` struct with `[FieldOffset(0)]` for each variant. This is
//!   layout-compatible with the C union the DLL exposes.
//! - **POD structs** (`!fields.is_empty()`, no boxed types,
//!   non-recursive) -> `[StructLayout(Sequential)] public struct AzFoo`
//! - **Generic templates** are skipped (they're always monomorphized).
//! - **Recursive / VecRef / DestructorOrClone / Boxed** categories are
//!   skipped here; they are exposed through the wrapper layer instead.
//!
//! Callback typedefs become Cdecl `[UnmanagedFunctionPointer]` delegates.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef,
    TypeCategory,
};
use super::{ffi_type_name, map_type_to_csharp, sanitize_identifier};

// ============================================================================
// Top-level type emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Type definitions: enums, POD structs, tagged-union FFI structs.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    // Enums first (they may be referenced by struct fields).
    for enum_def in &ir.enums {
        if !should_include_enum(enum_def, config) {
            continue;
        }
        if enum_def.is_union {
            generate_tagged_union(builder, enum_def, ir);
        } else {
            generate_unit_enum(builder, enum_def);
        }
    }

    // POD structs.
    for struct_def in &ir.structs {
        if !should_include_struct(struct_def, config) {
            continue;
        }
        generate_struct(builder, struct_def, ir);
    }

    Ok(())
}

pub fn generate_callback_delegates(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    _config: &CodegenConfig,
) -> Result<()> {
    if ir.callback_typedefs.is_empty() {
        return Ok(());
    }

    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Callback delegate types (Cdecl P/Invoke function pointers).");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    for cb in &ir.callback_typedefs {
        generate_callback_delegate(builder, cb, ir);
    }
    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
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
        | TypeCategory::GenericTemplate => false,
        _ => true,
    }
}

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    )
}

// ============================================================================
// Unit enum
// ============================================================================

fn generate_unit_enum(builder: &mut CodeBuilder, enum_def: &EnumDef) {
    let name = ffi_type_name(&enum_def.name);

    if !enum_def.doc.is_empty() {
        for d in &enum_def.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    let underlying = enum_underlying_type(enum_def);
    builder.line(&format!("public enum {} : {}", name, underlying));
    builder.line("{");
    builder.indent();

    for (idx, variant) in enum_def.variants.iter().enumerate() {
        // For unit enums, just emit names. Sequential default values match
        // the C ABI when there is no `repr` override.
        let _ = idx;
        let v = sanitize_identifier(&variant.name);
        builder.line(&format!("{},", v));
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn enum_underlying_type(enum_def: &EnumDef) -> &'static str {
    match enum_def.repr.as_deref() {
        Some(r) if r.contains("u8") => "byte",
        Some(r) if r.contains("i8") => "sbyte",
        Some(r) if r.contains("u16") => "ushort",
        Some(r) if r.contains("i16") => "short",
        Some(r) if r.contains("i32") => "int",
        Some(r) if r.contains("u64") => "ulong",
        Some(r) if r.contains("i64") => "long",
        // Default to uint to match C's default `unsigned int` enum width.
        _ => "uint",
    }
}

// ============================================================================
// Tagged union (FFI form: tag + payload union)
// ============================================================================

fn generate_tagged_union(builder: &mut CodeBuilder, enum_def: &EnumDef, ir: &CodegenIR) {
    let name = ffi_type_name(&enum_def.name);

    // Tag enum: `AzFoo_Tag : uint`
    builder.line(&format!("public enum {}_Tag : uint", name));
    builder.line("{");
    builder.indent();
    for v in &enum_def.variants {
        builder.line(&format!("{},", sanitize_identifier(&v.name)));
    }
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Per-variant payload struct (Sequential): tag + payload field(s).
    for v in &enum_def.variants {
        let variant_struct = format!("{}Variant_{}", name, v.name);
        builder.line("[StructLayout(LayoutKind.Sequential)]");
        builder.line(&format!("public struct {}", variant_struct));
        builder.line("{");
        builder.indent();
        builder.line(&format!("public {}_Tag tag;", name));

        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let cs_type = ref_kind_field_type(ty, ref_kind, ir);
                    builder.line(&format!("public {} payload;", cs_type));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let cs_type = ref_kind_field_type(ty, ref_kind, ir);
                        builder.line(&format!("public {} payload_{};", cs_type, i));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let cs_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                    builder.line(&format!(
                        "public {} {};",
                        cs_type,
                        sanitize_identifier(&f.name)
                    ));
                }
            }
        }

        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Outer Explicit struct: every variant struct overlapped at offset 0.
    if !enum_def.doc.is_empty() {
        for d in &enum_def.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }
    builder.line("[StructLayout(LayoutKind.Explicit)]");
    builder.line(&format!("public struct {}", name));
    builder.line("{");
    builder.indent();
    for v in &enum_def.variants {
        let variant_struct = format!("{}Variant_{}", name, v.name);
        builder.line("[FieldOffset(0)]");
        builder.line(&format!(
            "public {} {};",
            variant_struct,
            sanitize_identifier(&v.name)
        ));
    }
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// POD struct
// ============================================================================

fn generate_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    builder.line("[StructLayout(LayoutKind.Sequential)]");
    builder.line(&format!("public struct {}", name));
    builder.line("{");
    builder.indent();

    if s.fields.is_empty() {
        // C# disallows empty structs in some contexts; emit a dummy
        // byte to keep ABI alignment safe and the type instantiable.
        builder.line("private byte _dummy;");
    } else {
        for f in &s.fields {
            generate_field(builder, f, ir);
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn generate_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("/// <summary>{}</summary>", xml_escape(doc)));
    }

    // Detect array types: `[T; N]` -> `fixed` is unsafe; emit IntPtr-sized
    // inline storage by expanding to multiple fields.
    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        // Emit N sequential fields. C# `[StructLayout(Sequential)]` lays
        // them out contiguously. This is the safest portable approach
        // without `unsafe` `fixed` buffers.
        let cs_elem = map_type_to_csharp(&elem_ty, ir);
        for i in 0..count {
            builder.line(&format!(
                "public {} {}_{};",
                cs_elem,
                sanitize_identifier(&f.name),
                i
            ));
        }
        return;
    }

    let cs_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!(
        "public {} {};",
        cs_type,
        sanitize_identifier(&f.name)
    ));
}

// ============================================================================
// Callback delegate
// ============================================================================

fn generate_callback_delegate(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) {
    let name = ffi_type_name(&cb.name);

    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    builder.line("[UnmanagedFunctionPointer(CallingConvention.Cdecl)]");

    let return_type = cb
        .return_type
        .as_ref()
        .map(|r| map_type_to_csharp(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            // For C# delegates we use IntPtr for & and *. Owned blittable
            // structs are passed by value.
            let cs_type = match arg.ref_kind {
                super::super::ir::ArgRefKind::Owned => map_type_to_csharp(&arg.type_name, ir),
                super::super::ir::ArgRefKind::Ref
                | super::super::ir::ArgRefKind::RefMut
                | super::super::ir::ArgRefKind::Ptr
                | super::super::ir::ArgRefKind::PtrMut => "IntPtr".to_string(),
            };
            // Arg name fallback: the IR sometimes carries empty arg names
            // (e.g., destructor callback typedefs whose api.json entry only
            // declares the type). C# requires every parameter to be named,
            // so produce `arg{i}` when the IR didn't.
            let raw_name = arg.name.trim();
            let name = if raw_name.is_empty() {
                format!("arg{}", i)
            } else {
                sanitize_identifier(raw_name)
            };
            format!("{} {}", cs_type, name)
        })
        .collect();

    builder.line(&format!(
        "public delegate {} {}({});",
        return_type,
        name,
        args.join(", ")
    ));
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the C# field type string.
fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_csharp(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "IntPtr".to_string(),
    }
}

/// Parse a Rust array type spec like `[u8; 4]` into `(elem, count)`.
fn parse_array_type(s: &str) -> Option<(String, usize)> {
    let s = s.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let semi = inner.rfind(';')?;
    let elem = inner[..semi].trim().to_string();
    let count: usize = inner[semi + 1..].trim().parse().ok()?;
    Some((elem, count))
}

/// Escape characters that are illegal inside an XML doc comment.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
