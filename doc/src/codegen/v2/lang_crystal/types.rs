//! Crystal type emission: structs, enums, tagged unions (`union`),
//! callback proc-type aliases, and type aliases. All declarations live
//! inside the `lib LibAzul` block, so they are emitted at one indentation
//! level (two leading spaces).
//!
//! Crystal resolves `lib`-scope NAMES regardless of order, but it does NOT
//! lay structs out correctly regardless of order: a struct embedding by
//! value a type declared later in the file gets a wrong size/offsets. So
//! every declaration is emitted in the IR's topological `sort_order`, the
//! same order the C header uses. Skipped categories (recursive / generic
//! template) get an opaque `alias AzName = Void*` so any by-pointer
//! reference still resolves, mirroring the Odin backend.
//!
//! Tagged unions follow the tested `lang_c` layout exactly: one struct
//! per variant, each beginning with the discriminant `tag` field,
//! collected into a Crystal `union`. Because every variant struct shares
//! the same first `tag` at offset 0 and Crystal uses C struct / union
//! layout (natural alignment, no field reordering), the size and
//! alignment match Rust's `#[repr(C, u8)]` enums.

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
    enum_member_name, ffi_type_name, field_type_for_ref_kind, include_enum, include_struct,
    map_type_to_crystal, sanitize_identifier,
};

pub fn generate_types(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &mut BTreeSet<String>,
) {
    b.line("  # --------------------------------------------------------------------------");
    b.line("  # Type definitions (structs, enums, tagged unions, callback proc-types).");
    b.line("  # --------------------------------------------------------------------------");
    b.blank();

    // ONE list in dependency order. Crystal lays a `lib` struct out lazily,
    // and a struct that embeds by value a type declared LATER in the file
    // gets the wrong size and field offsets (measured: 1470 of 5633 types,
    // e.g. `AzAppConfig` 2392 bytes vs 2400 in C, which made
    // `AzAppConfig_create` write past its slot into the neighbouring RefAny).
    // The IR already carries the topological `sort_order` the C header uses,
    // so emit in exactly that order. The per-category walk below only picks
    // which declaration wins a name clash (enum, struct, alias, callback -
    // the historic precedence); the emission order is `sort_order`.
    enum Decl<'a> {
        Enum(&'a EnumDef),
        Struct(&'a StructDef),
        Alias(&'a TypeAliasDef),
        Callback(&'a CallbackTypedefDef),
    }
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
                    emit_opaque(b, &ffi_type_name(&e.name), &e.name);
                } else if e.is_union {
                    emit_tagged_union(b, e, ir);
                } else {
                    emit_simple_enum(b, e);
                }
            }
            Decl::Struct(s) => {
                if !include_struct(s, config) {
                    emit_opaque(b, &ffi_type_name(&s.name), &s.name);
                } else {
                    emit_struct(b, s, ir);
                }
            }
            Decl::Alias(ta) => match &ta.monomorphized_def {
                Some(mono) => emit_monomorphized_alias(b, ta, mono, ir),
                None => {
                    let target = map_type_to_crystal(&ta.target, ir);
                    b.line(&format!("  alias {} = {}", ffi_type_name(&ta.name), target));
                    b.blank();
                }
            },
            Decl::Callback(cb) => emit_callback_typedef(b, cb, ir),
        }
    }
}

// ============================================================================
// Opaque placeholder (skipped categories)
// ============================================================================

fn emit_opaque(b: &mut CodeBuilder, ffi_name: &str, orig: &str) {
    // Pointer-sized alias so by-pointer references still resolve (the type
    // is only ever reached through a `Box`/pointer in these categories).
    b.line(&format!("  alias {} = Void* # opaque: {}", ffi_name, orig));
    b.blank();
}

// ============================================================================
// Simple enum
// ============================================================================

