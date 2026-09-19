//! Idiomatic Zig wrapper-struct emission.
//!
//! For every IR struct that has a matching `Az<TypeName>_delete` C
//! function (or at least one useful method) we emit:
//!
//! ```zig
//! pub const App = struct {
//!     inner: C.AzApp,
//!     consumed: bool = false,
//!
//!     pub fn create(initial_data: anytype, app_config: anytype) App {
//!         return App{ .inner = C.AzApp_create(_asRefAny(initial_data), _asOwned(C.AzAppConfig, app_config, C.AzAppConfig_clone)) };
//!     }
//!
//!     pub fn run(self: *App, root_window: anytype) void {
//!         C.AzApp_run(&self.inner, _asOwned(C.AzWindowCreateOptions, root_window, null));
//!     }
//!
//!     pub fn deinit(self: *App) void {
//!         if (self.consumed) return;
//!         C.AzApp_delete(&self.inner);
//!     }
//! };
//! ```
//!
//! Conventions:
//!
//! * The wrapper struct uses the **unprefixed** type name (`App`, not `AzApp`). The raw C type
//!   stays reachable via `C.AzApp` for users who need it.
//! * Method names are lowerCamel, taken from the C symbol's suffix (`AzDom_createBody` →
//!   `createBody`); api.json's `new` becomes `create`. One function,
//!   [`idiomatic_method_name`], decides this for every caller.
//! * Heap-owning types get `pub fn deinit(self: *Self) void`. Users write `defer thing.deinit();`
//!   at the call site (we don't insert `defer` inside the wrapper itself — that would be wrong).
//! * Instance methods take `self: *Self` and call the C function with `&self.inner` (or
//!   `self.inner` when the C ABI consumes the receiver by value; the wrapper is then flagged
//!   `consumed` so `deinit` does not double-free).
//! * Arguments whose IR type is a `String`, `RefAny`, `Option*`, `*Vec`, a callback typedef, a
//!   callback wrapper (`ButtonOnClickCallback`) or a type that itself has a wrapper struct are
//!   typed `anytype` and converted by the `_as*` helpers of the runtime prelude
//!   ([`super::ZIG_RUNTIME`]); the decision is keyed on the IR
//!   (`TypeCategory`, `callback_typedefs`, `is_callback_wrapper`, the wrapper set), not on
//!   names. Everything else keeps its `C.Az*` type.
//! * A method whose return type has a wrapper returns the wrapper (`Button.dom()` → `Dom`).
//! * Anything the user can already reach through `C.*` (POD structs without `_delete`, plain
//!   enums, callback typedefs, etc.) is **not** re-emitted.
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
//! * `TypeCategory::CallbackTypedef`  — raw fn-pointer typedefs (visible to users via `C.*`; no
//!   wrapper makes sense).
//!
//! Tagged-union enum payload accessors are intentionally NOT emitted as
//! Zig `union(enum)` shadow types: the C-side layout already matches
//! `extern union`, and the pre-translated `C.AzWhatever` is a perfectly
//! usable view of it. Adding a parallel native union just creates
//! divergence risk. We instead emit a thin namespace per data-bearing
//! enum that exposes the C-ABI `_Tag_*` discriminator constants and
//! every variant constructor as `pub fn <variant>(...)`.

use std::collections::HashSet;

use super::{
    super::{
        ir::{
            ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionArg, FunctionDef,
            FunctionKind, StructDef, TypeCategory,
        },
        managed_host_invoker::shadow_callback_typedef,
    },
    ffi_type_name, sanitize_identifier,
};

/// Per-run context: the IR plus the set of struct names that get a wrapper
/// struct. Computed once from `should_emit_struct_wrapper`, so the argument /
/// return mapping and the emission loop can never disagree about which types
/// have a `Wrapper{ .inner = ... }` form.
struct Ctx<'a> {
    ir: &'a CodegenIR,
    wrappers: HashSet<String>,
}

impl<'a> Ctx<'a> {
    fn new(ir: &'a CodegenIR) -> Self {
        let wrappers = ir
            .structs
            .iter()
            .filter(|s| should_emit_struct_wrapper(s, ir))
            .map(|s| s.name.clone())
            .collect();
        Self { ir, wrappers }
    }

    fn has_wrapper(&self, ty: &str) -> bool {
        self.wrappers.contains(ty)
    }

    fn category_of(&self, ty: &str) -> Option<TypeCategory> {
        self.ir
            .find_struct(ty)
            .map(|s| s.category)
            .or_else(|| self.ir.find_enum(ty).map(|e| e.category))
    }

    fn is_callback_typedef(&self, ty: &str) -> bool {
        self.ir.callback_typedefs.iter().any(|c| c.name == ty)
    }

    /// Does the IR export `Az<class>_<suffix>`?
    fn has_c_fn(&self, class: &str, suffix: &str) -> bool {
        let want = format!("{}_{}", ffi_type_name(class), suffix);
        self.ir.functions_for_class(class).any(|f| f.c_name == want)
    }

    /// `C.Az<T>_clone` when the IR exports a `Clone` for `T`, else the Zig literal
    /// `null` (the runtime helpers then reject pointer inputs at compile time).
    fn clone_fn(&self, ty: &str) -> String {
        if self
            .ir
            .functions_for_class(ty)
            .any(|f| f.kind == FunctionKind::DeepCopy)
        {
            format!("C.{}_clone", ffi_type_name(ty))
        } else {
            "null".to_string()
        }
    }
}

