//! Idiomatic Ruby class wrappers under `module Azul`.
//!
//! Every struct that has a matching `<TypeName>_delete` C function gets a
//! lightweight Ruby class that:
//!
//! 1. Holds `@ptr` (the underlying FFI struct pointer);
//! 2. Registers an `ObjectSpace.define_finalizer` that calls
//!    `Native.az_<typename>_delete(ptr)` when the Ruby object is GC'd.
//!    The finalizer proc captures only `ptr` — never `self` — to avoid the
//!    well-known "finalizer keeps the instance alive forever" trap;
//! 3. Exposes idiomatic class methods (constructors, static helpers) and
//!    instance methods (anything else that takes `&self` / `&mut self`).
//!
//! Method naming: drop the `Az` prefix and the `<TypeName>_` segment, then
//! convert `camelCase` to `snake_case`. So:
//!
//! - `AzApp_create`        → `App.create`        (static)
//! - `AzApp_run`           → `app.run`           (instance)
//! - `AzAppConfig_default` → `AppConfig.default` (static)
//! - `AzDom_addChild`      → `dom.add_child`     (instance)
//!
//! POD structs without a `_delete` get no wrapper — users instantiate the
//! `Native::AzFoo` FFI::Struct directly. (`should_emit_struct` filters out
//! the truly internal types, so we never see those here.)

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionDef, FunctionKind, StructDef};
use super::types::{should_emit_struct, snake_case};

// ============================================================================
// Public entry point
// ============================================================================

