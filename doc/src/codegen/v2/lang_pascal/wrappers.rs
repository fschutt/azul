//! Idiomatic Pascal class wrappers with `destructor Destroy; override;`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit a `T<TypeName> = class` (descended from `TObject`) that:
//!
//! 1. Holds the underlying FFI record (`TAzTypeName`) by value in a
//!    private `FRaw` field.
//! 2. Provides a `constructor Create(...)` per IR `FunctionKind::Constructor`
//!    method on the type. When multiple constructors exist they are
//!    overloaded via `overload;` and named after their original method name
//!    (`CreateFoo`).
//! 3. Provides a `destructor Destroy; override;` that calls the
//!    `<TypeName>_delete` external. Standard Pascal `obj.Free;` invokes
//!    this destructor automatically.
//! 4. Surfaces every non-trait method on `TypeName` as an idiomatic
//!    instance / class method delegating to the underlying FFI symbol.
//!
//! Plain POD structs without a `_delete` get *no* wrapper — users
//! manipulate them through the `TAzFoo` record directly. Tagged-union
//! enums similarly aren't wrapped (Pascal already provides ergonomic
//! variant-record syntax).

use std::collections::BTreeSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::types::ptr_type_for_arg;
use super::{
    ffi_type_name, map_type_to_pascal, record_type_name, sanitize_identifier, to_pascal_case,
};

// ============================================================================
// Public entry points
// ============================================================================

pub fn generate_wrapper_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }

    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Idiomatic class wrappers (call .Free to release native resources).   }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();
    builder.line("type");
    builder.indent();

    for s in &targets {
        emit_wrapper_class_decl(builder, s, ir);
    }

    builder.dedent();
    builder.blank();
    Ok(())
}

pub fn generate_wrapper_implementation(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    for s in &targets {
        emit_wrapper_class_impl(builder, s, ir);
    }
    Ok(())
}

// ============================================================================
// Discovery
// ============================================================================

/// All structs that own native memory (`<Name>_delete` exists) and pass
/// the inclusion filter.
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
// Class declaration (interface side)
// ============================================================================

fn emit_wrapper_class_decl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = pascal_class_name(&s.name);
    let raw_record = record_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    builder.line(&format!("{} = class(TObject)", class_name));
    builder.line("private");
    builder.indent();
    builder.line(&format!("FRaw: {};", raw_record));
    builder.line("FOwned: Boolean;");
    builder.dedent();
    builder.line("public");
    builder.indent();

    // Wrap-existing constructor (used by static helpers that return the type).
    builder.line(&format!(
        "constructor Wrap(const ARaw: {}); overload;",
        raw_record
    ));

    // One constructor per IR Constructor function on this class.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_decl(builder, func, ir);
    }

    // Destructor.
    builder.line("destructor Destroy; override;");

    // Read-only access to the raw record (escape hatch for advanced users).
    builder.line(&format!("property Raw: {} read FRaw;", raw_record));

    // Instance & static methods (one declaration per surviving function).
    // Dedup by Pascal-cased name — multiple api.json methods can lower
    // to the same Pascal identifier (e.g. `get_raw_image` and
    // `get_rawimage` both PascalCase to `GetRawImage`). Skipping the
    // second avoids "overloaded functions have the same parameter list".
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
        let name = idiomatic_method_name(&func.method_name);
        if !emitted.insert(name.to_ascii_lowercase()) {
            builder.line(&format!(
                "{{ SKIPPED duplicate method: {} (collides with prior PascalCased name) }}",
                func.method_name
            ));
            continue;
        }
        emit_method_decl(builder, func, ir);
    }

    builder.dedent();
    builder.line("end;");
    builder.blank();
}

fn emit_constructor_decl(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let suffix = constructor_suffix(&func.method_name);
    if args_str.is_empty() {
        builder.line(&format!("constructor Create{}; overload;", suffix));
    } else {
        builder.line(&format!(
            "constructor Create{}({}); overload;",
            suffix, args_str
        ));
    }
}

