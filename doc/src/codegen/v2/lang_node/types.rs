//! Type-shape registrations for the JavaScript bindings.
//!
//! koffi accepts struct, union, and array registrations as JS calls:
//!
//! ```js
//! koffi.struct('AzAppConfig', {
//!     enable_visual_panic: 'bool',
//!     log_level:           'uint32_t',
//!     // ...
//! });
//! ```
//!
//! Bun and Deno don't pre-register types — their FFI layers infer struct
//! shape at the call site from the symbol-map specification. We still
//! emit the registration calls for both, because:
//!
//! - `azulFFI.struct(...)` returns `null` on Bun/Deno (see `mod.rs`),
//!   so the calls are no-ops on those runtimes.
//! - The shape is documented in JSDoc-comment form right above the call
//!   for human readers regardless of runtime.
//!
//! Tagged-union enums are emitted as koffi unions with an outer wrapper
//! struct carrying the tag. Each variant payload struct is registered
//! separately so its fields are nameable.
//!
//! ## Skipped categories
//!
//! Same filter as `lang_lua` and `lang_php`. See `mod.rs` doc-comment
//! for the full list.

use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, StructDef, TypeCategory,
};
use super::{ffi_type_name, map_type_to_koffi, sanitize_js_identifier};

// ============================================================================
// Public entry point
// ============================================================================

pub fn generate_type_registrations(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Type registrations. On Node/koffi these become live `koffi.struct(...)`");
    b.line("// handles. On Bun/Deno they are documentation no-ops.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    // Unit-only enums: emit as JS frozen objects keyed by variant name.
    // The numeric value is the enum's sequential index, matching the C ABI.
    b.line("// Unit-only enums: numeric constant tables.");
    b.line("const Enums = Object.create(null);");
    b.blank();

    for e in &ir.enums {
        if !should_emit(e.category) || !e.generic_params.is_empty() {
            continue;
        }
        if !is_unit_only(e) {
            continue;
        }
        emit_unit_enum(b, e);
    }

    b.blank();
    b.line("// Tag enums for tagged unions (one per data-bearing enum).");
    for e in &ir.enums {
        if !should_emit(e.category) || !e.generic_params.is_empty() {
            continue;
        }
        if is_unit_only(e) {
            continue;
        }
        if !e.is_union {
            continue;
        }
        emit_tag_enum(b, e);
    }

    b.blank();
    b.line("// Per-variant payload structs (one per non-unit variant of a tagged union).");
    for e in &ir.enums {
        if !should_emit(e.category) || !e.generic_params.is_empty() {
            continue;
        }
        if !e.is_union {
            continue;
        }
        emit_variant_payload_structs(b, e, ir);
    }

    b.blank();
    b.line("// POD struct registrations.");
    for s in &ir.structs {
        if !should_emit(s.category) || !s.generic_params.is_empty() {
            continue;
        }
        emit_struct_registration(b, s, ir);
    }

    b.blank();
    b.line("// Outer tagged-union wrapper structs (tag + union of variant payloads).");
    for e in &ir.enums {
        if !should_emit(e.category) || !e.generic_params.is_empty() {
            continue;
        }
        if !e.is_union {
            continue;
        }
        emit_tagged_union_wrapper(b, e);
    }

    b.blank();
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit(c: TypeCategory) -> bool {
    !matches!(
        c,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::Boxed
            | TypeCategory::GenericTemplate
            | TypeCategory::DestructorOrClone
            | TypeCategory::CallbackTypedef
    )
}

fn is_unit_only(e: &EnumDef) -> bool {
    !e.is_union
        && e.variants
            .iter()
            .all(|v| matches!(v.kind, EnumVariantKind::Unit))
}

// ============================================================================
// Unit enum emission
// ============================================================================

fn emit_unit_enum(b: &mut CodeBuilder, e: &EnumDef) {
    let ffi = ffi_type_name(&e.name);
    b.line(&format!("Enums.{} = Object.freeze({{", e.name));
    b.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        b.line(&format!(
            "{}: {}, // {}_{}",
            sanitize_js_identifier(&v.name),
            idx,
            ffi,
            v.name
        ));
    }
    b.dedent();
    b.line("});");
}

// ============================================================================
// Tag enum emission (for tagged-union enums)
// ============================================================================

fn emit_tag_enum(b: &mut CodeBuilder, e: &EnumDef) {
    let ffi = ffi_type_name(&e.name);
    b.line(&format!("Enums.{}_Tag = Object.freeze({{", e.name));
    b.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        b.line(&format!(
            "{}: {}, // {}_Tag_{}",
            sanitize_js_identifier(&v.name),
            idx,
            ffi,
            v.name
        ));
    }
    b.dedent();
    b.line("});");
}

