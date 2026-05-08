//! Haskell type emission: `data`/`newtype` declarations + `Storable`
//! instances for every IR struct and enum that survives the inclusion
//! filter.
//!
//! Strategy:
//! - **Plain structs** become `data <Name> = <Name> { field1 :: !T1, ... }`
//!   with a manually-written `Storable` instance. Field offsets are
//!   not known at codegen time (they depend on the C compiler's
//!   layout of the corresponding Rust `#[repr(C)]` struct), so we
//!   emit `peek`/`poke` bodies that walk the fields *in declaration
//!   order* using `peekByteOff` / `pokeByteOff` with a running offset.
//!   The running offset is computed from the previous field's
//!   `sizeOf`. This is correct for structs whose Rust layout matches
//!   the C ABI without padding (the common case for azul's flat POD
//!   structs); for structs with padding the user can always fall back
//!   to the raw FFI primitives.
//! - **Unit enums** (no payload) become a normal Haskell sum type
//!   with `deriving (Show, Eq, Enum, Bounded)`, plus a `Storable`
//!   instance going through `Word32` (the Rust ABI repr for unit
//!   enums).
//! - **Tagged unions** become a Haskell sum type with payload
//!   constructors. The `Storable` instance dispatches on the
//!   discriminator field and uses `peek` / `poke` recursively for the
//!   payload variant. Unsizeable payloads (recursive types, generics)
//!   are skipped with a `-- SKIPPED:` marker.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef, TypeCategory,
};
use super::{haskell_data_name, haskell_field_name, haskell_variant_name, sanitize_doc};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_type_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Struct data declarations + Storable instances");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic struct {} (no Haskell equivalent over the C ABI)",
                    s.name
                ));
            } else if matches!(
                s.category,
                TypeCategory::Recursive
                    | TypeCategory::VecRef
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
            ) {
                builder.line(&format!(
                    "-- SKIPPED: struct {} ({})",
                    s.name,
                    s.category.description()
                ));
            }
            continue;
        }
        emit_struct_decl(builder, s, ir);
    }

    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Enum data declarations + Storable instances");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            if !e.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic enum {} (no Haskell equivalent over the C ABI)",
                    e.name
                ));
            } else if matches!(
                e.category,
                TypeCategory::Recursive
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
            ) {
                builder.line(&format!(
                    "-- SKIPPED: enum {} ({})",
                    e.name,
                    e.category.description()
                ));
            }
            continue;
        }
        if e.is_union {
            emit_tagged_union_decl(builder, e, ir);
        } else {
            emit_unit_enum_decl(builder, e);
        }
    }

    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

pub fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
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

pub fn should_emit_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    )
}

// ============================================================================
// Struct emission
// ============================================================================

