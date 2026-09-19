//! Idiomatic Pascal class wrappers with `destructor Destroy; override;`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit a `T<TypeName> = class` (descended from `TObject`) that:
//!
//! 1. Holds the underlying FFI record (`TAzTypeName`) by value in a protected `FRaw` field
//!    (protected, not private: FPC 3.0 compiles a generic subclass such as
//!    `TAzApp<T>` in the specializing unit, where private fields are invisible).
//! 2. Provides one Pascal constructor per IR `FunctionKind::Constructor` / `Default` function.
//!    Naming (see [`constructor_pascal_names`]): `new` / `create` -> `Create`; `create_<x>` /
//!    `new_<x>` -> `<X>` (`create_body` -> `Body`, `create_p` -> `P`); a trailing `_with_<arg>`
//!    naming one of the constructor's own arguments collapses into an overload of the base name
//!    (`create_p_with_text(text)` -> `P(text)`); everything else keeps the legacy `Create<X>`
//!    spelling. A collision guard (sibling constructors, methods, reserved words, identical
//!    parameter lists) falls back to `Create<X>`.
//! 3. Provides a `destructor Destroy; override;` that calls the `<TypeName>_delete` external.
//!    Standard Pascal `obj.Free;` invokes this destructor automatically.
//! 4. Surfaces every non-trait method on `TypeName` as an idiomatic instance / class method
//!    delegating to the underlying FFI symbol, in up to three overloads ([`ArgVariant`]):
//!    raw records, wrapper classes for by-value args that have one, and Pascal `string` for
//!    by-value `String` args.
//! 5. Fluent rules (every wrapper class, no names involved):
//!    - a `void` instance method taking `self` by pointer returns `Self` (`AddChild(...)`,
//!      `SetButtonType(...)` chain);
//!    - a method taking `self` BY VALUE and returning its own class mutates `FRaw` in place and
//!      returns `Self` (`WithChild(...).WithCss(...)` allocates nothing);
//!    - any other by-value `self` is CONSUMED: the raw bytes move into libazul and the wrapper
//!      object frees itself (`Btn.Dom` leaves `Btn` dead, exactly like Rust's `self`);
//!    - by-value wrapper ARGUMENTS are consumed the same way: the callee takes ownership and the
//!      argument wrapper is freed (`Body.AddChild(LabelDom)` — do not use `LabelDom` afterwards,
//!      `Clone` it first if you need to).
//! 6. Smart callback setters: for every `with_on_<x>(self, data: RefAny, cb: <Kind>)` whose kind
//!    is a host-invoker kind (shared `smart_callback_setter_info`), an `On<X>` overload set that
//!    takes a Pascal callback (method pointer, plain function, or typed `<T>` function) and binds
//!    the model of the running callback / the current app as `data`.
//! 7. Provides `function Release: TAz<TypeName>;` — detaches and returns the raw record,
//!    transferring ownership to the caller (the destructor will no longer call `_delete`).
//!
//! All wrapper classes are forward-declared (`TDom = class;`) at the top of one `type` block that
//! also holds the managed callback surface and the app helper (they reference each other).
//!
//! Plain POD structs without a `_delete` get *no* wrapper — users manipulate them through the
//! `TAzFoo` record directly. Tagged-union enums similarly aren't wrapped (Pascal already provides
//! ergonomic variant-record syntax).

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory},
        managed_host_invoker::{host_invoker_kinds, smart_callback_setter_info, wrapper_name},
    },
    ffi_type_name, managed, map_type_to_pascal, record_type_name, sanitize_identifier, to_pascal_case,
    types::ptr_type_for_arg,
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

    // Forward declarations so wrapper methods, the callback types and the
    // app helper can reference sibling classes independent of order.
    builder.line("{ Forward declarations so wrapper methods can reference sibling classes. }");
    for s in &targets {
        builder.line(&format!("{} = class;", pascal_class_name(&s.name)));
    }
    builder.blank();

    managed::emit_callback_surface_types(builder, ir, &target_names);

    for s in &targets {
        emit_wrapper_class_decl(builder, s, ir, &target_names);
    }

    managed::emit_app_helper_types(builder, ir, config, &target_names);

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
pub(super) fn collect_wrapper_targets<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<&'a StructDef> {
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

