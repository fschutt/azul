//! Pascal `cdecl; external 'azul';` declarations for the C-ABI surface.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single Pascal `function` or `procedure` import. The C-ABI symbol name
//! is preserved verbatim (`AzApp_create`, `AzDom_addChild`, ...) so the
//! linker matches the same exported symbols as the C / C++ bindings.
//!
//! Idiomatic, namespaced wrappers (`TApp.Create(...)`) live in
//! `wrappers.rs` and call into these externals.
//!
//! Pascal calling-convention notes:
//!
//! - `cdecl` matches Rust's `extern "C"` ABI.
//! - `external AzulLib` causes the FPC linker to import the symbol from `azul.dll` / `libazul.so` /
//!   `libazul.dylib` at runtime.
//! - For arguments the IR marks as references / pointers, we emit a typed pointer (`PAzApp`) so
//!   callers get compile-time pointer-type checking (passing a `PAzWindow` where a `PAzApp` is
//!   expected is rejected).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory},
    },
    map_type_to_pascal, sanitize_identifier, unique_identifier,
    types::ptr_type_for_arg,
};

/// The Pascal identifier each declared export is imported under, keyed by
/// C symbol.
///
/// Pascal identifiers are CASE-INSENSITIVE, so two exports whose names
/// differ only in case — `AzImageRef_getRawImage` and
/// `AzImageRef_getRawimage` are both real libazul exports — cannot both be
/// declared under their own spelling. Dropping the second one dropped that
/// export from the binding entirely; instead the later symbol of a
/// case-insensitive group is declared as `<c_name>_<n>` and bound to the
/// real export through the `external ... name '<c_name>'` clause, which is
/// exactly what that clause is for. `n` counts up from 2 in `ir.functions`
/// order (api.json order), so every regeneration produces the same
/// spelling.
///
/// The value layer calls the identifiers in this map, never `c_name`
/// directly, so a renamed import stays reachable from the idiomatic side.
pub(super) fn external_idents(ir: &CodegenIR, config: &CodegenConfig) -> BTreeMap<String, String> {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        let ident = unique_identifier(&func.c_name, "_", &mut taken);
        out.insert(func.c_name.clone(), ident);
    }
    out
}

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ External cdecl declarations: every C-ABI function imported from      }");
    builder.line("{ libazul. Symbol names match the C bindings verbatim, except where     }");
    builder.line("{ two exports differ only in case (see external_idents).                }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    let idents = external_idents(ir, config);
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        let Some(ident) = idents.get(&func.c_name) else {
            continue;
        };
        emit_external(builder, func, ir, ident);
    }
    builder.blank();

    Ok(())
}

pub(super) fn should_emit_function(
    func: &FunctionDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> bool {
    // A trait entry point an api.json `derive` declares is not what the
    // `DestructorOrClone` exclusion below is for. That category is excluded
    // because those types' ordinary methods traffic in callback function
    // pointers this binding cannot marshal; `Az{T}_toDbgString(ptr) ->
    // AzString` traffics in neither, and is the same shape as the ~2800
    // `_toDbgString` declarations this binding already emits. Excluding it
    // wholesale is why every `*VecDestructor` declared `Debug` and named it
    // nowhere. (`*VecDestructor` is a tagged union, hence `find_enum`.)
    // RECURSIVE types are here for the same reason. `XmlNodeChild` and friends
    // are excluded below because their ORDINARY methods traffic in a shape
    // this binding cannot express by value - but `Az{T}_partialEq(a, b) ->
    // bool` and `Az{T}_toDbgString(ptr) -> AzString` take a pointer and return
    // a scalar, so the exclusion never applied to them. That is why the same
    // four types - `Xml`, `XmlNodeChild`, `XmlNodeChildVec`,
    // `ResultXmlXmlError` - showed up as the residue in fourteen bindings at
    // once: one cause, not fourteen.
    if func.kind.is_declared_capability()
        && (ir.find_enum(&func.class_name).is_some_and(|e| {
            matches!(
                e.category,
                TypeCategory::DestructorOrClone | TypeCategory::Recursive
            )
        }) || ir
            .find_struct(&func.class_name)
            .is_some_and(|s| s.category == TypeCategory::Recursive))
    {
        return config.should_include_type(&func.class_name);
    }

    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if matches!(
            s.category,
            // A borrowed slice (`VecRef`) is NOT excluded: the C struct is
            // emitted, so its trait functions belong in the FFI layer too.
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !s.generic_params.is_empty() {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&func.class_name) {
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !e.generic_params.is_empty() {
            return false;
        }
    }
    true
}

fn emit_external(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR, pascal_name: &str) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            // Pascal uses `{ ... }` for block comments. Embedded `{`
            // opens a nested level and trailing `}` closes the outer
            // comment, so swap BOTH braces for parens.
            let safe = d
                .replace('{', "(")
                .replace('}', ")")
                .replace(['\n', '\r'], " ");
            builder.line(&format!("{{ {} }}", safe));
        }
    }

    let args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let pas_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
                ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    ptr_type_for_arg(&a.type_name, ir)
                }
            };
            format!("{}: {}", sanitize_identifier(&a.name), pas_ty)
        })
        .collect();

    let args_str = if args.is_empty() {
        String::new()
    } else {
        format!("({})", args.join("; "))
    };

    // Functions with a callback-wrapper arg import the `<c_name>Struct`
    // C symbol (whole wrapper struct by value — matches the record
    // args declared here) under the ORIGINAL Pascal identifier, so
    // wrapper call sites don't change. The raw `<c_name>` takes a bare
    // fn ptr at the C ABI; importing it with record args crashed on
    // click.
    //
    // The `name '<symbol>'` clause is also what keeps a case-only
    // duplicate reachable: there the Pascal identifier carries an
    // ordinal suffix and the clause names the real export.
    let c_symbol = super::super::managed_host_invoker::managed_c_symbol(func);
    let external_clause = if c_symbol == pascal_name {
        "external AzulLib".to_string()
    } else {
        format!("external AzulLib name '{}'", c_symbol)
    };

    match &func.return_type {
        Some(ret) => {
            let pas_ret = map_type_to_pascal(ret, ir);
            builder.line(&format!(
                "function {}{}: {}; cdecl; {};",
                pascal_name, args_str, pas_ret, external_clause
            ));
        }
        None => {
            builder.line(&format!(
                "procedure {}{}; cdecl; {};",
                pascal_name, args_str, external_clause
            ));
        }
    }
}
