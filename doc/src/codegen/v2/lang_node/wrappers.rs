//! ES6 wrapper-class emission.
//!
//! For every IR struct that has a corresponding `<TypeName>_delete`
//! C function we emit a `class TypeName` that:
//!
//! - Holds the koffi-decoded struct value (or raw FFI pointer) in `_ptr`.
//! - Registers a `FinalizationRegistry` callback so the underlying native resource is released when
//!   the JS wrapper is GC'd.
//! - Exposes every non-trait method as an instance/static method that dispatches through the `lib`
//!   object. Method names are the lowerCamel form of the api.json key — the same derivation
//!   `ir_builder` uses for the C suffix (`create_p_with_text` → `createPWithText` ↔
//!   `AzDom_createPWithText`), so the JS surface reads like the C header without the prefix.
//! - Implements `[Symbol.for('nodejs.util.inspect.custom')]` so `console.log(obj)` produces `App {
//!   ptr: 0x... }` rather than leaking internals.
//!
//! Tagged-union enums get the same treatment plus per-variant
//! predicates (`isVariantName()`) that compare against the registered
//! tag enum constant.
//!
//! ## Ownership rules (all derived from the IR, no per-type cases)
//!
//! - A method whose receiver is `self` by value (`ArgRefKind::Owned`) consumes the wrapper: after
//!   the call it is unregistered from its FinalizationRegistry and `_ptr` is nulled, whatever the
//!   return type (`Dom.withChild(...) -> Dom`, `Button.dom() -> Dom`, `App.run(...)`).
//! - A by-value (`Owned`) wrapper-typed argument is consumed the same way (`_consume`). `RefAny`
//!   args are exempt: they are *created* from the user's JS value (`refanyCreate`), the user's
//!   object is never a wrapper.
//! - A `&mut self` receiver is bound as `_Inout_ T *` (see `functions.rs`) so koffi copies the
//!   struct back into `_ptr` after the call; void mutators return `this` for chaining
//!   (`body.addChild(a).addChild(b)`).
//! - A by-value return whose type has a wrapper class is wrapped (`new Dom(_ret)`); the caller owns
//!   it. Option/Result returns are unwrapped inline (see `emit_node_option_result_body`).
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

