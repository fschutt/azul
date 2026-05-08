//! Idiomatic FreeBASIC wrapper Types with `Constructor` / `Destructor`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit an idiomatic `Type` inside `Namespace Azul ...
//! End Namespace`. The wrapper:
//!
//! 1. Holds the underlying FFI record (`AzTypeName`) by value in a
//!    private `raw` field, plus an `owned` flag so `Wrap`-style
//!    factories can opt out of automatic deletion.
//! 2. Exposes a `Constructor (...)` overload per IR
//!    `FunctionKind::Constructor` / `FunctionKind::Default` method on
//!    the type. FreeBASIC supports overloaded constructors out of the
//!    box so we don't need to suffix names like in Pascal.
//! 3. Exposes a `Destructor ()` that calls `<TypeName>_delete(@raw)`
//!    when `owned` is true. The destructor fires automatically when
//!    a stack-allocated wrapper goes out of scope (or `Delete` is
//!    called on a heap-allocated one).
//! 4. Surfaces every non-trait method as an idiomatic instance method
//!    delegating to the matching FFI symbol.
//!
//! User-facing names drop the `Az` prefix:  `AzApp`  →  `Azul.App`,
//! `AzDom` → `Azul.Dom`, etc. The names are emitted as nested types
//! inside `Namespace Azul`. Plain POD structs without a `_delete` get
//! no wrapper.

use std::collections::BTreeSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::types::ptr_type_for_arg;
use super::{
    ffi_type_name, map_type_to_fb, sanitize_comment, sanitize_identifier, to_pascal_case,
};

// ============================================================================
// Public entry point
// ============================================================================

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }

    builder.line("' --------------------------------------------------------------------");
    builder.line("' Idiomatic wrappers — Constructor/Destructor handle native ownership.");
    builder.line("' Use:   Dim app As Azul.App = Azul.App(data, cfg)   ' auto-cleanup");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

    builder.line("Namespace Azul");
    builder.indent();

    for s in &targets {
        emit_wrapper_decl(builder, s, ir);
    }

    builder.dedent();
    builder.line("End Namespace");
    builder.blank();

    // Method bodies live OUTSIDE the Namespace block but reference
    // `Azul.<Name>` qualified names. FreeBASIC accepts both forms;
    // we prefer outside-of-namespace bodies so multi-line definitions
    // are easier to read.
    for s in &targets {
        emit_wrapper_impl(builder, s, ir);
    }

    Ok(())
}

// ============================================================================
// Discovery
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

// ============================================================================
// Type declaration (interface side)
// ============================================================================

fn emit_wrapper_decl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = wrapper_type_name(&s.name);
    let raw_record = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    builder.line(&format!("Type {}", class_name));
    builder.indent();

    builder.line("Private:");
    builder.indent();
    builder.line(&format!("raw As {}", raw_record));
    builder.line("owned As Boolean");
    builder.dedent();

    builder.line("Public:");
    builder.indent();

    // Wrap-existing constructor. Takes a raw FFI record and assumes
    // ownership. Useful when an FFI function returns `AzFoo` by value
    // and the caller wants to wrap it.
    builder.line(&format!("Declare Constructor (ByVal raw_in As {})", raw_record));

    // One Constructor per IR Constructor / Default function.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_decl(builder, func, ir);
    }

    builder.line("Declare Destructor ()");

    // Read-only access to the raw FFI record (escape hatch).
    builder.line(&format!("Declare Property GetRaw () As {}", raw_record));

    // Instance / static methods.
    for func in ir.functions_for_class(&s.name) {
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
        ) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        emit_method_decl(builder, func, ir);
    }

    builder.dedent();
    builder.dedent();
    builder.line("End Type");
    builder.blank();
}

fn emit_constructor_decl(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    if args_str.is_empty() {
        builder.line("Declare Constructor ()");
    } else {
        builder.line(&format!("Declare Constructor ({})", args_str));
    }
}

fn emit_method_decl(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);

    let prefix = if is_static { "Declare Static " } else { "Declare " };

    if let Some(ret) = &func.return_type {
        let fb_ret = map_type_to_fb(ret, ir);
        if args_str.is_empty() {
            builder.line(&format!(
                "{}Function {} () As {}",
                prefix, method_name, fb_ret
            ));
        } else {
            builder.line(&format!(
                "{}Function {} ({}) As {}",
                prefix, method_name, args_str, fb_ret
            ));
        }
    } else if args_str.is_empty() {
        builder.line(&format!("{}Sub {} ()", prefix, method_name));
    } else {
        builder.line(&format!("{}Sub {} ({})", prefix, method_name, args_str));
    }
}

// ============================================================================
// Type implementation (method-body side)
// ============================================================================

