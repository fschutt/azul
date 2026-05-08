//! VB6 `Public Type` / `Public Enum` emission.
//!
//! Strategy:
//!
//! - **Unit-only enums** → `Public Enum AzFoo : AzFoo_A = 0 : AzFoo_B = 1
//!   : End Enum`. VB6 enums are `Long`-backed integers; values are
//!   pinned explicitly so the layout matches the C-ABI tag values.
//! - **Tagged-union enums** → VB6 has **no native `Union` type**. We
//!   emit a `Public Type` with a `tag As Long` field plus a
//!   `payload(0 To N - 1) As Byte` byte array sized to the largest
//!   variant. The user must call `CopyMemory` (`RtlMoveMemory`) to
//!   marshal payload bytes in/out of a typed local. We also emit a
//!   tag-enum companion `Public Enum AzFooTag` with the variant
//!   discriminator values. Each variant gets a comment block listing
//!   its payload-field layout so the user knows what to copy.
//! - **POD structs** → `Public Type AzFoo ... End Type`. VB6 packs
//!   fields with natural-alignment by default, matching Rust's
//!   `extern "C"` ABI.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** are
//!   skipped with `' SKIPPED: <reason>` line comments.
//!
//! VB6 quirks worth recording:
//!
//! - VB6 cannot pass user-defined types (UDTs) `ByVal` to a `Declare`
//!   — only `ByRef`. Functions whose C signature takes a struct by
//!   value will be flagged with `' SKIPPED: struct-by-value` in
//!   `functions.rs`. (The IR already exposes `ArgRefKind::Owned` for
//!   these cases.)
//! - VB6 has no enum payload concept; tag-enum constants pin the
//!   discriminator value but the variant's payload bytes have to be
//!   extracted manually.
//! - VB6 string handling: `String` is BSTR (UTF-16). C `char*`
//!   arguments declared `ByVal As String` get auto-marshalled to
//!   ANSI on the way out, which loses non-ASCII characters. We
//!   default to `Long`-as-pointer for `*const c_char` and let the
//!   caller manage UTF-8 conversion via `StrPtr` + `CopyMemory`.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    StructDef, TypeCategory,
};
use super::{ffi_type_name, map_type_to_vb6, sanitize_comment, sanitize_identifier};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("' --------------------------------------------------------------------");
    builder.line("' Type definitions: enums, POD records, tagged-union shims.");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

    // Helpful CopyMemory declaration for tagged-union payload extraction.
    builder.line("' Marshal helper for tagged-union payload extraction. VB6 has no");
    builder.line("' native Union type and no native typed-pointer dereference, so");
    builder.line("' callers extract variant payloads with this RtlMoveMemory wrapper.");
    builder.line(
        "Public Declare Sub CopyMemory Lib \"kernel32\" Alias \"RtlMoveMemory\" \
         (ByRef Destination As Any, ByRef Source As Any, ByVal Length As Long)",
    );
    builder.blank();

    // 1. Unit-only enums first so they may be referenced as field types.
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum(builder, e);
        }
    }

    // 2. Tagged-union enums (Type with tag + Byte payload array).
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            emit_tagged_union(builder, e, ir);
        }
    }

    // 3. POD records.
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        emit_struct(builder, s, ir);
    }

    // 4. Callback procedural typedefs. VB6 has no first-class function
    //    pointers, but the convention is to pass the address of a
    //    Public Function via `AddressOf` — and the actual *typedef* on
    //    the VB6 side is just a `Long`-as-pointer. We emit a comment
    //    block documenting the expected callback signature so users
    //    know what to write on the VB6 side.
    for cb in &ir.callback_typedefs {
        emit_callback_typedef_comment(builder, cb, ir);
    }

    Ok(())
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

    if e.variants.is_empty() {
        // Empty enums aren't valid in VB6; emit a degenerate type alias
        // (a Public Const placeholder for documentation).
        builder.line(&format!("' SKIPPED: enum {} has no variants", e.name));
        return;
    }

    builder.line(&format!("Public Enum {}", t));
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        let nm = sanitize_identifier(&v.name);
        // Pin the value explicitly so the layout matches the C-ABI.
        builder.line(&format!("{}_{} = {}", t, nm, i));
    }
    builder.dedent();
    builder.line("End Enum");
    builder.blank();
}

