//! Fortran derived-type / enum / tagged-union emission.
//!
//! Strategy:
//!
//! - **POD structs** map to `type, bind(C) :: AzFoo ... end type AzFoo`.
//!   Fortran `bind(C)` derived types have C-compatible memory layout
//!   (matches Rust's `#[repr(C)]`), so values can flow across the FFI
//!   boundary by value.
//! - **Unit-only enums** become a F2008 `enum, bind(C)` block (which
//!   has fixed underlying integer kind compatible with C `int`) plus
//!   a public `integer(c_int)` named alias so users can declare
//!   `integer(c_int) :: my_button = AzButtonType_Primary`.
//! - **Tagged-union enums** have no native equivalent in Fortran; we
//!   emit a derived type holding an ABI-opaque blob with the EXACT
//!   size and alignment of the C `repr(C,u8)` union (computed by
//!   `super::layout`). Anything else corrupts every struct that embeds
//!   a union by value — see the 2026-07 e2e SIGSEGV post-mortem. The
//!   `_TAG_*` enumerator constants are still emitted for reference.
//! - **Callback typedefs** become `abstract interface` blocks plus a
//!   `procedure(...), pointer :: AzFooCallbackType` alias. Fortran
//!   procedure pointers with `bind(C)` are exactly C function pointers.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** types
//!   are emitted as ABI-opaque blob stand-ins when their layout is
//!   computable (they ARE embedded by value — every `AzXVec` carries an
//!   `AzXVecDestructor`), else skipped with a `! SKIPPED:` comment.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, StructDef, TypeAliasDef, TypeCategory,
};
use super::layout::{blob_field_decl, mono_layout, type_layout};
use super::{
    ffi_type_name, map_type_to_fortran, sanitize_comment_line, sanitize_identifier,
    truncate_identifier,
};

// ============================================================================
// Top-level type-block emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Type definitions: derived types, enums, tagged-union approximations.");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    // 1. Unit (simple) enums first so they may appear in derived-type
    //    field declarations as `integer(c_int)` aliases. Skipped-category
    //    tagged unions (DestructorOrClone etc.) are embedded BY VALUE in
    //    regular structs (every AzXVec carries an AzXVecDestructor), so
    //    they get an ABI-opaque blob stand-in here — mapping them to
    //    `type(c_ptr)` shrank every embedding struct and corrupted all
    //    by-value FFI calls (2026-07 Fortran e2e SIGSEGV root cause).
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            if e.is_union && e.generic_params.is_empty() {
                if let Some(l) = type_layout(&e.name, ir) {
                    emit_opaque_blob(builder, &e.name, l, e.category.description());
                    continue;
                }
            }
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // 2 + 3 + 3b. Interleave tagged-union enums, POD structs, AND
    // monomorphized type-alias instantiations in topological order.
    // Monomorphized aliases (PhysicalSizeU32 etc.) are referenced by
    // regular structs (AzTexture has a `size: AzPhysicalSizeU32`
    // field), so they must land at the right sort_order rather than
    // at the end. Same pattern as lang_pascal/types.rs.
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
        Mono(&'a TypeAliasDef, &'a MonomorphizedTypeDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            // Same ABI rule as skipped unions above: if the skipped
            // struct is layout-computable, other structs may embed it by
            // value (e.g. AzXmlNodeChild inside AzXmlNode), so emit an
            // exact-size blob stand-in instead of collapsing to c_ptr.
            if s.generic_params.is_empty() {
                if let Some(l) = type_layout(&s.name, ir) {
                    emit_opaque_blob(builder, &s.name, l, s.category.description());
                    continue;
                }
            }
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

    // 4. Callback (procedural) typedefs.
    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, ir);
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

pub(crate) fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
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

pub(crate) fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
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
        "! SKIPPED: struct {} ({})",
        s.name,
        s.category.description()
    ));
}

/// Emit an ABI-opaque stand-in type: a single array component of the
/// widest integer kind matching the C alignment, sized to the exact C
/// byte size. Field-level access is impossible (Fortran has no unions),
/// but embedding-by-value and pass-by-value are layout-exact — which is
/// all the generated wrappers ever need for these types.
fn emit_opaque_blob(
    builder: &mut CodeBuilder,
    name: &str,
    l: super::layout::AbiLayout,
    why: &str,
) {
    let ffi = truncate_identifier(&ffi_type_name(name));
    builder.line(&format!(
        "! ABI-opaque stand-in for {} ({}): exact C size/alignment ({} bytes, align {}).",
        name, why, l.size, l.align
    ));
    builder.line(&format!("type, bind(C) :: {}", ffi));
    builder.indent();
    builder.line(&blob_field_decl(l));
    builder.dedent();
    builder.line(&format!("end type {}", ffi));
    builder.line(&format!("public :: {}", ffi));
    builder.blank();
}

