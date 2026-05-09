//! Struct, enum and callback emission for the Java JNA generator.
//!
//! Strategy:
//! - **Unit-only enums** -> `public enum Az<Name> { ... }` plus a
//!   per-variant `int VALUE` for the JNA wire format (JNA passes
//!   enums as plain `int`).
//! - **Tagged-union enums** (`is_union == true`) -> three pieces:
//!     1. `Az<Name>_Tag` — Java enum giving every variant a stable
//!        ordinal that matches the C-ABI tag.
//!     2. `Az<Name>Variant<Variant>` — a per-variant `Structure`
//!        (Sequential layout: tag + payload field(s)).
//!     3. `Az<Name>` — a top-level `Structure` whose only field is a
//!        JNA `Union` of the variant payload types. The `Union` class
//!        inside JNA dispatches reads/writes via `setType` /
//!        `getTypedValue`. We expose plain public fields and trust
//!        the user to call `setType` themselves.
//! - **POD structs** -> `Structure` subclass with public fields, a
//!   `getFieldOrder()` override, and `ByValue` / `ByReference`
//!   inner classes.
//! - **Callback typedefs** -> a JNA `Callback` interface with a
//!   `callback(...)` method whose signature mirrors the C function
//!   pointer.
//! - **Generic templates / Recursive / VecRef / DestructorOrClone**
//!   are skipped here; they're exposed (or omitted) by the wrapper
//!   layer.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, MonomorphizedVariant, StructDef, TypeAliasDef,
    TypeCategory,
};
use super::{emit_file, ffi_type_name, map_jvm_type, sanitize_identifier};

// ============================================================================
// Top-level driver
// ============================================================================