/// The names of all wrapper targets (what the managed prelude needs).
pub(super) fn wrapper_target_names(ir: &CodegenIR, config: &CodegenConfig) -> BTreeSet<String> {
    collect_wrapper_targets(ir, config)
        .iter()
        .map(|s| s.name.clone())
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
// Argument variants
// ============================================================================

/// Which spelling of a function's argument list an overload uses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArgVariant {
    /// Every argument is the raw `TAz*` record / pointer.
    Raw,
    /// By-value args whose type has a wrapper class take the wrapper (consumed).
    Wrapper,
    /// Like `Wrapper`, but by-value `String` args take a Pascal `string`.
    Native,
}

fn is_owned_string_arg(a: &FunctionArg) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned) && a.type_name.trim() == "String"
}

/// Does this arg map to a wrapper class in the wrapper-typed overload?
/// Only BY-VALUE (owned) args qualify: pointer args may be buffer/base
/// pointers (`CopyFromPtr(ptr, len)`) where a single-object wrapper
/// would be semantically wrong.
fn is_owned_wrapper_arg(a: &FunctionArg, targets: &BTreeSet<String>) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned) && targets.contains(a.type_name.trim())
}

/// The overload variants a function gets: `Raw` always; `Wrapper` when a
/// by-value arg has a wrapper class; `Native` when a by-value arg is a
/// `String`.
fn variants_for(func: &FunctionDef, targets: &BTreeSet<String>) -> Vec<ArgVariant> {
    let visible = visible_user_args(func);
    let mut out = vec![ArgVariant::Raw];
    if visible.iter().any(|a| is_owned_wrapper_arg(a, targets)) {
        out.push(ArgVariant::Wrapper);
    }
    if visible.iter().any(|a| is_owned_string_arg(a)) {
        out.push(ArgVariant::Native);
    }
    out
}

/// Pascal parameter spelling of one argument under a variant.
fn arg_decl(a: &FunctionArg, variant: ArgVariant, ir: &CodegenIR, targets: &BTreeSet<String>, members: &BTreeSet<String>) -> String {
    let name = sanitize_arg(&a.name, members);
    if variant == ArgVariant::Native && is_owned_string_arg(a) {
        return format!("const {}: string", name);
    }
    if variant != ArgVariant::Raw && is_owned_wrapper_arg(a, targets) {
        return format!("{}: {}", name, pascal_class_name(a.type_name.trim()));
    }
    let pas_ty = match a.ref_kind {
        ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            ptr_type_for_arg(&a.type_name, ir)
        }
    };
    format!("{}: {}", name, pas_ty)
}

