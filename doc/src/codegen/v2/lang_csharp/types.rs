//! Struct, enum, and callback delegate emission for the C# generator.
//!
//! Strategy:
//! - **Unit-only enums** -> `public enum Foo : uint { ... }` (unprefixed
//!   — user-facing values inside `namespace Azul`; see
//!   `user_enum_type_name`). We do not emit explicit numeric values;
//!   the C ABI uses sequential numbering from 0, which matches the C#
//!   default.
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
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, MonomorphizedVariant, StructDef, TypeAliasDef,
    TypeCategory,
};
use super::{ffi_type_name, map_type_to_csharp, sanitize_identifier, user_enum_type_name};

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

    // Generic-instantiated type aliases (monomorphized). E.g.
    // `ColumnCountValue = CssPropertyValue<ColumnCount>` becomes a
    // concrete tagged union, parallel to what the C-header generator
    // already does. Without this step ~1600 references to types like
    // `AzColumnCountValue` are unresolved.
    for ta in &ir.type_aliases {
        let Some(ref mono_def) = ta.monomorphized_def else {
            continue;
        };
        if !config.should_include_type(&ta.name) {
            continue;
        }
        generate_monomorphized_alias(builder, ta, mono_def, ir);
    }

    Ok(())
}

fn generate_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono_def: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    let name = ffi_type_name(&ta.name);
    let doc = &ta.doc;

    match &mono_def.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            // User-facing enum values — emitted unprefixed, same rule
            // as generate_unit_enum (`Azul.AzX` was a double-prefix).
            let name = user_enum_type_name(&ta.name);
            for d in doc {
                builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
            }
            builder.line(&format!("public enum {} : uint", name));
            builder.line("{");
            builder.indent();
            for v in variants {
                builder.line(&format!("{},", sanitize_identifier(v)));
            }
            builder.dedent();
            builder.line("}");
            builder.blank();
        }

        MonomorphizedKind::Struct { fields } => {
            for d in doc {
                builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
            }
            builder.line("[StructLayout(LayoutKind.Sequential)]");
            builder.line(&format!("public struct {}", name));
            builder.line("{");
            builder.indent();
            if fields.is_empty() {
                builder.line("public byte _dummy;");
            } else {
                for f in fields {
                    let cs_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                    if cs_type == "bool" {
                        builder.line("[MarshalAs(UnmanagedType.U1)]");
                    }
                    builder.line(&format!(
                        "public {} {};",
                        cs_type,
                        sanitize_identifier(&f.name)
                    ));
                }
            }
            builder.dedent();
            builder.line("}");
            builder.blank();
        }

        MonomorphizedKind::TaggedUnion { variants, .. } => {
            // Tag enum
            builder.line(&format!("public enum {}_Tag : byte", name));
            builder.line("{");
            builder.indent();
            for v in variants {
                builder.line(&format!("{},", sanitize_identifier(&v.name)));
            }
            builder.dedent();
            builder.line("}");
            builder.blank();

            // Per-variant struct
            for v in variants {
                let variant_struct = format!("{}Variant_{}", name, v.name);
                builder.line("[StructLayout(LayoutKind.Sequential)]");
                builder.line(&format!("public struct {}", variant_struct));
                builder.line("{");
                builder.indent();
                builder.line(&format!("public {}_Tag tag;", name));
                emit_monomorphized_payload_csharp(builder, v, ir);
                builder.dedent();
                builder.line("}");
                builder.blank();
            }

            // Outer Explicit struct
            for d in doc {
                builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
            }
            builder.line("[StructLayout(LayoutKind.Explicit)]");
            builder.line(&format!("public struct {}", name));
            builder.line("{");
            builder.indent();
            for v in variants {
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
    }
}

fn emit_monomorphized_payload_csharp(
    builder: &mut CodeBuilder,
    v: &MonomorphizedVariant,
    ir: &CodegenIR,
) {
    let Some(ref payload_type) = v.payload_type else {
        return;
    };
    let cs_type = ref_kind_field_type(payload_type, &v.payload_ref_kind, ir);
    builder.line(&format!("public {} payload;", cs_type));
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
    // Note: `VecRef` and `DestructorOrClone` are NOT skipped — Vec
    // wrappers reference them as fields (e.g.
    // `AzCalcAstItemVec.destructor: AzCalcAstItemVecDestructor`,
    // `AzU8VecRef`). The C-header generator includes them; we follow.
    match s.category {
        TypeCategory::Recursive | TypeCategory::GenericTemplate => false,
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
        TypeCategory::Recursive | TypeCategory::GenericTemplate
    )
}

// ============================================================================
// Unit enum
// ============================================================================

