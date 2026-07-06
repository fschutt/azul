//! Idiomatic D wrapper-struct emission.
//!
//! The raw `extern(C)` layer + the `Az`-stripped aliases (see
//! `functions.rs`) already give D users a working, C-style surface
//! (`Button_setOnClick(&button, ...)`). This module adds the *idiomatic*
//! layer that brings D to parity with the Zig / Go bindings: for every
//! heap-owning / method-bearing IR struct we emit a thin D `struct` that
//! embeds the raw FFI value and exposes real member functions, so users
//! can write
//!
//! ```d
//! auto app = App.create(data, AzAppConfig_create());
//! scope(exit) app.deinit();
//! app.run(window);
//! ```
//!
//! instead of threading `&`-addresses through free functions.
//!
//! # Shape (mirrors the Zig / Go wrappers)
//!
//! ```d
//! struct App {
//!     AzApp inner;
//!     private bool _consumed = false;   // set after a by-value consume
//!
//!     static App create(AzRefAny data, AzAppConfig config) {
//!         App self;
//!         self.inner = AzApp_create(data, config);
//!         return self;
//!     }
//!
//!     void run(AzWindowCreateOptions options) {
//!         AzApp_run(&this.inner, options);
//!     }
//!
//!     // Pair `App.create(...)` with `scope(exit) app.deinit();`.
//!     void deinit() {
//!         if (this._consumed) return;
//!         AzApp_delete(&this.inner);
//!     }
//! }
//! ```
//!
//! Conventions (identical to the Zig / Go backends):
//!
//! * The wrapper struct uses the **unprefixed** type name (`App`, not
//!   `AzApp`). The raw C type stays reachable as `AzApp`, the raw
//!   functions as `AzApp_*` / the `App_*` aliases.
//! * Heap-owning types (those with an `Az<T>_delete`) get an explicit
//!   `void deinit()`. We deliberately do NOT emit a D `~this()`
//!   destructor: D copies structs by value freely, and an automatic
//!   destructor would double-free the shared `inner`. Explicit
//!   `scope(exit) x.deinit();` is the safe idiom, matching Zig's
//!   `defer x.deinit();` and Go's `defer x.Close()`.
//! * Constructors / static factories become `static Self <name>(...)`.
//!   The api.json `new` method is renamed to `create` (`new` is a D
//!   keyword).
//! * Instance methods forward to the C function with `&this.inner`
//!   (pointer self) or `this.inner` (by-value self, detected from the
//!   first arg's ref-kind). A by-value consume flips `_consumed` so the
//!   later `deinit` skips the now-double-free `_delete`.
//! * Members are referenced through `this.` so a user parameter that
//!   happens to be named `inner` / `_consumed` can shadow harmlessly.
//! * Callback-wrapper arguments bind the RAW fn-pointer typedef variant,
//!   exactly like `functions.rs` — so a plain `extern(C)` function's
//!   address passes straight through, no host-invoker.
//!
//! # Skipped categories
//!
//! Same set as the Zig / Go host-side wrappers — Recursive / VecRef /
//! Boxed / GenericTemplate / DestructorOrClone / CallbackTypedef never
//! get a wrapper. Tagged-union variant constructors keep the raw layer
//! only (matching Go, whose wrapper layer is struct-only).

use std::collections::BTreeSet;

use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::super::managed_host_invoker::{
    callback_typedef_for, has_callback_wrapper_arg, is_callback_wrapper,
};
use super::super::config::CodegenConfig;
use super::{
    arg_type_for_ref_kind, ffi_type_name, map_type_to_d, sanitize_identifier,
};

/// Emit the idiomatic wrapper-struct section.
pub fn generate_wrappers(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic wrappers: heap-owning / method-bearing types as D structs with");
    b.line("// member functions. `App.create(...)` + `scope(exit) app.deinit();`. The raw");
    b.line("// Az* types and Az*_/ Az-stripped functions remain fully available.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    for s in &ir.structs {
        if !should_emit_struct_wrapper(s, ir, config) {
            continue;
        }
        emit_struct_wrapper(b, ir, s);
    }
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit_struct_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    match s.category {
        TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::Boxed
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef => return false,
        _ => {}
    }
    has_destructor(&s.name, ir) || has_useful_method(&s.name, ir)
}

fn has_destructor(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class_name && f.kind == FunctionKind::Delete)
}

fn has_useful_method(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions.iter().any(|f| {
        f.class_name == class_name
            && matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
                    | FunctionKind::StaticMethod
                    | FunctionKind::Default
                    | FunctionKind::DeepCopy
            )
    })
}