fn format_arg_list(args: &[&FunctionArg], variant: ArgVariant, ir: &CodegenIR, targets: &BTreeSet<String>, members: &BTreeSet<String>) -> String {
    args.iter()
        .map(|a| arg_decl(a, variant, ir, targets, members))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The expression passed to the C call for one argument, plus the wrapper
/// objects consumed by the call (freed afterwards).
fn call_arg_exprs(args: &[&FunctionArg], variant: ArgVariant, targets: &BTreeSet<String>, members: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let mut consumed = Vec::new();
    let exprs = args
        .iter()
        .map(|a| {
            let name = sanitize_arg(&a.name, members);
            if variant == ArgVariant::Native && is_owned_string_arg(a) {
                format!("azul_string_from({})", name)
            } else if variant != ArgVariant::Raw && is_owned_wrapper_arg(a, targets) {
                consumed.push(name.clone());
                format!("{}.FRaw", name)
            } else {
                name
            }
        })
        .collect();
    (exprs, consumed)
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
    let members = class_member_names(ir, &s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
    }

    builder.line(&format!("{} = class(TObject)", class_name));
    builder.line("protected");
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
    // precomputed per class so the decl and impl passes agree.
    let ctor_names = constructor_pascal_names(ir, &s.name);
    let mut ctor_idx = 0usize;
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        let ctor_name = &ctor_names[ctor_idx];
        ctor_idx += 1;
        for variant in variants_for(func, targets) {
            emit_constructor_decl(builder, ctor_name, func, ir, targets, variant, &members);
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
    let mut emitted: BTreeSet<String> = fixed_member_names();
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
        let variants = variants_for(func, targets);
        let overloaded = variants.len() > 1;
        for variant in variants {
            emit_method_decl(builder, func, ir, targets, variant, overloaded, &members);
        }
    }

    // Smart callback setters.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart, kind)) = smart_callback_setter_info(func) else { continue };
        let name = smart_setter_name(&smart);
        if !emitted.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let Some(sig) = smart_setter_sig(&kind, ir, targets) else { continue };
        for (generic, ty, guard) in smart_setter_overloads(&sig) {
            if let Some(g) = guard {
                builder.line(g);
            }
            let tparams = if generic { "<T: class>" } else { "" };
            builder.line(&format!(
                "function {}{}(Callback: {}): {}; overload;",
                name, tparams, ty, class_name
            ));
            if guard.is_some() {
                builder.line(managed::FUNCREF_GUARD_END);
            }
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
    variant: ArgVariant,
    members: &BTreeSet<String>,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, variant, ir, targets, members);
    if args_str.is_empty() {
        builder.line(&format!("constructor {}; overload;", ctor_name));
    } else {
        builder.line(&format!(
            "constructor {}({}); overload;",
            ctor_name, args_str
        ));
    }
}

/// What a method returns on the Pascal side and what happens to `self`.
enum SelfPlan {
    /// Static (class) method: no receiver.
    Static,
    /// Receiver by pointer, C returns the class itself or nothing: `Result := Self`
    /// after an in-place call (fluent) — only for `void` returns.
    FluentVoid,
    /// Receiver by value and C returns the class: `FRaw := call; Result := Self`.
    InPlace,
    /// Receiver by pointer, ordinary return value.
    Borrowed,
    /// Receiver by value, ordinary (or no) return value: the wrapper frees itself.
    Consumed,
}

fn self_plan(func: &FunctionDef) -> SelfPlan {
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    if !takes_self {
        return SelfPlan::Static;
    }
    let by_value = func
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let returns_class = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);
    match (by_value, returns_class, func.return_type.is_some()) {
        (true, true, _) => SelfPlan::InPlace,
        (true, false, _) => SelfPlan::Consumed,
        (false, _, false) => SelfPlan::FluentVoid,
        (false, _, true) => SelfPlan::Borrowed,
    }
}

/// The Pascal return type of a method (None => procedure).
fn method_return(func: &FunctionDef, ir: &CodegenIR, targets: &BTreeSet<String>) -> Option<String> {
    match self_plan(func) {
        SelfPlan::FluentVoid | SelfPlan::InPlace => Some(pascal_class_name(&func.class_name)),
        _ => func.return_type.as_ref().map(|r| return_type_to_pascal(r, ir, targets)),
    }
}

