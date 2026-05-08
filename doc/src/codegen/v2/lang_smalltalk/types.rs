//! Struct, enum, and tagged-union emission for the Smalltalk generator.
//!
//! Mapping strategy (mirrors the C# generator's three-way split):
//!
//! - **Unit-only enums** -> `FFIExternalEnumeration subclass: #AzFoo`
//!   with a class-side `enumDecl` returning `#(VariantName 0 ...)`.
//!   UnifiedFFI auto-generates `AzFoo bar` accessors at image build.
//! - **Tagged-union enums** -> a `<Az>Foo_Tag` enumeration plus one
//!   `<Az>FooVariant_<Variant>` `FFIExternalStructure` per variant
//!   (each carries the tag plus the payload), plus an outer
//!   `FFIExternalUnion subclass: #AzFoo` whose `fields` overlap each
//!   variant struct at offset 0.
//! - **POD structs** -> `FFIExternalStructure subclass: #AzFoo` with a
//!   class-side `fields` method describing the layout. UFFI emits
//!   slot accessors automatically.
//!
//! Generic templates and recursive/destructor/VecRef categories are
//! deliberately skipped — they have no clean Smalltalk surface.
//!
//! Callback typedefs are emitted as plain UFFI `FFIExternalType`
//! aliases (`void*` under the hood) — Smalltalk's typical pattern is
//! to wrap a Smalltalk block via `ffiCallback:` at the call site, not
//! to declare a delegate type up-front.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef,
    TypeCategory,
};
use super::{
    ffi_type_name, map_type_to_uffi, method_category_line, sanitize_identifier, PACKAGE_TYPES,
};

// ============================================================================
// Top-level type emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("\"---------------------------------------------------------------------------");
    builder.line(" Type definitions: enumerations, POD records, tagged-union structures.");
    builder.line(" Each class is an UnifiedFFI subclass; UFFI builds slot accessors at load.");
    builder.line("---------------------------------------------------------------------------\"");
    builder.blank();

    // Enums first — POD struct fields may reference them.
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

    // Callback typedefs — emitted as `FFIExternalType` aliases.
    for cb in &ir.callback_typedefs {
        generate_callback_alias(builder, cb);
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
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
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
// Unit-only enum
// ============================================================================

fn generate_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = ffi_type_name(&e.name);

    emit_class_header(
        builder,
        &name,
        "FFIExternalEnumeration",
        &[],
        &[],
        PACKAGE_TYPES,
        &e.doc,
    );

    // Class-side enumDecl method.
    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> enumDecl [", name));
    builder.indent();
    builder.line("\"Variant -> integer mapping consumed by UnifiedFFI's");
    builder.line(" FFIExternalEnumeration to build #variantName accessors.\"");
    builder.line("^ #(");
    builder.indent();
    for (idx, variant) in e.variants.iter().enumerate() {
        builder.line(&format!("{} {}", sanitize_identifier(&variant.name), idx));
    }
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("]");
    builder.blank();
}

// ============================================================================
// Tagged union
// ============================================================================

fn generate_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let name = ffi_type_name(&e.name);
    let tag_name = format!("{}_Tag", name);

    // 1. Tag enumeration.
    emit_class_header(
        builder,
        &tag_name,
        "FFIExternalEnumeration",
        &[],
        &[],
        PACKAGE_TYPES,
        &[],
    );
    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> enumDecl [", tag_name));
    builder.indent();
    builder.line("^ #(");
    builder.indent();
    for (idx, variant) in e.variants.iter().enumerate() {
        builder.line(&format!("{} {}", sanitize_identifier(&variant.name), idx));
    }
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // 2. Per-variant payload struct (tag + payload field(s)).
    for v in &e.variants {
        let variant_struct = format!("{}Variant_{}", name, v.name);
        emit_class_header(
            builder,
            &variant_struct,
            "FFIExternalStructure",
            &[],
            &[],
            PACKAGE_TYPES,
            &[],
        );
        method_category_line(builder, "ffi");
        builder.line(&format!("{} class >> fields [", variant_struct));
        builder.indent();
        builder.line("^ #(");
        builder.indent();
        builder.line(&format!("({} tag)", tag_name));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let st_type = ref_kind_field_type(ty, ref_kind, ir);
                    builder.line(&format!("({} payload)", st_type));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let st_type = ref_kind_field_type(ty, ref_kind, ir);
                        builder.line(&format!("({} payload_{})", st_type, i));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let st_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                    builder.line(&format!(
                        "({} {})",
                        st_type,
                        sanitize_identifier(&f.name)
                    ));
                }
            }
        }
        builder.dedent();
        builder.line(")");
        builder.dedent();
        builder.line("]");
        builder.blank();
    }

    // 3. Outer union: every variant struct overlapped at offset 0.
    emit_class_header(
        builder,
        &name,
        "FFIExternalUnion",
        &[],
        &[],
        PACKAGE_TYPES,
        &e.doc,
    );
    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> fields [", name));
    builder.indent();
    builder.line("\"FFIExternalUnion overlaps every member at offset 0;");
    builder.line(" the embedded `tag` slot in each variant struct lets the");
    builder.line(" caller discriminate.\"");
    builder.line("^ #(");
    builder.indent();
    for v in &e.variants {
        let variant_struct = format!("{}Variant_{}", name, v.name);
        builder.line(&format!(
            "({} {})",
            variant_struct,
            sanitize_identifier(&v.name)
        ));
    }
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("]");
    builder.blank();
}

