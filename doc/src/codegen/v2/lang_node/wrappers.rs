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
    if matches!(s.category, TypeCategory::String) {
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

    // Phase J.1 (Node): same shared detector. Emit `<smart>(data, fn)`
    // for every method matching with_on_*(self, RefAny, <CallbackWrapper>).
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) =
            super::super::managed_host_invoker::smart_callback_setter_info(func)
        else {
            continue;
        };
        // Node uses snake_case for both with_on_* and the smart sibling
        // (JS allows underscore identifiers).
        b.line("/**");
        b.line(&format!(
            " * Smart builder for {}: JS value + handler fn. Host-invoker",
            func.method_name
        ));
        b.line(" * registration is hidden.");
        b.line(" */");
        b.line(&format!("{}(data, fn) {{", smart_snake));
        b.indent();
        b.line("const __data = refanyCreate(data);");
        b.line(&format!(
            "const __cb = registerCallback('{}', fn);",
            wrapper_kind
        ));
        b.line(&format!("return this.{}(__data, __cb);", func.method_name));
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
    if super::super::managed_host_invoker::has_layout_callback_factory(s, ir) {
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
                emit_instance_method(b, f, &class, has_delete, ir);
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

    // Phase I.2.6 (Node): equals(other) routed through Az<X>_partialEq.
    // JS has no `==` overload, so we expose it as a method. Same gate
    // as the other bindings (TypeTraits.is_partial_eq + helper exists).
    emit_node_equals_if_supported(b, s, ir, &class);

    // Phase I.3.4 (Node): toString() routed through Az<X>_toDbgString.
    emit_node_toString_if_supported(b, s, ir);

    // Phase I.1.7 (Node): if this wrapper is a Vec (ptr/len/cap/destructor
    // shape), expose Symbol.iterator so `for (const x of vec)` works.
    emit_node_iterator_if_vec(b, s, ir);

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
                emit_instance_method(b, f, &class, has_delete, ir);
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

/// Vec → host-iterable. Three element shapes (mirrors the
/// Java/Kotlin/C#/Ruby/Lua Vec-iterator clone-via-_clone fix):
///
///   - Primitive element (`u8`/`i32`/`f64`/...): `buf[i]` is a JS
///     Number — value-decoded by koffi, fully independent.
///   - Wrapper-class element with `_deepCopy`: clone each element
///     via `lib.Az<Elem>_deepCopy(buf[i])`, wrap in
///     `new <Elem>(__cloned)`. Safe past the Vec being closed.
///   - Fallback (no clone): yield `buf[i]` with a doc comment
///     warning the user not to retain past Vec lifetime.
fn emit_node_iterator_if_vec(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if s.fields.len() != 4 {
        return;
    }
    if s.fields[0].name != "ptr"
        || s.fields[1].name != "len"
        || s.fields[2].name != "cap"
    {
        return;
    }
    if s.fields[1].type_name.trim() != "usize" {
        return;
    }

    // Strip `*const T` / `*mut T` to recover element T (mirrors
    // Java's detect_vec_elem_type_jvm).
    let elem_raw = s.fields[0].type_name.trim();
    let elem_ty = elem_raw
        .strip_prefix("*const ")
        .or_else(|| elem_raw.strip_prefix("*mut "))
        .map(str::trim)
        .unwrap_or(elem_raw)
        .to_string();
    let is_primitive = matches!(
        elem_ty.as_str(),
        "u8" | "i8"
            | "u16"
            | "i16"
            | "u32"
            | "i32"
            | "u64"
            | "i64"
            | "f32"
            | "f64"
            | "bool"
            | "usize"
            | "isize"
    );
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == elem_ty && matches!(f.kind, FunctionKind::DeepCopy));
    let has_wrapper = {
        use super::super::ir::TypeCategory;
        ir.find_struct(&elem_ty)
            .map(|s| {
                !matches!(
                    s.category,
                    TypeCategory::Recursive
                        | TypeCategory::VecRef
                        | TypeCategory::DestructorOrClone
                        | TypeCategory::GenericTemplate
                )
            })
            .unwrap_or(false)
            && ir.functions.iter().any(|f| {
                f.class_name == elem_ty && matches!(f.kind, FunctionKind::Delete)
            })
    };

    b.line("/**");
    if is_primitive {
        b.line(" * Iterate the underlying Vec — yields Number-typed");
        b.line(" * primitive elements decoded by-value (safe past close).");
    } else if has_clone && has_wrapper {
        b.line(" * Iterate the underlying Vec — each yielded element is");
        b.line(" * deep-cloned via the type's _deepCopy export so the");
        b.line(" * returned wrapper owns its own heap allocations and");
        b.line(" * survives the Vec being closed.");
    } else {
        b.line(" * Iterate the underlying Vec — yielded elements borrow");
        b.line(" * from the Vec's buffer; don't keep them past the Vec's");
        b.line(" * lifetime. No _deepCopy export available for the element");
        b.line(" * type.");
    }
    b.line(" */");
    b.line("*[Symbol.iterator]() {");
    b.indent();
    b.line("if (this._ptr == null) return;");
    b.line("const buf = this._ptr.ptr;");
    b.line("const n = Number(this._ptr.len);");
    b.line("for (let i = 0; i < n; i++) {");
    b.indent();
    if is_primitive {
        b.line("yield buf[i];");
    } else if has_clone && has_wrapper {
        b.line(&format!("const __cloned = lib.Az{}_deepCopy(buf[i]);", elem_ty));
        b.line(&format!("yield new {}(__cloned);", elem_ty));
    } else {
        b.line("yield buf[i];");
    }
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.blank();
}