/// Append all type-related source files to `out`. Each top-level Java
/// public class becomes its own `// ==FILE: Foo.java ==` chunk.
pub fn emit_all_type_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    // Enums
    for enum_def in &ir.enums {
        if !should_include_enum(enum_def, config) {
            continue;
        }
        if enum_def.is_union {
            emit_tagged_union_files(out, enum_def, ir, config)?;
        } else {
            let name = ffi_type_name(&enum_def.name);
            let chunk = emit_file(
                &format!("{}.java", name),
                |b| {
                    emit_unit_enum(b, enum_def);
                    Ok(())
                },
                config,
            )?;
            out.push_str(&chunk);
        }
    }

    // POD Structs
    for struct_def in &ir.structs {
        if !should_include_struct(struct_def, config) {
            continue;
        }
        let name = ffi_type_name(&struct_def.name);
        let chunk = emit_file(
            &format!("{}.java", name),
            |b| {
                emit_struct(b, struct_def, ir);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    // Callback typedefs
    for cb in &ir.callback_typedefs {
        let name = ffi_type_name(&cb.name);
        let chunk = emit_file(
            &format!("{}.java", name),
            |b| {
                emit_callback_interface(b, cb, ir);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    // Generic-instantiated type aliases (monomorphized). E.g.
    // `ColumnCountValue = CssPropertyValue<ColumnCount>` becomes a
    // concrete tagged union with the variants of CssPropertyValue
    // expanded into a `ColumnCount` payload. The IR builder has
    // already monomorphized; we just emit the resulting concrete type.
    // Without this step, ~200-1600 references like `AzColumnCountValue`
    // / `AzU8VecRef` show up unresolved when compiling the bindings.
    for ta in &ir.type_aliases {
        let Some(ref mono_def) = ta.monomorphized_def else {
            continue;
        };
        if !config.should_include_type(&ta.name) {
            continue;
        }
        emit_monomorphized_alias_files(out, ta, mono_def, ir, config)?;
    }

    Ok(())
}

// ============================================================================
// Monomorphized type alias — multi-file emit
// ============================================================================

fn emit_monomorphized_alias_files(
    out: &mut String,
    ta: &TypeAliasDef,
    mono_def: &MonomorphizedTypeDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let name = ffi_type_name(&ta.name);
    let doc = &ta.doc;

    match &mono_def.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            let chunk = emit_file(
                &format!("{}.java", name),
                |b| {
                    if !doc.is_empty() {
                        b.line("/**");
                        for d in doc {
                            b.line(&format!(" * {}", javadoc_escape(d)));
                        }
                        b.line(" */");
                    }
                    b.line(&format!("public enum {} {{", name));
                    b.indent();
                    let last = variants.len().saturating_sub(1);
                    for (idx, v) in variants.iter().enumerate() {
                        let v_name = sanitize_identifier(v);
                        let sep = if idx == last { ";" } else { "," };
                        b.line(&format!("{}({}){}", v_name, idx, sep));
                    }
                    b.line("public final int value;");
                    b.line(&format!("{}(int v) {{ this.value = v; }}", name));
                    b.line(&format!("public static {} fromInt(int v) {{", name));
                    b.indent();
                    b.line(&format!("for ({} t : values()) if (t.value == v) return t;", name));
                    b.line(&format!(
                        "throw new IllegalArgumentException(\"Unknown {} ordinal: \" + v);",
                        name
                    ));
                    b.dedent();
                    b.line("}");
                    b.dedent();
                    b.line("}");
                    Ok(())
                },
                config,
            )?;
            out.push_str(&chunk);
        }

        MonomorphizedKind::Struct { fields } => {
            let chunk = emit_file(
                &format!("{}.java", name),
                |b| {
                    if !doc.is_empty() {
                        b.line("/**");
                        for d in doc {
                            b.line(&format!(" * {}", javadoc_escape(d)));
                        }
                        b.line(" */");
                    }
                    b.line(&format!("public class {} extends Structure {{", name));
                    b.indent();
                    let mut field_names: Vec<String> = Vec::new();
                    if fields.is_empty() {
                        b.line("public byte _dummy;");
                        field_names.push("\"_dummy\"".to_string());
                    } else {
                        for f in fields {
                            emit_field(b, f, ir, &mut field_names);
                        }
                    }
                    emit_field_order_override(b, &field_names);
                    emit_byvalue_byref(b, &name);
                    b.dedent();
                    b.line("}");
                    Ok(())
                },
                config,
            )?;
            out.push_str(&chunk);
        }

        MonomorphizedKind::TaggedUnion { variants, .. } => {
            // 1. <Name>_Tag.java
            let tag_file = emit_file(
                &format!("{}_Tag.java", name),
                |b| {
                    b.line(&format!("/** Discriminator tag for {}. */", name));
                    b.line(&format!("public enum {}_Tag {{", name));
                    b.indent();
                    let last = variants.len().saturating_sub(1);
                    for (idx, v) in variants.iter().enumerate() {
                        let sep = if idx == last { ";" } else { "," };
                        b.line(&format!(
                            "{}({}){}",
                            sanitize_identifier(&v.name),
                            idx,
                            sep
                        ));
                    }
                    b.line("public final int value;");
                    b.line(&format!("{}_Tag(int v) {{ this.value = v; }}", name));
                    b.dedent();
                    b.line("}");
                    Ok(())
                },
                config,
            )?;
            out.push_str(&tag_file);

            // 2. <Name>Variant_<V>.java per variant
            for v in variants {
                let variant_struct = format!("{}Variant_{}", name, v.name);
                let chunk = emit_file(
                    &format!("{}.java", variant_struct),
                    |b| {
                        b.line(&format!(
                            "/** Per-variant payload struct for {}.{}. */",
                            ta.name, v.name
                        ));
                        b.line(&format!(
                            "public class {} extends Structure {{",
                            variant_struct
                        ));
                        b.indent();
                        b.line(&format!("public int tag; // {}_Tag.{}", name, v.name));
                        let mut field_names: Vec<String> = vec!["\"tag\"".to_string()];
                        emit_monomorphized_payload(b, v, ir, &mut field_names);
                        emit_field_order_override(b, &field_names);
                        emit_byvalue_byref(b, &variant_struct);
                        b.dedent();
                        b.line("}");
                        Ok(())
                    },
                    config,
                )?;
                out.push_str(&chunk);
            }

            // 3. Outer Az<Name> Union file.
            let outer = emit_file(
                &format!("{}.java", name),
                |b| {
                    if !doc.is_empty() {
                        b.line("/**");
                        for d in doc {
                            b.line(&format!(" * {}", javadoc_escape(d)));
                        }
                        b.line(" */");
                    }
                    b.line(&format!("public class {} extends Union {{", name));
                    b.indent();
                    let mut field_names: Vec<String> = Vec::new();
                    for v in variants {
                        let variant_struct = format!("{}Variant_{}", name, v.name);
                        let field = sanitize_identifier(&v.name);
                        b.line(&format!("public {} {};", variant_struct, field));
                        field_names.push(format!("\"{}\"", v.name));
                    }
                    emit_field_order_override(b, &field_names);
                    b.line(&format!(
                        "public static class ByValue extends {} implements Structure.ByValue {{}}",
                        name
                    ));
                    b.line(&format!(
                        "public static class ByReference extends {} implements Structure.ByReference {{}}",
                        name
                    ));
                    b.dedent();
                    b.line("}");
                    Ok(())
                },
                config,
            )?;
            out.push_str(&outer);
        }
    }

    Ok(())
}

fn emit_monomorphized_payload(
    builder: &mut CodeBuilder,
    v: &MonomorphizedVariant,
    ir: &CodegenIR,
    field_names: &mut Vec<String>,
) {
    let Some(ref payload_type) = v.payload_type else {
        return;
    };
    let jt = ref_kind_field_type(payload_type, &v.payload_ref_kind, ir);
    builder.line(&format!("public {} payload;", jt));
    field_names.push("\"payload\"".to_string());
}

// ============================================================================
// Filters
// ============================================================================

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    // Note: `VecRef` and `DestructorOrClone` are NOT skipped — the C
    // header emits them and our Vec wrappers reference them as fields
    // (e.g. `AzU8VecRef`, `AzCalcAstItemVecDestructor`).
    !matches!(
        s.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate
    )
}

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    // Note: `DestructorOrClone` enums are NOT skipped — Vec wrappers carry
    // them as fields (e.g. `AzCalcAstItemVec.destructor: AzCalcAstItemVecDestructor`),
    // so they must be emitted as Java types. The C generator includes them
    // for the same reason.
    !matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate
    )
}

