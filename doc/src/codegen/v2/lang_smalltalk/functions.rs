//! UnifiedFFI primitive emission for the Smalltalk generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single class-side method on `AzulNative`. The method body uses the
//! standard UnifiedFFI primitive pragma and forwards to `ffiCall:` with
//! the C signature spelled out in a Smalltalk array literal:
//!
//! ```text
//! AzulNative class >> azAppCreate: opts [
//!     <primitive: #primitiveNativeCall module: #UnifiedFFI>
//!     ^ self ffiCall: #(void* AzApp_create(AzAppCreateOptions* opts))
//!             module: 'azul'
//! ]
//! ```
//!
//! UFFI compiles the array literal at image-build time into a libffi
//! call that resolves the symbol against the `module:` library handle.
//!
//! Idiomatic, prefix-stripped wrappers (`AzulApp create: anOptions`)
//! live in `wrappers.rs` and call into these primitives.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
use super::types::class_header;
use super::{
    map_type_to_uffi, method_category_line, sanitize_identifier, snake_to_lower_camel,
    FFI_MODULE, NATIVE_CLASS, PACKAGE_NATIVE,
};

pub fn generate_native_methods(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("\"---------------------------------------------------------------------------");
    builder.line(" AzulNative — class-side host of every C-ABI primitive. Each method emits");
    builder.line(" a UnifiedFFI ffiCall:module: invocation that resolves at image start.");
    builder.line("---------------------------------------------------------------------------\"");
    builder.blank();

    // Emit the host class declaration once.
    class_header(builder, NATIVE_CLASS, "Object", &[], &[], PACKAGE_NATIVE, &[]);

    // moduleName helpers — UFFI calls these on demand to resolve the
    // shared library across platforms.
    emit_module_name_methods(builder);

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_ffi_method(builder, func, ir);
    }

    Ok(())
}

fn emit_module_name_methods(builder: &mut CodeBuilder) {
    method_category_line(builder, "ffi-library-name");
    builder.line(&format!("{} class >> moduleName [", NATIVE_CLASS));
    builder.indent();
    builder.line("\"UFFI default; OS-specific overrides below take precedence on those platforms.\"");
    builder.line(&format!("^ '{}'", FFI_MODULE));
    builder.dedent();
    builder.line("]");
    builder.blank();

    method_category_line(builder, "ffi-library-name");
    builder.line(&format!("{} class >> macModuleName [ ^ 'libazul.dylib' ]", NATIVE_CLASS));
    builder.blank();

    method_category_line(builder, "ffi-library-name");
    builder.line(&format!("{} class >> unixModuleName [ ^ 'libazul.so' ]", NATIVE_CLASS));
    builder.blank();

    method_category_line(builder, "ffi-library-name");
    builder.line(&format!("{} class >> win32ModuleName [ ^ 'azul.dll' ]", NATIVE_CLASS));
    builder.blank();
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

fn emit_ffi_method(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let return_type = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_uffi(r, ir))
        .unwrap_or_else(|| "void".to_string());

    // Build the C signature array `(retType c_name(argType arg, ...))`.
    let mut sig_args: Vec<String> = Vec::with_capacity(func.args.len());
    for a in &func.args {
        let st_type = match a.ref_kind {
            ArgRefKind::Owned => map_type_to_uffi(&a.type_name, ir),
            ArgRefKind::Ref
            | ArgRefKind::RefMut
            | ArgRefKind::Ptr
            | ArgRefKind::PtrMut => format!("{}*", map_type_to_uffi(&a.type_name, ir)),
        };
        sig_args.push(format!("{} {}", st_type, sanitize_identifier(&a.name)));
    }

    // Build the Smalltalk selector. Selectors with N>0 arguments use
    // keyword form: `firstKeyword: arg1 secondKeyword: arg2`.
    // We base the first keyword on the C symbol name (in lowerCamel),
    // and use the FFI argument names as subsequent keywords.
    let base_selector = snake_to_lower_camel(&func.c_name);
    let selector = build_keyword_selector(&base_selector, &func.args);

    // Doc comment (if any).
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("\"{}\"", d.replace('"', "''")));
        }
    }

    method_category_line(builder, "ffi");
    builder.line(&format!("{} class >> {} [", NATIVE_CLASS, selector));
    builder.indent();
    builder.line("<primitive: #primitiveNativeCall module: #UnifiedFFI>");
    builder.line(&format!(
        "^ self ffiCall: #({} {}({}))",
        return_type,
        func.c_name,
        sig_args.join(", ")
    ));
    builder.indent();
    builder.line(&format!("module: '{}'", FFI_MODULE));
    builder.dedent();
    builder.dedent();
    builder.line("]");
    builder.blank();
}

/// Build a Smalltalk keyword selector. With zero arguments the
/// selector is the bare base; with one or more arguments each gets
/// its own `name:` keyword. The first keyword is `<base>:`.
fn build_keyword_selector(
    base: &str,
    args: &[super::super::ir::FunctionArg],
) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    let mut out = String::new();
    for (i, a) in args.iter().enumerate() {
        let arg_id = sanitize_identifier(&a.name);
        if i == 0 {
            out.push_str(&format!("{}: {}", base, arg_id));
        } else {
            // Each subsequent keyword is the argument name followed by
            // the same name as the parameter — Smalltalk convention.
            out.push_str(&format!(" {}: {}", a.name, arg_id));
        }
    }
    out
}