fn emit_skipped_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    builder.line(&format!(
        "! SKIPPED: enum {} ({})",
        e.name,
        e.category.description()
    ));
}

// ============================================================================
// Unit-only enum (F2008 `enum, bind(C)` block + integer alias)
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }

    let alias = ffi_type_name(&e.name);

    if e.variants.is_empty() {
        // Empty enums are illegal in F2008 `enum, bind(C)`; emit just
        // the integer alias as a degenerate type.
        builder.line(&format!(
            "! NOTE: enum {} has no variants; emitting integer alias only.",
            e.name
        ));
        builder.line(&format!(
            "integer, parameter :: {} = c_int  ! kind alias",
            truncate_identifier(&alias)
        ));
        builder.line(&format!("public :: {}", truncate_identifier(&alias)));
        builder.blank();
        return;
    }

    // F2008 enum block.
    builder.line("enum, bind(C)");
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        let variant_name =
            truncate_identifier(&format!("{}_{}", alias, sanitize_identifier(&v.name)));
        builder.line(&format!("enumerator :: {} = {}", variant_name, i));
    }
    builder.dedent();
    builder.line("end enum");

    // The enumerator names above are usable directly. Also expose them
    // as PUBLIC so consumers of the module can `use azul, only: AzFoo_Bar`.
    for v in &e.variants {
        let variant_name =
            truncate_identifier(&format!("{}_{}", alias, sanitize_identifier(&v.name)));
        builder.line(&format!("public :: {}", variant_name));
    }
    builder.blank();
}

// ============================================================================
// Tagged-union enum (Fortran has no union — emit exact-size ABI blob)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }

    let ffi = ffi_type_name(&e.name);
    // The tag-enum prefix must not collide with a sibling unit enum
    // that happens to share the base name with `Tag` appended — e.g.
    // `NodeType` is a tagged union and `NodeTypeTag` is a separate
    // unit enum, both naturally landing at `AzNodeTypeTag_*` prefix.
    // Using `_TAG_` makes the tagged-union variants disambiguated.
    let tag_alias = format!("{}_TAG", ffi);

    // Tag enum block.
    builder.line(&format!(
        "! Tagged-union {}: ABI-opaque blob (Fortran has no native union).",
        ffi
    ));
    builder.line("! Construct/inspect values via the C-API helper functions only.");
    builder.line("enum, bind(C)");
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        let variant_name =
            truncate_identifier(&format!("{}_{}", tag_alias, sanitize_identifier(&v.name)));
        builder.line(&format!("enumerator :: {} = {}", variant_name, i));
    }
    builder.dedent();
    builder.line("end enum");
    for v in &e.variants {
        let variant_name =
            truncate_identifier(&format!("{}_{}", tag_alias, sanitize_identifier(&v.name)));
        builder.line(&format!("public :: {}", variant_name));
    }
    builder.blank();

    // Document each variant's payload shape so users know how to cast.
    for v in &e.variants {
        let lbl = sanitize_identifier(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("!   variant {}: no payload", lbl));
            }
            EnumVariantKind::Tuple(types) => {
                if types.is_empty() {
                    builder.line(&format!("!   variant {}: no payload", lbl));
                } else if types.len() == 1 {
                    builder.line(&format!(
                        "!   variant {}: payload is single value of type `{}`",
                        lbl, types[0].0
                    ));
                } else {
                    let parts: Vec<String> = types.iter().map(|(t, _)| t.clone()).collect();
                    builder.line(&format!(
                        "!   variant {}: payload is tuple ({})",
                        lbl,
                        parts.join(", ")
                    ));
                }
            }
            EnumVariantKind::Struct(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.type_name))
                    .collect();
                builder.line(&format!(
                    "!   variant {}: payload is struct {{ {} }}",
                    lbl,
                    parts.join(", ")
                ));
            }
        }
    }

    // Derived type: ABI-opaque blob with the exact C size/alignment of
    // the repr(C,u8) union. The old `{integer(c_int) tag; type(c_ptr)
    // payload}` shape (16 bytes) mis-sized nearly every union (AzString
    // 32 vs 40, AzWindowCreateOptions 824 vs 1336, ...) and stack-smashed
    // every by-value FFI call — the 2026-07 Fortran e2e SIGSEGV.
    builder.line(&format!("type, bind(C) :: {}", truncate_identifier(&ffi)));
    builder.indent();
    match type_layout(&e.name, ir) {
        Some(l) => builder.line(&blob_field_decl(l)),
        None => {
            // Layout not computable (should not happen for non-generic
            // unions) — keep the legacy shape so the module still
            // compiles, and say so loudly.
            builder.line("! WARNING: union layout not computable; legacy tag+ptr shape");
            builder.line("integer(c_int) :: tag");
            builder.line("type(c_ptr) :: payload");
        }
    }
    builder.dedent();
    builder.line(&format!("end type {}", truncate_identifier(&ffi)));
    builder.line(&format!("public :: {}", truncate_identifier(&ffi)));
    builder.blank();
}

