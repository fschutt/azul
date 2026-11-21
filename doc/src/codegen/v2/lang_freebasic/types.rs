//! FreeBASIC `Type` / `Enum` / variant-record emission.
//!
//! Strategy:
//!
//! - **Unit-only enums** → `Enum AzFoo : AzFoo_A : AzFoo_B : End Enum`. FreeBASIC enums are
//!   integer-backed by default and start at 0, matching the default Rust `repr(C)` enum layout.
//! - **Tagged-union enums** (and monomorphized aliases like `LayoutWidthValue`) → a `Type` holding
//!   the tag (`UByte` for `repr(C, u8)`, the constants in a `<T>Tag` Enum) and a `Union` of the
//!   payloads, one per non-unit variant. The Union is aligned to its largest member, so every
//!   payload sits where Rust's `repr(C, u8)` puts it - the wire format of the prebuilt `libazul`.
//! - **POD structs** → `Type AzFoo ... End Type` with field types resolved via `map_type_to_fb`.
//!   Default natural alignment is used (no `Field = 1` packing) because `extern "C"` Rust structs
//!   use natural alignment, not packed.
//! - **Callback typedefs** → declared inside `Extern "C" Lib "azul"` as procedural-pointer aliases
//!   via `Type AzFooCallbackType As Function ... `. We emit those here (above the externals block)
//!   so they may appear in field types.
//! - **Plain aliases** → `Type AzFoo As <target>`.
//! - Everything but the unit enums is emitted in the IR's topological `sort_order` (azul.h's
//!   order): fbc lets a `Type` hold another Type by value only once that Type is complete.
//! - **Recursive / GenericTemplate** types are skipped with `' SKIPPED: <reason>` line comments.

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef,
            FieldRefKind, MonomorphizedKind, StructDef, TypeAliasDef, TypeCategory,
        },
    },
    ffi_type_name, map_type_to_fb, sanitize_comment, sanitize_identifier,
};

// ============================================================================
// Top-level type-block emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("' --------------------------------------------------------------------");
    builder.line("' Type definitions: enums, POD records, tagged unions, callback types.");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

    // 1. Unit (simple) enums first: they depend on nothing.
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // 2. Everything else in the IR's topological `sort_order` - the order
    //    azul.h uses. A FreeBASIC `Type` may hold another Type by value only
    //    once that Type is complete (pointers may point ahead); emitting all
    //    unions, then all structs, then the callback types put 1205 by-value
    //    fields before their type. The aliases (plain and monomorphized:
    //    `LayoutWidthValue`, `OptionU32`, ...) were never emitted at all.
    enum Item<'a> {
        Union(&'a EnumDef),
        Struct(&'a StructDef),
        Callback(&'a CallbackTypedefDef),
        Alias(&'a TypeAliasDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for e in &ir.enums {
        if e.is_union && should_include_enum(e, config) {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        items.push((s.sort_order, Item::Struct(s)));
    }
    for cb in &ir.callback_typedefs {
        if config.should_include_type(&cb.name) {
            items.push((cb.sort_order, Item::Callback(cb)));
        }
    }
    for ta in &ir.type_aliases {
        if config.should_include_type(&ta.name) {
            items.push((ta.sort_order, Item::Alias(ta)));
        }
    }
    // Stable: equal orders keep the kind order above, like lang_c.
    items.sort_by_key(|(order, _)| *order);
    for (_, item) in &items {
        match item {
            Item::Union(e) => emit_tagged_union(builder, e, ir),
            Item::Struct(s) => emit_struct(builder, s, ir),
            Item::Callback(cb) => emit_callback_typedef(builder, cb, ir),
            Item::Alias(ta) => emit_type_alias(builder, ta, ir),
        }
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Type aliases (plain and monomorphized)
// ============================================================================

fn emit_type_alias(builder: &mut CodeBuilder, ta: &TypeAliasDef, ir: &CodegenIR) {
    let t = ffi_type_name(&ta.name);
    let Some(mono) = &ta.monomorphized_def else {
        if ta.target.contains('<') {
            builder.line(&format!(
                "' SKIPPED: {} = {} (a generic alias without a monomorphized definition)",
                ta.name, ta.target
            ));
            return;
        }
        builder.line(&format!("Type {} As {}", t, map_type_to_fb(&ta.target, ir)));
        builder.blank();
        return;
    };
    for d in &ta.doc {
        builder.line(&format!("' {}", sanitize_comment(d)));
    }
    match &mono.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            let names: Vec<&str> = variants.iter().map(String::as_str).collect();
            emit_enum_body(builder, &t, &names);
        }
        MonomorphizedKind::Struct { fields } => {
            builder.line(&format!("Type {}", t));
            builder.indent();
            if fields.is_empty() {
                builder.line("__opaque As UByte  ' opaque, no fields exposed via FFI");
            }
            for f in fields {
                emit_struct_field(builder, f, ir);
            }
            builder.dedent();
            builder.line("End Type");
            builder.blank();
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let payloads: Vec<(String, Vec<(String, String)>)> = variants
                .iter()
                .map(|v| {
                    let fields = v
                        .payload_type
                        .iter()
                        .map(|p| {
                            (
                                "payload".to_string(),
                                field_type_for_ref_kind(p, &v.payload_ref_kind, ir),
                            )
                        })
                        .collect();
                    (sanitize_identifier(&v.name), fields)
                })
                .collect();
            emit_union_body(builder, &t, repr.as_deref(), &payloads);
        }
    }
}

// ============================================================================
// Inclusion filters
// ============================================================================

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    // VecRef slices and the destructor/clone unions and callbacks are plain
    // C types every Vec and many functions hold BY VALUE (`destructor As
    // AzU8VecDestructor`): skipping them left those fields naming undeclared
    // types. Only a generic template has no C spelling.
    !matches!(s.category, TypeCategory::Recursive | TypeCategory::GenericTemplate)
}

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    // See `should_include_struct`: the destructor/clone unions are held by value.
    !matches!(e.category, TypeCategory::Recursive | TypeCategory::GenericTemplate)
}

fn emit_skipped_struct(builder: &mut CodeBuilder, s: &StructDef) {
    builder.line(&format!(
        "' SKIPPED: struct {} ({})",
        s.name,
        s.category.description()
    ));
}

fn emit_skipped_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    builder.line(&format!(
        "' SKIPPED: enum {} ({})",
        e.name,
        e.category.description()
    ));
}

