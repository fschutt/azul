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
            // I.5.6 (OCaml): expose the Option/Result payload extractor
            // signatures so users can call `Azul.az_option_dom_intoSome opt`
            // from outside the umbrella module. The .ml emits the bodies
            // (`emit_tagged_union_storage_decl`); without matching val
            // declarations here they're confined to umbrella-internal use.
            emit_into_signature_if_option_or_result(builder, e, ir);
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
            // Match the implementation: tagged-union DestructorOrClone
            // enums get a structure typ (16 bytes); others get a
            // void-pointer placeholder.
            if e.is_union && matches!(e.category, TypeCategory::DestructorOrClone) {
                builder.line(&format!("type {}", ffi));
                builder.line(&format!("val {} : {} structure typ", ffi, ffi));
            } else {
                builder.line(&format!("type {}", ffi));
                builder.line(&format!("val {} : {} typ", ffi, ffi));
            }
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
            // DestructorOrClone tagged unions (e.g. AzU8VecDestructor:
            // `#[repr(C, u8)]` with `External(fn_ptr)` variant) are
            // **16 bytes** in Rust (u8 discriminator + 7 bytes padding
            // + 8-byte payload aligned). Mapping them to `unit ptr`
            // (8 bytes) shrinks every parent struct that embeds them
            // by 8 bytes per occurrence, corrupting field offsets
            // downstream — manifests as SIGABRT in `<U8Vec as Drop>::drop`
            // when libazul reads garbage U8Vec.ptr values from an
            // OCaml-side WCO. Emit a proper 16-byte Ctypes structure
            // instead. Non-union destructor categories (Recursive,
            // VecRef) keep the `unit ptr` shorthand — those are not
            // typically embedded by value in nested structs.
            if e.is_union && matches!(e.category, TypeCategory::DestructorOrClone) {
                // 16-byte struct: 8 bytes of tag+padding + 8-byte payload.
                // libffi doesn't accept array fields in struct
                // descriptors, so we use two uint64_t fields. The first
                // covers tag (u8 at offset 0) + 7 bytes of natural
                // alignment padding to the 8-byte boundary; the second
                // is the External-variant fn-pointer payload.
                builder.line(&format!("type {}", ffi));
                builder.line(&format!(
                    "let ({} : {} structure typ) = structure \"{}\"",
                    ffi, ffi, ffi
                ));
                builder.line(&format!(
                    "let _{}_tag_pad = field {} \"tag_and_pad\" uint64_t",
                    ffi, ffi
                ));
                builder.line(&format!(
                    "let _{}_payload = field {} \"payload\" uint64_t",
                    ffi, ffi
                ));
                builder.line(&format!("let () = seal {}", ffi));
            } else {
                builder.line(&format!("type {} = unit ptr", ffi));
                builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
            }
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

    // C ABI for `#[repr(C, u8)]` enum:
    //   { u8 tag; <pad to max-payload-alignment>; <max payload bytes>; <pad to overall alignment> }
    //
    // We need OCaml's Ctypes view of this struct to have BOTH the same
    // total size AND the same alignment as the C ABI, so that parent
    // structs that embed it compute identical field offsets on both
    // sides. Picking the right field-type granularity (uint8_t /
    // uint16_t / uint32_t / uint64_t) gives Ctypes' libffi descriptor
    // the right alignment without using `array N uint8_t` (rejected
    // for by-value struct marshalling).
    let (size, align) = c_size_of_tagged_enum(e, ir, &mut Vec::new());
    emit_byte_blob_fields(builder, &ffi, size, align);
    builder.line(&format!("let () = seal {}", ffi));

    // AzOption / AzResult tag-byte helpers. The variant tag is at
    // offset 0 of the blob (repr(C, u8)). Without per-variant typed
    // views (a bigger codegen rework) we can't extract the payload,
    // but we CAN expose `is_ok` / `is_err` (for Result) or
    // `is_some` / `is_none` (for Option) by reading byte 0 via
    // Ctypes.coerce. That's enough to write idiomatic `match` code:
    //     if Azul.is_result_ok r then ... else ...
    //
    // Variant ordering matters: AzOption has None first (tag 0) and
    // Some second (tag 1); AzResult has Ok first (tag 0) and Err
    // second (tag 1). The codegen finds the actual variant index
    // rather than assuming.
    if e.is_union {
        let some_or_ok_idx = e
            .variants
            .iter()
            .position(|v| v.name == "Some" || v.name == "Ok");
        let none_or_err_idx = e
            .variants
            .iter()
            .position(|v| v.name == "None" || v.name == "Err");
        let is_option_or_result = e.name.starts_with("Option")
            || e.name.starts_with("Result");
        if is_option_or_result && some_or_ok_idx.is_some() && none_or_err_idx.is_some() {
            let positive_idx = some_or_ok_idx.unwrap();
            let (positive_name, negative_name) = if e.name.starts_with("Option") {
                ("is_some", "is_none")
            } else {
                ("is_ok", "is_err")
            };
            builder.blank();
            builder.line(&format!(
                "(* Tag-byte accessors for {} (offset 0, repr(C,u8)). *)",
                ffi
            ));
            builder.line(&format!(
                "let {}_{} (r : {} Ctypes.structure) : bool =",
                ffi, positive_name, ffi
            ));
            builder.indent();
            builder.line("let raw_ptr = Ctypes.addr r in");
            builder.line(&format!(
                "let tag_ptr = Ctypes.coerce (Ctypes.ptr {}) (Ctypes.ptr Ctypes.uint8_t) raw_ptr in",
                ffi
            ));
            builder.line(&format!(
                "Unsigned.UInt8.to_int (Ctypes.(!@) tag_ptr) = {}",
                positive_idx
            ));
            builder.dedent();
            builder.line(&format!(
                "let {}_{} (r : {} Ctypes.structure) : bool = not ({}_{} r)",
                ffi, negative_name, ffi, ffi, positive_name
            ));

            // I.5.6 (OCaml): payload extractor. For repr(C, u8) tagged
            // unions the payload starts at offset `max 1 (align_of
            // payload)` — tag is the first byte; payload is laid out
            // at its own natural alignment immediately after. Coerce
            // the struct's raw byte pointer to that offset, then to a
            // typed payload pointer.
            //
            // Per the locked decision: no libazul-side
            // `AzOption<T>_intoSome` export is required — `Ctypes.alignment`
            // computed at runtime gives the right offset for any
            // payload type that's already sealed in the cdef block
            // (which by topological order is always true at the call
            // site). For struct payloads we return
            // `<payload_ffi> Ctypes.structure option`; the user wraps
            // manually via the per-class `Elem.make_<snake>` helper
            // if they want a managed handle.
            let positive_var = &e.variants[positive_idx];
            if let super::super::ir::EnumVariantKind::Tuple(types) = &positive_var.kind {
                if let Some((payload_ty, _)) = types.first() {
                    // Only emit for proper struct payloads. Skip
                    // VecRef / Boxed / Recursive / DestructorOrClone
                    // / GenericTemplate payloads — those types are
                    // either pointer-typedefs (no struct typ to
                    // coerce into) or codegen-internal scaffolding.
                    let payload_is_proper_struct =
                        ir.find_struct(payload_ty).map(|s| {
                            !matches!(
                                s.category,
                                super::super::ir::TypeCategory::VecRef
                                    | super::super::ir::TypeCategory::Boxed
                                    | super::super::ir::TypeCategory::Recursive
                                    | super::super::ir::TypeCategory::DestructorOrClone
                                    | super::super::ir::TypeCategory::GenericTemplate
                            )
                        }).unwrap_or(false);
                    if payload_is_proper_struct {
                        let payload_ffi = super::ocaml_ffi_type_name(payload_ty);
                        let into_name = if e.name.starts_with("Option") {
                            "intoSome".to_string()
                        } else if positive_var.name == "Ok" {
                            "intoOk".to_string()
                        } else {
                            "intoErr".to_string()
                        };
                        builder.line(&format!(
                            "let {}_{} (r : {} Ctypes.structure) : {} Ctypes.structure option =",
                            ffi, into_name, ffi, payload_ffi
                        ));
                        builder.indent();
                        builder.line(&format!(
                            "if not ({}_{} r) then None",
                            ffi, positive_name
                        ));
                        builder.line("else");
                        builder.indent();
                        builder.line("let raw_ptr = Ctypes.addr r in");
                        builder.line(&format!(
                            "let byte_ptr = Ctypes.coerce (Ctypes.ptr {}) (Ctypes.ptr Ctypes.char) raw_ptr in",
                            ffi
                        ));
                        // `max 1` guards primitive-aligned payloads
                        // (align_of u8 == 1 → offset 1 not 0).
                        builder.line(&format!(
                            "let payload_align = max 1 (Ctypes.alignment {}) in",
                            payload_ffi
                        ));
                        builder.line("let payload_byte_ptr = Ctypes.(+@) byte_ptr payload_align in");
                        builder.line(&format!(
                            "let payload_ptr = Ctypes.coerce (Ctypes.ptr Ctypes.char) (Ctypes.ptr {}) payload_byte_ptr in",
                            payload_ffi
                        ));
                        builder.line("Some (Ctypes.(!@) payload_ptr)");
                        builder.dedent();
                        builder.dedent();
                    }
                }
            }
        }
    }

    builder.blank();
}

