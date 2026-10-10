//! OCaml type emission: struct (Ctypes `structure`) declarations,
//! field/seal definitions, and enum / tagged-union accessors.
//!
//! Strategy:
//! - **Two-pass struct emission**: Ctypes requires a struct's typ value to exist before fields are
//!   added (because field types may reference other structs). Pass 1 emits the bare typ stub; pass
//!   2 adds fields and seals.
//! - **Unit enums** (`is_union == false`) -> the FFI view `type az_x = int` with its typ value,
//!   identity `_to_int` / `_of_int` helpers and one `az_x_variant_<v>` constant per variant. The
//!   user-facing ADT module (`Update.RefreshDom`) lives in `wrappers.rs` (`azul_enums_<module>.ml`).
//! - **Tagged-union enums** (`is_union == true`) -> the FFI-side `structure` with a `tag :
//!   uint32_t` field plus a `payload` byte array sized for the largest variant. The OCaml-side
//!   polymorphic variant + conversion helpers live in `wrappers.rs`.
//! - **Types with no public surface** (`Recursive`, `DestructorOrClone`, `GenericTemplate`) get a
//!   one-line note saying what they are. They are not omissions: a destructor/clone union is
//!   libazul's own plumbing and still gets its ABI placeholder above, and a generic template has
//!   no C form of its own - each instantiation is emitted as its own monomorphized alias.

use anyhow::Result;

use super::{
    super::{
        c_layout::{mono_layout, type_layout, union_payload_layout, AbiLayout},
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            CodegenIR, EnumDef, FieldDef, FieldRefKind, MonomorphizedKind, StructDef, TypeAliasDef,
            TypeCategory,
        },
    },
    map_type_to_ocaml, map_type_to_ocaml_typ, ocaml_ffi_type_name, sanitize_doc,
    sanitize_identifier,
};

// ============================================================================
// One plan chunk = stubs, then unit enums, then fields/seal
// ============================================================================

/// Every declaration of the types `belongs` accepts, in the order Ctypes
/// needs: the `structure` stubs (so mutually recursive references resolve),
/// then the unit enums (struct fields name their typ values), then the
/// fields + `seal` of structs and tagged unions in dependency order. The
/// types a chunk references but does not declare live in chunks the caller
/// `open`ed.
pub fn emit_types_chunk(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    emit_forward_struct_decls(builder, ir, config, belongs);
    emit_struct_fields_and_enums(builder, ir, config, belongs)
}

// ============================================================================
// Implementation: forward stubs (pass 1)
// ============================================================================

pub fn emit_forward_struct_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Phase 1: struct typ stubs (fields are added after every typ exists).      *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !should_emit_struct(s, config) {
            continue;
        }
        let ffi = ocaml_ffi_type_name(&s.name);
        // OCaml: `type az_app` is a phantom abstract type; the typ
        // value carries the runtime structure.
        builder.line(&format!("type {}", ffi));
        builder.line(&format!(
            "let ({} : {} structure typ) = structure \"{}\"",
            ffi,
            ffi,
            format_c_struct_name(&s.name)
        ));
    }

    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !should_emit_enum(e, config) {
            continue;
        }
        if e.is_union {
            let ffi = ocaml_ffi_type_name(&e.name);
            builder.line(&format!("type {}", ffi));
            builder.line(&format!(
                "let ({} : {} structure typ) = structure \"{}\"",
                ffi,
                ffi,
                format_c_struct_name(&e.name)
            ));
        }
    }
    // Callback typedefs alias to `(ptr void)` at the value level.
    // The signature pieces (`val with_resolver :
    // az_icon_resolver_callback_type -> t`) only need the type token
    // to exist; actual marshalling happens via `static_funptr` /
    // `Foreign.funptr` at call sites.
    for cb in ir.callback_typedefs.iter().filter(|cb| belongs(&cb.name)) {
        let ffi = ocaml_ffi_type_name(&cb.name);
        builder.line(&format!("type {} = unit ptr", ffi));
        builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
    }

    // Filtered-out / monomorphized types — same placeholders as the
    // .mli so the implementation side has matching declarations.
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !config.should_include_type(&s.name) || !s.generic_params.is_empty() {
            continue;
        }
        if matches!(
            s.category,
            TypeCategory::Recursive | TypeCategory::DestructorOrClone
        ) {
            let ffi = ocaml_ffi_type_name(&s.name);
            builder.line(&format!("type {} = unit ptr", ffi));
            builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
        }
    }
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
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
    for ta in ir.type_aliases.iter().filter(|ta| belongs(&ta.name)) {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        emit_type_alias(builder, ta, ir);
    }
    builder.blank();
}

