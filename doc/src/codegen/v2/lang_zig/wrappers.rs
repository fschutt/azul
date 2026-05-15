//! Idiomatic Zig wrapper-struct emission.
//!
//! For every IR struct that has a matching `Az<TypeName>_delete` C
//! function we emit:
//!
//! ```zig
//! pub const App = struct {
//!     inner: C.AzApp,
//!
//!     pub fn create(data: C.AzRefAny, config: C.AzAppConfig) App {
//!         return .{ .inner = C.AzApp_create(data, config) };
//!     }
//!
//!     pub fn run(self: *App, options: C.AzWindowCreateOptions) void {
//!         C.AzApp_run(&self.inner, options);
//!     }
//!
//!     pub fn deinit(self: *App) void {
//!         C.AzApp_delete(&self.inner);
//!     }
//! };
//! ```
//!
//! Conventions:
//!
//! * The wrapper struct uses the **unprefixed** type name (`App`, not
//!   `AzApp`). The raw C type stays reachable via `C.AzApp` for users
//!   who need it.
//! * Heap-owning types get `pub fn deinit(self: *Self) void`. Users
//!   write `defer thing.deinit();` at the call site (we don't insert
//!   `defer` inside the wrapper itself — that would be wrong).
//! * Constructors / static factories become `pub fn <name>(...) Self`.
//!   The api.json `"new"` method is renamed to `create` to align with
//!   common Zig idiom.
//! * Instance methods take `self: *Self` and call the C function with
//!   `&self.inner`. We don't try to distinguish `&self` from `&mut self`
//!   at the Zig level — every method takes a pointer because the C ABI
//!   does.
//! * Anything Zig can already see for free through `@cImport` (POD
//!   structs without `_delete`, plain enums, callback typedefs, etc.) is
//!   **not** re-emitted — the user accesses it as `C.AzWhatever`.
//!
//! # Skipped categories
//!
//! Same set as the other host-side bindings:
//!
//! * `TypeCategory::Recursive`        — would create infinite-size types.
//! * `TypeCategory::VecRef`           — raw slice pointers, internal.
//! * `TypeCategory::Boxed`            — internal heap wrappers.
//! * `TypeCategory::GenericTemplate`  — generic shells, not instantiable.
//! * `TypeCategory::DestructorOrClone`— internal callback typedefs.
//! * `TypeCategory::CallbackTypedef`  — raw fn-pointer typedefs (visible
//!   to users via `C.*`; no wrapper makes sense).
//!
//! Tagged-union enum payload accessors are intentionally NOT emitted as
//! Zig `union(enum)` shadow types: the C-side layout already matches
//! `extern union`, and `@cImport` exposes a perfectly usable view of it
//! under `C.AzWhatever`. Adding a parallel native union just creates
//! divergence risk. We instead emit a thin namespace per data-bearing
//! enum that exposes the C-ABI `_Tag_*` discriminator constants and
//! every variant constructor as `pub fn <variant>(...)`.

use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::{ffi_type_name, sanitize_identifier};

