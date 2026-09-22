//! Go-native type emission for the Go (purego) generator.
//!
//! `types.go` no longer references a single `C.` name: every api.json type
//! is spelled as a Go type whose memory layout is the `repr(C)` layout the
//! C header (`azul.h`) and the DLL agree on. Go lays out structs with the
//! same natural-alignment rules as C on every 64-bit target, so a struct
//! with the same field order and fixed-size field types IS the C struct.
//!
//! * **structs** -> `type AzFoo struct { Field Type; ... }` (fields
//!   PascalCased so they are exported; fixed-size Go types; pointers as
//!   `*AzT` / `unsafe.Pointer`; `usize`/`isize` as `uintptr`/`int`).
//! * **unit enums** -> `type AzFoo uint32` + a `const` block (a C `enum`
//!   is `int`-sized; `lang_fortran::layout` pins the same 4 bytes).
//! * **tagged unions** (`#[repr(C, u8)]` or `#[repr(C)]`) -> a struct of
//!   exactly the union's size and alignment: `{ _ [0]uint64; Tag; data_
//!   [N]byte }`. Per variant a `AzFoo_Variant_X` struct mirrors the C
//!   `struct AzFooVariant_X { tag; payload... }` so Go computes the payload
//!   offsets itself; constructors (`AzFoo_X(payload)`), predicates
//!   (`IsX()`) and accessors (`AsX()`) cast through `unsafe.Pointer`.
//!   Byte storage is what makes the Go GC safe here: a `0x8` enum tag can
//!   never sit in a pointer-typed slot (the crash the ledger's B2 records).
//! * **callback typedefs** -> `type AzFooType unsafe.Pointer` (a C
//!   function pointer, opaque to Go).
//! * **type aliases** -> Go aliases (`type AzScanCode = uint32`); generic
//!   aliases use their monomorphized definition.
//!
//! Sizes and alignments come from `lang_fortran::layout` (the one IR
//! layout computation, verified against clang's `sizeof`/`alignof` of
//! every type in azul.h). Each emitted type carries a compile-time
//! assertion (`const _ = uint(S-unsafe.Sizeof(T{})) + ...`) so a Go
//! layout that drifts from the C one is a build error, not a crash.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, StructDef, TypeAliasDef,
};
use super::super::lang_fortran::layout::{type_layout, AbiLayout};
use super::{ffi_type_name, primitive_to_go, snake_to_pascal};

/// Generate the contents of `types.go`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);
    emit_header(&mut b);

    for c in &ir.callback_typedefs {
        if !config.should_include_type(&c.name) {
            continue;
        }
        emit_callback_typedef(&mut b, c);
    }

    for t in &ir.type_aliases {
        if !config.should_include_type(&t.name) {
            continue;
        }
        emit_type_alias(&mut b, t, ir);
    }

    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        emit_enum(&mut b, e, ir);
    }

    for s in &ir.structs {
        if !should_include_struct(s, config) {
            continue;
        }
        emit_struct(&mut b, s, ir);
    }


    b.line("// ============================================================================");
    b.line("// Ergonomic Type Aliases");
    b.line("// ============================================================================");
    for e in &ir.enums {
        if config.should_include_type(&e.name) && e.generic_params.is_empty() {
            b.line(&format!("type {} = {}", super::sanitize_identifier(&e.name), ffi_type_name(&e.name)));
        }
    }
    for s in &ir.structs {
        if config.should_include_type(&s.name) && s.generic_params.is_empty() && !super::wrappers::should_emit_wrapper(s, ir, config) {
            b.line(&format!("type {} = {}", super::sanitize_identifier(&s.name), ffi_type_name(&s.name)));
        }
    }
    for c in &ir.callback_typedefs {
        if config.should_include_type(&c.name) {
            b.line(&format!("type {} = {}", super::sanitize_identifier(&c.name), ffi_type_name(&c.name)));
        }
    }
    emit_constants(&mut b, ir);
    Ok(b.finish())
}

