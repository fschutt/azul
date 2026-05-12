//! Struct, enum, callback typedef, and tagged-union emission for the
//! Common Lisp generator.
//!
//! Strategy:
//! - **Unit-only enums** -> `(cffi:defcenum az-foo (:variant-a 0)
//!   (:variant-b 1) ...)`. CFFI converts between the keyword and the
//!   integer transparently at call sites.
//! - **Tagged-union enums** -> a `defcenum` for the discriminator
//!   (`az-foo-tag`) + one `defcstruct` per variant payload (each leads
//!   with the `tag` slot so the layout matches the C ABI's
//!   tag-then-payload representation) + a `defcunion az-foo` overlapping
//!   all variant payloads at offset 0. We do NOT emit a separate "outer"
//!   struct because CFFI's `defcunion` already supports
//!   `:struct`-typed slots.
//! - **POD structs** -> `(cffi:defcstruct az-foo (slot-a :uint32) ...)`.
//! - **Callback typedefs** -> `(cffi:defctype az-foo-callback-type
//!   :pointer)` plus a comment describing the canonical signature. CFFI
//!   `defcallback` is used at call sites by user code; we don't emit
//!   trampolines here because we have no Lisp callable to bind.
//! - **Generic / Recursive / VecRef / Boxed / DestructorOrClone** are
//!   skipped entirely (they're internal to the Rust side of the API).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, StructDef, TypeAliasDef, TypeCategory,
};
use super::{ident_to_kebab, map_type_to_cffi, to_kebab_case};

// =============================================================================
// Top-level type emission
// =============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Type definitions: enums, POD structs, tagged-union structs+unions.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    // Emit type definitions in the IR builder's topological `sort_order`
    // (same pass `lang_c` uses). Earlier code claimed CFFI tolerates
    // forward references inside `(:struct foo)`, but in practice
    // `notice-foreign-struct-definition` resolves the payload's struct
    // at evaluation time and raises `Unknown CFFI type (:STRUCT AZ-FOO)`
    // if the layout hasn't been declared yet.
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

    // Callback typedefs: emit each as `defctype foo-type :pointer`.
    if !ir.callback_typedefs.is_empty() {
        builder.line(";; ----------------------------------------------------------------------------");
        builder.line(";; Callback typedefs (raw function pointers).");
        builder.line(";;");
        builder.line(";; Lisp callers use (cffi:defcallback name ret ((arg type) ...) body)");
        builder.line(";; and pass the resulting pointer where these typedefs are expected.");
        builder.line(";; ----------------------------------------------------------------------------");
        builder.blank();
        for cb in &ir.callback_typedefs {
            emit_callback_typedef(builder, cb, ir);
        }
    }

    Ok(())
}

// =============================================================================
// Inclusion filters (mirror the C# / Lua filters)
// =============================================================================

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    // VecRef + DestructorOrClone unfiltered to match the C header — Vec
    // wrappers reference them as fields.
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
// Unit-only enum -> defcenum
// =============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    let lisp_name = to_kebab_case(&e.name);

    // Emit doc as preceding comment lines.
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    let underlying = enum_underlying_type(e);
    builder.line(&format!("(defcenum ({} {})", lisp_name, underlying));
    builder.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        let kw = ident_to_kebab(&v.name);
        if idx == e.variants.len() - 1 {
            builder.line(&format!("(:{} {}))", kw, idx));
        } else {
            builder.line(&format!("(:{} {})", kw, idx));
        }
    }
    builder.dedent();
    builder.blank();
}

fn enum_underlying_type(e: &EnumDef) -> &'static str {
    repr_to_underlying(e.repr.as_deref())
}

fn repr_to_underlying(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => ":uint8",
        Some(r) if r.contains("i8") => ":int8",
        Some(r) if r.contains("u16") => ":uint16",
        Some(r) if r.contains("i16") => ":int16",
        Some(r) if r.contains("i32") => ":int32",
        Some(r) if r.contains("u64") => ":uint64",
        Some(r) if r.contains("i64") => ":int64",
        // Default to :uint32 -- matches C's default `unsigned int` enum.
        _ => ":uint32",
    }
}

