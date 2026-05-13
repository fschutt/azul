//! ES6 wrapper-class emission.
//!
//! For every IR struct that has a corresponding `<TypeName>_delete`
//! C function we emit a `class TypeName` that:
//!
//! - Holds the raw FFI pointer in `#ptr` (a private class field).
//! - Registers a `FinalizationRegistry` callback so the underlying
//!   native resource is released when the JS wrapper is GC'd.
//! - Exposes every non-trait method as an instance/static method
//!   that dispatches through the `lib` object.
//! - Implements `[Symbol.for('nodejs.util.inspect.custom')]` so
//!   `console.log(obj)` produces `App { ptr: 0x... }` rather than
//!   leaking internals.
//!
//! Tagged-union enums get the same treatment plus per-variant
//! predicates (`isVariantName()`) that compare against the registered
//! tag enum constant.
//!
//! ## FinalizationRegistry caveat
//!
//! `FinalizationRegistry` callbacks are best-effort: the spec does
//! not guarantee they run before process exit. For native resources
//! that *must* be freed in a specific order (e.g. an `App` that owns
//! its child windows) callers should still call `.delete()`
//! explicitly. The registry exists as a safety net, not as a
//! deterministic destructor.
//!
//! ## Skipped categories
//!
//! Same filter as PHP / Lua. See `mod.rs` doc-comment for the list.

use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::{ffi_type_name, sanitize_export_name, sanitize_js_identifier};

// ============================================================================
// Public entry point
// ============================================================================