/// The api.json constants, grouped by the class that owns them.
///
/// These are the OpenGL enum values, and they are API: without them a caller
/// cannot name a single argument of the GL surface. The C header has always
/// had them (`#define AzGlContextPtr_ACCUM 0x0100`); Go had none.
fn emit_constants(b: &mut CodeBuilder, ir: &CodegenIR) {
    if ir.constants.is_empty() {
        return;
    }
    b.blank();
    b.line("// ============================================================================");
    b.line("// Constants");
    b.line("// ============================================================================");
    b.line("");
    b.line("const (");
    b.indent();
    for c in &ir.constants {
        // `GlContextPtr_ACCUM` keeps the owning class in the name: Go has no
        // scope to hang them off, and the C spelling is what GL documents.
        b.line(&format!("{} {} = {}", c.name, go_scalar(&c.type_name), c.value));
    }
    b.dedent();
    b.line(")");
    b.blank();
}

/// The Go type of a constant's declared scalar type.
fn go_scalar(rust_type: &str) -> &'static str {
    match rust_type.trim() {
        "u8" => "uint8",
        "u16" => "uint16",
        "u64" => "uint64",
        "i8" => "int8",
        "i16" => "int16",
        "i32" => "int32",
        "i64" => "int64",
        "f32" => "float32",
        "f64" => "float64",
        _ => "uint32",
    }
}

fn emit_header(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// types.go - Go-native mirrors of every azul C-ABI type (no cgo in this file).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Every type here has the exact size and alignment of its C twin in azul.h");
    b.line("// (64-bit ABI: pointers are 8 bytes). Structs mirror the C field order with");
    b.line("// fixed-size Go types; tagged unions are byte blobs with a leading tag and");
    b.line("// per-variant constructors / accessors; unit enums are uint32 constants.");
    b.line("// A `const _ = ...` line after each type turns any layout drift between");
    b.line("// this file and the generator's ABI model into a compile error.");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import \"unsafe\"");
    b.blank();
    b.line("var _ unsafe.Pointer");
    b.blank();
}

// ============================================================================
// Inclusion filters
// ============================================================================

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&e.name) && e.generic_params.is_empty()
}

fn should_include_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name) && s.generic_params.is_empty()
}

// ============================================================================
// Type mapping (shared with functions.rs / wrappers.rs / managed.rs)
// ============================================================================

/// Go type for a type spelled by VALUE (`u32`, `Dom`, `*const c_void`,
/// `[u8; 4]`, ...). Named api.json types become `Az<Name>`.
pub(crate) fn go_value_type(type_name: &str, ir: &CodegenIR) -> String {
    let t = type_name.trim();
    for prefix in ["*const ", "*mut ", "&mut ", "&"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return go_pointer_to(&go_value_type(rest, ir));
        }
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some((elem, n)) = inner.rsplit_once(';') {
            if n.trim().parse::<usize>().is_ok() {
                return format!("[{}]{}", n.trim(), go_value_type(elem, ir));
            }
        }
    }
    if let Some(p) = primitive_to_go(t) {
        if p.is_empty() {
            // `c_void` / `()` by value only ever appears under a pointer.
            return "unsafe.Pointer".to_string();
        }
        return p.to_string();
    }
    let _ = ir; // every non-primitive name is an api.json type: Az<Name>
    ffi_type_name(t)
}

/// Go pointer to a Go value type. A pointer to `void` stays `unsafe.Pointer`.
pub(crate) fn go_pointer_to(base: &str) -> String {
    if base == "unsafe.Pointer" {
        base.to_string()
    } else {
        format!("*{}", base)
    }
}

/// Go type for a struct field / variant payload with its ref_kind applied.
pub(crate) fn go_field_type(type_name: &str, ref_kind: FieldRefKind, ir: &CodegenIR) -> String {
    let base = go_value_type(type_name, ir);
    match ref_kind {
        FieldRefKind::Owned => base,
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => go_pointer_to(&base),
    }
}