/// Lay out a `size`-byte, `align`-aligned blob inside an OCaml Ctypes
/// `structure` definition as a sequence of fields whose composite
/// size and alignment match the C ABI exactly. The fields are named
/// `_<ffi>_blob_<i>` and aren't intended to be read by user code —
/// the wrappers use `_create` / `_match` helpers from libazul.
/// I.5.6 (OCaml) — `.mli` val signature for the Option/Result payload
/// extractors. Only emits when the .ml-side `emit_tagged_union_storage_decl`
/// would have emitted the matching `let` (same shape predicate).
fn emit_into_signature_if_option_or_result(
    builder: &mut CodeBuilder,
    e: &super::super::ir::EnumDef,
    ir: &CodegenIR,
) {
    if !e.is_union {
        return;
    }
    let is_option_or_result =
        e.name.starts_with("Option") || e.name.starts_with("Result");
    if !is_option_or_result {
        return;
    }
    let some_or_ok_idx = e
        .variants
        .iter()
        .position(|v| v.name == "Some" || v.name == "Ok");
    let none_or_err_idx = e
        .variants
        .iter()
        .position(|v| v.name == "None" || v.name == "Err");
    let (Some(positive_idx), Some(_)) = (some_or_ok_idx, none_or_err_idx) else {
        return;
    };
    let ffi = super::ocaml_ffi_type_name(&e.name);
    let positive_var = &e.variants[positive_idx];
    let super::super::ir::EnumVariantKind::Tuple(types) = &positive_var.kind else {
        return;
    };
    let Some((payload_ty, _)) = types.first() else {
        return;
    };
    let payload_is_proper_struct = ir
        .find_struct(payload_ty)
        .map(|s| {
            !matches!(
                s.category,
                super::super::ir::TypeCategory::VecRef
                    | super::super::ir::TypeCategory::Boxed
                    | super::super::ir::TypeCategory::Recursive
                    | super::super::ir::TypeCategory::DestructorOrClone
                    | super::super::ir::TypeCategory::GenericTemplate
            )
        })
        .unwrap_or(false);
    if !payload_is_proper_struct {
        return;
    }
    let payload_ffi = super::ocaml_ffi_type_name(payload_ty);
    let into_name = if e.name.starts_with("Option") {
        "intoSome"
    } else if positive_var.name == "Ok" {
        "intoOk"
    } else {
        "intoErr"
    };
    builder.line(&format!(
        "val {}_{} : {} structure -> {} structure option",
        ffi, into_name, ffi, payload_ffi
    ));
}