pub fn generate_wrappers(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// One FinalizationRegistry per disposable type. Each registry calls the");
    b.line("// matching `<Type>_delete` C function on garbage collection. Registries");
    b.line("// are keyed by the type's C name to keep the symbol lookup local.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line("function makeRegistry(deleteFn) {");
    b.indent();
    b.line("// Skip registry creation entirely on runtimes that lack it (very old");
    b.line("// Node). The user pays only the missing-cleanup cost.");
    b.line("if (typeof FinalizationRegistry === 'undefined') {");
    b.indent();
    b.line("return { register() {}, unregister() {} };");
    b.dedent();
    b.line("}");
    b.line("return new FinalizationRegistry((ptr) => {");
    b.indent();
    b.line("try { deleteFn(ptr); } catch (_e) { /* native cleanup is best-effort */ }");
    b.dedent();
    b.line("});");
    b.dedent();
    b.line("}");
    b.blank();

    // Mark a wrapper instance as consumed: unregister from its class's
    // FinalizationRegistry and null out `_ptr`. Used by consuming
    // builder methods (`with_*`) and by mutators routed through them.
    // The C side just moved this struct's internal heap pointers into a
    // new owner; if we let the registry's finalizer fire later it would
    // call `<Type>_delete` on the now-transferred pointers — a double
    // free. Calling this with a non-wrapper value (primitive, plain
    // koffi struct value, undefined) is a no-op.
    b.line("function _consume(val) {");
    b.indent();
    b.line("if (val && typeof val === 'object' && val.constructor &&");
    b.indent();
    b.line("typeof val.constructor._registry !== 'undefined') {");
    b.dedent();
    b.indent();
    b.line("val.constructor._registry.unregister(val);");
    b.line("val._ptr = null;");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.blank();

    // Auto-AzString-conversion helper. Wrapper methods route Owned
    // `String` args through this so user code can pass plain JS
    // strings directly (`Dom.create_text(\"hi\")`). Pass-through for
    // already-AzString values (existing koffi objects or wrapper
    // instances with `_ptr`). Pure type-driven; no method-name allow-
    // list (the codegen detects `type_name == \"String\"` + Owned in
    // `render_call_args`).
    b.line("// Auto-AzString-conversion helper. Wrapper methods route Owned");
    b.line("// `String` args through this so plain JS strings work directly.");
    b.line("// `globalThis.String` avoids colliding with the wrapper class");
    b.line("// `String` emitted in this module's local scope.");
    b.line("function _azString(val) {");
    b.indent();
    b.line("if (val == null) return val;");
    b.line("if (typeof val === 'object') {");
    b.indent();
    b.line("if (val._ptr !== undefined) return val._ptr;");
    b.line("return val;");
    b.dedent();
    b.line("}");
    b.line("const buf = Buffer.from(globalThis.String(val), 'utf8');");
    b.line("return lib.AzString_fromUtf8(buf, buf.length);");
    b.dedent();
    b.line("}");
    b.blank();

    b.line("// ----------------------------------------------------------------------------");
    b.line("// Wrapper classes (one per disposable struct / tagged-union enum).");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    for s in &ir.structs {
        if !should_emit_struct(s) {
            continue;
        }
        emit_struct_wrapper(b, ir, s);
    }
    for e in &ir.enums {
        if !should_emit_enum(e) {
            continue;
        }
        emit_enum_wrapper(b, ir, e);
    }
}

// ============================================================================
// Public filters (also called from mod.rs::emit_exports)
// ============================================================================

pub fn should_emit_struct(s: &StructDef) -> bool {
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::Boxed
            | TypeCategory::GenericTemplate
            | TypeCategory::DestructorOrClone
            | TypeCategory::CallbackTypedef
    )
}

pub fn should_emit_enum(e: &EnumDef) -> bool {
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

fn has_delete_for(class: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class && f.kind == FunctionKind::Delete)
}

// ============================================================================
// Struct wrapper
// ============================================================================

fn emit_struct_wrapper(b: &mut CodeBuilder, ir: &CodegenIR, s: &StructDef) {
    let class = sanitize_export_name(&s.name);
    let ffi = ffi_type_name(&s.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&s.name).collect();
    if funcs.is_empty() {
        // Nothing useful to wrap; the FFI type is registered above and
        // power users can reach it via `azul.__ffi`.
        return;
    }
    let has_delete = has_delete_for(&s.name, ir);

    if !s.doc.is_empty() {
        b.line("/**");
        for d in &s.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(" */");
    }

    b.line(&format!("class {} {{", class));
    b.indent();

    // Private storage. We use the `_ptr` convention rather than `#ptr`
    // because some bundlers / older runtimes still mishandle private
    // class fields. The leading underscore is a soft-private marker.
    b.line("/** @type {*} */");
    b.line("_ptr;");
    b.blank();

    // Per-class FinalizationRegistry. Created once at class-definition
    // time; instances register themselves in their constructor.
    if has_delete {
        b.line(&format!(
            "static _registry = makeRegistry((ptr) => lib.{}_delete(ptr));",
            ffi
        ));
        b.blank();
    }

    // Constructor: takes a raw FFI pointer. Public callers should use
    // the static factories below (`create()`, `default()`, etc.).
    b.line("/**");
    b.line(" * Wrap an existing FFI pointer (takes ownership for GC purposes).");
    b.line(" * Most callers should use the static factory methods instead.");
    b.line(" * @param {*} ptr raw FFI pointer/handle to a native value");
    b.line(" */");
    b.line("constructor(ptr) {");
    b.indent();
    b.line("this._ptr = ptr;");
    if has_delete {
        b.line(&format!(
            "{}._registry.register(this, ptr, this);",
            class
        ));
    }
    b.dedent();
    b.line("}");
    b.blank();

    // Raw accessor.
    b.line("/** Return the underlying FFI pointer. Use with care. */");
    b.line("get raw() { return this._ptr; }");
    b.blank();

    // Idiomatic console.log output.
    b.line(&format!(
        "[Symbol.for('nodejs.util.inspect.custom')]() {{ return `{} {{ ptr: ${{this._ptr}} }}`; }}",
        class
    ));
    b.blank();

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a JS string. AzString's C-side layout is
    // `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`.
    // koffi.decode handles the struct read; len comes back as BigInt
    // (size_t), so coerce to Number for the array bound.
    if s.name == "String" {
        b.line("/**");
        b.line(" * Decode the wrapped UTF-8 bytes into a JS string.");
        b.line(" * Returns '' if not available on the current runtime (koffi only).");
        b.line(" */");
        b.line("toString() {");
        b.indent();
        b.line("if (!this._ptr) return '';");
        b.line("// koffi-only path; Bun / Deno would need separate helpers.");
        b.line("if (azulFFI.runtime !== 'node-koffi') return '[AzString — decode not implemented for this runtime]';");
        b.line("const koffi = azulFFI.koffi;");
        b.line("const az = koffi.decode(this._ptr, 'AzString');");
        b.line("const len = Number(az.vec.len);");
        b.line("if (!az.vec.ptr || len === 0) return '';");
        b.line("const bytes = koffi.decode(az.vec.ptr, koffi.array('uint8_t', len));");
        b.line("return Buffer.from(bytes).toString('utf8');");
        b.dedent();
        b.line("}");
        b.blank();
    }

    // Button.onClick(data, fn) — smart builder. Wraps `data` in a
    // RefAny and `fn` in a Callback via the host invoker. Returns a
    // new Button koffi struct with the click wiring set.
    if s.name == "Button" {
        b.line("/**");
        b.line(" * Smart builder: pass any JS value as the data payload and a");
        b.line(" * click-handler function. Host-invoker registration is hidden.");
        b.line(" */");
        b.line("onClick(data, fn) {");
        b.indent();
        b.line("const __data = refanyCreate(data);");
        b.line("const __cb = registerCallback('Callback', fn);");
        b.line("return this.with_on_click(__data, __cb);");
        b.dedent();
        b.line("}");
        b.blank();
    }

    // WindowCreateOptions.createWithLayout(fn) — smart factory. The
    // codegen-emitted `create(fn)` routes through
    // `AzWindowCreateOptions_create` which takes a raw `AzLayoutCallbackType`
    // function pointer and discards the host-invoker `ctx` — callbacks
    // would never reach the user's JS function. This helper instead
    // registers the callback through the host-invoker handle table,
    // grabs a `_default()` WCO, and assigns the full AzLayoutCallback
    // struct (cb + ctx) to `window_state.layout_callback` so dispatch
    // works. koffi's JS-side nested-struct assignment is byte-copy
    // semantics, matching the C side.
    if s.name == "WindowCreateOptions" {
        b.line("/**");
        b.line(" * Smart factory: pass a layout-callback function; the host-invoker");
        b.line(" * registration and field-copy plumbing happen internally. The");
        b.line(" * caller never has to touch AzulHostInvoker or `_register_callback`.");
        b.line(" */");
        b.line("static createWithLayout(fn) {");
        b.indent();
        b.line("const cb = registerCallback('LayoutCallback', fn);");
        b.line("const opts = lib.AzWindowCreateOptions_default();");
        b.line("opts.window_state.layout_callback = cb;");
        b.line("return new WindowCreateOptions(opts);");
        b.dedent();
        b.line("}");
        b.blank();
    }

    // Methods: Method, MethodMut, DeepCopy, DebugToString, plus static
    // factories (Constructor, StaticMethod, Default, EnumVariantConstructor).
    let mut emitted_any = false;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(b, f, &class, has_delete);
                emitted_any = true;
            }
            FunctionKind::DeepCopy => {
                emit_instance_alias(b, f, "clone", &class);
                emitted_any = true;
            }
            FunctionKind::DebugToString => {
                emit_instance_alias(b, f, "toString", &class);
                emitted_any = true;
            }
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                emit_static_factory(b, f, &class);
                emitted_any = true;
            }
            // SKIPPED: Delete is wired through FinalizationRegistry.
            // SKIPPED: PartialEq/Cmp/Hash are surfaced via `azul.__ffi.lib`
            //          for callers who need them; they are not idiomatic JS.
            // SKIPPED: EnumVariantConstructor doesn't apply to structs.
            _ => {}
        }
    }
    if !emitted_any {
        b.line("// SKIPPED: no idiomatic methods to surface (use azul.__ffi.lib for raw access).");
    }

    // Explicit `delete()` for callers who need deterministic disposal.
    if has_delete {
        b.line("/**");
        b.line(" * Explicitly free the underlying native resources. After calling");
        b.line(" * delete(), the wrapper must not be used. Calling delete() twice is");
        b.line(" * a no-op.");
        b.line(" */");
        b.line("delete() {");
        b.indent();
        b.line("if (this._ptr === null) return;");
        b.line(&format!("{}._registry.unregister(this);", class));
        b.line(&format!("lib.{}_delete(this._ptr);", ffi));
        b.line("this._ptr = null;");
        b.dedent();
        b.line("}");
        b.blank();
    }

    b.dedent();
    b.line("}");
    b.blank();
}

