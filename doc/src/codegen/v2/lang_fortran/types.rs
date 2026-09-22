//! Fortran derived-type / enum / tagged-union emission.
//!
//! Strategy:
//!
//! - **POD structs** map to `type, bind(C) :: AzFoo ... end type AzFoo`.
//!   Fortran `bind(C)` derived types have C-compatible memory layout
//!   (matches Rust's `#[repr(C)]`), so values can flow across the FFI
//!   boundary by value.
//! - **Unit-only enums** become an F2003 `enum, bind(C)` block (which
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
//! - **Recursive / VecRef / DestructorOrClone** types are emitted as
//!   ABI-opaque blob stand-ins when their layout is computable (they ARE
//!   embedded by value — every `AzXVec` carries an `AzXVecDestructor`).
//! - **Generic templates** (`CssPropertyValue<T>`, `PhysicalSize<T>`) have
//!   no C ABI of their own and get no declaration: what crosses the
//!   boundary is an INSTANTIATION (`StyleCursorValue`, `PhysicalSizeU32`),
//!   and those are declared from `ir.type_aliases` like any other type.
//!   The template's place in the file carries a note naming them.
//! - **Named constants** (`GlContextPtr_ACCUM_ALPHA_BITS`) become
//!   `integer(kind), parameter` declarations in the chunk that declares
//!   their owning class.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, ConstantDef, EnumDef, EnumVariantKind, FieldDef,
    FieldRefKind, MonomorphizedKind, MonomorphizedTypeDef, StructDef, TypeAliasDef, TypeCategory,
};
use super::layout::{blob_field_decl, mono_layout, type_layout};
use super::{
    ffi_type_name, map_type_to_fortran, sanitize_comment_line, sanitize_identifier,
    truncate_identifier,
};

// ============================================================================
// Top-level type-block emission
// ============================================================================