// ============================================================================
// Struct wrapper
// ============================================================================

fn emit_struct_wrapper(b: &mut CodeBuilder, ir: &CodegenIR, s: &StructDef) {
    let d_name = sanitize_identifier(&s.name);
    let ffi_name = ffi_type_name(&s.name);
    let has_delete = has_destructor(&s.name, ir);
    let self_arg_name = to_snake_case(&s.name);

    for d in &s.doc {
        b.line(&format!("// {}", d.replace('\n', " ").replace('\r', " ")));
    }

    b.line(&format!("struct {} {{", d_name));
    b.line(&format!("\t{} inner;", ffi_name));
    // By-value-consume sentinel: set after a C ABI call takes `inner` by
    // value (DeepCopy / consuming-self method). `deinit` then skips
    // `_delete` to avoid a double-free on the stale Rust-owned bytes.
    b.line("\tprivate bool _consumed = false;");
    b.line("");

    // Dedup member-function names. D supports overloading, but skipping
    // exact-name dups (matching the Zig backend) keeps the surface free of
    // ambiguous-overload errors from IR types that expose e.g. both `new`
    // and `create`.
    let mut seen: BTreeSet<String> = BTreeSet::new();

    // Static factories: Constructor / StaticMethod / Default.
    for f in ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                let name = sanitize_identifier(&idiomatic_method_name(&f.method_name));
                if !seen.insert(name.clone()) {
                    b.line(&format!(
                        "\t// SKIPPED duplicate `{}` (maps to same name; use raw {}).",
                        name, f.c_name
                    ));
                    continue;
                }
                emit_static_factory(b, ir, f, &d_name, &self_arg_name);
            }
            _ => {}
        }
    }

    // Instance methods: Method / MethodMut / DeepCopy.
    for f in ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy => {
                let is_clone = matches!(f.kind, FunctionKind::DeepCopy);
                let label = if is_clone {
                    "clone".to_string()
                } else {
                    idiomatic_method_name(&f.method_name)
                };
                let name = sanitize_identifier(&label);
                if !seen.insert(name.clone()) {
                    b.line(&format!(
                        "\t// SKIPPED duplicate `{}` (maps to same name; use raw {}).",
                        name, f.c_name
                    ));
                    continue;
                }
                emit_instance_method(b, ir, f, &d_name, &self_arg_name, is_clone);
            }
            _ => {}
        }
    }

    // Destructor.
    if has_delete {
        b.line("\t// Free the underlying native resources. Pair the factory with");
        b.line("\t// `scope(exit) x.deinit();`. Skipped when a prior by-value consume");
        b.line("\t// transferred ownership of `inner` to Rust (would double-free).");
        b.line("\tvoid deinit() {");
        b.line("\t\tif (this._consumed) return;");
        b.line(&format!("\t\t{}_delete(&this.inner);", ffi_name));
        b.line("\t}");
    }

    b.line("}");
    b.blank();
}

// ============================================================================
// Static factories
// ============================================================================

fn emit_static_factory(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    f: &FunctionDef,
    d_name: &str,
    self_arg_name: &str,
) {
    let name = sanitize_identifier(&idiomatic_method_name(&f.method_name));
    for d in &f.doc {
        b.line(&format!("\t// {}", d.replace('\n', " ").replace('\r', " ")));
    }

    let cb_mode = has_callback_wrapper_arg(f);
    let params = format_params(&f.args, self_arg_name, /* skip_self */ false, cb_mode, ir);
    let call_args = format_call_args(&f.args, self_arg_name, /* skip_self */ false);

    let returns_self = returns_self(f);
    let ret_ty = return_type(f, d_name, ir);

    b.line(&format!("\tstatic {} {}({}) {{", ret_ty, name, params));
    let call = format!("{}({})", f.c_name, call_args);
    if returns_self {
        b.line(&format!("\t\t{} self;", d_name));
        b.line(&format!("\t\tself.inner = {};", call));
        b.line("\t\treturn self;");
    } else if f.return_type.is_none() {
        b.line(&format!("\t\t{};", call));
    } else {
        b.line(&format!("\t\treturn {};", call));
    }
    b.line("\t}");
    b.line("");
}

// ============================================================================
// Instance methods
// ============================================================================

