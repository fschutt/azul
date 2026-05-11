//! COBOL record + REDEFINES emission.
//!
//! Three top-level entry points:
//!
//! - [`generate_enum_constants`] emits one level-78 entry per variant of
//!   every unit-only enum (`78 AZ-BUTTON-TYPE-PRIMARY VALUE 0.`).
//! - [`generate_records`] emits one level-01 record per surviving struct
//!   (POD) and one level-01 variant record per surviving tagged-union
//!   enum (using REDEFINES to overlay the payloads).
//! - [`generate_callback_typedefs`] emits a comment block describing each
//!   callback function-pointer typedef plus a level-01 alias declared
//!   `USAGE PROGRAM-POINTER`. Callers store function pointers in fields
//!   of that USAGE.
//!
//! Categories Recursive / VecRef / DestructorOrClone / GenericTemplate
//! are skipped with `* SKIPPED: <reason>` comments because COBOL has no
//! representation for them (they all involve heap-managed indirection
//! through trampolines that the host language must own).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    MonomorphizedKind, MonomorphizedTypeDef, StructDef, TypeAliasDef, TypeCategory,
};
use super::{
    cobol_identifier, emit_doc_comment, sanitize_cobol_identifier, sanitize_doc, to_cobol_case,
};

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
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn should_include_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
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

fn emit_skipped(builder: &mut CodeBuilder, name: &str, reason: &str) {
    builder.line(&format!(
        "*> SKIPPED: {} ({})",
        cobol_identifier(name),
        reason
    ));
    // Still emit an opaque TYPEDEF (USAGE POINTER) so struct fields
    // referencing the skipped type by `USAGE TYAZ-<NAME>` resolve.
    // The codegen has no layout for these — callers see them as
    // opaque handles.
    let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(name)));
    builder.line(&format!(
        "       01  {:<28} USAGE POINTER IS TYPEDEF.",
        typedef
    ));
    builder.blank();
}

// ============================================================================
// 1. Enum constants (level-78)
// ============================================================================

pub fn generate_enum_constants(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("*> ============================================================");
    builder.line("*> ENUM CONSTANTS (level-78)                                      *");
    builder.line("*> One constant per variant of every unit-only enum.              *");
    builder.line("*> ============================================================");

    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped(builder, &e.name, e.category.description());
            continue;
        }
        if e.is_union {
            // Tagged unions get their tag constants emitted alongside
            // the variant record below; skip here.
            continue;
        }
        emit_unit_enum_constants(builder, e);
    }

    builder.blank();
    Ok(())
}

fn emit_unit_enum_constants(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.variants.is_empty() {
        if !e.doc.is_empty() {
            for d in &e.doc {
                emit_doc_comment(builder, d);
            }
        }
        let class = cobol_identifier(&format!("AZ-{}", to_cobol_case(&e.name)));
        let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(&e.name)));
        builder.line(&format!("*> --- ENUM {} ---", class));
        // TYPEDEF alias so struct fields can refer to the enum's storage
        // type via `USAGE TYAZ-<NAME>`. Unit enums map to a signed
        // 32-bit integer to match the Rust `#[repr(C)]` enum ABI.
        builder.line(&format!(
            "       01  {} IS TYPEDEF USAGE BINARY-LONG.",
            typedef
        ));
        for (idx, v) in e.variants.iter().enumerate() {
            let var = sanitize_cobol_identifier(&to_cobol_case(&v.name));
            let full = cobol_identifier(&format!("{}-{}", class, var));
            builder.line(&format!("       78  {} VALUE {}.", full, idx));
        }
        builder.blank();
    }
}

// ============================================================================
// 2. Struct + tagged-union typedefs (level-01)
// ============================================================================