// ============================================================================
// Tagged-union enum wrapper
// ============================================================================

fn emit_enum_wrapper(b: &mut CodeBuilder, ir: &CodegenIR, e: &EnumDef) {
    let class = sanitize_export_name(&e.name);
    let ffi = ffi_type_name(&e.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&e.name).collect();
    let has_delete = has_delete_for(&e.name, ir);

    // Unit-only enums are exposed as frozen objects, not as classes.
    // Only data-bearing enums get a class wrapper.
    let unit_only = !e.is_union
        && e.variants
            .iter()
            .all(|v| matches!(v.kind, EnumVariantKind::Unit));
    if unit_only {
        b.line(&format!(
            "// {0} is a unit-only enum; numeric constants live on Enums.{0}.",
            e.name
        ));
        b.line(&format!(
            "const {0} = Enums.{0};",
            class
        ));
        b.blank();
        return;
    }

    if !e.doc.is_empty() {
        b.line("/**");
        for d in &e.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(" */");
    }

    b.line(&format!("class {} {{", class));
    b.indent();

    b.line("/** @type {*} */");
    b.line("_ptr;");
    b.blank();

    if has_delete {
        b.line(&format!(
            "static _registry = makeRegistry((ptr) => lib.{}_delete(ptr));",
            ffi
        ));
        b.blank();
    }

    // Tag constants accessible as static members for caller-side checks.
    b.line("/** Discriminator-tag values (one per variant). */");
    b.line(&format!("static Tag = Enums.{}_Tag;", e.name));
    b.blank();

    b.line("constructor(ptr) {");
    b.indent();
    b.line("this._ptr = ptr;");
    if has_delete {
        b.line(&format!(
            "{}._registry.register(this, ptr, this);",
            class
        ));
    }
    b.dedent();
    b.line("}");
    b.blank();

    b.line("get raw() { return this._ptr; }");
    b.blank();

    b.line(&format!(
        "[Symbol.for('nodejs.util.inspect.custom')]() {{ return `{} {{ ptr: ${{this._ptr}} }}`; }}",
        class
    ));
    b.blank();

    // Per-variant predicates. Each variant's payload struct begins
    // with a `tag` field at offset 0; reading through any one of them
    // yields the same value because of the union layout.
    if e.is_union {
        if let Some(first) = e.variants.first() {
            let first_field = sanitize_js_identifier(&first.name);
            b.line("/** Return the variant discriminator tag value (an int). */");
            b.line("tag() {");
            b.indent();
            b.line("// Read through the first variant's `tag` field; every variant");
            b.line("// payload struct begins with the same tag, so this is layout-safe.");
            b.line(&format!(
                "return this._ptr ? this._ptr.{}.tag : -1;",
                first_field
            ));
            b.dedent();
            b.line("}");
            b.blank();
        }
        for v in &e.variants {
            let pred = format!("is{}", v.name);
            b.line(&format!(
                "/** True if this {} value carries the {} variant. */",
                e.name, v.name
            ));
            b.line(&format!("{}() {{", pred));
            b.indent();
            b.line(&format!("return this.tag() === {}.Tag.{};", class, sanitize_js_identifier(&v.name)));
            b.dedent();
            b.line("}");
            b.blank();
            // SKIPPED: per-variant payload extractors. The shape varies wildly
            // by variant; we expose `this._ptr.<variantField>` as the escape
            // hatch for callers that need it.
        }
    }

    let mut emitted_any = false;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(b, f, &class, has_delete);
                emitted_any = true;
            }
            FunctionKind::DeepCopy => {
                emit_instance_alias(b, f, "clone", &class);
                emitted_any = true;
            }
            FunctionKind::DebugToString => {
                emit_instance_alias(b, f, "toString", &class);
                emitted_any = true;
            }
            FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default
            | FunctionKind::EnumVariantConstructor => {
                emit_static_factory(b, f, &class);
                emitted_any = true;
            }
            _ => {}
        }
    }
    if !emitted_any && !e.is_union {
        b.line("// SKIPPED: no idiomatic methods to surface.");
    }

    if has_delete {
        b.line("delete() {");
        b.indent();
        b.line("if (this._ptr === null) return;");
        b.line(&format!("{}._registry.unregister(this);", class));
        b.line(&format!("lib.{}_delete(this._ptr);", ffi));
        b.line("this._ptr = null;");
        b.dedent();
        b.line("}");
        b.blank();
    }

    b.dedent();
    b.line("}");
    b.blank();
}

