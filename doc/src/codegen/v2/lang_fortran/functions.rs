//! Fortran `interface ... end interface` block declarations for the
//! C-ABI surface.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! Fortran `function` (returns a value) or `subroutine` (returns void)
//! declaration carrying a `bind(C, name="...")` attribute that pins the
//! C-ABI symbol verbatim. Fortran is case-INSENSITIVE for its own
//! identifiers but the `name="AzApp_create"` argument is case-SENSITIVE,
//! so the linker matches the same exported symbols as the C / C++ /
//! Pascal bindings.
//!
//! Calling-convention notes:
//!
//! - `bind(C)` is the Fortran 2003 way to spell extern "C" linkage.
//! - C primitives, pointers (`type(c_ptr)`), and `bind(C)` derived types
//!   are passed by VALUE; we add `, value :: arg` to every dummy
//!   argument because Fortran defaults to pass-by-reference.
//! - The `import` statement at the top of each interface body brings
//!   the host module's derived types (`AzAppConfig`, etc.) into scope
//!   so the body can reference them.
//! - We name the Fortran-side procedure with a snake_case alias
//!   (e.g. `az_app_create`) and bind it to the verbatim C symbol via
//!   `name="AzApp_create"`. This avoids any case-folding ambiguity in
//!   user code while keeping the linker symbol exact.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{
    map_type_to_fortran, pascal_to_snake_case, sanitize_identifier, truncate_identifier,
};

pub fn generate_externals(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! C-ABI interface block: every exported `azul` C symbol is declared");
    builder.line("! here. Symbol names match the C bindings verbatim via bind(C, name=).");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    builder.line("interface");
    builder.indent();
    builder.blank();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_external(builder, func, ir);
    }

    builder.dedent();
    builder.line("end interface");
    builder.blank();

    Ok(())
}

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
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

/// Lower a C-ABI symbol to a Fortran-friendly snake_case alias.
///
/// `AzApp_create` -> `az_app_create`. We use the lowered alias as the
/// Fortran procedure name so user code can call it without worrying
/// about case-folding; the original symbol survives in `bind(C, name=)`
/// for the linker.
pub(crate) fn fortran_alias_for(c_symbol: &str) -> String {
    let snake = pascal_to_snake_case(c_symbol);
    truncate_identifier(&snake)
}

fn emit_external(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            let safe = d.replace('\r', " ").replace('\n', " ");
            builder.line(&format!("! {}", safe));
        }
    }

    let alias = fortran_alias_for(&func.c_name);
    let arg_names: Vec<String> = func
        .args
        .iter()
        .map(|a| sanitize_identifier(&a.name))
        .collect();
    let arg_list = arg_names.join(", ");

    let is_function = func.return_type.is_some();

    if is_function {
        builder.line(&format!(
            "function {}({}) bind(C, name=\"{}\") result(r)",
            alias, arg_list, func.c_name
        ));
    } else {
        builder.line(&format!(
            "subroutine {}({}) bind(C, name=\"{}\")",
            alias, arg_list, func.c_name
        ));
    }
    builder.indent();
    // `import` brings module-scoped derived types into the interface
    // body so the dummy-argument declarations can refer to them.
    builder.line("import");

    for arg in &func.args {
        let f_ty = match arg.ref_kind {
            ArgRefKind::Owned => map_type_to_fortran(&arg.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "type(c_ptr)".to_string()
            }
        };
        let nm = sanitize_identifier(&arg.name);
        builder.line(&format!("{}, value :: {}", f_ty, nm));
    }

    if let Some(ret) = &func.return_type {
        let ret_ty = map_type_to_fortran(ret, ir);
        builder.line(&format!("{} :: r", ret_ty));
    }

    builder.dedent();
    if is_function {
        builder.line(&format!("end function {}", alias));
    } else {
        builder.line(&format!("end subroutine {}", alias));
    }
    builder.blank();
}