/// The type definitions of every type `belongs` accepts — one plan
/// chunk's worth — each group preceded by the api.json-module marker the
/// per-module facades are built from.
pub fn generate_types_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
    split: &super::Split,
) -> Result<()> {
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Type definitions: derived types, enums, tagged-union approximations.");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    // 0. The named constants of the classes this chunk declares (the
    //    OpenGL enum values api.json files under `GlContextPtr`). They
    //    are the same kind of declaration as the unit-enum enumerators
    //    below, so they live with the types rather than with the
    //    interface blocks.
    emit_constants_for(builder, ir, belongs, split);

    // 1. Unit (simple) enums first so they may appear in derived-type
    //    field declarations as `integer(c_int)` aliases. Skipped-category
    //    tagged unions (DestructorOrClone etc.) are embedded BY VALUE in
    //    regular structs (every AzXVec carries an AzXVecDestructor), so
    //    they get an ABI-opaque blob stand-in here — mapping them to
    //    `type(c_ptr)` shrank every embedding struct and corrupted all
    //    by-value FFI calls (2026-07 Fortran e2e SIGSEGV root cause).
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !should_include_enum(e, config) {
            if e.is_union && e.generic_params.is_empty() {
                if let Some(l) = type_layout(&e.name, ir) {
                    builder.line(&split.marker(&e.name));
                    emit_opaque_blob(builder, &e.name, l, e.category.description());
                    continue;
                }
            }
            emit_skipped_enum(builder, ir, e);
            continue;
        }
        if !e.is_union {
            builder.line(&split.marker(&e.name));
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
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !should_include_struct(s, config) {
            // Same ABI rule as skipped unions above: if the skipped
            // struct is layout-computable, other structs may embed it by
            // value (e.g. AzXmlNodeChild inside AzXmlNode), so emit an
            // exact-size blob stand-in instead of collapsing to c_ptr.
            if s.generic_params.is_empty() {
                if let Some(l) = type_layout(&s.name, ir) {
                    builder.line(&split.marker(&s.name));
                    emit_opaque_blob(builder, &s.name, l, s.category.description());
                    continue;
                }
            }
            emit_skipped_struct(builder, ir, s);
            continue;
        }
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    for ta in ir.type_aliases.iter().filter(|ta| belongs(&ta.name)) {
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
            Item::Struct(s) => {
                builder.line(&split.marker(&s.name));
                emit_struct(builder, s, ir)
            }
            Item::Union(e) => {
                builder.line(&split.marker(&e.name));
                emit_tagged_union(builder, e, ir)
            }
            Item::Mono(ta, mono) => {
                builder.line(&split.marker(&ta.name));
                emit_monomorphized_alias(builder, ta, mono, ir)
            }
        }
    }

    // 4. Callback (procedural) typedefs.
    for cb in ir.callback_typedefs.iter().filter(|cb| belongs(&cb.name)) {
        builder.line(&split.marker(&cb.name));
        emit_callback_typedef(builder, cb, ir);
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Named constants
// ============================================================================

/// The Fortran name of an api.json constant.
///
/// api.json files a constant under its class (`GlContextPtr_ACCUM_ALPHA_BITS`)
/// and the C binding spells it `AzGlContextPtr_ACCUM_ALPHA_BITS`. Keeping
/// that prefix here is not decoration: a Fortran `parameter` is visible in
/// every scope that reaches the module, `use azul` reaches all of them, and
/// the OpenGL enum names include `INT`, `EXP`, `INDEX`, `MIN`, `MAX`,
/// `REPEAT` and `NEAREST` — every one of them a Fortran intrinsic a bare
/// constant would shadow in the user's own program. The prefixed spelling is
/// also the one the unit-enum enumerators already use
/// (`AzUpdate_RefreshDom`), so the whole binding names its compile-time
/// values one way.
pub(crate) fn constant_name(c: &ConstantDef) -> String {
    truncate_identifier(&ffi_type_name(&c.name))
}

/// The class an api.json constant is filed under: `GlContextPtr` of
/// `GlContextPtr_ACCUM_ALPHA_BITS`. api.json class names are PascalCase
/// with no `_`, so the first one separates the class from the constant.
fn constant_owner(c: &ConstantDef) -> &str {
    c.name
        .split_once('_')
        .map(|(owner, _)| owner)
        .unwrap_or(&c.name)
}

/// The named constants of every class this chunk declares, grouped by
/// owning class so each group can carry the api.json-module marker and the
/// `public ::` run the per-module facades are built from.
fn emit_constants_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    belongs: &dyn Fn(&str) -> bool,
    split: &super::Split,
) {
    let mine: Vec<&ConstantDef> = ir
        .constants
        .iter()
        .filter(|c| belongs(constant_owner(c)))
        .collect();
    let mut i = 0;
    while i < mine.len() {
        let owner = constant_owner(mine[i]);
        let mut j = i;
        while j < mine.len() && constant_owner(mine[j]) == owner {
            j += 1;
        }
        emit_constant_group(builder, ir, split, owner, &mine[i..j]);
        i = j;
    }
}

fn emit_constant_group(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    split: &super::Split,
    owner: &str,
    group: &[&ConstantDef],
) {
    builder.line(&split.marker(owner));
    builder.line(&format!(
        "! The {} compile-time constants of {}.",
        group.len(),
        owner
    ));
    let mut declared = Vec::with_capacity(group.len());
    for c in group {
        let f_ty = map_type_to_fortran(&c.type_name, ir);
        // Every api.json constant is an integer today. A non-integer one
        // has no `parameter` spelling this emitter knows, and inventing a
        // wrong one would be worse than the loud "undefined name" a caller
        // gets instead.
        let Some(kind) = f_ty.strip_prefix("integer(").and_then(|k| k.strip_suffix(')')) else {
            continue;
        };
        let name = constant_name(c);
        for d in &c.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
        let (literal, note) = constant_literal(&c.value, kind);
        if let Some(note) = note {
            builder.line(&format!("! {}", note));
        }
        builder.line(&format!("{}, parameter :: {} = {}", f_ty, name, literal));
        declared.push(name);
    }
    for name in &declared {
        builder.line(&format!("public :: {}", name));
    }
    builder.blank();
}

/// The Fortran literal for an api.json constant value.
///
/// api.json spells the OpenGL values in hex (`"0x0D5B"`), which is how the
/// GL specification and every other binding writes them, so `int(z'0D5B',
/// c_int32_t)` keeps that spelling. Fortran has no unsigned integer kind,
/// however, and a handful of these values have the sign bit set
/// (`0xFFFFFFFF`, `TIMEOUT_IGNORED`): they have no positive image in the
/// kind that carries their bits, and a BOZ wider than its kind is not
/// portable — F2008 reads a BOZ as a value, so it overflows, while F2018
/// reads it as a bit pattern. Those are written as the equivalent negative
/// decimal — the same bits — and the second half of the pair is the comment
/// line that says so, so the api.json spelling is never lost.
fn constant_literal(value: &str, kind: &str) -> (String, Option<String>) {
    // `c_int32_t` -> 32. A kind with no width in its name (none occurs
    // today) is assumed to be the widest, which only ever costs a wider
    // two's-complement wrap than needed.
    let bits: u32 = kind
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(64);
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"));
    let parsed = match hex {
        Some(h) => u128::from_str_radix(h, 16).ok(),
        None => value.parse::<u128>().ok(),
    };
    let signed_max = (1u128 << (bits - 1)) - 1;
    match (parsed, hex) {
        (Some(v), _) if v > signed_max => (
            format!("{}_{}", (v as i128) - (1i128 << bits), kind),
            Some(format!(
                "api.json value {}: the same {} bits, spelled the only way a signed kind can hold them.",
                value, bits
            )),
        ),
        (Some(_), Some(h)) => (format!("int(z'{}', {})", h.to_ascii_uppercase(), kind), None),
        (Some(v), None) => (format!("{}_{}", v, kind), None),
        // Not a whole number: pass api.json's own spelling through, so an
        // unspellable value fails the Fortran build loudly instead of
        // disappearing from the binding.
        (None, _) => (value.to_string(), None),
    }
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

/// The instantiations of a generic template that DO cross the C ABI:
/// `CssPropertyValue<T>` is nothing at the boundary, `StyleCursorValue`
/// (= `CssPropertyValue<StyleCursor>`) is a concrete union with a concrete
/// layout. The IR carries each one as a type alias with a monomorphized
/// definition, and [`generate_types_for`] declares them from there.
fn instantiations_of<'a>(ir: &'a CodegenIR, template: &str) -> Vec<&'a str> {
    ir.type_aliases
        .iter()
        .filter(|ta| ta.monomorphized_def.is_some() && ta.target.trim() == template)
        .map(|ta| ta.name.as_str())
        .collect()
}