/// Exported Go field name for an api.json field (`window_state` ->
/// `WindowState`). `tag` is reserved by the union variant structs.
pub(crate) fn go_field_name(name: &str) -> String {
    let p = snake_to_pascal(name);
    if p.starts_with(|c: char| c.is_ascii_digit()) {
        // Tuple-struct fields are named `0`, `1`, ... in api.json.
        return format!("F{}", p);
    }
    if p == "Tag" {
        "Tag_".to_string()
    } else {
        p
    }
}

/// `uint8` iff the repr carries `u8` (the C header's `uint8_t tag;`), else
/// the C tag enum, which is `int`-sized.
fn tag_go_type(repr: Option<&str>) -> &'static str {
    if repr.map(|r| r.contains("u8")).unwrap_or(false) {
        "uint8"
    } else {
        "uint32"
    }
}

/// The zero-size field that pins a byte blob to the C alignment.
fn align_field(align: usize) -> Option<&'static str> {
    match align {
        2 => Some("[0]uint16"),
        4 => Some("[0]uint32"),
        8 => Some("[0]uint64"),
        _ => None,
    }
}

/// Compile-time size/alignment assertion. `uint(negative)` is a constant
/// overflow, so either direction of drift fails the Go build.
fn emit_layout_check(b: &mut CodeBuilder, go_name: &str, l: AbiLayout) {
    b.line(&format!(
        "const _ = uint({s}-unsafe.Sizeof({n}{{}})) + uint(unsafe.Sizeof({n}{{}})-{s}) + uint({a}-unsafe.Alignof({n}{{}})) + uint(unsafe.Alignof({n}{{}})-{a}) // ABI: {s} bytes, align {a}",
        s = l.size,
        a = l.align,
        n = go_name
    ));
}

fn emit_docs(b: &mut CodeBuilder, doc: &[String]) {
    for d in doc {
        b.line(&format!("// {}", d));
    }
}

// ============================================================================
// Callback typedefs + aliases
// ============================================================================

fn emit_callback_typedef(b: &mut CodeBuilder, c: &CallbackTypedefDef) {
    emit_docs(b, &c.doc);
    b.line(&format!(
        "// {} is a C function pointer (opaque to Go; use Register* in callbacks.go).",
        ffi_type_name(&c.name)
    ));
    b.line(&format!("type {} unsafe.Pointer", ffi_type_name(&c.name)));
    b.blank();
}

fn emit_type_alias(b: &mut CodeBuilder, t: &TypeAliasDef, ir: &CodegenIR) {
    let go_name = ffi_type_name(&t.name);
    if let Some(mono) = &t.monomorphized_def {
        let Some(layout) = type_layout(&t.name, ir) else {
            b.line(&format!("// SKIPPED: {} (no ABI layout)", go_name));
            b.blank();
            return;
        };
        match &mono.kind {
            MonomorphizedKind::SimpleEnum { variants, .. } => {
                emit_unit_enum_body(b, &go_name, &t.doc, variants);
            }
            MonomorphizedKind::Struct { fields } => {
                emit_struct_body(b, &go_name, &t.doc, fields, layout, ir);
            }
            MonomorphizedKind::TaggedUnion { repr, variants } => {
                let vs: Vec<UnionVariant> = variants
                    .iter()
                    .map(|v| UnionVariant {
                        name: v.name.clone(),
                        members: v
                            .payload_type
                            .as_ref()
                            .map(|p| {
                                vec![(
                                    "Payload".to_string(),
                                    go_field_type(p, v.payload_ref_kind, ir),
                                )]
                            })
                            .unwrap_or_default(),
                        single_payload: v.payload_type.is_some(),
                    })
                    .collect();
                emit_union_body(b, &go_name, &t.doc, repr.as_deref(), &vs, layout);
            }
        }
        return;
    }
    if t.target.contains('<') {
        b.line(&format!(
            "// SKIPPED: {} = {} (generic alias without a monomorphized definition)",
            go_name, t.target
        ));
        b.blank();
        return;
    }
    emit_docs(b, &t.doc);
    b.line(&format!("type {} = {}", go_name, go_value_type(&t.target, ir)));
    b.blank();
}