use super::{
    super::{
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionArg, FunctionDef,
            FunctionKind, StructDef, TypeCategory,
        },
        managed_host_invoker::{
            layout_callback_factory_info, smart_callback_setter_info, LayoutCallbackFactoryInfo,
            HOST_INVOKER_KINDS,
        },
    },
    ffi_type_name, is_refany_type, js_arg_name, js_method_name, sanitize_export_name,
    sanitize_js_identifier, string_struct,
};

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
    // FinalizationRegistry and null out `_ptr`. Used for by-value
    // wrapper args and by-value receivers. The C side just moved this
    // struct's internal heap pointers into a new owner; if we let the
    // registry's finalizer fire later it would call `<Type>_delete` on
    // the now-transferred pointers — a double free. Calling this with a
    // non-wrapper value (primitive, plain koffi struct value, undefined)
    // is a no-op.
    b.line("// Mark a wrapper instance as moved into the C side: its finalizer must");
    b.line("// never run on the transferred bytes. No-op for non-wrapper values.");
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
    // strings directly (`Dom.createPWithText("hi")`). Pass-through for
    // already-AzString values (existing koffi objects or wrapper
    // instances with `_ptr`). Pure type-driven; no method-name allow-
    // list (the codegen detects `TypeCategory::String` + Owned in
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

    emit_az_string_decode(b, ir);

    // Recursive opts-object applier. Each struct wrapper's `with(opts)`
    // instance method routes through this helper to assign nested
    // fields of the underlying koffi struct. Keys are accepted in the
    // C field spelling (`window_state`) or its lowerCamel form
    // (`windowState`); unknown keys throw instead of being silently
    // dropped. Plain `{}` literals recurse into the nested struct
    // field; JS string values auto-convert via `_azString`; wrapper-
    // class instances forward via their `_ptr`; everything else
    // (numbers, enum constants, booleans, Buffers, raw koffi structs)
    // assigns directly. Pure type-driven; no per-field allow-list.
    b.line("// lowerCamel -> snake_case for `with({ windowState: ... })` keys.");
    b.line("function _snakeKey(key) {");
    b.indent();
    b.line("return key.replace(/[A-Z]/g, (c) => '_' + c.toLowerCase());");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("// Recursive opts-object applier. Routed through every struct wrapper's");
    b.line("// `with(opts)` method below. `path` is only used for error messages.");
    b.line("function _applyOpts(struct, opts, path) {");
    b.indent();
    b.line("if (opts == null) return;");
    b.line("if (struct == null || typeof struct !== 'object') {");
    b.indent();
    b.line("throw new TypeError(`azul: ${path} is not a struct value; cannot assign options into it`);");
    b.dedent();
    b.line("}");
    b.line("for (const key of Object.keys(opts)) {");
    b.indent();
    b.line("const value = opts[key];");
    b.line("if (value === null || value === undefined) continue;");
    b.line("const field = (key in struct) ? key : _snakeKey(key);");
    b.line("if (!(field in struct)) {");
    b.indent();
    b.line("throw new TypeError(`azul: unknown field '${key}' on ${path} (fields: ${Object.keys(struct).join(', ')})`);");
    b.dedent();
    b.line("}");
    b.line("if (typeof value === 'string') {");
    b.indent();
    b.line("struct[field] = _azString(value);");
    b.dedent();
    // Wrapper-class instance: forward its underlying koffi value.
    // Checked before the plain-object branch so we don't recurse
    // into wrapper internals.
    b.line("} else if (typeof value === 'object' && value._ptr !== undefined) {");
    b.indent();
    b.line("struct[field] = value._ptr;");
    b.dedent();
    // Plain-object literal: recurse into nested koffi struct. We
    // detect via `Object.getPrototypeOf(value) === Object.prototype`
    // so class instances, Buffers, Arrays, koffi-managed structs
    // (with non-Object prototypes) fall through to direct-assign.
    b.line("} else if (typeof value === 'object'");
    b.indent();
    b.line("&& Object.getPrototypeOf(value) === Object.prototype) {");
    b.dedent();
    b.indent();
    b.line("_applyOpts(struct[field], value, path + '.' + field);");
    b.dedent();
    b.line("} else {");
    b.indent();
    b.line("struct[field] = value;");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
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

/// `_azStringDecode(az)`: decode a koffi-decoded `AzString` value into a JS
/// string. Field names come from the IR (`String { vec: U8Vec { ptr, len,
/// .. } }`), and the byte copy goes through the runtime adapter's
/// `readBytes`, so the same helper serves Node/koffi, Bun and Deno.
fn emit_az_string_decode(b: &mut CodeBuilder, ir: &CodegenIR) {
    let (vec_field, ptr_field, len_field) = string_layout(ir).unwrap_or_else(|| {
        ("vec".to_string(), "ptr".to_string(), "len".to_string())
    });
    b.line("// Decode an AzString value (koffi-decoded struct) into a JS string.");
    b.line("// Does not free the AzString; callers that own it delete it afterwards.");
    b.line("function _azStringDecode(az) {");
    b.indent();
    b.line("if (az == null) return '';");
    b.line(&format!("const v = az.{};", vec_field));
    b.line("if (v == null) return '';");
    b.line(&format!("const len = Number(v.{});", len_field));
    b.line(&format!("if (len <= 0 || v.{} == null) return '';", ptr_field));
    b.line(&format!(
        "return new TextDecoder().decode(azulFFI.readBytes(v.{}, len));",
        ptr_field
    ));
    b.dedent();
    b.line("}");
    b.blank();
}