fn emit_wrapper_impl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = format!("Azul.{}", wrapper_type_name(&s.name));
    let ffi = ffi_type_name(&s.name);
    let raw_record = ffi.clone();

    // Wrap-from-raw constructor.
    builder.line(&format!(
        "Constructor {} (ByVal raw_in As {})",
        class_name, raw_record
    ));
    builder.indent();
    builder.line("this.raw = raw_in");
    builder.line("this.owned = True");
    builder.dedent();
    builder.line("End Constructor");
    builder.blank();

    // Constructors from IR.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_impl(builder, &class_name, &ffi, func, ir);
    }

    // Destructor.
    builder.line(&format!("Destructor {} ()", class_name));
    builder.indent();
    builder.line("If this.owned Then");
    builder.indent();
    builder.line(&format!("{}_delete(@this.raw)", ffi));
    builder.dedent();
    builder.line("End If");
    builder.dedent();
    builder.line("End Destructor");
    builder.blank();

    // Raw accessor.
    builder.line(&format!(
        "Property {}.GetRaw () As {}",
        class_name, raw_record
    ));
    builder.indent();
    builder.line("Return this.raw");
    builder.dedent();
    builder.line("End Property");
    builder.blank();

    // Instance / static methods.
    for func in ir.functions_for_class(&s.name) {
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
        ) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        emit_method_impl(builder, &class_name, &ffi, func, ir);
    }
}

fn emit_constructor_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);

    let signature = if args_str.is_empty() {
        format!("Constructor {} ()", class_name)
    } else {
        format!("Constructor {} ({})", class_name, args_str)
    };
    builder.line(&signature);
    builder.indent();

    let call_args: Vec<String> = visible
        .iter()
        .map(|a| sanitize_identifier(&a.name))
        .collect();

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let call = format!("{}_{}({})", ffi, func.method_name, call_args.join(", "));

    if returns_self {
        builder.line(&format!("this.raw = {}", call));
    } else {
        builder.line(&format!(
            "' SKIPPED: constructor returns {:?}, not {}",
            func.return_type, func.class_name
        ));
        builder.line(&call);
    }
    builder.line("this.owned = True");
    builder.dedent();
    builder.line("End Constructor");
    builder.blank();
}

fn emit_method_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    let signature = if let Some(ret) = &func.return_type {
        let fb_ret = map_type_to_fb(ret, ir);
        if args_str.is_empty() {
            format!(
                "{}Function {}.{} () As {}",
                if is_static { "Static " } else { "" },
                class_name,
                method_name,
                fb_ret
            )
        } else {
            format!(
                "{}Function {}.{} ({}) As {}",
                if is_static { "Static " } else { "" },
                class_name,
                method_name,
                args_str,
                fb_ret
            )
        }
    } else if args_str.is_empty() {
        format!(
            "{}Sub {}.{} ()",
            if is_static { "Static " } else { "" },
            class_name,
            method_name
        )
    } else {
        format!(
            "{}Sub {}.{} ({})",
            if is_static { "Static " } else { "" },
            class_name,
            method_name,
            args_str
        )
    };

    builder.line(&signature);
    builder.indent();

    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        // Pass `@this.raw` for `&self` / `&mut self`. Most azul C-ABI
        // methods take a pointer to the raw record.
        call_args.push("@this.raw".to_string());
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }

    let call = format!("{}_{}({})", ffi, func.method_name, call_args.join(", "));

    if func.return_type.is_some() {
        builder.line(&format!("Return {}", call));
    } else {
        builder.line(&call);
    }
    builder.dedent();
    if func.return_type.is_some() {
        builder.line("End Function");
    } else {
        builder.line("End Sub");
    }
    builder.blank();
}

// ============================================================================
// Argument helpers
// ============================================================================

/// Filter the implicit `self` argument out of a function's arg list.
fn visible_user_args(func: &FunctionDef) -> Vec<&FunctionArg> {
    let class_lower = func.class_name.to_lowercase();
    func.args
        .iter()
        .filter(|a| a.name != "self" && a.name != class_lower)
        .collect()
}

fn format_arg_list(args: &[&FunctionArg], ir: &CodegenIR) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let fb_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_fb(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type_for_arg(&a.type_name, ir),
            };
            format!("ByVal {} As {}", sanitize_identifier(&a.name), fb_ty)
        })
        .collect();
    parts.join(", ")
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Idiomatic wrapper type name — drop the `Az` prefix, keep the rest.
/// e.g. `App` -> `App`, `Dom` -> `Dom`. (The IR already strips the `Az`
/// prefix from struct names; this is here as a sentinel in case a
/// future change re-introduces it.)
fn wrapper_type_name(raw: &str) -> String {
    raw.strip_prefix("Az").unwrap_or(raw).to_string()
}

/// Convert an api.json method name (snake_case / camelCase / "new") to
/// an idiomatic FreeBASIC method name (PascalCase). `new` is reserved
/// by FreeBASIC and is mapped to `Create`.
fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "Create".to_string();
    }
    if method_name.contains('_') {
        return to_pascal_case(method_name);
    }
    let mut chars = method_name.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