fn emit_byte_blob_fields(builder: &mut CodeBuilder, ffi: &str, size: usize, align: usize) {
    let align = align.max(1);
    let unit_size = match align {
        a if a >= 8 => 8,
        4 => 4,
        2 => 2,
        _ => 1,
    };
    let unit_typ = match unit_size {
        8 => "uint64_t",
        4 => "uint32_t",
        2 => "uint16_t",
        _ => "uint8_t",
    };
    let mut remaining = size;
    let mut idx = 0;
    while remaining >= unit_size {
        builder.line(&format!(
            "let _{ffi}_blob_{idx} = field {ffi} \"blob_{idx}\" {unit_typ}"
        ));
        remaining -= unit_size;
        idx += 1;
    }
    // Trailing partial word: emit per-byte fields, which still satisfy
    // alignment (uint8_t fields have alignment 1, and they come after
    // a sequence of `align`-aligned fields so the tail's offset is
    // already at the natural byte boundary).
    let mut tail_i = 0;
    while remaining > 0 {
        builder.line(&format!(
            "let _{ffi}_blob_tail_b{tail_i} = field {ffi} \"blob_tail_b{tail_i}\" uint8_t"
        ));
        remaining -= 1;
        tail_i += 1;
    }
    // If `size == 0` (unit-only enum), seal anyway by emitting a
    // single zero-cost marker so Ctypes has at least one field.
    if size == 0 {
        builder.line(&format!(
            "let _{ffi}_blob_unit = field {ffi} \"blob_unit\" uint8_t"
        ));
    }
}