// ============================================================================
// Unit enum
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, enum_def: &EnumDef) {
    let name = ffi_type_name(&enum_def.name);

    if !enum_def.doc.is_empty() {
        builder.line("/**");
        for d in &enum_def.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!("public enum {} {{", name));
    builder.indent();

    let last = enum_def.variants.len().saturating_sub(1);
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        let v = sanitize_identifier(&variant.name);
        let sep = if idx == last { ";" } else { "," };
        builder.line(&format!("{}({}){}", v, idx, sep));
    }

    // Backing int + reverse lookup so callers can convert to/from the
    // C-ABI ordinal directly.
    builder.line("public final int value;");
    builder.line(&format!("{}(int v) {{ this.value = v; }}", name));
    builder.line(&format!("public static {} fromInt(int v) {{", name));
    builder.indent();
    builder.line(&format!("for ({} t : values()) if (t.value == v) return t;", name));
    builder.line(&format!(
        "throw new IllegalArgumentException(\"Unknown {} ordinal: \" + v);",
        name
    ));
    builder.dedent();
    builder.line("}");

    builder.dedent();
    builder.line("}");
}

// ============================================================================
// Tagged union — emits multiple files
// ============================================================================

fn emit_tagged_union_files(
    out: &mut String,
    enum_def: &EnumDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let name = ffi_type_name(&enum_def.name);

    // 1. AzFoo_Tag enum file
    let tag_file = emit_file(
        &format!("{}_Tag.java", name),
        |b| {
            b.line(&format!("/** Discriminator tag for {}. */", name));
            b.line(&format!("public enum {}_Tag {{", name));
            b.indent();
            let last = enum_def.variants.len().saturating_sub(1);
            for (idx, v) in enum_def.variants.iter().enumerate() {
                let sep = if idx == last { ";" } else { "," };
                b.line(&format!("{}({}){}", sanitize_identifier(&v.name), idx, sep));
            }
            b.line("public final int value;");
            b.line(&format!("{}_Tag(int v) {{ this.value = v; }}", name));
            b.dedent();
            b.line("}");
            Ok(())
        },
        config,
    )?;
    out.push_str(&tag_file);

    // 2. Per-variant payload Structure files
    //    AzFooVariant_Bar { tag; payload; }
    for v in &enum_def.variants {
        let variant_struct = format!("{}Variant_{}", name, v.name);
        let chunk = emit_file(
            &format!("{}.java", variant_struct),
            |b| {
                b.line(&format!(
                    "/** Per-variant payload struct for {}.{}. */",
                    enum_def.name, v.name
                ));
                b.line(&format!(
                    "public class {} extends Structure {{",
                    variant_struct
                ));
                b.indent();
                b.line(&format!("public int tag; // {}_Tag.{}", name, v.name));

                let mut field_names: Vec<String> = vec!["\"tag\"".to_string()];
                match &v.kind {
                    EnumVariantKind::Unit => {}
                    EnumVariantKind::Tuple(types) => {
                        if types.len() == 1 {
                            let (ty, ref_kind) = &types[0];
                            let jt = ref_kind_field_type(ty, ref_kind, ir);
                            b.line(&format!("public {} payload;", jt));
                            field_names.push("\"payload\"".to_string());
                        } else {
                            for (i, (ty, ref_kind)) in types.iter().enumerate() {
                                let jt = ref_kind_field_type(ty, ref_kind, ir);
                                b.line(&format!("public {} payload_{};", jt, i));
                                field_names.push(format!("\"payload_{}\"", i));
                            }
                        }
                    }
                    EnumVariantKind::Struct(fields) => {
                        for f in fields {
                            let jt = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                            let fname = sanitize_identifier(&f.name);
                            b.line(&format!("public {} {};", jt, fname));
                            field_names.push(format!("\"{}\"", f.name));
                        }
                    }
                }

                emit_field_order_override(b, &field_names);
                emit_byvalue_byref(b, &variant_struct);
                b.dedent();
                b.line("}");
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    // 3. Outer Az<Name> file: contains a JNA Union sized to fit any
    //    variant payload. We pick the variant by setType().
    let outer = emit_file(
        &format!("{}.java", name),
        |b| {
            if !enum_def.doc.is_empty() {
                b.line("/**");
                for d in &enum_def.doc {
                    b.line(&format!(" * {}", javadoc_escape(d)));
                }
                b.line(" */");
            }
            b.line(&format!("public class {} extends Union {{", name));
            b.indent();
            let mut field_names: Vec<String> = Vec::new();
            for v in &enum_def.variants {
                let variant_struct = format!("{}Variant_{}", name, v.name);
                let field = sanitize_identifier(&v.name);
                b.line(&format!("public {} {};", variant_struct, field));
                field_names.push(format!("\"{}\"", v.name));
            }
            // JNA Union exposes its writer-field names via the
            // implicit Structure machinery; getFieldOrder() is still
            // recommended.
            emit_field_order_override(b, &field_names);
            // ByValue / ByReference variants for passing the union by
            // value across the FFI boundary.
            b.line(&format!(
                "public static class ByValue extends {} implements Structure.ByValue {{}}",
                name
            ));
            b.line(&format!(
                "public static class ByReference extends {} implements Structure.ByReference {{}}",
                name
            ));
            b.dedent();
            b.line("}");
            Ok(())
        },
        config,
    )?;
    out.push_str(&outer);

    Ok(())
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        builder.line("/**");
        for d in &s.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!("public class {} extends Structure {{", name));
    builder.indent();

    let mut field_names: Vec<String> = Vec::new();

    if s.fields.is_empty() {
        // JNA's Structure requires at least one field. Add a single
        // padding byte so the ABI stays well-defined.
        builder.line("public byte _dummy;");
        field_names.push("\"_dummy\"".to_string());
    } else {
        for f in &s.fields {
            emit_field(builder, f, ir, &mut field_names);
        }
    }

    emit_field_order_override(builder, &field_names);
    emit_byvalue_byref(builder, &name);

    builder.dedent();
    builder.line("}");
}

fn emit_field(
    builder: &mut CodeBuilder,
    f: &FieldDef,
    ir: &CodegenIR,
    field_names: &mut Vec<String>,
) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("/** {} */", javadoc_escape(doc)));
    }

    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        let elem = map_jvm_type(&elem_ty, ir);
        // Java arrays in JNA Structures need an explicit fixed length:
        // declare and pre-allocate so JNA reads N elements.
        builder.line(&format!(
            "public {}[] {} = new {}[{}];",
            elem,
            sanitize_identifier(&f.name),
            elem,
            count
        ));
        field_names.push(format!("\"{}\"", f.name));
        return;
    }

    let jt = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("public {} {};", jt, sanitize_identifier(&f.name)));
    field_names.push(format!("\"{}\"", f.name));
}

