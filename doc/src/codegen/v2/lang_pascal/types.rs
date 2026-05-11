//! Pascal record / enum / variant-record emission.
//!
//! Strategy:
//!
//! - **Forward declarations** for *every* surviving struct + enum
//!   (`PAzFoo = ^TAzFoo;`) emitted at the top of the `type` block. Pascal
//!   requires types to exist before they are referenced; a single block of
//!   forward pointer aliases means the rest of the file can refer to any
//!   type's pointer in any order.
//! - **POD structs** (`!fields.is_empty()`, non-recursive,
//!   non-generic-template) -> `TAzFoo = record ... end;` with field types
//!   resolved via `map_type_to_pascal`. The unit-level
//!   `{$PACKRECORDS C}` directive at the top of `azul.pas` ensures the
//!   layout matches the Rust C ABI exactly.
//! - **Unit-only enums** -> `TAzBar = (TAzBar_Foo, TAzBar_Bar);` Pascal
//!   sequences enum constants from 0 by default which matches Rust's
//!   default `repr(C)` enum layout.
//! - **Tagged-union enums** -> emitted as `record case Tag of ... end;`
//!   (a Pascal *variant record*). The tag enum is emitted first; each
//!   variant maps to a payload field (or empty section for unit variants).
//! - **Callback typedefs** -> `type TAzFooCallbackType = function(...): ...;
//!   cdecl;` declarations. Pascal's procedural-type support is exactly
//!   what we need here.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** are
//!   skipped with `{ SKIPPED: <reason> }` block comments.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    StructDef, TypeCategory,
};
use super::{
    map_type_to_pascal, pointer_type_name, record_type_name, sanitize_identifier,
};

// ============================================================================
// Top-level type-block emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Type definitions: forward decls, records, enums, variant records.    }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    builder.line("type");
    builder.indent();

    // 1. Forward declare every pointer alias up front so later
    //    `record` / function bodies can refer to them in any order.
    emit_forward_pointer_decls(builder, ir, config);
    builder.blank();

    // 2. Unit (simple) enums first so they may appear in record fields.
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // 3+4. Interleave tagged-union enums (variant records) and plain
    // records in topological order (`sort_order` from the IR's
    // analyze_dependencies pass). Tagged-union payloads frequently
    // reference structs (`Payload_RGB: TAzColorU`) and structs
    // sometimes embed tagged unions, so emitting one group entirely
    // before the other always leaves dangling references.
    #[derive(Debug)]
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    items.sort_by_key(|(d, _)| *d);
    for (_, item) in &items {
        match item {
            Item::Struct(s) => emit_struct(builder, s, ir),
            Item::Union(e) => emit_tagged_union(builder, e, ir),
        }
    }

    // 5. Callback (procedural) typedefs.
    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, ir);
    }

    builder.dedent();
    builder.blank();

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
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn emit_skipped_struct(builder: &mut CodeBuilder, s: &StructDef) {
    builder.line(&format!(
        "{{ SKIPPED: struct {} ({}) }}",
        s.name,
        s.category.description()
    ));
}

fn emit_skipped_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    builder.line(&format!(
        "{{ SKIPPED: enum {} ({}) }}",
        e.name,
        e.category.description()
    ));
}

// ============================================================================
// Forward pointer declarations
// ============================================================================

fn emit_forward_pointer_decls(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("{ Forward pointer declarations (every type may be referenced via P-alias). }");
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            continue;
        }
        let p = pointer_type_name(&s.name);
        let t = record_type_name(&s.name);
        builder.line(&format!("{} = ^{};", p, t));
    }
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        let p = pointer_type_name(&e.name);
        let t = record_type_name(&e.name);
        builder.line(&format!("{} = ^{};", p, t));
    }
}

// ============================================================================
// Unit-only enum
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    let t = record_type_name(&e.name);
    if e.variants.is_empty() {
        // Pascal does not allow empty enums; emit a degenerate alias.
        builder.line(&format!("{} = cuint32;", t));
        builder.blank();
        return;
    }

    let names: Vec<String> = e
        .variants
        .iter()
        .map(|v| format!("{}_{}", t, sanitize_identifier(&v.name)))
        .collect();

    builder.line(&format!("{} = (", t));
    builder.indent();
    for (i, name) in names.iter().enumerate() {
        let comma = if i + 1 < names.len() { "," } else { "" };
        builder.line(&format!("{}{}", name, comma));
    }
    builder.dedent();
    builder.line(");");
    builder.blank();
}