fn emit_instance_method(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    f: &FunctionDef,
    d_name: &str,
    self_arg_name: &str,
    is_clone: bool,
) {
    let name = if is_clone {
        "clone".to_string()
    } else {
        sanitize_identifier(&idiomatic_method_name(&f.method_name))
    };
    for d in &f.doc {
        b.line(&format!("\t// {}", d.replace('\n', " ").replace('\r', " ")));
    }

    let cb_mode = has_callback_wrapper_arg(f);
    let params = format_params(&f.args, self_arg_name, /* skip_self */ true, cb_mode, ir);
    let user_call_args = format_call_args(&f.args, self_arg_name, /* skip_self */ true);

    let returns_self = returns_self(f);
    let ret_ty = return_type(f, d_name, ir);

    // args[0] Owned => C takes self BY VALUE (AzFoo); Ref/Ptr => pointer.
    let self_by_value = f
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let self_expr = if self_by_value {
        "this.inner"
    } else {
        "&this.inner"
    };
    let call_args_full = if user_call_args.is_empty() {
        self_expr.to_string()
    } else {
        format!("{}, {}", self_expr, user_call_args)
    };
    let call = format!("{}({})", f.c_name, call_args_full);

    b.line(&format!("\t{} {}({}) {{", ret_ty, name, params));
    if returns_self {
        b.line(&format!("\t\t{} ret;", d_name));
        b.line(&format!("\t\tret.inner = {};", call));
        if self_by_value {
            b.line("\t\tthis._consumed = true;");
        }
        b.line("\t\treturn ret;");
    } else if f.return_type.is_none() {
        b.line(&format!("\t\t{};", call));
        if self_by_value {
            b.line("\t\tthis._consumed = true;");
        }
    } else {
        b.line(&format!("\t\tauto ret = {};", call));
        if self_by_value {
            b.line("\t\tthis._consumed = true;");
        }
        b.line("\t\treturn ret;");
    }
    b.line("\t}");
    b.line("");
}

// ============================================================================
// Helpers
// ============================================================================

fn returns_self(f: &FunctionDef) -> bool {
    f.return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false)
}

fn return_type(f: &FunctionDef, d_name: &str, ir: &CodegenIR) -> String {
    match (&f.return_type, returns_self(f)) {
        (None, _) => "void".to_string(),
        (Some(_), true) => d_name.to_string(),
        (Some(rt), false) => map_type_to_d(rt, ir),
    }
}

fn format_params(
    args: &[FunctionArg],
    self_arg_name: &str,
    skip_self: bool,
    cb_mode: bool,
    ir: &CodegenIR,
) -> String {
    let mut out = Vec::new();
    let iter: Box<dyn Iterator<Item = &FunctionArg>> = if skip_self && !args.is_empty() {
        Box::new(args.iter().skip(1))
    } else {
        Box::new(args.iter())
    };
    for a in iter {
        if is_self_arg(&a.name, self_arg_name) {
            continue;
        }
        let effective = if cb_mode && is_callback_wrapper(&a.type_name) {
            callback_typedef_for(a.type_name.trim())
        } else {
            a.type_name.clone()
        };
        let ty = arg_type_for_ref_kind(&effective, &a.ref_kind, ir);
        out.push(format!("{} {}", ty, sanitize_identifier(&a.name)));
    }
    out.join(", ")
}

fn format_call_args(args: &[FunctionArg], self_arg_name: &str, skip_self: bool) -> String {
    let mut out = Vec::new();
    let iter: Box<dyn Iterator<Item = &FunctionArg>> = if skip_self && !args.is_empty() {
        Box::new(args.iter().skip(1))
    } else {
        Box::new(args.iter())
    };
    for a in iter {
        if is_self_arg(&a.name, self_arg_name) {
            continue;
        }
        out.push(sanitize_identifier(&a.name));
    }
    out.join(", ")
}

fn is_self_arg(name: &str, self_arg_name: &str) -> bool {
    name == "self"
        || name == "&self"
        || name == "&mut self"
        || (!self_arg_name.is_empty() && name == self_arg_name)
}

/// PascalCase / camelCase -> snake_case, matching the IR builder's
/// self-arg renaming (`StyleTextView` -> `style_text_view`).
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = bytes[i - 1] as char;
                let next = bytes.get(i + 1).map(|&n| n as char).unwrap_or(' ');
                let prev_lower_or_digit = prev.is_ascii_lowercase() || prev.is_ascii_digit();
                let next_lower = next.is_ascii_lowercase();
                if prev_lower_or_digit || (prev.is_ascii_uppercase() && next_lower) {
                    out.push('_');
                }
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// api.json method name -> idiomatic D method name. `new` is a D keyword,
/// so it becomes `create`.
fn idiomatic_method_name(method_name: &str) -> String {
    match method_name {
        "new" => "create".to_string(),
        other => other.to_string(),
    }
}
