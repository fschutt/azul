//! Idiomatic Pascal class wrappers with `destructor Destroy; override;`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit a `T<TypeName> = class` (descended from `TObject`) that:
//!
//! 1. Holds the underlying FFI record (`TAzTypeName`) by value in a
//!    private `FRaw` field.
//! 2. Provides a `constructor Create(...)` per IR `FunctionKind::Constructor`
//!    method on the type. When multiple constructors exist they are
//!    overloaded via `overload;`. api.json constructor names that already
//!    carry a `create_` / `new_` verb drop it before the Pascal `Create`
//!    prefix is applied so `create_body` surfaces as `CreateBody`, not
//!    `CreateCreateBody` (Delphi named-constructor idiom). A per-class
//!    collision guard falls back to the unstripped spelling when stripping
//!    would collide with a sibling constructor (Pascal identifiers are
//!    case-insensitive), e.g. `ImageRef.new_rawimage` stays
//!    `CreateNewRawimage` because `CreateRawImage` already exists.
//! 3. Provides a `destructor Destroy; override;` that calls the
//!    `<TypeName>_delete` external. Standard Pascal `obj.Free;` invokes
//!    this destructor automatically.
//! 4. Surfaces every non-trait method on `TypeName` as an idiomatic
//!    instance / class method delegating to the underlying FFI symbol.
//!    Wherever the IR return type has a wrapper class of its own, the
//!    method returns that wrapper (`function WithChild(...): TDom`), built
//!    via `T<Ret>.Wrap(...)`. Wherever a BY-VALUE (owned) argument's type
//!    has a wrapper class, an additional `overload` variant accepting the
//!    wrapper is emitted next to the raw-record variant; the wrapper
//!    overload passes `arg.FRaw` and flips `arg.FOwned := False` because
//!    libazul consumed the bytes (prevents a double-free in the arg's
//!    destructor). Pointer-args keep their raw `PAz*` spelling in both
//!    variants (they may be buffer/base pointers, e.g. `CopyFromPtr`).
//! 5. Provides `function Release: TAz<TypeName>;` — detaches and returns
//!    the raw record, transferring ownership to the caller (the destructor
//!    will no longer call `_delete`). This is the bridge back into raw
//!    FFI surfaces such as `PAzDom(out_ptr)^ := body.Release;`.
//!
//! All wrapper classes are forward-declared (`TDom = class;`) at the top
//! of the wrapper `type` section so methods may accept/return sibling
//! wrapper classes regardless of declaration order.
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
    let target_names: BTreeSet<String> = targets.iter().map(|s| s.name.clone()).collect();

    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Idiomatic class wrappers (call .Free to release native resources).   }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();
    builder.line("type");
    builder.indent();

    // Forward declarations so wrapper methods can accept/return sibling
    // wrapper classes independent of declaration order.
    builder.line("{ Forward declarations so wrapper methods can reference sibling classes. }");
    for s in &targets {
        builder.line(&format!("{} = class;", pascal_class_name(&s.name)));
    }
    builder.blank();

    for s in &targets {
        emit_wrapper_class_decl(builder, s, ir, &target_names);
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
    let target_names: BTreeSet<String> = targets.iter().map(|s| s.name.clone()).collect();
    for s in &targets {
        emit_wrapper_class_impl(builder, s, ir, &target_names);
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

fn emit_wrapper_class_decl(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
) {
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

    // One constructor per IR Constructor function on this class. Names are
    // precomputed per class so the decl and impl passes agree and the
    // create_/new_ stutter-strip collision guard sees all siblings.
    let ctor_names = constructor_pascal_names(ir, &s.name);
    let mut ctor_idx = 0usize;
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        let ctor_name = &ctor_names[ctor_idx];
        ctor_idx += 1;
        emit_constructor_decl(builder, ctor_name, func, ir, targets, false);
        if has_owned_wrapper_arg(func, targets) {
            emit_constructor_decl(builder, ctor_name, func, ir, targets, true);
        }
    }

    // Destructor.
    builder.line("destructor Destroy; override;");

    // Read-only access to the raw record (escape hatch for advanced users).
    builder.line(&format!("property Raw: {} read FRaw;", raw_record));

    // Detach-and-transfer: returns the raw record and relinquishes
    // ownership (the destructor will no longer call `_delete`).
    builder.line(&format!("function Release: {};", raw_record));

    // Instance & static methods (one declaration per surviving function).
    // Dedup by Pascal-cased name — multiple api.json methods can lower
    // to the same Pascal identifier (e.g. `get_raw_image` and
    // `get_rawimage` both PascalCase to `GetRawImage`). Skipping the
    // second avoids "overloaded functions have the same parameter list".
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // Reserve the names the wrapper itself defines so an api.json method
    // of the same (case-insensitive) name can't collide with them.
    emitted.insert("wrap".to_string());
    emitted.insert("raw".to_string());
    emitted.insert("release".to_string());
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
        let twin = has_owned_wrapper_arg(func, targets);
        emit_method_decl(builder, func, ir, targets, false, twin);
        if twin {
            emit_method_decl(builder, func, ir, targets, true, twin);
        }
    }

    builder.dedent();
    builder.line("end;");
    builder.blank();
}