fn generate_unit_enum(builder: &mut CodeBuilder, enum_def: &EnumDef) {
    // Unit enums are user-facing values (`Update.RefreshDom`,
    // `ButtonType.Primary`) — emit unprefixed inside `namespace Azul`.
    // The C ABI passes them as plain integers, so no marshalling
    // signature depends on the type NAME. Collision-checked against
    // every other emitted type name (see `user_enum_type_name`).
    let name = user_enum_type_name(&enum_def.name);

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
    builder.line(&format!("public enum {}_Tag : byte", name));
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
                    if cs_type == "bool" {
                        builder.line("[MarshalAs(UnmanagedType.U1)]");
                    }
                    builder.line(&format!("public {} payload;", cs_type));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let cs_type = ref_kind_field_type(ty, ref_kind, ir);
                        if cs_type == "bool" {
                            builder.line("[MarshalAs(UnmanagedType.U1)]");
                        }
                        builder.line(&format!("public {} payload_{};", cs_type, i));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let cs_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                    if cs_type == "bool" {
                        builder.line("[MarshalAs(UnmanagedType.U1)]");
                    }
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

    // AzOption<T>.AsNullable() — Nullable<T> for value-type payloads,
    // direct reference for reference-type payloads (C# nullable
    // reference types: returns null when tag is None). Detected shape-
    // only — types like `MaybeFoo` whose variants are [None, Some]
    // still get the accessor.
    if enum_def.variants.len() == 2 {
        let none = enum_def.variants.iter().find(|v| v.name == "None");
        let some = enum_def.variants.iter().find(|v| v.name == "Some");
        if let (Some(_), Some(sv)) = (none, some) {
            let payload_tuple = match &sv.kind {
                EnumVariantKind::Tuple(types) if types.len() == 1 => {
                    Some((types[0].0.clone(), types[0].1.clone()))
                }
                _ => None,
            };
            if let Some((payload_ty, ref_kind)) = payload_tuple {
                let payload_cs = ref_kind_field_type(&payload_ty, &ref_kind, ir);
                // Every Az* struct or primitive in C# is a value type
                // (`struct`), so use `Nullable<T>` syntax via `T?`. For
                // primitives `int?` resolves to `Nullable<int>`; for
                // value-type structs `AzString?` resolves to
                // `Nullable<AzString>`.
                builder.line("/// <summary>");
                builder.line("/// Decode this Option as a C# nullable.");
                builder.line("/// Returns null when the C-ABI tag is None, the Some");
                builder.line("/// payload otherwise.");
                builder.line("/// </summary>");
                builder.line(&format!("public {}? AsNullable()", payload_cs));
                builder.line("{");
                builder.indent();
                builder.line(&format!(
                    "if (None.tag == {}_Tag.None) return null;",
                    name
                ));
                builder.line("return Some.payload;");
                builder.dedent();
                builder.line("}");
            }
        }
    }

    // AzResult<T, E>.Unwrap() — return Ok or throw on Err. Detected
    // shape-only — types like `IcuResult` whose variants are [Ok,
    // Err] still get the accessor regardless of name prefix.
    if enum_def.variants.len() == 2 {
        let ok = enum_def.variants.iter().find(|v| v.name == "Ok");
        let err = enum_def.variants.iter().find(|v| v.name == "Err");
        if let (Some(ov), Some(_)) = (ok, err) {
            let payload_tuple = match &ov.kind {
                EnumVariantKind::Tuple(types) if types.len() == 1 => {
                    Some((types[0].0.clone(), types[0].1.clone()))
                }
                _ => None,
            };
            if let Some((payload_ty, ref_kind)) = payload_tuple {
                let payload_cs = ref_kind_field_type(&payload_ty, &ref_kind, ir);
                builder.line("/// <summary>");
                builder.line("/// Return the Ok payload, or throw on Err.");
                builder.line("/// </summary>");
                builder.line(&format!("public {} Unwrap()", payload_cs));
                builder.line("{");
                builder.indent();
                builder.line(&format!(
                    "if (Ok.tag == {}_Tag.Ok) return Ok.payload;",
                    name
                ));
                builder.line(&format!(
                    "throw new System.InvalidOperationException(\"{} unwrap on Err: \" + Err.payload.ToString());",
                    name
                ));
                builder.dedent();
                builder.line("}");
                builder.line("/// <summary>True when the tag is Ok.</summary>");
                builder.line(&format!(
                    "public bool IsOk() => Ok.tag == {}_Tag.Ok;",
                    name
                ));
                builder.line("/// <summary>True when the tag is Err.</summary>");
                builder.line("public bool IsErr() => !IsOk();");
            }
        }
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

    // AzVec<T> → host array/list — mirror of lang_java's emit_vec_to_list_java.
    if s.category == TypeCategory::Vec {
        emit_vec_to_list_cs(builder, s, ir);
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_vec_to_list_cs(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let Some(ptr_field) = s.fields.iter().find(|f| f.name == "ptr") else {
        return;
    };
    let elem_rust = ptr_field.type_name.trim();
    let elem_cs = map_type_to_csharp(elem_rust, ir);
    // Skip exotic types (void / IntPtr collapse).
    if elem_cs == "void" || elem_cs == "System.IntPtr" || elem_cs == "IntPtr" {
        return;
    }

    let primitive_size = match elem_cs.as_str() {
        "byte" | "sbyte" => Some(1usize),
        "short" | "ushort" => Some(2),
        "int" | "uint" => Some(4),
        "long" | "ulong" => Some(8),
        "float" => Some(4),
        "double" => Some(8),
        _ => None,
    };

    builder.line("/// <summary>");
    builder.line(&format!(
        "/// Decode the wrapped {} elements into a managed array.",
        elem_rust
    ));
    builder.line("/// </summary>");
    builder.line(&format!("public {}[] ToArray()", elem_cs));
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "if (ptr == System.IntPtr.Zero || len == System.UIntPtr.Zero) return new {}[0];",
        elem_cs
    ));
    builder.line("var __n = (int)len.ToUInt64();");
    builder.line(&format!("var __out = new {}[__n];", elem_cs));
    if let Some(size) = primitive_size {
        // Marshal.Copy supports primitive arrays directly for byte/short/int/long/float/double.
        let marshal_method = match elem_cs.as_str() {
            "byte" => "Copy",
            "short" => "Copy",
            "int" => "Copy",
            "long" => "Copy",
            "float" => "Copy",
            "double" => "Copy",
            _ => "Copy",
        };
        // Marshal.Copy doesn't support unsigned types directly; cast through signed buffer for those.
        match elem_cs.as_str() {
            "byte" | "short" | "int" | "long" | "float" | "double" => {
                builder.line(&format!(
                    "System.Runtime.InteropServices.Marshal.{}(ptr, __out, 0, __n);",
                    marshal_method
                ));
            }
            "sbyte" => {
                // Marshal has no Copy(IntPtr, sbyte[], ...). Use a byte buffer and reinterpret.
                builder.line("var __buf = new byte[__n];");
                builder.line(
                    "System.Runtime.InteropServices.Marshal.Copy(ptr, __buf, 0, __n);",
                );
                builder.line("for (int __i = 0; __i < __n; __i++) __out[__i] = (sbyte)__buf[__i];");
            }
            "ushort" => {
                builder.line("var __buf = new short[__n];");
                builder.line(
                    "System.Runtime.InteropServices.Marshal.Copy(ptr, __buf, 0, __n);",
                );
                builder.line("for (int __i = 0; __i < __n; __i++) __out[__i] = (ushort)__buf[__i];");
            }
            "uint" => {
                builder.line("var __buf = new int[__n];");
                builder.line(
                    "System.Runtime.InteropServices.Marshal.Copy(ptr, __buf, 0, __n);",
                );
                builder.line("for (int __i = 0; __i < __n; __i++) __out[__i] = (uint)__buf[__i];");
            }
            "ulong" => {
                builder.line("var __buf = new long[__n];");
                builder.line(
                    "System.Runtime.InteropServices.Marshal.Copy(ptr, __buf, 0, __n);",
                );
                builder.line("for (int __i = 0; __i < __n; __i++) __out[__i] = (ulong)__buf[__i];");
            }
            _ => unreachable!(),
        }
        let _ = size;
    } else {
        // Struct element — use Marshal.PtrToStructure per element.
        builder.line(&format!(
            "int __size = System.Runtime.InteropServices.Marshal.SizeOf<{}>();",
            elem_cs
        ));
        builder.line("for (int __i = 0; __i < __n; __i++) {");
        builder.indent();
        builder.line(
            "var __ep = System.IntPtr.Add(ptr, __i * __size);",
        );
        builder.line(&format!(
            "__out[__i] = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__ep);",
            elem_cs
        ));
        builder.dedent();
        builder.line("}");
    }
    builder.line("return __out;");
    builder.dedent();
    builder.line("}");
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
        let is_bool = elem_ty.trim() == "bool";
        for i in 0..count {
            if is_bool {
                // C#'s default `bool` marshals as 4-byte Win32 BOOL; the
                // C ABI's `bool` is 1 byte (sizeof(_Bool) == 1). Force
                // 1-byte marshalling so struct layouts match.
                builder.line("[MarshalAs(UnmanagedType.U1)]");
            }
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
    if cs_type == "bool" {
        // Same rationale as the array case above.
        builder.line("[MarshalAs(UnmanagedType.U1)]");
    }
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
pub(super) fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => {
            // Callback typedefs are .NET delegate types (managed object
            // references). Embedding one as a struct field in an
            // `[StructLayout(LayoutKind.Explicit)]` union — what tagged-
            // union variants emit — fails GC-alignment validation
            // ("contains an object field at offset 0 that is incorrectly
            // aligned or overlapped by a non-object field"). The C ABI
            // representation is a raw function pointer, so use IntPtr
            // in struct fields. Delegate-typed function PARAMETERS
            // (where the marshaler can handle the conversion) still keep
            // the delegate type via the general `map_type_to_csharp`
            // mapping.
            if ir.callback_typedefs.iter().any(|c| c.name == type_name.trim()) {
                "IntPtr".to_string()
            } else {
                map_type_to_csharp(type_name, ir)
            }
        }
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
