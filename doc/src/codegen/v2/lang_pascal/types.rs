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
    MonomorphizedKind, MonomorphizedTypeDef, MonomorphizedVariant, StructDef, TypeAliasDef,
    TypeCategory,
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
    // Interleave structs, tagged-union enums, AND monomorphized
    // type-alias instantiations (PhysicalSizeU32, OptionU32, ...) in
    // topological order. Monomorphized aliases are emitted as concrete
    // records / variant records and frequently appear as struct fields,
    // so they must land at the right sort_order rather than at the end.
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
        Mono(&'a TypeAliasDef, &'a MonomorphizedTypeDef),
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
    for ta in &ir.type_aliases {
        let Some(ref mono) = ta.monomorphized_def else {
            continue;
        };
        if !config.should_include_type(&ta.name) {
            continue;
        }
        items.push((ta.sort_order, Item::Mono(ta, mono)));
    }
    items.sort_by_key(|(d, _)| *d);
    for (_, item) in &items {
        match item {
            Item::Struct(s) => emit_struct(builder, s, ir),
            Item::Union(e) => emit_tagged_union(builder, e, ir),
            Item::Mono(ta, mono) => emit_monomorphized_alias(builder, ta, mono, ir),
        }
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
    // VecRef and DestructorOrClone are NOT skipped — Vec wrappers
    // reference them as fields (e.g. `AzU8Vec.destructor:
    // AzU8VecDestructor`), and skipping them as `Pointer` aliases
    // shrinks the surrounding struct by 8 bytes per occurrence,
    // corrupting every subsequent field's offset.
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
    !matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate
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
    // Included structs/enums: P-alias to the record/variant-record type.
    // Skipped categories (Recursive / VecRef / DestructorOrClone /
    // GenericTemplate / callback typedef): emit an opaque T = pointer
    // alias so field declarations referencing them still resolve.
    // Without the second pass, struct fields whose type is a callback
    // typedef or a destructor function pointer leave dangling
    // identifiers like `TAzU8VecDestructor`.
    for s in &ir.structs {
        let p = pointer_type_name(&s.name);
        let t = record_type_name(&s.name);
        if !should_include_struct(s, config) {
            // Opaque T-alias so the type name resolves; users get a
            // Pointer because we don't have a layout to map. Also
            // emit a P-alias so struct fields like
            // `ptr_: PAzXmlNodeChild` resolve to a pointer type.
            builder.line(&format!("{} = Pointer;", t));
            builder.line(&format!("{} = {};", p, t));
            continue;
        }
        builder.line(&format!("{} = ^{};", p, t));
    }
    for e in &ir.enums {
        let p = pointer_type_name(&e.name);
        let t = record_type_name(&e.name);
        if !should_include_enum(e, config) {
            builder.line(&format!("{} = Pointer;", t));
            builder.line(&format!("{} = {};", p, t));
            continue;
        }
        builder.line(&format!("{} = ^{};", p, t));
    }
    // Monomorphized type aliases (PhysicalSizeU32 etc.) also need a
    // P-alias so wrapper PROC declarations referring to them by pointer
    // resolve.
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        let p = pointer_type_name(&ta.name);
        let t = record_type_name(&ta.name);
        if ta.monomorphized_def.is_some() {
            // Will be emitted as a record/variant-record below; the
            // P-alias targets that real type.
            builder.line(&format!("{} = ^{};", p, t));
        } else {
            // Simple alias to a primitive or another existing type.
            // Emit BOTH the T-alias (forwarded to the target type) and
            // its P-alias. Without the T-alias, field declarations
            // mentioning the alias by name (TAzScanCode) dangle.
            let target_ty = map_type_to_pascal(&ta.target, ir);
            builder.line(&format!("{} = {};", t, target_ty));
            builder.line(&format!("{} = ^{};", p, t));
        }
    }
    // Callback typedefs: opaque T-alias = Pointer. Pascal procedural
    // types in this codegen would force a strict forward-ordering with
    // every struct they reference, so we forward-decl them as Pointer
    // and skip the procedural-type emission entirely. Callers cast to
    // a typed function pointer at the call site if they need to invoke
    // one — for field storage Pointer is sufficient.
    for cb in &ir.callback_typedefs {
        let t = record_type_name(&cb.name);
        builder.line(&format!("{} = Pointer;", t));
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

    // Rust's tagged unions are `#[repr(C, u8)]`. Pascal enums under
    // `{$PACKRECORDS C}` default to 4 bytes, which would offset every
    // variant payload by 3 bytes and corrupt every struct that embeds
    // a tagged union. Emit a 1-byte `cuint8` tag with integer case
    // labels and a per-label `{ <Variant> }` block comment for
    // readability — same shape lang_java / lang_csharp / lang_ocaml
    // settled on. (We can't emit named `const` tag values here because
    // a `const` block would close the surrounding `type` block and
    // FPC requires all forward-typed identifiers to resolve before
    // the type block ends.)
    let tag_name = format!("{}_TAG", record_type_name(&e.name));

    // Variant record: case Tag: cuint8 of ... end;
    let t = record_type_name(&e.name);
    builder.line(&format!("{} = record", t));
    builder.indent();
    builder.line(&format!("case Tag: cuint8 of {{ values of {} }}", tag_name));
    builder.indent();

    // Pascal variant records share one namespace across all `case`
    // branches — field names must be unique within the whole record,
    // not just within one variant arm. We disambiguate by suffixing
    // every payload field name with the variant tag. Case labels are
    // integer literals (matching the cuint8 selector); the original
    // variant name is kept in a `{ ... }` block comment.
    for (i, v) in e.variants.iter().enumerate() {
        let case_label = format!("{} {{ {}_{} }}", i, tag_name, sanitize_identifier(&v.name));
        let variant_suffix = sanitize_identifier(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                // Empty payload -> `0 { Variant }: ();`
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
                    for (j, (ty, ref_kind)) in types.iter().enumerate() {
                        let pas_ty = field_type_for_ref_kind(ty, ref_kind, ir);
                        parts.push(format!("Payload_{}_{}: {}", variant_suffix, j, pas_ty));
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

// emit_callback_typedef removed — callback typedefs are now forward-
// declared as opaque `T = Pointer;` (see emit_forward_pointer_decls)
// because Pascal procedural-type definitions need every referenced
// struct already declared, which we can't guarantee for the recursive
// shapes in api.json. Field-position storage uses Pointer; callers
// cast through a procedural type at the invocation site.

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

// ============================================================================
// Monomorphized type aliases (generic instantiations like CssPropertyValue<T>)
// ============================================================================

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    if !ta.doc.is_empty() {
        for d in &ta.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    let t = record_type_name(&ta.name);

    match &mono.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            // Pascal enum: `type TAzFooTag = (A, B, C);`. The
            // constants are unscoped — prefix to avoid collisions.
            let tag_name = format!("{}Tag", t);
            builder.line(&format!("{} = (", tag_name));
            builder.indent();
            for (i, v) in variants.iter().enumerate() {
                let suffix = if i + 1 < variants.len() { "," } else { "" };
                builder.line(&format!(
                    "{}_{}{}",
                    tag_name,
                    sanitize_identifier(v),
                    suffix
                ));
            }
            builder.dedent();
            builder.line(");");
            // Type alias so `TAzFoo` works wherever the tag is referenced.
            builder.line(&format!("{} = {};", t, tag_name));
            builder.blank();
        }

        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
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
            for f in fields {
                emit_field(builder, f, ir);
            }
            builder.dedent();
            builder.line("end;");
            builder.blank();
        }

        MonomorphizedKind::TaggedUnion { variants, .. } => {
            // Tag width must be 1 byte (Rust `#[repr(C, u8)]`) — see
            // the comment in `emit_tagged_union` above. Case labels are
            // integer literals; the variant name is kept in a block
            // comment for readability.
            let tag_name = format!("{}Tag", t);

            builder.line(&format!("{} = record", t));
            builder.indent();
            builder.line(&format!("case Tag: cuint8 of {{ values of {} }}", tag_name));
            builder.indent();
            for (i, v) in variants.iter().enumerate() {
                let case_label =
                    format!("{} {{ {}_{} }}", i, tag_name, sanitize_identifier(&v.name));
                let variant_suffix = sanitize_identifier(&v.name);
                match &v.payload_type {
                    None => builder.line(&format!("{}: ();", case_label)),
                    Some(payload_ty) => {
                        let pas_ty =
                            field_type_for_ref_kind(payload_ty, &v.payload_ref_kind, ir);
                        builder.line(&format!(
                            "{}: (Payload_{}: {});",
                            case_label, variant_suffix, pas_ty
                        ));
                    }
                }
            }
            builder.dedent();
            builder.dedent();
            builder.line("end;");
            builder.blank();
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