fn emit_method_decl(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    variant: ArgVariant,
    overloaded: bool,
    members: &BTreeSet<String>,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, variant, ir, targets, members);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);

    let prefix_kw = if is_static { "class " } else { "" };
    let tail = if overloaded { " overload;" } else { "" };
    let params = if args_str.is_empty() { String::new() } else { format!("({})", args_str) };
    match method_return(func, ir, targets) {
        Some(pas_ret) => builder.line(&format!(
            "{}function {}{}: {};{}",
            prefix_kw, method_name, params, pas_ret, tail
        )),
        None => builder.line(&format!("{}procedure {}{};{}", prefix_kw, method_name, params, tail)),
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
    let members = class_member_names(ir, &s.name);

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
        for variant in variants_for(func, targets) {
            emit_constructor_impl(builder, &class_name, ctor_name, func, ir, targets, variant, &members);
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
    let mut emitted: BTreeSet<String> = fixed_member_names();
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
        for variant in variants_for(func, targets) {
            emit_method_impl(builder, &class_name, func, ir, targets, variant, &members);
        }
    }

    for func in ir.functions_for_class(&s.name) {
        let Some((smart, kind)) = smart_callback_setter_info(func) else { continue };
        let name = smart_setter_name(&smart);
        if !emitted.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let Some(sig) = smart_setter_sig(&kind, ir, targets) else { continue };
        emit_smart_setter_impl(builder, &class_name, &name, func, &sig, &kind);
    }
}

fn emit_constructor_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    ctor_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    variant: ArgVariant,
    members: &BTreeSet<String>,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, variant, ir, targets, members);
    let signature = if args_str.is_empty() {
        format!("constructor {}.{};", class_name, ctor_name)
    } else {
        format!("constructor {}.{}({});", class_name, ctor_name, args_str)
    };

    builder.line(&signature);
    builder.line("begin");
    builder.indent();
    builder.line("inherited Create;");

    let (call_args, consumed) = call_arg_exprs(&visible, variant, targets, members);

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
    emit_consumed(builder, &consumed);
    builder.line("FOwned := True;");
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

/// libazul took the bytes of by-value wrapper args: disarm their
/// destructors and free the (now empty) wrapper objects.
fn emit_consumed(builder: &mut CodeBuilder, consumed: &[String]) {
    for name in consumed {
        builder.line(&format!("{}.FOwned := False;", name));
        builder.line(&format!("{}.Free;", name));
    }
}

fn emit_method_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    targets: &BTreeSet<String>,
    variant: ArgVariant,
    members: &BTreeSet<String>,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, variant, ir, targets, members);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);
    let plan = self_plan(func);

    let prefix_kw = if is_static { "class " } else { "" };
    let params = if args_str.is_empty() { String::new() } else { format!("({})", args_str) };
    let signature = match method_return(func, ir, targets) {
        Some(pas_ret) => format!("{}function {}.{}{}: {};", prefix_kw, class_name, method_name, params, pas_ret),
        None => format!("{}procedure {}.{}{};", prefix_kw, class_name, method_name, params),
    };

    builder.line(&signature);
    builder.line("begin");
    builder.indent();

    let mut call_args: Vec<String> = Vec::new();
    match plan {
        SelfPlan::Static => {}
        SelfPlan::InPlace | SelfPlan::Consumed => call_args.push("FRaw".to_string()),
        SelfPlan::FluentVoid | SelfPlan::Borrowed => call_args.push("@FRaw".to_string()),
    }
    let (user_args, consumed) = call_arg_exprs(&visible, variant, targets, members);
    call_args.extend(user_args);

    // See emit_constructor_impl for why we use `func.c_name` instead
    // of `{ffi}_{method_name}`.
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    match plan {
        SelfPlan::InPlace => {
            builder.line(&format!("FRaw := {};", call));
            emit_consumed(builder, &consumed);
            builder.line("Result := Self;");
        }
        SelfPlan::FluentVoid => {
            builder.line(&format!("{};", call));
            emit_consumed(builder, &consumed);
            builder.line("Result := Self;");
        }
        SelfPlan::Static | SelfPlan::Borrowed | SelfPlan::Consumed => {
            emit_result_assignment(builder, func, &call, targets);
            emit_consumed(builder, &consumed);
            if matches!(plan, SelfPlan::Consumed) {
                // Rust now owns the bytes that were in FRaw: disarm the
                // destructor and free this (dead) wrapper object.
                builder.line("FOwned := False;");
                builder.line("Free;");
            }
        }
    }

    builder.dedent();
    builder.line("end;");
    builder.blank();
}