fn emit_constructor_decl(
    builder: &mut CodeBuilder,
    ctor_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    wrapper_variant: bool,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir, targets, wrapper_variant);
    if args_str.is_empty() {
        builder.line(&format!("constructor {}; overload;", ctor_name));
    } else {
        builder.line(&format!(
            "constructor {}({}); overload;",
            ctor_name, args_str
        ));
    }
}

fn emit_method_decl(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    wrapper_variant: bool,
    overloaded: bool,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir, targets, wrapper_variant);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);

    let prefix_kw = if is_static { "class " } else { "" };
    let tail = if overloaded { " overload;" } else { "" };
    if let Some(ret) = &func.return_type {
        let pas_ret = return_type_to_pascal(ret, ir, targets);
        if args_str.is_empty() {
            builder.line(&format!(
                "{}function {}: {};{}",
                prefix_kw, method_name, pas_ret, tail
            ));
        } else {
            builder.line(&format!(
                "{}function {}({}): {};{}",
                prefix_kw, method_name, args_str, pas_ret, tail
            ));
        }
    } else {
        if args_str.is_empty() {
            builder.line(&format!("{}procedure {};{}", prefix_kw, method_name, tail));
        } else {
            builder.line(&format!(
                "{}procedure {}({});{}",
                prefix_kw, method_name, args_str, tail
            ));
        }
    }
}

// ============================================================================
// Class implementation (implementation side)
// ============================================================================

fn emit_wrapper_class_impl(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
) {
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
    let ctor_names = constructor_pascal_names(ir, &s.name);
    let mut ctor_idx = 0usize;
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        let ctor_name = &ctor_names[ctor_idx];
        ctor_idx += 1;
        emit_constructor_impl(builder, &class_name, ctor_name, func, ir, targets, false);
        if has_owned_wrapper_arg(func, targets) {
            emit_constructor_impl(builder, &class_name, ctor_name, func, ir, targets, true);
        }
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

    // Release: detach the raw record; ownership transfers to the caller.
    builder.line(&format!(
        "function {}.Release: {};",
        class_name,
        record_type_name(&s.name)
    ));
    builder.line("begin");
    builder.indent();
    builder.line("Result := FRaw;");
    builder.line("FOwned := False;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Instance + static method bodies. Same dedup-by-Pascal-name as in
    // emit_wrapper_class_decl above — otherwise we'd emit two function
    // bodies for the same forward declaration.
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    emitted.insert("wrap".to_string());
    emitted.insert("raw".to_string());
    emitted.insert("release".to_string());
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
        emit_method_impl(builder, &class_name, &ffi, func, ir, targets, false);
        if has_owned_wrapper_arg(func, targets) {
            emit_method_impl(builder, &class_name, &ffi, func, ir, targets, true);
        }
    }
}