fn emit_simple_enum(b: &mut CodeBuilder, e: &EnumDef) {
    for d in &e.doc {
        b.line(&format!("  # {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let backing = enum_backing(e.repr.as_deref());

    if e.variants.is_empty() {
        // Crystal has no zero-member enum; degrade to the backing integer.
        b.line(&format!("  alias {} = {}", name, backing));
        b.blank();
        return;
    }

    b.line(&format!("  enum {} : {}", name, backing));
    for v in &e.variants {
        b.line(&format!("    {}", enum_member_name(&v.name)));
    }
    b.line("  end");
    b.blank();
}

// ============================================================================
// Tagged union (Crystal `union` of per-variant structs)
// ============================================================================

fn emit_tagged_union(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    for d in &e.doc {
        b.line(&format!("  # {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&e.name);
    let tag_ty = tag_type(e.repr.as_deref());

    // One struct per variant, each carrying the discriminant first.
    for v in &e.variants {
        let vstruct = format!("{}Variant_{}", name, enum_member_name(&v.name));
        b.line(&format!("  struct {}", vstruct));
        b.line(&format!("    tag : {}", tag_ty));
        match &v.kind {
            EnumVariantKind::Unit => {}
            EnumVariantKind::Tuple(types) => {
                for (j, (ty, rk)) in types.iter().enumerate() {
                    let fty = field_type_for_ref_kind(ty, rk, ir);
                    if types.len() == 1 {
                        b.line(&format!("    payload : {}", fty));
                    } else {
                        b.line(&format!("    payload_{} : {}", j, fty));
                    }
                }
            }
            EnumVariantKind::Struct(fields) => {
                for f in fields {
                    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                    b.line(&format!("    {} : {}", sanitize_identifier(&f.name), fty));
                }
            }
        }
        b.line("  end");
    }

    // The union itself: one field per variant struct.
    b.line(&format!("  union {}", name));
    for v in &e.variants {
        let vname = enum_member_name(&v.name);
        // Union field name is a lowercase identifier; the variant struct
        // name (constant) is capitalized.
        b.line(&format!(
            "    {} : {}Variant_{}",
            sanitize_identifier(&union_field_name(&v.name)),
            name,
            vname
        ));
    }
    b.line("  end");
    b.blank();
}

/// Union member (field) names must be lowercase-initial in Crystal.
fn union_field_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            for c in first.to_lowercase() {
                out.push(c);
            }
            out.push_str(chars.as_str());
            out
        }
        None => name.to_string(),
    }
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    for d in &s.doc {
        b.line(&format!("  # {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&s.name);

    if s.fields.is_empty() {
        // Mirror the C header's 1-byte opaque struct so by-value size
        // matches the generated `azul.h` (Crystal lib structs need >=1
        // field).
        b.line(&format!("  struct {}", name));
        b.line("    dummy : UInt8");
        b.line("  end");
        b.blank();
        return;
    }

    b.line(&format!("  struct {}", name));
    for f in &s.fields {
        emit_field(b, f, ir);
    }
    b.line("  end");
    b.blank();
}

fn emit_field(b: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        b.line(&format!("    # {}", sanitize_comment(doc)));
    }
    let fty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    b.line(&format!("    {} : {}", sanitize_identifier(&f.name), fty));
}

// ============================================================================
// Callback typedef -> Crystal proc type alias
// ============================================================================

fn emit_callback_typedef(b: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    for d in &cb.doc {
        b.line(&format!("  # {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&cb.name);

    // Pointer args to azul types are spelled `Void*` here, never `AzFoo*`.
    //
    // Crystal lays `lib` structs out lazily, on first use, and a proc type
    // naming `AzFoo*` makes it build `AzFoo` while it is still in the middle
    // of building whatever contains the proc. Every `*VecDestructor` is such
    // a cycle - `AzStringVec` embeds `AzStringVecDestructor`, whose
    // `External` variant holds `(AzStringVec*) -> Void` - and it came out
    // 1 byte instead of 16, shrinking 1470 of 5633 types, depending only on
    // which type a program happened to touch first. Declaration order cannot
    // fix that; `Void*` breaks the cycle, and a function pointer's pointer
    // args are all the same size to the C ABI anyway. The idiomatic layer's
    // trampolines cast them back to the typed pointer.
    let params: Vec<String> = cb
        .args
        .iter()
        .map(|a| {
            let ty = super::arg_type_for_ref_kind(&a.type_name, &a.ref_kind, ir);
            if ty.starts_with("Az") && ty.ends_with('*') {
                "Void*".to_string()
            } else {
                ty
            }
        })
        .collect();

    // Crystal proc types always name a return type; void -> `Void`.
    let ret = cb
        .return_type
        .as_ref()
        .map(|r| map_type_to_crystal(r, ir))
        .unwrap_or_else(|| "Void".to_string());

    b.line(&format!(
        "  alias {} = ({}) -> {}",
        name,
        params.join(", "),
        ret
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
        b.line(&format!("  # {}", sanitize_comment(d)));
    }
    let name = ffi_type_name(&ta.name);

    match &mono.kind {
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let backing = enum_backing(repr.as_deref());
            if variants.is_empty() {
                b.line(&format!("  alias {} = {}", name, backing));
                b.blank();
                return;
            }
            b.line(&format!("  enum {} : {}", name, backing));
            for v in variants {
                b.line(&format!("    {}", enum_member_name(v)));
            }
            b.line("  end");
            b.blank();
        }
        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
                b.line(&format!("  struct {}", name));
                b.line("    dummy : UInt8");
                b.line("  end");
                b.blank();
                return;
            }
            b.line(&format!("  struct {}", name));
            for f in fields {
                emit_field(b, f, ir);
            }
            b.line("  end");
            b.blank();
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let tag_ty = tag_type(repr.as_deref());
            for v in variants {
                let vstruct = format!("{}Variant_{}", name, enum_member_name(&v.name));
                b.line(&format!("  struct {}", vstruct));
                b.line(&format!("    tag : {}", tag_ty));
                if let Some(payload_ty) = &v.payload_type {
                    let fty = field_type_for_ref_kind(payload_ty, &v.payload_ref_kind, ir);
                    b.line(&format!("    payload : {}", fty));
                }
                b.line("  end");
            }
            b.line(&format!("  union {}", name));
            for v in variants {
                let vname = enum_member_name(&v.name);
                b.line(&format!(
                    "    {} : {}Variant_{}",
                    sanitize_identifier(&union_field_name(&v.name)),
                    name,
                    vname
                ));
            }
            b.line("  end");
            b.blank();
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Backing integer for a simple enum. Rust `#[repr(C, u8)]` -> `UInt8`,
/// otherwise a `#[repr(C)]` enum is C-`int` sized -> `Int32`.
fn enum_backing(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "UInt8",
        Some(r) if r.contains("u16") => "UInt16",
        Some(r) if r.contains("u32") => "UInt32",
        Some(r) if r.contains("i64") || r.contains("u64") => "Int64",
        _ => "Int32",
    }
}

/// Discriminant field width for a tagged union. Rust `#[repr(C, u8)]`
/// tagged unions use a 1-byte tag; a plain `#[repr(C)]` data enum uses a
/// C-`int` tag.
fn tag_type(repr: Option<&str>) -> &'static str {
    match repr {
        Some(r) if r.contains("u8") => "UInt8",
        Some(r) if r.contains("u16") => "UInt16",
        Some(r) if r.contains("u32") => "UInt32",
        _ => "Int32",
    }
}

/// Strip characters that would break a Crystal `#` line comment.
fn sanitize_comment(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}