// ============================================================================
// Method emission helpers
// ============================================================================

fn emit_instance_method(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class: &str,
    has_delete: bool,
) {
    let method = sanitize_js_identifier(&f.method_name);
    let user_args = user_args(f);
    let params = render_params(&user_args);
    let call_args = render_call_args(&user_args);

    if !f.doc.is_empty() {
        b.line("/**");
        for d in &f.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(&format!(" * Wraps `lib.{}` with `this` bound as the receiver.", f.c_name));
        b.line(" */");
    }
    b.line(&format!("{}({}) {{", method, params));
    b.indent();
    emit_callback_register_lines(b, &user_args);
    let mut call = format!("lib.{}(this._ptr", f.c_name);
    if !call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&call_args);
    }
    call.push(')');

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);
    let consumed_args = consumed_wrapper_args(&user_args);

    if returns_self {
        // Consuming-builder pattern: `body.with_child(label)` moves
        // `body` (self) and `label` (by-value wrapper arg) into the C
        // call; their internal heap pointers are now owned by the
        // returned struct. We must unregister both from their
        // FinalizationRegistries and null their `_ptr` to prevent the
        // finalizer firing later on the already-transferred memory
        // (double free).
        b.line(&format!("const _next = {};", call));
        for n in &consumed_args {
            b.line(&format!("_consume({});", n));
        }
        if has_delete {
            b.line(&format!("{}._registry.unregister(this);", class));
        }
        b.line("this._ptr = null;");
        b.line(&format!("return new {}(_next);", class));
    } else if f.return_type.is_none() {
        // Side-effecting call (no return value). koffi cannot write
        // back through `T *` args, so structural mutators like
        // `add_child` are effectively no-ops here. The fix at the
        // emission site is to route through the matching `with_*`
        // form; that pass is intentionally separate and not done
        // here so the simple void-return case stays a one-liner.
        b.line(&format!("{};", call));
        // Still consume any by-value wrapper args — even no-op mutators
        // semantically take ownership of them on the C side.
        for n in &consumed_args {
            b.line(&format!("_consume({});", n));
        }
    } else {
        b.line(&format!("return {};", call));
        for n in &consumed_args {
            b.line(&format!("_consume({});", n));
        }
    }

    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_instance_alias(b: &mut CodeBuilder, f: &FunctionDef, alias: &str, class: &str) {
    let user_args = user_args(f);
    let params = render_params(&user_args);
    let call_args = render_call_args(&user_args);

    b.line(&format!(
        "/** Idiomatic alias dispatching to `lib.{}`. */",
        f.c_name
    ));
    b.line(&format!("{}({}) {{", alias, params));
    b.indent();
    emit_callback_register_lines(b, &user_args);
    let mut call = format!("lib.{}(this._ptr", f.c_name);
    if !call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&call_args);
    }
    call.push(')');

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    if returns_self {
        // DeepCopy/Clone path: C returns a freshly-allocated copy,
        // self is unaffected. Wrap so callers get a Class instance.
        b.line(&format!("return new {}({});", class, call));
    } else if f.return_type.is_none() {
        b.line(&format!("{};", call));
    } else {
        b.line(&format!("return {};", call));
    }
    b.dedent();
    b.line("}");
    b.blank();
}

