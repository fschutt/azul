//! Julia type emission: structs, `@enum`s, tagged-union blob structs,
//! callback-pointer aliases and type aliases.
//!
//! Unlike Odin, Julia has no forward declaration for concrete structs, so
//! types are emitted in dependency (`sort_order`) order — the same
//! topological order the IR builder computes for C/C++. Leaves that carry
//! no ordering constraint (callback-pointer aliases and opaque skipped
//! types, all `Ptr{Cvoid}`) are emitted first; the real structs / enums /
//! type-aliases follow in `sort_order`.
//!
//! Tagged unions follow the tested `lang_c` layout: one isbits `struct`
//! per variant (each beginning with the discriminant `tag`), then an
//! `@eval`'d blob `struct` whose size and alignment are computed at module
//! load from the variant structs (`max(sizeof(...))` / `max(alignof(...))`).
//! The blob is isbits and byte-for-byte ABI-compatible with the Rust
//! `#[repr(C, u8)]` union.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, MonomorphizedKind,
    MonomorphizedTypeDef, StructDef, TypeAliasDef,
};
use super::{
    enum_backing, ffi_type_name, field_type_for_ref_kind, include_enum, include_struct,
    map_type_to_julia, sanitize_comment, sanitize_identifier, tag_type,
};

/// A type item to be emitted in `sort_order`.
enum SortedType<'a> {
    Struct(&'a StructDef),
    Enum(&'a EnumDef),
    TypeAlias(&'a TypeAliasDef),
}

pub fn generate_types(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("# ----------------------------------------------------------------------------");
    b.line("# Type definitions (structs, @enums, tagged-union blobs, type aliases).");
    b.line("# Emitted in dependency order — Julia has no forward declarations.");
    b.line("# ----------------------------------------------------------------------------");
    b.blank();

    let mut emitted: BTreeSet<String> = BTreeSet::new();

    // --- Phase 1: leaves (no ordering constraint) ---------------------------
    // Callback function-pointer typedefs -> `const AzXCallbackType = Ptr{Cvoid}`.
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        let name = ffi_type_name(&cb.name);
        if !emitted.insert(name.clone()) {
            continue;
        }
        emit_callback_typedef(b, cb);
    }
    // Opaque skipped structs / enums -> `const AzX = Ptr{Cvoid}`.
    for s in &ir.structs {
        let name = ffi_type_name(&s.name);
        if emitted.contains(&name) {
            continue;
        }
        if !include_struct(s, config) {
            emitted.insert(name.clone());
            emit_opaque(b, &name, &s.name);
        }
    }
    for e in &ir.enums {
        let name = ffi_type_name(&e.name);
        if emitted.contains(&name) {
            continue;
        }
        if !include_enum(e, config) {
            emitted.insert(name.clone());
            emit_opaque(b, &name, &e.name);
        }
    }

    // --- Phase 2: real types, in dependency (sort_order) order --------------
    let mut all: Vec<SortedType> = Vec::new();
    for s in &ir.structs {
        if include_struct(s, config) {
            all.push(SortedType::Struct(s));
        }
    }
    for e in &ir.enums {
        if include_enum(e, config) {
            all.push(SortedType::Enum(e));
        }
    }
    for ta in &ir.type_aliases {
        if config.should_include_type(&ta.name) {
            all.push(SortedType::TypeAlias(ta));
        }
    }
    all.sort_by_key(|t| match t {
        SortedType::Struct(s) => s.sort_order,
        SortedType::Enum(e) => e.sort_order,
        SortedType::TypeAlias(t) => t.sort_order,
    });

    for item in &all {
        match item {
            SortedType::Struct(s) => {
                let name = ffi_type_name(&s.name);
                if !emitted.insert(name.clone()) {
                    continue;
                }
                emit_struct(b, s, ir);
            }
            SortedType::Enum(e) => {
                let name = ffi_type_name(&e.name);
                if !emitted.insert(name.clone()) {
                    continue;
                }
                if e.is_union {
                    emit_tagged_union(b, e, ir);
                } else {
                    emit_simple_enum(b, e);
                }
            }
            SortedType::TypeAlias(ta) => {
                let name = ffi_type_name(&ta.name);
                if !emitted.insert(name.clone()) {
                    continue;
                }
                match &ta.monomorphized_def {
                    Some(mono) => emit_monomorphized_alias(b, ta, mono, ir),
                    None => {
                        let target = map_type_to_julia(&ta.target, ir);
                        b.line(&format!("const {} = {}", name, target));
                        b.blank();
                    }
                }
            }
        }
    }
}

// ============================================================================
// Opaque placeholder (skipped categories) + callback pointer alias
// ============================================================================

fn emit_opaque(b: &mut CodeBuilder, ffi_name: &str, orig: &str) {
    b.line(&format!("const {} = Ptr{{Cvoid}} # opaque: {}", ffi_name, orig));
    b.blank();
}

fn emit_callback_typedef(b: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    for d in &cb.doc {
        b.line(&format!("# {}", sanitize_comment(d)));
    }
    // A C function pointer is `Ptr{Cvoid}` in Julia; the concrete signature
    // is supplied at the `@cfunction` call site by the user.
    b.line(&format!("const {} = Ptr{{Cvoid}}", ffi_type_name(&cb.name)));
    b.blank();
}

// ============================================================================
// Simple enum
// ============================================================================

fn emit_simple_enum(b: &mut CodeBuilder, e: &EnumDef) {
    for d in &e.doc {
        b.line(&format!("# {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let backing = enum_backing(e.repr.as_deref());

    if e.variants.is_empty() {
        // Julia has no zero-variant @enum; degrade to the backing integer.
        b.line(&format!("const {} = {}", name, backing));
        b.blank();
        return;
    }

    // `@enum` injects variant names into module scope, so they are prefixed
    // with the enum name (`AzUpdate_RefreshDom`) to avoid collisions.
    b.line(&format!("@enum {}::{} begin", name, backing));
    for v in &e.variants {
        b.line(&format!("    {}_{}", name, sanitize_identifier(&v.name)));
    }
    b.line("end");
    b.blank();
}

// ============================================================================
// Tagged union (per-variant structs + @eval'd isbits blob)
// ============================================================================

fn emit_tagged_union(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    for d in &e.doc {
        b.line(&format!("# {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let tag_ty = tag_type(e.repr.as_deref());

    if e.variants.is_empty() {
        b.line(&format!("const {} = Ptr{{Cvoid}}", name));
        b.blank();
        return;
    }

    // One isbits struct per variant, each carrying the discriminant first.
    let mut variant_structs: Vec<String> = Vec::new();
    for v in &e.variants {
        let vstruct = format!("{}Variant_{}", name, sanitize_identifier(&v.name));
        b.line(&format!("struct {}", vstruct));
        b.line(&format!("    tag::{}", tag_ty));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                for (j, (ty, rk)) in types.iter().enumerate() {
                    let fty = field_type_for_ref_kind(ty, rk, ir);
                    if types.len() == 1 {
                        b.line(&format!("    payload::{}", fty));
                    } else {
                        b.line(&format!("    payload_{}::{}", j, fty));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!("    {}::{}", sanitize_identifier(&f.name), fty));
                }
            }
        }
        b.line("end");
        variant_structs.push(vstruct);
    }

    emit_union_blob(b, &name, &variant_structs);
    b.blank();
}

/// Emit the `@eval`'d isbits blob whose size / alignment match the C union.
fn emit_union_blob(b: &mut CodeBuilder, name: &str, variant_structs: &[String]) {
    let sizes = variant_structs
        .iter()
        .map(|v| format!("sizeof({})", v))
        .collect::<Vec<_>>()
        .join(", ");
    let aligns = variant_structs
        .iter()
        .map(|v| format!("_az_alignof({})", v))
        .collect::<Vec<_>>()
        .join(", ");
    // `let` with no binding list opens a local scope; the size / alignment
    // are computed as local statements and spliced into the @eval'd struct.
    b.line("let");
    b.line(&format!("    _sz = max({})", sizes));
    b.line(&format!("    _al = max({})", aligns));
    b.line(&format!("    @eval struct {}", name));
    b.line("        _data::NTuple{$(cld(_sz, _al)), $(_az_blob_eltype(_al))}");
    b.line("    end");
    b.line("end");
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    for d in &s.doc {
        b.line(&format!("# {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&s.name);

    if s.fields.is_empty() {
        // Mirror the C header's 1-byte opaque struct so by-value size matches.
        b.line(&format!("struct {}", name));
        b.line("    _dummy::UInt8");
        b.line("end");
        b.blank();
        return;
    }

    b.line(&format!("struct {}", name));
    for f in &s.fields {
        emit_field(b, f, ir);
    }
    b.line("end");
    b.blank();
}

fn emit_field(b: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        b.line(&format!("    # {}", sanitize_comment(doc)));
    }
    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    b.line(&format!("    {}::{}", sanitize_identifier(&f.name), fty));
}

// ============================================================================
// Monomorphized type aliases (OptionU32, PhysicalSizeU32, ...)
// ============================================================================

fn emit_monomorphized_alias(
    b: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    for d in &ta.doc {
        b.line(&format!("# {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&ta.name);

    match &mono.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let backing = enum_backing(repr.as_deref());
            if variants.is_empty() {
                b.line(&format!("const {} = {}", name, backing));
                b.blank();
                return;
            }
            b.line(&format!("@enum {}::{} begin", name, backing));
            for v in variants {
                b.line(&format!("    {}_{}", name, sanitize_identifier(v)));
            }
            b.line("end");
            b.blank();
        }
        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
                b.line(&format!("struct {}", name));
                b.line("    _dummy::UInt8");
                b.line("end");
                b.blank();
                return;
            }
            b.line(&format!("struct {}", name));
            for f in fields {
                emit_field(b, f, ir);
            }
            b.line("end");
            b.blank();
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let tag_ty = tag_type(repr.as_deref());
            if variants.is_empty() {
                b.line(&format!("const {} = Ptr{{Cvoid}}", name));
                b.blank();
                return;
            }
            let mut variant_structs: Vec<String> = Vec::new();
            for v in variants {
                let vstruct = format!("{}Variant_{}", name, sanitize_identifier(&v.name));
                b.line(&format!("struct {}", vstruct));
                b.line(&format!("    tag::{}", tag_ty));
                if let Some(payload_ty) = &v.payload_type {
                    let fty = field_type_for_ref_kind(payload_ty, &v.payload_ref_kind, ir);
                    b.line(&format!("    payload::{}", fty));
                }
                b.line("end");
                variant_structs.push(vstruct);
            }
            emit_union_blob(b, &name, &variant_structs);
            b.blank();
        }
    }
}