/// Generate the full wrapper section as a single Zig source string.
///
/// The output begins with a separator banner and ends with a trailing
/// newline so it inserts cleanly after the `@cImport` block.
pub fn generate_wrappers(ir: &CodegenIR) -> String {
    let mut out = String::new();

    out.push_str("// ============================================================================\n");
    out.push_str("// Idiomatic wrappers (heap-owning types with `deinit()`).\n");
    out.push_str("// ============================================================================\n");
    out.push('\n');

    for s in &ir.structs {
        if !should_emit_struct_wrapper(s, ir) {
            continue;
        }
        emit_struct_wrapper(&mut out, ir, s);
    }

    // Tagged-union helper namespaces (variant constructors + Tag table).
    out.push_str("\n// ============================================================================\n");
    out.push_str("// Tagged-union helpers (variant constructors + Tag discriminators).\n");
    out.push_str("// ============================================================================\n");
    out.push('\n');

    for e in &ir.enums {
        if !should_emit_enum_helper(e) {
            continue;
        }
        if !e.is_union {
            // Unit-only enums are already directly usable through `C.*`.
            continue;
        }
        emit_union_helper(&mut out, ir, e);
    }

    out
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit_struct_wrapper(s: &StructDef, ir: &CodegenIR) -> bool {
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
    // Only emit a wrapper for types that have *something* to wrap:
    // either a destructor or at least one non-trait method/constructor.
    has_destructor(&s.name, ir) || has_useful_method(&s.name, ir)
}

fn should_emit_enum_helper(e: &EnumDef) -> bool {
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(
        e.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::Boxed
            | TypeCategory::GenericTemplate
            | TypeCategory::DestructorOrClone
            | TypeCategory::CallbackTypedef
    )
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

fn emit_struct_wrapper(out: &mut String, ir: &CodegenIR, s: &StructDef) {
    let zig_name = sanitize_identifier(&s.name);
    let ffi_name = ffi_type_name(&s.name);
    let has_delete = has_destructor(&s.name, ir);

    if !s.doc.is_empty() {
        for d in &s.doc {
            out.push_str(&format!("/// {}\n", d));
        }
    }

    out.push_str(&format!("pub const {} = struct {{\n", zig_name));
    out.push_str(&format!("    inner: C.{},\n", ffi_name));
    // Consume-after-by-value sentinel: set true after a C ABI call
    // takes `self.inner` by value (DeepCopy / consuming-self
    // method). `deinit` then skips `_delete` to avoid double-free
    // on stale Rust-owned bytes. Defaults to false on every
    // wrapper-construction path. Mirrors the JVM/CLR `closed` flag
    // pattern landed in commit 62094b885.
    out.push_str("    consumed: bool = false,\n");
    out.push('\n');
    out.push_str("    const Self = @This();\n");
    out.push('\n');

    // The IR builder renames the implicit `self` arg to `to_snake_case(class_name)`
    // (e.g. method on `App` carries an arg named `app`; method on `StyleTextView`
    // carries an arg named `style_text_view`). We filter that out at the
    // wrapper level so users only see the user-supplied parameters.
    let self_arg_name = to_snake_case(&s.name);

    // Zig disallows duplicate struct members. Some IR types expose
    // BOTH a `new` and a `create` factory (e.g. `ColorU.new`, exposed
    // by the C ABI for back-compat); both map to Zig `create()` after
    // `idiomatic_method_name` runs. Skip dups; emit a comment so the
    // hidden function is at least documented.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Zig also disallows function parameters whose name shadows ANY
    // declaration in the containing scope, including sibling methods.
    // `from_millis(millis: u64)` shadows the `millis(self: *Self)`
    // method on the same struct. Precompute the set of Zig method
    // names this class will emit so the per-method param formatter
    // can rename colliding params with an `_arg` suffix.
    let mut emitted_method_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for f in ir.functions_for_class(&s.name) {
        let label = match f.kind {
            FunctionKind::DeepCopy => "clone".to_string(),
            FunctionKind::Delete => "deinit".to_string(),
            _ => idiomatic_method_name(&f.method_name),
        };
        emitted_method_names.insert(sanitize_identifier(&label));
    }

    // Constructors / static factories.
    for f in ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                let zig_method = sanitize_identifier(&idiomatic_method_name(&f.method_name));
                if !seen.insert(zig_method.clone()) {
                    out.push_str(&format!(
                        "    // SKIPPED: duplicate `pub fn {}` — IR carries another factory mapping to the same Zig method name (calls C.{}).\n",
                        zig_method, f.c_name
                    ));
                    continue;
                }
                emit_static_factory(out, f, &self_arg_name, &emitted_method_names);
            }
            _ => {}
        }
    }

    // Instance methods.
    for f in ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy => {
                let method_label = if matches!(f.kind, FunctionKind::DeepCopy) {
                    "clone".to_string()
                } else {
                    idiomatic_method_name(&f.method_name)
                };
                let zig_method = sanitize_identifier(&method_label);
                if !seen.insert(zig_method.clone()) {
                    out.push_str(&format!(
                        "    // SKIPPED: duplicate `pub fn {}` — IR carries another method mapping to the same Zig method name (calls C.{}).\n",
                        zig_method, f.c_name
                    ));
                    continue;
                }
                emit_instance_method(
                    out,
                    f,
                    &self_arg_name,
                    /* clone */ matches!(f.kind, FunctionKind::DeepCopy),
                    &emitted_method_names,
                );
            }
            _ => {}
        }
    }

    // Destructor.
    if has_delete {
        out.push_str("    /// Free the underlying native resources.\n");
        out.push_str("    /// Idiomatic Zig: pair `App.create(...)` with `defer app.deinit();`.\n");
        out.push_str("    /// Skipped when `self.consumed` is set — a previous DeepCopy /\n");
        out.push_str("    /// consuming-self call transferred ownership of `inner` to Rust\n");
        out.push_str("    /// and a follow-up `_delete` would double-free.\n");
        out.push_str("    pub fn deinit(self: *Self) void {\n");
        out.push_str("        if (self.consumed) return;\n");
        out.push_str(&format!("        C.{}_delete(&self.inner);\n", ffi_name));
        out.push_str("    }\n");
    }

    out.push_str("};\n\n");
}

