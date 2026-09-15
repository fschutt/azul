//! The raw layer's types: every api.json struct, enum, tagged union, callback
//! typedef and alias as the `extern(C)` declaration libazul's C header has.
//!
//! D lays out a `struct` exactly like C (natural alignment, no reordering) and
//! resolves module-scope names in any order, so a plain walk is enough for
//! correctness; declarations are still emitted in the IR's topological
//! `sort_order`, the order `azul.h` uses, so the two files read side by side.
//!
//! Tagged unions follow the tested `lang_c` layout: one struct per variant,
//! each beginning with the discriminant `tag`, collected into a `union`.
//!
//! Only generic templates stay opaque (`alias AzCssPropertyValue = void*;`):
//! they have no single layout. `Recursive` types are real structs - they
//! recurse only through a Vec pointer, and as opaque aliases they shrank every
//! type embedding them by value (`AzOptionXmlNodeChild` 16 bytes vs 136 in C).
//!
//! No doc comments are emitted here: the idiomatic declarations carry them.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, MonomorphizedKind,
            MonomorphizedTypeDef, StructDef, TypeAliasDef,
        },
    },
    ffi_type_name, field_type_for_ref_kind, include_enum, include_struct, map_type_to_d,
    raw_identifier,
};

pub fn generate_types(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &mut BTreeSet<String>,
) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// C ABI types: structs, enums, tagged unions, callback typedefs, aliases.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    enum Decl<'a> {
        Enum(&'a EnumDef),
        Struct(&'a StructDef),
        Alias(&'a TypeAliasDef),
        Callback(&'a CallbackTypedefDef),
    }
    // The per-category walk only decides which declaration wins a name clash
    // (enum, struct, alias, callback); emission order is `sort_order`.
    let mut decls: Vec<(usize, Decl)> = Vec::new();
    for e in &ir.enums {
        if emitted.insert(ffi_type_name(&e.name)) {
            decls.push((e.sort_order, Decl::Enum(e)));
        }
    }
    for s in &ir.structs {
        if emitted.insert(ffi_type_name(&s.name)) {
            decls.push((s.sort_order, Decl::Struct(s)));
        }
    }
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        if emitted.insert(ffi_type_name(&ta.name)) {
            decls.push((ta.sort_order, Decl::Alias(ta)));
        }
    }
    for cb in &ir.callback_typedefs {
        if emitted.insert(ffi_type_name(&cb.name)) {
            decls.push((cb.sort_order, Decl::Callback(cb)));
        }
    }
    decls.sort_by_key(|(order, _)| *order);

    for (_, decl) in decls {
        match decl {
            Decl::Enum(e) => {
                if !include_enum(e, config) {
                    emit_opaque(b, &ffi_type_name(&e.name));
                } else if e.is_union {
                    emit_tagged_union(b, e, ir);
                } else {
                    emit_simple_enum(b, e);
                }
            }
            Decl::Struct(s) => {
                if !include_struct(s, config) {
                    emit_opaque(b, &ffi_type_name(&s.name));
                } else {
                    emit_struct(b, s, ir);
                }
            }
            Decl::Alias(ta) => match &ta.monomorphized_def {
                Some(mono) => emit_monomorphized_alias(b, ta, mono, ir),
                None => {
                    let target = map_type_to_d(&ta.target, ir);
                    b.line(&format!("alias {} = {};", ffi_type_name(&ta.name), target));
                }
            },
            Decl::Callback(cb) => emit_callback_typedef(b, cb, ir),
        }
    }
    b.blank();
}

fn emit_opaque(b: &mut CodeBuilder, ffi_name: &str) {
    // A generic template: only ever reached through a pointer.
    b.line(&format!("alias {} = void*;", ffi_name));
}

fn emit_simple_enum(b: &mut CodeBuilder, e: &EnumDef) {
    let name = ffi_type_name(&e.name);
    let backing = enum_backing(e.repr.as_deref());
    if e.variants.is_empty() {
        b.line(&format!("alias {} = {};", name, backing));
        return;
    }
    b.line(&format!("enum {} : {}", name, backing));
    b.line("{");
    for v in &e.variants {
        b.line(&format!("    {},", raw_identifier(&v.name)));
    }
    b.line("}");
}

fn emit_tagged_union(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let name = ffi_type_name(&e.name);
    let tag_ty = tag_type(e.repr.as_deref());
    for v in &e.variants {
        let vstruct = format!("{}Variant_{}", name, raw_identifier(&v.name));
        b.line(&format!("struct {}", vstruct));
        b.line("{");
        b.line(&format!("    {} tag;", tag_ty));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                for (j, (ty, rk)) in types.iter().enumerate() {
                    let fty = field_type_for_ref_kind(ty, rk, ir);
                    if types.len() == 1 {
                        b.line(&format!("    {} payload;", fty));
                    } else {
                        b.line(&format!("    {} payload_{};", fty, j));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!("    {} {};", fty, raw_identifier(&f.name)));
                }
            }
        }
        b.line("}");
    }
    b.line(&format!("union {}", name));
    b.line("{");
    for v in &e.variants {
        let vname = raw_identifier(&v.name);
        b.line(&format!("    {}Variant_{} {};", name, vname, vname));
    }
    b.line("}");
}