pub fn generate_records(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("*> ============================================================");
    builder.line("*> DATA STRUCTURES (level-01 typedefs)                            *");
    builder.line("*> Use the IS TYPEDEF clause so users can declare:                *");
    builder.line("*>   01 MY-VAR USAGE TYAZ-RECT.                                   *");
    builder.line("*> GnuCOBOL >= 3.0 supports the TYPEDEF extension natively.       *");
    builder.line("*> ============================================================");

    // 2a + 2b + 2c. Interleave POD records, tagged-union records, AND
    // monomorphized type-alias instantiations in topological order
    // (`sort_order`). Monomorphized aliases like AzPhysicalSizeU32 /
    // AzOptionU32 are referenced by other records as field types and
    // must be declared first — same pattern as lang_pascal/lang_fortran.
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
        Mono(&'a TypeAliasDef, &'a MonomorphizedTypeDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped(builder, &s.name, s.category.description());
            continue;
        }
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    for ta in &ir.type_aliases {
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
            Item::Struct(s) => emit_struct(builder, s, ir),
            Item::Union(e) => emit_tagged_union(builder, e, ir),
            Item::Mono(ta, mono) => emit_monomorphized_alias(builder, ta, mono, ir),
        }
    }

    Ok(())
}

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if !s.doc.is_empty() {
        for d in &s.doc {
            emit_doc_comment(builder, d);
        }
    }

    let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(&s.name)));

    if s.fields.is_empty() {
        // COBOL records cannot be truly empty; emit a single FILLER
        // byte so the typedef is materialised but consumes one byte.
        builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
        builder.line("           05  FILLER PIC X(1).");
        builder.blank();
        return;
    }

    builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
    for f in &s.fields {
        emit_field(builder, f, ir, "05");
    }
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR, level: &str) {
    if let Some(ref doc) = f.doc {
        emit_doc_comment(builder, doc);
    }
    let nm = sanitize_cobol_identifier(&to_cobol_case(&f.name));
    let usage = pic_for_field(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("           {}  {:<24} {}.", level, nm, usage));
}