// ============================================================================
// Static factories (constructors, static methods, default)
// ============================================================================

fn emit_static_factory(
    out: &mut String,
    f: &FunctionDef,
    self_arg_name: &str,
    reserved_names: &std::collections::HashSet<String>,
) {
    let method_name = idiomatic_method_name(&f.method_name);
    let safe_name = sanitize_identifier(&method_name);

    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("    /// {}\n", d));
        }
    }

    // Static factories shouldn't carry a self arg in practice, but filter
    // defensively in case the IR ever surfaces one.
    let params = format_params(&f.args, self_arg_name, /* skip_self */ false, reserved_names);
    let call_args = format_call_args(&f.args, self_arg_name, /* skip_self */ false, reserved_names);

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    let return_zig = match (&f.return_type, returns_self) {
        (None, _) => "void".to_string(),
        (Some(_), true) => "Self".to_string(),
        (Some(rt), false) => map_return_type(rt),
    };

    out.push_str(&format!(
        "    pub fn {}({}) {} {{\n",
        safe_name, params, return_zig
    ));

    // We always reach for the canonical C symbol via `f.c_name`. The IR
    // builder formats it as `Az<Class>_<lowerCamelMethod>`, which is the
    // exact name `@cImport` exposes under `C.*`.
    let call = format!("C.{}({})", f.c_name, call_args);

    if return_zig == "void" {
        out.push_str(&format!("        {};\n", call));
    } else if returns_self {
        out.push_str(&format!("        return Self{{ .inner = {} }};\n", call));
    } else {
        out.push_str(&format!("        return {};\n", call));
    }

    out.push_str("    }\n\n");
}

// ============================================================================
// Instance methods (Method, MethodMut, DeepCopy)
// ============================================================================

fn emit_instance_method(
    out: &mut String,
    f: &FunctionDef,
    self_arg_name: &str,
    clone: bool,
    reserved_names: &std::collections::HashSet<String>,
) {
    let method_name = if clone {
        "clone".to_string()
    } else {
        idiomatic_method_name(&f.method_name)
    };
    let safe_name = sanitize_identifier(&method_name);

    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("    /// {}\n", d));
        }
    }

    let params = format_params(&f.args, self_arg_name, /* skip_self */ true, reserved_names);
    let user_call_args = format_call_args(&f.args, self_arg_name, /* skip_self */ true, reserved_names);

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    let return_zig = match (&f.return_type, returns_self) {
        (None, _) => "void".to_string(),
        (Some(_), true) => "Self".to_string(),
        (Some(rt), false) => map_return_type(rt),
    };

    let self_param = "self: *Self";
    let full_params = if params.is_empty() {
        self_param.to_string()
    } else {
        format!("{}, {}", self_param, params)
    };

    out.push_str(&format!(
        "    pub fn {}({}) {} {{\n",
        safe_name, full_params, return_zig
    ));

    // Inspect args[0]: Owned ⇒ C ABI takes `self` by value
    // (`AzFoo`); Ref/Ptr ⇒ takes a pointer (`AzFoo*`). The C
    // declaration must match — passing `&self.inner` where a value
    // is expected produces a Zig type-checker error
    // ("expected AzFoo, found *AzFoo"). Same detection JVM/CLR/Pascal
    // wrappers use.
    let self_by_value = f
        .args
        .first()
        .map(|a| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
        .unwrap_or(false);
    let self_expr = if self_by_value {
        "self.inner"
    } else {
        "&self.inner"
    };
    let call_args_full = if user_call_args.is_empty() {
        self_expr.to_string()
    } else {
        format!("{}, {}", self_expr, user_call_args)
    };

    // Use the canonical C symbol from the IR (`Az<Class>_<lowerCamelMethod>`).
    let call = format!("C.{}({})", f.c_name, call_args_full);

    // Mark `self` consumed when the C ABI took it by value — the
    // sentinel is checked in `deinit` to skip the now-double-free
    // `_delete` call. Mirrors the JVM/CLR `__consume()` pattern.
    let consume_self_line = if self_by_value {
        "        self.consumed = true;\n"
    } else {
        ""
    };

    if return_zig == "void" {
        out.push_str(&format!("        {};\n", call));
        out.push_str(consume_self_line);
    } else if returns_self {
        out.push_str(&format!(
            "        const _ret = Self{{ .inner = {} }};\n",
            call
        ));
        out.push_str(consume_self_line);
        out.push_str("        return _ret;\n");
    } else {
        out.push_str(&format!("        const _ret = {};\n", call));
        out.push_str(consume_self_line);
        out.push_str("        return _ret;\n");
    }

    out.push_str("    }\n\n");
}

