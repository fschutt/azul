//! C-ABI layout (size + alignment) of any IR type, shared by every binding
//! that needs true C sizes: Fortran spells tagged unions as opaque blobs of
//! exactly this size (it has no `union`; the old 16-byte `{tag; payload}`
//! shape shifted every embedding struct, the 2026-07 Fortran e2e SIGSEGV in
//! `AzApp_create`), and the host-invoker writeback copies exactly this many
//! bytes of a callback's return value
//! (`managed_host_invoker::return_c_size`).
//!
//! This module computes `(size, align)` for any IR type under the shared
//! 64-bit C ABI (pointers = 8 bytes; identical on x86_64/aarch64
//! Linux/macOS/Windows for every construct api.json uses), mirroring
//! exactly what `lang_c` emits into `azul.h`:
//!
//! - unit enums are C `enum`s → 4 bytes (azul.h spells fields with the enum type, which is
//!   `int`-sized in C);
//! - tagged unions are `union { struct { tag; [padding;] payload... } variant; ... }` where the
//!   tag is `uint8_t` iff the repr contains "u8", else the C tag enum (4 bytes);
//! - non-Owned field refs (Ref/Ptr/Boxed/...) are pointers (8 bytes);
//! - callback typedefs are function pointers (8 bytes).
//!
//! # Tagged-union payloads: the ONE place that decides where they sit
//!
//! Rust lays a `#[repr(C, u8)]` / `#[repr(C)]` enum out as
//! `struct { tag; union { one repr(C) struct per variant } }`, so EVERY
//! variant's payload starts at the tag's size rounded up to the largest
//! alignment of ANY variant. A C struct `{ tag; payload; }` per variant
//! aligns the payload to its OWN alignment instead, which put e.g.
//! `StyleBackgroundContent::Color` (ColorU, align 1) at offset 1 in azul.h
//! and at 8 in Rust (210 variants of 47 unions were off; the sizes agreed,
//! so only readers broke). [`union_payload_layout`] computes Rust's offset
//! and the padding each per-variant struct needs after its tag to reach
//! it; lang_c and every binding that mirrors the per-variant structs emit
//! exactly that padding, and this module's own union layout models the
//! padded structs.
//!
//! # What was verified against what
//!
//! 2026-07-04: sizeof/alignof of every type in azul.h, computed here,
//! matched clang on the header itself (1536/1536). That compares this model
//! with the HEADER, not with Rust - it did not catch the misplaced union
//! payloads above, because the header had them misplaced too and the
//! sizes agree. Against Rust, the tests are
//! `bug_classes::a_union_variant_payload_starts_where_rust_puts_it`
//! (every union, offsets), `bug_classes::c_layout_sizes_every_tagged_union_like_rust`
//! (every union, sizes), the `_Static_assert`s azul.h carries for every
//! padded variant (checked by every C/C++ build), and the Rust ground truth
//! in `css/tests/a_union_payload_sits_after_the_largest_alignment.rs`.

use super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, MonomorphizedKind,
    MonomorphizedTypeDef, MonomorphizedVariant,
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
    union_layout(&enum_union_shape(e), ir, depth).map(|u| u.abi)
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
            union_layout(&mono_union_shape(repr.as_deref(), variants), ir, depth).map(|u| u.abi)
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

// ----------------------------------------------------------------------------
// Tagged-union payloads
// ----------------------------------------------------------------------------

/// Where the variant payloads of one tagged union sit (Rust's rule, see the
/// module doc), and what each per-variant C struct `{ tag; payload... }`
/// needs between its tag and its first payload member to put its payload
/// there. Computed by [`union_payload_layout`] - the one place bindings ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnionPayloadLayout {
    /// The tag: `uint8_t` for `repr(C, u8)`, the int-sized C enum otherwise.
    pub tag: AbiLayout,
    /// Byte offset of EVERY variant's payload: the tag's size rounded up to
    /// the largest alignment of any variant.
    pub payload_offset: usize,
    /// The whole union: the largest padded per-variant struct.
    pub abi: AbiLayout,
    /// Per variant, in declaration order: `(name, padding)`.
    variants: Vec<(String, usize)>,
}