// ============================================================================
// Callback typedef
// ============================================================================

fn emit_callback_interface(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) {
    let name = ffi_type_name(&cb.name);

    if !cb.doc.is_empty() {
        builder.line("/**");
        for d in &cb.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!(
        "public interface {} extends com.sun.jna.Callback {{",
        name
    ));
    builder.indent();

    let return_type = cb
        .return_type
        .as_ref()
        .map(|r| super::map_jvm_type_byvalue(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let jt = match arg.ref_kind {
                ArgRefKind::Owned => super::map_jvm_type_byvalue(&arg.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "Pointer".to_string(),
            };
            // Arg-name fallback: the IR sometimes carries empty arg names
            // (e.g., destructor-shaped callback typedefs whose api.json
            // entry only declares the type). Java requires every parameter
            // to be named; produce `arg{i}` when the IR didn't.
            let raw_name = arg.name.trim();
            let name = if raw_name.is_empty() {
                format!("arg{}", i)
            } else {
                sanitize_identifier(raw_name)
            };
            format!("{} {}", jt, name)
        })
        .collect();

    builder.line(&format!(
        "{} callback({});",
        return_type,
        args.join(", ")
    ));

    builder.dedent();
    builder.line("}");
}

// ============================================================================
// Helpers shared with wrappers.rs
// ============================================================================

pub(crate) fn emit_field_order_override(builder: &mut CodeBuilder, field_names: &[String]) {
    builder.blank();
    builder.line("@Override");
    // Use the fully-qualified `java.lang.String` so that this method's
    // return type matches JNA's `Structure.getFieldOrder()` exactly,
    // even when our package contains a wrapper class named `String`
    // that would otherwise shadow `java.lang.String` (resolving to
    // `List<com.azul.String>` and breaking the override).
    builder.line("protected java.util.List<java.lang.String> getFieldOrder() {");
    builder.indent();
    builder.line(&format!("return Arrays.asList({});", field_names.join(", ")));
    builder.dedent();
    builder.line("}");
}

pub(crate) fn emit_byvalue_byref(builder: &mut CodeBuilder, type_name: &str) {
    builder.blank();
    builder.line(&format!(
        "public static class ByValue extends {} implements Structure.ByValue {{}}",
        type_name
    ));
    builder.line(&format!(
        "public static class ByReference extends {} implements Structure.ByReference {{}}",
        type_name
    ));
}

/// Map a `(type_name, FieldRefKind)` pair to the Java field type.
fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_jvm_type(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "Pointer".to_string(),
    }
}

/// Parse `[T; N]` → `(T, N)`.
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

fn javadoc_escape(s: &str) -> String {
    s.replace("*/", "*&#47;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('&', "&amp;")
}