fn emit_constructor_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    ctor_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    wrapper_variant: bool,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir, targets, wrapper_variant);
    let signature = if args_str.is_empty() {
        format!("constructor {}.{};", class_name, ctor_name)
    } else {
        format!("constructor {}.{}({});", class_name, ctor_name, args_str)
    };

    builder.line(&signature);
    builder.line("begin");
    builder.indent();
    builder.line("inherited Create;");

    let mut consumed: Vec<String> = Vec::new();
    let call_args: Vec<String> = visible
        .iter()
        .map(|a| {
            let name = sanitize_identifier(&a.name);
            if wrapper_variant && is_owned_wrapper_arg(a, targets) {
                consumed.push(name.clone());
                format!("{}.FRaw", name)
            } else {
                name
            }
        })
        .collect();

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
    // libazul consumed the bytes of by-value wrapper args: disarm their
    // destructors so they don't double-free.
    for name in &consumed {
        builder.line(&format!("{}.FOwned := False;", name));
    }
    builder.line("FOwned := True;");
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

fn emit_method_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    _ffi: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    wrapper_variant: bool,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir, targets, wrapper_variant);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    let prefix_kw = if is_static { "class " } else { "" };

    let signature = if let Some(ret) = &func.return_type {
        let pas_ret = return_type_to_pascal(ret, ir, targets);
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
    let mut self_by_value = false;
    if takes_self {
        // Inspect args[0] of the IR signature: Owned means the C
        // function takes the record by value (`AzFoo`), Ref/Ptr/etc.
        // means it takes a pointer (`AzFoo*`). The C external
        // declaration mirrors this, so we must match — passing `@FRaw`
        // where a value is expected raises "Incompatible type for
        // arg no. 1: Got Pointer, expected TAzFoo".
        self_by_value = func
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
    let mut consumed: Vec<String> = Vec::new();
    for a in &visible {
        let name = sanitize_identifier(&a.name);
        if wrapper_variant && is_owned_wrapper_arg(a, targets) {
            consumed.push(name.clone());
            call_args.push(format!("{}.FRaw", name));
        } else {
            call_args.push(name);
        }
    }

    // See emit_constructor_impl for why we use `func.c_name` instead
    // of `{ffi}_{method_name}`.
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    if let Some(ret) = &func.return_type {
        if let Some(ret_wrapper) = wrapper_class_for_return(ret, targets) {
            // Wrap the raw return value in a fresh wrapper instance so
            // the idiomatic surface composes (`body.WithChild(child)`
            // returns a TDom, not a TAzDom). `property Raw` /
            // `Release` remain the escape hatches back to the record.
            builder.line(&format!("Result := {}.Wrap({});", ret_wrapper, call));
        } else {
            builder.line(&format!("Result := {};", call));
        }
    } else {
        builder.line(&format!("{};", call));
    }

    // libazul consumed the bytes of by-value wrapper args: disarm their
    // destructors so they don't double-free. Mirrors the self-consume
    // below (JVM/CLR `__consume` pattern, commit 62094b885).
    for name in &consumed {
        builder.line(&format!("{}.FOwned := False;", name));
    }

    // Consume-after-by-value: when the C ABI takes `self` by value
    // (DeepCopy / consuming-self method), Rust now owns the bytes
    // inside `FRaw`. Flip `FOwned := False;` so the destructor's
    // `if FOwned then <Type>_delete(@FRaw)` guard skips on cleanup
    // and we don't double-free. Mirrors the JVM/CLR `__consume`
    // pattern landed in commit 62094b885.
    if self_by_value {
        builder.line("FOwned := False;");
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

/// Does this arg map to a wrapper class in the wrapper-typed overload?
/// Only BY-VALUE (owned) args qualify: pointer args may be buffer/base
/// pointers (`CopyFromPtr(ptr, len)`) where a single-object wrapper
/// would be semantically wrong.
fn is_owned_wrapper_arg(
    a: &super::super::ir::FunctionArg,
    targets: &BTreeSet<String>,
) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned) && targets.contains(a.type_name.trim())
}

/// Does the function take at least one by-value arg whose type has a
/// wrapper class (=> a wrapper-typed `overload` variant is emitted)?
fn has_owned_wrapper_arg(func: &FunctionDef, targets: &BTreeSet<String>) -> bool {
    visible_user_args(func)
        .iter()
        .any(|a| is_owned_wrapper_arg(a, targets))
}

/// The wrapper class name for a return type, if the returned struct has
/// a wrapper. Only exact by-value struct returns qualify (`Dom`), never
/// pointer forms.
fn wrapper_class_for_return(ret: &str, targets: &BTreeSet<String>) -> Option<String> {
    let trimmed = ret.trim();
    if targets.contains(trimmed) {
        Some(pascal_class_name(trimmed))
    } else {
        None
    }
}

/// Map a return type: wrapper class when one exists, raw Pascal type
/// otherwise.
fn return_type_to_pascal(ret: &str, ir: &CodegenIR, targets: &BTreeSet<String>) -> String {
    if let Some(w) = wrapper_class_for_return(ret, targets) {
        w
    } else {
        map_type_to_pascal(ret, ir)
    }
}

fn format_arg_list(
    args: &[&super::super::ir::FunctionArg],
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    wrapper_variant: bool,
) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let pas_ty = if wrapper_variant && is_owned_wrapper_arg(a, targets) {
                pascal_class_name(a.type_name.trim())
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
                    ArgRefKind::Ref
                    | ArgRefKind::RefMut
                    | ArgRefKind::Ptr
                    | ArgRefKind::PtrMut => ptr_type_for_arg(&a.type_name, ir),
                }
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

/// Compute the final Pascal constructor names for a class, in
/// `functions_for_class` order (Constructor/Default kinds only). Both the
/// declaration and implementation passes consume this list by index so
/// they always agree.
///
/// Naming: `Create` + suffix, where the suffix drops a leading
/// `create_` / `new_` verb from the api.json method name (`create_body`
/// -> `CreateBody`, `new_c` -> `CreateC`). Collision guard: Pascal
/// identifiers are case-insensitive, and stripping may produce a name
/// that collides with a sibling constructor whose parameter list is
/// identical (`ImageRef.new_rawimage` vs `ImageRef.raw_image`). When the
/// stripped name matches an already-assigned name or another sibling's
/// UNSTRIPPED name, fall back to the unstripped spelling. (Same-name
/// overloads that already exist today — e.g. `new` and `create` both
/// mapping to plain `Create` — are preserved; their parameter lists
/// differ, which Pascal's `overload` handles.)
fn constructor_pascal_names(ir: &CodegenIR, class_name: &str) -> Vec<String> {
    let ctors: Vec<&FunctionDef> = ir
        .functions_for_class(class_name)
        .into_iter()
        .filter(|f| matches!(f.kind, FunctionKind::Constructor | FunctionKind::Default))
        .collect();

    let unstripped: Vec<String> = ctors
        .iter()
        .map(|f| format!("Create{}", constructor_suffix(&f.method_name)))
        .collect();

    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<String> = Vec::with_capacity(ctors.len());
    for (i, func) in ctors.iter().enumerate() {
        let candidate = format!(
            "Create{}",
            stripped_constructor_suffix(&func.method_name)
        );
        let cand_lower = candidate.to_ascii_lowercase();
        let collides_with_sibling = unstripped
            .iter()
            .enumerate()
            .any(|(j, u)| j != i && u.to_ascii_lowercase() == cand_lower);
        let final_name = if used.contains(&cand_lower) || collides_with_sibling {
            unstripped[i].clone()
        } else {
            candidate
        };
        used.insert(final_name.to_ascii_lowercase());
        out.push(final_name);
    }
    out
}

/// Legacy (unstripped) constructor suffix: the canonical IR `new` /
/// `create` map to the empty suffix (plain `Create`); any other
/// constructor name appears as a PascalCased suffix verbatim.
fn constructor_suffix(method_name: &str) -> String {
    if method_name == "new" || method_name == "create" {
        return String::new();
    }
    to_pascal_case(method_name)
}

/// Stutter-free constructor suffix: additionally drops a leading
/// `create_` / `new_` verb so the Pascal `Create` prefix isn't doubled
/// (`create_body` -> `Body`, `new_rawimage` -> `Rawimage`).
fn stripped_constructor_suffix(method_name: &str) -> String {
    if method_name == "new" || method_name == "create" {
        return String::new();
    }
    let stripped = method_name
        .strip_prefix("create_")
        .or_else(|| method_name.strip_prefix("new_"))
        .unwrap_or(method_name);
    to_pascal_case(stripped)
}

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