fn emit_struct(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = ffi_type_name(&s.name);
    b.line(&format!("struct {}", name));
    b.line("{");
    if s.fields.is_empty() {
        // The C header's 1-byte stand-in, so by-value sizes agree.
        b.line("    ubyte _dummy;");
    }
    for f in &s.fields {
        emit_field(b, f, ir);
    }
    b.line("}");
}

fn emit_field(b: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    b.line(&format!("    {} {};", fty, raw_identifier(&f.name)));
}

fn emit_callback_typedef(b: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    let name = ffi_type_name(&cb.name);
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
        "alias {} = extern (C) {} function({});",
        name,
        ret,
        params.join(", ")
    ));
}

fn emit_monomorphized_alias(
    b: &mut CodeBuilder,
    ta: &TypeAliasDef,
    mono: &MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    let name = ffi_type_name(&ta.name);
    match &mono.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let backing = enum_backing(repr.as_deref());
            if variants.is_empty() {
                b.line(&format!("alias {} = {};", name, backing));
                return;
            }
            b.line(&format!("enum {} : {}", name, backing));
            b.line("{");
            for v in variants {
                b.line(&format!("    {},", raw_identifier(v)));
            }
            b.line("}");
        }
        MonomorphizedKind::Struct { fields } => {
            b.line(&format!("struct {}", name));
            b.line("{");
            if fields.is_empty() {
                b.line("    ubyte _dummy;");
            }
            for f in fields {
                emit_field(b, f, ir);
            }
            b.line("}");
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let tag_ty = tag_type(repr.as_deref());
            for v in variants {
                let vstruct = format!("{}Variant_{}", name, raw_identifier(&v.name));
                b.line(&format!("struct {}", vstruct));
                b.line("{");
                b.line(&format!("    {} tag;", tag_ty));
                if let Some(payload_ty) = &v.payload_type {
                    let fty = field_type_for_ref_kind(payload_ty, &v.payload_ref_kind, ir);
                    b.line(&format!("    {} payload;", fty));
                }
                b.line("}");
            }
            b.line(&format!("union {}", name));
            b.line("{");
            for v in variants {
                let vname = raw_identifier(&v.name);
                b.line(&format!("    {}Variant_{} {};", name, vname, vname));
            }
            b.line("}");
        }
    }
}

/// Backing integer of a fieldless enum: `#[repr(C, u8)]` -> `ubyte`, a plain
/// `#[repr(C)]` enum is C-`int` sized.
pub fn enum_backing(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "ubyte",
        Some(r) if r.contains("u16") => "ushort",
        Some(r) if r.contains("u32") => "uint",
        Some(r) if r.contains("i64") || r.contains("u64") => "long",
        _ => "int",
    }
}

/// Discriminant field of a tagged union: `#[repr(C, u8)]` has a 1-byte tag, a
/// plain `#[repr(C)]` data enum a C-`int` one.
pub fn tag_type(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "ubyte",
        Some(r) if r.contains("u16") => "ushort",
        Some(r) if r.contains("u32") => "uint",
        _ => "int",
    }
}
