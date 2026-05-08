//! Idiomatic Perl class wrappers under `package Azul::<Type>`.
//!
//! Every struct that has a matching `<TypeName>_delete` C function gets a
//! lightweight Perl class that:
//!
//! 1. Stores the underlying opaque pointer in a blessed scalar reference
//!    (`bless \$ptr, $class`). This is the canonical Perl idiom for opaque
//!    handles — accessing `$$self` recovers the raw pointer.
//! 2. Defines a `DESTROY` method (Perl's deterministic refcount-driven
//!    finalizer) that calls the corresponding `Azul::FFI::Az<Type>_delete`
//!    when the object is freed.
//! 3. Exposes idiomatic class methods (constructors, static helpers) and
//!    instance methods (anything that takes `&self` / `&mut self`).
//!
//! Method naming: drop the `Az` prefix and the `<TypeName>_` segment, then
//! convert `camelCase` to `snake_case`. So:
//!
//! - `AzApp_create`        → `Azul::App->create`        (static)
//! - `AzApp_run`           → `$app->run`                (instance)
//! - `AzAppConfig_default` → `Azul::AppConfig->default` (static)
//! - `AzDom_addChild`      → `$dom->add_child`          (instance)
//!
//! POD structs without a `_delete` get no wrapper — users instantiate the
//! `Azul::AzFoo` record class directly via `Azul::AzFoo->new(...)`.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef, FunctionKind, StructDef};
use super::types::{should_emit_struct, snake_case};

// ============================================================================
// Public entry point
// ============================================================================

/// Emit Perl wrapper packages for every struct that owns heap memory.
pub fn emit_wrappers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# ============================================================");
    builder.line("# Idiomatic wrappers (Az prefix dropped). Use these in user code.");
    builder.line("# ============================================================");
    builder.blank();

    let delete_set = collect_delete_targets(ir);

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            // POD struct — no finalizer required, no wrapper class.
            builder.line(&format!(
                "# (no wrapper for {} -- no _delete; use Azul::{} directly)",
                s.name,
                config.apply_prefix(&s.name)
            ));
            continue;
        }
        emit_class_wrapper(builder, s, ir, config);
        builder.blank();
    }
}

// ============================================================================
// Discovery
// ============================================================================

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

// ============================================================================
// Per-class emission
// ============================================================================