/// Compute (size, alignment) in bytes for any IR type name as the C
/// ABI sees it on a 64-bit LP64 host. Recurses through structs and
/// tagged unions; primitive sizes are hard-coded. Cycles are broken by
/// returning a conservative `(8, 8)` for a re-entered type.
fn c_size_of_type(type_name: &str, ir: &CodegenIR, visiting: &mut Vec<String>) -> (usize, usize) {
    let trimmed = type_name.trim();

    // Pointer / reference forms — always 8/8 on 64-bit.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return (8, 8);
    }

    // Fixed-size array `[T; N]`.
    if let Some((elem, count)) = parse_array_type(trimmed) {
        let (es, ea) = c_size_of_type(&elem, ir, visiting);
        return (es * count, ea);
    }

    // Primitives.
    match trimmed {
        "bool" | "u8" | "i8" | "c_char" | "c_uchar" | "char" => return (1, 1),
        "u16" | "i16" => return (2, 2),
        "u32" | "i32" | "c_int" | "c_uint" | "f32" => return (4, 4),
        "u64" | "i64" | "usize" | "isize" | "f64" => return (8, 8),
        "c_void" | "()" | "void" => return (0, 1),
        _ => {}
    }

    // Cycle break.
    if visiting.iter().any(|v| v == trimmed) {
        return (8, 8);
    }

    if let Some(s) = ir.find_struct(trimmed) {
        visiting.push(trimmed.to_string());
        let (sz, al) = c_size_of_struct(s, ir, visiting);
        visiting.pop();
        return (sz, al);
    }

    if let Some(e) = ir.find_enum(trimmed) {
        visiting.push(trimmed.to_string());
        let r = if e.is_union {
            c_size_of_tagged_enum(e, ir, visiting)
        } else {
            // `#[repr(C)]` unit enum -> C `enum X` -> sizeof(int) = 4
            // on every LP64 platform we target. `#[repr(u8)]` (1 byte)
            // is also possible but rare in this codebase; honor it
            // when explicit. We don't currently distinguish at the IR
            // level, so default to 4 (matches `int` width that the
            // OCaml-side `int` Ctypes typ uses).
            match e.repr.as_deref() {
                Some("u8") | Some("i8") => (1, 1),
                Some("u16") | Some("i16") => (2, 2),
                Some("u64") | Some("i64") => (8, 8),
                _ => (4, 4),
            }
        };
        visiting.pop();
        return r;
    }

    // Callback function pointers, opaque types, unknown — pointer-sized.
    (8, 8)
}