impl UnionPayloadLayout {
    /// Bytes the C struct of `variant` needs right after its tag (azul.h:
    /// `uint8_t _pad0[N];`) so its payload starts at `payload_offset`:
    /// `payload_offset - tag.size` when C's own alignment of the first
    /// payload member would put it anywhere else, 0 otherwise (and for a
    /// unit variant or an unknown name). A binding whose records follow C's
    /// alignment rules emits exactly this; one that lays records out
    /// without alignment needs `payload_offset` itself.
    pub fn padding(&self, variant: &str) -> usize {
        self.variants
            .iter()
            .find(|(n, _)| n == variant)
            .map_or(0, |(_, p)| *p)
    }

    /// The variants that need padding, in declaration order:
    /// `(name, padding)`.
    pub fn padded_variants(&self) -> impl Iterator<Item = (&str, usize)> + '_ {
        self.variants
            .iter()
            .filter(|(_, p)| *p > 0)
            .map(|(n, p)| (n.as_str(), *p))
    }
}

/// The payload layout of the tagged union named `name` (api.json spelling,
/// no `Az` prefix): a data-carrying enum or a monomorphized generic alias
/// (`CaretColorValue = CssPropertyValue<CaretColor>`). `None` for any other
/// type, a generic template, or a payload without a known C layout.
pub(crate) fn union_payload_layout(name: &str, ir: &CodegenIR) -> Option<UnionPayloadLayout> {
    let name = name.trim();
    if let Some(e) = ir.find_enum(name) {
        if !e.is_union || !e.generic_params.is_empty() {
            return None;
        }
        return union_layout(&enum_union_shape(e), ir, 0);
    }
    match &ir.find_type_alias(name)?.monomorphized_def.as_ref()?.kind {
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            union_layout(&mono_union_shape(repr.as_deref(), variants), ir, 0)
        }
        _ => None,
    }
}

/// Shorthand for an emitter that writes one variant struct at a time:
/// [`UnionPayloadLayout::padding`] of `variant` in the union `union_name`
/// (0 when the union's layout is unknown). An emitter looping over all
/// variants of a union should call [`union_payload_layout`] once instead.
pub(crate) fn variant_payload_padding(union_name: &str, variant: &str, ir: &CodegenIR) -> usize {
    union_payload_layout(union_name, ir).map_or(0, |u| u.padding(variant))
}

/// A tagged union as its layout sees it: the tag, and per variant its name
/// and payload members `(api type, ref kind)` in declaration order (empty
/// for a unit variant). Regular enums and monomorphized aliases both reduce
/// to this, so ONE function lays both out.
struct UnionShape<'a> {
    tag: AbiLayout,
    variants: Vec<(&'a str, Vec<(&'a str, FieldRefKind)>)>,
}

fn enum_union_shape(e: &EnumDef) -> UnionShape<'_> {
    UnionShape {
        tag: tag_layout(e.repr.as_deref()),
        variants: e
            .variants
            .iter()
            .map(|v| {
                let members = match &v.kind {
                    EnumVariantKind::Unit => Vec::new(),
                    EnumVariantKind::Tuple(types) => {
                        types.iter().map(|(t, rk)| (t.as_str(), *rk)).collect()
                    }
                    EnumVariantKind::Struct(fields) => fields
                        .iter()
                        .map(|f| (f.type_name.as_str(), f.ref_kind))
                        .collect(),
                };
                (v.name.as_str(), members)
            })
            .collect(),
    }
}

fn mono_union_shape<'a>(
    repr: Option<&str>,
    variants: &'a [MonomorphizedVariant],
) -> UnionShape<'a> {
    UnionShape {
        tag: tag_layout(repr),
        variants: variants
            .iter()
            .map(|v| {
                let members = v
                    .payload_type
                    .iter()
                    .map(|p| (p.as_str(), v.payload_ref_kind))
                    .collect();
                (v.name.as_str(), members)
            })
            .collect(),
    }
}