// =============================================================================
// Tagged union -> defcenum + per-variant defcstruct + defcunion
// =============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let lisp_name = to_kebab_case(&e.name);
    let tag_name = format!("{}-tag", lisp_name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    // Tag enum. Use the actual repr from Rust source (e.g. `#[repr(C, u8)]`
    // → `:uint8`) so CFFI reads exactly the right number of bytes from the
    // tag slot. Hard-coding `:uint32` here mis-reads `#[repr(C, u8)]` tagged
    // unions: CFFI reads 4 bytes at offset 0, the top 3 bytes are payload
    // padding/data, the resulting u32 value is meaningless and CFFI then
    // can't match it against any declared variant (manifests as
    // `2706158337 is not defined as a value for enum type` on
    // AzNamedFontVecDestructor's `External` variant).
    builder.line(&format!(
        "(defcenum ({} {})",
        tag_name,
        enum_underlying_type(e)
    ));
    builder.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        let kw = ident_to_kebab(&v.name);
        if idx == e.variants.len() - 1 {
            builder.line(&format!("(:{} {}))", kw, idx));
        } else {
            builder.line(&format!("(:{} {})", kw, idx));
        }
    }
    builder.dedent();
    builder.blank();

    // Per-variant payload structs: each carries the tag as its first slot
    // followed by the payload, exactly like the C-ABI memory layout.
    for v in &e.variants {
        let variant_struct =
            format!("{}-variant-{}", lisp_name, ident_to_kebab(&v.name));
        builder.line(&format!("(defcstruct {}", variant_struct));
        builder.indent();
        builder.line(&format!("(tag {})", tag_name));
        match &v.kind {
            EnumVariantKind::Unit => {
                // No payload.
            }
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let cffi_ty = ref_kind_field_type(ty, ref_kind, ir);
                    builder.line(&format!("(payload {})", cffi_ty));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let cffi_ty = ref_kind_field_type(ty, ref_kind, ir);
                        builder.line(&format!("(payload-{} {})", i, cffi_ty));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let cffi_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
                    builder.line(&format!(
                        "({} {})",
                        ident_to_kebab(&f.name),
                        cffi_ty
                    ));
                }
            }
        }
        builder.dedent();
        // Close the defcstruct
        emit_close(builder);
    }

    // Outer union: every variant struct overlapping at offset 0.
    builder.line(&format!("(defcunion {}", lisp_name));
    builder.indent();
    for v in &e.variants {
        let variant_struct =
            format!("{}-variant-{}", lisp_name, ident_to_kebab(&v.name));
        builder.line(&format!(
            "({} (:struct {}))",
            ident_to_kebab(&v.name),
            variant_struct
        ));
    }
    builder.dedent();
    emit_close(builder);
}

// =============================================================================
// POD struct -> defcstruct
// =============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let lisp_name = to_kebab_case(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    builder.line(&format!("(defcstruct {}", lisp_name));
    builder.indent();

    if s.fields.is_empty() {
        // CFFI accepts empty structs but some Lisps grumble; emit a
        // single zero-byte filler slot for layout-compat with the C
        // ABI's "empty struct == 0 bytes" rule (Rust extern "C" follows
        // this).
        builder.line(";; SKIPPED: zero-field struct -- emit a 1-byte filler so CFFI accepts it.");
        builder.line("(_dummy :uint8)");
    } else {
        for f in &s.fields {
            emit_struct_field(builder, f, ir);
        }
    }

    builder.dedent();
    emit_close(builder);
}

fn emit_struct_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!(";; {}", sanitize_comment(doc)));
    }

    // Detect array types: `[T; N]` -> `(:array <elem> N)`.
    let cffi_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("({} {})", ident_to_kebab(&f.name), cffi_ty));
}

