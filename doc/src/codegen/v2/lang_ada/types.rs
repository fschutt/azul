//! Ada type emission: enums, POD records, and tagged-union variant
//! records — everything that lands inside `package Azul is`.
//!
//! Strategy:
//! - **Forward declarations**: For every IR struct/enum we emit
//!   `type Az_Foo;` plus `pragma Convention (C, Az_Foo);` so that access
//!   types (`type Az_Foo_Access is access all Az_Foo;`) and mutually
//!   recursive records work.
//! - **Unit enums** (`is_union == false`) become Ada enumerated types with
//!   an explicit representation clause that pins the underlying numeric
//!   value. We use `pragma Convention (C, ...)` so they round-trip with
//!   the C ABI.
//! - **Tagged-union enums** (`is_union == true`) become a tag enum plus a
//!   variant record with a discriminant on the tag. Variants carrying
//!   payloads expand as `when <Variant> => <field> : <type>;`. Empty
//!   variants render as `when <Variant> => null;`.
//! - **POD structs** become plain `record` types with
//!   `pragma Convention (C, ...)`.
//! - **Recursive / VecRef / DestructorOrClone / GenericTemplate** types
//!   are skipped, leaving an `-- SKIPPED:` comment for traceability.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef, TypeCategory,
};
use super::{ada_ffi_type_name, map_type_to_ada, sanitize_identifier};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_forward_declarations(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Forward declarations (incomplete types + access types).");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        let name = ada_ffi_type_name(&s.name);
        builder.line(&format!("type {};", name));
        builder.line(&format!("type {}_Access is access all {};", name, name));
        builder.line(&format!("pragma Convention (C, {}_Access);", name));
    }
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        // Unit enums don't strictly need an incomplete-type forward decl
        // (they're not records), but unions do; emit access type for
        // unions (so nested fields can reference them via System.Address
        // anyway, but a typed access aids debugging tools).
        if e.is_union {
            let name = ada_ffi_type_name(&e.name);
            builder.line(&format!("type {};", name));
            builder.line(&format!("type {}_Access is access all {};", name, name));
            builder.line(&format!("pragma Convention (C, {}_Access);", name));
        }
    }
    builder.blank();
}

pub fn emit_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Type definitions: unit enums, tagged-union variant records, records.");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    // Enums first (they may be referenced by struct fields).
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            if !e.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic enum {} cannot be emitted (Ada has no generics over C ABI here)",
                    e.name
                ));
            } else {
                builder.line(&format!(
                    "-- SKIPPED: enum {} ({})",
                    e.name,
                    e.category.description()
                ));
            }
            continue;
        }
        if e.is_union {
            emit_tagged_union(builder, e, ir);
        } else {
            emit_unit_enum(builder, e);
        }
    }

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic struct {} cannot be emitted (no Ada equivalent over C ABI)",
                    s.name
                ));
            } else {
                builder.line(&format!(
                    "-- SKIPPED: struct {} ({})",
                    s.name,
                    s.category.description()
                ));
            }
            continue;
        }
        emit_record(builder, s, ir);
    }

    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

pub fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
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

pub fn should_emit_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
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

fn emit_unit_enum(builder: &mut CodeBuilder, enum_def: &EnumDef) {
    let name = ada_ffi_type_name(&enum_def.name);

    if !enum_def.doc.is_empty() {
        for d in &enum_def.doc {
            builder.line(&format!("-- {}", sanitize_doc(d)));
        }
    }

    if enum_def.variants.is_empty() {
        builder.line(&format!(
            "-- SKIPPED: enum {} has no variants (Ada enums require >= 1 literal)",
            enum_def.name
        ));
        builder.blank();
        return;
    }

    // type Foo is (V0, V1, V2);
    let mut literals: Vec<String> = Vec::with_capacity(enum_def.variants.len());
    for v in &enum_def.variants {
        let lit = sanitize_identifier(&v.name);
        literals.push(lit);
    }
    builder.line(&format!("type {} is", name));
    let joined = literals.join(", ");
    builder.line(&format!("   ({});", joined));

    // Explicit representation clause: pin sequential numbering 0..N-1.
    builder.line(&format!("for {} use", name));
    builder.line("   (");
    for (idx, v) in enum_def.variants.iter().enumerate() {
        let lit = sanitize_identifier(&v.name);
        let sep = if idx + 1 == enum_def.variants.len() {
            ""
        } else {
            ","
        };
        builder.line(&format!("    {} => {}{}", lit, idx, sep));
    }
    builder.line("   );");
    builder.line(&format!("pragma Convention (C, {});", name));
    builder.blank();
}