// ============================================================================
// Enums (unit + tagged union)
// ============================================================================

fn emit_enum(b: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let go_name = ffi_type_name(&e.name);
    if !e.is_union {
        let names: Vec<String> = e.variants.iter().map(|v| v.name.clone()).collect();
        emit_unit_enum_body(b, &go_name, &e.doc, &names);
        return;
    }
    let Some(layout) = type_layout(&e.name, ir) else {
        b.line(&format!("// SKIPPED: {} (no ABI layout)", go_name));
        b.blank();
        return;
    };
    let vs: Vec<UnionVariant> = e
        .variants
        .iter()
        .map(|v| match &v.kind {
            EnumVariantKind::Unit => UnionVariant {
                name: v.name.clone(),
                members: vec![],
                single_payload: false,
            },
            EnumVariantKind::Tuple(payloads) => UnionVariant {
                name: v.name.clone(),
                members: if payloads.len() == 1 {
                    vec![("Payload".to_string(), go_field_type(&payloads[0].0, payloads[0].1, ir))]
                } else {
                    payloads
                        .iter()
                        .enumerate()
                        .map(|(i, (t, rk))| (format!("Payload{}", i), go_field_type(t, *rk, ir)))
                        .collect()
                },
                single_payload: payloads.len() == 1,
            },
            EnumVariantKind::Struct(fields) => UnionVariant {
                name: v.name.clone(),
                members: fields
                    .iter()
                    .map(|f| (go_field_name(&f.name), go_field_type(&f.type_name, f.ref_kind, ir)))
                    .collect(),
                single_payload: false,
            },
        })
        .collect();
    emit_union_body(b, &go_name, &e.doc, e.repr.as_deref(), &vs, layout);
}

fn emit_unit_enum_body(b: &mut CodeBuilder, go_name: &str, doc: &[String], variants: &[String]) {
    emit_docs(b, doc);
    b.line(&format!("type {} uint32", go_name));
    b.blank();
    if !variants.is_empty() {
        b.line("const (");
        b.indent();
        for (i, v) in variants.iter().enumerate() {
            b.line(&format!("{}_{} {} = {}", if go_name.starts_with("Az") { &go_name[2..] } else { go_name }, v, go_name, i));
        }
        b.dedent();
        b.line(")");
        b.blank();
    }
}

/// One variant of a tagged union, already mapped to Go member types.
struct UnionVariant {
    name: String,
    /// `(go_field_name, go_type)` per payload member; empty for unit variants.
    members: Vec<(String, String)>,
    /// One tuple payload named `Payload` -> `AsX()` returns `*Payload` directly.
    single_payload: bool,
}