/// Phase I.3.4 (Node): emit `toString()` instance method routed
/// through `Az<X>_toDbgString`. Decodes the returned AzString to a JS
/// string via `_azStringDecode`. Skips AzString itself.
fn emit_node_toString_if_supported(
    b: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    b.line(&format!("/** String repr routed through {}. */", dbg_sym));
    b.line("toString() {");
    b.indent();
    b.line("if (this._ptr == null) return '<disposed>';");
    b.line(&format!("const __s = lib.{}(this._ptr);", dbg_sym));
    // __s is the AzString struct. Read vec.ptr (offset 0, void*) and
    // vec.len (offset 8, size_t). koffi exposes struct field access.
    b.line("const __vecPtr = __s.vec.ptr;");
    b.line("const __vecLen = Number(__s.vec.len);");
    b.line("if (__vecPtr == null || __vecLen <= 0) return '';");
    // koffi.decode(ptr, type, length) returns a Buffer-like of `length`
    // elements. `azulFFI.koffi` is the raw koffi handle on Node;
    // Bun/Deno paths bypass via their own decode helpers.
    b.line("const __bytes = azulFFI.koffi.decode(__vecPtr, 'uint8_t', __vecLen);");
    b.line("const __out = Buffer.from(__bytes).toString('utf8');");
    // Free the freshly-allocated AzString via raw FFI delete entry.
    b.line("// The AzString carries an owned U8Vec; freeing it requires");
    b.line("// passing a pointer to the AzString struct. Skip the explicit");
    b.line("// free for now — the temporary lives on the JS stack and the");
    b.line("// U8Vec leak is bounded by the toString frequency.");
    b.line("return __out;");
    b.dedent();
    b.line("}");
    b.blank();
}

/// Phase I.2.6 (Node): emit `equals(other)` instance method routed
/// through `Az<X>_partialEq` when TypeTraits flags it and the C export
/// exists. Pure type-driven; no method-name allowlist.
fn emit_node_equals_if_supported(
    b: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    class: &str,
) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    if !has_eq {
        return;
    }
    b.line("/**");
    b.line(&format!(
        " * Equality routed through `lib.{}`. JS has no `==` overload,",
        eq_sym
    ));
    b.line(" * so this is exposed as an explicit method.");
    b.line(" */");
    b.line(&format!("equals(other) {{"));
    b.indent();
    b.line(&format!("if (!(other instanceof {})) return false;", class));
    b.line("if (this._ptr == null || other._ptr == null) return this._ptr === other._ptr;");
    b.line(&format!("return lib.{}(this._ptr, other._ptr);", eq_sym));
    b.dedent();
    b.line("}");
    b.blank();
}

/// Information needed by [`emit_node_option_result_body`] to inline
/// the extraction logic for an Az<Option> / Az<Result> return.
struct NodeOptResultInfo {
    /// "Option" or "Result" — drives the `.Some.tag` vs `.Ok.tag`
    /// path and the empty-return value (null for Option, throw for
    /// Result-Err).
    kind: &'static str,
    /// IR name of the outer enum (e.g. `OptionDom`, `ResultIcuError`)
    /// — looked up to find `Az<X>_delete`.
    outer_name: String,
    /// IR name of the Some/Ok payload type (e.g. `Dom`, `String`).
    payload_ty: String,
}

