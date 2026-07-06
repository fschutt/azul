//! Nim `object` / `enum` / tagged-union / callback-typedef emission.
//!
//! Everything is emitted INSIDE a single `type` block opened by the
//! parent `mod.rs` (Nim resolves forward references within one `type`
//! section, so declaration order never matters). Each declaration is
//! indented one level under the `type` keyword.
//!
//! Strategy:
//!
//! - **Unit (simple) enums** -> `AzFoo* {.size: 4.} = enum` with each
//!   member pinned to its explicit ordinal (`AzFoo_A = 0`). A C `enum`
//!   is `int`-wide (4 bytes) — matching Rust `#[repr(C)]` — so we pin the
//!   size at 4. Members carry the full `AzFoo_` prefix so they never
//!   clash across enums.
//! - **Tagged-union enums** -> one `{.bycopy.} object` per variant, each
//!   starting with a `tag*: uint8` field (mirroring the C header's
//!   `uint8_t tag;`) followed by the payload fields, grouped under a
//!   `{.union.} object` whose members are named after the variants. This
//!   is a byte-for-byte match of the C-API layout the prebuilt `libazul`
//!   was compiled against.
//! - **POD structs** -> `{.bycopy.} object` with fields resolved via
//!   `map_type_to_nim`. Empty (opaque) structs become a field-less
//!   `object` (Nim allows this; such types are only ever held by pointer
//!   or moved by value as an opaque blob).
//! - **Callback typedefs** -> `AzFooCallbackType* = proc (...): Ret
//!   {.cdecl.}` proc-pointer types.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** are
//!   skipped with a `# SKIPPED: <reason>` comment.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    StructDef,
};
use super::{
    ffi_type_name, map_type_to_nim, ptr_type_for, sanitize_comment, sanitize_identifier,
};

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("# --- Unit enums ---------------------------------------------------------");
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    builder.line("# --- Callback function-pointer typedefs ---------------------------------");
    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, ir);
    }

    builder.line("# --- POD records --------------------------------------------------------");
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        emit_struct(builder, s, ir);
    }

    builder.line("# --- Tagged unions ------------------------------------------------------");
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            emit_tagged_union(builder, e, ir);
        }
    }

    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

// NOTE ON FILTERING: unlike the Python / FreeBASIC backends we do NOT skip
// Recursive / VecRef / DestructorOrClone structs. If an *included* type has
// a field (or an included function has an argument) of such a type, skipping
// it would leave an undefined `AzFoo` identifier and break the whole
// `azul.nim` compile. Emitting them is safe: every azul `#[repr(C)]`
// recursion goes through a pointer (Vec/Box), so there is never a by-value
// type cycle for Nim to reject. We only skip genuine generic *templates*
// (`generic_params` non-empty) — those are never referenced by name; code
// references their concrete monomorphizations (e.g. `OptionDom`) instead.

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    s.generic_params.is_empty()
}

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    e.generic_params.is_empty()
}

fn emit_skipped_struct(builder: &mut CodeBuilder, s: &StructDef) {
    builder.line(&format!(
        "# SKIPPED: struct {} ({})",
        s.name,
        s.category.description()
    ));
}

fn emit_skipped_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    builder.line(&format!(
        "# SKIPPED: enum {} ({})",
        e.name,
        e.category.description()
    ));
}

// ============================================================================
// Unit-only enum
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    for d in &e.doc {
        builder.line(&format!("# {}", sanitize_comment(d)));
    }

    let t = ffi_type_name(&e.name);
    if e.variants.is_empty() {
        // A field-less enum is invalid in Nim; alias to the wire integer.
        builder.line(&format!("{}* = uint32", t));
        return;
    }

    // A C `enum` is `int`-wide (4 bytes) under every mainstream ABI, which
    // is what the prebuilt libazul was compiled against. Pin the size so
    // Nim agrees regardless of how it would otherwise pack the enum.
    builder.line(&format!("{}* {{.size: 4.}} = enum", t));
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        // Fully-prefixed member (`AzFoo_Bar`) — one compound identifier, so
        // the raw variant name is used verbatim (it is never a bare keyword
        // here) and the prefix keeps it unique across every enum.
        builder.line(&format!("{}_{} = {}", t, v.name, i));
    }
    builder.dedent();
}