fn emit_union_body(
    b: &mut CodeBuilder,
    go_name: &str,
    doc: &[String],
    repr: Option<&str>,
    variants: &[UnionVariant],
    layout: AbiLayout,
) {
    let tag_ty = format!("{}_Tag", go_name);
    let tag_under = tag_go_type(repr);
    let tag_size = if tag_under == "uint8" { 1 } else { 4 };

    b.line(&format!("// {} is the discriminant of {}.", tag_ty, go_name));
    b.line(&format!("type {} {}", tag_ty, tag_under));
    b.blank();
    if !variants.is_empty() {
        b.line("const (");
        b.indent();
        for (i, v) in variants.iter().enumerate() {
            b.line(&format!("{}_{} {} = {}", tag_ty, v.name, tag_ty, i));
        }
        b.dedent();
        b.line(")");
        b.blank();
    }

    emit_docs(b, doc);
    b.line(&format!(
        "// {} is a tagged union ({} bytes, {}-aligned): read Tag, then use the",
        go_name, layout.size, layout.align
    ));
    b.line("// Is<Variant>() / As<Variant>() accessors; build one with the <Type>_<Variant>()");
    b.line("// constructors. The payload bytes are deliberately untyped so the Go GC");
    b.line("// never mistakes a tag or a non-pointer payload for a pointer.");
    b.line(&format!("type {} struct {{", go_name));
    b.indent();
    if let Some(af) = align_field(layout.align) {
        b.line(&format!("_ {}", af));
    }
    b.line(&format!("Tag {}", tag_ty));
    if layout.size > tag_size {
        b.line(&format!("data_ [{}]byte", layout.size - tag_size));
    }
    b.dedent();
    b.line("}");
    b.blank();
    emit_layout_check(b, go_name, layout);
    b.blank();

    for v in variants {
        let variant_struct = format!("{}_Variant_{}", go_name, v.name);
        let tag_const = format!("{}_{}", tag_ty, v.name);
        if !v.members.is_empty() {
            b.line(&format!(
                "// {} mirrors the C `struct {}Variant_{}` (tag + payload).",
                variant_struct, go_name, v.name
            ));
            b.line(&format!("type {} struct {{", variant_struct));
            b.indent();
            b.line(&format!("Tag {}", tag_ty));
            for (n, t) in &v.members {
                b.line(&format!("{} {}", n, t));
            }
            b.dedent();
            b.line("}");
            b.blank();
        }

        // Constructor. Named after the IR's variant-constructor method, not
        // the bare variant name: a variant called `Default` is exported as
        // `Az<Enum>_defaultVariant`, and the Go constructor every other
        // binding's user reads about must carry the same name.
        let ctor_name = upper_first(&crate::codegen::v2::ir_builder::variant_constructor_method_name(
            &v.name,
        ));
        let params: Vec<String> = v
            .members
            .iter()
            .map(|(n, t)| format!("{} {}", ctor_param_name(n), t))
            .collect();
        b.line(&format!(
            "// {}_{} builds the {} variant of {}.",
            go_name, ctor_name, v.name, go_name
        ));
        b.line(&format!("func {}_{}({}) {} {{", go_name, ctor_name, params.join(", "), go_name));
        b.indent();
        b.line(&format!("var u {}", go_name));
        if v.members.is_empty() {
            b.line(&format!("u.Tag = {}", tag_const));
        } else {
            b.line(&format!("v := (*{})(unsafe.Pointer(&u))", variant_struct));
            b.line(&format!("v.Tag = {}", tag_const));
            for (n, _) in &v.members {
                b.line(&format!("v.{} = {}", n, ctor_param_name(n)));
            }
        }
        b.line("return u");
        b.dedent();
        b.line("}");
        b.blank();

        // Predicate.
        b.line(&format!("// Is{} reports whether u holds the {} variant.", v.name, v.name));
        b.line(&format!("func (u *{}) Is{}() bool {{", go_name, v.name));
        b.indent();
        b.line(&format!("return u.Tag == {}", tag_const));
        b.dedent();
        b.line("}");
        b.blank();

        // Accessor.
        if !v.members.is_empty() {
            let ret_ty = if v.single_payload {
                format!("*{}", v.members[0].1)
            } else {
                format!("*{}", variant_struct)
            };
            b.line(&format!(
                "// As{} returns the {} payload, or nil if u holds another variant.",
                v.name, v.name
            ));
            b.line(&format!("func (u *{}) As{}() {} {{", go_name, v.name, ret_ty));
            b.indent();
            b.line(&format!("if u.Tag != {} {{", tag_const));
            b.indent();
            b.line("return nil");
            b.dedent();
            b.line("}");
            if v.single_payload {
                b.line(&format!(
                    "return &(*{})(unsafe.Pointer(u)).Payload",
                    variant_struct
                ));
            } else {
                b.line(&format!("return (*{})(unsafe.Pointer(u))", variant_struct));
            }
            b.dedent();
            b.line("}");
            b.blank();
        }
    }
}