/// The FFI view of one type alias.
///
/// A monomorphized generic alias (`LayoutClearValue =
/// CssPropertyValue<LayoutClear>`) is not a pointer: azul.h emits
/// `union AzLayoutClearValue { ... }` and every entry point takes it
/// and returns it BY VALUE. Giving all of them `unit ptr` made the
/// binding read and write 8 bytes where the C ABI has a 16-, 24- or
/// 88-byte union, and shrank every parent that embeds one - the same
/// defect the `DestructorOrClone` unions above were fixed for. So an
/// aggregate alias gets a sized blob, exactly as a tagged union of the
/// same layout does, a scalar alias gets its target's own view
/// (`GLuint` IS a `u32`), and only an alias the IR cannot size stays
/// opaque.
fn emit_type_alias(builder: &mut CodeBuilder, ta: &TypeAliasDef, ir: &CodegenIR) {
    let ffi = ocaml_ffi_type_name(&ta.name);
    // An aggregate alias (a monomorphized tagged union or struct) is sized
    // by the shared C layout - the same numbers azul.h and every other
    // blob-sizing binding use.
    let aggregate = ta
        .monomorphized_def
        .as_ref()
        .filter(|m| !matches!(m.kind, MonomorphizedKind::SimpleEnum { .. }))
        .and_then(|m| mono_layout(m, ir, 0));
    if let Some(AbiLayout { size, align }) = aggregate {
        builder.line(&format!("type {}", ffi));
        builder.line(&format!(
            "let ({} : {} structure typ) = structure \"{}\"",
            ffi,
            ffi,
            format_c_struct_name(&ta.name)
        ));
        emit_byte_blob_fields(builder, &ffi, size, align);
        builder.line(&format!("let () = seal {}", ffi));
        return;
    }
    match alias_scalar_view(ta, ir) {
        Some((ocaml_type, ctypes_value)) => {
            builder.line(&format!("type {} = {}", ffi, ocaml_type));
            builder.line(&format!("let ({} : {} typ) = {}", ffi, ffi, ctypes_value));
        }
        None => {
            builder.line(&format!("type {} = unit ptr", ffi));
            builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));
        }
    }
}

/// The `(OCaml type, Ctypes view)` of an alias that crosses the ABI as a
/// scalar: a monomorphized unit enum (an `int`, like every other unit
/// enum) or an alias of a primitive (`GLuint = u32`). `None` for anything
/// whose target this emitter cannot name.
fn alias_scalar_view(ta: &TypeAliasDef, ir: &CodegenIR) -> Option<(String, String)> {
    if let Some(m) = &ta.monomorphized_def {
        if matches!(m.kind, MonomorphizedKind::SimpleEnum { .. }) {
            return Some(("int".to_string(), "int".to_string()));
        }
        return None;
    }
    primitive_size(ta.target.trim())?;
    Some((
        map_type_to_ocaml_typ(&ta.target, ir),
        map_type_to_ocaml(&ta.target, ir),
    ))
}

// ============================================================================
// Implementation: fields / seal / enum constants (pass 2)
// ============================================================================