// ============================================================================
// Tagged-union helper
// ============================================================================

fn emit_union_helper(out: &mut String, ir: &CodegenIR, e: &EnumDef) {
    let zig_name = sanitize_identifier(&e.name);
    let ffi_name = ffi_type_name(&e.name);
    let self_arg_name = to_snake_case(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            out.push_str(&format!("/// {}\n", d));
        }
    }

    out.push_str(&format!("pub const {} = struct {{\n", zig_name));
    out.push_str(&format!("    /// The raw FFI tagged-union type, as exposed by `@cImport`.\n"));
    out.push_str(&format!("    pub const Raw = C.{};\n", ffi_name));
    out.push('\n');

    // Tag discriminator constants.
    out.push_str("    /// Discriminator constants for the underlying C tagged union.\n");
    out.push_str("    pub const Tag = struct {\n");
    for v in &e.variants {
        let safe = sanitize_identifier(&v.name);
        out.push_str(&format!(
            "        pub const {}: c_uint = C.{}_Tag_{};\n",
            safe, ffi_name, v.name
        ));
    }
    out.push_str("    };\n\n");

    // Variant constructors come from FunctionKind::EnumVariantConstructor /
    // Constructor / StaticMethod / Default.
    for f in ir.functions_for_class(&e.name) {
        match f.kind {
            FunctionKind::EnumVariantConstructor
            | FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default => {
                let method_name = idiomatic_method_name(&f.method_name);
                let safe = sanitize_identifier(&method_name);

                let empty_reserved: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let params = format_params(
                    &f.args,
                    &self_arg_name,
                    /* skip_self */ false,
                    &empty_reserved,
                );
                let call_args = format_call_args(
                    &f.args,
                    &self_arg_name,
                    /* skip_self */ false,
                    &empty_reserved,
                );

                let return_zig = match &f.return_type {
                    None => "void".to_string(),
                    Some(_) => format!("C.{}", ffi_name),
                };

                if !f.doc.is_empty() {
                    for d in &f.doc {
                        out.push_str(&format!("    /// {}\n", d));
                    }
                }
                out.push_str(&format!(
                    "    pub fn {}({}) {} {{\n",
                    safe, params, return_zig
                ));
                let call = format!("C.{}({})", f.c_name, call_args);
                if return_zig == "void" {
                    out.push_str(&format!("        {};\n", call));
                } else {
                    out.push_str(&format!("        return {};\n", call));
                }
                out.push_str("    }\n\n");
            }
            _ => {}
        }
    }

    out.push_str("};\n\n");
}

// ============================================================================
// Argument / type formatting
// ============================================================================

/// Format a function's arguments as a Zig parameter list
/// (no `self` parameter — that's prepended by the caller).
///
/// Argument types are referenced through `C.*` because Zig sees every
/// FFI type that way. We don't translate primitives because the C
/// header's `typedef`s already map them (e.g. `c_int`, `f32`).
///
/// `self_arg_name` is the snake_case class name the IR builder uses
/// for the implicit-self argument (see `IRBuilder::build_function_def`).
fn format_params(
    args: &[super::super::ir::FunctionArg],
    self_arg_name: &str,
    skip_self: bool,
    reserved_names: &std::collections::HashSet<String>,
) -> String {
    let mut out = Vec::new();
    // When skip_self is set this is an instance method — the first IR
    // arg IS the implicit self regardless of how api.json named it.
    // Skip args[0] unconditionally; same fix the JVM/.NET/Go wrappers
    // landed in earlier phases.
    let iter: Box<dyn Iterator<Item = &super::super::ir::FunctionArg>> =
        if skip_self && !args.is_empty() {
            Box::new(args.iter().skip(1))
        } else {
            Box::new(args.iter())
        };
    for a in iter {
        if is_self_arg(&a.name, self_arg_name) {
            continue;
        }
        out.push(format!(
            "{}: {}",
            renamed_param(&a.name, reserved_names),
            map_arg_type(&a.type_name, a.ref_kind)
        ));
    }
    out.join(", ")
}

/// Format a function's call-site arguments (just names, comma-separated).
fn format_call_args(
    args: &[super::super::ir::FunctionArg],
    self_arg_name: &str,
    skip_self: bool,
    reserved_names: &std::collections::HashSet<String>,
) -> String {
    let mut out = Vec::new();
    let iter: Box<dyn Iterator<Item = &super::super::ir::FunctionArg>> =
        if skip_self && !args.is_empty() {
            Box::new(args.iter().skip(1))
        } else {
            Box::new(args.iter())
        };
    for a in iter {
        if is_self_arg(&a.name, self_arg_name) {
            continue;
        }
        out.push(renamed_param(&a.name, reserved_names));
    }
    out.join(", ")
}