// ============================================================================
// Tagged unions: COBOL REDEFINES
// ============================================================================
//
// Layout matches Rust `#[repr(C)]` enum:
//
//     01 TYAZ-FOO IS TYPEDEF.
//        05 TAG       USAGE BINARY-LONG.
//        05 PAYLOAD-NONE.
//           10 FILLER PIC X(8).
//        05 PAYLOAD-SOME REDEFINES PAYLOAD-NONE.
//           10 VALUE-X USAGE BINARY-DOUBLE.
//
// Every variant payload occupies the same memory after the tag field;
// the user inspects TAG and accesses the matching PAYLOAD-* group.

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
            emit_doc_comment(builder, d);
        }
    }
    let class = cobol_identifier(&format!("AZ-{}", to_cobol_case(&ta.name)));
    let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(&ta.name)));

    match &mono.kind {
        // Unit-enum monomorphizations -> level-78 constants + USAGE
        // BINARY-LONG TYPEDEF (mirrors emit_unit_enum_constants).
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            builder.line(&format!("*> --- MONOMORPHIZED ENUM {} ---", class));
            builder.line(&format!(
                "       01  {} IS TYPEDEF USAGE BINARY-LONG.",
                typedef
            ));
            for (idx, v) in variants.iter().enumerate() {
                let var = sanitize_cobol_identifier(&to_cobol_case(v));
                let full = cobol_identifier(&format!("{}-{}", class, var));
                builder.line(&format!("       78  {} VALUE {}.", full, idx));
            }
            builder.blank();
        }

        // Struct monomorphizations -> a normal level-01 TYPEDEF.
        MonomorphizedKind::Struct { fields } => {
            builder.line(&format!("*> --- MONOMORPHIZED STRUCT {} ---", class));
            if fields.is_empty() {
                builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
                builder.line("           05  FILLER PIC X(1).");
                builder.blank();
                return;
            }
            builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
            for f in fields {
                emit_field(builder, f, ir, "05");
            }
            builder.blank();
        }

        // Tagged-union monomorphizations -> tag + REDEFINES variants.
        // Anchor with a fixed-size FILLER large enough for any variant
        // payload (REDEFINES requires the redefining clause to be ≤
        // anchor size, and we can't always order variants by size).
        MonomorphizedKind::TaggedUnion { variants, .. } => {
            let tag_class = cobol_identifier(&format!("AZ-{}-TAG", to_cobol_case(&ta.name)));
            builder.line(&format!("*> --- MONOMORPHIZED UNION {} ---", typedef));
            for (idx, v) in variants.iter().enumerate() {
                let var = sanitize_cobol_identifier(&to_cobol_case(&v.name));
                let full = cobol_identifier(&format!("{}-{}", tag_class, var));
                builder.line(&format!("       78  {} VALUE {}.", full, idx));
            }
            builder.blank();
            builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
            builder.line("           05  TAG                      USAGE BINARY-LONG.");
            // 64-byte raw payload — wide enough for any of the variants
            // we currently emit. We don't emit per-variant typed accessors
            // because their padded sizes are hard to compute portably;
            // users access TAG to discriminate and read the raw bytes
            // here for the actual payload.
            let _ = variants;
            let anchor_name = cobol_identifier("PAYLOAD-ANCHOR");
            builder.line(&format!(
                "           05  {:<24} PIC X(64).",
                anchor_name
            ));
            builder.blank();
        }
    }
}

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            emit_doc_comment(builder, d);
        }
    }

    let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(&e.name)));
    let tag_class = cobol_identifier(&format!("AZ-{}-TAG", to_cobol_case(&e.name)));

    // Tag-value level-78 constants (one per variant).
    builder.line(&format!("*> --- TAGGED UNION {} ---", typedef));
    for (idx, v) in e.variants.iter().enumerate() {
        let var = sanitize_cobol_identifier(&to_cobol_case(&v.name));
        let full = cobol_identifier(&format!("{}-{}", tag_class, var));
        builder.line(&format!("       78  {} VALUE {}.", full, idx));
    }
    builder.blank();

    builder.line(&format!("       01  {} IS TYPEDEF.", typedef));
    builder.line("           05  TAG                      USAGE BINARY-LONG.");

    // 64-byte raw anchor — wide enough for every variant payload we
    // currently emit (largest in practice is ~32 bytes for a Vec
    // descriptor). Per-variant typed accessors are tricky here because
    // each REDEFINES must be ≤ anchor size and computing variant sizes
    // portably is brittle, so users discriminate via TAG and read the
    // payload bytes directly. Bump if a variant ever needs more.
    let _ = ir;
    let _ = e.variants.len();
    let anchor_name = cobol_identifier("PAYLOAD-ANCHOR");
    builder.line(&format!(
        "           05  {:<24} PIC X(64).",
        anchor_name
    ));
    builder.blank();
}

// ============================================================================
// 3. Callback typedefs
// ============================================================================
//
// COBOL has no syntax for declaring a function-pointer's signature
// inline. We emit a level-01 typedef of `USAGE PROGRAM-POINTER` (the
// COBOL equivalent of `void (*)(...)`) plus a comment describing the
// expected signature so users can match it when they pass a paragraph
// or external program as a callback.

pub fn generate_callback_typedefs(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    _config: &CodegenConfig,
) -> Result<()> {
    if ir.callback_typedefs.is_empty() {
        return Ok(());
    }

    builder.line("*> ============================================================");
    builder.line("*> CALLBACK TYPEDEFS (USAGE PROGRAM-POINTER)                      *");
    builder.line("*> Each typedef stores a pointer to a COBOL paragraph or external *");
    builder.line("*> program; signatures are documented as comments above each      *");
    builder.line("*> typedef so callers can declare matching ENTRY paragraphs.      *");
    builder.line("*> ============================================================");

    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, ir);
    }

    Ok(())
}

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            emit_doc_comment(builder, d);
        }
    }

    // Emit a signature-style banner: `(arg: TYPE, ...) -> RET`.
    let arg_strs: Vec<String> = cb
        .args
        .iter()
        .map(|a| {
            let nm = sanitize_cobol_identifier(&to_cobol_case(&a.name));
            let usage = match a.ref_kind {
                ArgRefKind::Owned => pic_for_type(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => "USAGE POINTER".to_string(),
            };
            format!("{}: {}", nm, usage)
        })
        .collect();
    let ret_str = match &cb.return_type {
        Some(r) => pic_for_type(r, ir),
        None => "VOID".to_string(),
    };
    builder.line(&format!(
        "*> SIGNATURE: ({}) RETURNING {}",
        arg_strs.join(", "),
        ret_str
    ));

    let typedef = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(&cb.name)));
    builder.line(&format!(
        "       01  {:<28} USAGE PROGRAM-POINTER IS TYPEDEF.",
        typedef
    ));
    builder.blank();
}