// ============================================================================
// Unit-only enum
// ============================================================================

fn emit_unit_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let t = ffi_type_name(&e.name);
    let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    emit_enum_body(builder, &t, &names);
}

/// `Enum T ... End Enum` with explicitly pinned values (0..N-1, the C ABI's).
fn emit_enum_body(builder: &mut CodeBuilder, t: &str, variants: &[&str]) {
    if variants.is_empty() {
        // Empty enums aren't valid in FB; emit a degenerate alias.
        builder.line(&format!("Type {} As ULong", t)); // a C enum is 32-bit
        builder.blank();
        return;
    }

    builder.line(&format!("Enum {}", t));
    builder.indent();
    for (i, v) in variants.iter().enumerate() {
        let nm = sanitize_identifier(v);
        // Pin the value explicitly so renumbering matches the C-ABI.
        builder.line(&format!("{}_{} = {}", t, nm, i));
    }
    builder.dedent();
    builder.line("End Enum");
    builder.blank();
}

// ============================================================================
// Tagged-union enum (FreeBASIC Type with embedded Union)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let t = ffi_type_name(&e.name);

    // Collect per-variant payload field lists (empty for unit variants).
    let mut payload_lines: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for v in &e.variants {
        let payload_fields: Vec<(String, String)> = match &v.kind {
            EnumVariantKind::Unit => Vec::new(),
            EnumVariantKind::Tuple(types) => {
                if types.is_empty() {
                    Vec::new()
                } else if types.len() == 1 {
                    let (ty, ref_kind) = &types[0];
                    let fb_ty = field_type_for_ref_kind(ty, ref_kind, ir);
                    vec![("payload".to_string(), fb_ty)]
                } else {
                    types
                        .iter()
                        .enumerate()
                        .map(|(i, (ty, ref_kind))| {
                            let fb_ty = field_type_for_ref_kind(ty, ref_kind, ir);
                            (format!("payload{}", i), fb_ty)
                        })
                        .collect()
                }
            }
            EnumVariantKind::Struct(fields) => fields
                .iter()
                .map(|f| {
                    let fb_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
                    (sanitize_identifier(&f.name), fb_ty)
                })
                .collect(),
        };
        payload_lines.push((sanitize_identifier(&v.name), payload_fields));
    }
    emit_union_body(builder, &t, e.repr.as_deref(), &payload_lines);
}