// ============================================================================
// POD derived type
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }

    let ffi = truncate_identifier(&ffi_type_name(&s.name));

    if s.fields.is_empty() {
        // F2003 `bind(C)` derived types may not be empty; emit a
        // single dummy field so the type is well-formed but obviously
        // opaque. Layout matches a 1-byte C struct (which Rust does
        // not actually emit for ZSTs — but this surfaces nowhere on
        // the FFI boundary in practice).
        builder.line(&format!("type, bind(C) :: {}", ffi));
        builder.indent();
        builder.line("! opaque - no fields exposed via FFI");
        builder.line("integer(c_int8_t) :: opaque_padding_ = 0_c_int8_t");
        builder.dedent();
        builder.line(&format!("end type {}", ffi));
        builder.line(&format!("public :: {}", ffi));
        builder.blank();
        return;
    }

    builder.line(&format!("type, bind(C) :: {}", ffi));
    builder.indent();

    for f in &s.fields {
        emit_field(builder, f, ir);
    }

    builder.dedent();
    builder.line(&format!("end type {}", ffi));
    builder.line(&format!("public :: {}", ffi));
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("! {}", sanitize_comment_line(doc)));
    }
    let f_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    let nm = sanitize_identifier(&f.name);
    builder.line(&format!("{} :: {}", f_ty, nm));
}

// ============================================================================
// Callback typedef
// ============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }

    let alias_ifname = truncate_identifier(&format!("{}_iface", ffi_type_name(&cb.name)));
    let alias_ptr = truncate_identifier(&ffi_type_name(&cb.name));

    builder.line("abstract interface");
    builder.indent();

    // Synthesize arg names when the IR leaves them empty (the api
    // sometimes elides parameter names for callback typedefs). Without
    // synthetic names the emitted signature `function foo(, )` parses
    // as a malformed argument list.
    let arg_names: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                sanitize_identifier(&a.name)
            }
        })
        .collect();

    let header = if cb.return_type.is_some() {
        format!(
            "function {}({}) bind(C) result(r)",
            alias_ifname,
            arg_names.join(", ")
        )
    } else {
        format!("subroutine {}({}) bind(C)", alias_ifname, arg_names.join(", "))
    };
    builder.line(&header);
    builder.indent();
    builder.line("import");
    for (i, arg) in cb.args.iter().enumerate() {
        let f_ty = match arg.ref_kind {
            ArgRefKind::Owned => map_type_to_fortran(&arg.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "type(c_ptr)".to_string()
            }
        };
        let nm = if arg.name.is_empty() {
            format!("arg{}", i)
        } else {
            sanitize_identifier(&arg.name)
        };
        // Pointers are passed by VALUE (the address itself is the value).
        // C primitives are passed by VALUE. Derived types pass by VALUE
        // since `bind(C)` records mirror Rust `extern "C"` ABI.
        builder.line(&format!("{}, value :: {}", f_ty, nm));
    }
    if let Some(ret) = &cb.return_type {
        let ret_ty = map_type_to_fortran(ret, ir);
        builder.line(&format!("{} :: r", ret_ty));
        builder.dedent();
        builder.line(&format!("end function {}", alias_ifname));
    } else {
        builder.dedent();
        builder.line(&format!("end subroutine {}", alias_ifname));
    }

    builder.dedent();
    builder.line("end interface");

    // Procedure pointer alias: `procedure(<iface>), pointer :: AzFooCallbackType`
    // Users assign C function pointers to this via `c_f_procpointer`.
    builder.line(&format!(
        "procedure({}), pointer :: {}_default => null()",
        alias_ifname, alias_ptr
    ));
    builder.line(&format!("public :: {}_default", alias_ptr));
    builder.blank();
}