/// Generate the full wrapper section as a single Zig source string.
///
/// The output begins with a separator banner and ends with a trailing
/// newline so it inserts cleanly after the runtime prelude.
pub fn generate_wrappers(ir: &CodegenIR) -> String {
    let ctx = Ctx::new(ir);
    let mut out = String::new();

    out.push_str(
        "// ============================================================================\n",
    );
    out.push_str("// Idiomatic wrappers (heap-owning types with `deinit()`).\n");
    out.push_str(
        "// ============================================================================\n",
    );
    out.push('\n');

    for s in &ir.structs {
        if !ctx.has_wrapper(&s.name) {
            continue;
        }
        emit_struct_wrapper(&mut out, &ctx, s);
    }

    // Tagged-union helper namespaces (variant constructors + Tag table).
    out.push_str(
        "\n// ============================================================================\n",
    );
    out.push_str("// Tagged-union helpers (variant constructors + Tag discriminators).\n");
    out.push_str(
        "// ============================================================================\n",
    );
    out.push('\n');

    for e in &ir.enums {
        if !should_emit_enum_helper(e) {
            continue;
        }
        if !e.is_union {
            // The VALUE of a unit-only enum is usable straight from `C.*`, so
            // it needs no constructors - but its trait entry points still had
            // nowhere to hang, which is the whole of zig's remaining gap.
            // Emit a namespace carrying just those.
            emit_unit_enum_helper(&mut out, &ctx, e);
            continue;
        }
        emit_union_helper(&mut out, &ctx, e);
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
        | TypeCategory::CallbackTypedef => {
            // ... unless the class declares capabilities. Then the wrapper
            // exists solely to name them, and carries the whole list.
            if !has_declared_capability(&s.name, ir) {
                return false;
            }
        }
        _ => {}
    }
    // Only emit a wrapper for types that have *something* to wrap:
    // either a destructor or at least one non-trait method/constructor.
    has_destructor(&s.name, ir) || has_useful_method(&s.name, ir)
}

/// Does this class declare capabilities the C ABI exports for it?
///
/// Used to admit a class whose CATEGORY would otherwise exclude it. The
/// category exclusions above are about a type's ordinary methods - a `Boxed`
/// heap wrapper or a raw callback typedef cannot be handed around by value -
/// and that reasoning does not reach `Az{T}_partialEq(a, b) -> bool` or
/// `Az{T}_toDbgString(ptr) -> AzString`, which take pointers and return
/// scalars. Same argument as the recursive-type carve-out in every
/// `should_emit_function`.
fn has_declared_capability(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions_for_class(class_name)
        .any(|f| f.kind.is_declared_capability())
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
                    // The auto-generated trait entry points count as useful.
                    // A class whose only exports are `_partialEq` / `_cmp` /
                    // `_hash` / `_toDbgString` still has something a caller
                    // wants; excluding them here gave such a class no wrapper
                    // at all, and a C symbol no wrapper calls cannot be named
                    // idiomatically from Zig. That is what left 5168 declared
                    // derives unreachable. Emitted by `emit_trait_methods`,
                    // which must stay in step with this list.
                    | FunctionKind::PartialEq
                    | FunctionKind::PartialCmp
                    | FunctionKind::Cmp
                    | FunctionKind::Hash
                    | FunctionKind::DebugToString
            )
    })
}

// ============================================================================
// Struct wrapper
// ============================================================================

fn emit_struct_wrapper(out: &mut String, ctx: &Ctx, s: &StructDef) {
    let zig_name = sanitize_identifier(&s.name);
    let ffi_name = ffi_type_name(&s.name);
    let has_delete = has_destructor(&s.name, ctx.ir);

    if !s.doc.is_empty() {
        for d in &s.doc {
            out.push_str(&format!("/// {}\n", d));
        }
    }

    out.push_str(&format!("pub const {} = struct {{\n", zig_name));
    out.push_str(&format!("    inner: C.{},\n", ffi_name));
    // Consume-after-by-value sentinel: set true after a C ABI call
    // takes `self.inner` by value (DeepCopy / consuming-self method) or
    // after `_asOwned` moved `inner` out through a `*Self` pointer.
    // `deinit` then skips `_delete` to avoid double-free on stale
    // Rust-owned bytes. Defaults to false on every wrapper-construction
    // path. Mirrors the JVM/CLR `closed` flag pattern landed in commit
    // 62094b885.
    out.push_str("    consumed: bool = false,\n");
    out.push('\n');
    out.push_str("    const Self = @This();\n");
    out.push('\n');

    // Zig disallows duplicate struct members. Some IR types expose
    // BOTH a `new` and a `create` factory (e.g. `ColorU.new`, exposed
    // by the C ABI for back-compat); both map to Zig `create()` after
    // `idiomatic_method_name` runs. Skip dups; emit a comment so the
    // hidden function is at least documented.
    let mut seen: HashSet<String> = HashSet::new();

    // Zig also disallows function parameters whose name shadows ANY
    // declaration in the containing scope, including sibling methods.
    // `fromMillis(millis: u64)` shadows the `millis(self: *Self)`
    // method on the same struct. Precompute the set of Zig method
    // names this class will emit so the per-method param formatter
    // can rename colliding params with an `_arg` suffix.
    let emitted_method_names: HashSet<String> = ctx
        .ir
        .functions_for_class(&s.name)
        .map(|f| sanitize_identifier(&idiomatic_method_name(f)))
        .collect();

    // Constructors / static factories.
    for f in ctx.ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                let zig_method = sanitize_identifier(&idiomatic_method_name(f));
                if !seen.insert(zig_method.clone()) {
                    out.push_str(&format!(
                        "    // SKIPPED: duplicate `pub fn {}` — IR carries another factory \
                         mapping to the same Zig method name (calls C.{}).\n",
                        zig_method, f.c_name
                    ));
                    continue;
                }
                emit_static_factory(out, ctx, f, &emitted_method_names);
            }
            _ => {}
        }
    }

    // Instance methods.
    for f in ctx.ir.functions_for_class(&s.name) {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy => {
                let zig_method = sanitize_identifier(&idiomatic_method_name(f));
                if !seen.insert(zig_method.clone()) {
                    out.push_str(&format!(
                        "    // SKIPPED: duplicate `pub fn {}` — IR carries another method \
                         mapping to the same Zig method name (calls C.{}).\n",
                        zig_method, f.c_name
                    ));
                    continue;
                }
                emit_instance_method(out, ctx, f, &emitted_method_names);
            }
            _ => {}
        }
    }

    // Trait entry points (PartialEq / PartialCmp / Cmp / Hash / Debug).
    emit_trait_methods(out, &s.name, &ffi_name, ctx.ir, &mut seen);

    // Destructor.
    if has_delete {
        out.push_str("    /// Free the underlying native resources.\n");
        out.push_str("    /// Idiomatic Zig: pair `App.create(...)` with `defer app.deinit();`.\n");
        out.push_str("    /// Skipped when `self.consumed` is set — a previous DeepCopy /\n");
        out.push_str("    /// consuming-self call (or `_asOwned` through a `*Self`) transferred\n");
        out.push_str("    /// ownership of `inner` to Rust and a follow-up `_delete` would double-free.\n");
        out.push_str("    pub fn deinit(self: *Self) void {\n");
        out.push_str("        if (self.consumed) return;\n");
        out.push_str(&format!("        C.{}_delete(&self.inner);\n", ffi_name));
        out.push_str("    }\n");
    }

    out.push_str("};\n\n");
}