// ============================================================================
// Tagged union (variant record with discriminant)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, enum_def: &EnumDef, ir: &CodegenIR) {
    let name = ada_ffi_type_name(&enum_def.name);
    let tag_name = format!("{}_Tag", name);

    if enum_def.variants.is_empty() {
        builder.line(&format!(
            "-- SKIPPED: tagged-union enum {} has no variants",
            enum_def.name
        ));
        builder.blank();
        return;
    }

    // Tag enum.
    builder.line(&format!("type {} is", tag_name));
    let lits: Vec<String> = enum_def
        .variants
        .iter()
        .map(|v| sanitize_identifier(&v.name))
        .collect();
    builder.line(&format!("   ({});", lits.join(", ")));
    builder.line(&format!("for {} use", tag_name));
    builder.line("   (");
    for (idx, v) in enum_def.variants.iter().enumerate() {
        let lit = sanitize_identifier(&v.name);
        let sep = if idx + 1 == enum_def.variants.len() {
            ""
        } else {
            ","
        };
        builder.line(&format!("    {} => {}{}", lit, idx, sep));
    }
    builder.line("   );");
    builder.line(&format!("pragma Convention (C, {});", tag_name));
    builder.blank();

    // Variant record with discriminant.
    if !enum_def.doc.is_empty() {
        for d in &enum_def.doc {
            builder.line(&format!("-- {}", sanitize_doc(d)));
        }
    }
    let default_variant = lits.first().cloned().unwrap_or_else(|| "V0".to_string());
    builder.line(&format!(
        "type {} (Tag : {} := {}) is record",
        name, tag_name, default_variant
    ));
    builder.line("   case Tag is");

    for v in &enum_def.variants {
        let lit = sanitize_identifier(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("      when {} =>", lit));
                builder.line("         null;");
            }
            EnumVariantKind::Tuple(types) => {
                builder.line(&format!("      when {} =>", lit));
                if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let ada_ty = ref_kind_field_type(ty, ref_kind, ir);
                    builder.line(&format!("         Payload : {};", ada_ty));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let ada_ty = ref_kind_field_type(ty, ref_kind, ir);
                        builder.line(&format!("         Payload_{} : {};", i, ada_ty));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                builder.line(&format!("      when {} =>", lit));
                if fields.is_empty() {
                    builder.line("         null;");
                } else {
                    for f in fields {
                        let ada_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                        let name = sanitize_identifier(&pascalize_field_name(&f.name));
                        builder.line(&format!("         {} : {};", name, ada_ty));
                    }
                }
            }
        }
    }

    builder.line("   end case;");
    builder.line("end record;");
    builder.line(&format!("pragma Convention (C, {});", name));
    builder.blank();
}

// ============================================================================
// POD record
// ============================================================================

fn emit_record(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = ada_ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- {}", sanitize_doc(d)));
        }
    }

    if s.fields.is_empty() {
        // Ada disallows empty records; emit a placeholder field.
        builder.line(&format!("type {} is record", name));
        builder.line("   Reserved : Interfaces.C.unsigned_char;");
        builder.line("end record;");
        builder.line(&format!("pragma Convention (C, {});", name));
        builder.blank();
        return;
    }

    builder.line(&format!("type {} is record", name));
    for f in &s.fields {
        emit_field(builder, f, ir);
    }
    builder.line("end record;");
    builder.line(&format!("pragma Convention (C, {});", name));
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("   -- {}", sanitize_doc(doc)));
    }

    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        // Ada array typed inline as `array (0 .. N-1) of T`. Anonymous
        // array types in records require `array (...) of T`.
        let ada_elem = map_type_to_ada(&elem_ty, ir);
        let field_name = sanitize_identifier(&pascalize_field_name(&f.name));
        let last = count.saturating_sub(1);
        builder.line(&format!(
            "   {} : array (0 .. {}) of {};",
            field_name, last, ada_elem
        ));
        return;
    }

    let ada_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    let field_name = sanitize_identifier(&pascalize_field_name(&f.name));
    builder.line(&format!("   {} : {};", field_name, ada_ty));
}

// ============================================================================
// Helpers
// ============================================================================

fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_ada(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "System.Address".to_string(),
    }
}

/// Parse `[T; N]` into `(T, N)`.
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

/// Convert an api.json field name (`snake_case` or `camelCase`) into
/// `Pascal_Snake_Case`. Ada style prefers `Each_Word_Capitalized`.
fn pascalize_field_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut chars = name.chars().peekable();
    let mut upper_next = true;
    let mut prev_lower = false;
    while let Some(c) = chars.next() {
        if c == '_' {
            out.push('_');
            upper_next = true;
            prev_lower = false;
            continue;
        }
        if prev_lower && c.is_ascii_uppercase() {
            out.push('_');
            upper_next = true;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.extend(c.to_ascii_lowercase().to_string().chars());
        }
        prev_lower = c.is_ascii_lowercase();
    }
    out
}

/// Sanitize a doc-comment line so a stray `--` mid-string doesn't break
/// out of the surrounding line comment. (Ada doesn't have block comments
/// but `--` already starts a comment, so we just collapse problematic
/// runs.)
fn sanitize_doc(s: &str) -> String {
    s.replace('\n', " ").trim().to_string()
}