/// Return the JS identifiers of arguments that are consumed
/// (by-value, i.e. `ArgRefKind::Owned`) by the C call. The wrapper
/// class can't know at codegen time whether the user-supplied value
/// is a wrapper instance or a primitive; `_consume` no-ops on
/// primitives, so we emit the call unconditionally for every
/// owned-by-value arg.
fn consumed_wrapper_args(args: &[&super::super::ir::FunctionArg]) -> Vec<String> {
    use super::super::ir::ArgRefKind;
    args.iter()
        .filter(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .map(|a| sanitize_js_identifier(&a.name))
        .collect()
}

fn emit_static_factory(b: &mut CodeBuilder, f: &FunctionDef, class_name: &str) {
    let method = sanitize_js_identifier(&f.method_name);
    let user_args = user_args(f);
    let params = render_params(&user_args);
    let call_args = render_call_args(&user_args);

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    if !f.doc.is_empty() {
        b.line("/**");
        for d in &f.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(&format!(" * Wraps `lib.{}`.", f.c_name));
        b.line(" */");
    }
    b.line(&format!("static {}({}) {{", method, params));
    b.indent();
    emit_callback_register_lines(b, &user_args);
    let call = format!("lib.{}({})", f.c_name, call_args);
    if returns_self {
        b.line(&format!("return new {}({});", class_name, call));
    } else if f.return_type.is_none() {
        b.line(&format!("{};", call));
    } else {
        b.line(&format!("return {};", call));
    }
    b.dedent();
    b.line("}");
    b.blank();
}

/// For every arg whose IR `callback_info` is in the host-invoker
/// allowlist, emit `name = registerCallback('Wrapper', name);` before
/// the lib call so the user can pass a plain JS function.
fn emit_callback_register_lines(b: &mut CodeBuilder, args: &[&super::super::ir::FunctionArg]) {
    for a in args {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        if !super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper) {
            continue;
        }
        let name = sanitize_js_identifier(&a.name);
        b.line(&format!(
            "{n} = registerCallback('{w}', {n});",
            n = name,
            w = wrapper
        ));
    }
}