// =============================================================================
// Monomorphized type alias (e.g. `PhysicalSize<u32>` -> `AzPhysicalSizeU32`)
// =============================================================================
//
// The IR builder has already pre-instantiated each generic alias with
// concrete payloads. Emit the resulting concrete type the same way we
// emit non-generic types — SimpleEnum -> defcenum, Struct -> defcstruct,
// TaggedUnion -> tag defcenum + per-variant defcstruct + defcunion.

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    md: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    let lisp_name = to_kebab_case(&ta.name);

    if !ta.doc.is_empty() {
        for d in &ta.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    match &md.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let underlying = repr
                .as_deref()
                .map(|r| match r {
                    s if s.contains("u8") => ":uint8",
                    s if s.contains("i8") => ":int8",
                    s if s.contains("u16") => ":uint16",
                    s if s.contains("i16") => ":int16",
                    s if s.contains("i32") => ":int32",
                    s if s.contains("u64") => ":uint64",
                    s if s.contains("i64") => ":int64",
                    _ => ":uint32",
                })
                .unwrap_or(":uint32");
            builder.line(&format!("(defcenum ({} {})", lisp_name, underlying));
            builder.indent();
            for (idx, v) in variants.iter().enumerate() {
                let kw = ident_to_kebab(v);
                if idx == variants.len() - 1 {
                    builder.line(&format!("(:{} {}))", kw, idx));
                } else {
                    builder.line(&format!("(:{} {})", kw, idx));
                }
            }
            builder.dedent();
            builder.blank();
        }
        MonomorphizedKind::Struct { fields } => {
            builder.line(&format!("(defcstruct {}", lisp_name));
            builder.indent();
            if fields.is_empty() {
                builder.line("(_dummy :uint8)");
            } else {
                for f in fields {
                    emit_struct_field(builder, f, ir);
                }
            }
            builder.dedent();
            emit_close(builder);
        }
        MonomorphizedKind::TaggedUnion { variants, repr } => {
            let tag_name = format!("{}-tag", lisp_name);
            let underlying = repr_to_underlying(repr.as_deref());
            builder.line(&format!("(defcenum ({} {})", tag_name, underlying));
            builder.indent();
            for (idx, v) in variants.iter().enumerate() {
                let kw = ident_to_kebab(&v.name);
                if idx == variants.len() - 1 {
                    builder.line(&format!("(:{} {}))", kw, idx));
                } else {
                    builder.line(&format!("(:{} {})", kw, idx));
                }
            }
            builder.dedent();
            builder.blank();

            for v in variants {
                let variant_struct =
                    format!("{}-variant-{}", lisp_name, ident_to_kebab(&v.name));
                builder.line(&format!("(defcstruct {}", variant_struct));
                builder.indent();
                builder.line(&format!("(tag {})", tag_name));
                if let Some(ref payload_ty) = v.payload_type {
                    let cffi_ty = ref_kind_field_type(payload_ty, &v.payload_ref_kind, ir);
                    builder.line(&format!("(payload {})", cffi_ty));
                }
                builder.dedent();
                emit_close(builder);
            }

            builder.line(&format!("(defcunion {}", lisp_name));
            builder.indent();
            for v in variants {
                let variant_struct =
                    format!("{}-variant-{}", lisp_name, ident_to_kebab(&v.name));
                builder.line(&format!(
                    "({} (:struct {}))",
                    ident_to_kebab(&v.name),
                    variant_struct
                ));
            }
            builder.dedent();
            emit_close(builder);
        }
    }
}

// =============================================================================
// Callback typedef -> defctype az-foo-callback-type :pointer
// =============================================================================

fn emit_callback_typedef(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) {
    let lisp_name = to_kebab_case(&cb.name);

    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    // Emit the canonical signature in a comment so users know what to
    // bind in their `defcallback`.
    let return_cffi = cb
        .return_type
        .as_deref()
        .map(|r| map_type_to_cffi(r, ir))
        .unwrap_or_else(|| ":void".to_string());
    let arg_descs: Vec<String> = cb
        .args
        .iter()
        .map(|a| {
            let ty = match a.ref_kind {
                super::super::ir::ArgRefKind::Owned => map_type_to_cffi(&a.type_name, ir),
                _ => ":pointer".to_string(),
            };
            format!("({} {})", ident_to_kebab(&a.name), ty)
        })
        .collect();
    builder.line(&format!(
        ";; signature: ({} {} ({}))",
        return_cffi,
        lisp_name,
        arg_descs.join(" ")
    ));
    builder.line(&format!("(defctype {} :pointer)", lisp_name));
    builder.blank();
}

// =============================================================================
// Helpers
// =============================================================================

fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_cffi(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => ":pointer".to_string(),
    }
}

/// Emit the closing `)` of a top-level form on its own line followed
/// by a blank line. Keeping this in a helper keeps the per-form code
/// shape identical across all emitters.
fn emit_close(builder: &mut CodeBuilder) {
    // The previous `line(...)` for the last slot/variant has already
    // been written; we just need to close the outermost form. Use
    // `raw` so we don't get a leading indent.
    builder.line(")");
    builder.blank();
}

/// Strip newlines / disallowed characters from a doc string so it fits
/// on a single Lisp `;` comment line.
fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
