//! C-ABI layout (size + alignment) computation for the Fortran binding.
//!
//! Fortran has no native `union`, so tagged-union enums cannot be spelled
//! field-for-field like in C. The ONLY layout-correct representation is an
//! opaque blob with the exact size and alignment of the C union — anything
//! else (the old `{integer(c_int) tag; type(c_ptr) payload}` 16-byte shape)
//! shifts every embedding struct and corrupts all by-value FFI calls
//! (root cause of the 2026-07 Fortran e2e SIGSEGV in `AzApp_create`).
//!
//! This module computes `(size, align)` for any IR type under the shared
//! 64-bit C ABI (pointers = 8 bytes; identical on x86_64/aarch64
//! Linux/macOS/Windows for every construct api.json uses), mirroring
//! exactly what `lang_c` emits into `azul.h`:
//!
//! - unit enums are C `enum`s → 4 bytes (azul.h spells fields with the enum type, which is
//!   `int`-sized in C);
//! - tagged unions are `union { struct { tag; payload... } variant; ... }` where the tag is
//!   `uint8_t` iff the repr contains "u8", else the C tag enum (4 bytes);
//! - non-Owned field refs (Ref/Ptr/Boxed/...) are pointers (8 bytes);
//! - callback typedefs are function pointers (8 bytes).
//!
//! Verified against `clang` ground truth (sizeof/alignof of every type in
//! azul.h) — 1536/1536 types match; see the 2026-07-04 session notes.

use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, MonomorphizedKind,
    MonomorphizedTypeDef,
};

/// Size + alignment of a type under the 64-bit C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AbiLayout {
    pub size: usize,
    pub align: usize,
}

impl AbiLayout {
    const fn new(size: usize, align: usize) -> Self {
        Self { size, align }
    }
}

const PTR: AbiLayout = AbiLayout::new(8, 8);

fn align_to(off: usize, align: usize) -> usize {
    if align == 0 {
        return off;
    }
    off.div_ceil(align) * align
}

/// Recursion guard: layouts deeper than this bail out with `None`.
/// api.json's by-value nesting is ~15 deep; true recursion always goes
/// through a Box/pointer (8 bytes) and never recurses here.
const MAX_DEPTH: usize = 64;

/// Compute the C-ABI layout of the type named `name` (api.json spelling,
/// no `Az` prefix). Returns `None` for generic templates / unknown names.
pub(crate) fn type_layout(name: &str, ir: &CodegenIR) -> Option<AbiLayout> {
    type_layout_inner(name, ir, 0)
}

fn primitive_layout(name: &str) -> Option<AbiLayout> {
    // Must classify identically to `map_type_to_fortran` so the emitted
    // Fortran field decl and the computed layout can never disagree.
    Some(match name {
        "bool" | "GLboolean" | "i8" | "u8" | "c_char" | "char" | "c_uchar" => AbiLayout::new(1, 1),
        "i16" | "u16" => AbiLayout::new(2, 2),
        "i32" | "u32" | "c_int" | "c_uint" | "GLint" | "GLuint" | "GLenum" | "GLbitfield"
        | "GLsizei" | "f32" | "GLfloat" | "GLclampf" => AbiLayout::new(4, 4),
        "i64" | "u64" | "GLint64" | "GLuint64" | "f64" | "GLdouble" | "GLclampd" | "usize"
        | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => {
            AbiLayout::new(8, 8)
        }
        _ => return None,
    })
}