/// Note why a type carries no declaration of its own.
///
/// A GENERIC TEMPLATE is not a gap. It has no C ABI at all — nothing
/// crosses the boundary as a `CssPropertyValue` or a `PhysicalSize`, only
/// as one of their instantiations — and every one of those IS declared,
/// under its own name, from `ir.type_aliases`. Calling that a skipped item
/// claimed a hole in the binding that was never there. Anything else
/// reaching here is a real gap and still says so.
fn emit_undeclared(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    what: &str,
    name: &str,
    why: &str,
    generic: bool,
) {
    if !generic {
        builder.line(&format!("! SKIPPED: {} {} ({})", what, name, why));
        return;
    }
    let inst = instantiations_of(ir, name);
    if inst.is_empty() {
        builder.line(&format!(
            "! Generic template {} {}: no C ABI of its own, and no instantiation of it crosses the boundary.",
            what, name
        ));
        return;
    }
    // Name the first few so a reader looking for `PhysicalSize` in this
    // file finds the type they actually want.
    let shown: Vec<String> = inst.iter().take(3).map(|n| ffi_type_name(n)).collect();
    builder.line(&format!(
        "! Generic template {} {}: no C ABI of its own. Its {} instantiation(s)",
        what,
        name,
        inst.len()
    ));
    builder.line(&format!(
        "! are declared each under its own name ({}{}).",
        shown.join(", "),
        if inst.len() > shown.len() { ", ..." } else { "" }
    ));
}

fn emit_skipped_struct(builder: &mut CodeBuilder, ir: &CodegenIR, s: &StructDef) {
    emit_undeclared(
        builder,
        ir,
        "struct",
        &s.name,
        s.category.description(),
        !s.generic_params.is_empty() || s.category == TypeCategory::GenericTemplate,
    );
}

/// Emit an ABI-opaque stand-in type: a single array component of the
/// widest integer kind matching the C alignment, sized to the exact C
/// byte size. Field-level access is impossible (Fortran has no unions),
/// but embedding-by-value and pass-by-value are layout-exact — which is
/// all the generated wrappers ever need for these types.
fn emit_opaque_blob(builder: &mut CodeBuilder, name: &str, l: super::layout::AbiLayout, why: &str) {
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

fn emit_skipped_enum(builder: &mut CodeBuilder, ir: &CodegenIR, e: &EnumDef) {
    emit_undeclared(
        builder,
        ir,
        "enum",
        &e.name,
        e.category.description(),
        !e.generic_params.is_empty() || e.category == TypeCategory::GenericTemplate,
    );
}

// ============================================================================
// Unit-only enum (F2003 `enum, bind(C)` block + integer alias)
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("! {}", sanitize_comment_line(d)));
        }
    }

    let alias = ffi_type_name(&e.name);

    if e.variants.is_empty() {
        // Empty enums are illegal in an `enum, bind(C)`; emit just
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

    // F2003 enum block.
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
        format!(
            "subroutine {}({}) bind(C)",
            alias_ifname,
            arg_names.join(", ")
        )
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
                let vname = truncate_identifier(&format!("{}_{}", ffi, sanitize_identifier(v)));
                builder.line(&format!("enumerator :: {} = {}", vname, i));
            }
            builder.dedent();
            builder.line("end enum");
            for v in variants {
                let vname = truncate_identifier(&format!("{}_{}", ffi, sanitize_identifier(v)));
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
                let vname =
                    truncate_identifier(&format!("{}_{}", tag_alias, sanitize_identifier(&v.name)));
                builder.line(&format!("enumerator :: {} = {}", vname, i));
            }
            builder.dedent();
            builder.line("end enum");
            for v in variants {
                let vname =
                    truncate_identifier(&format!("{}_{}", tag_alias, sanitize_identifier(&v.name)));
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