/// Rust's payload offset for `shape`, each variant's padding, and the
/// union's layout as azul.h declares it (the largest padded per-variant
/// struct, which is also Rust's size: see
/// `bug_classes::c_layout_sizes_every_tagged_union_like_rust`).
fn union_layout(
    shape: &UnionShape<'_>,
    ir: &CodegenIR,
    depth: usize,
) -> Option<UnionPayloadLayout> {
    let tag = shape.tag;
    // Each variant's payload members: the repr(C) struct Rust puts in the
    // union. The union is aligned to the most-aligned of them.
    let mut members_of: Vec<Vec<AbiLayout>> = Vec::with_capacity(shape.variants.len());
    let mut union_align = 1usize;
    for (_, members) in &shape.variants {
        let layouts = members
            .iter()
            .map(|(t, rk)| member_layout(t, *rk, ir, depth))
            .collect::<Option<Vec<AbiLayout>>>()?;
        union_align = layouts.iter().fold(union_align, |a, l| a.max(l.align));
        members_of.push(layouts);
    }
    let payload_offset = align_to(tag.size, union_align);

    let mut size = tag.size;
    let mut align = tag.align;
    let mut variants = Vec::with_capacity(shape.variants.len());
    for ((name, _), layouts) in shape.variants.iter().zip(&members_of) {
        // Only the first payload member needs help: once it sits at
        // `payload_offset` (a multiple of every member's alignment), C
        // places the rest exactly where Rust's repr(C) payload struct does.
        let padding = match layouts.first() {
            Some(first) if align_to(tag.size, first.align) != payload_offset => {
                payload_offset - tag.size
            }
            _ => 0,
        };
        let v = variant_struct_layout(tag, padding, layouts);
        size = size.max(v.size);
        align = align.max(v.align);
        variants.push((name.to_string(), padding));
    }
    Some(UnionPayloadLayout {
        tag,
        payload_offset,
        abi: AbiLayout::new(align_to(size.max(1), align), align),
        variants,
    })
}

/// One per-variant struct of azul.h, `{ tag; uint8_t _pad0[padding];
/// members... }`, under C's struct rules.
fn variant_struct_layout(tag: AbiLayout, padding: usize, members: &[AbiLayout]) -> AbiLayout {
    let mut off = tag.size + padding;
    let mut align = tag.align;
    for l in members {
        off = align_to(off, l.align) + l.size;
        align = align.max(l.align);
    }
    AbiLayout::new(align_to(off, align), align)
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

    fn variant(name: &str, payload: Option<&str>) -> MonomorphizedVariant {
        MonomorphizedVariant {
            name: name.into(),
            payload_type: payload.map(str::to_string),
            payload_ref_kind: FieldRefKind::Owned,
        }
    }

    /// Rust puts EVERY payload of a `repr(C, u8)` union at the tag rounded up
    /// to the largest variant alignment: the `u8` payload sits at 8 next to
    /// a `u64` one, so its C struct needs 7 bytes after the tag; the `u64`
    /// variant is already there on its own, the unit variant has no payload.
    #[test]
    fn a_small_payload_is_padded_to_the_largest_variant_alignment() {
        let ir = CodegenIR::new();
        let variants = vec![
            variant("Empty", None),
            variant("Small", Some("u8")),
            variant("Wide", Some("u64")),
        ];
        let u = union_layout(&mono_union_shape(Some("C, u8"), &variants), &ir, 0)
            .expect("layout");
        assert_eq!(u.payload_offset, 8);
        assert_eq!(
            (u.padding("Empty"), u.padding("Small"), u.padding("Wide")),
            (0, 7, 0)
        );
        assert_eq!(u.padded_variants().collect::<Vec<_>>(), vec![("Small", 7)]);
        assert_eq!((u.abi.size, u.abi.align), (16, 8), "Rust: 8 + 8, 8-aligned");
    }

    /// A `repr(C)` union's tag is the int-sized C enum: a `u16` payload next
    /// to a `u64` one sits at 8, 4 bytes after the tag, not at 4.
    #[test]
    fn an_int_tag_pads_to_the_largest_variant_alignment_too() {
        let ir = CodegenIR::new();
        let variants = vec![variant("Short", Some("u16")), variant("Wide", Some("f64"))];
        let u = union_layout(&mono_union_shape(Some("C"), &variants), &ir, 0).expect("layout");
        assert_eq!((u.tag.size, u.payload_offset), (4, 8));
        assert_eq!((u.padding("Short"), u.padding("Wide")), (4, 0));
        assert_eq!((u.abi.size, u.abi.align), (16, 8));
    }

    /// When no variant is more aligned than the tag allows, nothing moves.
    #[test]
    fn a_union_of_one_alignment_needs_no_padding() {
        let ir = CodegenIR::new();
        let variants = vec![variant("A", Some("u32")), variant("B", Some("f32"))];
        let u = union_layout(&mono_union_shape(Some("C, u8"), &variants), &ir, 0).expect("layout");
        assert_eq!(u.payload_offset, 4);
        assert_eq!(u.padded_variants().count(), 0);
        assert_eq!((u.abi.size, u.abi.align), (8, 4));
    }
}