// ============================================================================
// Tagged-union enum
// ============================================================================
//
// VB6 has no native `Union` type. We emit:
//
//   1. A companion `Public Enum AzFooTag` with one constant per variant
//      so user code can compare `foo.tag` against named values.
//   2. A `Public Type AzFoo` containing:
//        tag       As Long
//        payload(0 To N - 1) As Byte    ' N = max payload size in bytes
//      The byte array is sized to the largest variant. Smaller variants
//      simply leave the trailing bytes unused.
//   3. A documentation comment block listing each variant's payload
//      shape so the user knows what to read out via CopyMemory.
//
// We DO NOT compute the actual largest variant size (that requires
// platform-specific size knowledge for nested types). Instead we
// emit a generous fixed buffer (256 bytes) that comfortably covers
// every payload Azul exposes, with a `' SKIPPED:` note explaining
// the trade-off.

const PAYLOAD_BUFFER_BYTES: usize = 256;

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    let t = ffi_type_name(&e.name);
    let tag_t = format!("{}Tag", t);

    // 1. Companion tag enum.
    builder.line(&format!("Public Enum {}", tag_t));
    builder.indent();
    for (i, v) in e.variants.iter().enumerate() {
        let nm = sanitize_identifier(&v.name);
        builder.line(&format!("{}_{} = {}", tag_t, nm, i));
    }
    builder.dedent();
    builder.line("End Enum");
    builder.blank();

    // 2. Payload-shape documentation per variant.
    builder.line(&format!("' Tagged-union variants for {}:", t));
    builder.line(
        "' SKIPPED: VB6 has no native Union type. We emit a fixed 256-byte payload",
    );
    builder.line("' buffer; callers must use CopyMemory with the per-variant layout below.");
    for v in &e.variants {
        let nm = sanitize_identifier(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("'   {}_{}: (no payload)", tag_t, nm));
            }
            EnumVariantKind::Tuple(types) => {
                let parts: Vec<String> = types
                    .iter()
                    .map(|(ty, ref_kind)| field_type_for_ref_kind(ty, ref_kind, ir))
                    .collect();
                builder.line(&format!("'   {}_{}: ({})", tag_t, nm, parts.join(", ")));
            }
            EnumVariantKind::Struct(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{} As {}",
                            sanitize_identifier(&f.name),
                            field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir)
                        )
                    })
                    .collect();
                builder.line(&format!("'   {}_{}: ({})", tag_t, nm, parts.join("; ")));
            }
        }
    }

    // 3. Outer Type: tag + opaque byte payload.
    builder.line(&format!("Public Type {}", t));
    builder.indent();
    builder.line("tag As Long");
    builder.line(&format!("payload(0 To {}) As Byte", PAYLOAD_BUFFER_BYTES - 1));
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
        // Opaque type — VB6 requires at least one field. Emit a single
        // padding byte. Real users only ever hold a `Long`-as-pointer
        // to such records; the byte is never read.
        builder.line(&format!("Public Type {}", t));
        builder.indent();
        builder.line("opaque_ As Byte  ' opaque, no fields exposed via FFI");
        builder.dedent();
        builder.line("End Type");
        builder.blank();
        return;
    }

    builder.line(&format!("Public Type {}", t));
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
    let vb_ty = field_type_for_ref_kind(&f.type_name, &f.ref_kind, ir);
    builder.line(&format!("{} As {}", nm, vb_ty));
}

// ============================================================================
// Callback typedef (commented documentation; VB6 has no fn pointer types)
// ============================================================================

fn emit_callback_typedef_comment(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }
    let t = ffi_type_name(&cb.name);
    builder.line(&format!("' Callback typedef: {}", t));
    builder.line("' SKIPPED: VB6 has no first-class function-pointer types. Pass the");
    builder.line("' address of a Public Function via `AddressOf MyHandler` (returns Long).");
    builder.line("' Expected callback signature on the VB6 side:");

    let args: Vec<String> = cb
        .args
        .iter()
        .map(|a| {
            let vb_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_vb6(&a.type_name, ir),
                _ => "Long".to_string(), // Pointer args become Long.
            };
            format!("ByVal {} As {}", sanitize_identifier(&a.name), vb_ty)
        })
        .collect();

    let header = if let Some(ret) = &cb.return_type {
        let vb_ret = map_type_to_vb6(ret, ir);
        format!("'   Public Function MyHandler ({}) As {}", args.join(", "), vb_ret)
    } else {
        format!("'   Public Sub MyHandler ({})", args.join(", "))
    };
    builder.line(&header);
    builder.blank();
}

// ============================================================================
// Field/argument type helpers
// ============================================================================

/// Map a `(type_name, FieldRefKind)` pair to the VB6 field type.
pub(crate) fn field_type_for_ref_kind(
    type_name: &str,
    ref_kind: &FieldRefKind,
    ir: &CodegenIR,
) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_vb6(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "Long".to_string(),
    }
}