// ============================================================================
// Unit-only enum namespace
// ============================================================================

/// A namespace for a unit-only enum, carrying only its trait entry points.
///
/// The pre-translated C layer already gives the value itself
/// (`C.AzAccessibilityRole_Alert`), so this deliberately emits no
/// constructors and no `Tag` block - it exists so `_partialEq` / `_cmp` /
/// `_hash` / `_toDbgString`, which libazul exports for these types, can be
/// named from Zig at all. A type that declares none of them gets no
/// namespace rather than an empty one.
fn emit_unit_enum_helper(out: &mut String, ctx: &Ctx, e: &EnumDef) {
    let has_traits = ctx.ir.functions_for_class(&e.name).any(|f| {
        matches!(
            f.kind,
            FunctionKind::PartialEq
                | FunctionKind::PartialCmp
                | FunctionKind::Cmp
                | FunctionKind::Hash
                | FunctionKind::DebugToString
                | FunctionKind::DeepCopy
                | FunctionKind::Default
        )
    });
    if !has_traits {
        return;
    }

    let zig_name = sanitize_identifier(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            out.push_str(&format!("/// {}\n", d));
        }
    }
    out.push_str(&format!("pub const {} = enum(C.{}) {{\n", zig_name, ffi_name));
    for v in &e.variants {
        out.push_str(&format!(
            "    {} = C.{}_{},\n",
            sanitize_identifier(&v.name),
            ffi_name,
            v.name
        ));
    }
    out.push_str("    _,\n\n");
    out.push_str(&format!("    pub const Raw = C.{};\n\n", ffi_name));
    emit_trait_methods_raw(out, &e.name, &ffi_name, ctx.ir);

    // `Default` is a STATIC factory, not an instance method, so it is not part
    // of `emit_trait_methods_raw` - the union helper already emits it through
    // its constructor loop and would duplicate. For a unit enum this namespace
    // is the only place it can live.
    if let Some(f) = ctx
        .ir
        .functions_for_class(&e.name)
        .find(|f| f.kind == FunctionKind::Default)
    {
        out.push_str("\n    /// The Rust `Default`.\n");
        out.push_str("    pub fn default() Raw {\n");
        out.push_str(&format!("        return C.{}();\n", f.c_name));
        out.push_str("    }\n");
    }
    out.push_str("};\n\n");
}

// ============================================================================
// Trait entry points
// ============================================================================

/// The enum flavour of [`emit_trait_methods`]: same entry points, but an enum
/// wrapper has no `inner` field, so these operate on `*const Raw`.
fn emit_trait_methods_raw(out: &mut String, class_name: &str, ffi_name: &str, ir: &CodegenIR) {
    // Zig forbids a parameter shadowing ANY declaration in the containing
    // scope, and these namespaces already declare one function per variant -
    // `NodeType` has `pub fn a()`, so a parameter named `a` is a hard error.
    // The file solves this for ordinary methods with `renamed_param`; this
    // does the same, against the names this namespace will emit.
    let reserved: HashSet<String> = ir
        .functions_for_class(class_name)
        .map(|f| sanitize_identifier(&idiomatic_method_name(f)))
        .collect();
    let pa = renamed_param("a", &reserved);
    let pb = renamed_param("b", &reserved);
    let pv = renamed_param("v", &reserved);
    let mut seen: HashSet<String> = HashSet::new();
    for f in ir.functions_for_class(class_name) {
        let name = idiomatic_method_name(f);
        let text = match f.kind {
            FunctionKind::PartialEq => format!(
                "    /// Structural equality, delegating to the Rust `PartialEq`.\n    pub fn \
                 {name}({pa}: *const Raw, {pb}: *const Raw) bool {{\n        return \
                 C.{ffi_name}_partialEq({pa}, {pb});\n    }}\n"
            ),
            FunctionKind::Cmp => format!(
                "    /// Total order, delegating to the Rust `Ord`.\n    \
                 /// The C ABI answers 0 = less, 1 = equal, 2 = greater.\n    \
                 pub fn {name}({pa}: *const Raw, {pb}: *const Raw) u8 {{\n        \
                 return C.{ffi_name}_cmp({pa}, {pb});\n    }}\n"
            ),
            FunctionKind::PartialCmp => format!(
                "    /// Partial order, delegating to the Rust `PartialOrd`.\n    /// Same \
                 encoding as `order`.\n    pub fn {name}({pa}: *const Raw, {pb}: *const \
                 Raw) u8 {{\n        return C.{ffi_name}_partialCmp({pa}, {pb});\n    }}\n"
            ),
            FunctionKind::Hash => format!(
                "    /// The Rust `Hash`, as a 64-bit digest.\n    pub fn {name}({pv}: *const \
                 Raw) u64 {{\n        return C.{ffi_name}_hash({pv});\n    }}\n"
            ),
            FunctionKind::DebugToString => format!(
                "    /// The Rust `{{:#?}}` rendering. The returned `AzString` owns its\n    \
                 /// buffer — free it with `C.AzString_delete` when done.\n    pub fn \
                 {name}({pv}: *const Raw) C.AzString {{\n        return \
                 C.{ffi_name}_toDbgString({pv});\n    }}\n"
            ),
            FunctionKind::DeepCopy => format!(
                "    /// A deep copy, delegating to the Rust `Clone`.\n    pub fn {name}({pv}: \
                 *const Raw) Raw {{\n        return C.{ffi_name}_clone({pv});\n    }}\n"
            ),
            _ => continue,
        };
        if !seen.insert(name) {
            continue;
        }
        out.push_str(&text);
    }
}

