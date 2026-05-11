//! Algol 68 MODE / type emission.
//!
//! Strategy for each IR construct:
//!
//! - **Plain struct** -> `MODE AZFOO = STRUCT (INT a, REAL b, REF AZBAR c);`
//!   Algol 68 STRUCT field syntax is `MODE name`, comma-separated, parens.
//! - **Empty / opaque struct** -> a one-INT-field struct
//!   `MODE AZFOO = STRUCT (INT opaque dummy);` so it can still be passed
//!   by reference. Algol 68 has no zero-sized records.
//! - **Unit enum** -> a sequence of named INT constants:
//!   `INT azbuttontype primary = 0, azbuttontype secondary = 1;`
//!   Algol 68 has no `enum` keyword.
//! - **Tagged-union enum** -> Algol 68 *does* have a native `UNION` MODE:
//!   `MODE AZFOOPAYLOAD = UNION (INT, REAL, REF VOID);` plus a tagged
//!   wrapper `MODE AZFOO = STRUCT (INT tag, AZFOOPAYLOAD payload);`.
//!   Discriminator integer constants are emitted alongside.
//! - **Callback typedef** -> a `MODE AZFOOCALLBACK = PROC (...) RET;`
//!   declaration; Algol 68's procedure-mode syntax matches what we need.
//! - **Recursive / VecRef / GenericTemplate / DestructorOrClone** are
//!   skipped with `# SKIPPED: <reason> #` comments.
//!
//! Algol 68 is single-pass: every MODE referenced in a STRUCT body must
//! already be declared. We keep this honest by emitting in topological
//! order: callback typedefs and unit enums first, then plain records,
//! then tagged unions (which can reference any earlier MODE).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind,
    StructDef, TypeCategory,
};
use super::{
    algol_mode_name, camel_or_snake_to_spaced_lower_pub, map_type_to_algol, ptr_type,
    sanitize_comment, sanitize_identifier,
};

// ============================================================================
// Top-level type-block emission
// ============================================================================

pub fn generate_types(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.line("# MODE / type definitions: enums (as INT constants), STRUCTs, UNIONs, PROCs.   #");
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.blank();

    // 1. Unit-only enums -> INT constants (no MODE; Algol 68 has no enum kw).
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            emit_skipped_enum(builder, e);
            continue;
        }
        if !e.is_union {
            emit_unit_enum_constants(builder, e);
        }
    }
    builder.blank();

    // 2. Callback PROC modes (referenced by struct fields below).
    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, ir);
    }
    builder.blank();

    // 3. Plain record MODEs.
    for s in &ir.structs {
        if !should_include_struct(s, config) {
            emit_skipped_struct(builder, s);
            continue;
        }
        emit_struct(builder, s, ir);
    }

    // 4. Tagged-union MODEs (use Algol 68's native UNION).
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        if e.is_union {
            emit_tagged_union(builder, e, ir);
        }
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Inclusion filters (mirror lang_pascal/lang_ada)
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
        "# SKIPPED: struct {} ({}) #",
        s.name,
        s.category.description()
    ));
}

fn emit_skipped_enum(builder: &mut CodeBuilder, e: &EnumDef) {
    builder.line(&format!(
        "# SKIPPED: enum {} ({}) #",
        e.name,
        e.category.description()
    ));
}

// ============================================================================
// Unit-only enum -> INT constants
// ============================================================================

fn emit_unit_enum_constants(builder: &mut CodeBuilder, e: &EnumDef) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("# {} #", sanitize_comment(d)));
        }
    }

    // Always emit a MODE alias so later PROC return types and STRUCT
    // fields that mention the enum (e.g. `PROC (...) AZUPDATE`) resolve
    // to a known tag. Without this, a68g raises "tag <FOO> has not
    // been declared properly" for every callback typedef whose return
    // type is a unit enum.
    let mode = algol_mode_name(&e.name);
    builder.line(&format!("MODE {} = INT;", mode));

    if e.variants.is_empty() {
        builder.line(&format!(
            "# enum {} has no variants — emitted as MODE alias only #",
            e.name
        ));
        builder.blank();
        return;
    }

    let prefix = camel_or_snake_to_spaced_lower_pub(&e.name);

    // Emit one `INT <prefix> <variant> = N;` per variant. We could batch
    // them on a single comma-separated line (Algol 68 allows `INT a = 0,
    // b = 1;`), but separate lines keep the diff readable.
    for (i, v) in e.variants.iter().enumerate() {
        let variant_lower = camel_or_snake_to_spaced_lower_pub(&v.name);
        builder.line(&format!(
            "INT {} {} = {};",
            prefix, variant_lower, i
        ));
    }
    builder.blank();
}

// ============================================================================
// Plain record -> MODE AZFOO = STRUCT (...)
// ============================================================================

fn emit_struct(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("# {} #", sanitize_comment(d)));
        }
    }

    let mode = algol_mode_name(&s.name);

    if s.fields.is_empty() {
        // Algol 68 has no zero-sized struct. Emit a single-INT placeholder
        // so the MODE still exists and can be passed by REF.
        builder.line(&format!(
            "MODE {} = STRUCT (INT opaque dummy); # opaque #",
            mode
        ));
        builder.blank();
        return;
    }

    builder.line(&format!("MODE {} = STRUCT (", mode));
    builder.indent();
    let last = s.fields.len() - 1;
    for (i, f) in s.fields.iter().enumerate() {
        emit_field(builder, f, ir, i == last);
    }
    builder.dedent();
    builder.line(");");
    builder.blank();
}