// ============================================================================
// Tagged-union enum (per-variant tag+payload structs + {.union.} object)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    for d in &e.doc {
        builder.line(&format!("# {}", sanitize_comment(d)));
    }

    let t = ffi_type_name(&e.name);

    // 1. One `{tag: uint8, payload…}` struct per variant.
    for v in &e.variants {
        // `AzFooVariant_Bar` is a single compound identifier -> raw name.
        let variant_ty = format!("{}Variant_{}", t, v.name);
        builder.line(&format!("{}* {{.bycopy.}} = object", variant_ty));
        builder.indent();
        // Every variant leads with the discriminant byte (matches the
        // C header's `uint8_t tag;`).
        builder.line("tag*: uint8");
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                if types.len() == 1 {
                    let (ty, rk) = &types[0];
                    builder.line(&format!("payload*: {}", field_type_for_ref_kind(ty, rk, ir)));
                } else {
                    for (i, (ty, rk)) in types.iter().enumerate() {
                        builder.line(&format!(
                            "payload{}*: {}",
                            i,
                            field_type_for_ref_kind(ty, rk, ir)
                        ));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    builder.line(&format!(
                        "{}*: {}",
                        sanitize_identifier(&f.name),
                        field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir)
                    ));
                }
            }
        }
        builder.dedent();
    }

    // 2. The `{.union.} object` overlaying all variants.
    builder.line(&format!("{}* {{.bycopy, union.}} = object", t));
    builder.indent();
    for v in &e.variants {
        // The union member is a *bare* identifier -> sanitize (a variant
        // named e.g. `Ref` would collide with the Nim keyword). The variant
        // type it points at is the compound name -> raw.
        let member = sanitize_identifier(&v.name);
        let variant_ty = format!("{}Variant_{}", t, v.name);
        builder.line(&format!("{}*: {}", member, variant_ty));
    }
    builder.dedent();
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    for d in &s.doc {
        builder.line(&format!("# {}", sanitize_comment(d)));
    }

    let t = ffi_type_name(&s.name);

    if s.fields.is_empty() {
        // Opaque type — a field-less Nim object. Such types are only ever
        // held via `ptr AzFoo` or moved as an opaque by-value blob.
        builder.line(&format!("{}* {{.bycopy.}} = object", t));
        return;
    }

    builder.line(&format!("{}* {{.bycopy.}} = object", t));
    builder.indent();
    for f in &s.fields {
        emit_struct_field(builder, f, ir);
    }
    builder.dedent();
}

fn emit_struct_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("# {}", sanitize_comment(doc)));
    }
    let nm = sanitize_identifier(&f.name);
    let nim_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("{}*: {}", nm, nim_ty));
}

// ============================================================================
// Callback typedef (Nim {.cdecl.} proc type)
// ============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    for d in &cb.doc {
        builder.line(&format!("# {}", sanitize_comment(d)));
    }
    let t = ffi_type_name(&cb.name);

    let args: Vec<String> = cb
        .args
        .iter()
        .map(|arg| {
            let nim_ty = arg_type(arg.ref_kind, &arg.type_name, ir);
            format!("{}: {}", sanitize_identifier(&arg.name), nim_ty)
        })
        .collect();

    match &cb.return_type {
        Some(ret) if ret.trim() != "void" && ret.trim() != "()" => {
            let nim_ret = map_type_to_nim(ret, ir);
            builder.line(&format!(
                "{}* = proc ({}): {} {{.cdecl.}}",
                t,
                args.join(", "),
                nim_ret
            ));
        }
        _ => {
            builder.line(&format!("{}* = proc ({}) {{.cdecl.}}", t, args.join(", ")));
        }
    }
}

// ============================================================================
// Field/argument type helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the Nim field type.
pub(crate) fn field_type_for_ref_kind(
    type_name: &str,
    ref_kind: &FieldRefKind,
    ir: &CodegenIR,
) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_nim(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => ptr_type_for(type_name, ir),
    }
}

/// Map a `(ArgRefKind, type_name)` pair to the Nim argument type.
pub(crate) fn arg_type(ref_kind: ArgRefKind, type_name: &str, ir: &CodegenIR) -> String {
    match ref_kind {
        ArgRefKind::Owned => map_type_to_nim(type_name, ir),
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            ptr_type_for(type_name, ir)
        }
    }
}