// ============================================================================
// Variant payload structs
// ============================================================================

fn emit_variant_payload_structs(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let ffi = ffi_type_name(&e.name);
    let tag_field = format!("{}_Tag", ffi);
    for v in &e.variants {
        let payload_name = format!("{}Variant_{}", ffi, v.name);
        b.line(&format!("// Payload struct for {}::{}.", e.name, v.name));
        b.line(&format!("azulFFI.struct('{}', {{", payload_name));
        b.indent();
        b.line(&format!("tag: '{}',", tag_field));
        match &v.kind {
            EnumVariantKind::Unit => {
                // tag-only; nothing else
            }
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let spec = ref_kind_spec(ty, ref_kind, ir);
                    b.line(&format!("payload: '{}',", spec));
                } else {
                    for (i, (ty, ref_kind)) in types.iter().enumerate() {
                        let spec = ref_kind_spec(ty, ref_kind, ir);
                        b.line(&format!("payload_{}: '{}',", i, spec));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let spec = ref_kind_spec(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!(
                        "{}: '{}',",
                        sanitize_js_identifier(&f.name),
                        spec
                    ));
                }
            }
        }
        b.dedent();
        b.line("});");
    }
}

// ============================================================================
// Struct registration
// ============================================================================

fn emit_struct_registration(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let ffi = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        b.line(&format!("// {}", s.doc.join(" ")));
    }
    if s.fields.is_empty() {
        // koffi disallows empty structs; emit a 1-byte placeholder so the
        // type still has a registered handle.
        b.line(&format!(
            "azulFFI.struct('{}', {{ _placeholder: 'uint8_t' }});",
            ffi
        ));
        return;
    }

    b.line(&format!("azulFFI.struct('{}', {{", ffi));
    b.indent();
    for f in &s.fields {
        // Detect array `[T; N]` types.
        if let Some((elem_ty, count)) = parse_array_type(&f.type_name) {
            let elem_spec = map_type_to_koffi(&elem_ty, ir);
            // koffi supports inline array types via `T [N]` syntax.
            b.line(&format!(
                "{}: '{} [{}]',",
                sanitize_js_identifier(&f.name),
                elem_spec,
                count
            ));
            continue;
        }
        let spec = ref_kind_spec(&f.type_name, &f.ref_kind, ir);
        b.line(&format!(
            "{}: '{}',",
            sanitize_js_identifier(&f.name),
            spec
        ));
    }
    b.dedent();
    b.line("});");
}

// ============================================================================
// Tagged-union outer wrapper
// ============================================================================

fn emit_tagged_union_wrapper(b: &mut CodeBuilder, e: &EnumDef) {
    let ffi = ffi_type_name(&e.name);
    // The C ABI lays out a tagged union as:
    //   union { Variant1Payload v1; Variant2Payload v2; ... }
    // because every variant struct begins with the same `tag` field.
    // We register that as a koffi union; on Bun/Deno this is a no-op.
    b.line(&format!("azulFFI.union('{}', {{", ffi));
    b.indent();
    for v in &e.variants {
        let variant_name = sanitize_js_identifier(&v.name);
        let variant_struct = format!("{}Variant_{}", ffi, v.name);
        b.line(&format!("{}: '{}',", variant_name, variant_struct));
    }
    b.dedent();
    b.line("});");
}

// ============================================================================
// Type-spec helpers
// ============================================================================

/// Build a koffi type-spec string for a field with a given ref_kind.
/// Pointer / reference kinds become `'<type> *'`. Owned types become
/// the bare registered name.
fn ref_kind_spec(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    let base = map_type_to_koffi(type_name, ir);
    match ref_kind {
        FieldRefKind::Owned => base,
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => format!("{} *", base),
    }
}

/// Parse `[T; N]` array type strings. Returns `(elem_type, count)`.
fn parse_array_type(s: &str) -> Option<(String, usize)> {
    let trimmed = s.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let (elem, count) = inner.split_once(';')?;
    let count: usize = count.trim().parse().ok()?;
    Some((elem.trim().to_string(), count))
}