/// Give the auto-generated trait exports a Zig name.
///
/// The wrapper structs are the only idiomatic way to name a C function, so
/// a derive is reachable from Zig only if a wrapper method calls its entry
/// point, which is why this exists and why `has_useful_method` must admit
/// these kinds: otherwise the class gets no wrapper and the symbol, though
/// exported by libazul, can only be spelled through `C.*`.
///
/// Only kinds the class actually exports are emitted, so a type that declares
/// no `Hash` gets no `hash()`.
fn emit_trait_methods(
    out: &mut String,
    class_name: &str,
    ffi_name: &str,
    ir: &CodegenIR,
    seen: &mut HashSet<String>,
) {
    for f in ir.functions_for_class(class_name) {
        let name = idiomatic_method_name(f);
        let text = match f.kind {
            FunctionKind::PartialEq => format!(
                "    /// Structural equality, delegating to the Rust `PartialEq`.\n    pub fn \
                 {name}(self: *const Self, other: *const Self) bool {{\n        return \
                 C.{ffi_name}_partialEq(&self.inner, &other.inner);\n    }}\n"
            ),
            FunctionKind::Cmp => format!(
                "    /// Total order, delegating to the Rust `Ord`.\n    /// The C ABI \
                 answers 0 = less, 1 = equal, 2 = greater.\n    pub fn {name}(self: *const \
                 Self, other: *const Self) u8 {{\n        return \
                 C.{ffi_name}_cmp(&self.inner, &other.inner);\n    }}\n"
            ),
            FunctionKind::PartialCmp => format!(
                "    /// Partial order, delegating to the Rust `PartialOrd`.\n    /// Same \
                 encoding as `order`.\n    pub fn {name}(self: *const Self, other: \
                 *const Self) u8 {{\n        return C.{ffi_name}_partialCmp(&self.inner, \
                 &other.inner);\n    }}\n"
            ),
            FunctionKind::Hash => format!(
                "    /// The Rust `Hash`, as a 64-bit digest.\n    pub fn {name}(self: *const \
                 Self) u64 {{\n        return C.{ffi_name}_hash(&self.inner);\n    }}\n"
            ),
            FunctionKind::DebugToString => format!(
                "    /// The Rust `{{:#?}}` rendering. The returned `AzString` owns its\n    \
                 /// buffer — free it with `C.AzString_delete` when done.\n    pub fn \
                 {name}(self: *const Self) C.AzString {{\n        return \
                 C.{ffi_name}_toDbgString(&self.inner);\n    }}\n"
            ),
            _ => continue,
        };
        if !seen.insert(name.clone()) {
            out.push_str(&format!(
                "    // SKIPPED: `pub fn {name}` — the class already emits a method of that \
                 name.\n"
            ));
            continue;
        }
        out.push_str(&text);
    }
}

// ============================================================================
// Static factories (constructors, static methods, default)
// ============================================================================

fn emit_static_factory(
    out: &mut String,
    ctx: &Ctx,
    f: &FunctionDef,
    reserved_names: &HashSet<String>,
) {
    let safe_name = sanitize_identifier(&idiomatic_method_name(f));

    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("    /// {}\n", d));
        }
    }

    // Static factories shouldn't carry a self arg in practice, but filter
    // defensively in case the IR ever surfaces one.
    let params = format_params(ctx, f, /* skip_self */ false, reserved_names);
    let call_args = format_call_args(ctx, f, /* skip_self */ false, reserved_names);
    let ret = classify_return(ctx, f);

    out.push_str(&format!(
        "    pub fn {}({}) {} {{\n",
        safe_name,
        params,
        ret.zig_type()
    ));

    // We always reach for the canonical C symbol via `f.c_name`. The IR
    // builder formats it as `Az<Class>_<lowerCamelMethod>`, which is the
    // exact name the `C` namespace declares.
    let call = format!("C.{}({})", f.c_name, call_args);

    match ret {
        RetConv::Void => out.push_str(&format!("        {};\n", call)),
        _ => out.push_str(&format!("        return {};\n", ret.wrap(&call))),
    }

    out.push_str("    }\n\n");
}

// ============================================================================
// Instance methods (Method, MethodMut, DeepCopy)
// ============================================================================