fn type_layout_inner(name: &str, ir: &CodegenIR, depth: usize) -> Option<AbiLayout> {
    if depth > MAX_DEPTH {
        return None;
    }
    let trimmed = name.trim();

    // Pointer / reference spellings inside variant payload types.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return Some(PTR);
    }

    if let Some(p) = primitive_layout(trimmed) {
        return Some(p);
    }

    // Callback typedefs are C function pointers.
    if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
        return Some(PTR);
    }

    if let Some(e) = ir.find_enum(trimmed) {
        if !e.generic_params.is_empty() {
            return None;
        }
        return enum_layout(e, ir, depth);
    }

    if let Some(s) = ir.find_struct(trimmed) {
        if !s.generic_params.is_empty() {
            return None;
        }
        return fields_layout(s.fields.iter(), ir, depth, None);
    }

    if let Some(ta) = ir.find_type_alias(trimmed) {
        if let Some(mono) = &ta.monomorphized_def {
            return mono_layout(mono, ir, depth);
        }
        return type_layout_inner(&ta.target, ir, depth + 1);
    }

    None
}

/// Layout of a unit enum (C `enum` → int) or a tagged union.
fn enum_layout(e: &EnumDef, ir: &CodegenIR, depth: usize) -> Option<AbiLayout> {
    if !e.is_union {
        // azul.h spells unit-enum fields with the C enum type (int-sized),
        // and the Fortran side declares them `integer(c_int)`.
        return Some(AbiLayout::new(4, 4));
    }
    let tag = tag_layout(e.repr.as_deref());
    let mut size = 0usize;
    let mut align = tag.align;
    for v in &e.variants {
        let vl = match &v.kind {
            EnumVariantKind::Unit => tag,
            EnumVariantKind::Tuple(types) => variant_layout(
                tag,
                types.iter().map(|(t, rk)| (t.as_str(), *rk)),
                ir,
                depth,
            )?,
            EnumVariantKind::Struct(fields) => variant_layout(
                tag,
                fields.iter().map(|f| (f.type_name.as_str(), f.ref_kind)),
                ir,
                depth,
            )?,
        };
        size = size.max(vl.size);
        align = align.max(vl.align);
    }
    Some(AbiLayout::new(align_to(size.max(1), align), align))
}

/// Layout of a monomorphized generic instantiation.
pub(crate) fn mono_layout(
    mono: &MonomorphizedTypeDef,
    ir: &CodegenIR,
    depth: usize,
) -> Option<AbiLayout> {
    match &mono.kind {
        MonomorphizedKind::SimpleEnum { .. } => Some(AbiLayout::new(4, 4)),
        MonomorphizedKind::Struct { fields } => fields_layout(fields.iter(), ir, depth, None),
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            // The tag is read from the SAME `repr` string lang_c reads for its
            // `uint8_t tag;` / `AzFoo_Tag tag;` choice, so a monomorphized
            // `CssPropertyValue<Color>` (repr "C, u8", payload ColorU = 4×u8)
            // is 5 bytes here exactly as in azul.h. This branch used to pin
            // the tag at 4 bytes on the theory that lang_c always emitted the
            // C enum for monos; lang_c stopped doing that, and every one of
            // the 13 `CssPropertyValue<Color>` blobs was 8 bytes in Fortran
            // against 5 in C. See `tests::mono_u8_union_tag_matches_lang_c`.
            let tag = tag_layout(repr.as_deref());
            let mut size = 0usize;
            let mut align = tag.align;
            for v in variants {
                let vl = match &v.payload_type {
                    None => tag,
                    Some(p) => variant_layout(
                        tag,
                        std::iter::once((p.as_str(), v.payload_ref_kind)),
                        ir,
                        depth,
                    )?,
                };
                size = size.max(vl.size);
                align = align.max(vl.align);
            }
            Some(AbiLayout::new(align_to(size.max(1), align), align))
        }
    }
}

/// Tag layout: `uint8_t` iff the repr contains "u8" (mirrors lang_c's
/// `is_u8_repr` + `Force8Bit` emission), else the C tag enum (int).
fn tag_layout(repr: Option<&str>) -> AbiLayout {
    if repr.map(|r| r.contains("u8")).unwrap_or(false) {
        AbiLayout::new(1, 1)
    } else {
        AbiLayout::new(4, 4)
    }
}