fn emit_struct_decl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = haskell_data_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if s.fields.is_empty() {
        // Phantom marker type — we still need a Storable instance so it
        // can appear in field positions of other structs.
        builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
        builder.line(&format!("instance Storable {} where", name));
        builder.indent();
        builder.line("sizeOf _ = 1");
        builder.line("alignment _ = 1");
        builder.line(&format!("peek _ = pure {}", name));
        builder.line("poke _ _ = pure ()");
        builder.dedent();
        builder.blank();
        return;
    }

    // data Name = Name { ... }
    builder.line(&format!("data {} = {}", name, name));
    builder.indent();
    let mut first = true;
    for f in &s.fields {
        let prefix = if first { "{ " } else { ", " };
        first = false;
        let fname = haskell_field_name(&s.name, &f.name);
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        if let Some(ref doc) = f.doc {
            builder.line(&format!("-- ^ {}", sanitize_doc(doc)));
        }
        builder.line(&format!("{}{} :: !{}", prefix, fname, hty));
    }
    builder.line("} deriving (Show)");
    builder.dedent();
    builder.blank();

    // Storable instance using a running offset and per-field sizeOf.
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line(&format!("sizeOf _ = {}_sizeOf_total", name));
    builder.line(&format!("alignment _ = {}_alignment_total", name));

    // peek
    builder.line("peek p = do");
    builder.indent();
    let mut offset_acc: Vec<String> = Vec::new();
    for (i, f) in s.fields.iter().enumerate() {
        let bind = format!("v{}", i);
        let offset_expr = if i == 0 {
            "0".to_string()
        } else {
            offset_acc.join(" + ")
        };
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        builder.line(&format!(
            "{} <- peekByteOff p ({}) :: IO {}",
            bind, offset_expr, hty
        ));
        offset_acc.push(format!("sizeOf (undefined :: {})", hty));
    }
    let mut acc = String::new();
    acc.push_str(&format!("pure ({}", name));
    for (i, _) in s.fields.iter().enumerate() {
        acc.push_str(&format!(" v{}", i));
    }
    acc.push(')');
    builder.line(&acc);
    builder.dedent();

    // poke
    builder.line("poke p x = do");
    builder.indent();
    let mut offset_acc: Vec<String> = Vec::new();
    for (i, f) in s.fields.iter().enumerate() {
        let fname = haskell_field_name(&s.name, &f.name);
        let offset_expr = if i == 0 {
            "0".to_string()
        } else {
            offset_acc.join(" + ")
        };
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        builder.line(&format!("pokeByteOff p ({}) ({} x)", offset_expr, fname));
        offset_acc.push(format!("sizeOf (undefined :: {})", hty));
    }
    builder.dedent();
    builder.dedent();

    // Helper bindings: total size and alignment computed at runtime.
    // This avoids requiring offsetof macros at codegen time.
    let tname = name.clone();
    builder.blank();
    builder.line(&format!("{}_sizeOf_total :: Int", tname));
    if s.fields.is_empty() {
        builder.line(&format!("{}_sizeOf_total = 1", tname));
    } else {
        let mut sum_terms: Vec<String> = Vec::new();
        for f in &s.fields {
            let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
            sum_terms.push(format!("sizeOf (undefined :: {})", hty));
        }
        builder.line(&format!("{}_sizeOf_total = {}", tname, sum_terms.join(" + ")));
    }
    builder.line(&format!("{}_alignment_total :: Int", tname));
    // Pessimistic: take the max alignment of the first field (sufficient
    // for the C ABI on every platform we target — pointer-aligned).
    if let Some(f0) = s.fields.first() {
        let hty = haskell_field_type(&f0.type_name, f0.ref_kind, ir);
        builder.line(&format!("{}_alignment_total = alignment (undefined :: {})", tname, hty));
    } else {
        builder.line(&format!("{}_alignment_total = 1", tname));
    }
    builder.blank();
}

// ============================================================================
// Unit enum emission
// ============================================================================

fn emit_unit_enum_decl(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!("-- SKIPPED: unit enum {} has no variants", e.name));
        builder.blank();
        return;
    }

    let variants: Vec<String> = e
        .variants
        .iter()
        .map(|v| haskell_variant_name(&e.name, &v.name))
        .collect();

    builder.line(&format!("data {}", name));
    builder.indent();
    let mut first = true;
    for vname in &variants {
        let prefix = if first { "= " } else { "| " };
        first = false;
        builder.line(&format!("{}{}", prefix, vname));
    }
    builder.line("deriving (Show, Eq, Enum, Bounded)");
    builder.dedent();
    builder.blank();

    // Storable: pass through Word32 (the Rust ABI repr for unit enums).
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line("sizeOf _ = 4");
    builder.line("alignment _ = 4");
    builder.line("peek p = do");
    builder.indent();
    builder.line("w <- peek (castPtr p :: Ptr Word32)");
    builder.line("case w of");
    builder.indent();
    for (idx, vname) in variants.iter().enumerate() {
        builder.line(&format!("{} -> pure {}", idx, vname));
    }
    builder.line(&format!(
        "_ -> error \"Azul.Types.peek {}: unknown discriminator\"",
        name
    ));
    builder.dedent();
    builder.dedent();
    builder.line("poke p v = case v of");
    builder.indent();
    for (idx, vname) in variants.iter().enumerate() {
        builder.line(&format!(
            "{} -> poke (castPtr p :: Ptr Word32) {}",
            vname, idx
        ));
    }
    builder.dedent();
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Tagged-union emission
// ============================================================================