/// `Result := <wrapped call>` for a method with an ordinary return value.
fn emit_result_assignment(builder: &mut CodeBuilder, func: &FunctionDef, call: &str, targets: &BTreeSet<String>) {
    match &func.return_type {
        Some(ret) => match wrapper_class_for_return(ret, targets) {
            // Wrap the raw return value in a fresh wrapper instance so the
            // idiomatic surface composes. `Raw` / `Release` remain the
            // escape hatches back to the record.
            Some(ret_wrapper) => builder.line(&format!("Result := {}.Wrap({});", ret_wrapper, call)),
            None => builder.line(&format!("Result := {};", call)),
        },
        None => builder.line(&format!("{};", call)),
    }
}

// ============================================================================
// Smart callback setters
// ============================================================================

fn smart_setter_name(smart_snake: &str) -> String {
    idiomatic_method_name(smart_snake)
}

fn smart_setter_sig(kind: &str, ir: &CodegenIR, targets: &BTreeSet<String>) -> Option<managed::CallbackSig> {
    host_invoker_kinds(ir)
        .find(|cb| wrapper_name(cb) == kind)
        .map(|cb| managed::callback_sig(cb, ir, targets))
}

/// `(generic?, parameter type, needs the function-reference guard?)` for
/// every overload of a smart setter.
/// Every `On<Event>` overload as `(generic, parameter type, version guard)`.
/// The `TAz<K>Invoker` overload takes a ready dispatcher object (on FPC 3.0.x
/// the only typed form: `TAz<K>TypedWrapper<T>.Create(Fn)`); the generic
/// methods need FPC 3.2.0+, the `reference to` ones FPC 3.3.1+.
fn smart_setter_overloads(sig: &managed::CallbackSig) -> Vec<(bool, String, Option<&'static str>)> {
    let k = &sig.kind;
    let mut out = vec![
        (false, managed::event_type(k), None),
        (false, managed::proc_type(k), None),
        (false, managed::invoker_class(k), None),
    ];
    if sig.model_arg.is_some() {
        let g = Some(managed::GENERIC_METHOD_GUARD);
        let r = Some(managed::FUNCREF_GUARD);
        out.push((true, format!("{}<T>", managed::func_type(k)), g));
        if sig.has_model_only_form() {
            out.push((true, format!("{}<T>", managed::model_func_type(k)), g));
        }
        out.push((true, format!("{}<T>", managed::ref_type(k)), r));
        if sig.has_model_only_form() {
            out.push((true, format!("{}<T>", managed::model_ref_type(k)), r));
        }
    }
    out
}

fn emit_smart_setter_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    name: &str,
    func: &FunctionDef,
    sig: &managed::CallbackSig,
    kind: &str,
) {
    let plan = self_plan(func);
    let recv = match plan {
        SelfPlan::InPlace | SelfPlan::Consumed => "FRaw",
        _ => "@FRaw",
    };
    for (generic, ty, guard) in smart_setter_overloads(sig) {
        if let Some(g) = guard {
            builder.line(g);
        }
        let tparam = if generic { "<T>" } else { "" };
        builder.line(&format!(
            "function {}.{}{}(Callback: {}): {};",
            class_name, name, tparam, ty, class_name
        ));
        builder.line("begin");
        builder.indent();
        let dispatcher = if generic {
            format!("{}<T>.Create(Callback)", managed::typed_wrapper_class(kind))
        } else if ty == managed::invoker_class(kind) {
            // A ready dispatcher: the handle table owns it from here on.
            "Callback".to_string()
        } else {
            format!("{}.Create(Callback)", managed::wrapper_class(kind))
        };
        let call = format!(
            "{}({}, azul_refany_current, {}({}))",
            func.c_name,
            recv,
            managed::register_fn(kind),
            dispatcher
        );
        match plan {
            SelfPlan::InPlace => builder.line(&format!("FRaw := {};", call)),
            _ => builder.line(&format!("{};", call)),
        }
        builder.line("Result := Self;");
        builder.dedent();
        builder.line("end;");
        if guard.is_some() {
            builder.line(managed::FUNCREF_GUARD_END);
        }
        builder.blank();
    }
}