// ============================================================================
// POD struct
// ============================================================================

fn generate_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = ffi_type_name(&s.name);

    emit_class_header(
        builder,
        &name,
        "FFIExternalStructure",
        &[],
        &[],
        PACKAGE_TYPES,
        &s.doc,
    );

    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> fields [", name));
    builder.indent();
    builder.line("\"UnifiedFFI builds slot accessors from this layout. Each entry is");
    builder.line(" `(<C type spec> <field name>)`.\"");
    builder.line("^ #(");
    builder.indent();

    if s.fields.is_empty() {
        // Smalltalk's UFFI tolerates empty structs but most C ABIs expect
        // at least one byte; emit a placeholder so the class is loadable.
        builder.line("(uint8 _placeholder)");
    } else {
        for f in &s.fields {
            emit_field(builder, f, ir);
        }
    }

    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("]");
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    // Inline fixed-size arrays (`[u8; 4]`) — UFFI expresses these as a
    // `(<elem-type> <name>[N])` triple.
    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        let st_elem = map_type_to_uffi(&elem_ty, ir);
        builder.line(&format!(
            "({} {}[{}])",
            st_elem,
            sanitize_identifier(&f.name),
            count
        ));
        return;
    }

    let st_type = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!(
        "({} {})",
        st_type,
        sanitize_identifier(&f.name)
    ));
}

// ============================================================================
// Callback typedef alias
// ============================================================================

fn generate_callback_alias(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let name = ffi_type_name(&cb.name);
    // SKIPPED: a richer mapping would emit a Smalltalk block adapter;
    // we instead alias the typedef to a generic opaque pointer so it
    // can appear in `fields` declarations.
    builder.line(&format!(
        "\"SKIPPED: callback typedef {} - exposed as void* (use #ffiCallback: at the call site).\"",
        name
    ));
    emit_class_header(
        builder,
        &name,
        "FFIExternalType",
        &[],
        &[],
        PACKAGE_TYPES,
        &cb.doc,
    );
    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> typeSize [ ^ Smalltalk wordSize ]", name));
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Emit the canonical Tonel `Class { ... }` header for a class.
fn emit_class_header(
    builder: &mut CodeBuilder,
    name: &str,
    superclass: &str,
    inst_vars: &[&str],
    class_vars: &[&str],
    category: &str,
    doc: &[String],
) {
    if !doc.is_empty() {
        builder.line("\"");
        for d in doc {
            builder.line(&format!(" {}", d.replace('"', "''")));
        }
        builder.line("\"");
    }

    builder.line("Class {");
    builder.indent();
    builder.line(&format!("#name : '{}',", name));
    builder.line(&format!("#superclass : '{}',", superclass));

    let inst_vars_joined: String = inst_vars
        .iter()
        .map(|v| format!("'{}'", v))
        .collect::<Vec<_>>()
        .join(" ");
    builder.line(&format!("#instVars : [ {} ],", inst_vars_joined));

    let class_vars_joined: String = class_vars
        .iter()
        .map(|v| format!("'{}'", v))
        .collect::<Vec<_>>()
        .join(" ");
    builder.line(&format!("#classVars : [ {} ],", class_vars_joined));

    builder.line(&format!("#category : '{}'", category));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Map a `(type_name, FieldRefKind)` pair to the Smalltalk UFFI field
/// type spec. Reference and pointer field kinds become `<type>*`.
fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_uffi(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => format!("{}*", map_type_to_uffi(type_name, ir)),
    }
}

/// Parse `[u8; 4]` -> `("u8", 4)`.
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

/// Public re-export of [`emit_class_header`] so `wrappers.rs` and
/// `functions.rs` can reuse the Tonel `Class { ... }` shape. The
/// thin wrapper avoids exposing the privately-named helper.
pub(crate) fn class_header(
    builder: &mut CodeBuilder,
    name: &str,
    superclass: &str,
    inst_vars: &[&str],
    class_vars: &[&str],
    category: &str,
    doc: &[String],
) {
    emit_class_header(builder, name, superclass, inst_vars, class_vars, category, doc);
}