// ============================================================================
// Argument helpers
// ============================================================================

fn user_args<'a>(f: &'a FunctionDef) -> Vec<&'a super::super::ir::FunctionArg> {
    let class_lower = f.class_name.to_lowercase();
    f.args
        .iter()
        .filter(|a| a.name != "self" && a.name != class_lower)
        .collect()
}

fn render_params(args: &[&super::super::ir::FunctionArg]) -> String {
    args.iter()
        .map(|a| sanitize_js_identifier(&a.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_call_args(args: &[&super::super::ir::FunctionArg]) -> String {
    args.iter()
        .map(|a| {
            // Auto-string-conversion (type-driven; no method-name allow-
            // list): Owned `String` args route through `_azString` so
            // plain JS strings get converted to AzString in line. The
            // helper is a pass-through for already-AzString values.
            let n = sanitize_js_identifier(&a.name);
            if is_az_string_owned_arg(a) {
                return format!("_azString({n})", n = n);
            }
            // If the arg is a wrapper-class instance the user will pass
            // the wrapper directly; pull `._ptr` out so the FFI gets a
            // raw pointer. We use a permissive `?._ptr ?? value` guard
            // so primitives (numbers, booleans) pass through unchanged.
            format!("({n} && {n}._ptr !== undefined ? {n}._ptr : {n})", n = n)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Auto-string-conversion rule (mirrors Java/Kotlin/C#/Ruby): any Owned
/// `String` arg at the C ABI accepts a plain JS string at the wrapper
/// level. The call site routes the value through `_azString` (emitted
/// in the module preamble).
fn is_az_string_owned_arg(a: &super::super::ir::FunctionArg) -> bool {
    a.type_name.trim() == "String"
        && matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
}

fn jsdoc_escape(s: &str) -> String {
    s.replace("*/", "* /")
}
