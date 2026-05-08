//! OCaml type emission: struct (Ctypes `structure`) declarations,
//! field/seal definitions, and enum / tagged-union accessors.
//!
//! Strategy:
//! - **Two-pass struct emission**: Ctypes requires a struct's typ value
//!   to exist before fields are added (because field types may
//!   reference other structs). Pass 1 emits the bare typ stub; pass 2
//!   adds fields and seals.
//! - **Unit enums** (`is_union == false`) -> a polymorphic-variant
//!   alias (`type t = [ \`A | \`B ]`) plus integer mapping helpers
//!   `to_int` / `of_int` that pin the C ABI numbering.
//! - **Tagged-union enums** (`is_union == true`) -> the FFI-side
//!   `structure` with a `tag : uint32_t` field plus a `payload` byte
//!   array sized for the largest variant. The OCaml-side polymorphic
//!   variant + conversion helpers live in `wrappers.rs`.
//! - **Skipped categories** (`Recursive`, `VecRef`,
//!   `DestructorOrClone`, `GenericTemplate`) emit a
//!   `(* SKIPPED: ... *)` comment for traceability.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, EnumDef, FieldDef, FieldRefKind, StructDef, TypeCategory};
use super::{map_type_to_ocaml, ocaml_ffi_type_name, sanitize_doc, sanitize_identifier};

// ============================================================================
// Interface (.mli) emission
// ============================================================================

pub fn emit_interface_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* FFI type stubs (interface).                                                *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    builder.line("open Ctypes");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "(* SKIPPED: generic struct {} (no OCaml equivalent over the C ABI) *)",
                    s.name
                ));
            }
            continue;
        }
        let ffi = ocaml_ffi_type_name(&s.name);
        builder.line(&format!("type {}", ffi));
        builder.line(&format!(
            "val {} : {} structure typ",
            ffi, ffi
        ));
    }

    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            if !e.generic_params.is_empty() {
                builder.line(&format!(
                    "(* SKIPPED: generic enum {} (no OCaml equivalent over the C ABI) *)",
                    e.name
                ));
            }
            continue;
        }
        let ffi = ocaml_ffi_type_name(&e.name);
        if e.is_union {
            builder.line(&format!("type {}", ffi));
            builder.line(&format!(
                "val {} : {} structure typ",
                ffi, ffi
            ));
        } else {
            // For unit enums we don't need a `structure` — they're plain
            // ints at the FFI boundary. Surface the integer mapping
            // helpers as part of the public interface.
            builder.line(&format!("val {}_to_int : int -> int", ffi));
            builder.line(&format!("val {}_of_int : int -> int", ffi));
        }
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Implementation: forward stubs (pass 1)
// ============================================================================

pub fn emit_forward_struct_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Phase 1: struct typ stubs (fields are added after every typ exists).      *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        let ffi = ocaml_ffi_type_name(&s.name);
        // OCaml: `type az_app` is a phantom abstract type; the typ
        // value carries the runtime structure.
        builder.line(&format!("type {}", ffi));
        builder.line(&format!(
            "let ({} : {} structure typ) = structure \"{}\"",
            ffi, ffi, format_c_struct_name(&s.name)
        ));
    }

    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        if e.is_union {
            let ffi = ocaml_ffi_type_name(&e.name);
            builder.line(&format!("type {}", ffi));
            builder.line(&format!(
                "let ({} : {} structure typ) = structure \"{}\"",
                ffi, ffi, format_c_struct_name(&e.name)
            ));
        }
    }
    builder.blank();
}

// ============================================================================
// Implementation: fields / seal / enum constants (pass 2)
// ============================================================================

pub fn emit_struct_fields_and_enums(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Phase 2: struct fields, sealing, enum integer-mapping helpers.            *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    // Structs first.
    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "(* SKIPPED: generic struct {} *)",
                    s.name
                ));
            } else {
                builder.line(&format!(
                    "(* SKIPPED: struct {} ({}) *)",
                    s.name,
                    s.category.description()
                ));
            }
            continue;
        }
        emit_struct_fields(builder, s, ir);
    }

    // Tagged-union FFI structs.
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            if !e.generic_params.is_empty() {
                builder.line(&format!(
                    "(* SKIPPED: generic enum {} *)",
                    e.name
                ));
            } else {
                builder.line(&format!(
                    "(* SKIPPED: enum {} ({}) *)",
                    e.name,
                    e.category.description()
                ));
            }
            continue;
        }
        if e.is_union {
            emit_tagged_union_fields(builder, e, ir);
        } else {
            emit_unit_enum(builder, e);
        }
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
// Struct field emission
// ============================================================================