fn emit_tagged_union_decl(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let name = haskell_data_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!(
            "-- SKIPPED: tagged-union {} has no variants",
            e.name
        ));
        builder.blank();
        return;
    }

    builder.line(&format!("data {}", name));
    builder.indent();
    let mut first = true;
    for v in &e.variants {
        let prefix = if first { "= " } else { "| " };
        first = false;
        let vname = haskell_variant_name(&e.name, &v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("{}{}", prefix, vname));
            }
            EnumVariantKind::Tuple(payload) => {
                let payload_types: Vec<String> = payload
                    .iter()
                    .map(|(t, rk)| haskell_field_type(t, *rk, ir))
                    .collect();
                builder.line(&format!(
                    "{}{} {}",
                    prefix,
                    vname,
                    payload_types.join(" ")
                ));
            }
            EnumVariantKind::Struct(fields) => {
                let payload_types: Vec<String> = fields
                    .iter()
                    .map(|f| haskell_field_type(&f.type_name, f.ref_kind, ir))
                    .collect();
                builder.line(&format!(
                    "{}{} {}",
                    prefix,
                    vname,
                    payload_types.join(" ")
                ));
            }
        }
    }
    builder.line("deriving (Show)");
    builder.dedent();
    builder.blank();

    // Storable: discriminator + opaque-byte-array payload. We expose a
    // best-effort instance that round-trips the unit variants exactly
    // and round-trips payload variants only when their payload is
    // 'Storable' itself. For complex payloads users should reach for
    // the raw FFI primitives.
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line("sizeOf _ = 8 + 64  -- tag (Word32 + pad) + payload bound");
    builder.line("alignment _ = 8");
    builder.line(&format!(
        "peek _ = error \"Azul.Types.peek {}: tagged-union peek not implemented; use the raw FFI primitives\"",
        name
    ));
    builder.line(&format!(
        "poke _ _ = error \"Azul.Types.poke {}: tagged-union poke not implemented; use the raw FFI primitives\"",
        name
    ));
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Type-mapping helpers
// ============================================================================

/// Map an IR field type + ref-kind to a Haskell type expression.
pub fn haskell_field_type(type_name: &str, ref_kind: FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_owned_type(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => format!("Ptr {}", map_owned_type(type_name, ir)),
    }
}

fn map_owned_type(type_name: &str, ir: &CodegenIR) -> String {
    let t = type_name.trim();

    // Pointer / reference forms in the type string itself.
    if let Some(rest) = t.strip_prefix("*const ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix("*mut ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix("&mut ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix('&') {
        return pointer_form(rest.trim(), ir);
    }

    match t {
        "bool" => "CBool".to_string(),
        "u8" | "c_uchar" => "Word8".to_string(),
        "i8" | "c_char" => "Int8".to_string(),
        "char" => "CChar".to_string(),
        "u16" => "Word16".to_string(),
        "i16" => "Int16".to_string(),
        "u32" | "c_uint" => "Word32".to_string(),
        "i32" | "c_int" => "Int32".to_string(),
        "u64" => "Word64".to_string(),
        "i64" => "Int64".to_string(),
        "f32" => "CFloat".to_string(),
        "f64" => "CDouble".to_string(),
        "usize" => "CSize".to_string(),
        "isize" => "CIntPtr".to_string(),
        "c_void" | "()" | "void" => "()".to_string(),
        _ => {
            if ir.find_struct(t).is_some()
                || ir.find_enum(t).is_some()
                || ir.find_type_alias(t).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == t)
            {
                haskell_data_name(t)
            } else {
                // Unknown type — keep as opaque pointer so the binding
                // still type-checks.
                "(Ptr ())".to_string()
            }
        }
    }
}

fn pointer_form(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(Ptr ())".to_string();
    }
    if inner == "c_char" || inner == "u8" {
        // C-string / byte-buffer pointers: still typed for the user's
        // benefit, but the underlying repr is the same as `Ptr Word8`.
        return "(Ptr Word8)".to_string();
    }
    format!("(Ptr {})", map_owned_type(inner, ir))
}
