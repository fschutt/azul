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
    CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, MonomorphizedKind, MonomorphizedTypeDef,
    MonomorphizedVariant, StructDef, TypeAliasDef, TypeCategory,
};
use super::{ffi_type_name, map_type_to_koffi, sanitize_js_identifier};

// ============================================================================
// Public entry point
// ============================================================================

pub fn generate_type_registrations(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Type registrations. On Node/koffi these become live `koffi.struct(...)`");
    b.line("// handles. On Bun/Deno they are documentation no-ops.");
    b.line("//");
    b.line("// Emitted in the IR's topological-sort order (lower sort_order first), so");
    b.line("// koffi.struct(...) calls never reference types whose shape isn't yet");
    b.line("// registered. Within a single enum/struct's sort slot we emit all of its");
    b.line("// pieces — tag enum, per-variant payload structs, then the outer wrapper.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    b.line("// Unit-only enums: numeric constant tables.");
    b.line("const Enums = Object.create(null);");
    b.blank();

    // Merge structs, enums, and monomorphized type aliases into a
    // single emit list, ordered by `sort_order`. The IR builder has
    // already computed a topological sort; we just unify the three
    // streams so types are registered before any later type references
    // them.
    enum SortedItem<'a> {
        Struct(&'a StructDef),
        Enum(&'a EnumDef),
        MonomorphizedAlias(&'a TypeAliasDef, &'a MonomorphizedTypeDef),
        CallbackTypedef(&'a str),
    }
    let mut sorted: Vec<(usize, SortedItem)> = Vec::new();
    for e in &ir.enums {
        if !should_emit(e.category) || !e.generic_params.is_empty() {
            continue;
        }
        sorted.push((e.sort_order, SortedItem::Enum(e)));
    }
    for s in &ir.structs {
        if !should_emit(s.category) || !s.generic_params.is_empty() {
            continue;
        }
        sorted.push((s.sort_order, SortedItem::Struct(s)));
    }
    for ta in &ir.type_aliases {
        let Some(ref mono_def) = ta.monomorphized_def else {
            continue;
        };
        sorted.push((ta.sort_order, SortedItem::MonomorphizedAlias(ta, mono_def)));
    }
    // Callback typedefs (function pointers) — register as koffi aliases
    // for `void *` so struct fields can carry them as `'AzFooCallbackType'`
    // without forcing koffi to model the function signature.
    for cb in &ir.callback_typedefs {
        sorted.push((cb.sort_order, SortedItem::CallbackTypedef(&cb.name)));
    }
    // Stable sort: ties (same sort_order) keep their declaration order.
    sorted.sort_by_key(|(o, _)| *o);

    for (_, item) in sorted {
        match item {
            SortedItem::Enum(e) => {
                if is_unit_only(e) {
                    emit_unit_enum(b, e);
                } else if e.is_union {
                    emit_tag_enum(b, e);
                    emit_variant_payload_structs(b, e, ir);
                    emit_tagged_union_wrapper(b, e);
                }
            }
            SortedItem::Struct(s) => {
                emit_struct_registration(b, s, ir);
            }
            SortedItem::MonomorphizedAlias(ta, mono_def) => {
                emit_monomorphized_alias(b, ta, mono_def, ir);
            }
            SortedItem::CallbackTypedef(name) => {
                b.line(&format!(
                    "azulFFI.alias('{}', 'void *');",
                    ffi_type_name(name)
                ));
            }
        }
    }

    b.blank();
}

fn emit_monomorphized_alias(
    b: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono_def: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    let name = ffi_type_name(&ta.name);
    if !ta.doc.is_empty() {
        b.line(&format!("// {}", ta.doc.join(" ")));
    }
    match &mono_def.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            b.line(&format!("azulFFI.alias('{}', 'uint32_t');", name));
            b.line(&format!("Enums.{} = Object.freeze({{", ta.name));
            b.indent();
            for (idx, v) in variants.iter().enumerate() {
                b.line(&format!("{}: {},", sanitize_js_identifier(v), idx));
            }
            b.dedent();
            b.line("});");
        }
        MonomorphizedKind::Struct { fields } => {
            b.line(&format!("azulFFI.struct('{}', {{", name));
            b.indent();
            if fields.is_empty() {
                b.line("_placeholder: 'uint8_t',");
            } else {
                for f in fields {
                    let spec = ref_kind_spec(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!(
                        "{}: '{}',",
                        sanitize_js_identifier(&f.name),
                        spec
                    ));
                }
            }
            b.dedent();
            b.line("});");
        }
        MonomorphizedKind::TaggedUnion { variants, .. } => {
            // Tag alias + JS frozen object.
            b.line(&format!("azulFFI.alias('{}_Tag', 'uint32_t');", name));
            b.line(&format!("Enums.{}_Tag = Object.freeze({{", ta.name));
            b.indent();
            for (idx, v) in variants.iter().enumerate() {
                b.line(&format!(
                    "{}: {},",
                    sanitize_js_identifier(&v.name),
                    idx
                ));
            }
            b.dedent();
            b.line("});");

            // Per-variant payload structs.
            for v in variants {
                b.line(&format!(
                    "azulFFI.struct('{}Variant_{}', {{",
                    name, v.name
                ));
                b.indent();
                b.line(&format!("tag: '{}_Tag',", name));
                if let Some(ref payload_type) = v.payload_type {
                    let spec = ref_kind_spec(payload_type, &v.payload_ref_kind, ir);
                    b.line(&format!("payload: '{}',", spec));
                }
                b.dedent();
                b.line("});");
            }

            // Outer wrapper (struct with tag + union of payloads).
            b.line(&format!("azulFFI.union('{}_Union', {{", name));
            b.indent();
            for v in variants {
                b.line(&format!(
                    "{}: '{}Variant_{}',",
                    sanitize_js_identifier(&v.name),
                    name,
                    v.name
                ));
            }
            b.dedent();
            b.line("});");
            b.line(&format!("azulFFI.struct('{}', {{", name));
            b.indent();
            b.line(&format!("tag: '{}_Tag',", name));
            b.line(&format!("payload: '{}_Union',", name));
            b.dedent();
            b.line("});");
        }
    }
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit(c: TypeCategory) -> bool {
    // Note: Boxed / VecRef / DestructorOrClone are NOT skipped — they're
    // referenced by value as field types in tagged-union variants and
    // Vec wrappers (e.g., `AzImageRef` is `is_boxed_object` in api.json
    // but appears in `AzMenuItemIconVariant_Image`'s payload). Without
    // emit, koffi rejects "Unknown or invalid type name".
    !matches!(
        c,
        TypeCategory::Recursive
            | TypeCategory::GenericTemplate
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

fn emit_unit_enum_alias_only(b: &mut CodeBuilder, e: &EnumDef) {
    // Unit enums are emitted as JS frozen objects above, but they also
    // need a koffi alias so struct fields with `type: 'AzFoo'` resolve.
    let ffi = ffi_type_name(&e.name);
    b.line(&format!("azulFFI.alias('{}', 'uint32_t');", ffi));
}

fn emit_unit_enum(b: &mut CodeBuilder, e: &EnumDef) {
    emit_unit_enum_alias_only(b, e);
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
    // koffi side: register the tag enum's name as an alias for uint8_t
    // (C-ABI `#[repr(C, u8)]`; uint32 would shift every payload offset
    // on small-aligned variants — same family of bug Java/C#/Kotlin
    // fixed last week).
    b.line(&format!(
        "azulFFI.alias('{}_Tag', 'uint8_t');",
        ffi
    ));
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
        // For pointer fields koffi only needs the marshaling size (sizeof
        // void*), not the referent's shape — and emitting `AzVideoMode *`
        // here forces topological dependence on `AzVideoMode` even though
        // we never dereference it through the field. Collapse to `void *`.
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "void *".to_string(),
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