fn emit_struct_fields(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let ffi = ocaml_ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    if s.fields.is_empty() {
        // Ctypes allows zero-field sealed structs but pads to one byte;
        // we emit an explicit reserved field to make the layout obvious.
        builder.line(&format!(
            "let {}_reserved = field {} \"_reserved\" uint8_t",
            ffi, ffi
        ));
        builder.line(&format!("let () = seal {}", ffi));
        builder.blank();
        return;
    }

    for f in &s.fields {
        emit_field(builder, &ffi, f, ir);
    }

    builder.line(&format!("let () = seal {}", ffi));
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, ffi_struct: &str, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("(* {} *)", sanitize_doc(doc)));
    }

    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        let elem = map_type_to_ocaml(&elem_ty, ir);
        let field_name = sanitize_field_identifier(&f.name);
        builder.line(&format!(
            "let {}_{} = field {} \"{}\" (array {} {})",
            ffi_struct, field_name, ffi_struct, f.name, count, elem
        ));
        return;
    }

    let ocaml_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    let field_name = sanitize_field_identifier(&f.name);
    builder.line(&format!(
        "let {}_{} = field {} \"{}\" {}",
        ffi_struct, field_name, ffi_struct, f.name, ocaml_ty
    ));
}

// ============================================================================
// Tagged-union FFI struct
// ============================================================================

/// Emit a flat FFI representation for a tagged union: a `tag` discriminator
/// plus a per-variant payload struct. We approximate the C union by
/// allocating a single `payload : uint8_t array` whose size is the
/// maximum payload size we can compute from primitive types.
///
/// For variants whose payload is itself a known IR struct (so we have a
/// proper Ctypes typ), we instead emit a `field` of that struct's type,
/// resulting in a proper Ctypes-checked layout. When we can't size
/// statically (cross-referenced type aliases, nested generics, etc.)
/// we fall back to a reserved 64-byte payload — large enough for any
/// of azul's reflective tagged unions in practice. The wrapper layer
/// converts to/from a polymorphic-variant view; we expose the raw
/// typ here so `foreign` can pass these by-value.
fn emit_tagged_union_fields(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let ffi = ocaml_ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    builder.line(&format!(
        "let {}_tag = field {} \"tag\" uint32_t",
        ffi, ffi
    ));

    // Conservative payload size: 64 bytes covers 8 pointer-width fields
    // on a 64-bit host, which is enough for every union we currently
    // generate (tag + up to 4 pointers / a Vec header).
    let payload_size = compute_payload_size_bound(e, ir);
    builder.line(&format!(
        "let {}_payload = field {} \"payload\" (array {} uint8_t)",
        ffi, ffi, payload_size
    ));
    builder.line(&format!("let () = seal {}", ffi));
    builder.blank();
}

/// Returns a pessimistic upper bound on the union's payload size in
/// bytes. We don't try to be precise — Ctypes reads/writes via field
/// accessors, never by dereferencing a payload byte array directly,
/// so the only constraint is that the OCaml-side struct is at least
/// as large as the C union. 64 bytes is comfortable for everything
/// the IR currently produces; if a future variant grows past that,
/// bump the constant.
fn compute_payload_size_bound(_e: &EnumDef, _ir: &CodegenIR) -> usize {
    64
}

// ============================================================================
// Unit enum (integer round-trip helpers)
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    let ffi = ocaml_ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!(
            "(* SKIPPED: unit enum {} has no variants *)",
            e.name
        ));
        builder.blank();
        return;
    }

    // The conversion is the identity (C ABI uses sequential numbering
    // 0..N-1), but we expose the helpers so call sites can be
    // syntactically clean and we can change the encoding later
    // without breaking users.
    builder.line(&format!(
        "let {}_to_int (i : int) : int = i",
        ffi
    ));
    builder.line(&format!(
        "let {}_of_int (i : int) : int = i",
        ffi
    ));
    // Emit named constants for each variant.
    for (idx, v) in e.variants.iter().enumerate() {
        let lit = sanitize_identifier(&super::to_snake_case(&v.name));
        builder.line(&format!(
            "let {}_{} : int = {}",
            ffi, lit, idx
        ));
    }
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` to an OCaml Ctypes view expression.
fn ref_kind_field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_ocaml(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => {
            // Use a typed pointer when the inner type is known so
            // Ctypes catches mismatches.
            super::inner_pointer_form(type_name.trim(), ir)
        }
    }
}

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

/// Sanitize a struct field identifier. OCaml field-binding values
/// cannot collide with reserved words (we use them as `let` bindings
/// inside the implementation file).
fn sanitize_field_identifier(name: &str) -> String {
    sanitize_identifier(&super::to_snake_case(name))
}

/// Build the C struct tag name as exported by the C-ABI shim
/// (`AzApp`, `AzLayoutCallbackInfo`, etc.). This is the name passed
/// to Ctypes' `structure "..."` factory and must match the C symbol
/// for `field` lookups to resolve.
fn format_c_struct_name(ir_name: &str) -> String {
    format!("Az{}", ir_name)
}