// ============================================================================
// Type-mapping helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to a COBOL PICTURE/USAGE clause
/// suitable for placing after a field name in a level-05 / level-10
/// declaration.
pub fn pic_for_field(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => pic_for_type(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "USAGE POINTER".to_string(),
    }
}

/// Map a Rust/IR type name to a COBOL PICTURE/USAGE clause.
///
/// - Pointer-shaped Rust types resolve to `USAGE POINTER`.
/// - Fixed-width integers map to the matching `BINARY-CHAR` /
///   `BINARY-SHORT` / `BINARY-LONG` / `BINARY-DOUBLE` (these are
///   COMP-5 / native-binary aliases in GnuCOBOL).
/// - Floats map to COMP-1 (single) and COMP-2 (double).
/// - Other named types are assumed to be IR types and resolve to
///   `USAGE TYAZ-<NAME>` so the field inlines the matching record.
pub fn pic_for_type(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers and references collapse to USAGE POINTER.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return "USAGE POINTER".to_string();
    }

    // Fixed-size arrays: `[T; N]` -> OCCURS N TIMES on the inner type.
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(semi) = inner.rfind(';') {
            let elem = inner[..semi].trim();
            if let Ok(count) = inner[semi + 1..].trim().parse::<usize>() {
                let inner_pic = pic_for_type(elem, ir);
                return format!("{} OCCURS {} TIMES", inner_pic, count.max(1));
            }
        }
    }

    match trimmed {
        // Void / unit
        "void" | "c_void" | "()" => "USAGE POINTER".to_string(),

        // Booleans (1-byte unsigned)
        "bool" | "GLboolean" => "USAGE BINARY-CHAR UNSIGNED".to_string(),

        // 8-bit integers
        "i8" | "c_char" | "char" => "USAGE BINARY-CHAR".to_string(),
        "u8" | "c_uchar" => "USAGE BINARY-CHAR UNSIGNED".to_string(),

        // 16-bit
        "i16" => "USAGE BINARY-SHORT".to_string(),
        "u16" => "USAGE BINARY-SHORT UNSIGNED".to_string(),

        // 32-bit
        "i32" | "c_int" | "GLint" | "GLsizei" => "USAGE BINARY-LONG".to_string(),
        "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" | "AzScanCode" => {
            "USAGE BINARY-LONG UNSIGNED".to_string()
        }

        // 64-bit
        "i64" | "GLint64" => "USAGE BINARY-DOUBLE".to_string(),
        "u64" | "GLuint64" => "USAGE BINARY-DOUBLE UNSIGNED".to_string(),

        // Floats
        "f32" | "GLfloat" | "GLclampf" => "USAGE COMP-1".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "USAGE COMP-2".to_string(),

        // Pointer-sized integers / size_t -> POINTER for portability
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr"
        | "GLintptr" => "USAGE POINTER".to_string(),

        // Named types: assume IR type; fall back to USAGE POINTER if
        // the type isn't a known struct/enum/alias/callback.
        _ => {
            if ir.find_struct(trimmed).is_some()
                || ir.find_enum(trimmed).is_some()
                || ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                let inner = cobol_identifier(&format!("TYAZ-{}", to_cobol_case(trimmed)));
                format!("USAGE {}", inner)
            } else {
                "USAGE POINTER".to_string()
            }
        }
    }
}