fn emit_class_wrapper(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let class_name = &s.name; // unprefixed, e.g. "App"
    let prefixed = config.apply_prefix(class_name); // e.g. "AzApp"
    let snake = snake_case(class_name); // e.g. "app"

    builder.line(&format!("package Azul::{} {{", class_name));
    builder.indent();
    builder.line("use strict;");
    builder.line("use warnings;");
    builder.blank();

    // Constructor: stores the pointer (or undef) in a blessed scalar ref.
    builder.line("sub new {");
    builder.indent();
    builder.line("my ($class, $ptr) = @_;");
    builder.line("my $self = \\$ptr;");
    builder.line("return bless $self, $class;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Raw pointer accessor (escape hatch + used when one wrapper is passed
    // as an argument to another wrapper's method).
    builder.line("sub ptr {");
    builder.indent();
    builder.line("my $self = shift;");
    builder.line("return $$self;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Destructor: Perl calls DESTROY exactly once, when the refcount hits 0.
    builder.line("sub DESTROY {");
    builder.indent();
    builder.line("my $self = shift;");
    builder.line("return unless defined $$self;");
    builder.line(&format!(
        "Azul::FFI::{}_delete($$self) if Azul::FFI->can('{}_delete');",
        prefixed, prefixed
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Methods.
    let mut emitted_any_method = false;
    for func in &ir.functions {
        if func.class_name != *class_name {
            continue;
        }
        if !should_emit_method(func) {
            continue;
        }
        emit_method(builder, func, &prefixed, &snake);
        emitted_any_method = true;
    }

    if !emitted_any_method {
        builder.line("# (no public methods exposed)");
    }

    builder.dedent();
    builder.line(&format!("}} # package Azul::{}", class_name));
}

/// Should this function be exposed as a Perl method on the wrapper?
///
/// We hide the auto-generated trait functions; `_delete` runs from
/// `DESTROY`, the others (`_partialEq`, `_hash`, ...) are surfaced via
/// custom Perl operators which we don't autogenerate today.
fn should_emit_method(func: &FunctionDef) -> bool {
    !matches!(
        func.kind,
        FunctionKind::Delete
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::DebugToString
            | FunctionKind::EnumVariantConstructor
    )
}

fn emit_method(builder: &mut CodeBuilder, func: &FunctionDef, prefixed: &str, type_snake: &str) {
    let perl_method = perl_method_name(&func.method_name);
    let ffi_call = format!("Azul::FFI::{}", &func.c_name);

    let takes_self = matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut);

    let owning_class = prefixed
        .strip_prefix("Az")
        .unwrap_or(prefixed)
        .to_string();
    let returns_self_type = func
        .return_type
        .as_deref()
        .map(|t| t.trim() == owning_class)
        .unwrap_or(false);

    // Strip any explicit `self` (or class-named receiver) from the visible
    // arg list; Perl supplies it via `$$self`.
    let visible_args: Vec<&_> = func
        .args
        .iter()
        .filter(|a| a.name != "self" && a.name != type_snake)
        .collect();
    let arg_names: Vec<String> = visible_args.iter().map(|a| perl_arg_name(&a.name)).collect();

    if takes_self {
        builder.line(&format!("sub {} {{", perl_method));
        builder.indent();
        let arg_decl = if arg_names.is_empty() {
            "my $self = shift;".to_string()
        } else {
            format!(
                "my ($self, {}) = @_;",
                arg_names
                    .iter()
                    .map(|n| format!("${}", n))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        builder.line(&arg_decl);
        let mut call_args = vec!["$$self".to_string()];
        for n in &arg_names {
            call_args.push(unwrap_expr(n));
        }
        let call = format!("{}({})", ffi_call, call_args.join(", "));
        emit_method_body(builder, &call, &func.return_type, returns_self_type);
        builder.dedent();
        builder.line("}");
        builder.blank();
        return;
    }

    // Static / class method.
    builder.line(&format!("sub {} {{", perl_method));
    builder.indent();
    let arg_decl = if arg_names.is_empty() {
        "my $class = shift;".to_string()
    } else {
        format!(
            "my ($class, {}) = @_;",
            arg_names
                .iter()
                .map(|n| format!("${}", n))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    builder.line(&arg_decl);
    let call_args: Vec<String> = arg_names.iter().map(|n| unwrap_expr(n)).collect();
    let call = format!("{}({})", ffi_call, call_args.join(", "));
    emit_method_body(builder, &call, &func.return_type, returns_self_type);
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Body of a wrapper method.
///
/// If the C function returns the same struct as the wrapper class, wrap
/// the result in `$class->new($ptr)` so the caller gets a managed instance
/// with a finalizer. Otherwise return the raw FFI result verbatim.
fn emit_method_body(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
) {
    match return_type {
        None => builder.line(&format!("{};", call)),
        Some(_) if returns_self_type => {
            builder.line(&format!("return __PACKAGE__->new({});", call));
        }
        Some(_) => builder.line(&format!("return {};", call)),
    }
}

/// Unwrap a wrapper into its raw pointer when it's blessed; otherwise pass
/// the value through unchanged. Mirrors Ruby's `respond_to?(:ptr)` trick.
fn unwrap_expr(name: &str) -> String {
    format!(
        "(blessed(${n}) && ${n}->can('ptr') ? ${n}->ptr : ${n})",
        n = name
    )
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Perl method names use snake_case. The IR's `method_name` is camelCase
/// (e.g. `addChild`) or already snake-ish — normalise either to snake.
fn perl_method_name(method: &str) -> String {
    let snake = camel_to_snake(method);
    if PERL_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

fn perl_arg_name(name: &str) -> String {
    let snake = camel_to_snake(name);
    if PERL_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

fn camel_to_snake(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    let mut prev_was_lower = false;
    for (i, c) in input.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 && prev_was_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            out.push(c);
            prev_was_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

const PERL_RESERVED: &[&str] = &[
    "and", "cmp", "continue", "do", "else", "elsif", "eq", "for", "foreach", "ge", "gt", "if",
    "le", "lt", "ne", "next", "no", "not", "or", "package", "redo", "require", "return", "sub",
    "unless", "until", "use", "while", "x", "xor",
];