/// Emit Ruby wrapper classes for every struct that owns heap memory.
pub fn emit_wrappers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# ============================================================");
    builder.line("# Idiomatic wrappers (Az prefix dropped). Use these in user code.");
    builder.line("# ============================================================");

    let delete_set = collect_delete_targets(ir);

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            // POD struct — no finalizer needed, no wrapper class.
            builder.line(&format!(
                "# (no wrapper for {} — no _delete; use Native::{} directly)",
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

/// Build the set of struct names that have a `<Name>_delete` C function.
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
    let class_name = &s.name;
    let prefixed = config.apply_prefix(class_name); // e.g. "AzApp"
    let snake = snake_case(class_name); // e.g. "app"

    builder.line(&format!("class {}", class_name));
    builder.indent();

    // attr reader for low-level access (escape hatch, also used to pass an
    // instance to other Native.* calls that take a pointer).
    builder.line("attr_reader :ptr");
    builder.blank();

    // Constructor: stores the pointer and arms the finalizer.
    builder.line("def initialize(ptr)");
    builder.indent();
    builder.line("@ptr = ptr");
    builder.line("ObjectSpace.define_finalizer(self, self.class.finalize(ptr))");
    builder.dedent();
    builder.line("end");
    builder.blank();

    // Finalizer factory: a class-level proc that closes over `ptr` only.
    // This is critical — closing over `self` would keep the instance alive
    // and the finalizer would never fire.
    builder.line("def self.finalize(ptr)");
    builder.indent();
    builder.line(&format!("proc {{ Native.az_{}_delete(ptr) }}", snake));
    builder.dedent();
    builder.line("end");
    builder.blank();

    // AzString gets a `to_s` override that decodes the wrapped UTF-8
    // bytes into a Ruby String. AzString's C-side layout is `{ vec:
    // AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`, so we
    // read offset 0 (vec.ptr) and offset 8 (vec.len) via FFI::Pointer.
    if s.name == "String" {
        builder.line("# Decode the wrapped UTF-8 bytes into a Ruby String.");
        builder.line("def to_s");
        builder.indent();
        builder.line("vec_ptr = @ptr.get_pointer(0)");
        builder.line("vec_len = @ptr.get_uint64(8)");
        builder.line("return '' if vec_ptr.null? || vec_len.zero?");
        builder.line("vec_ptr.read_bytes(vec_len).force_encoding('UTF-8')");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // Button#on_click(data, fn_or_block) — smart instance method:
    // accepts any Ruby object as the data payload and a Proc / lambda
    // / block as the click handler. Wraps both via the host invoker.
    if s.name == "Button" {
        builder.line("# Smart builder: pass any Ruby object as the data payload and");
        builder.line("# a click-handler Proc/lambda/block. Returns a new Button with");
        builder.line("# the callback wired up. `with_on_click` already auto-registers");
        builder.line("# the callable, so we just wrap the data + delegate.");
        builder.line("def on_click(data, click_fn = nil, &block)");
        builder.indent();
        builder.line("fn = click_fn || block");
        builder.line("raise ArgumentError, 'click fn required' unless fn");
        builder.line("data_ref = Azul::RefAny.wrap(data)");
        builder.line("self.with_on_click(data_ref, fn)");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // WindowCreateOptions.create_with_layout(fn) — smart factory that
    // hides the host-invoker plumbing. Ruby FFI's nested-struct field
    // assignment uses the same memory (no JNA reference-swap quirk),
    // so we register the user's callable, fetch a `_default()` wco,
    // splice the AzLayoutCallback struct into the embedded
    // window_state.layout_callback, and return the wrapper. Existing
    // `create()` is left intact for the legacy non-host-invoker path.
    //
    // The codegen-emitted `create()` already auto-registers via
    // `Azul._register_callback('LayoutCallback', ...)` for the
    // function-pointer arg, but it then passes the raw fnptr to the
    // C-side `_create` which discards the ctx (the host-handle id) —
    // so callbacks fire but the user's Proc is never reached. This
    // helper fixes that by going through `_default` + struct splice.
    if s.name == "WindowCreateOptions" {
        builder.line("# Smart factory: pass a layout-callback Proc/lambda/block;");
        builder.line("# the host-invoker registration and struct-field splice happen");
        builder.line("# internally. Replaces the manual register_callback +");
        builder.line("# `_default` + field-assign dance.");
        builder.line("def self.create_with_layout(layout_fn = nil, &block)");
        builder.indent();
        builder.line("fn = layout_fn || block");
        builder.line("raise ArgumentError, 'layout fn required' unless fn");
        builder.line("cb_struct = Azul._register_callback('LayoutCallback', fn)");
        builder.line("wco = Native.az_window_create_options_default()");
        builder.line("# Splice the AzLayoutCallback into the embedded slot.");
        builder.line("ws = Native::AzFullWindowState.new(wco[:window_state].to_ptr)");
        builder.line("ws[:layout_callback] = cb_struct");
        builder.line("new(wco)");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // Emit each function on this class as a Ruby method.
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

    // Phase I.2.5: route ==/eql?/hash through the codegen-emitted
    // C-ABI helpers when TypeTraits flags them and the symbol exists.
    emit_rb_eq_hash_if_supported(builder, s, ir);

    // Phase I.3.3 (Ruby): override to_s + inspect through
    // Az<X>_toDbgString when TypeTraits.is_debug.
    emit_rb_to_s_if_supported(builder, s, ir);

    builder.dedent();
    builder.line(&format!("end # class {}", class_name));
}

/// Phase I.3.3 (Ruby): override `to_s` + `inspect` routed through
/// `Az<X>_toDbgString` when TypeTraits.is_debug + helper exists. Skips
/// AzString (its existing `to_s` vec-direct decoder).
fn emit_rb_to_s_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if s.name == "String" {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let snake = snake_case(&s.name);
    builder.line(&format!("# String repr routed through {}.", dbg_sym));
    builder.line("def to_s");
    builder.indent();
    builder.line("return '' if @ptr.nil?");
    builder.line(&format!(
        "az_str = Native.az_{}_to_dbg_string(@ptr)",
        snake
    ));
    // az_str is an AzString::ByValue FFI::Struct. Decode via vec.ptr/.len.
    builder.line("vec_ptr = az_str[:vec][:ptr]");
    builder.line("vec_len = az_str[:vec][:len]");
    builder.line("return '' if vec_ptr.null? || vec_len.zero?");
    builder.line("out = vec_ptr.read_bytes(vec_len).force_encoding('UTF-8')");
    // Free the AzString via the FFI struct's address.
    builder.line("Native.az_string_delete(FFI::Pointer.new(az_str.to_ptr.address))");
    builder.line("out");
    builder.dedent();
    builder.line("end");
    builder.line("alias_method :inspect, :to_s");
    builder.blank();
}

/// Phase I.2.5 (Ruby): override `==` / `eql?` / `hash` routed through
/// the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash` exports.
fn emit_rb_eq_hash_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash
        && ir.functions.iter().any(|f| f.c_name == hash_sym);
    let snake = snake_case(&s.name);

    if has_eq {
        builder.line(&format!("# Equality routed through {}.", eq_sym));
        builder.line("def ==(other)");
        builder.indent();
        builder.line("return false unless other.is_a?(self.class)");
        builder.line("return @ptr == other.ptr if @ptr.nil? || other.ptr.nil?");
        builder.line(&format!(
            "Native.az_{}_partial_eq(@ptr, other.ptr)",
            snake
        ));
        builder.dedent();
        builder.line("end");
        builder.line("alias_method :eql?, :==");
        builder.blank();
    }

    if has_hash {
        builder.line(&format!("# Hash routed through {}.", hash_sym));
        builder.line("def hash");
        builder.indent();
        builder.line("return 0 if @ptr.nil?");
        builder.line(&format!("Native.az_{}_hash(@ptr)", snake));
        builder.dedent();
        builder.line("end");
        builder.blank();
    } else if has_eq {
        // == / hash contract: equal values must hash equal. Fall back
        // to pointer-address hash.
        builder.line("def hash");
        builder.indent();
        builder.line("@ptr.nil? ? 0 : @ptr.address.hash");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }
}

/// Should this function be exposed as a Ruby method on the wrapper?
///
/// We hide all auto-generated trait functions (`_delete`, `_partialEq`,
/// `_hash`, ...) — they're either driven by the runtime (delete via
/// finalizer) or surfaced via custom Ruby methods (`==`, `hash`) which we
/// don't autogenerate today.
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
    let ruby_method = ruby_method_name(&func.method_name);
    let native_call = native_function_name(&func.c_name);

    let takes_self = matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut);
    // `return_type` is the unprefixed IR name (e.g. "App"); `prefixed` is
    // "AzApp". Strip the leading prefix off `prefixed` for the comparison.
    let owning_class = prefixed
        .strip_prefix("Az")
        .unwrap_or(prefixed)
        .to_string();
    let returns_self_type = func
        .return_type
        .as_deref()
        .map(|t| t.trim() == owning_class)
        .unwrap_or(false);

    // Strip an explicit `self` from the visible argument list (Ruby
    // supplies it via `@ptr`).
    let visible_args: Vec<&_> = func
        .args
        .iter()
        .filter(|a| {
            // The api.json/ir convention is to name the receiver after the
            // class (lower-cased). Skip both that and a literal "self".
            a.name != "self" && a.name != type_snake
        })
        .collect();

    let arg_names: Vec<String> = visible_args.iter().map(|a| ruby_arg_name(&a.name)).collect();

    // Names of args we should mark consumed after the C call: owned-by-
    // value wrapper-class instances. The C side moves them; the
    // wrapper's `ObjectSpace` finalizer must not fire on the now-
    // transferred memory. Callback args are skipped — they've already
    // been replaced by FFI::Struct values via `_register_callback` and
    // those FFI::Struct values are not wrappers themselves.
    let consumed_names: Vec<String> = visible_args
        .iter()
        .zip(arg_names.iter())
        .filter(|(a, _)| {
            a.callback_info.is_none()
                && matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
        })
        .map(|(_, n)| n.clone())
        .collect();

    if takes_self {
        // Instance method: forward `@ptr` as the receiver.
        if arg_names.is_empty() {
            builder.line(&format!("def {}", ruby_method));
        } else {
            builder.line(&format!("def {}({})", ruby_method, arg_names.join(", ")));
        }
        builder.indent();
        emit_callback_register_lines(builder, &visible_args, &arg_names);
        let mut call_args = vec!["@ptr".to_string()];
        for (i, name) in arg_names.iter().enumerate() {
            // Callback args have already been replaced by an FFI::Struct
            // value; pass them as-is. Other args go through _unwrap so
            // both wrapper instances and raw cdata are accepted.
            //
            // Auto-string-conversion: Owned `String` args go through
            // `Azul._az_string` (defined in managed.rs preamble) so
            // user code can pass plain Ruby strings directly. Pure
            // type-driven; no method-name allowlist.
            if visible_args[i].callback_info.is_some() {
                call_args.push(name.clone());
            } else if is_az_string_owned_arg(visible_args[i]) {
                call_args.push(format!("Azul._az_string({})", name));
            } else {
                call_args.push(unwrap_expr(name));
            }
        }
        let call = format!("Native.{}({})", native_call, call_args.join(", "));
        emit_method_body_instance(
            builder,
            &call,
            &func.return_type,
            returns_self_type,
            prefixed,
            &consumed_names,
        );
        builder.dedent();
        builder.line("end");
        builder.blank();
        return;
    }

    // Static method (constructor / static helper).
    if arg_names.is_empty() {
        builder.line(&format!("def self.{}", ruby_method));
    } else {
        builder.line(&format!(
            "def self.{}({})",
            ruby_method,
            arg_names.join(", ")
        ));
    }
    builder.indent();
    emit_callback_register_lines(builder, &visible_args, &arg_names);
    // For static calls we forward the user-supplied args. Callback args
    // are already wrapper structs (from `_register_callback`); Owned
    // `String` args go through `Azul._az_string` so user code can pass
    // plain Ruby strings directly. Other args go through `_unwrap` so
    // both wrapper instances and raw cdata are accepted.
    let call_args: Vec<String> = arg_names
        .iter()
        .zip(visible_args.iter())
        .map(|(n, a)| {
            if a.callback_info.is_some() {
                n.clone()
            } else if is_az_string_owned_arg(a) {
                format!("Azul._az_string({})", n)
            } else {
                unwrap_expr(n)
            }
        })
        .collect();
    let call = format!("Native.{}({})", native_call, call_args.join(", "));
    emit_method_body_static(
        builder,
        &call,
        &func.return_type,
        returns_self_type,
        prefixed,
        &consumed_names,
    );
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Emit `name = Azul._register_callback('Wrapper', name)` for every
/// callback-typed arg whose wrapper is in the host-invoker list. Hosts
/// pass plain Ruby callables (Proc / lambda / method); the helper
/// stashes them in `@_ruby_handles` and returns the `Az<Wrapper>`
/// FFI::Struct the C-ABI takes.
fn emit_callback_register_lines(
    builder: &mut CodeBuilder,
    args: &[&super::super::ir::FunctionArg],
    arg_names: &[String],
) {
    if !args.iter().any(|a| a.callback_info.is_some()) {
        return;
    }
    for (i, a) in args.iter().enumerate() {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        // Only kinds with `impl_managed_callback!` applied work via this
        // path. The shared `HOST_INVOKER_KINDS` allowlist drives every
        // managed-FFI adapter, so new kinds light up here automatically.
        if super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper) {
            builder.line(&format!(
                "{n} = Azul._register_callback('{w}', {n})",
                n = arg_names[i],
                w = wrapper
            ));
        }
    }
}

/// Emit the body of an instance method (`def foo ... end`). The
/// `consumed_names` are owned-by-value wrapper-typed args that the C
/// side took ownership of; we tag them as consumed after the call so
/// the wrapper's finalizer won't fire on transferred memory.
fn emit_method_body_instance(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
    prefixed: &str,
    consumed_names: &[String],
) {
    let _ = prefixed;
    match return_type {
        None => {
            builder.line(call);
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
        }
        Some(_) if returns_self_type => {
            // Consuming-builder: self is moved into the C call along
            // with any owned-by-value wrapper args. The returned value
            // is a fresh struct; wrap in a new instance, and mark all
            // moved-from wrappers (self + args) as consumed.
            builder.line(&format!("_next = {}", call));
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            builder.line("begin");
            builder.indent();
            builder.line("ObjectSpace.undefine_finalizer(self)");
            builder.dedent();
            builder.line("rescue StandardError");
            builder.line("end");
            builder.line("@ptr = nil");
            builder.line("self.class.new(_next)");
        }
        Some(_) => {
            if consumed_names.is_empty() {
                builder.line(call);
            } else {
                builder.line(&format!("_ret = {}", call));
                for n in consumed_names {
                    builder.line(&format!("Azul._consume({})", n));
                }
                builder.line("_ret");
            }
        }
    }
}

/// Emit the body of a class method (`def self.foo ... end`). Same
/// rules as the instance variant, minus the self-consume step.
fn emit_method_body_static(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
    prefixed: &str,
    consumed_names: &[String],
) {
    let _ = prefixed;
    match return_type {
        None => {
            builder.line(call);
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
        }
        Some(_) if returns_self_type => {
            builder.line(&format!("_next = {}", call));
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            // `new(...)` resolves to the surrounding class in both
            // instance methods (`def foo`) and class methods (`def self.foo`).
            // `self.class.new(...)` is wrong in class methods because
            // there `self.class` is `Class`, not the wrapper class.
            builder.line("new(_next)");
        }
        Some(_) => {
            if consumed_names.is_empty() {
                builder.line(call);
            } else {
                builder.line(&format!("_ret = {}", call));
                for n in consumed_names {
                    builder.line(&format!("Azul._consume({})", n));
                }
                builder.line("_ret");
            }
        }
    }
}

/// Wrap a positional argument in a tiny `_unwrap` helper that accepts both
/// raw pointers/values and wrapper instances. We emit the inline form so
/// no module-level helper is required.
fn unwrap_expr(name: &str) -> String {
    format!("({n}.respond_to?(:ptr) ? {n}.ptr : {n})", n = name)
}

/// Auto-string-conversion rule (mirrors Java/Kotlin/C#): any Owned
/// `String` arg at the C ABI accepts a plain Ruby string at the wrapper
/// level. The call site routes the value through `Azul._az_string`
/// (emitted from `managed.rs`). Pure type-driven; no method-name
/// allowlist.
fn is_az_string_owned_arg(a: &super::super::ir::FunctionArg) -> bool {
    a.type_name.trim() == "String"
        && matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Ruby method names use snake_case. The IR's `method_name` is camelCase
/// (e.g. `addChild`) or already snake-ish — normalise either to snake.
fn ruby_method_name(method: &str) -> String {
    camel_to_snake(method)
}

/// Argument names from the IR are usually already snake_case; if they
/// aren't, normalise. Also append a trailing `_` if the name collides
/// with a Ruby keyword.
fn ruby_arg_name(name: &str) -> String {
    let snake = camel_to_snake(name);
    if RUBY_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

/// Convert a C-ABI symbol (`AzApp_create`, `AzDom_addChild`) into the
/// snake_case Ruby attach_function name (`az_app_create`, `az_dom_add_child`).
///
/// Mirrors `functions::ruby_attach_name`. Duplicated here to avoid a
/// cross-module dependency on a private helper.
fn native_function_name(c_name: &str) -> String {
    let mut out = String::with_capacity(c_name.len() + 4);
    let mut prev_was_lower = false;
    let mut prev_was_underscore = false;
    for (i, c) in c_name.chars().enumerate() {
        if c == '_' {
            out.push('_');
            prev_was_lower = false;
            prev_was_underscore = true;
            continue;
        }
        if c.is_ascii_uppercase() {
            if i != 0 && prev_was_lower && !prev_was_underscore {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            out.push(c);
            prev_was_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
        prev_was_underscore = false;
    }
    out
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

const RUBY_RESERVED: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until",
    "when", "while", "yield",
];

