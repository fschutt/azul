//! Struct, enum, tagged-union, and callback-typedef emission for the
//! Racket generator.
//!
//! Strategy (mirrors the CFFI generator, adapted to `ffi/unsafe`):
//! - **Unit-only enums** → integer `(define AzUpdate_RefreshDom 1)`
//!   constants plus a `(define _AzUpdate _uint32)` ctype alias. The C ABI
//!   passes fieldless enums as their repr int, so a plain int alias is the
//!   right ctype for a by-value arg/field.
//! - **Tagged-union enums** → one `define-cstruct` per variant payload
//!   (each leading with a `tag` slot, matching the C ABI's
//!   tag-then-payload layout) + a wrapping `(define _AzFoo (_union …))`
//!   so a by-value arg/field gets the correct max-variant size. Tag
//!   constants (`AzFoo_Tag_Bar`) are emitted for inspection.
//! - **POD structs** → `(define-cstruct _AzFoo ([slot _uint32] …))`,
//!   which also binds `make-AzFoo`, `AzFoo-slot`, `set-AzFoo-slot!`, and
//!   `_AzFoo-pointer`.
//! - **Callback typedefs** → `(define _AzFooCallbackType _fpointer)`.
//! - **Generic / Recursive** types are skipped (internal to the Rust side).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, StructDef, TypeAliasDef, TypeCategory,
};
use super::{c_name, ctype_name, field_ident, map_type_to_racket};

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Type definitions: enums, POD structs, tagged-union structs + unions.");
    builder.line(";;");
    builder.line(";; Emitted in the IR builder's topological sort_order (same pass lang_c and");
    builder.line(";; lang_lisp use): `define-cstruct` resolves an embedded `_AzFoo` ctype at");
    builder.line(";; expansion time, so the layout it references must already be defined.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    enum TypeItem<'a> {
        UnitEnum(&'a EnumDef),
        TaggedUnion(&'a EnumDef),
        Struct(&'a StructDef),
        MonoAlias(&'a TypeAliasDef, &'a MonomorphizedTypeDef),
    }
    let mut items: Vec<(usize, TypeItem)> = Vec::new();
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            items.push((e.sort_order, TypeItem::TaggedUnion(e)));
        } else {
            items.push((e.sort_order, TypeItem::UnitEnum(e)));
        }
    }
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            continue;
        }
        items.push((s.sort_order, TypeItem::Struct(s)));
    }
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        if let Some(ref md) = ta.monomorphized_def {
            items.push((ta.sort_order, TypeItem::MonoAlias(ta, md)));
        }
    }
    items.sort_by_key(|(ord, _)| *ord);
    for (_, item) in items {
        match item {
            TypeItem::UnitEnum(e) => emit_unit_enum(builder, e),
            TypeItem::TaggedUnion(e) => emit_tagged_union(builder, e, ir),
            TypeItem::Struct(s) => emit_struct(builder, s, ir),
            TypeItem::MonoAlias(ta, md) => emit_monomorphized_alias(builder, ta, md, ir),
        }
    }

    if !ir.callback_typedefs.is_empty() {
        builder.line(";; ----------------------------------------------------------------------------");
        builder.line(";; Callback typedefs (raw function pointers).");
        builder.line(";;");
        builder.line(";; Racket closures become real C fn-ptrs via `_fun`; user code passes");
        builder.line(";; plain procedures to the wrapper setters (e.g. button-set-on-click),");
        builder.line(";; which route through `register-callback`. These `_fpointer` aliases are");
        builder.line(";; the opaque ctype for the `cb` slot inside each callback-wrapper struct.");
        builder.line(";; ----------------------------------------------------------------------------");
        builder.blank();
        for cb in &ir.callback_typedefs {
            emit_callback_typedef(builder, cb);
        }
    }

    Ok(())
}

// =============================================================================
// Inclusion filters (mirror lang_lisp)
// =============================================================================

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
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

// =============================================================================
// Unit-only enum -> int-alias ctype + constants
// =============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    let ct = ctype_name(&e.name);
    let underlying = enum_underlying_type(e);

    for d in &e.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }
    // The by-value ctype: a fieldless C enum is its repr int at the ABI.
    builder.line(&format!("(define {} {})", ct, underlying));
    // One integer constant per variant (0-based, C-ABI order).
    let cn = c_name(&e.name);
    for (idx, v) in e.variants.iter().enumerate() {
        builder.line(&format!("(define {}_{} {})", cn, v.name, idx));
    }
    builder.blank();
}

fn enum_underlying_type(e: &EnumDef) -> &'static str {
    repr_to_underlying(e.repr.as_deref())
}

fn repr_to_underlying(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "_uint8",
        Some(r) if r.contains("i8") => "_int8",
        Some(r) if r.contains("u16") => "_uint16",
        Some(r) if r.contains("i16") => "_int16",
        Some(r) if r.contains("i32") => "_int32",
        Some(r) if r.contains("u64") => "_uint64",
        Some(r) if r.contains("i64") => "_int64",
        _ => "_uint32",
    }
}