fn emit_instance_method(
    out: &mut String,
    ctx: &Ctx,
    f: &FunctionDef,
    reserved_names: &HashSet<String>,
) {
    let safe_name = sanitize_identifier(&idiomatic_method_name(f));

    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("    /// {}\n", d));
        }
    }

    let params = format_params(ctx, f, /* skip_self */ true, reserved_names);
    let user_call_args = format_call_args(ctx, f, /* skip_self */ true, reserved_names);
    let ret = classify_return(ctx, f);

    let self_param = "self: *Self";
    let full_params = if params.is_empty() {
        self_param.to_string()
    } else {
        format!("{}, {}", self_param, params)
    };

    out.push_str(&format!(
        "    pub fn {}({}) {} {{\n",
        safe_name,
        full_params,
        ret.zig_type()
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
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
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

    match ret {
        RetConv::Void => {
            out.push_str(&format!("        {};\n", call));
            out.push_str(consume_self_line);
        }
        _ => {
            out.push_str(&format!("        const _ret = {};\n", ret.wrap(&call)));
            out.push_str(consume_self_line);
            out.push_str("        return _ret;\n");
        }
    }

    out.push_str("    }\n\n");
}

// ============================================================================
// Tagged-union helper
// ============================================================================

fn emit_union_helper(out: &mut String, ctx: &Ctx, e: &EnumDef) {
    let zig_name = sanitize_identifier(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            out.push_str(&format!("/// {}\n", d));
        }
    }

    out.push_str(&format!("pub const {} = struct {{\n", zig_name));
    out.push_str("    /// The raw FFI tagged-union type, as declared in `C`.\n");
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
    let empty_reserved: HashSet<String> = HashSet::new();
    for f in ctx.ir.functions_for_class(&e.name) {
        match f.kind {
            FunctionKind::EnumVariantConstructor
            | FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default => {
                let safe = sanitize_identifier(&idiomatic_method_name(f));
                let params = format_params(ctx, f, /* skip_self */ false, &empty_reserved);
                let call_args =
                    format_call_args(ctx, f, /* skip_self */ false, &empty_reserved);

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

    // Trait entry points. An enum wrapper holds no `inner` — its values are
    // raw C tagged unions — so these take `*const Raw` rather than `*Self`.
    // Without them every derive an enum declares was unreachable from Zig,
    // which is what the C++ emitter had to fix for the same reason.
    emit_trait_methods_raw(out, &e.name, &ffi_name, ctx.ir);
    out.push_str("};\n\n");
}

// ============================================================================
// Argument classification (keyed on the IR, not on type names)
// ============================================================================

/// How a wrapper parameter is declared and converted before the C call.
enum ArgConv {
    /// `_asAzString(x)`: Zig string shapes, `azul.String`, `C.AzString`.
    String,
    /// `_asRefAny(x)`: a model value, a `Ref`, `azul.RefAny`, `C.AzRefAny`.
    RefAny,
    /// `_asAzOption(x, spec)`: `?V`, `null`, `V`, `C.AzOptionX`.
    Option { ty: String, map: &'static str },
    /// `_asAzVec(x, spec)`: a slice/array of items, one item, `C.AzXVec`.
    Vec {
        ty: String,
        item: String,
        map: &'static str,
    },
    /// `_asCallback(C.Az<typedef>, x)`: a Zig `fn` or a C fn pointer.
    Callback { typedef: String },
    /// `_asOwned(C.Az<T>, x, clone)`: the C struct, its wrapper, or pointers to either.
    OwnedWrapper { ty: String },
    /// `_asConstPtr` / `_asMutPtr`: a pointer to the C struct or to its wrapper.
    PtrWrapper { ty: String, mutable: bool },
    /// A typed parameter, passed verbatim.
    Plain(String),
}

impl ArgConv {
    fn param_type(&self) -> &str {
        match self {
            ArgConv::Plain(t) => t,
            _ => "anytype",
        }
    }

    fn call_expr(&self, ctx: &Ctx, name: &str) -> String {
        match self {
            ArgConv::String => format!("_asAzString({name})"),
            ArgConv::RefAny => format!("_asRefAny({name})"),
            ArgConv::Option { ty, map } => {
                let t = ffi_type_name(ty);
                let clone = ctx.clone_fn(ty);
                format!(
                    "_asAzOption({name}, .{{ .T = C.{t}, .some = C.{t}_some, .none = C.{t}_none, \
                     .clone = {clone}, .map = {map} }})"
                )
            }
            ArgConv::Vec { ty, item, map } => {
                let t = ffi_type_name(ty);
                let clone = ctx.clone_fn(ty);
                format!(
                    "_asAzVec({name}, .{{ .T = C.{t}, .Item = {item}, .create = C.{t}_create, \
                     .copyFromPtr = C.{t}_copyFromPtr, .fromItem = C.{t}_fromItem, .clone = \
                     {clone}, .map = {map} }})"
                )
            }
            ArgConv::Callback { typedef } => {
                format!("_asCallback(C.{}, {name})", ffi_type_name(typedef))
            }
            ArgConv::OwnedWrapper { ty } => {
                format!(
                    "_asOwned(C.{}, {name}, {})",
                    ffi_type_name(ty),
                    ctx.clone_fn(ty)
                )
            }
            ArgConv::PtrWrapper { ty, mutable: false } => {
                format!("_asConstPtr(C.{}, {name})", ffi_type_name(ty))
            }
            ArgConv::PtrWrapper { ty, mutable: true } => {
                format!("_asMutPtr(C.{}, {name})", ffi_type_name(ty))
            }
            ArgConv::Plain(_) => name.to_string(),
        }
    }
}

/// The runtime helper that converts one Option payload / Vec item, by the IR
/// category of the inner type.
fn inner_map(ctx: &Ctx, inner: Option<&str>) -> &'static str {
    match inner.and_then(|t| ctx.category_of(t)) {
        Some(TypeCategory::String) => "_asAzString",
        Some(TypeCategory::RefAny) => "_asRefAny",
        _ => "_identity",
    }
}

/// The `Some(T)` payload type of an Option-shaped enum.
fn option_payload(ctx: &Ctx, ty: &str) -> Option<String> {
    let e = ctx.ir.find_enum(ty)?;
    let some = e.variants.iter().find(|v| v.name == "Some")?;
    match &some.kind {
        EnumVariantKind::Tuple(items) if items.len() == 1 => Some(items[0].0.clone()),
        _ => None,
    }
}

/// The element type of a Vec-shaped struct: the pointee of its `ptr` field.
fn vec_item_type(ctx: &Ctx, ty: &str) -> Option<String> {
    let s = ctx.ir.find_struct(ty)?;
    let ptr = s.fields.iter().find(|f| f.name == "ptr")?;
    let t = ptr.type_name.trim();
    let t = t
        .strip_prefix("*const ")
        .or_else(|| t.strip_prefix("*mut "))
        .unwrap_or(t);
    Some(t.to_string())
}

/// Decide the parameter shape for `a` of `f`.
///
/// Every branch is keyed on IR metadata: the struct/enum `TypeCategory`, the
/// presence of the `_some`/`_none` or `_create`/`_copyFromPtr`/`_fromItem`
/// exports, the `callback_typedefs` table, `shadow_callback_typedef` (which
/// is also what decides that the C symbol takes the raw fn pointer, see
/// `c_decls::emit_function`) and the wrapper set.
fn classify_arg(ctx: &Ctx, f: &FunctionDef, a: &FunctionArg) -> ArgConv {
    let ty = a.type_name.trim();

    // Pointer-as-prefix spellings and primitives are typed verbatim.
    if ty.starts_with('*') || ty.starts_with('&') || primitive_to_zig(ty).is_some() {
        return ArgConv::Plain(map_arg_type(ty, a.ref_kind));
    }

    if !matches!(a.ref_kind, ArgRefKind::Owned) {
        if ctx.has_wrapper(ty) {
            return ArgConv::PtrWrapper {
                ty: ty.to_string(),
                mutable: matches!(a.ref_kind, ArgRefKind::RefMut | ArgRefKind::PtrMut),
            };
        }
        return ArgConv::Plain(map_arg_type(ty, a.ref_kind));
    }

    match ctx.category_of(ty) {
        Some(TypeCategory::String) => return ArgConv::String,
        Some(TypeCategory::RefAny) => return ArgConv::RefAny,
        Some(TypeCategory::Option) if ctx.has_c_fn(ty, "some") && ctx.has_c_fn(ty, "none") => {
            return ArgConv::Option {
                ty: ty.to_string(),
                map: inner_map(ctx, option_payload(ctx, ty).as_deref()),
            };
        }
        Some(TypeCategory::Vec) => {
            if let Some(item) = vec_item_type(ctx, ty) {
                if ctx.has_c_fn(ty, "create")
                    && ctx.has_c_fn(ty, "copyFromPtr")
                    && ctx.has_c_fn(ty, "fromItem")
                {
                    return ArgConv::Vec {
                        ty: ty.to_string(),
                        item: zig_c_type(&item),
                        map: inner_map(ctx, Some(&item)),
                    };
                }
            }
        }
        _ => {}
    }

    // A raw fn-pointer typedef argument (`LayoutCallbackType`): the C symbol
    // takes it as declared. (`a.callback_info` is deliberately NOT the key: the
    // IR builder also fills it for wrapper-typed args, with the typedef under
    // `callback_typedef_name`; the table lookup is unambiguous.)
    if ctx.is_callback_typedef(ty) {
        return ArgConv::Callback {
            typedef: ty.to_string(),
        };
    }

    // A callback-wrapper struct argument of an API function: libazul exports
    // the raw form `Az<Class>_<method>(..., cb: Az<Kind>CallbackType)` for it
    // (the same predicate `c_decls::emit_function` uses), so the wrapper takes
    // a Zig `fn`. For any other function kind the C symbol takes the struct.
    if f.kind.is_api_function() {
        if let Some(typedef) = shadow_callback_typedef(f, a) {
            return ArgConv::Callback {
                typedef: typedef.to_string(),
            };
        }
    }

    if ctx.has_wrapper(ty) {
        return ArgConv::OwnedWrapper { ty: ty.to_string() };
    }

    ArgConv::Plain(map_arg_type(ty, a.ref_kind))
}

/// The user-visible arguments of `f`: the receiver (`self` / the snake_case
/// class name the IR builder synthesises) is dropped, and for instance
/// methods `args[0]` is the receiver regardless of how api.json named it.
fn user_args<'a>(f: &'a FunctionDef, skip_self: bool) -> impl Iterator<Item = &'a FunctionArg> {
    f.args
        .iter()
        .enumerate()
        .filter(move |(i, a)| !(skip_self && *i == 0) && !f.is_receiver_arg(a))
        .map(|(_, a)| a)
}

/// Format a function's arguments as a Zig parameter list
/// (no `self` parameter — that's prepended by the caller).
fn format_params(
    ctx: &Ctx,
    f: &FunctionDef,
    skip_self: bool,
    reserved_names: &HashSet<String>,
) -> String {
    user_args(f, skip_self)
        .map(|a| {
            format!(
                "{}: {}",
                renamed_param(&a.name, reserved_names),
                classify_arg(ctx, f, a).param_type()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a function's call-site arguments (converted, comma-separated).
fn format_call_args(
    ctx: &Ctx,
    f: &FunctionDef,
    skip_self: bool,
    reserved_names: &HashSet<String>,
) -> String {
    user_args(f, skip_self)
        .map(|a| classify_arg(ctx, f, a).call_expr(ctx, &renamed_param(&a.name, reserved_names)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Sanitise + rename a parameter name so it (a) is a valid Zig
/// identifier and (b) doesn't shadow any sibling declaration on the
/// containing struct. Zig 0.16 forbids parameter names that match
/// any in-scope declaration ("function parameter shadows declaration
/// of 'X'"); we suffix `_arg` when the param name is in the set of
/// emitted method names for this class.
fn renamed_param(name: &str, reserved_names: &HashSet<String>) -> String {
    let safe = sanitize_identifier(name);
    if reserved_names.contains(&safe) {
        format!("{}_arg", safe)
    } else {
        safe
    }
}

// ============================================================================
// Type formatting
// ============================================================================

/// The `C.*` spelling of an IR type (pointer prefixes honoured, primitives native).
fn zig_c_type(type_name: &str) -> String {
    let trimmed = type_name.trim();
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return format!("*const {}", zig_c_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return format!("*{}", zig_c_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return format!("*{}", zig_c_type(rest));
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return format!("*const {}", zig_c_type(rest));
    }
    if let Some(zig) = primitive_to_zig(trimmed) {
        return zig.to_string();
    }
    format!("C.{}", ffi_type_name(trimmed))
}

/// Map an IR argument type to its typed Zig parameter spelling.
///
/// Pointer-as-prefix forms (`*const T`, `&mut T`) already carry their
/// indirection; everything else gets it from `ref_kind`.
fn map_arg_type(type_name: &str, ref_kind: ArgRefKind) -> String {
    let trimmed = type_name.trim();
    if trimmed.starts_with('*') || trimmed.starts_with('&') {
        return zig_c_type(trimmed);
    }
    apply_ref_kind(zig_c_type(trimmed), ref_kind)
}

fn apply_ref_kind(base: String, ref_kind: ArgRefKind) -> String {
    match ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::Ptr => format!("*const {}", base),
        ArgRefKind::RefMut | ArgRefKind::PtrMut => format!("*{}", base),
    }
}

/// How a method's return value is typed and (un)wrapped.
enum RetConv {
    Void,
    /// The class's own wrapper (`Self`).
    SelfWrapper,
    /// Another type that has a wrapper struct: `Wrapper{ .inner = call }`.
    Wrapper(String),
    /// Typed verbatim (`C.Az*`, primitives, pointers).
    Plain(String),
}

impl RetConv {
    fn zig_type(&self) -> &str {
        match self {
            RetConv::Void => "void",
            RetConv::SelfWrapper => "Self",
            RetConv::Wrapper(w) => w,
            RetConv::Plain(t) => t,
        }
    }

    fn wrap(&self, call: &str) -> String {
        match self {
            RetConv::Void | RetConv::Plain(_) => call.to_string(),
            RetConv::SelfWrapper => format!("Self{{ .inner = {call} }}"),
            RetConv::Wrapper(w) => format!("{w}{{ .inner = {call} }}"),
        }
    }
}

fn classify_return(ctx: &Ctx, f: &FunctionDef) -> RetConv {
    let Some(rt) = f.return_type.as_deref().map(str::trim) else {
        return RetConv::Void;
    };
    if rt == f.class_name {
        return RetConv::SelfWrapper;
    }
    if ctx.has_wrapper(rt) {
        return RetConv::Wrapper(sanitize_identifier(rt));
    }
    RetConv::Plain(zig_c_type(rt))
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

// ============================================================================
// Naming
// ============================================================================

/// The Zig method name of an IR function — the ONE place that decides it.
///
/// * Trait exports get their Zig-conventional names (`deinit`, `clone`, `eql`,
///   `order`, `partialOrder`, `hash`, `toDbgString`).
/// * Everything else takes the lowerCamel suffix of the C symbol the IR
///   builder already formatted (`AzDom_createBody` → `createBody`,
///   `AzButton_setOnClick` → `setOnClick`); api.json's `new` becomes `create`
///   (`new` is reserved for namespacing on Zig types).
pub fn idiomatic_method_name(f: &FunctionDef) -> String {
    match f.kind {
        FunctionKind::Delete => "deinit".to_string(),
        FunctionKind::DeepCopy => "clone".to_string(),
        FunctionKind::PartialEq => "eql".to_string(),
        FunctionKind::Cmp => "order".to_string(),
        FunctionKind::PartialCmp => "partialOrder".to_string(),
        FunctionKind::Hash => "hash".to_string(),
        FunctionKind::DebugToString => "toDbgString".to_string(),
        _ => {
            let prefix = format!("{}_", ffi_type_name(&f.class_name));
            let suffix = f
                .c_name
                .strip_prefix(&prefix)
                .or_else(|| f.c_name.rsplit('_').next())
                .unwrap_or(&f.c_name);
            match suffix {
                "new" => "create".to_string(),
                other => other.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            super::{
                config::CodegenConfig,
                ir::{
                    CallbackArgInfo, FieldDef, FieldRefKind, FunctionArg, FunctionDef,
                    FunctionKind, StructDef,
                },
            },
            c_decls::tests::fixture_ir,
        },
        *,
    };

    fn arg(name: &str, ty: &str, rk: ArgRefKind) -> FunctionArg {
        FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        }
    }

    /// A callback-wrapper-typed argument the way `ir_builder` really builds
    /// it: `callback_info` is populated for the WRAPPER type too (typedef
    /// under `callback_typedef_name`), so a generator keying on
    /// `callback_info.is_some()` alone would mistake the wrapper for the typedef.
    fn wrapper_cb_arg(name: &str, wrapper: &str) -> FunctionArg {
        let mut a = arg(name, wrapper, ArgRefKind::Owned);
        a.callback_info = Some(CallbackArgInfo {
            callback_typedef_name: format!("{wrapper}Type"),
            callback_wrapper_name: wrapper.into(),
            trampoline_name: String::new(),
        });
        a
    }

    fn func(
        c_name: &str,
        class: &str,
        kind: FunctionKind,
        args: Vec<FunctionArg>,
        ret: Option<&str>,
    ) -> FunctionDef {
        FunctionDef {
            c_name: c_name.into(),
            class_name: class.into(),
            method_name: "unused_by_zig".into(),
            kind,
            args,
            return_type: ret.map(str::to_string),
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        }
    }

    fn strukt(name: &str, category: TypeCategory, fields: Vec<FieldDef>) -> StructDef {
        StructDef {
            name: name.into(),
            doc: vec![],
            fields,
            external_path: None,
            module: "dom".into(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".into()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        }
    }

    fn field(name: &str, ty: &str, rk: FieldRefKind) -> FieldDef {
        FieldDef {
            name: name.into(),
            type_name: ty.into(),
            doc: None,
            is_public: true,
            ref_kind: rk,
        }
    }

    /// The shared c_decls fixture (`Foo` with `create`/`bump`/`delete`,
    /// `Update`, `OptionFoo`, `FooCallbackType`) plus a `Bar` class exercising
    /// every argument shape and a `FooVec`.
    fn ir() -> CodegenIR {
        let mut ir = fixture_ir();
        ir.enums
            .iter_mut()
            .find(|e| e.name == "OptionFoo")
            .unwrap()
            .category = TypeCategory::Option;
        ir.structs.push(strukt("String", TypeCategory::String, vec![]));
        ir.structs.push(strukt("RefAny", TypeCategory::RefAny, vec![]));
        ir.structs.push(strukt(
            "FooVec",
            TypeCategory::Vec,
            vec![
                field("ptr", "Foo", FieldRefKind::Ptr),
                field("len", "usize", FieldRefKind::Owned),
            ],
        ));
        ir.structs.push(strukt("Bar", TypeCategory::Regular, vec![]));
        for (c, kind, args, ret) in [
            ("AzOptionFoo_some", FunctionKind::EnumVariantConstructor, vec![arg("payload", "Foo", ArgRefKind::Owned)], Some("OptionFoo")),
            ("AzOptionFoo_none", FunctionKind::EnumVariantConstructor, vec![], Some("OptionFoo")),
            ("AzOptionFoo_clone", FunctionKind::DeepCopy, vec![arg("option_foo", "OptionFoo", ArgRefKind::Ref)], Some("OptionFoo")),
            ("AzFooVec_create", FunctionKind::Constructor, vec![], Some("FooVec")),
            ("AzFooVec_copyFromPtr", FunctionKind::Constructor, vec![arg("ptr", "*const Foo", ArgRefKind::Owned), arg("len", "usize", ArgRefKind::Owned)], Some("FooVec")),
            ("AzFooVec_fromItem", FunctionKind::Constructor, vec![arg("item", "Foo", ArgRefKind::Owned)], Some("FooVec")),
            ("AzBar_new", FunctionKind::Constructor, vec![], Some("Bar")),
            ("AzBar_setLabel", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("label", "String", ArgRefKind::Owned)], None),
            ("AzBar_setOnClick", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("data", "RefAny", ArgRefKind::Owned), wrapper_cb_arg("on_click", "ButtonOnClickCallback")], None),
            // Not an API kind (a variant constructor): the C symbol takes the wrapper struct by value.
            ("AzOptionFoo_fromCallback", FunctionKind::EnumVariantConstructor, vec![wrapper_cb_arg("payload", "ButtonOnClickCallback")], Some("OptionFoo")),
            ("AzBar_withCallback", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("cb", "FooCallbackType", ArgRefKind::Owned)], None),
            ("AzBar_setMaybe", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("maybe", "OptionFoo", ArgRefKind::Owned)], None),
            ("AzBar_setItems", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("items", "FooVec", ArgRefKind::Owned)], None),
            ("AzBar_addFoo", FunctionKind::MethodMut, vec![arg("bar", "Bar", ArgRefKind::RefMut), arg("foo", "Foo", ArgRefKind::Owned)], None),
            ("AzBar_eqFoo", FunctionKind::Method, vec![arg("bar", "Bar", ArgRefKind::Ref), arg("foo", "Foo", ArgRefKind::Ref)], Some("bool")),
            ("AzBar_foo", FunctionKind::Method, vec![arg("bar", "Bar", ArgRefKind::Owned)], Some("Foo")),
            ("AzBar_delete", FunctionKind::Delete, vec![arg("bar", "Bar", ArgRefKind::RefMut)], None),
        ] {
            ir.functions.push(func(c, c.split('_').next().unwrap().trim_start_matches("Az"), kind, args, ret));
        }
        ir
    }

    fn zig() -> String {
        super::super::generate(&ir(), &CodegenConfig::c_header()).unwrap()
    }

    #[test]
    fn method_names_are_lower_camel_from_the_c_symbol_and_new_is_create() {
        let z = zig();
        assert!(z.contains("    pub fn create() Self {\n        return Self{ .inner = C.AzBar_new() };\n"), "{z}");
        assert!(z.contains("    pub fn setLabel(self: *Self, label: anytype) void {\n        C.AzBar_setLabel(&self.inner, _asAzString(label));\n"), "{z}");
        assert!(z.contains("    pub fn bump(self: *Self, @\"align\": usize) void {\n"), "{z}");
        assert!(!z.contains("pub fn set_label("), "{z}");
    }

    #[test]
    fn conversions_are_keyed_on_ir_categories() {
        let z = zig();
        // RefAny + callback wrapper -> raw typedef trampoline (the pair-pattern raw form).
        assert!(z.contains("    pub fn setOnClick(self: *Self, data: anytype, on_click: anytype) void {\n        C.AzBar_setOnClick(&self.inner, _asRefAny(data), _asCallback(C.AzButtonOnClickCallbackType, on_click));\n"), "{z}");
        // A callback typedef argument.
        assert!(z.contains("C.AzBar_withCallback(&self.inner, _asCallback(C.AzFooCallbackType, cb));\n"), "{z}");
        // The same wrapper type on a non-API kind stays the struct the C symbol takes.
        assert!(z.contains("    pub fn fromCallback(payload: C.AzButtonOnClickCallback) C.AzOptionFoo {\n        return C.AzOptionFoo_fromCallback(payload);\n"), "{z}");
        // Option: some/none/clone from the IR, payload map by category.
        assert!(z.contains("_asAzOption(maybe, .{ .T = C.AzOptionFoo, .some = C.AzOptionFoo_some, .none = C.AzOptionFoo_none, .clone = C.AzOptionFoo_clone, .map = _identity })"), "{z}");
        // Vec: item type from the `ptr` field; no Clone export -> null.
        assert!(z.contains("_asAzVec(items, .{ .T = C.AzFooVec, .Item = C.AzFoo, .create = C.AzFooVec_create, .copyFromPtr = C.AzFooVec_copyFromPtr, .fromItem = C.AzFooVec_fromItem, .clone = null, .map = _identity })"), "{z}");
        // A wrapper-typed owned argument and a by-reference one. (`Bar` also has a
        // method `foo()`, so the parameter is renamed `foo_arg` — Zig forbids a
        // parameter shadowing a sibling declaration.)
        assert!(z.contains("    pub fn addFoo(self: *Self, foo_arg: anytype) void {\n        C.AzBar_addFoo(&self.inner, _asOwned(C.AzFoo, foo_arg, null));\n"), "{z}");
        assert!(z.contains("    pub fn eqFoo(self: *Self, foo_arg: anytype) bool {\n        const _ret = C.AzBar_eqFoo(&self.inner, _asConstPtr(C.AzFoo, foo_arg));\n"), "{z}");
    }

    #[test]
    fn wrapper_typed_returns_are_wrapped_and_by_value_self_is_consumed() {
        let z = zig();
        assert!(z.contains("    pub fn foo(self: *Self) Foo {\n        const _ret = Foo{ .inner = C.AzBar_foo(self.inner) };\n        self.consumed = true;\n        return _ret;\n"), "{z}");
    }

    #[test]
    fn runtime_prelude_is_emitted_once_after_the_c_namespace() {
        let z = zig();
        let c = z.find("pub const C = struct {\n").unwrap();
        let r = z.find("pub fn ReflectModel(comptime T: type) type {\n").unwrap();
        let w = z.find("pub const Bar = struct {\n").unwrap();
        assert!(c < r && r < w, "{z}");
        assert_eq!(z.matches("pub inline fn _asCallback(").count(), 1);
    }
}