/// The tag Enum, the helper Types of multi-field variants and the outer
/// `Type T: tag + Union of payloads` of one tagged union - a data-carrying
/// enum or a monomorphized alias. `payload_lines`: per variant, its name and
/// payload fields `(name, FreeBASIC type)` (empty for a unit variant).
///
/// The tag of a `repr(C, u8)` union is `UByte` (azul.h: `uint8_t tag;`), not
/// the Enum, which fbc makes as wide as a C enum or wider; the Union after it
/// is aligned to its largest member, which puts every payload where Rust's
/// `repr(C, u8)` does - no padding needed.
fn emit_union_body(
    builder: &mut CodeBuilder,
    t: &str,
    repr: Option<&str>,
    payload_lines: &[(String, Vec<(String, String)>)],
) {
    // Tag enum (the variant constants, whatever the tag field's width).
    let tag_name = format!("{}Tag", t);
    builder.line(&format!("Enum {}", tag_name));
    builder.indent();
    for (i, (nm, _)) in payload_lines.iter().enumerate() {
        builder.line(&format!("{}_{} = {}", tag_name, nm, i));
    }
    builder.dedent();
    builder.line("End Enum");
    builder.blank();
    let tag_field_type = if repr.is_some_and(|r| r.contains("u8")) {
        "UByte".to_string()
    } else {
        tag_name.clone()
    };

    // For multi-field variants, emit a helper Type at top level that
    // groups the fields. (FB does not allow nested Type definitions
    // inside a Union; multi-field variants must reference a previously
    // declared Type.)
    for (variant_name, fields) in payload_lines {
        if fields.len() > 1 {
            builder.line(&format!("Type {}_{}_payload", t, variant_name));
            builder.indent();
            for (fname, fty) in fields {
                emit_field_line(builder, fname, fty);
            }
            builder.dedent();
            builder.line("End Type");
            builder.blank();
        }
    }

    // Discard unit-only variants from the Union (they contribute no
    // members).
    let union_members: Vec<&(String, Vec<(String, String)>)> = payload_lines
        .iter()
        .filter(|(_, f)| !f.is_empty())
        .collect();

    // Outer Type: tag + (optional) Union of payloads.
    builder.line(&format!("Type {}", t));
    builder.indent();
    builder.line(&format!("tag As {}", tag_field_type));

    if !union_members.is_empty() {
        builder.line("Union");
        builder.indent();
        for (variant_name, fields) in &union_members {
            if fields.len() == 1 {
                // Single-payload variant: emit the lone field as a
                // member named after the variant.
                let (_, fty) = &fields[0];
                emit_field_line(builder, variant_name, fty);
            } else {
                // Multi-field payload: reference the helper Type
                // emitted above.
                builder.line(&format!(
                    "{} As {}_{}_payload",
                    variant_name, t, variant_name
                ));
            }
        }
        builder.dedent();
        builder.line("End Union");
    }

    builder.dedent();
    builder.line("End Type");
    builder.blank();
}

// ============================================================================
// POD struct
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let t = ffi_type_name(&s.name);

    if s.fields.is_empty() {
        // Opaque type — FB requires at least one field; emit a single
        // padding byte. The actual size is irrelevant: client code
        // only ever holds an `AzFoo Ptr` to such types.
        builder.line(&format!("Type {}", t));
        builder.indent();
        builder.line("__opaque As UByte  ' opaque, no fields exposed via FFI");
        builder.dedent();
        builder.line("End Type");
        builder.blank();
        return;
    }

    builder.line(&format!("Type {}", t));
    builder.indent();
    for f in &s.fields {
        emit_struct_field(builder, f, ir);
    }
    builder.dedent();
    builder.line("End Type");
    builder.blank();
}

fn emit_struct_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("' {}", sanitize_comment(doc)));
    }
    let nm = sanitize_identifier(&f.name);
    let fb_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    emit_field_line(builder, &nm, &fb_ty);
}

/// Emit a single field declaration. Handles the `__FB_ARRAY__upper__elem`
/// marker that `map_type_to_fb` uses to encode fixed-size arrays
/// (`[T; N]` in Rust).
fn emit_field_line(builder: &mut CodeBuilder, name: &str, fb_ty: &str) {
    if let Some(rest) = fb_ty.strip_prefix("__FB_ARRAY__") {
        // rest is "<upper>__<elem>"
        if let Some((upper_str, elem)) = rest.split_once("__") {
            builder.line(&format!("{}(0 To {}) As {}", name, upper_str, elem));
            return;
        }
    }
    builder.line(&format!("{} As {}", name, fb_ty));
}

// ============================================================================
// Callback typedef (FreeBASIC procedural-pointer alias)
// ============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }
    let t = ffi_type_name(&cb.name);

    let args: Vec<String> = cb
        .args
        .iter()
        .map(|arg| {
            let fb_ty = match arg.ref_kind {
                ArgRefKind::Owned => map_type_to_fb(&arg.type_name, ir),
                ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    ptr_type_for_arg(&arg.type_name, ir)
                }
            };
            format!("ByVal {} As {}", sanitize_identifier(&arg.name), fb_ty)
        })
        .collect();

    let header = if let Some(ret) = &cb.return_type {
        let fb_ret = map_type_to_fb(ret, ir);
        format!(
            "Type {} As Function Cdecl ({}) As {}",
            t,
            args.join(", "),
            fb_ret
        )
    } else {
        format!("Type {} As Sub Cdecl ({})", t, args.join(", "))
    };
    builder.line(&header);
    builder.blank();
}

// ============================================================================
// Field/argument type helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the FreeBASIC field type.
pub(crate) fn field_type_for_ref_kind(
    type_name: &str,
    ref_kind: &FieldRefKind,
    ir: &CodegenIR,
) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_fb(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => ptr_type_for_arg(type_name, ir),
    }
}

/// Pointer-form type mapping for arguments and reference fields.
pub(crate) fn ptr_type_for_arg(type_name: &str, ir: &CodegenIR) -> String {
    let inner = type_name.trim();
    match inner {
        "c_char" | "char" => "ZString Ptr".to_string(),
        "i8" | "u8" => "UByte Ptr".to_string(),
        "c_void" | "void" | "()" => "Any Ptr".to_string(),
        _ => {
            if ir.find_struct(inner).is_some()
                || ir.find_enum(inner).is_some()
                || ir.find_type_alias(inner).is_some()
            {
                format!("{} Ptr", ffi_type_name(inner))
            } else {
                "Any Ptr".to_string()
            }
        }
    }
}