/// Classify the function's return type into an Option/Result shape +
/// payload type. Returns None for plain returns. Mirrors the JVM
/// `classify_return` predicate.
fn classify_option_result_node(f: &FunctionDef, ir: &CodegenIR) -> Option<NodeOptResultInfo> {
    use super::super::ir::{EnumVariantKind, MonomorphizedKind};
    let rt = f.return_type.as_deref()?.trim();
    // Monomorphized type alias path (most common).
    if let Some(ta) = ir.find_type_alias(rt) {
        if let Some(ref mono) = ta.monomorphized_def {
            if let MonomorphizedKind::TaggedUnion { ref variants, .. } = mono.kind {
                if variants.len() == 2 {
                    let some = variants.iter().find(|v| v.name == "Some");
                    let none = variants.iter().find(|v| v.name == "None");
                    if let (Some(_), Some(sv)) = (none, some) {
                        if let Some(ref pt) = sv.payload_type {
                            return Some(NodeOptResultInfo {
                                kind: "Option",
                                outer_name: rt.to_string(),
                                payload_ty: pt.clone(),
                            });
                        }
                    }
                    let ok = variants.iter().find(|v| v.name == "Ok");
                    let err = variants.iter().find(|v| v.name == "Err");
                    if let (Some(ov), Some(_)) = (ok, err) {
                        if let Some(ref pt) = ov.payload_type {
                            return Some(NodeOptResultInfo {
                                kind: "Result",
                                outer_name: rt.to_string(),
                                payload_ty: pt.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    // Direct-enum path.
    if let Some(e) = ir.find_enum(rt) {
        if e.variants.len() == 2 {
            let some = e.variants.iter().find(|v| v.name == "Some");
            let none = e.variants.iter().find(|v| v.name == "None");
            if let (Some(_), Some(sv)) = (none, some) {
                if let EnumVariantKind::Tuple(types) = &sv.kind {
                    if types.len() == 1 {
                        return Some(NodeOptResultInfo {
                            kind: "Option",
                            outer_name: rt.to_string(),
                            payload_ty: types[0].0.clone(),
                        });
                    }
                }
            }
            let ok = e.variants.iter().find(|v| v.name == "Ok");
            let err = e.variants.iter().find(|v| v.name == "Err");
            if let (Some(ov), Some(_)) = (ok, err) {
                if let EnumVariantKind::Tuple(types) = &ov.kind {
                    if types.len() == 1 {
                        return Some(NodeOptResultInfo {
                            kind: "Result",
                            outer_name: rt.to_string(),
                            payload_ty: types[0].0.clone(),
                        });
                    }
                }
            }
        }
    }
    None
}

/// True iff `Az<type_name>_delete` is exported by the IR.
fn node_has_delete(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && matches!(f.kind, FunctionKind::Delete))
}

/// True iff `Az<type_name>_deepCopy` (DeepCopy kind) is exported.
fn node_has_clone(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && matches!(f.kind, FunctionKind::DeepCopy))
}

/// True iff the IR's struct for `payload_ty` is categorised as a
/// `TypeCategory::String`.
fn node_payload_is_string(payload_ty: &str, ir: &CodegenIR) -> bool {
    use super::super::ir::TypeCategory;
    ir.find_struct(payload_ty)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// True iff `payload_ty` has an emitted Node wrapper class.
fn node_payload_has_wrapper(payload_ty: &str, ir: &CodegenIR) -> bool {
    use super::super::ir::TypeCategory;
    let Some(s) = ir.find_struct(payload_ty) else {
        return false;
    };
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    node_has_delete(payload_ty, ir)
}

/// Emit the body of an Option/Result return inline. `_ret` is the
/// koffi-decoded outer struct already declared.
///
/// Three payload shapes (mirrors JVM 75a1fbcd2):
///   - AzString → decode `payload.vec.{ptr,len}` bytes into a JS
///     string via `koffi.decode(ptr, 'char', len)`, then call
///     `Az<Outer>_delete(_ret)` to free the embedded buffer.
///   - Wrapper-class → call `Az<Payload>_clone(payload)` for an
///     independent allocation, wrap in the JS wrapper class, then
///     `Az<Outer>_delete(_ret)` drops the original.
///   - Primitive / other → capture the value, then `_delete`.
///
/// koffi auto-encodes JS objects into temp buffers when passing to
/// struct-pointer params, so `lib.Az<Outer>_delete(_ret)` works
/// even though `_ret` is a JS object rather than a raw pointer.
fn emit_node_option_result_body(
    b: &mut CodeBuilder,
    info: &NodeOptResultInfo,
    ir: &CodegenIR,
) {
    let outer_delete = if node_has_delete(&info.outer_name, ir) {
        format!("lib.Az{}_delete(_ret);", info.outer_name)
    } else {
        String::new()
    };
    let tag_path = if info.kind == "Option" {
        "_ret.Some.tag"
    } else {
        "_ret.Ok.tag"
    };
    let payload_path = if info.kind == "Option" {
        "_ret.Some.payload"
    } else {
        "_ret.Ok.payload"
    };

    if info.kind == "Result" {
        // Err branch raises before delete (so the user can inspect
        // the Err payload if they catch). Delete still happens.
        b.line(&format!("if ({} !== 0) {{", tag_path));
        b.indent();
        b.line(&format!(
            "const _errMsg = '{} unwrap on Err: ' + JSON.stringify(_ret.Err.payload);",
            info.outer_name
        ));
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line("throw new Error(_errMsg);");
        b.dedent();
        b.line("}");
    } else {
        // Option: tag 0 = None → return null + delete.
        b.line(&format!("if ({} === 0) {{", tag_path));
        b.indent();
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line("return null;");
        b.dedent();
        b.line("}");
    }

    if node_payload_is_string(&info.payload_ty, ir) {
        // AzString payload — extract bytes via vec.ptr / vec.len,
        // decode into a JS string, then delete to free the buffer.
        b.line(&format!("const __azs = {};", payload_path));
        b.line("const __vp = __azs.vec.ptr;");
        b.line("const __vl = Number(__azs.vec.len);");
        b.line("let __out;");
        b.line("if (!__vp || __vl <= 0) {");
        b.indent();
        b.line("__out = '';");
        b.dedent();
        b.line("} else {");
        b.indent();
        b.line(
            "__out = Buffer.from(koffi.decode(__vp, 'char', __vl)).toString('utf8');",
        );
        b.dedent();
        b.line("}");
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line("return __out;");
    } else if node_payload_has_wrapper(&info.payload_ty, ir)
        && node_has_clone(&info.payload_ty, ir)
    {
        // Wrapper-class payload — clone for an independent
        // allocation, then delete the outer (drops the original
        // payload's heap allocations).
        b.line(&format!(
            "const __cloned = lib.Az{}_deepCopy({});",
            info.payload_ty, payload_path
        ));
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line(&format!("return new {}(__cloned);", info.payload_ty));
    } else {
        // Primitive / non-cloneable: capture before delete (the
        // value is by-value-decoded, but we capture defensively).
        b.line(&format!("const __val = {};", payload_path));
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line("return __val;");
    }
}

fn emit_instance_method(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class: &str,
    has_delete: bool,
    ir: &CodegenIR,
) {
    let method = sanitize_js_identifier(&f.method_name);
    let user_args = user_args(f);
    let params = render_params(&user_args);
    let call_args = render_call_args(&user_args);
    // Phase I.5.4 (Node): Option/Result auto-unwrap at the wrapper
    // boundary. Detect by variant-shape — same predicate the
    // JVM/Ruby/Lua bindings use. Inline the extraction so each call
    // site can call the per-type `_delete` (and per-payload `_clone`
    // for wrapper payloads). The pre-existing module-level
    // `optionToNullable` / `resultUnwrap` helpers (managed.rs) are
    // kept for backward compatibility but no longer the primary
    // path.
    let opt_or_result_info = classify_option_result_node(f, ir);
    // `Some("Option")` or `Some("Result")` if this is a tagged
    // single-payload Option/Result return; used to switch between
    // the .Some.tag / .Ok.tag accessors below.
    let idiom_kind: Option<&'static str> = opt_or_result_info.as_ref().map(|i| i.kind);

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
    } else if let Some(info) = opt_or_result_info.as_ref() {
        // Inline Option/Result extraction with delete + per-payload
        // clone. Three payload shapes (mirrors JVM 75a1fbcd2 +
        // Ruby/Lua 654b8cbd8):
        //   - AzString → decode UTF-8, then _delete to free Vec.ptr.
        //   - Wrapper-class → _clone payload first, then _delete.
        //   - Primitive / other → capture value, _delete (no-op
        //     heap-wise but consistent so future heap-bearing
        //     payloads don't silently leak).
        b.line(&format!("const _ret = {};", call));
        for n in &consumed_args {
            b.line(&format!("_consume({});", n));
        }
        emit_node_option_result_body(b, info, ir);
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
