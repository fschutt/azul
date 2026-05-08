//! Convention helpers for manual cleanup of native resources.
//!
//! Algol 68 has **no destructors**. The Revised Report (1973) does not
//! describe RAII, smart pointers, or scope-bound finalisation; the
//! handful of language extensions that exist in a68g (HEAP allocation,
//! garbage collection of `LOC` / `HEAP` cells) cover only Algol 68's own
//! managed memory and do not run on resources reached through `ALIEN`.
//!
//! As a result, an Algol 68 user must release every owned native
//! resource by hand. We help them in three ways:
//!
//! 1. For every IR struct that has a matching `<TypeName>_delete` C
//!    function, we emit a paired Algol 68 PROC named `delete <type>`
//!    whose body is an `ALIEN` call to that same C symbol. The user
//!    writes `delete app(app)` instead of having to remember the C name.
//! 2. For each such type we emit a comment block reminding the user
//!    that they own the value and must explicitly `delete` it before
//!    its `REF` goes out of scope.
//! 3. We provide a `# Manual cleanup convention #` overview comment at
//!    the top of the wrappers section so users browsing the file
//!    understand the contract before they hit individual procedures.
//!
//! ## Why no idiomatic class wrappers?
//!
//! Object-oriented features (constructors / destructors / methods) do
//! not exist in Algol 68. A68G does not ship an OO extension. We
//! deliberately stop at the convention layer rather than synthesising a
//! pseudo-OO API on top of records — anyone using a68g for a serious
//! project will have grown a hand-rolled cleanup discipline already.

use std::collections::BTreeSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionKind, StructDef, TypeCategory};
use super::{algol_mode_name, camel_or_snake_to_spaced_lower_pub, LIB_NAME};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }

    emit_convention_header(builder);

    for s in &targets {
        emit_delete_helper(builder, s);
    }

    builder.blank();
    Ok(())
}

fn emit_convention_header(builder: &mut CodeBuilder) {
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.line("# Manual cleanup convention                                                     #");
    builder.line("#                                                                                #");
    builder.line("# Algol 68 has no destructors and a68g's reference-counted heap does NOT track  #");
    builder.line("# resources reached through ALIEN. Every value listed below is owned by the     #");
    builder.line("# caller; the caller MUST invoke the matching `delete <type>` PROC before the   #");
    builder.line("# REF leaves scope, otherwise the underlying C-side resource leaks.             #");
    builder.line("#                                                                                #");
    builder.line("# Idiomatic pattern:                                                             #");
    builder.line("#                                                                                #");
    builder.line("#     REF AZAPP app := az app create (data, config);                             #");
    builder.line("#     az app run (app, window);                                                  #");
    builder.line("#     delete az app (app)   # release native memory before scope exit #          #");
    builder.line("#                                                                                #");
    builder.line("# Wrapping a value in a HEAP cell with a hand-rolled cleanup ON SCOPE EXIT       #");
    builder.line("# pragma is also a viable pattern for users who want some automation.           #");
    builder.line("# ---------------------------------------------------------------------------- #");
    builder.blank();
}

fn emit_delete_helper(builder: &mut CodeBuilder, s: &StructDef) {
    let type_lower = camel_or_snake_to_spaced_lower_pub(&s.name);
    let mode = algol_mode_name(&s.name);
    let c_symbol = format!("Az{}_delete", s.name);

    builder.line(&format!(
        "# Release native memory owned by an {}. Caller must NOT use the value afterwards. #",
        mode
    ));
    builder.line(&format!(
        "PROC delete az {} = (REF {} value) VOID: ALIEN \"{}\" ! \"{}\";",
        type_lower, mode, c_symbol, LIB_NAME
    ));
}

// ============================================================================
// Discovery (mirrors lang_pascal/wrappers.rs:collect_wrapper_targets)
// ============================================================================

fn collect_wrapper_targets<'a>(
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Vec<&'a StructDef> {
    let delete_set: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    ir.structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config) && delete_set.contains(s.name.as_str()))
        .collect()
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
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