/// `(vec_field, ptr_field, len_field)` of the IR's `String` struct.
fn string_layout(ir: &CodegenIR) -> Option<(String, String, String)> {
    let s = string_struct(ir)?;
    let vec_field = s.fields.first()?;
    let vec = ir.find_struct(vec_field.type_name.trim())?;
    let ptr = vec.fields.first()?;
    let len = vec.fields.get(1)?;
    Some((vec_field.name.clone(), ptr.name.clone(), len.name.clone()))
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

/// Unit-only enums are exposed as frozen constant tables, not classes.
fn is_unit_only_enum(e: &EnumDef) -> bool {
    !e.is_union
        && e.variants
            .iter()
            .all(|v| matches!(v.kind, EnumVariantKind::Unit))
}

/// The JS wrapper class name for an IR type, if this module emits one.
/// Mirrors exactly the conditions under which `emit_struct_wrapper` /
/// `emit_enum_wrapper` produce a `class`: a struct passes
/// [`should_emit_struct`] and has at least one function; a data-bearing
/// enum passes [`should_emit_enum`]. Used to wrap by-value returns.
fn node_wrapper_class_for(type_name: &str, ir: &CodegenIR) -> Option<String> {
    let t = type_name.trim();
    if let Some(s) = ir.find_struct(t) {
        if should_emit_struct(s) && ir.functions_for_class(&s.name).next().is_some() {
            return Some(sanitize_export_name(&s.name));
        }
        return None;
    }
    if let Some(e) = ir.find_enum(t) {
        if should_emit_enum(e) && !is_unit_only_enum(e) {
            return Some(sanitize_export_name(&e.name));
        }
    }
    None
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
    // the static factories below (`create()`, `createDefault()`, etc.).
    b.line("/**");
    b.line(" * Wrap an existing FFI value (takes ownership for GC purposes).");
    b.line(" * Most callers should use the static factory methods instead.");
    b.line(" * @param {*} ptr koffi struct value / raw FFI pointer of a native value");
    b.line(" */");
    b.line("constructor(ptr) {");
    b.indent();
    b.line("this._ptr = ptr;");
    if has_delete {
        b.line(&format!("{}._registry.register(this, ptr, this);", class));
    }
    b.dedent();
    b.line("}");
    b.blank();

    // Raw accessor.
    b.line("/** Return the underlying FFI value. Use with care. */");
    b.line("get raw() { return this._ptr; }");
    b.blank();

    // Idiomatic console.log output.
    b.line(&format!(
        "[Symbol.for('nodejs.util.inspect.custom')]() {{ return `{} {{ ptr: ${{this._ptr}} }}`; }}",
        class
    ));
    b.blank();

    // Fluent `.with(opts)` builder. Recursively assigns nested object
    // literals into the underlying koffi struct's fields (keys in C
    // spelling or lowerCamel), auto-converting JS strings to AzString
    // and unwrapping wrapper instances via `_ptr`. Returns `this`.
    // Calling on a wrapper whose `_ptr` is an opaque pointer (not a
    // koffi struct) is a user error and throws.
    b.line("/**");
    b.line(" * Fluent builder: recursively assign `opts` into the wrapper's");
    b.line(" * underlying koffi struct fields. Keys may be spelled as in the C");
    b.line(" * header (`window_state`) or lowerCamel (`windowState`); unknown");
    b.line(" * keys throw. JS strings auto-convert to AzString. Returns this.");
    b.line(" */");
    b.line("with(opts) {");
    b.indent();
    b.line(&format!("_applyOpts(this._ptr, opts, '{}');", class));
    b.line("return this;");
    b.dedent();
    b.line("}");
    b.blank();

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a JS string (`_ptr` is the koffi-decoded struct
    // value, so the field walk happens in JS; no re-decode).
    if matches!(s.category, TypeCategory::String) {
        b.line("/** Decode the wrapped UTF-8 bytes into a JS string. */");
        b.line("toString() {");
        b.indent();
        b.line("return _azStringDecode(this._ptr);");
        b.dedent();
        b.line("}");
        b.blank();
    }

    // Phase J.1 (Node): shared detector. Emit `<smart>(data, fn)` for
    // every method matching with_on_*(self, RefAny, <CallbackWrapper>).
    // `withOnClick(data, fn)` itself already accepts a plain function
    // (see `emit_callback_register_lines`); the smart sibling is the
    // shorter spelling every binding offers.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) = smart_callback_setter_info(func) else {
            continue;
        };
        let smart = js_method_name(&smart_snake);
        let target = js_method_name(&func.method_name);
        b.line("/**");
        b.line(&format!(
            " * Smart builder for {}: JS value + handler fn. Host-invoker",
            target
        ));
        b.line(" * registration is hidden.");
        b.line(" */");
        b.line(&format!("{}(data, fn) {{", smart));
        b.indent();
        b.line(&format!(
            "return this.{}(data, registerCallback('{}', fn));",
            target, wrapper_kind
        ));
        b.dedent();
        b.line("}");
        b.blank();
    }

    // Layout-callback factory pattern (shared detector): the class has a
    // `_default` factory and a 1-arg constructor taking a host-invoker
    // callback *typedef* (raw fn pointer, no ctx slot at the C ABI).
    // `emit_static_factory` emits that constructor under its api.json
    // name with a body that registers the JS function through the
    // host-invoker table and splices the `{cb, ctx}` struct into the
    // default value's nested field. Everything (class, default factory,
    // wrapper kind, field path) is IR-derived.
    let factory_info = layout_callback_factory_info(s, ir);

    // Methods: Method, MethodMut, DeepCopy, DebugToString, plus static
    // factories (Constructor, StaticMethod, Default, EnumVariantConstructor).
    //
    // A raw `toString` alias would collide with the decoded `toString()`
    // this file emits elsewhere: AzString's UTF-8 decode above, and the
    // Phase I.3.4 `Az<X>_toDbgString`-routed variant below. Duplicate
    // class members are legal JS but the LATER definition silently wins,
    // so the alias must be skipped whenever a decoded toString will be
    // emitted (it is strictly better: it decodes the AzString instead of
    // returning the raw koffi struct).
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let emits_decoded_tostring = matches!(s.category, TypeCategory::String)
        || (s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym));
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
                if !emits_decoded_tostring {
                    emit_instance_alias(b, f, "toString", &class);
                }
                emitted_any = true;
            }
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                emit_static_factory(b, f, &class, ir, factory_info.as_ref());
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
    emit_node_to_string_if_supported(b, s, ir);

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
    if is_unit_only_enum(e) {
        b.line(&format!(
            "// {0} is a unit-only enum; numeric constants live on Enums.{0}.",
            e.name
        ));
        b.line(&format!("const {0} = Enums.{0};", class));
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
        b.line(&format!("{}._registry.register(this, ptr, this);", class));
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
            b.line(&format!(
                "return this.tag() === {}.Tag.{};",
                class,
                sanitize_js_identifier(&v.name)
            ));
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
                emit_static_factory(b, f, &class, ir, None);
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
///   - Primitive element (`u8`/`i32`/`f64`/...): `buf[i]` is a JS Number — value-decoded by koffi,
///     fully independent.
///   - Wrapper-class element with `_deepCopy`: clone each element via
///     `lib.Az<Elem>_deepCopy(buf[i])`, wrap in `new <Elem>(__cloned)`. Safe past the Vec being
///     closed.
///   - Fallback (no clone): yield `buf[i]` with a doc comment warning the user not to retain past
///     Vec lifetime.
fn emit_node_iterator_if_vec(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if s.fields.len() != 4 {
        return;
    }
    if s.fields[0].name != "ptr" || s.fields[1].name != "len" || s.fields[2].name != "cap" {
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
    let has_clone = node_has_clone(&elem_ty, ir);
    let wrapper_class = node_wrapper_class_for(&elem_ty, ir)
        .filter(|_| node_has_delete(&elem_ty, ir));

    b.line("/**");
    if is_primitive {
        b.line(" * Iterate the underlying Vec — yields Number-typed");
        b.line(" * primitive elements decoded by-value (safe past close).");
    } else if has_clone && wrapper_class.is_some() {
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
    match (is_primitive, has_clone, wrapper_class) {
        (true, _, _) => b.line("yield buf[i];"),
        (false, true, Some(class)) => {
            b.line(&format!(
                "const __cloned = lib.Az{}_deepCopy(buf[i]);",
                elem_ty
            ));
            b.line(&format!("yield new {}(__cloned);", class));
        }
        _ => b.line("yield buf[i];"),
    }
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.blank();
}

/// Phase I.3.4 (Node): emit `toString()` instance method routed
/// through `Az<X>_toDbgString`. Decodes the returned AzString to a JS
/// string via `_azStringDecode` and frees it. Skips AzString itself.
fn emit_node_to_string_if_supported(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let string_delete = string_struct(ir)
        .filter(|st| node_has_delete(&st.name, ir))
        .map(|st| format!("lib.{}_delete(__s);", ffi_type_name(&st.name)));
    b.line(&format!("/** String repr routed through {}. */", dbg_sym));
    b.line("toString() {");
    b.indent();
    b.line("if (this._ptr == null) return '<disposed>';");
    b.line(&format!("const __s = lib.{}(this._ptr);", dbg_sym));
    b.line("const __out = _azStringDecode(__s);");
    if let Some(del) = string_delete {
        // The returned AzString is owned by us: free its U8Vec buffer.
        // koffi encodes the decoded JS object into a temporary and
        // passes its address, which is all the destructor needs.
        b.line(&del);
    }
    b.line("return __out;");
    b.dedent();
    b.line("}");
    b.blank();
}

/// Phase I.2.6 (Node): emit `equals(other)` instance method routed
/// through `Az<X>_partialEq` when TypeTraits flags it and the C export
/// exists. Pure type-driven; no method-name allowlist.
fn emit_node_equals_if_supported(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR, class: &str) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == eq_sym);
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
    b.line("equals(other) {");
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
    use super::super::ir::MonomorphizedKind;
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
    ir.find_struct(payload_ty)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// Emit the body of an Option/Result return inline. `_ret` is the
/// koffi-decoded outer struct already declared.
///
/// Three payload shapes (mirrors JVM 75a1fbcd2):
///   - AzString → decode `payload.vec.{ptr,len}` bytes into a JS string via `_azStringDecode`,
///     then call `Az<Outer>_delete(_ret)` to free the embedded buffer.
///   - Wrapper-class → call `Az<Payload>_deepCopy(payload)` for an independent allocation, wrap
///     in the JS wrapper class, then `Az<Outer>_delete(_ret)` drops the original.
///   - Primitive / other → capture the value, then `_delete`.
///
/// koffi auto-encodes JS objects into temp buffers when passing to
/// struct-pointer params, so `lib.Az<Outer>_delete(_ret)` works
/// even though `_ret` is a JS object rather than a raw pointer.
fn emit_node_option_result_body(b: &mut CodeBuilder, info: &NodeOptResultInfo, ir: &CodegenIR) {
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

    let payload_wrapper = node_wrapper_class_for(&info.payload_ty, ir);
    if node_payload_is_string(&info.payload_ty, ir) {
        // AzString payload — decode into a JS string, then delete the
        // outer to free the buffer.
        b.line(&format!("const __out = _azStringDecode({});", payload_path));
        if !outer_delete.is_empty() {
            b.line(&outer_delete);
        }
        b.line("return __out;");
    } else if let (Some(class), true) = (payload_wrapper, node_has_clone(&info.payload_ty, ir)) {
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
        b.line(&format!("return new {}(__cloned);", class));
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

/// Emit the post-call ownership bookkeeping shared by every method
/// shape: consume by-value wrapper args, and — when the receiver is
/// `self` by value — unregister `this` and null its `_ptr`.
fn emit_consume_lines(
    b: &mut CodeBuilder,
    consumed_args: &[String],
    receiver_consumed: bool,
    class: &str,
    has_delete: bool,
) {
    for n in consumed_args {
        b.line(&format!("_consume({});", n));
    }
    if receiver_consumed {
        if has_delete {
            b.line(&format!("{}._registry.unregister(this);", class));
        }
        b.line("this._ptr = null;");
    }
}

fn emit_instance_method(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class: &str,
    has_delete: bool,
    ir: &CodegenIR,
) {
    let method = js_method_name(&f.method_name);
    let user_args = user_args(f);
    let params = render_params(&user_args);
    let call_args = render_call_args(&user_args, ir);
    // `self` by value: the C call moves the struct out of `_ptr`.
    let receiver_consumed = f
        .args
        .iter()
        .find(|a| f.is_receiver_arg(a))
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let consumed_args = consumed_wrapper_args(&user_args, ir);
    // Phase I.5.4 (Node): Option/Result auto-unwrap at the wrapper
    // boundary. Detect by variant-shape — same predicate the
    // JVM/Ruby/Lua bindings use. Inline the extraction so each call
    // site can call the per-type `_delete` (and per-payload `_clone`
    // for wrapper payloads).
    let opt_or_result_info = classify_option_result_node(f, ir);
    let return_wrapper = f
        .return_type
        .as_deref()
        .and_then(|rt| node_wrapper_class_for(rt, ir));

    if !f.doc.is_empty() {
        b.line("/**");
        for d in &f.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(&format!(
            " * Wraps `lib.{}` with `this` bound as the receiver.",
            f.c_name
        ));
        b.line(" */");
    }
    b.line(&format!("{}({}) {{", method, params));
    b.indent();
    emit_callback_register_lines(b, f, &user_args);
    let mut call = format!("lib.{}(this._ptr", f.c_name);
    if !call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&call_args);
    }
    call.push(')');

    if f.return_type.is_none() {
        // Side-effecting call. `&mut self` receivers are bound as
        // `_Inout_ T *` (functions.rs) so koffi writes the mutated
        // struct back into `_ptr`; returning `this` makes mutators
        // chainable. A consumed (`self` by value) receiver cannot be
        // returned, its `_ptr` is gone.
        b.line(&format!("{};", call));
        emit_consume_lines(b, &consumed_args, receiver_consumed, class, has_delete);
        if !receiver_consumed {
            b.line("return this;");
        }
    } else if let Some(info) = opt_or_result_info.as_ref() {
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, receiver_consumed, class, has_delete);
        emit_node_option_result_body(b, info, ir);
    } else if let Some(wrapper) = return_wrapper {
        // By-value return of a wrapped type: the caller owns it.
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, receiver_consumed, class, has_delete);
        b.line(&format!("return new {}(_ret);", wrapper));
    } else {
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, receiver_consumed, class, has_delete);
        b.line("return _ret;");
    }

    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_instance_alias(b: &mut CodeBuilder, f: &FunctionDef, alias: &str, class: &str) {
    // DeepCopy (`clone`) and DebugToString (`toString`) take only the
    // receiver on the C side. The IR spells that receiver arg
    // `instance`, which `user_args` doesn't filter (it only knows
    // `self` / the lowercased class name) — emitting it produced a
    // phantom `instance` parameter that was passed as an extra FFI arg
    // (harmless only because koffi ignores extras, and wrong on
    // Bun/Deno). Aliases are receiver-only by construction, so emit a
    // zero-parameter method that calls with just `this._ptr`.
    b.line(&format!(
        "/** Idiomatic alias dispatching to `lib.{}`. */",
        f.c_name
    ));
    b.line(&format!("{}() {{", alias));
    b.indent();
    let call = format!("lib.{}(this._ptr)", f.c_name);

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
/// owned-by-value arg — except `RefAny` args, which are never the
/// user's wrapper: `render_call_args` creates a fresh host-handle
/// RefAny from the user's JS value, and that value must stay intact.
fn consumed_wrapper_args(args: &[&FunctionArg], ir: &CodegenIR) -> Vec<String> {
    args.iter()
        .filter(|a| matches!(a.ref_kind, ArgRefKind::Owned) && !is_refany_type(&a.type_name, ir))
        .map(|a| js_arg_name(a))
        .collect()
}

/// Is `f` the 1-arg callback constructor that
/// [`layout_callback_factory_info`] matched for its class? Same
/// predicate as the shared helper's `create_func` scan (constructor /
/// static method, one arg carrying `callback_info` of the detected
/// wrapper kind, returns the class).
fn is_layout_callback_factory_fn(f: &FunctionDef, info: &LayoutCallbackFactoryInfo) -> bool {
    matches!(f.kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
        && f.class_name == info.class_name
        && f.args.len() == 1
        && f.args[0]
            .callback_info
            .as_ref()
            .is_some_and(|c| c.callback_wrapper_name == info.callback_wrapper)
        && f.return_type.as_deref().map(str::trim) == Some(f.class_name.as_str())
}

/// Is `f` the constructor of a host-invoker callback wrapper struct
/// from its own raw fn-pointer typedef (`AzCallback_create(AzCallbackType)`,
/// `AzLayoutCallback_create(AzLayoutCallbackType)`)? For a JS function
/// the ctx-carrying constructor is `registerCallback(kind, fn)`; the
/// raw C entry would build `{cb, ctx: None}` and never reach JS.
fn is_own_wrapper_constructor(f: &FunctionDef) -> bool {
    HOST_INVOKER_KINDS.contains(&f.class_name.as_str())
        && matches!(f.kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
        && f.args.len() == 1
        && f.args[0]
            .callback_info
            .as_ref()
            .is_some_and(|c| c.callback_wrapper_name == f.class_name)
        && f.return_type.as_deref().map(str::trim) == Some(f.class_name.as_str())
}

fn emit_static_factory(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class_name: &str,
    ir: &CodegenIR,
    factory_info: Option<&LayoutCallbackFactoryInfo>,
) {
    let method = js_method_name(&f.method_name);
    let user_args = user_args(f);
    let params = render_params(&user_args);

    if !f.doc.is_empty() {
        b.line("/**");
        for d in &f.doc {
            b.line(&format!(" * {}", jsdoc_escape(d)));
        }
        b.line(&format!(" * Wraps `lib.{}`.", f.c_name));
        b.line(" */");
    }

    // Layout-callback factory (`WindowCreateOptions.create(layoutFn)`):
    // the C entry takes a bare fn pointer and would drop the host-handle
    // ctx, so build the value from `_default()` and splice the registered
    // `{cb, ctx}` struct into the IR-derived field path instead.
    if let Some(info) = factory_info.filter(|i| is_layout_callback_factory_fn(f, i)) {
        let arg = js_arg_name(user_args[0]);
        b.line(&format!("static {}({}) {{", method, arg));
        b.indent();
        b.line("// Registers the JS function through the host-invoker table and");
        b.line("// splices the {cb, ctx} struct into the default value; the raw");
        b.line(&format!(
            "// `lib.{}` entry has no ctx slot and could never call back into JS.",
            f.c_name
        ));
        b.line(&format!(
            "const cb = registerCallback('{}', {});",
            info.callback_wrapper, arg
        ));
        b.line(&format!("const opts = lib.{}();", info.default_c_name));
        b.line(&format!("opts.{} = cb;", info.field_path.join(".")));
        b.line(&format!("return new {}(opts);", class_name));
        b.dedent();
        b.line("}");
        b.blank();
        return;
    }

    // `Callback.create(fn)` / `LayoutCallback.create(fn)`: the ctx-
    // carrying constructor for a JS function IS the host-invoker
    // registration.
    if is_own_wrapper_constructor(f) {
        let arg = js_arg_name(user_args[0]);
        b.line(&format!("static {}({}) {{", method, arg));
        b.indent();
        b.line(&format!(
            "return new {}(registerCallback('{}', {}));",
            class_name, f.class_name, arg
        ));
        b.dedent();
        b.line("}");
        b.blank();
        return;
    }

    let call_args = render_call_args(&user_args, ir);
    // Any wrapper-typed owned-by-value arg has its bytes transferred
    // to Rust by the C call; the caller's JS wrapper would otherwise
    // double-drop on FinalizationRegistry sweep.
    let consumed_args = consumed_wrapper_args(&user_args, ir);
    let opt_or_result_info = classify_option_result_node(f, ir);
    let return_wrapper = f
        .return_type
        .as_deref()
        .and_then(|rt| node_wrapper_class_for(rt, ir));

    b.line(&format!("static {}({}) {{", method, params));
    b.indent();
    emit_callback_register_lines(b, f, &user_args);
    let call = format!("lib.{}({})", f.c_name, call_args);
    if f.return_type.is_none() {
        b.line(&format!("{};", call));
        emit_consume_lines(b, &consumed_args, false, class_name, false);
    } else if let Some(info) = opt_or_result_info.as_ref() {
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, false, class_name, false);
        emit_node_option_result_body(b, info, ir);
    } else if let Some(wrapper) = return_wrapper {
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, false, class_name, false);
        b.line(&format!("return new {}(_ret);", wrapper));
    } else {
        b.line(&format!("const _ret = {};", call));
        emit_consume_lines(b, &consumed_args, false, class_name, false);
        b.line("return _ret;");
    }
    b.dedent();
    b.line("}");
    b.blank();
}

/// For every arg whose IR `callback_info` is in the host-invoker
/// allowlist, emit the line that turns a plain JS function into what
/// the C entry point accepts:
///
/// - wrapper-struct arg (`on_click: ButtonOnClickCallback`; the binding links the `<c_name>Struct`
///   twin, see `managed_c_symbol`): `name = registerCallback('Kind', name);`
/// - raw typedef arg (`callback: CallbackType`) with no ctx-carrying C twin: a JS function cannot
///   be routed (the C side builds `{cb, ctx: None}` and the thunk finds no host handle), so passing
///   one throws a descriptive TypeError instead of a koffi "expected void *" or a silent no-op.
///   Native pointers still pass through.
fn emit_callback_register_lines(b: &mut CodeBuilder, f: &FunctionDef, args: &[&FunctionArg]) {
    for a in args {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        if !HOST_INVOKER_KINDS.contains(&wrapper) {
            continue;
        }
        let name = js_arg_name(a);
        if a.type_name.trim() == wrapper {
            b.line(&format!(
                "{n} = registerCallback('{w}', {n});",
                n = name,
                w = wrapper
            ));
        } else {
            b.line(&format!("if (typeof {} === 'function') {{", name));
            b.indent();
            b.line(&format!(
                "throw new TypeError('{}.{}: `{}` is a bare C function pointer ({}) at the C ABI; \
                 there is no ctx slot to route a JS function through. Pass a native pointer, or \
                 use an API that takes a {} struct.');",
                sanitize_export_name(&f.class_name),
                js_method_name(&f.method_name),
                name,
                ffi_type_name(&cb.callback_typedef_name),
                ffi_type_name(wrapper)
            ));
            b.dedent();
            b.line("}");
        }
    }
}

// ============================================================================
// Argument helpers
// ============================================================================

fn user_args(f: &FunctionDef) -> Vec<&FunctionArg> {
    f.args.iter().filter(|a| !f.is_receiver_arg(a)).collect()
}

fn render_params(args: &[&FunctionArg]) -> String {
    args.iter()
        .map(|a| js_arg_name(a))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_call_args(args: &[&FunctionArg], ir: &CodegenIR) -> String {
    args.iter()
        .map(|a| {
            // Auto-string-conversion (type-driven; no method-name allow-
            // list): Owned `String` args route through `_azString` so
            // plain JS strings get converted to AzString in line. The
            // helper is a pass-through for already-AzString values.
            let n = js_arg_name(a);
            if is_az_string_owned_arg(a, ir) {
                return format!("_azString({n})", n = n);
            }
            // `RefAny` args: the user hands over any JS value; it is
            // stashed in the host-handle table and a fresh RefAny that
            // points at it is what crosses the FFI.
            if is_refany_type(&a.type_name, ir) {
                return format!("refanyCreate({n})", n = n);
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
/// arg of the IR's `String` type accepts a plain JS string at the
/// wrapper level. The call site routes the value through `_azString`
/// (emitted in the module preamble).
fn is_az_string_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned)
        && ir
            .find_struct(a.type_name.trim())
            .is_some_and(|s| matches!(s.category, TypeCategory::String))
}

fn jsdoc_escape(s: &str) -> String {
    s.replace("*/", "* /")
}