// ============================================================================
// Argument helpers
// ============================================================================

/// Filter the implicit `self` argument out of a function's arg list.
/// For instance / mutating / deep-copy methods args[0] IS the self,
/// regardless of how api.json named it (`instance`, snake-cased class,
/// `mime_type_data_vec`, etc.) — matches the C#/Java/Kotlin/Fortran fix.
fn visible_user_args(func: &FunctionDef) -> Vec<&FunctionArg> {
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

/// Member names every wrapper class defines itself.
fn fixed_member_names() -> BTreeSet<String> {
    ["wrap", "raw", "release", "destroy", "fraw", "fowned", "free", "create"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Every (case-insensitive) member name the wrapper class declares: the
/// fixed ones plus one per surviving api.json method and smart setter.
fn class_member_names(ir: &CodegenIR, class_name: &str) -> BTreeSet<String> {
    let mut members = fixed_member_names();
    members.extend(
        constructor_pascal_names(ir, class_name)
            .iter()
            .map(|n| n.to_ascii_lowercase()),
    );
    for func in ir.functions_for_class(class_name) {
        if func.kind.is_trait_function() {
            continue;
        }
        members.insert(idiomatic_method_name(&func.method_name).to_ascii_lowercase());
        if let Some((smart, _)) = smart_callback_setter_info(func) {
            members.insert(smart_setter_name(&smart).to_ascii_lowercase());
        }
    }
    members
}

/// Pascal is case-insensitive and gives parameters the same scope as the
/// enclosing class's members, so `Connect(ticket: ...)` is a duplicate
/// identifier once the class also declares `function Ticket`. A parameter
/// that collides with a member of ITS OWN class gets a trailing underscore.
fn sanitize_arg(name: &str, members: &BTreeSet<String>) -> String {
    let base = sanitize_identifier(name);
    if members.contains(&base.to_ascii_lowercase()) {
        format!("{}_", base)
    } else {
        base
    }
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Pascal class name for an IR type — drop the `Az` prefix and prepend `T`.
/// e.g. `App` -> `TApp`, `Dom` -> `TDom`. The `Az` prefix is preserved on
/// the underlying record (`TAzApp`) and on the external symbol; only the
/// idiomatic class is unprefixed for nicer user-facing names.
pub(super) fn pascal_class_name(raw: &str) -> String {
    format!("T{}", raw)
}

/// Compute the final Pascal constructor names for a class, in
/// `functions_for_class` order (Constructor/Default kinds only). Both the
/// declaration and implementation passes consume this list by index so
/// they always agree.
///
/// Preferred spelling ([`preferred_constructor_name`]): `new` / `create`
/// -> `Create`; `create_<x>` / `new_<x>` -> `<X>`, with a trailing
/// `_with_<arg>` (naming one of the constructor's own args) collapsed into
/// an overload of `<X>`; `Default` -> `CreateDefault`; anything else
/// `Create<X>`. Guard: when the preferred name is a reserved word, collides
/// (case-insensitively) with a method of the class, with another sibling's
/// legacy spelling, or with an already-assigned name whose raw parameter
/// list is identical (Pascal cannot overload those), fall back to the
/// legacy `Create<X>` spelling.
fn constructor_pascal_names(ir: &CodegenIR, class_name: &str) -> Vec<String> {
    let ctors: Vec<&FunctionDef> = ir
        .functions_for_class(class_name)
        .filter(|f| matches!(f.kind, FunctionKind::Constructor | FunctionKind::Default))
        .collect();
    let method_names: BTreeSet<String> = ir
        .functions_for_class(class_name)
        .filter(|f| !matches!(f.kind, FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete))
        .filter(|f| !f.kind.is_trait_function())
        .map(|f| idiomatic_method_name(&f.method_name).to_ascii_lowercase())
        .chain(fixed_member_names().into_iter().filter(|n| n != "create"))
        .collect();

    let legacy: Vec<String> = ctors.iter().map(|f| legacy_constructor_name(f)).collect();
    let raw_sig = |f: &FunctionDef| -> Vec<String> {
        f.args.iter().map(|a| format!("{:?}:{}", a.ref_kind, a.type_name.trim())).collect()
    };

    let mut used: Vec<(String, Vec<String>)> = Vec::new();
    let mut out: Vec<String> = Vec::with_capacity(ctors.len());
    for (i, func) in ctors.iter().enumerate() {
        let candidate = preferred_constructor_name(func);
        let cand_lower = candidate.to_ascii_lowercase();
        let sig = raw_sig(func);
        let collides_with_sibling = legacy
            .iter()
            .enumerate()
            .any(|(j, u)| j != i && u.to_ascii_lowercase() == cand_lower);
        let same_signature_taken = used
            .iter()
            .any(|(n, s)| *n == cand_lower && *s == sig);
        let final_name = if method_names.contains(&cand_lower)
            || collides_with_sibling
            || same_signature_taken
            || super::is_pascal_reserved(&cand_lower)
        {
            legacy[i].clone()
        } else {
            candidate
        };
        used.push((final_name.to_ascii_lowercase(), sig));
        out.push(final_name);
    }
    out
}

/// Legacy spelling: `Create` + PascalCase of the api.json name (`new` /
/// `create` -> plain `Create`, `createDefault` -> `CreateDefault`).
fn legacy_constructor_name(func: &FunctionDef) -> String {
    if func.kind == FunctionKind::Default {
        return "CreateDefault".to_string();
    }
    let m = func.method_name.as_str();
    if m == "new" || m == "create" {
        return "Create".to_string();
    }
    format!("Create{}", to_pascal_case(m))
}

/// Preferred spelling (see [`constructor_pascal_names`]).
fn preferred_constructor_name(func: &FunctionDef) -> String {
    if func.kind == FunctionKind::Default {
        return "CreateDefault".to_string();
    }
    let m = func.method_name.as_str();
    if m == "new" || m == "create" {
        return "Create".to_string();
    }
    let Some(rest) = m.strip_prefix("create_").or_else(|| m.strip_prefix("new_")) else {
        return format!("Create{}", to_pascal_case(m));
    };
    // `<x>_with_<arg>` where <arg> is one of this constructor's args:
    // overload of `<x>`.
    let mut base = rest;
    if let Some(pos) = rest.rfind("_with_") {
        let arg = &rest[pos + "_with_".len()..];
        if func.args.iter().any(|a| a.name == arg) {
            base = &rest[..pos];
        }
    }
    if base.is_empty() {
        return "Create".to_string();
    }
    sanitize_identifier(&to_pascal_case(base))
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
    let named = match pascal.as_str() {
        "ToString" | "Equals" | "GetHashCode" | "Free" | "Destroy" | "ClassName" | "ClassType"
        | "Dispatch" => format!("{}_X", pascal),
        _ => pascal,
    };
    // Pascal is case-insensitive: `Set` IS the reserved word `set`. The
    // snake_case → PascalCase step above never went through
    // `sanitize_identifier`, so an api.json method called `set` (Db.set,
    // 2026-09-07) produced `function Set(...)` and FPC stopped with
    // `"identifier" expected but "SET" found` — the same trailing-underscore
    // rule the parameter names already follow.
    if super::is_pascal_reserved(&named.to_ascii_lowercase()) {
        return format!("{}_", named);
    }
    named
}

fn sanitize_comment(s: &str) -> String {
    // Strip both `{` and `}` so doc strings cannot accidentally open or
    // close Pascal block comments (matches lang_pascal/types.rs).
    s.replace('{', "(")
        .replace('}', ")")
        .replace(['\n', '\r'], " ")
}
