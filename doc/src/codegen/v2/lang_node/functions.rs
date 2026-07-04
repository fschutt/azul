//! Function-binding emission.
//!
//! For each IR function we emit one `azulFFI.func({...})` call. The
//! adapter in `mod.rs::emit_load_lib()` normalises the argument shape
//! across runtimes:
//!
//! ```js
//! const az_app_create = azulFFI.func({
//!     name: 'AzApp_create',
//!     parameters: ['AzAppConfig', 'void *'],   // koffi uses string types,
//!     returns: 'AzApp',                         // Bun/Deno use FFIType ints
//! });
//! ```
//!
//! On Node/koffi the adapter rewrites this internally to a koffi-style
//! C-decl string (see `lang_node/mod.rs::loadNodeKoffi.func`). On Bun
//! and Deno the parameters/returns spec is fed directly into the
//! symbol map.
//!
//! All bound symbols are collected into a flat `lib` object so wrappers
//! can dispatch through `lib.AzApp_create(...)`.

use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::{map_type_to_koffi, sanitize_js_identifier};

pub fn generate_function_bindings(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Function bindings: one entry per exported C-ABI symbol. Symbols are");
    b.line("// stored on the `lib` object indexed by their C name (`lib.AzApp_create`).");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line("const lib = Object.create(null);");
    b.blank();

    for f in &ir.functions {
        if !should_emit_function(f, ir) {
            continue;
        }
        emit_function_binding(b, f, ir);
    }
    b.blank();
}

// ============================================================================
// Inclusion filter
// ============================================================================

fn should_emit_function(f: &FunctionDef, ir: &CodegenIR) -> bool {
    if let Some(s) = ir.find_struct(&f.class_name) {
        if !s.generic_params.is_empty() {
            return false;
        }
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&f.class_name) {
        if !e.generic_params.is_empty() {
            return false;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
    }
    true
}

// ============================================================================
// Emission
// ============================================================================

fn emit_function_binding(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    let return_spec = f
        .return_type
        .as_ref()
        .map(|r| map_type_to_koffi(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let param_specs: Vec<String> = f
        .args
        .iter()
        .map(|a| arg_spec(a, ir))
        .collect();

    if !f.doc.is_empty() {
        b.line(&format!("// {}", f.doc.join(" ")));
    }

    // Emit a koffi-style decl AND a parameters/returns triple. The
    // adapter in mod.rs picks whichever shape its runtime needs.
    //
    // Functions with a callback-wrapper arg bind the `<c_name>Struct`
    // C symbol (whole wrapper struct by value — matches the struct
    // parameter spec built here). The raw `<c_name>` takes a bare fn
    // ptr at the C ABI; binding it with these params crashed on click.
    // The JS-side property name stays `lib.<c_name>` so wrappers don't
    // change.
    let c_symbol = super::super::managed_host_invoker::managed_c_symbol(f);
    let decl_string = format!(
        "{} {}({})",
        return_spec,
        c_symbol,
        param_specs.join(", ")
    );

    b.line(&format!(
        "lib.{} = azulFFI.func({{ name: '{}', decl: '{}', parameters: [{}], returns: '{}' }});",
        sanitize_js_identifier(&f.c_name),
        c_symbol,
        decl_string.replace('\'', "\\'"),
        param_specs
            .iter()
            .map(|p| format!("'{}'", p))
            .collect::<Vec<_>>()
            .join(", "),
        return_spec
    ));
}

fn arg_spec(arg: &super::super::ir::FunctionArg, ir: &CodegenIR) -> String {
    let base = map_type_to_koffi(&arg.type_name, ir);
    match arg.ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            format!("{} *", base)
        }
    }
}