/// Multiple constructors in Pascal must use `overload;` — and they must all
/// be named `Create*`. We name the canonical IR `new` / `create` simply
/// `Create`; any other constructor name appears as a PascalCased suffix
/// (`Create<MethodName>`).
fn constructor_suffix(method_name: &str) -> String {
    if method_name == "new" || method_name == "create" {
        return String::new();
    }
    to_pascal_case(method_name)
}

fn emit_method_decl(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);

    let prefix_kw = if is_static { "class " } else { "" };
    if let Some(ret) = &func.return_type {
        let pas_ret = map_type_to_pascal(ret, ir);
        if args_str.is_empty() {
            builder.line(&format!(
                "{}function {}: {};",
                prefix_kw, method_name, pas_ret
            ));
        } else {
            builder.line(&format!(
                "{}function {}({}): {};",
                prefix_kw, method_name, args_str, pas_ret
            ));
        }
    } else {
        if args_str.is_empty() {
            builder.line(&format!("{}procedure {};", prefix_kw, method_name));
        } else {
            builder.line(&format!(
                "{}procedure {}({});",
                prefix_kw, method_name, args_str
            ));
        }
    }
}

// ============================================================================
// Class implementation (implementation side)
// ============================================================================

fn emit_wrapper_class_impl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = pascal_class_name(&s.name);
    let ffi = ffi_type_name(&s.name);

    // Wrap(ARaw) constructor: take ownership of an already-built FFI record.
    builder.line(&format!(
        "constructor {}.Wrap(const ARaw: {});",
        class_name,
        record_type_name(&s.name)
    ));
    builder.line("begin");
    builder.indent();
    builder.line("inherited Create;");
    builder.line("FRaw := ARaw;");
    builder.line("FOwned := True;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // One Pascal constructor per IR Constructor / Default function.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_impl(builder, &class_name, &ffi, func, ir);
    }

    // Destructor: call `<Type>_delete(@FRaw)` if we own the underlying memory.
    builder.line(&format!("destructor {}.Destroy;", class_name));
    builder.line("begin");
    builder.indent();
    builder.line("if FOwned then");
    builder.line(&format!("  {}_delete(@FRaw);", ffi));
    builder.line("inherited Destroy;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Instance + static method bodies. Same dedup-by-Pascal-name as in
    // emit_wrapper_class_decl above — otherwise we'd emit two function
    // bodies for the same forward declaration.
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
        let name = idiomatic_method_name(&func.method_name);
        if !emitted.insert(name.to_ascii_lowercase()) {
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
    let suffix = constructor_suffix(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let signature = if args_str.is_empty() {
        format!("constructor {}.Create{};", class_name, suffix)
    } else {
        format!(
            "constructor {}.Create{}({});",
            class_name, suffix, args_str
        )
    };

    builder.line(&signature);
    builder.line("begin");
    builder.indent();
    builder.line("inherited Create;");

    let call_args: Vec<String> = visible.iter().map(|a| sanitize_identifier(&a.name)).collect();

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // Always invoke the C symbol verbatim (`func.c_name`) rather than
    // reconstructing it from `{ffi}_{method_name}` — the latter mixes
    // PascalCase class + snake_case method, but the externals are
    // declared with the camelCase form (`AzSvg_fromString`, not
    // `AzSvg_from_string`).
    let call = format!("{}({})", func.c_name, call_args.join(", "));
    if returns_self {
        builder.line(&format!("FRaw := {};", call));
    } else {
        // A constructor whose IR return type isn't Self doesn't really
        // map to a Pascal constructor — we still emit a stub to let the
        // user wire it up, but mark it explicitly.
        builder.line(&format!(
            "{{ SKIPPED: constructor returns {:?}, not {} }}",
            func.return_type, func.class_name
        ));
        builder.line(&format!("{};", call));
    }
    builder.line("FOwned := True;");
    builder.dedent();
    builder.line("end;");
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

    let prefix_kw = if is_static { "class " } else { "" };

    let signature = if let Some(ret) = &func.return_type {
        let pas_ret = map_type_to_pascal(ret, ir);
        if args_str.is_empty() {
            format!(
                "{}function {}.{}: {};",
                prefix_kw, class_name, method_name, pas_ret
            )
        } else {
            format!(
                "{}function {}.{}({}): {};",
                prefix_kw, class_name, method_name, args_str, pas_ret
            )
        }
    } else {
        if args_str.is_empty() {
            format!("{}procedure {}.{};", prefix_kw, class_name, method_name)
        } else {
            format!(
                "{}procedure {}.{}({});",
                prefix_kw, class_name, method_name, args_str
            )
        }
    };

    builder.line(&signature);
    builder.line("begin");
    builder.indent();

    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        // Inspect args[0] of the IR signature: Owned means the C
        // function takes the record by value (`AzFoo`), Ref/Ptr/etc.
        // means it takes a pointer (`AzFoo*`). The C external
        // declaration mirrors this, so we must match — passing `@FRaw`
        // where a value is expected raises "Incompatible type for
        // arg no. 1: Got Pointer, expected TAzFoo".
        let self_by_value = func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
        if self_by_value {
            call_args.push("FRaw".to_string());
        } else {
            call_args.push("@FRaw".to_string());
        }
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }

    // See emit_constructor_impl for why we use `func.c_name` instead
    // of `{ffi}_{method_name}`.
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    if let Some(ret) = &func.return_type {
        let returns_self = ret.trim() == func.class_name;
        if returns_self {
            // Wrap the raw value in a fresh class instance.
            builder.line(&format!("Result := {};", call));
            // Note: in real use the caller may want a TFoo.Wrap(Result)
            // instead. We return the raw record verbatim; users can
            // construct the wrapper class manually:
            //   wrapped := TFoo.Wrap(SomeOther.Method);
        } else {
            builder.line(&format!("Result := {};", call));
        }
    } else {
        builder.line(&format!("{};", call));
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

// ============================================================================
// Argument helpers
// ============================================================================

/// Filter the implicit `self` argument out of a function's arg list.
/// For instance / mutating / deep-copy methods args[0] IS the self,
/// regardless of how api.json named it (`instance`, snake-cased class,
/// `mime_type_data_vec`, etc.) — matches the C#/Java/Kotlin/Fortran fix.
fn visible_user_args(func: &FunctionDef) -> Vec<&super::super::ir::FunctionArg> {
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        func.args.iter().collect()
    }
}

fn format_arg_list(args: &[&super::super::ir::FunctionArg], ir: &CodegenIR) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let pas_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
                ArgRefKind::Ref
                | ArgRefKind::RefMut
                | ArgRefKind::Ptr
                | ArgRefKind::PtrMut => ptr_type_for_arg(&a.type_name, ir),
            };
            format!("{}: {}", sanitize_identifier(&a.name), pas_ty)
        })
        .collect();
    parts.join("; ")
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Pascal class name for an IR type — drop the `Az` prefix and prepend `T`.
/// e.g. `App` -> `TApp`, `Dom` -> `TDom`. The `Az` prefix is preserved on
/// the underlying record (`TAzApp`) and on the external symbol; only the
/// idiomatic class is unprefixed for nicer user-facing names.
fn pascal_class_name(raw: &str) -> String {
    format!("T{}", raw)
}

/// Convert an api.json method name to an idiomatic Pascal method name.
///
/// - `new` -> `Create` (constructor — though for non-constructor uses
///   we still want `Create`).
/// - snake_case / camelCase -> PascalCase.
fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "Create".to_string();
    }
    let pascal = if method_name.contains('_') {
        to_pascal_case(method_name)
    } else {
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // Rename methods that shadow TObject's inherited members. FPC
    // warns on every shadowed name and our build treats warnings as
    // errors — append "_X" (a valid Pascal identifier) so the wrapper
    // method is uniquely named while still recognisable.
    match pascal.as_str() {
        "ToString" | "Equals" | "GetHashCode" | "Free" | "Destroy"
        | "ClassName" | "ClassType" | "Dispatch" => format!("{}_X", pascal),
        _ => pascal,
    }
}

fn sanitize_comment(s: &str) -> String {
    // Strip both `{` and `}` so doc strings cannot accidentally open or
    // close Pascal block comments (matches lang_pascal/types.rs).
    s.replace('{', "(")
        .replace('}', ")")
        .replace('\n', " ")
        .replace('\r', " ")
}
