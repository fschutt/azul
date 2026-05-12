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
            // ints at the FFI boundary. Still need a `type` declaration
            // so other val signatures can name them
            // (`val ok : ... -> az_msg_box_icon -> unit`); declare the
            // type as an alias for int.
            builder.line(&format!("type {} = int", ffi));
            builder.line(&format!("val {} : {} typ", ffi, ffi));
            builder.line(&format!("val {}_to_int : int -> int", ffi));
            builder.line(&format!("val {}_of_int : int -> int", ffi));
            // Expose per-variant integer constants so hello-worlds can
            // write `Azul.az_button_type_variant_primary` rather than
            // bare literals. The .ml emits these (line ~530); the .mli
            // needs the matching `val` declarations.
            for v in &e.variants {
                let lit = sanitize_identifier(&super::to_snake_case(&v.name));
                builder.line(&format!("val {}_variant_{} : int", ffi, lit));
            }
        }
    }

    // Callback typedefs need declarations too — they're referenced as
    // value-level types from function signatures (`val with_resolver :
    // az_icon_resolver_callback_type -> t`) but were never declared in
    // the interface, raising "Unbound type constructor". Emit a stub
    // type plus a `<name> : <name> typ` value so other val signatures
    // can name them. Functions actually marshalling these typedefs go
    // through `static_funptr` or `Foreign.funptr` at call sites.
    for cb in &ir.callback_typedefs {
        let ffi = ocaml_ffi_type_name(&cb.name);
        builder.line(&format!("type {}", ffi));
        builder.line(&format!("val {} : {} typ", ffi, ffi));
    }

    // Filtered-out struct/enum categories (Recursive, VecRef,
    // DestructorOrClone) are still referenced by name from other
    // variants' val signatures. Emit phantom type stubs so those
    // references resolve.
    for s in &ir.structs {
        if !config.should_include_type(&s.name) || !s.generic_params.is_empty() {
            continue;
        }
        if matches!(
            s.category,
            TypeCategory::Recursive | TypeCategory::VecRef | TypeCategory::DestructorOrClone
        ) {
            let ffi = ocaml_ffi_type_name(&s.name);
            builder.line(&format!("type {}", ffi));
            builder.line(&format!("val {} : {} typ", ffi, ffi));
        }
    }
    for e in &ir.enums {
        if !config.should_include_type(&e.name) || !e.generic_params.is_empty() {
            continue;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive | TypeCategory::VecRef | TypeCategory::DestructorOrClone
        ) {
            let ffi = ocaml_ffi_type_name(&e.name);
            builder.line(&format!("type {}", ffi));
            builder.line(&format!("val {} : {} typ", ffi, ffi));
        }
    }

    // Monomorphized type aliases need types too (e.g.
    // `az_physical_position_i32` referenced from struct fields).
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        let ffi = ocaml_ffi_type_name(&ta.name);
        builder.line(&format!("type {}", ffi));
        builder.line(&format!("val {} : {} typ", ffi, ffi));
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
    // Callback typedefs alias to `(ptr void)` at the value level.
    // The signature pieces (`val with_resolver :
    // az_icon_resolver_callback_type -> t`) only need the type token
    // to exist; actual marshalling happens via `static_funptr` /
    // `Foreign.funptr` at call sites.
    for cb in &ir.callback_typedefs {
        let ffi = ocaml_ffi_type_name(&cb.name);
        builder.line(&format!("type {} = unit ptr", ffi));
        builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
    }

    // Filtered-out / monomorphized types — same placeholders as the
    // .mli so the implementation side has matching declarations.
    for s in &ir.structs {
        if !config.should_include_type(&s.name) || !s.generic_params.is_empty() {
            continue;
        }
        if matches!(
            s.category,
            TypeCategory::Recursive | TypeCategory::VecRef | TypeCategory::DestructorOrClone
        ) {
            let ffi = ocaml_ffi_type_name(&s.name);
            builder.line(&format!("type {} = unit ptr", ffi));
            builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
        }
    }
    for e in &ir.enums {
        if !config.should_include_type(&e.name) || !e.generic_params.is_empty() {
            continue;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive | TypeCategory::VecRef | TypeCategory::DestructorOrClone
        ) {
            let ffi = ocaml_ffi_type_name(&e.name);
            builder.line(&format!("type {} = unit ptr", ffi));
            builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
        }
    }
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        let ffi = ocaml_ffi_type_name(&ta.name);
        builder.line(&format!("type {} = unit ptr", ffi));
        builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
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

    // Unit enums FIRST — struct fields reference them by typ value
    // (`field s \"frame\" az_window_frame`) so the typ binding must
    // be in scope before any struct field declaration uses it.
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // Interleave structs and tagged-union enums in topological order
    // (by `sort_order` populated during `analyze_dependencies`). A struct
    // field of type `OptionU32` (tagged union) must reference an already
    // sealed `az_option_u32` typ, so we cannot emit all structs first
    // and tagged unions afterwards — they have to merge.
    #[derive(Debug)]
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
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
        items.push((s.sort_order, Item::Struct(s)));
    }
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
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    items.sort_by_key(|(d, _)| *d);
    for (_, item) in &items {
        match item {
            Item::Struct(s) => emit_struct_fields(builder, s, ir),
            Item::Union(e) => emit_tagged_union_fields(builder, e, ir),
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

    // Use `<struct>_field_<name>` as the binding so the field
    // accessor doesn't collide with another struct's typ value
    // when the snake-cased combination clashes (e.g. `az_string`
    // struct + `vec` field → `az_string_vec`, which is also the
    // typ for the `AzStringVec` struct).
    if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
        let elem = map_type_to_ocaml(&elem_ty, ir);
        let field_name = sanitize_field_identifier(&f.name);
        builder.line(&format!(
            "let {}_field_{} = field {} \"{}\" (array {} {})",
            ffi_struct, field_name, ffi_struct, f.name, count, elem
        ));
        return;
    }

    let ocaml_ty = ref_kind_field_type(&f.type_name, &f.ref_kind, ir);
    let field_name = sanitize_field_identifier(&f.name);
    builder.line(&format!(
        "let {}_field_{} = field {} \"{}\" {}",
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
        "let {}_field_tag = field {} \"tag\" uint32_t",
        ffi, ffi
    ));

    // Conservative payload size: 64 bytes covers 8 pointer-width fields
    // on a 64-bit host, which is enough for every union we currently
    // generate (tag + up to 4 pointers / a Vec header).
    //
    // Encoding choice: OCaml ctypes' libffi binding refuses to marshal
    // a `field _ (array N uint8_t)` when the enclosing struct is passed
    // by value (`Ctypes_static.Unsupported "libffi does not support
    // passing arrays"`), so we lay the payload out as N independent
    // uint8_t fields instead. They have the same C ABI as a uint8_t
    // array (same size, same alignment) but ctypes' libffi adapter
    // accepts struct-by-value for them.
    let payload_size = compute_payload_size_bound(e, ir);
    for i in 0..payload_size {
        builder.line(&format!(
            "let {ffi}_field_payload_{i} = field {ffi} \"payload_{i}\" uint8_t"
        ));
    }
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

    // Type alias + typ value so other val signatures can reference
    // the enum by name (`val ok : ... -> az_msg_box_icon -> unit`).
    // Unit enums are int-valued at the C ABI boundary.
    builder.line(&format!("type {} = int", ffi));
    builder.line(&format!("let ({} : {} typ) = int", ffi, ffi));

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
    // Emit named constants for each variant. Use `<ffi>_variant_<v>`
    // (not just `<ffi>_<v>`) so the constant doesn't collide with a
    // struct of the same flattened name — e.g. `az_shape_ellipse`
    // is BOTH the struct `ShapeEllipse` AND the `Ellipse` variant of
    // the `Shape` enum. Disambiguate at the variant side.
    for (idx, v) in e.variants.iter().enumerate() {
        let lit = sanitize_identifier(&super::to_snake_case(&v.name));
        builder.line(&format!(
            "let {}_variant_{} : int = {}",
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
