//! D type emission: structs, enums, tagged unions (`union`), callback
//! fn-pointer aliases, and type aliases.
//!
//! D module-scope declarations are order-independent, so we simply walk
//! the IR and emit each type once. Skipped categories (recursive /
//! generic template) get an opaque `alias AzName = void*;` so any
//! by-pointer reference still resolves, mirroring the Odin backend.
//!
//! Tagged unions follow the tested `lang_c` layout exactly: one struct
//! per variant, each beginning with the discriminant `tag` field,
//! collected into a `union`. Because every variant struct shares the same
//! first `tag` at offset 0 and D uses C struct / union layout (natural
//! alignment, no field reordering — D's default aggregate layout is
//! C-compatible), the size and alignment match Rust's `#[repr(C, u8)]`
//! enums.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, MonomorphizedKind, MonomorphizedTypeDef,
    StructDef, TypeAliasDef,
};
use super::{
    ffi_type_name, field_type_for_ref_kind, include_enum, include_struct, map_type_to_d,
    sanitize_identifier,
};

pub fn generate_types(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &mut BTreeSet<String>,
) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Type definitions (structs, enums, tagged unions, callback fn-pointer aliases).");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    // Enums (simple + tagged unions).
    for e in &ir.enums {
        let name = ffi_type_name(&e.name);
        if !emitted.insert(name.clone()) {
            continue;
        }
        if !include_enum(e, config) {
            emit_opaque(b, &name, &e.name);
            continue;
        }
        if e.is_union {
            emit_tagged_union(b, e, ir);
        } else {
            emit_simple_enum(b, e);
        }
    }

    // Structs (POD records + callback-wrapper pairs).
    for s in &ir.structs {
        let name = ffi_type_name(&s.name);
        if !emitted.insert(name.clone()) {
            continue;
        }
        if !include_struct(s, config) {
            emit_opaque(b, &name, &s.name);
            continue;
        }
        emit_struct(b, s, ir);
    }

    // Type aliases: monomorphized generics become concrete records /
    // enums; simple aliases become `alias AzName = <target>;`.
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        let name = ffi_type_name(&ta.name);
        if !emitted.insert(name.clone()) {
            continue;
        }
        match &ta.monomorphized_def {
            Some(mono) => emit_monomorphized_alias(b, ta, mono, ir),
            None => {
                let target = map_type_to_d(&ta.target, ir);
                b.line(&format!("alias {} = {};", name, target));
                b.blank();
            }
        }
    }

    // Callback typedefs -> D fn-pointer aliases (C function pointers).
    for cb in &ir.callback_typedefs {
        let name = ffi_type_name(&cb.name);
        if !emitted.insert(name.clone()) {
            continue;
        }
        emit_callback_typedef(b, cb, ir);
    }
}

// ============================================================================
// Opaque placeholder (skipped categories)
// ============================================================================

fn emit_opaque(b: &mut CodeBuilder, ffi_name: &str, orig: &str) {
    // Pointer-sized alias so by-pointer references still resolve (the type
    // is only ever reached through a `Box`/pointer in these categories).
    b.line(&format!("alias {} = void*; // opaque: {}", ffi_name, orig));
    b.blank();
}

// ============================================================================
// Simple enum
// ============================================================================

