//! Idiomatic Ada wrapper types using `Ada.Finalization.Controlled`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit:
//!
//! - In the **spec** (`azul.ads`):
//!     - A tagged record `type Foo is new Ada.Finalization.Controlled with
//!       record Inner : aliased Az_Foo; Owned : Boolean := True; end record;`
//!     - `overriding procedure Finalize (Self : in out Foo);`
//!     - `overriding procedure Adjust (Self : in out Foo);` (set the
//!       Owned flag conservatively — clones aren't free, so we don't
//!       silently double-free).
//! - In the **body** (`azul.adb`):
//!     - The implementation of `Finalize`, which calls
//!       `Az_Foo_Delete (Self.Inner'Access)` once. After finalization the
//!       Owned flag is cleared so a second pass is a no-op.
//!     - The implementation of `Adjust`, which marks the freshly assigned
//!       copy as not-owned by default (Ada `Adjust` runs after a copy;
//!       we cannot safely deep-copy through the C ABI without an
//!       explicit clone API, so the safe default is "the copy does not
//!       own").
//!
//! Plain POD structs without a `_delete` get *no* wrapper. Tagged-union
//! enums likewise: the FFI variant record is the user-facing surface.

use anyhow::Result;
use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionKind, StructDef, TypeCategory};
use super::{ada_ffi_type_name, ada_wrapper_type_name};

// ============================================================================
// Public entry points (called from mod.rs)
// ============================================================================

pub fn emit_wrapper_specs(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Idiomatic Controlled wrappers (Finalize calls _delete on scope exit).");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    let delete_set = collect_delete_targets(ir);

    for s in &ir.structs {
        if !should_wrap(s, ir, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_spec(builder, s);
    }
    Ok(())
}

pub fn emit_wrapper_bodies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, ir, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_body(builder, s);
    }
    Ok(())
}

// ============================================================================
// Discovery / filtering
// ============================================================================

fn should_wrap(s: &StructDef, _ir: &CodegenIR, config: &CodegenConfig) -> bool {
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

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

// ============================================================================
// Spec emission
// ============================================================================

fn emit_wrapper_spec(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ada_wrapper_type_name(&s.name);
    let ffi = ada_ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- {}", d.replace('\n', " ").trim()));
        }
    }

    builder.line(&format!(
        "type {} is new Ada.Finalization.Controlled with record",
        wrapper
    ));
    builder.line(&format!("   Inner : aliased {};", ffi));
    builder.line("   Owned : Boolean := True;");
    builder.line("end record;");
    builder.blank();

    builder.line(&format!(
        "overriding procedure Finalize (Self : in out {});",
        wrapper
    ));
    builder.line(&format!(
        "overriding procedure Adjust   (Self : in out {});",
        wrapper
    ));
    builder.blank();
}

// ============================================================================
// Body emission
// ============================================================================

fn emit_wrapper_body(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ada_wrapper_type_name(&s.name);
    let ffi = ada_ffi_type_name(&s.name);

    builder.line(&format!(
        "overriding procedure Finalize (Self : in out {}) is",
        wrapper
    ));
    builder.line("begin");
    builder.line("   if Self.Owned then");
    // The FFI delete subprogram takes the address of the inner record;
    // we use `Self.Inner'Address` to obtain a `System.Address`.
    builder.line(&format!(
        "      {}_Delete (Self.Inner'Address);",
        ffi
    ));
    builder.line("      Self.Owned := False;");
    builder.line("   end if;");
    builder.line("end Finalize;");
    builder.blank();

    builder.line(&format!(
        "overriding procedure Adjust   (Self : in out {}) is",
        wrapper
    ));
    builder.line("begin");
    builder.line("   -- After assignment, the copy is conservatively not the owner;");
    builder.line("   -- the user must call an explicit clone primitive to take ownership.");
    builder.line("   Self.Owned := False;");
    builder.line("end Adjust;");
    builder.blank();
}