/// Constructor parameter name for a variant member (`Payload` -> `payload`,
/// `Payload0` -> `payload0`, `Width` -> `width`); `u`/`v` are taken.
/// `defaultVariant` -> `DefaultVariant`: an exported Go identifier.
fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn ctor_param_name(member: &str) -> String {
    let mut c = member.chars();
    let lower = match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    };
    let s = super::sanitize_identifier(&lower);
    if s == "u" || s == "v" {
        format!("{}_", s)
    } else {
        s
    }
}

// ============================================================================
// Structs
// ============================================================================

fn emit_struct(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let go_name = ffi_type_name(&s.name);
    let Some(layout) = type_layout(&s.name, ir) else {
        b.line(&format!("// SKIPPED: {} (no ABI layout)", go_name));
        b.blank();
        return;
    };
    emit_struct_body(b, &go_name, &s.doc, &s.fields, layout, ir);
}

fn emit_struct_body(
    b: &mut CodeBuilder,
    go_name: &str,
    doc: &[String],
    fields: &[FieldDef],
    layout: AbiLayout,
    ir: &CodegenIR,
) {
    emit_docs(b, doc);
    b.line(&format!("type {} struct {{", go_name));
    b.indent();
    if fields.is_empty() {
        // The C header spells an empty struct with a `uint8_t _dummy;`.
        b.line("dummy_ uint8");
    }
    for f in fields {
        if let Some(d) = &f.doc {
            b.line(&format!("// {}", d));
        }
        b.line(&format!(
            "{} {}",
            go_field_name(&f.name),
            go_field_type(&f.type_name, f.ref_kind, ir)
        ));
    }
    b.dedent();
    b.line("}");
    b.blank();
    emit_layout_check(b, go_name, layout);
    b.blank();
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::super::ir::{EnumVariantDef, MonomorphizedTypeDef, MonomorphizedVariant, TypeCategory};
    use super::*;

    pub(crate) fn field(fname: &str, ty: &str, rk: FieldRefKind) -> FieldDef {
        FieldDef {
            name: fname.to_string(),
            type_name: ty.to_string(),
            doc: None,
            is_public: true,
            ref_kind: rk,
        }
    }

    pub(crate) fn plain_struct(name: &str, fields: Vec<FieldDef>) -> StructDef {
        StructDef {
            name: name.to_string(),
            doc: vec![],
            fields,
            external_path: None,
            module: "test".to_string(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".to_string()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category: TypeCategory::default(),
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        }
    }

    pub(crate) fn plain_enum(name: &str, repr: &str, variants: Vec<EnumVariantDef>) -> EnumDef {
        let is_union = variants
            .iter()
            .any(|v| !matches!(v.kind, EnumVariantKind::Unit));
        EnumDef {
            name: name.to_string(),
            doc: vec![],
            variants,
            external_path: None,
            module: "test".to_string(),
            derives: vec![],
            has_explicit_derive: false,
            is_union,
            repr: Some(repr.to_string()),
            is_send_safe: true,
            traits: Default::default(),
            generic_params: vec![],
            category: TypeCategory::default(),
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
        }
    }

    fn unit(name: &str) -> EnumVariantDef {
        EnumVariantDef {
            name: name.into(),
            doc: None,
            kind: EnumVariantKind::Unit,
        }
    }

    fn tuple(name: &str, ty: &str) -> EnumVariantDef {
        EnumVariantDef {
            name: name.into(),
            doc: None,
            kind: EnumVariantKind::Tuple(vec![(ty.into(), FieldRefKind::Owned)]),
        }
    }

    /// The fixture every lang_go test shares: a Vec (`U8Vec`), a `Dom`-like
    /// struct that embeds it, `Option<Dom>` as a u8-tagged union, a unit enum,
    /// a callback typedef, and a struct with a pointer field.
    pub(crate) fn fixture_ir() -> CodegenIR {
        let mut ir = CodegenIR::new();
        ir.enums.push(plain_enum(
            "Update",
            "C",
            vec![unit("DoNothing"), unit("RefreshDom")],
        ));
        ir.enums.push(plain_enum(
            "U8VecDestructor",
            "C, u8",
            vec![
                unit("DefaultRust"),
                unit("NoDestructor"),
                tuple("External", "*mut c_void"),
            ],
        ));
        ir.structs.push(plain_struct(
            "U8Vec",
            vec![
                field("ptr", "u8", FieldRefKind::Ptr),
                field("len", "usize", FieldRefKind::Owned),
                field("cap", "usize", FieldRefKind::Owned),
                field("destructor", "U8VecDestructor", FieldRefKind::Owned),
            ],
        ));
        ir.structs.push(plain_struct(
            "Dom",
            vec![
                field("root", "u32", FieldRefKind::Owned),
                field("children", "U8Vec", FieldRefKind::Owned),
                field("flag", "bool", FieldRefKind::Owned),
            ],
        ));
        ir.enums.push(plain_enum(
            "OptionDom",
            "C, u8",
            vec![unit("None"), tuple("Some", "Dom")],
        ));
        ir.callback_typedefs.push(CallbackTypedefDef {
            name: "CallbackType".into(),
            args: vec![],
            return_type: None,
            doc: vec![],
            module: "test".into(),
            external_path: None,
            wrapper: None,
            dependencies: vec![],
            sort_order: 0,
        });
        ir.structs.push(plain_struct(
            "Callback",
            vec![
                field("cb", "CallbackType", FieldRefKind::Owned),
                field("ctx", "*const c_void", FieldRefKind::Owned),
            ],
        ));
        ir.type_aliases.push(TypeAliasDef {
            name: "ScanCode".into(),
            target: "u32".into(),
            generic_args: vec![],
            doc: vec![],
            module: "test".into(),
            external_path: None,
            traits: Default::default(),
            monomorphized_def: None,
            dependencies: vec![],
            sort_order: 0,
        });
        ir.type_aliases.push(TypeAliasDef {
            name: "WidthValue".into(),
            target: "CssPropertyValue".into(),
            generic_args: vec!["u32".into()],
            doc: vec![],
            module: "test".into(),
            external_path: None,
            traits: Default::default(),
            monomorphized_def: Some(MonomorphizedTypeDef {
                kind: MonomorphizedKind::TaggedUnion {
                    repr: Some("C, u8".into()),
                    variants: vec![
                        MonomorphizedVariant {
                            name: "Auto".into(),
                            payload_type: None,
                            payload_ref_kind: FieldRefKind::Owned,
                        },
                        MonomorphizedVariant {
                            name: "Exact".into(),
                            payload_type: Some("u32".into()),
                            payload_ref_kind: FieldRefKind::Owned,
                        },
                    ],
                },
            }),
            dependencies: vec![],
            sort_order: 0,
        });
        ir
    }

    fn gen() -> String {
        generate(&fixture_ir(), &CodegenConfig::c_header()).expect("types.go")
    }

    #[test]
    fn types_go_names_no_c_symbols() {
        let out = gen();
        assert!(!out.contains("import \"C\""), "types.go must not use cgo");
        assert!(!out.contains("C.Az"), "types.go must not reference C.Az* names");
    }

    #[test]
    fn struct_is_native_with_fixed_size_fields_and_layout_check() {
        let out = gen();
        assert!(out.contains("type AzU8Vec struct {"));
        assert!(out.contains("    Ptr *uint8\n"));
        assert!(out.contains("    Len uintptr\n"));
        assert!(out.contains("    Destructor AzU8VecDestructor\n"));
        // ptr(8) + len(8) + cap(8) + destructor(16: u8 tag padded to a pointer) = 40
        assert!(out.contains("uint(40-unsafe.Sizeof(AzU8Vec{}))"), "{out}");
        assert!(out.contains("uint(8-unsafe.Alignof(AzU8Vec{}))"));
        // Dom: u32 + U8Vec(40, align 8) + bool -> 4 pad 4, 40, 1 pad 7 = 56
        assert!(out.contains("type AzDom struct {"));
        assert!(out.contains("uint(56-unsafe.Sizeof(AzDom{}))"), "{out}");
    }

    #[test]
    fn unit_enum_is_uint32_with_sequential_consts() {
        let out = gen();
        assert!(out.contains("type AzUpdate uint32\n"));
        // Variant constants carry the unprefixed IR name (the guide writes
        // `azul.Update_RefreshDom`); the type keeps the C name.
        assert!(out.contains("    Update_DoNothing AzUpdate = 0\n"), "{out}");
        assert!(out.contains("    Update_RefreshDom AzUpdate = 1\n"), "{out}");
    }

    #[test]
    fn u8_tagged_union_is_aligned_byte_blob_with_accessors() {
        let out = gen();
        assert!(out.contains("type AzOptionDom_Tag uint8\n"));
        assert!(out.contains("    AzOptionDom_Tag_Some AzOptionDom_Tag = 1\n"));
        // tag(1) + Dom(56, align 8) at offset 8 = 64
        assert!(out.contains("type AzOptionDom struct {\n    _ [0]uint64\n    Tag AzOptionDom_Tag\n    data_ [63]byte\n}"), "{out}");
        assert!(out.contains("uint(64-unsafe.Sizeof(AzOptionDom{}))"));
        assert!(out.contains("type AzOptionDom_Variant_Some struct {\n    Tag AzOptionDom_Tag\n    Payload AzDom\n}"));
        assert!(out.contains("func AzOptionDom_Some(payload AzDom) AzOptionDom {"));
        assert!(out.contains("func AzOptionDom_None() AzOptionDom {"));
        assert!(out.contains("func (u *AzOptionDom) IsSome() bool {"));
        assert!(out.contains("func (u *AzOptionDom) AsSome() *AzDom {"));
        assert!(out.contains("return &(*AzOptionDom_Variant_Some)(unsafe.Pointer(u)).Payload"));
        // A union whose only payload is a pointer: tag(1) padded to 8, ptr at 8 = 16
        assert!(out.contains("type AzU8VecDestructor struct {\n    _ [0]uint64\n    Tag AzU8VecDestructor_Tag\n    data_ [15]byte\n}"), "{out}");
        assert!(out.contains("func (u *AzU8VecDestructor) AsExternal() *unsafe.Pointer {"));
    }

    #[test]
    fn monomorphized_alias_emits_union_and_simple_alias_is_go_alias() {
        let out = gen();
        assert!(out.contains("type AzScanCode = uint32\n"));
        // WidthValue: u8 tag + u32 payload at offset 4 = 8, align 4
        assert!(out.contains("type AzWidthValue struct {\n    _ [0]uint32\n    Tag AzWidthValue_Tag\n    data_ [7]byte\n}"), "{out}");
        assert!(out.contains("func AzWidthValue_Exact(payload uint32) AzWidthValue {"));
    }

    #[test]
    fn callback_typedef_is_opaque_pointer_and_struct_field_uses_it() {
        let out = gen();
        assert!(out.contains("type AzCallbackType unsafe.Pointer\n"));
        assert!(out.contains("type AzCallback struct {\n    Cb AzCallbackType\n    Ctx unsafe.Pointer\n}"), "{out}");
        assert!(out.contains("uint(16-unsafe.Sizeof(AzCallback{}))"));
    }
}