fn emit_simple_enum(b: &mut CodeBuilder, e: &EnumDef) {
    for d in &e.doc {
        b.line(&format!("// {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let backing = enum_backing(e.repr.as_deref());

    if e.variants.is_empty() {
        // D has no zero-variant enum; degrade to the backing integer.
        b.line(&format!("alias {} = {};", name, backing));
        b.blank();
        return;
    }

    b.line(&format!("enum {} : {} {{", name, backing));
    for v in &e.variants {
        b.line(&format!("\t{},", sanitize_identifier(&v.name)));
    }
    b.line("}");
    b.blank();
}

// ============================================================================
// Tagged union (union of per-variant structs)
// ============================================================================

fn emit_tagged_union(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    for d in &e.doc {
        b.line(&format!("// {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let tag_ty = tag_type(e.repr.as_deref());

    // One struct per variant, each carrying the discriminant first.
    for v in &e.variants {
        let vstruct = format!("{}Variant_{}", name, sanitize_identifier(&v.name));
        b.line(&format!("struct {} {{", vstruct));
        b.line(&format!("\t{} tag;", tag_ty));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                for (j, (ty, rk)) in types.iter().enumerate() {
                    let fty = field_type_for_ref_kind(ty, rk, ir);
                    if types.len() == 1 {
                        b.line(&format!("\t{} payload;", fty));
                    } else {
                        b.line(&format!("\t{} payload_{};", fty, j));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!("\t{} {};", fty, sanitize_identifier(&f.name)));
                }
            }
        }
        b.line("}");
    }

    // The union itself: one field per variant struct.
    b.line(&format!("union {} {{", name));
    for v in &e.variants {
        let vname = sanitize_identifier(&v.name);
        b.line(&format!("\t{}Variant_{} {};", name, sanitize_identifier(&v.name), vname));
    }
    b.line("}");
    b.blank();
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    for d in &s.doc {
        b.line(&format!("// {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&s.name);

    if s.fields.is_empty() {
        // Mirror the C header's 1-byte opaque struct so by-value size
        // matches the generated `azul.h`.
        b.line(&format!("struct {} {{", name));
        b.line("\tubyte _dummy;");
        b.line("}");
        b.blank();
        return;
    }

    b.line(&format!("struct {} {{", name));
    for f in &s.fields {
        emit_field(b, f, ir);
    }
    b.line("}");
    b.blank();
}

fn emit_field(b: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        b.line(&format!("\t// {}", sanitize_comment(doc)));
    }
    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    b.line(&format!("\t{} {};", fty, sanitize_identifier(&f.name)));
}

// ============================================================================
// Callback typedef -> D fn-pointer alias
// ============================================================================

fn emit_callback_typedef(
    b: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    for d in &cb.doc {
        b.line(&format!("// {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&cb.name);

    // Unnamed parameter types keep fn-pointer declarations free of
    // keyword-collision worries.
    let params: Vec<String> = cb
        .args
        .iter()
        .map(|a| super::arg_type_for_ref_kind(&a.type_name, &a.ref_kind, ir))
        .collect();

    let ret = cb
        .return_type
        .as_ref()
        .map(|r| map_type_to_d(r, ir))
        .unwrap_or_else(|| "void".to_string());

    b.line(&format!(
        "alias {} = extern(C) {} function({});",
        name,
        ret,
        params.join(", ")
    ));
    b.blank();
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
        b.line(&format!("// {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&ta.name);

    match &mono.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let backing = enum_backing(repr.as_deref());
            if variants.is_empty() {
                b.line(&format!("alias {} = {};", name, backing));
                b.blank();
                return;
            }
            b.line(&format!("enum {} : {} {{", name, backing));
            for v in variants {
                b.line(&format!("\t{},", sanitize_identifier(v)));
            }
            b.line("}");
            b.blank();
        }
        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
                b.line(&format!("struct {} {{", name));
                b.line("\tubyte _dummy;");
                b.line("}");
                b.blank();
                return;
            }
            b.line(&format!("struct {} {{", name));
            for f in fields {
                emit_field(b, f, ir);
            }
            b.line("}");
            b.blank();
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let tag_ty = tag_type(repr.as_deref());
            for v in variants {
                let vstruct = format!("{}Variant_{}", name, sanitize_identifier(&v.name));
                b.line(&format!("struct {} {{", vstruct));
                b.line(&format!("\t{} tag;", tag_ty));
                if let Some(payload_ty) = &v.payload_type {
                    let fty = field_type_for_ref_kind(payload_ty, &v.payload_ref_kind, ir);
                    b.line(&format!("\t{} payload;", fty));
                }
                b.line("}");
            }
            b.line(&format!("union {} {{", name));
            for v in variants {
                let vname = sanitize_identifier(&v.name);
                b.line(&format!(
                    "\t{}Variant_{} {};",
                    name,
                    sanitize_identifier(&v.name),
                    vname
                ));
            }
            b.line("}");
            b.blank();
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Backing integer for a simple enum. Rust `#[repr(C, u8)]` -> `ubyte`,
/// otherwise a `#[repr(C)]` enum is C-`int` sized -> `int`. (D's default
/// enum backing is `int`, which is right for the C-`int` case but we make
/// the width explicit for the sized reprs.)
fn enum_backing(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "ubyte",
        Some(r) if r.contains("u16") => "ushort",
        Some(r) if r.contains("u32") => "uint",
        Some(r) if r.contains("i64") || r.contains("u64") => "long",
        _ => "int",
    }
}

/// Discriminant field width for a tagged union. Rust `#[repr(C, u8)]`
/// tagged unions use a 1-byte tag; a plain `#[repr(C)]` data enum uses a
/// C-`int` tag.
fn tag_type(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "ubyte",
        Some(r) if r.contains("u16") => "ushort",
        Some(r) if r.contains("u32") => "uint",
        _ => "int",
    }
}

/// Strip characters that would break a D `//` line comment.
fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