fn emit_field(builder: &mut CodeBuilder, f: &FieldDef, ir: &CodegenIR, is_last: bool) {
    if let Some(ref doc) = f.doc {
        builder.line(&format!("# {} #", sanitize_comment(doc)));
    }
    let mode = field_type(&f.type_name, &f.ref_kind, ir);
    let nm = sanitize_identifier(&f.name);
    let comma = if is_last { "" } else { "," };
    builder.line(&format!("{} {}{}", mode, nm, comma));
}

// ============================================================================
// Tagged-union enum -> MODE = STRUCT(INT tag, UNION payload)
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("# {} #", sanitize_comment(d)));
        }
    }

    let mode = algol_mode_name(&e.name);
    let tag_prefix = camel_or_snake_to_spaced_lower_pub(&e.name);

    // 1. Discriminator constants.
    builder.line(&format!("# Tag values for {} #", mode));
    for (i, v) in e.variants.iter().enumerate() {
        let variant_lower = camel_or_snake_to_spaced_lower_pub(&v.name);
        builder.line(&format!(
            "INT {} tag {} = {};",
            tag_prefix, variant_lower, i
        ));
    }

    // 2. Collect payload modes for the UNION.
    let mut union_modes: Vec<String> = Vec::new();
    let mut has_unit_variant = false;
    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                has_unit_variant = true;
            }
            EnumVariantKind::Tuple(types) => {
                if types.is_empty() {
                    has_unit_variant = true;
                } else if types.len() == 1 {
                    let (ty, rk) = &types[0];
                    let m = field_type(ty, rk, ir);
                    if !union_modes.contains(&m) {
                        union_modes.push(m);
                    }
                } else {
                    // Multiple payload fields — fall back to REF VOID.
                    let m = "REF VOID".to_string();
                    if !union_modes.contains(&m) {
                        union_modes.push(m);
                    }
                }
            }
            EnumVariantKind::Struct(_fields) => {
                // Anonymous struct payload — Algol 68 UNION cannot host
                // an inline anonymous record. Degrade to REF VOID.
                let m = "REF VOID".to_string();
                if !union_modes.contains(&m) {
                    union_modes.push(m);
                }
            }
        }
    }
    if has_unit_variant && !union_modes.iter().any(|m| m == "VOID") {
        // Algol 68 UNION requires at least two distinct member modes for
        // the discriminator to make sense. We add the synthetic placeholder
        // `INT` for unit variants if no other INT-family member is present.
        if !union_modes.iter().any(|m| m == "INT") {
            union_modes.push("INT".to_string());
        }
    }

    if union_modes.is_empty() {
        union_modes.push("INT".to_string());
    }

    // 3. Emit the UNION mode and its STRUCT wrapper. Algol 68 UNION
    //    requires >= 2 distinct member modes; if we only have one,
    //    skip the UNION layer entirely and store the payload directly.
    if union_modes.len() < 2 {
        builder.line(&format!(
            "MODE {} = STRUCT (INT tag, {} payload);",
            mode, union_modes[0]
        ));
    } else {
        // Concatenate (no space) — a68g UPPER stropping treats a space
        // between bold identifiers as a token boundary, so
        // `AZFOO PAYLOAD` lexes as two separate MODE names.
        let payload_mode = format!("{}PAYLOAD", mode);
        builder.line(&format!(
            "MODE {} = UNION ({});",
            payload_mode,
            union_modes.join(", ")
        ));
        builder.line(&format!(
            "MODE {} = STRUCT (INT tag, {} payload);",
            mode, payload_mode
        ));
    }
    builder.blank();
}

// ============================================================================
// Callback PROC mode
// ============================================================================

fn emit_callback_typedef(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("# {} #", sanitize_comment(d)));
        }
    }
    let mode = algol_mode_name(&cb.name);

    let args: Vec<String> = cb
        .args
        .iter()
        .map(|arg| arg_type(&arg.type_name, &arg.ref_kind, ir))
        .collect();

    let signature = if let Some(ret) = &cb.return_type {
        let ret_ty = map_type_to_algol(ret, ir);
        if args.is_empty() {
            format!("MODE {} = PROC {};", mode, ret_ty)
        } else {
            format!("MODE {} = PROC ({}) {};", mode, args.join(", "), ret_ty)
        }
    } else if args.is_empty() {
        format!("MODE {} = PROC VOID;", mode)
    } else {
        format!("MODE {} = PROC ({}) VOID;", mode, args.join(", "))
    };
    builder.line(&signature);
}

// ============================================================================
// Field / argument type helpers
// ============================================================================

/// Map `(type_name, FieldRefKind)` to the Algol 68 MODE expression for
/// a STRUCT field.
pub(crate) fn field_type(type_name: &str, ref_kind: &FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_type_to_algol(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => ptr_type(type_name, ir),
    }
}

/// Map `(type_name, ArgRefKind)` to the Algol 68 MODE expression for a
/// PROC argument or callback typedef argument.
pub(crate) fn arg_type(type_name: &str, ref_kind: &ArgRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        ArgRefKind::Owned => map_type_to_algol(type_name, ir),
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            ptr_type(type_name, ir)
        }
    }
}