/// Sanitise + rename a parameter name so it (a) is a valid Zig
/// identifier and (b) doesn't shadow any sibling declaration on the
/// containing struct. Zig 0.16 forbids parameter names that match
/// any in-scope declaration ("function parameter shadows declaration
/// of 'X'"); we suffix `_arg` when the param name is in the set of
/// emitted method names for this class.
fn renamed_param(name: &str, reserved_names: &std::collections::HashSet<String>) -> String {
    let safe = sanitize_identifier(name);
    if reserved_names.contains(&safe) {
        format!("{}_arg", safe)
    } else {
        safe
    }
}

/// Match the IR builder's renaming of the `&self` parameter:
/// the literal name `self`, the snake_case class name (e.g. `app`,
/// `style_text_view`), or the `&self` / `&mut self` raw forms in case
/// they ever leak through.
fn is_self_arg(name: &str, self_arg_name: &str) -> bool {
    name == "self"
        || name == "&self"
        || name == "&mut self"
        || (!self_arg_name.is_empty() && name == self_arg_name)
}

/// PascalCase / camelCase → snake_case. Used to mirror the IR builder's
/// `to_snake_case` helper for class names (e.g. `StyleTextView` →
/// `style_text_view`).
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

/// Map an IR argument type to its Zig representation.
///
/// We always reach for `C.<TypeName>` for known FFI types because that's
/// the ground truth Zig sees from `@cImport`. Pointers are emitted as
/// many-pointers / single-pointers depending on the ref kind.
fn map_arg_type(type_name: &str, ref_kind: ArgRefKind) -> String {
    let trimmed = type_name.trim();

    // Pointer-as-suffix forms (the IR sometimes carries `*const T` /
    // `*mut T` directly in the type name string).
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return format!("*const {}", map_arg_type(rest, ArgRefKind::Owned));
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return format!("*{}", map_arg_type(rest, ArgRefKind::Owned));
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return format!("*{}", map_arg_type(rest, ArgRefKind::Owned));
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return format!("*const {}", map_arg_type(rest, ArgRefKind::Owned));
    }

    // Primitive types — emit them natively, not via C.* (Zig's primitive
    // names match the C-typedef names we'd otherwise reach for).
    if let Some(zig) = primitive_to_zig(trimmed) {
        return apply_ref_kind(zig.to_string(), ref_kind);
    }

    // Everything else is assumed to be an FFI type from `azul.h`.
    let base = format!("C.{}", ffi_type_name(trimmed));
    apply_ref_kind(base, ref_kind)
}

fn apply_ref_kind(base: String, ref_kind: ArgRefKind) -> String {
    match ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::Ptr => format!("*const {}", base),
        ArgRefKind::RefMut | ArgRefKind::PtrMut => format!("*{}", base),
    }
}

fn map_return_type(ty: &str) -> String {
    let trimmed = ty.trim();

    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return format!("*const {}", map_return_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return format!("*{}", map_return_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return format!("*{}", map_return_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return format!("*const {}", map_return_type(rest));
    }

    if let Some(zig) = primitive_to_zig(trimmed) {
        return zig.to_string();
    }
    format!("C.{}", ffi_type_name(trimmed))
}

/// Translate a Rust/IR primitive name to its Zig equivalent.
/// Returns `None` for non-primitives (caller routes those through `C.*`).
fn primitive_to_zig(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "bool",
        "u8" | "c_uchar" => "u8",
        "i8" | "c_char" => "i8",
        "u16" => "u16",
        "i16" => "i16",
        "u32" | "c_uint" => "u32",
        "i32" | "c_int" => "i32",
        "u64" => "u64",
        "i64" => "i64",
        "f32" => "f32",
        "f64" => "f64",
        "usize" => "usize",
        "isize" => "isize",
        "c_void" | "void" | "()" => "void",
        _ => return None,
    })
}

/// Convert an api.json method name to an idiomatic Zig method name.
///
/// * `new` → `create` (`new` is reserved for namespacing on Zig types).
/// * `default` stays as-is.
/// * Other camelCase / snake_case names are passed through verbatim;
///   Zig identifiers tolerate both styles.
fn idiomatic_method_name(method_name: &str) -> String {
    match method_name {
        "new" => "create".to_string(),
        other => other.to_string(),
    }
}