// ============================================================================
// Monomorphized type-alias emission (generic instantiations)
// ============================================================================

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    if !ta.doc.is_empty() {
        for d in &ta.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }
    let ffi = truncate_identifier(&ffi_type_name(&ta.name));
    match &mono.kind {
        // Simple enum monomorphizations map to integer constants —
        // mirrors `emit_unit_enum` for IR enums. Emit an `enum,
        // bind(C)` block plus the type name as an integer alias.
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            builder.line("enum, bind(C)");
            builder.indent();
            for (i, v) in variants.iter().enumerate() {
                let vname =
                    truncate_identifier(&format!("{}_{}", ffi, sanitize_identifier(v)));
                builder.line(&format!("enumerator :: {} = {}", vname, i));
            }
            builder.dedent();
            builder.line("end enum");
            for v in variants {
                let vname =
                    truncate_identifier(&format!("{}_{}", ffi, sanitize_identifier(v)));
                builder.line(&format!("public :: {}", vname));
            }
            builder.blank();
        }

        // Struct monomorphizations get a regular `type, bind(C)` block.
        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
                builder.line(&format!("type, bind(C) :: {}", ffi));
                builder.indent();
                builder.line("integer(c_int) :: opaque_dummy_");
                builder.dedent();
                builder.line(&format!("end type {}", ffi));
                builder.line(&format!("public :: {}", ffi));
                builder.blank();
                return;
            }
            builder.line(&format!("type, bind(C) :: {}", ffi));
            builder.indent();
            for f in fields {
                let ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                let nm = sanitize_identifier(&f.name);
                builder.line(&format!("{} :: {}", ty, nm));
            }
            builder.dedent();
            builder.line(&format!("end type {}", ffi));
            builder.line(&format!("public :: {}", ffi));
            builder.blank();
        }

        // Tagged-union monomorphizations follow the same shape as
        // `emit_tagged_union`: tag enum constants + a derived type holding
        // an ABI-opaque blob of the exact C union size/alignment.
        MonomorphizedKind::TaggedUnion { variants, .. } => {
            let tag_alias = format!("{}_TAG", ffi);
            builder.line(&format!("! Monomorphized tagged-union {}", ffi));
            builder.line("enum, bind(C)");
            builder.indent();
            for (i, v) in variants.iter().enumerate() {
                let vname = truncate_identifier(&format!(
                    "{}_{}",
                    tag_alias,
                    sanitize_identifier(&v.name)
                ));
                builder.line(&format!("enumerator :: {} = {}", vname, i));
            }
            builder.dedent();
            builder.line("end enum");
            for v in variants {
                let vname = truncate_identifier(&format!(
                    "{}_{}",
                    tag_alias,
                    sanitize_identifier(&v.name)
                ));
                builder.line(&format!("public :: {}", vname));
            }
            // ABI-opaque blob body — same rationale as emit_tagged_union.
            builder.line(&format!("type, bind(C) :: {}", ffi));
            builder.indent();
            match mono_layout(mono, ir, 0) {
                Some(l) => builder.line(&blob_field_decl(l)),
                None => {
                    builder.line("! WARNING: union layout not computable; legacy tag+ptr shape");
                    builder.line("integer(c_int) :: tag");
                    builder.line("type(c_ptr) :: payload");
                }
            }
            builder.dedent();
            builder.line(&format!("end type {}", ffi));
            builder.line(&format!("public :: {}", ffi));
            builder.blank();
        }
    }
}

// ============================================================================
// Field/argument type helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the Fortran field type
/// string. Pointer/reference kinds collapse to `type(c_ptr)`.
pub(crate) fn field_type_for_ref_kind(
    type_name: &str,
    ref_kind: &FieldRefKind,
    ir: &CodegenIR,
) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_fortran(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "type(c_ptr)".to_string(),
    }
}