/// C struct layout over `tag` + the payload members (a union variant).
fn variant_layout<'a>(
    tag: AbiLayout,
    payloads: impl Iterator<Item = (&'a str, FieldRefKind)>,
    ir: &CodegenIR,
    depth: usize,
) -> Option<AbiLayout> {
    let mut off = tag.size;
    let mut align = tag.align;
    for (ty, rk) in payloads {
        let l = member_layout(ty, rk, ir, depth)?;
        off = align_to(off, l.align) + l.size;
        align = align.max(l.align);
    }
    Some(AbiLayout::new(align_to(off, align), align))
}

/// C struct layout over plain named fields.
fn fields_layout<'a>(
    fields: impl Iterator<Item = &'a FieldDef>,
    ir: &CodegenIR,
    depth: usize,
    prepend: Option<AbiLayout>,
) -> Option<AbiLayout> {
    let mut off = 0usize;
    let mut align = 1usize;
    if let Some(p) = prepend {
        off = p.size;
        align = p.align;
    }
    let mut any = prepend.is_some();
    for f in fields {
        let l = member_layout(&f.type_name, f.ref_kind, ir, depth)?;
        off = align_to(off, l.align) + l.size;
        align = align.max(l.align);
        any = true;
    }
    if !any {
        // Empty structs are emitted with a 1-byte dummy field.
        return Some(AbiLayout::new(1, 1));
    }
    Some(AbiLayout::new(align_to(off, align), align))
}

fn member_layout(
    type_name: &str,
    ref_kind: FieldRefKind,
    ir: &CodegenIR,
    depth: usize,
) -> Option<AbiLayout> {
    match ref_kind {
        FieldRefKind::Owned => type_layout_inner(type_name, ir, depth + 1),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => Some(PTR),
    }
}

/// Emit the single Fortran component declaration for an ABI-opaque blob of
/// the given layout: an array of the widest integer kind matching the
/// alignment. `size` is always a multiple of `align` for C aggregates.
pub(crate) fn blob_field_decl(l: AbiLayout) -> String {
    match l.align {
        8 => format!("integer(c_int64_t) :: opaque_({})", l.size / 8),
        4 => format!("integer(c_int32_t) :: opaque_({})", l.size / 4),
        2 => format!("integer(c_int16_t) :: opaque_({})", l.size / 2),
        _ => format!("integer(c_int8_t) :: opaque_({})", l.size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::v2::ir::{
        FieldRefKind, MonomorphizedKind, MonomorphizedTypeDef, MonomorphizedVariant,
    };

    fn union(repr: Option<&str>) -> MonomorphizedTypeDef {
        MonomorphizedTypeDef {
            kind: MonomorphizedKind::TaggedUnion {
                repr: repr.map(str::to_string),
                variants: vec![
                    MonomorphizedVariant {
                        name: "Auto".into(),
                        payload_type: None,
                        payload_ref_kind: FieldRefKind::Owned,
                    },
                    MonomorphizedVariant {
                        name: "Exact".into(),
                        payload_type: Some("u8".into()),
                        payload_ref_kind: FieldRefKind::Owned,
                    },
                ],
            },
        }
    }

    /// A monomorphized `#[repr(C, u8)]` union has a one-byte tag, exactly as
    /// lang_c spells it (`uint8_t tag;`); without the u8 repr the tag is the
    /// int-sized C enum. The 13 `CssPropertyValue<Color>` blobs were 8 bytes
    /// in Fortran against 5 in azul.h when this was pinned at 4.
    #[test]
    fn mono_u8_union_tag_matches_lang_c() {
        let ir = CodegenIR::new();
        let u8_tag = mono_layout(&union(Some("C, u8")), &ir, 0).expect("layout");
        assert_eq!((u8_tag.size, u8_tag.align), (2, 1), "u8 tag + u8 payload");
        let int_tag = mono_layout(&union(None), &ir, 0).expect("layout");
        assert_eq!(
            (int_tag.size, int_tag.align),
            (8, 4),
            "int tag + u8 payload, padded"
        );
    }
}