pub fn emit_struct_fields_and_enums(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Phase 2: struct fields, sealing, enum integer-mapping helpers.            *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    // Unit enums FIRST — struct fields reference them by typ value
    // (`field s \"frame\" az_window_frame`) so the typ binding must
    // be in scope before any struct field declaration uses it.
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !should_emit_enum(e, config) {
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e, ir);
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
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !should_emit_struct(s, config) {
            builder.line(&no_public_surface_note(
                "struct",
                &s.name,
                &s.generic_params,
                s.category,
            ));
            continue;
        }
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !should_emit_enum(e, config) {
            builder.line(&no_public_surface_note(
                "enum",
                &e.name,
                &e.generic_params,
                e.category,
            ));
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
    // A borrowed slice (`VecRef`) is NOT excluded. `struct AzU8VecRef {
    // const void* ptr; size_t len; }` is an ordinary two-field struct that
    // every `AzGl_*` entry point takes BY VALUE; emitting it as one opaque
    // word passed only the pointer and left the length whatever was in the
    // next register. It gets no wrapper record (nothing here owns borrowed
    // memory, so nothing may free it) - only the honest layout.
    !matches!(
        s.category,
        TypeCategory::Recursive | TypeCategory::DestructorOrClone | TypeCategory::GenericTemplate
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
    // The shared C layout (`c_layout::type_layout`), which every tagged
    // union has (`bug_classes::c_layout_sizes_every_tagged_union_like_rust`).
    let AbiLayout { size, align } = type_layout(&e.name, ir).unwrap_or(AbiLayout { size: 8, align: 8 });
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
        let is_option_or_result = e.name.starts_with("Option") || e.name.starts_with("Result");
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
                "let tag_ptr = Ctypes.coerce (Ctypes.ptr {}) (Ctypes.ptr Ctypes.uint8_t) raw_ptr \
                 in",
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
            // unions the payload starts at `payload_offset` (below) - the
            // tag byte rounded up to the largest alignment of any
            // variant, NOT the payload's own alignment (the old
            // `max 1 (Ctypes.alignment payload)` read a 4-aligned `Ok`
            // next to an 8-aligned `Err` 4 bytes early). Coerce
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
                    // Where Rust puts the payload: the tag rounded up to
                    // the largest alignment of ANY variant
                    // (`c_layout::union_payload_layout`, the one place
                    // that decides it), not the payload's own alignment -
                    // an `Ok` of 4-byte alignment next to an 8-aligned `Err`
                    // sits at 8.
                    let payload_offset =
                        union_payload_layout(&e.name, ir).map(|p| p.payload_offset);
                    if let (true, Some(payload_offset)) = (payload_is_proper_struct, payload_offset) {
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
                        builder.line(&format!("if not ({}_{} r) then None", ffi, positive_name));
                        builder.line("else");
                        builder.indent();
                        builder.line("let raw_ptr = Ctypes.addr r in");
                        builder.line(&format!(
                            "let byte_ptr = Ctypes.coerce (Ctypes.ptr {}) (Ctypes.ptr \
                             Ctypes.char) raw_ptr in",
                            ffi
                        ));
                        builder.line(&format!(
                            "let payload_byte_ptr = Ctypes.(+@) byte_ptr {} in",
                            payload_offset
                        ));
                        builder.line(&format!(
                            "let payload_ptr = Ctypes.coerce (Ctypes.ptr Ctypes.char) (Ctypes.ptr \
                             {}) payload_byte_ptr in",
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

/// (size, alignment) of a C primitive, or `None` when the name is not one.
fn primitive_size(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "bool" | "u8" | "i8" | "c_char" | "c_uchar" | "char" => (1, 1),
        "u16" | "i16" => (2, 2),
        "u32" | "i32" | "c_int" | "c_uint" | "f32" => (4, 4),
        "u64" | "i64" | "usize" | "isize" | "f64" => (8, 8),
        "c_void" | "()" | "void" => (0, 1),
        _ => return None,
    })
}

// ============================================================================
// Unit enum (integer round-trip helpers)
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let ffi = ocaml_ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!(
            "(* unit enum {}: api.json declares no variants, so there is no value to emit *)",
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
    builder.line(&format!("let {}_to_int (i : int) : int = i", ffi));
    builder.line(&format!("let {}_of_int (i : int) : int = i", ffi));
    // Emit named constants for each variant. Use `<ffi>_variant_<v>`
    // (not just `<ffi>_<v>`) so the constant doesn't collide with a
    // struct of the same flattened name — e.g. `az_shape_ellipse`
    // is BOTH the struct `ShapeEllipse` AND the `Ellipse` variant of
    // the `Shape` enum. Disambiguate at the variant side.
    for (idx, v) in e.variants.iter().enumerate() {
        let lit = sanitize_identifier(&super::to_snake_case(&v.name));
        builder.line(&format!("let {}_variant_{} : int = {}", ffi, lit, idx));
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
    // Moved to the late idiomatic pass - see the interface side.
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

/// The one-line note that stands where a type with no public surface would
/// have been declared.
///
/// None of these is an omission the emitter should be fixed for, and none
/// of them costs the binding any API:
///
/// - a `DestructorOrClone` union is libazul's own drop glue (its fields are function pointers
///   libazul calls on its own values); the ABI placeholder it needs in order to sit inside a
///   parent struct is declared in pass 1, and there is nothing else to wrap;
/// - a `Recursive` type cannot be laid out by value at all (it contains itself);
/// - a generic template has no C form of its own - `CssPropertyValue<T>` exists in azul.h only as
///   its instantiations, which this emitter writes as monomorphized aliases.
///
/// It says so, instead of the "SKIPPED" it used to say, because "skipped"
/// reads as "the emitter gave up here" and sent every reader looking for
/// missing API.
fn no_public_surface_note(
    what: &str,
    name: &str,
    generic_params: &[String],
    category: TypeCategory,
) -> String {
    if !generic_params.is_empty() {
        return format!(
            "(* {} {} is a generic template: it has no C form of its own, only the \
             monomorphized instantiations emitted as type aliases *)",
            what, name
        );
    }
    format!(
        "(* {} {} is internal to libazul ({}): no public surface, and its ABI \
         placeholder is declared above *)",
        what,
        name,
        category.description()
    )
}
