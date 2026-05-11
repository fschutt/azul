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
//!   emit a derived type with a `tag :: integer(c_int)` discriminant
//!   plus a single `payload :: type(c_ptr)` field that the user
//!   reinterprets manually with `c_f_pointer`. A SKIPPED comment
//!   block documents this; it's a known soft-spot of the binding.
//! - **Callback typedefs** become `abstract interface` blocks plus a
//!   `procedure(...), pointer :: AzFooCallbackType` alias. Fortran
//!   procedure pointers with `bind(C)` are exactly C function pointers.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** are
//!   skipped with `! SKIPPED: <reason>` comments.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    StructDef, TypeCategory,
};
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
    //    field declarations as `integer(c_int)` aliases.
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // 2. Tagged-union enums (no native union -> tag + payload pointer).
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            emit_tagged_union(builder, e, ir);
        }
    }

    // 3. POD derived types.
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        emit_struct(builder, s, ir);
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
// Tagged-union enum (Fortran has no union — emit tag + opaque payload)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, _ir: &CodegenIR) {
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
    builder.line(&format!("! Tagged-union {}: tag + opaque payload pointer.", ffi));
    builder.line("! Fortran has no native `union`; users cast `payload` via");
    builder.line("! `c_f_pointer(self%payload, ...)` to the variant-specific type.");
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

    // Derived type: tag + payload pointer.
    builder.line(&format!("type, bind(C) :: {}", truncate_identifier(&ffi)));
    builder.indent();
    builder.line("integer(c_int) :: tag");
    builder.line("type(c_ptr) :: payload");
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

    let arg_names: Vec<String> = cb
        .args
        .iter()
        .map(|a| sanitize_identifier(&a.name))
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
    for arg in &cb.args {
        let f_ty = match arg.ref_kind {
            ArgRefKind::Owned => map_type_to_fortran(&arg.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "type(c_ptr)".to_string()
            }
        };
        let nm = sanitize_identifier(&arg.name);
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