// ============================================================================
// Tagged-union enum (Pascal variant record)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    // Tag enum: TAzFooTag = (TAzFooTag_Variant1, TAzFooTag_Variant2, ...);
    let tag_name = format!("{}Tag", record_type_name(&e.name));
    builder.line(&format!("{} = (", tag_name));
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        let comma = if i + 1 < e.variants.len() { "," } else { "" };
        builder.line(&format!(
            "{}_{}{}",
            tag_name,
            sanitize_identifier(&v.name),
            comma
        ));
    }
    builder.dedent();
    builder.line(");");
    builder.blank();

    // Variant record: case Tag: TAzFooTag of ... end;
    let t = record_type_name(&e.name);
    builder.line(&format!("{} = record", t));
    builder.indent();
    builder.line(&format!("case Tag: {} of", tag_name));
    builder.indent();

    // Pascal variant records share one namespace across all `case`
    // branches — field names must be unique within the whole record,
    // not just within one variant arm. We disambiguate by suffixing
    // every payload field name with the variant tag.
    for v in &e.variants {
        let case_label = format!("{}_{}", tag_name, sanitize_identifier(&v.name));
        let variant_suffix = sanitize_identifier(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                // Empty payload -> `Variant: ();`
                builder.line(&format!("{}: ();", case_label));
            }
            EnumVariantKind::Tuple(types) => {
                if types.is_empty() {
                    builder.line(&format!("{}: ();", case_label));
                } else if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let pas_ty = field_type_for_ref_kind(ty, ref_kind, ir);
                    builder.line(&format!(
                        "{}: (Payload_{}: {});",
                        case_label, variant_suffix, pas_ty
                    ));
                } else {
                    let mut parts = Vec::with_capacity(types.len());
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let pas_ty = field_type_for_ref_kind(ty, ref_kind, ir);
                        parts.push(format!("Payload_{}_{}: {}", variant_suffix, i, pas_ty));
                    }
                    builder.line(&format!("{}: ({});", case_label, parts.join("; ")));
                }
            }
            EnumVariantKind::Struct(fields) => {
                if fields.is_empty() {
                    builder.line(&format!("{}: ();", case_label));
                } else {
                    let mut parts = Vec::with_capacity(fields.len());
                    for f in fields {
                        let pas_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                        let nm = sanitize_identifier(&f.name);
                        parts.push(format!("{}_{}: {}", variant_suffix, nm, pas_ty));
                    }
                    builder.line(&format!("{}: ({});", case_label, parts.join("; ")));
                }
            }
        }
    }

    builder.dedent();
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

// ============================================================================
// POD record
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    let t = record_type_name(&s.name);

    if s.fields.is_empty() {
        // FPC accepts `record end;` for an opaque/zero-sized struct.
        builder.line(&format!("{} = record", t));
        builder.indent();
        builder.line("{ opaque - no fields exposed via FFI }");
        builder.dedent();
        builder.line("end;");
        builder.blank();
        return;
    }

    builder.line(&format!("{} = record", t));
    builder.indent();

    for f in &s.fields {
        emit_field(builder, f, ir);
    }

    builder.dedent();
    builder.line("end;");
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("{{ {} }}", sanitize_comment(doc)));
    }
    let pas_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    let nm = sanitize_identifier(&f.name);
    builder.line(&format!("{}: {};", nm, pas_ty));
}

// ============================================================================
// Callback typedef (Pascal procedural type)
// ============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }
    let t = record_type_name(&cb.name);

    let args: Vec<String> = cb
        .args
        .iter()
        .map(|arg| {
            let pas_ty = match arg.ref_kind {
                ArgRefKind::Owned => map_type_to_pascal(&arg.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type_for_arg(&arg.type_name, ir),
            };
            format!("{}: {}", sanitize_identifier(&arg.name), pas_ty)
        })
        .collect();

    let header = if let Some(ret) = &cb.return_type {
        let pas_ret = map_type_to_pascal(ret, ir);
        format!(
            "{} = function({}): {}; cdecl;",
            t,
            args.join("; "),
            pas_ret
        )
    } else {
        format!("{} = procedure({}); cdecl;", t, args.join("; "))
    };
    builder.line(&header);
    builder.blank();
}

// ============================================================================
// Field/argument type helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the Pascal field type string.
pub(crate) fn field_type_for_ref_kind(
    type_name: &str,
    ref_kind: &FieldRefKind,
    ir: &CodegenIR,
) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_pascal(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => ptr_type_for_arg(type_name, ir),
    }
}

/// Pointer-form mapping: pick `PAzFoo` when the inner is a known IR type,
/// `PChar` for `char`/`c_char`/`u8`/`i8`, `Pointer` otherwise.
pub(crate) fn ptr_type_for_arg(type_name: &str, ir: &CodegenIR) -> String {
    let inner = type_name.trim();
    match inner {
        "c_char" | "char" | "i8" | "u8" => "PChar".to_string(),
        "c_void" | "void" | "()" => "Pointer".to_string(),
        _ => {
            if ir.find_struct(inner).is_some()
                || ir.find_enum(inner).is_some()
                || ir.find_type_alias(inner).is_some()
            {
                pointer_type_name(inner)
            } else {
                "Pointer".to_string()
            }
        }
    }
}

/// Sanitize a doc comment for inclusion in a Pascal `{ ... }` block.
/// We strip out closing braces (which would terminate the comment) and
/// replace newlines with spaces.
fn sanitize_comment(s: &str) -> String {
    // Pascal block comments use `{ ... }` and `(* ... *)`. Embedded
    // `{` opens a nested comment level which raises "Comment level N
    // found" warnings and ultimately swallows the rest of the file.
    // Replace both braces with parens so the comment text is harmless.
    s.replace('{', "(")
        .replace('}', ")")
        .replace('\n', " ")
        .replace('\r', " ")
}