fn c_size_of_struct(s: &StructDef, ir: &CodegenIR, visiting: &mut Vec<String>) -> (usize, usize) {
    let mut offset: usize = 0;
    let mut max_align: usize = 1;
    for f in &s.fields {
        // Ref-kind pointers are 8/8.
        let (fs, fa) = match f.ref_kind {
            FieldRefKind::Owned => c_size_of_type(&f.type_name, ir, visiting),
            FieldRefKind::Ref
            | FieldRefKind::RefMut
            | FieldRefKind::Ptr
            | FieldRefKind::PtrMut
            | FieldRefKind::Boxed
            | FieldRefKind::OptionBoxed => (8, 8),
        };
        if fa > max_align {
            max_align = fa;
        }
        // Align offset up to fa.
        offset = (offset + fa - 1) / fa * fa;
        offset += fs;
    }
    if max_align == 0 {
        max_align = 1;
    }
    // Round size up to struct alignment.
    let size = (offset + max_align - 1) / max_align * max_align;
    (size, max_align)
}

fn c_size_of_tagged_enum(
    e: &EnumDef,
    ir: &CodegenIR,
    visiting: &mut Vec<String>,
) -> (usize, usize) {
    use super::super::ir::EnumVariantKind;
    let mut max_payload_size: usize = 0;
    let mut max_payload_align: usize = 1;
    for v in &e.variants {
        let (psz, pal) = match &v.kind {
            EnumVariantKind::Unit => (0, 1),
            EnumVariantKind::Tuple(parts) => {
                let mut off: usize = 0;
                let mut al: usize = 1;
                for (ty, rk) in parts {
                    let (fs, fa) = match rk {
                        FieldRefKind::Owned => c_size_of_type(ty, ir, visiting),
                        _ => (8, 8),
                    };
                    if fa > al {
                        al = fa;
                    }
                    off = (off + fa - 1) / fa * fa;
                    off += fs;
                }
                let sz = if al > 0 { (off + al - 1) / al * al } else { off };
                (sz, al)
            }
            EnumVariantKind::Struct(fields) => {
                let mut off: usize = 0;
                let mut al: usize = 1;
                for f in fields {
                    let (fs, fa) = match f.ref_kind {
                        FieldRefKind::Owned => c_size_of_type(&f.type_name, ir, visiting),
                        _ => (8, 8),
                    };
                    if fa > al {
                        al = fa;
                    }
                    off = (off + fa - 1) / fa * fa;
                    off += fs;
                }
                let sz = if al > 0 { (off + al - 1) / al * al } else { off };
                (sz, al)
            }
        };
        if psz > max_payload_size {
            max_payload_size = psz;
        }
        if pal > max_payload_align {
            max_payload_align = pal;
        }
    }

    // #[repr(C, u8)]: 1-byte tag, padded up to max_payload_align, then
    // max_payload_size, then total padded up to max_payload_align.
    let head = if max_payload_align == 0 {
        1
    } else {
        ((1 + max_payload_align - 1) / max_payload_align) * max_payload_align
    };
    let total = head + max_payload_size;
    let aligned = if max_payload_align == 0 {
        total
    } else {
        (total + max_payload_align - 1) / max_payload_align * max_payload_align
    };
    (aligned, max_payload_align.max(1))
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

    // Idiomatic module wrapper: `Azul.Update.refresh_dom` instead of
    // `Azul.az_update_variant_refresh_dom`. Variants land as snake_case
    // module values (OCaml convention; uppercase would be reserved for
    // constructors, which we don't use here because the C ABI value is
    // an int, not a typed variant). The module shadows any sibling
    // type alias by name; OCaml resolves identifiers from the most
    // recent binding, so `Azul.Update.refresh_dom : int` and
    // `Azul.az_update : int typ` coexist without conflict.
    builder.line(&format!("module {} = struct", e.name));
    builder.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        let lit = sanitize_identifier(&super::to_snake_case(&v.name));
        builder.line(&format!("let {} : int = {}", lit, idx));
    }
    builder.dedent();
    builder.line("end");
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