// =============================================================================
// Tagged union -> per-variant define-cstruct + _union alias + tag constants
// =============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let ct = ctype_name(&e.name);
    let cn = c_name(&e.name);

    for d in &e.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }

    // Tag constants (Az<Enum>_Tag_<Variant>) — mirror the C header, used
    // for inspecting the active variant of a returned union.
    let underlying = enum_underlying_type(e);
    for (idx, v) in e.variants.iter().enumerate() {
        builder.line(&format!("(define {}_Tag_{} {})", cn, v.name, idx));
    }

    // Per-variant payload cstructs: `tag` slot (plain int, matching the C
    // ABI byte width) followed by the payload. Every variant struct starts
    // with the tag at offset 0, so overlapping them in a union is
    // ABI-faithful (max-variant size, correct field offsets).
    for v in &e.variants {
        let variant_ct = format!("{}_Variant_{}", ct, v.name);
        builder.line(&format!("(define-cstruct {}", variant_ct));
        builder.indent();
        builder.line("(");
        builder.indent();
        // Field name must NOT be `tag`: define-cstruct auto-binds `<name>-tag`
        // (the C pointer tag), so a field named `tag` would make its accessor
        // collide with that binding ("identifier already defined") and the whole
        // module fails to load. `variant-tag` accessor is `<name>-variant-tag`.
        builder.line(&format!("[variant-tag {}]", underlying));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, rk) = &types[0];
                    builder.line(&format!("[payload {}]", ref_kind_field_type(ty, rk, ir)));
                } else {
                    for (i, (ty, rk)) in types.iter().enumerate() {
                        builder.line(&format!(
                            "[payload-{} {}]",
                            i,
                            ref_kind_field_type(ty, rk, ir)
                        ));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    builder.line(&format!(
                        "[{} {}]",
                        field_ident(&f.name),
                        ref_kind_field_type(&f.type_name, &f.ref_kind, ir)
                    ));
                }
            }
        }
        builder.dedent();
        builder.line("))");
        builder.dedent();
    }

    // Outer union alias: every variant struct overlapping at offset 0.
    let members: Vec<String> = e
        .variants
        .iter()
        .map(|v| format!("{}_Variant_{}", ct, v.name))
        .collect();
    builder.line(&format!("(define {} (_union {}))", ct, members.join(" ")));
    builder.blank();
}

// =============================================================================
// POD struct -> define-cstruct
// =============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let ct = ctype_name(&s.name);

    for d in &s.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }

    builder.line(&format!("(define-cstruct {}", ct));
    builder.indent();
    builder.line("(");
    builder.indent();
    if s.fields.is_empty() {
        // Rust extern "C" empty structs are 0 bytes; Racket's define-cstruct
        // needs at least one slot. A single u8 filler keeps the binding
        // loadable (nothing reads these zero-field internal types).
        builder.line("[_dummy _uint8]");
    } else {
        for f in &s.fields {
            emit_struct_field(builder, f, ir);
        }
    }
    builder.dedent();
    builder.line("))");
    builder.dedent();
    builder.blank();
}

fn emit_struct_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!(";; {}", sanitize_comment(doc)));
    }
    let ct = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("[{} {}]", field_ident(&f.name), ct));
}

// =============================================================================
// Monomorphized type alias
// =============================================================================

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    md: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    let ct = ctype_name(&ta.name);
    let cn = c_name(&ta.name);

    for d in &ta.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }

    match &md.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let underlying = repr_to_underlying(repr.as_deref());
            builder.line(&format!("(define {} {})", ct, underlying));
            for (idx, v) in variants.iter().enumerate() {
                builder.line(&format!("(define {}_{} {})", cn, v, idx));
            }
            builder.blank();
        }
        MonomorphizedKind::Struct { fields } => {
            builder.line(&format!("(define-cstruct {}", ct));
            builder.indent();
            builder.line("(");
            builder.indent();
            if fields.is_empty() {
                builder.line("[_dummy _uint8]");
            } else {
                for f in fields {
                    emit_struct_field(builder, f, ir);
                }
            }
            builder.dedent();
            builder.line("))");
            builder.dedent();
            builder.blank();
        }
        MonomorphizedKind::TaggedUnion { variants, repr } => {
            let underlying = repr_to_underlying(repr.as_deref());
            for (idx, v) in variants.iter().enumerate() {
                builder.line(&format!("(define {}_Tag_{} {})", cn, v.name, idx));
            }
            for v in variants {
                let variant_ct = format!("{}_Variant_{}", ct, v.name);
                builder.line(&format!("(define-cstruct {}", variant_ct));
                builder.indent();
                builder.line("(");
                builder.indent();
                // See emit_tagged_union: `tag` collides with define-cstruct's
                // auto-bound `<name>-tag`, so name the discriminant `variant-tag`.
                builder.line(&format!("[variant-tag {}]", underlying));
                if let Some(ref payload_ty) = v.payload_type {
                    builder.line(&format!(
                        "[payload {}]",
                        ref_kind_field_type(payload_ty, &v.payload_ref_kind, ir)
                    ));
                }
                builder.dedent();
                builder.line("))");
                builder.dedent();
            }
            let members: Vec<String> = variants
                .iter()
                .map(|v| format!("{}_Variant_{}", ct, v.name))
                .collect();
            builder.line(&format!("(define {} (_union {}))", ct, members.join(" ")));
            builder.blank();
        }
    }
}

// =============================================================================
// Callback typedef -> _fpointer alias
// =============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let ct = ctype_name(&cb.name);
    for d in &cb.doc {
        builder.line(&format!(";; {}", sanitize_comment(d)));
    }
    builder.line(&format!("(define {} _fpointer)", ct));
}

// =============================================================================
// Helpers
// =============================================================================

fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_racket(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "_pointer".to_string(),
    }
}

fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
