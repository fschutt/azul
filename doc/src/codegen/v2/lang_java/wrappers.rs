//! AutoCloseable wrapper-class emission for the Java JNA generator.
//!
//! For every IR struct that `managed_lang_helpers::has_wrapper_class`
//! accepts — it has a `<TypeName>_delete` C function OR at least one
//! instance method — we emit a `public final class <TypeName> implements
//! AutoCloseable` that:
//!
//! - Holds the underlying JNA `Pointer` in a private field
//! - Provides `close()` calling `AzulNative.INSTANCE.Az<Type>_delete(ptr)` when the type has a
//!   `_delete` (`has_delete_function`); types without one (engine-borrowed `CallbackInfo`, POD
//!   value types) get a no-op `close()` and no finalizer
//! - Surfaces every non-trait method on the IR class as either an instance method (`fn(self, ...)`)
//!   or a `public static` factory (`fn() -> Self`)
//! - Adds idiomatic siblings derived from shared IR descriptors: typed `<T> withOn<Event>(T,
//!   <Cb>WithData<T>)` builders (`smart_callback_setter_info`), `create(<SAM>)` / `create()`
//!   factories (`layout_callback_factory_info`) and the application-object `create(T,
//!   <Cb>WithData<T>)` (`app_factory_info`)
//!
//! Tagged-union enums get a separate, very minimal helper class with
//! static factories per unit variant. (Payload-bearing variants are
//! left for the user to construct via the generated `Az<Type>` Union
//! plus the matching `<Type>Variant_<Variant>` payload struct — see
//! `types.rs`.)

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, FunctionArg,
            FunctionDef, FunctionKind, MonomorphizedKind, StructDef, TypeCategory,
        },
        managed_host_invoker::{
            app_factory_info, layout_callback_factory_info, managed_c_symbol,
            smart_callback_setter_info, AppFactoryInfo,
        },
        managed_lang_helpers::{has_delete_function, has_wrapper_class, is_refany_type, takes_self},
    },
    derives, emit_file, ffi_type_name, javadoc_escape, map_jvm_type_byvalue,
    managed::data_typed_sam_for_kind, sanitize_identifier, snake_to_lower_camel,
    types::{java_boxed, ref_kind_field_type},
};

// ============================================================================
// Top-level driver
// ============================================================================

pub fn emit_all_wrapper_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    // The application-object shape is matched once for the whole IR.
    let app_info = app_factory_info(ir);
    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        let class_name = wrapper_class_name(&s.name);
        let chunk = emit_file(
            &format!("{}.java", class_name),
            |b| {
                emit_wrapper_class(b, s, ir, config, app_info.as_ref());
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        let helper_name = format!("{}Helpers", wrapper_class_name(&e.name));
        let chunk = emit_file(
            &format!("{}.java", helper_name),
            |b| {
                emit_union_helper(b, e, ir, config);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    Ok(())
}

// ============================================================================
// Filters
// ============================================================================

/// Per-target inclusion + the shared wrapper predicate. The same
/// `has_wrapper_class` decides, at every use site (arg conversion,
/// return wrapping, typed SAMs, Iterable elements), whether `new
/// <X>(Pointer)` exists — so those sites can never reference a class
/// this driver did not emit.
fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name) && has_wrapper_class(&s.name, ir)
}

fn should_emit_union_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    if matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    ) {
        return false;
    }
    e.is_union
}

// ============================================================================
// Wrapper emission
// ============================================================================

fn emit_wrapper_class(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    app_info: Option<&AppFactoryInfo>,
) {
    let has_delete = has_delete_function(&s.name, ir);
    let class_name = wrapper_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        builder.line("/**");
        for d in &s.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    // Phase I.1.2 (Java): if this wrapper's underlying struct matches
    // the Vec shape (ptr/len/cap/destructor), declare `implements
    // Iterable<T>` so user code can write `for (T x : vec)`.
    let vec_elem_type = detect_vec_elem_type_jvm(s);
    // Iterable only when the element type is an emitted struct
    // wrapper class — skip enum/typedef elements (`IdOrClass`,
    // `DynamicSelector`, etc.) which don't get their own class.
    // Every wrapper is AutoCloseable so `try (...)` works uniformly;
    // `close()` only frees when the type has a `_delete` (see
    // `emit_close_method`).
    let mut interfaces = match &vec_elem_type {
        Some(elem) if has_wrapper_class(elem, ir) => {
            format!("AutoCloseable, Iterable<{}>", wrapper_class_name(elem))
        }
        _ => "AutoCloseable".to_string(),
    };
    // A type whose api.json traits give it an ordering is Comparable, routed
    // through the same `_cmp` / `_partialCmp` export the FFI value class uses.
    if let Some(cmp) = derives::wrapper_comparable_interface(&s.name, ir, config) {
        interfaces.push_str(", ");
        interfaces.push_str(&cmp);
    }
    builder.line(&format!(
        "public final class {} implements {} {{",
        class_name, interfaces
    ));
    builder.indent();

    builder.line("private Pointer ptr;");
    builder.line("private boolean closed;");
    // `owned == false`: the pointer belongs to the engine (callback
    // args, Vec elements yielded without `_clone`); close() must never
    // `_delete` it, only invalidate the wrapper.
    builder.line("private boolean owned;");
    builder.blank();

    // Internal pointer-wrapping constructor (package-private).
    if has_delete {
        builder.line(&format!(
            "/** Wrap an existing native {} pointer; takes ownership (freed by close()). */",
            ffi_name
        ));
    } else {
        builder.line(&format!(
            "/** Wrap an existing native {} pointer; BORROWED — the engine (or the enclosing \
             value) owns it and there is no {}_delete, so close() only marks the wrapper closed. */",
            ffi_name, ffi_name
        ));
    }
    builder.line(&format!(
        "{}(Pointer ptr) {{ this.ptr = ptr; this.closed = false; this.owned = true; }}",
        class_name
    ));
    builder.blank();
    builder.line(&format!(
        "/** Internal: wrap a pointer the ENGINE owns (a callback argument, a Vec element) — \
         never freed by this wrapper; the codegen bridge invalidates it with close() when the \
         engine's borrow ends. */"
    ));
    builder.line(&format!(
        "static {} __borrow(Pointer ptr) {{ {} w = new {}(ptr); w.owned = false; return w; }}",
        class_name, class_name, class_name
    ));
    builder.blank();

    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("public Pointer rawPointer() { return ptr; }");
    builder.blank();

    // The api.json constants declared on this class (the OpenGL enum values
    // on the GL context, ...) belong on the user-facing class, not on the raw
    // JNA struct.
    derives::emit_constants(builder, &s.name, ir);

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a `java.lang.String`. AzString's C-side layout
    // is `{ vec: AzU8Vec }`, and AzU8Vec is
    // `{ ptr: u8*, len: usize, cap: usize, destructor: AzU8VecDestructor }`.
    // The wrapper's `ptr` is the address of the AzString struct, so
    // offset 0 is `vec.ptr` (the UTF-8 byte buffer) and offset 8 is
    // `vec.len` (byte length).
    if matches!(s.category, TypeCategory::String) {
        builder.line("/**");
        builder.line(" * Decode the wrapped UTF-8 bytes into a `java.lang.String`.");
        builder.line(" * Reads `vec.ptr` (offset 0) and `vec.len` (offset 8) from");
        builder.line(" * the AzString struct directly via JNA.");
        builder.line(" */");
        builder.line("@Override");
        builder.line("public java.lang.String toString() {");
        builder.indent();
        builder.line("if (ptr == null || closed) return \"\";");
        builder.line("Pointer vecPtr = ptr.getPointer(0);");
        builder.line("long vecLen = ptr.getLong(8);");
        builder.line("if (vecPtr == null || vecLen <= 0) return \"\";");
        builder.line("byte[] bytes = vecPtr.getByteArray(0, (int) vecLen);");
        builder
            .line("return new java.lang.String(bytes, java.nio.charset.StandardCharsets.UTF_8);");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Phase J.1: any method with the `with_on_*(self, data: RefAny,
    // callback: <Cb>)` shape gets idiomatic siblings that register the
    // host handles internally. The detector (`smart_callback_setter_info`)
    // returns Some((smart_name, kind)) when the method matches the
    // pattern AND the wrapper kind is in HOST_INVOKER_KINDS — so
    // Button.withOnClick, CheckBox.withOnToggle, TextInput.withOnTextInput,
    // DropDown.withOnChoiceChange, ... all light up.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) = smart_callback_setter_info(func) else {
            continue;
        };
        emit_smart_callback_setters(
            builder,
            &class_name,
            func,
            &smart_snake,
            &wrapper_kind,
            ir,
            config,
        );
    }

    // Smart factories for a class with a `_default` factory and a
    // `create(<layout-callback fn ptr>)` constructor (today:
    // WindowCreateOptions): `create(<SAM>)` registers the SAM via
    // AzulHostInvoker and splices the resulting Az<Cb> bytes into the
    // `_default()` struct's embedded callback field. Replaces the manual:
    //
    //     AzLayoutCallback.ByValue cb = AzulHostInvoker.registerLayoutCallback(fn);
    //     AzWindowCreateOptions.ByValue wco = AzulNativeWindow.AzWindowCreateOptions_default();
    //     cb.write(); wco.write();
    //     wco.window_state.layout_callback.getPointer().write(0, cb.getPointer().getByteArray(0,
    // cb.size()), 0, cb.size());     wco.read();
    //
    // boilerplate every JVM hello-world had.
    if let Some(info) = layout_callback_factory_info(s, ir) {
        let wrapper_class = wrapper_class_name(&info.class_name);
        let ffi_class = ffi_type_name(&info.class_name);
        let cb_ffi = ffi_type_name(&info.callback_wrapper);
        let register_fn = format!("register{}", info.callback_wrapper);
        let native_class = super::functions::native_class_for_class(&info.class_name, ir);
        let field_path = info.field_path.join(".");
        let sam_raw = format!("AzulNativeManaged.{}InvokerCallback", info.callback_wrapper);
        let sam_typed = format!("AzulHostInvoker.{}", info.callback_wrapper);

        // Emit two overloads — raw (4-arg outPtr-write) and typed
        // (returns wrapper struct directly). Both bodies are derived
        // from the same factory info; differ only in the SAM type.
        for (sam_type, doc_note) in [
            (
                sam_raw.as_str(),
                "Smart factory: pass a layout-callback lambda; the host-invoker registration and \
                 bytes-copy plumbing happen internally.",
            ),
            (
                sam_typed.as_str(),
                "Smart factory (typed): pass a typed callback that returns a wrapper struct \
                 directly; the bridge splices the bytes into the embedded callback field.",
            ),
        ] {
            builder.line("/**");
            builder.line(&format!(" * {}", doc_note));
            builder.line(" */");
            builder.line(&format!(
                "public static {} create({} fn) {{",
                wrapper_class, sam_type
            ));
            builder.indent();
            builder.line(&format!(
                "{}.ByValue __cb = AzulHostInvoker.{}(fn);",
                cb_ffi, register_fn
            ));
            builder.line(&format!(
                "{}.ByValue __wco = {}.INSTANCE.{}();",
                ffi_class, native_class, info.default_c_name
            ));
            builder.line("__cb.write();");
            builder.line("__wco.write();");
            builder.line("byte[] __cbBytes = __cb.getPointer().getByteArray(0, __cb.size());");
            builder.line(&format!(
                "__wco.{}.getPointer().write(0, __cbBytes, 0, __cbBytes.length);",
                field_path
            ));
            builder.line("__wco.read();");
            builder.line(&format!(
                "return new {}(__wco.getPointer());",
                wrapper_class
            ));
            builder.dedent();
            builder.line("}");
            builder.blank();
        }

        // Zero-arg `create()`: the `_default` factory under the
        // idiomatic name. The layout callback then comes from the
        // application object (`App.create(T, fn)` splices it into every
        // window whose callback is still the default) — or from one of
        // the overloads above. Skipped when the IR already has a
        // zero-arg factory that would take the name.
        let has_zero_arg_create = ir.functions_for_class(&s.name).any(|f| {
            !f.kind.is_trait_function()
                && !takes_self(f)
                && f.args.is_empty()
                && idiomatic_method_name(&f.method_name) == "create"
        });
        if !has_zero_arg_create {
            builder.line("/**");
            builder.line(&format!(
                " * Default options. The layout callback is supplied by the application \
                 object's typed factory (see {{@code create(T, ...WithData<T>)}} on the class \
                 whose run/addWindow takes a {}), or set one explicitly with the \
                 {{@code create(fn)}} overloads.",
                wrapper_class
            ));
            builder.line(" */");
            builder.line(&format!("public static {} create() {{", wrapper_class));
            builder.indent();
            builder.line(&format!(
                "{}.ByValue __raw = {}.INSTANCE.{}();",
                ffi_class, native_class, info.default_c_name
            ));
            builder.line(&format!("return new {}(__raw.getPointer());", wrapper_class));
            builder.dedent();
            builder.line("}");
            builder.blank();
        }
    }

    // Application-object factory (`App.create(model, layoutFn)`): see
    // `managed_host_invoker::app_factory_info`. Matched structurally;
    // nothing here names App / AppConfig / WindowCreateOptions.
    let app_emit = match app_info {
        Some(app) if app.class_name == s.name => emit_app_factory(builder, s, app, ir, config),
        _ => AppEmit::default(),
    };

    // Methods.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        // Window-taking methods of the application class splice the
        // factory-registered layout callback into their options arg.
        let splice_arg_idx = if app_emit.splice_helper {
            app_info.and_then(|app| {
                app.window_methods
                    .iter()
                    .find(|(c_name, _, _)| *c_name == func.c_name)
                    .map(|(_, idx, _)| *idx)
            })
        } else {
            None
        };
        emit_wrapper_method(builder, &class_name, func, ir, splice_arg_idx);
    }

    // Phase I.2: route Object.equals(Object) + hashCode() through the
    // codegen-emitted `_partialEq` / `_hash` C-ABI helpers when
    // TypeTraits says they're supported. Pure type-driven; falls back
    // to identity-based defaults when the helpers aren't available.
    emit_equals_hashcode_if_supported(builder, s, &class_name, ir);

    // Phase I.3 (Java): override Object.toString() through the
    // codegen-emitted `Az<X>_toDbgString` C-ABI helper when TypeTraits
    // flags `is_debug`. Existing AzString toString override is left in
    // place since it accesses the underlying U8Vec directly (no helper
    // round-trip).
    emit_to_string_if_supported(builder, s, ir);

    // Ordering through the type's `_cmp` (or, when that is all it has,
    // `_partialCmp`) export — the counterpart of the `Comparable` clause
    // added to the class declaration above.
    derives::emit_wrapper_compare_to(builder, &s.name, &class_name, ir, config);

    // Phase I.1.2 (Java): emit Iterable<T>.iterator() when the Vec
    // shape was detected AND the element has a wrapper class. The
    // body overlays AzXVec via JNA Structure.newInstance, reads
    // ptr+len, and walks the buffer one element at a time.
    if let Some(elem) = vec_elem_type.as_deref() {
        if has_wrapper_class(elem, ir) {
            emit_jvm_vec_iterator(builder, s, elem, ir);
        } else {
            // Primitive (or non-wrapper) element: emit a bulk-copy
            // sibling array method (`toByteArray()` / `toIntArray()`
            // / etc.) that uses JNA's typed `getXxxArray(0, len)`
            // primitives. One memcpy into a fresh JVM-managed array
            // — fully independent of the Vec's lifetime, and faster
            // than per-element iteration for primitive buffers.
            emit_jvm_vec_primitive_array(builder, s, elem);
        }
    }

    // close() / AutoCloseable.
    emit_close_method(builder, &s.name, ir, &app_emit.close_lines);

    builder.dedent();
    builder.line("}");
}

/// Phase J.1: for a `with_on_*(self, data: RefAny, callback: <Cb>)`
/// builder emit two idiomatic siblings next to the IR method:
///
/// * `on<Event>(Object data, AzulNativeManaged.<Cb>InvokerCallback fn)` — raw SAM;
/// * `<T> with<Event>(T data, AzulHostInvoker.<Cb>WithData<T> fn)` — typed SAM, only when
///   `managed.rs` emitted the `<Cb>WithData<T>` interface for that kind (same predicate:
///   [`data_typed_sam_for_kind`]).
///
/// Both register the host handles internally and forward to the IR
/// method. Arguments are matched positionally / by IR category — the
/// receiver is `args[0]` whenever `takes_self`, the data slot is the
/// `RefAny`-category arg, the callback slot is the `callback_info` arg;
/// anything else is passed through by name — never by the name api.json
/// gave the receiver.
fn emit_smart_callback_setters(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    smart_snake: &str,
    wrapper_kind: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let with_camel = idiomatic_method_name(&func.method_name);
    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);
    let returns_void = func.return_type.is_none();
    if !(returns_self || returns_void) {
        return;
    }
    let (ret_ty, ret_kw) = if returns_self {
        (class_name.to_string(), "return ")
    } else {
        ("void".to_string(), "")
    };
    let skip = if takes_self(func) { 1 } else { 0 };
    let cb_ffi = ffi_type_name(wrapper_kind);

    // Forwarded argument list of the IR method: the data and callback
    // slots are synthesised from the locals `__data` / `__cb`; anything
    // else is passed through by name (and added to the signature).
    let mut extra_sig: Vec<String> = Vec::new();
    let mut call: Vec<String> = Vec::new();
    for a in func.args.iter().skip(skip) {
        let tn = a.type_name.trim();
        if is_refany_type(tn, ir) {
            call.push(format!(
                "new {}(__data.getPointer())",
                wrapper_class_name(tn)
            ));
        } else if a.callback_info.is_some() {
            call.push(format!("new {}(__cb.getPointer())", wrapper_class_name(tn)));
        } else {
            let n = sanitize_identifier(&a.name);
            extra_sig.push(format!("{} {}", map_jvm_type_byvalue(tn, ir), n));
            call.push(n);
        }
    }
    let call = call.join(", ");

    // (1) Raw-SAM sibling: `onClick(Object data, <Cb>InvokerCallback fn)`.
    let smart_camel = snake_to_lower_camel(smart_snake);
    let mut sig = vec![
        "Object data".to_string(),
        format!("AzulNativeManaged.{}InvokerCallback fn", wrapper_kind),
    ];
    sig.extend(extra_sig.iter().cloned());
    builder.line("/**");
    builder.line(&format!(
        " * Smart builder for {}: takes a Java object as data and a",
        with_camel
    ));
    builder.line(" * raw SAM callback; host-invoker registration of both happens");
    builder.line(" * internally.");
    builder.line(" */");
    builder.line(&format!(
        "public {} {}({}) {{",
        ret_ty,
        smart_camel,
        sig.join(", ")
    ));
    builder.indent();
    builder.line("AzRefAny.ByValue __data = AzulHostInvoker.refanyCreate(data);");
    builder.line(&format!(
        "{}.ByValue __cb = AzulHostInvoker.register{}(fn);",
        cb_ffi, wrapper_kind
    ));
    builder.line(&format!("{}{}({});", ret_kw, with_camel, call));
    builder.dedent();
    builder.line("}");
    builder.blank();

    // (2) Typed sibling: `<T> withOnClick(T data, <Cb>WithData<T> fn)`.
    if data_typed_sam_for_kind(wrapper_kind, ir, config).is_none() {
        return;
    }
    let mut sig = vec![
        "T data".to_string(),
        format!("AzulHostInvoker.{}WithData<T> fn", wrapper_kind),
    ];
    sig.extend(extra_sig.iter().cloned());
    builder.line("/**");
    builder.line(&format!(
        " * Typed builder for {}: {{@code data}} is any Java object, {{@code fn}}",
        with_camel
    ));
    builder.line(" * receives it back as its declared type (plus the decoded callback");
    builder.line(" * args) — no RefAny / callback-struct plumbing at the call site.");
    builder.line(" * Method references work: {@code .withOnClick(model, MyApp::onClick)}.");
    builder.line(" */");
    builder.line("@SuppressWarnings(\"unchecked\")");
    builder.line(&format!(
        "public <T> {} {}({}) {{",
        ret_ty,
        with_camel,
        sig.join(", ")
    ));
    builder.indent();
    builder.line("if (data == null) throw new NullPointerException(\"data\");");
    builder.line("AzRefAny.ByValue __data = AzulHostInvoker.refanyCreate(data);");
    builder.line(&format!(
        "{}.ByValue __cb = AzulHostInvoker.register{}((Class<T>) data.getClass(), fn);",
        cb_ffi, wrapper_kind
    ));
    builder.line(&format!("{}{}({});", ret_kw, with_camel, call));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// What [`emit_app_factory`] added to the class, so the method emitter
/// and `close()` can cooperate with it.
#[derive(Default)]
struct AppEmit {
    /// `__spliceLayoutCallback(<Options>.ByValue)` overloads exist;
    /// window-taking methods call them before the native call.
    splice_helper: bool,
    /// Statements `close()` runs after `_delete` (release the stored
    /// callback).
    close_lines: Vec<String>,
}

/// C-ABI name of the trait function of `kind` on class `class_name`
/// (`Az<X>_clone`, `Az<X>_delete`, ...), taken from the IR rather than
/// re-derived from the type name.
fn trait_c_name(class_name: &str, kind: FunctionKind, ir: &CodegenIR) -> Option<String> {
    ir.functions
        .iter()
        .find(|f| f.class_name == class_name && f.kind == kind)
        .map(|f| f.c_name.clone())
}

/// Application-object factory. For the class `app_factory_info` matched
/// (constructor `[RefAny, Config-with-_default]`, methods taking a
/// window-options struct with a layout-callback field) emit:
///
/// * a private field holding the registered `Az<Cb>.ByValue`;
/// * `public static <T> App create(T data, AzulHostInvoker.<Cb>WithData<T> fn)` — registers
///   the typed layout callback once, wraps `data` as a host-handle RefAny, builds the config
///   with its `_default` factory and calls the IR constructor;
/// * one private `__spliceLayoutCallback(<Options>.ByValue)` per options type — when the
///   options still carry the DEFAULT callback (detected with the callback type's own
///   `_default` + `_partialEq` exports when it has them, else by comparing the field bytes
///   against a fresh `_default` options struct), a CLONE of the stored callback (a fresh
///   RefAny refcount per window) is written into `options.<field_path>`. An explicit
///   `<Options>.create(fn)` therefore keeps winning;
/// * a `close()` statement that `_delete`s the stored callback.
///
/// Emitted only when every window-taking method uses the same callback
/// kind and that kind has a `<Cb>WithData<T>` SAM (so the typed factory
/// can exist at all).
fn emit_app_factory(
    builder: &mut CodeBuilder,
    s: &StructDef,
    app: &AppFactoryInfo,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> AppEmit {
    let Some((_, _, first)) = app.window_methods.first() else {
        return AppEmit::default();
    };
    let kind = first.callback_wrapper.as_str();
    if !app
        .window_methods
        .iter()
        .all(|(_, _, info)| info.callback_wrapper == kind)
    {
        return AppEmit::default();
    }
    if data_typed_sam_for_kind(kind, ir, config).is_none() {
        return AppEmit::default();
    }
    let Some(create) = ir.functions.iter().find(|f| f.c_name == app.create_c_name) else {
        return AppEmit::default();
    };

    let class_name = wrapper_class_name(&s.name);
    let app_ffi = ffi_type_name(&s.name);
    let app_native = super::functions::native_class_for_class(&s.name, ir);
    let cfg_ffi = ffi_type_name(&app.config_type);
    let cfg_native = super::functions::native_class_for_class(&app.config_type, ir);
    let cb_ffi = ffi_type_name(kind);
    let cb_native = super::functions::native_class_for_class(kind, ir);
    let cb_clone = trait_c_name(kind, FunctionKind::DeepCopy, ir);
    let cb_delete = trait_c_name(kind, FunctionKind::Delete, ir);
    let cb_default = trait_c_name(kind, FunctionKind::Default, ir);
    let cb_eq = trait_c_name(kind, FunctionKind::PartialEq, ir);

    // --- field -------------------------------------------------------------
    builder.line("/**");
    builder.line(&format!(
        " * Layout callback registered by {{@code create(T, AzulHostInvoker.{}WithData<T>)}};",
        kind
    ));
    builder.line(" * spliced into every window-options struct passed to this object that still");
    builder.line(" * carries the default callback. null when the object was created otherwise.");
    builder.line(" */");
    builder.line(&format!("private {}.ByValue __layoutCallback;", cb_ffi));
    builder.blank();

    // --- typed factory -----------------------------------------------------
    builder.line("/**");
    builder.line(" * Create the application from a plain Java data model and a typed layout");
    builder.line(" * callback. {@code data} is handed back to {@code layout} (and to every");
    builder.line(" * widget callback registered with it) as its declared type; the config is");
    builder.line(&format!(
        " * the {} default. Pass the resulting object's windows through",
        cfg_ffi
    ));
    builder.line(" * {@code run(...)} / {@code addWindow(...)}: any options still carrying the");
    builder.line(" * default callback receive this one.");
    builder.line(" */");
    builder.line("@SuppressWarnings(\"unchecked\")");
    builder.line(&format!(
        "public static <T> {} create(T data, AzulHostInvoker.{}WithData<T> layout) {{",
        class_name, kind
    ));
    builder.indent();
    builder.line("if (data == null) throw new NullPointerException(\"data\");");
    builder.line("if (layout == null) throw new NullPointerException(\"layout\");");
    builder.line(&format!(
        "{}.ByValue __cb = AzulHostInvoker.register{}((Class<T>) data.getClass(), layout);",
        cb_ffi, kind
    ));
    builder.line("AzRefAny.ByValue __data = AzulHostInvoker.refanyCreate(data);");
    builder.line(&format!(
        "{}.ByValue __config = {}.INSTANCE.{}();",
        cfg_ffi, cfg_native, app.config_default_c_name
    ));
    let mut create_args = vec![String::new(); create.args.len()];
    create_args[app.data_arg_index] = "__data".to_string();
    create_args[app.config_arg_index] = "__config".to_string();
    builder.line(&format!(
        "{}.ByValue __raw = {}.INSTANCE.{}({});",
        app_ffi,
        app_native,
        managed_c_symbol(create),
        create_args.join(", ")
    ));
    builder.line(&format!(
        "{} __app = new {}(__raw.getPointer());",
        class_name, class_name
    ));
    builder.line("__app.__layoutCallback = __cb;");
    builder.line("return __app;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // --- splice helper, one per distinct options type ------------------------
    let mut seen: Vec<&str> = Vec::new();
    for (_, _, info) in &app.window_methods {
        if seen.contains(&info.class_name.as_str()) {
            continue;
        }
        seen.push(info.class_name.as_str());
        let opt_ffi = ffi_type_name(&info.class_name);
        let opt_native = super::functions::native_class_for_class(&info.class_name, ir);
        let opt_delete = trait_c_name(&info.class_name, FunctionKind::Delete, ir);
        let field_path = info.field_path.join(".");

        builder.line("/**");
        builder.line(" * Internal: write the factory-registered layout callback into");
        builder.line(&format!(
            " * {{@code options.{}}} when it still holds the default callback",
            field_path
        ));
        builder.line(" * (an explicit callback set by the caller wins). Each window gets");
        builder.line(" * its own clone, i.e. its own RefAny refcount.");
        builder.line(" */");
        builder.line(&format!(
            "private void __spliceLayoutCallback({}.ByValue __options) {{",
            opt_ffi
        ));
        builder.indent();
        builder.line("if (this.__layoutCallback == null) return;");
        builder.line(&format!(
            "Pointer __field = __options.{}.getPointer();",
            field_path
        ));
        match (&cb_default, &cb_eq) {
            (Some(default_c), Some(eq_c)) => {
                builder.line(&format!(
                    "{}.ByValue __default = {}.INSTANCE.{}();",
                    cb_ffi, cb_native, default_c
                ));
                builder.line(&format!(
                    "boolean __isDefault = {}.INSTANCE.{}(__field, __default.getPointer()) != 0;",
                    cb_native, eq_c
                ));
                if let Some(delete_c) = &cb_delete {
                    builder.line(&format!(
                        "{}.INSTANCE.{}(__default.getPointer());",
                        cb_native, delete_c
                    ));
                }
            }
            _ => {
                // No equality export on the callback type: compare the
                // raw field bytes against a fresh default options struct.
                builder.line(&format!(
                    "{}.ByValue __defaults = {}.INSTANCE.{}();",
                    opt_ffi, opt_native, info.default_c_name
                ));
                builder.line(&format!(
                    "int __n = __defaults.{}.size();",
                    field_path
                ));
                builder.line(&format!(
                    "boolean __isDefault = java.util.Arrays.equals(__field.getByteArray(0, __n), \
                     __defaults.{}.getPointer().getByteArray(0, __n));",
                    field_path
                ));
                if let Some(delete_c) = &opt_delete {
                    builder.line(&format!(
                        "{}.INSTANCE.{}(__defaults.getPointer());",
                        opt_native, delete_c
                    ));
                }
            }
        }
        builder.line("if (!__isDefault) return;");
        match &cb_clone {
            Some(clone_c) => {
                builder.line(&format!(
                    "{}.ByValue __copy = {}.INSTANCE.{}(this.__layoutCallback.getPointer());",
                    cb_ffi, cb_native, clone_c
                ));
            }
            None => {
                // Not clonable: move it into the first window; a second
                // window cannot receive the same refcount twice.
                builder.line(&format!(
                    "{}.ByValue __copy = this.__layoutCallback;",
                    cb_ffi
                ));
                builder.line("this.__layoutCallback = null;");
            }
        }
        builder.line("byte[] __bytes = __copy.getPointer().getByteArray(0, __copy.size());");
        builder.line("__field.write(0, __bytes, 0, __bytes.length);");
        builder.line("__options.read();");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    let mut close_lines = Vec::new();
    if let Some(delete_c) = &cb_delete {
        close_lines.push("if (__layoutCallback != null) {".to_string());
        close_lines.push(format!(
            "    {}.INSTANCE.{}(__layoutCallback.getPointer());",
            cb_native, delete_c
        ));
        close_lines.push("    __layoutCallback = null;".to_string());
        close_lines.push("}".to_string());
    }
    AppEmit {
        splice_helper: true,
        close_lines,
    }
}

/// Phase I.2 (Java): override Object.equals(Object) + hashCode() to
/// route through the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash`
/// C exports. Pure type-driven from `TypeTraits.is_partial_eq` /
/// `TypeTraits.is_hash`; only emits the override when the helper
/// actually exists in `ir.functions`.
fn emit_equals_hashcode_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class_name: &str,
    ir: &CodegenIR,
) {
    let native = super::functions::native_class_for_class(&s.name, ir);
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash && ir.functions.iter().any(|f| f.c_name == hash_sym);

    if has_eq {
        builder.line("/**");
        builder.line(" * Equality routed through the codegen-emitted");
        builder.line(&format!(" * {} C-ABI helper.", eq_sym));
        builder.line(" */");
        builder.line("@Override");
        builder.line("public boolean equals(Object other) {");
        builder.indent();
        builder.line(&format!(
            "if (!(other instanceof {})) return false;",
            class_name
        ));
        builder.line(&format!("{} o = ({}) other;", class_name, class_name));
        builder.line("if (this.ptr == null || o.ptr == null) return this.ptr == o.ptr;");
        // JNA maps C `bool` to `byte` on macOS/Linux (no explicit
        // @MarshalAs(U1)). Compare against zero.
        builder.line(&format!(
            "return {}.INSTANCE.{}(this.ptr, o.ptr) != 0;",
            native, eq_sym
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    if has_hash {
        builder.line("/**");
        builder.line(" * Hash routed through the codegen-emitted");
        builder.line(&format!(" * {} C-ABI helper.", hash_sym));
        builder.line(" */");
        builder.line("@Override");
        builder.line("public int hashCode() {");
        builder.indent();
        builder.line("if (ptr == null) return 0;");
        builder.line(&format!("long h = {}.INSTANCE.{}(ptr);", native, hash_sym));
        builder.line("return (int) (h ^ (h >>> 32));");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else if has_eq {
        // equals compares VALUES (the C `_partialEq`) and the type has no C
        // `_hash`: equal values must hash equal, so the only hash that keeps
        // the contract is a constant (a pointer's hash differs between two
        // equal values; a debug string may print fields equality ignores).
        builder.line("/** Constant: equal values must hash equal, and the type has no value hash. */");
        builder.line("@Override");
        builder.line("public int hashCode() {");
        builder.indent();
        builder.line("return 0;");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// Phase I.3 (Java): override Object.toString() routed through the
/// codegen-emitted `Az<X>_toDbgString` C export when TypeTraits.is_debug
/// is set and the helper actually exists. Skips when this is the String
/// wrapper class — that already has a vec-direct toString.
fn emit_to_string_if_supported(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let native = super::functions::native_class_for_class(&s.name, ir);
    builder.line("/**");
    builder.line(&format!(
        " * String representation routed through {}.",
        dbg_sym
    ));
    builder.line(" */");
    builder.line("@Override");
    builder.line("public java.lang.String toString() {");
    builder.indent();
    builder.line("if (ptr == null || closed) return super.toString();");
    builder.line(&format!(
        "AzString.ByValue __s = {}.INSTANCE.{}(ptr);",
        native, dbg_sym
    ));
    // Decode the AzString. The AzString struct's first field is a
    // U8Vec; offset 0 is vec.ptr, offset 8 is vec.len.
    builder.line("__s.write();");
    builder.line("Pointer __sp = __s.getPointer();");
    builder.line("Pointer __vecPtr = __sp.getPointer(0);");
    builder.line("long __vecLen = __sp.getLong(8);");
    builder.line("if (__vecPtr == null || __vecLen <= 0) return \"\";");
    builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
    builder.line(
        "java.lang.String __out = new java.lang.String(__bytes, \
         java.nio.charset.StandardCharsets.UTF_8);",
    );
    // Free the freshly-allocated AzString to avoid leaking the U8Vec.
    builder.line("AzulNativeStr.INSTANCE.AzString_delete(__sp);");
    builder.line("return __out;");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// Phase J.1 detector now lives in `codegen::v2::managed_host_invoker`
// as `smart_callback_setter_info` — shared across every binding.

/// Phase I.1.2 (Java): Vec-shape detector. Mirrors the Haskell H.3 /
/// Ruby I.1.6 pattern: struct fields exactly [ptr, len, cap, destructor]
/// with ptr being a `*mut|*const T` typedef. Returns the element type T.
fn detect_vec_elem_type_jvm(s: &StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    if s.fields[0].name != "ptr" || s.fields[1].name != "len" || s.fields[2].name != "cap" {
        return None;
    }
    if s.fields[1].type_name.trim() != "usize" || s.fields[2].type_name.trim() != "usize" {
        return None;
    }
    let raw = s.fields[0].type_name.trim();
    let elem = raw
        .strip_prefix("*mut ")
        .or_else(|| raw.strip_prefix("*const "))
        .map(str::trim)
        .unwrap_or(raw);
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

/// Emit `Iterable<T>.iterator()` body for a Vec wrapper.
///
/// **Memory safety**: each `next()` call clones the element via the
/// type's `Az<X>_clone` C export so the returned wrapper owns its
/// own heap allocations independent of the Vec's buffer. Without
/// the clone, every element wrapper would hold a Pointer into the
/// Vec's `ptr` buffer — closing the Vec (or GC'ing it) would free
/// that buffer and leave every yielded wrapper dangling. The
/// per-element clone cost is amortized over typical iteration; for
/// hot loops over primitive-element Vecs we fall back to the
/// codegen-emitted `toByteArray` / `toIntArray` / etc. helpers
/// which copy the whole buffer once.
///
/// If the element type doesn't expose a `_clone` C export, fall
/// back to a borrow-shape iterator that yields wrappers backed by
/// the Vec's buffer + arms the wrappers consumed so their
/// finalize-time `AzX_delete` doesn't try to free Vec-internal
/// memory.
fn emit_jvm_vec_iterator(
    builder: &mut CodeBuilder,
    s: &StructDef,
    elem_type: &str,
    ir: &CodegenIR,
) {
    let vec_ffi = ffi_type_name(&s.name);
    let elem_ffi = ffi_type_name(elem_type);
    let elem_wrapper = wrapper_class_name(elem_type);
    let clone_call = format_clone_call_jvm(elem_type, ir);

    builder.line("/**");
    builder.line(&format!(
        " * Iterate the underlying Vec yielding {} elements. Each",
        elem_wrapper
    ));
    if clone_call.is_some() {
        builder.line(" * element is deep-cloned via the type's _clone C export so the");
        builder.line(" * yielded wrapper owns its own heap allocations and survives");
        builder.line(" * the Vec being closed.");
    } else {
        builder.line(" * element is a non-owning wrapper over the Vec's buffer (no");
        builder.line(" * finalize-time AzX_delete on Vec-internal memory). Don't store");
        builder.line(" * yielded wrappers past the Vec's lifetime.");
    }
    builder.line(" */");
    builder.line("@Override");
    builder.line(&format!(
        "public java.util.Iterator<{}> iterator() {{",
        elem_wrapper
    ));
    builder.indent();
    builder.line(&format!(
        "final {}.ByValue __raw = (({}.ByValue) Structure.newInstance({}.ByValue.class, ptr));",
        vec_ffi, vec_ffi, vec_ffi
    ));
    builder.line("__raw.read();");
    builder.line("final Pointer __buf = __raw.ptr;");
    builder.line("final long __n = __raw.len;");
    builder.line(&format!(
        "final int __sz = Structure.newInstance({}.class).size();",
        elem_ffi
    ));
    builder.line(&format!(
        "return new java.util.Iterator<{}>() {{",
        elem_wrapper
    ));
    builder.indent();
    builder.line("private long __i = 0;");
    builder.line("@Override public boolean hasNext() { return __i < __n; }");
    builder.line("@Override");
    builder.line(&format!("public {} next() {{", elem_wrapper));
    builder.indent();
    builder.line("if (__i >= __n) throw new java.util.NoSuchElementException();");
    builder.line("Pointer __ep = __buf.share(__i * __sz);");
    builder.line("__i++;");
    if let Some(ref clone) = clone_call {
        // Deep-clone via the type's _clone C export. The returned
        // wrapper owns its own heap allocations; safe even after
        // the Vec is closed.
        builder.line(&format!("{}.ByValue __cloned = {}(__ep);", elem_ffi, clone));
        builder.line("__cloned.write();");
        builder.line(&format!(
            "return new {}(__cloned.getPointer());",
            elem_wrapper
        ));
    } else {
        // No _clone available — yield a NON-OWNING wrapper over the
        // Vec's buffer (usable, but close()/finalize() never
        // AzX_delete Vec-internal memory).
        builder.line(&format!(
            "return {}.__borrow(__ep);",
            elem_wrapper
        ));
    }
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("};");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Primitive-element Vecs get a `toXxxArray()` sibling method that
/// bulk-copies the buffer into a JVM-managed array. Driven by the
/// Rust primitive name from `detect_vec_elem_type_jvm`; non-
/// primitives fall through (no method emitted) and the caller
/// gets the `Iterable<T>` clone-each path instead.
fn emit_jvm_vec_primitive_array(builder: &mut CodeBuilder, s: &StructDef, elem_rust: &str) {
    // Map Rust primitive → (JVM array type, JNA getXxxArray method).
    let (arr_ty, getter, method_name) = match elem_rust.trim() {
        "u8" | "i8" | "bool" => ("byte[]", "getByteArray", "toByteArray"),
        "u16" | "i16" => ("short[]", "getShortArray", "toShortArray"),
        "u32" | "i32" => ("int[]", "getIntArray", "toIntArray"),
        "u64" | "i64" | "usize" | "isize" => ("long[]", "getLongArray", "toLongArray"),
        "f32" => ("float[]", "getFloatArray", "toFloatArray"),
        "f64" => ("double[]", "getDoubleArray", "toDoubleArray"),
        _ => return,
    };
    let vec_ffi = ffi_type_name(&s.name);
    builder.line("/**");
    builder.line(&format!(
        " * Bulk-copy the Vec's `{}` elements into a JVM-managed `{}`.",
        elem_rust, arr_ty
    ));
    builder.line(" * One memcpy — safe to use past the Vec being closed");
    builder.line(" * (the returned array owns its own JVM-heap memory).");
    builder.line(" */");
    // Strip the trailing `[]` for the empty-array literal so it
    // reads `new byte[0]` not `new byte[][0]`.
    let elem_arr_ty = &arr_ty[..arr_ty.len() - 2];
    builder.line(&format!("public {} {}() {{", arr_ty, method_name));
    builder.indent();
    builder.line(&format!(
        "{}.ByValue __raw = ({}.ByValue) Structure.newInstance({}.ByValue.class, ptr);",
        vec_ffi, vec_ffi, vec_ffi
    ));
    builder.line("__raw.read();");
    builder.line(&format!(
        "if (__raw.ptr == null || __raw.len <= 0) return new {}[0];",
        elem_arr_ty
    ));
    builder.line(&format!("return __raw.ptr.{}(0, (int) __raw.len);", getter));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `close()` / `finalize()` / `__consume()`.
///
/// * `has_delete_function`: `close()` calls `Az<X>_delete(ptr)` (then `extra_close_lines`),
///   and a defensive `finalize()` calls `close()`.
/// * no `_delete` (engine-borrowed `CallbackInfo`, POD value types): `close()` only marks the
///   wrapper closed and there is NO finalizer — nothing to free, and a finalizer would only cost
///   a GC pass.
fn emit_close_method(
    builder: &mut CodeBuilder,
    raw_type_name: &str,
    ir: &CodegenIR,
    extra_close_lines: &[String],
) {
    let has_delete = has_delete_function(raw_type_name, ir);
    if has_delete {
        builder.line(
            "/** Frees the underlying native resources (owned wrappers only; a borrowed wrapper \
             is just invalidated). Idempotent. */",
        );
        builder.line("@Override");
        builder.line("public void close() {");
        builder.indent();
        builder.line("if (closed || ptr == null) return;");
        builder.line(&format!(
            "if (owned) {}.INSTANCE.Az{}_delete(ptr);",
            super::functions::native_class_for_class(raw_type_name, ir),
            raw_type_name
        ));
        builder.line("ptr = null;");
        for l in extra_close_lines {
            builder.line(l);
        }
        builder.line("closed = true;");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else {
        builder.line(
            "/** No native resources to free (the pointer is borrowed from the engine, or a \
             plain value): marks the wrapper closed. Idempotent. */",
        );
        builder.line("@Override");
        builder.line("public void close() {");
        builder.indent();
        builder.line("ptr = null;");
        builder.line("closed = true;");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Mark this wrapper as consumed without calling Az<X>_delete.
    // Used by codegen-emitted call sites where the C ABI takes
    // ownership of the underlying bytes by-value (DeepCopy `with_*`
    // methods, owned-by-value wrapper args, CC-2 typed-SAM byte
    // splice). Without this, the wrapper's deferred finalizer
    // double-drops the now-Rust-owned struct.
    builder.line(
        "/** Internal: mark consumed (called by codegen-emitted bridges that transfer ownership \
         to the C ABI by-value). */",
    );
    builder.line("void __consume() {");
    builder.indent();
    builder.line("closed = true;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    if has_delete {
        // Defensive finalizer in case the user forgets try-with-resources.
        builder.line("@Override");
        builder.line("@SuppressWarnings(\"deprecation\")");
        builder.line("protected void finalize() throws Throwable {");
        builder.indent();
        builder.line("try { close(); } finally { super.finalize(); }");
        builder.dedent();
        builder.line("}");
    }
}

/// Phase I.5.1: how the wrapper method should idiomise an Option<T> /
/// Result<T, E> return. Detection lives in [`classify_return`]; the
/// caller computes the user-visible display type from the carried
/// `payload_ty` + `ref_kind` and rewrites the method body to call
/// `__ret.toNullable()` / `__ret.unwrap()` on the FFI struct.
#[derive(Clone)]
enum ReturnIdiom {
    Plain,
    Option {
        payload_ty: String,
        ref_kind: FieldRefKind,
    },
    Result {
        payload_ty: String,
        ref_kind: FieldRefKind,
    },
}

/// Look up the wrapper-method return type and decide whether it should
/// be idiomised at the call site. Mirrors the Ruby/Node `classify_return`
/// predicate but extracts the payload type so the Java side can produce
/// a typed `java.util.Optional<T>` signature rather than a raw `Object`.
fn classify_return(func: &FunctionDef, ir: &CodegenIR) -> ReturnIdiom {
    let Some(rt) = func.return_type.as_deref() else {
        return ReturnIdiom::Plain;
    };
    let rt = rt.trim();
    // Az*Option / Az*Result types are monomorphized aliases — the
    // codegen IR stores them as `TypeAliasDef.monomorphized_def`.
    if let Some(ta) = ir.find_type_alias(rt) {
        if let Some(ref mono) = ta.monomorphized_def {
            if let MonomorphizedKind::TaggedUnion { ref variants, .. } = mono.kind {
                if variants.len() == 2 {
                    let none = variants.iter().find(|v| v.name == "None");
                    let some = variants.iter().find(|v| v.name == "Some");
                    if let (Some(_), Some(sv)) = (none, some) {
                        if let Some(ref payload_ty) = sv.payload_type {
                            return ReturnIdiom::Option {
                                payload_ty: payload_ty.clone(),
                                ref_kind: sv.payload_ref_kind,
                            };
                        }
                    }
                    let ok = variants.iter().find(|v| v.name == "Ok");
                    let err = variants.iter().find(|v| v.name == "Err");
                    if let (Some(ov), Some(_)) = (ok, err) {
                        if let Some(ref payload_ty) = ov.payload_type {
                            return ReturnIdiom::Result {
                                payload_ty: payload_ty.clone(),
                                ref_kind: ov.payload_ref_kind,
                            };
                        }
                    }
                }
            }
        }
    }
    // Fallback: hand-authored Option/Result enums (rare; the api.json
    // sources are normally typedefs).
    if let Some(e) = ir.find_enum(rt) {
        if e.variants.len() == 2 {
            let none = e.variants.iter().find(|v| v.name == "None");
            let some = e.variants.iter().find(|v| v.name == "Some");
            if let (Some(_), Some(sv)) = (none, some) {
                if let EnumVariantKind::Tuple(types) = &sv.kind {
                    if types.len() == 1 {
                        return ReturnIdiom::Option {
                            payload_ty: types[0].0.clone(),
                            ref_kind: types[0].1,
                        };
                    }
                }
            }
            let ok = e.variants.iter().find(|v| v.name == "Ok");
            let err = e.variants.iter().find(|v| v.name == "Err");
            if let (Some(ov), Some(_)) = (ok, err) {
                if let EnumVariantKind::Tuple(types) = &ov.kind {
                    if types.len() == 1 {
                        return ReturnIdiom::Result {
                            payload_ty: types[0].0.clone(),
                            ref_kind: types[0].1,
                        };
                    }
                }
            }
        }
    }
    ReturnIdiom::Plain
}

/// Map a payload's "raw" Java field type (what JNA carries on the
/// FFI struct) to the user-visible display type at the wrapper
/// boundary:
///
/// - `AzString` → `java.lang.String` (UTF-8 decode inline)
/// - `AzX` with a wrapper class → `X` (the wrapper)
/// - Anything else → the raw type itself (primitives stay primitives; `Pointer` stays `Pointer`;
///   raw FFI structs without a wrapper stay `AzY`)
fn payload_display_type(raw: &str, ir: &CodegenIR) -> String {
    if let Some(unprefixed) = raw.strip_prefix("Az") {
        // TypeCategory-driven (J.3 pattern): any struct flagged as
        // String in api.json — not just the literal "String" type —
        // gets the java.lang.String round-trip at the wrapper boundary.
        if let Some(s) = ir.find_struct(unprefixed) {
            if matches!(s.category, TypeCategory::String) {
                return "java.lang.String".to_string();
            }
        }
        if has_wrapper_class(unprefixed, ir) {
            return unprefixed.to_string();
        }
    }
    raw.to_string()
}

/// Detect whether a payload's raw Java type maps to an
/// `azul.json`-categorised String struct (i.e. UTF-8-decode at the
/// boundary applies).
fn is_az_string_jvm(raw: &str, ir: &CodegenIR) -> bool {
    let Some(unprefixed) = raw.strip_prefix("Az") else {
        return false;
    };
    ir.find_struct(unprefixed)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// `splice_arg_idx`: for window-taking methods of the application class
/// (see `emit_app_factory`), the index in `func.args` of the options arg
/// that receives the factory-registered layout callback.
fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    splice_arg_idx: Option<usize>,
) {
    let method_name = idiomatic_method_name(&func.method_name);

    let return_jvm = func
        .return_type
        .as_ref()
        .map(|r| map_jvm_type_byvalue(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let idiom = classify_return(func, ir);

    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // For takes_self methods the first arg in `func.args` IS the
    // implicit self, regardless of the name the api.json gave it
    // (`instance`, lowercased class name, etc.). Skip args[0] unconditionally
    // when takes_self; otherwise filter by conventional self names so a
    // legitimate user arg named after the class still passes through.
    let user_args: Vec<_> = if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        let class_lower = func.class_name.to_lowercase();
        func.args
            .iter()
            .filter(|a| a.name != class_lower && a.name != "self")
            .collect()
    };

    // Auto-conversion rules (both type-driven; no method-name allow-
    // list, no per-class hardcoding):
    //
    // 1. AzString Owned: parameter takes `java.lang.String`; emit a UTF-8-bytes → AzString_fromUtf8
    //    conversion pre-call line.
    // 2. Wrapper-class Owned: parameter takes the wrapper class (e.g. `Dom child` instead of
    //    `AzDom.ByValue child`); emit a Structure.newInstance + .read() splice pre-call line.
    //
    // Both apply uniformly to every emitted wrapper method.
    let is_az_string_owned_arg = |a: &&FunctionArg| -> bool {
        // The engine's UTF-8 string type, recognised by its IR category —
        // never by the name api.json happens to give it.
        matches!(a.ref_kind, ArgRefKind::Owned)
            && ir
                .find_struct(a.type_name.trim())
                .is_some_and(|st| matches!(st.category, TypeCategory::String))
    };
    // Only treat as wrapper-class arg if the codegen actually emits a
    // wrapper file for it — the SAME predicate `should_emit_wrapper`
    // uses, so the generated code can never reference a missing class
    // and arg/return conversion stay symmetric. (Checked after the
    // String rule at every use site.)
    let is_wrapper_class_owned_arg = |a: &&FunctionArg| -> bool {
        matches!(a.ref_kind, ArgRefKind::Owned)
            && !is_az_string_owned_arg(a)
            && has_wrapper_class(a.type_name.trim(), ir)
    };

    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let jt = if is_az_string_owned_arg(a) {
                "java.lang.String".to_string()
            } else if is_wrapper_class_owned_arg(a) {
                // Wrapper class — strip `Az` prefix the same way
                // `wrapper_class_name` would.
                wrapper_class_name(a.type_name.trim())
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_jvm_type_byvalue(&a.type_name, ir),
                    ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                        "Pointer".to_string()
                    }
                }
            };
            format!("{} {}", jt, sanitize_identifier(&a.name))
        })
        .collect();

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Some C ABIs take self by VALUE (e.g. `AzRibbon_renderDom(AzRibbon r)`)
    // rather than by pointer (`AzFoo_*(IntPtr instance, ...)`). Detect via
    // the first arg's ref_kind (Owned = by value). When self-by-value, we
    // construct a `Az<Type>.ByValue` whose Pointer points at our heap-held
    // instance and pass it in.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);

    let mut pre_call_lines: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();
    // After the C-ABI call, mark these wrapper names consumed (no-ops
    // their finalizer's AzX_delete) — the C side took ownership of
    // their bytes by-value.
    let mut consume_after_call: Vec<String> = Vec::new();
    if takes_self {
        if self_by_value {
            // Build a JNA `.ByValue` Structure overlaying our pointer
            // via the public Structure.newInstance(Class, Pointer)
            // factory. `useMemory` is protected and not callable from
            // here; `newInstance` is the canonical replacement.
            let self_ty = ffi_type_name(&func.class_name);
            pre_call_lines.push(format!(
                "{}.ByValue __self = Structure.newInstance({}.ByValue.class, this.ptr);",
                self_ty, self_ty
            ));
            pre_call_lines.push("__self.read();".to_string());
            call_args.push("__self".to_string());
            // DeepCopy / consuming-self method: the C ABI takes the
            // struct by-value; this wrapper's bytes are now Rust-owned
            // and will be dropped by libazul. Mark `this` consumed so
            // the deferred finalizer doesn't double-drop.
            consume_after_call.push("this".to_string());
        } else {
            call_args.push("this.ptr".to_string());
        }
    }
    // Callback args: do NOT auto-substitute at the wrapper-method
    // boundary. The wrapper signature carries the C ABI type (e.g.
    // `AzCallback.ByValue` or `AzCallbackType`) and is passed through
    // unchanged. Users construct the wrapper struct via
    // `AzulHostInvoker.register*(handler)` themselves and pass that.
    // (Same conclusion C# / Lua reached.)
    //
    // Auto-string-conversion: any Owned `String` arg accepts a
    // `java.lang.String` at the wrapper level. Convert UTF-8 bytes →
    // AzString.ByValue via the C-API helper before the call.
    for a in &user_args {
        let raw_name = sanitize_identifier(&a.name);
        if is_az_string_owned_arg(a) {
            let az_name = format!("__{}_az", raw_name);
            let bytes_name = format!("__{}_bytes", raw_name);
            let mem_name = format!("__{}_mem", raw_name);
            pre_call_lines.push(format!(
                "byte[] {bytes} = {raw}.getBytes(java.nio.charset.StandardCharsets.UTF_8);",
                bytes = bytes_name,
                raw = raw_name,
            ));
            // JNA's Memory constructor throws IllegalArgumentException
            // for size 0, so `Button.create("")` etc. must not allocate
            // `new Memory(0)`. Allocate at least 1 byte; the native
            // AzString_fromUtf8 (css corety.rs from_utf8) returns
            // AzString::default() whenever len == 0, so the 1-byte
            // buffer is never read and "" round-trips correctly.
            pre_call_lines.push(format!(
                "com.sun.jna.Memory {mem} = new com.sun.jna.Memory(Math.max(1, {bytes}.length));",
                mem = mem_name,
                bytes = bytes_name,
            ));
            pre_call_lines.push(format!(
                "{mem}.write(0, {bytes}, 0, {bytes}.length);",
                mem = mem_name,
                bytes = bytes_name,
            ));
            pre_call_lines.push(format!(
                "AzString.ByValue {az} = AzulNativeStr.INSTANCE.AzString_fromUtf8({mem}, \
                 {bytes}.length);",
                az = az_name,
                mem = mem_name,
                bytes = bytes_name,
            ));
            call_args.push(az_name);
        } else if is_wrapper_class_owned_arg(a) {
            // Splice the wrapper's underlying Pointer into a
            // by-value Structure overlay so the C ABI sees a real
            // struct value. Same pattern the self-by-value path uses.
            let ffi = ffi_type_name(a.type_name.trim());
            let raw_local = format!("__{}_raw", raw_name);
            pre_call_lines.push(format!(
                "{ffi}.ByValue {raw_local} = Structure.newInstance({ffi}.ByValue.class, \
                 {arg}.rawPointer());",
                ffi = ffi,
                raw_local = raw_local,
                arg = raw_name,
            ));
            pre_call_lines.push(format!("{}.read();", raw_local));
            call_args.push(raw_local);
            // C ABI receives the struct by-value → take ownership of
            // the underlying heap allocations. Mark the caller's
            // wrapper consumed so its deferred finalizer doesn't
            // double-drop.
            consume_after_call.push(raw_name.clone());
        } else {
            call_args.push(raw_name);
        }
    }

    // Application class, window-taking method: hand the by-value
    // options overlay to the splice helper before the native call.
    if let Some(idx) = splice_arg_idx {
        if let Some(a) = func.args.get(idx) {
            if is_wrapper_class_owned_arg(&a) {
                pre_call_lines.push(format!(
                    "this.__spliceLayoutCallback(__{}_raw);",
                    sanitize_identifier(&a.name)
                ));
            }
        }
    }

    if !func.doc.is_empty() {
        builder.line("/**");
        for d in &func.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // Auto-wrap non-self wrapper-class returns: if the IR return
    // type is a struct that has an emitted wrapper class (i.e.
    // `has_wrapper_class` is true), surface the wrapper at the
    // boundary instead of the raw FFI struct. Pattern matches the
    // payload-display-type rule applied to plain (non-Option/Result)
    // returns — same IR-driven predicate, no name allowlist.
    // Route the IR name through `wrapper_class_name` so the rename
    // rules (e.g. `String` → `AzulString` to avoid shadowing
    // `java.lang.String` inside `package com.azul`) apply uniformly
    // to display + construction sites.
    let returns_wrapper_other: Option<String> = if returns_self {
        None
    } else if matches!(idiom, ReturnIdiom::Plain) {
        func.return_type
            .as_deref()
            .map(|r| r.trim())
            .filter(|r| has_wrapper_class(r, ir))
            .map(wrapper_class_name)
    } else {
        None
    };

    // Phase I.5.1: idiomise Option<T> / Result<T, E> return types at the
    // wrapper boundary. The FFI struct still exposes
    // `toNullable()` / `unwrap()` (from `types.rs`); the wrapper layer
    // simply rebrands the visible signature to `java.util.Optional<T>`
    // for Option and the bare payload type for Result (which throws on
    // Err — same idiom as Rust's `Result::unwrap`).
    let displayed_return = if returns_self {
        class_name.to_string()
    } else if let Some(ref wrapper) = returns_wrapper_other {
        wrapper.clone()
    } else {
        match &idiom {
            ReturnIdiom::Plain => return_jvm.clone(),
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                let display = payload_display_type(&raw, ir);
                format!("java.util.Optional<{}>", java_boxed(&display))
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                let display = payload_display_type(&raw, ir);
                java_boxed(&display)
            }
        }
    };

    let modifiers = if is_static { "public static" } else { "public" };

    builder.line(&format!(
        "{} {} {}({}) {{",
        modifiers,
        displayed_return,
        method_name,
        arg_sig.join(", ")
    ));
    builder.indent();

    if !is_static {
        builder.line("if (closed) throw new IllegalStateException(\"closed\");");
    }

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `managed_c_symbol(func)` — normally `func.c_name` verbatim
    // (already the camelCase native symbol; reconstructing from
    // method_name yields snake_case drift), but functions with a
    // callback-wrapper arg bind the `<c_name>Struct` triple-variant,
    // whose signature (whole wrapper struct by value) matches the
    // ByValue args we pass here. Binding the raw `<c_name>` (bare
    // fn ptr at the C ABI) with these args crashed on click.
    let call = format!(
        "{}.INSTANCE.{}({})",
        super::functions::native_class_for_func(func, ir),
        managed_c_symbol(func),
        call_args.join(", ")
    );

    let emit_consume = |b: &mut CodeBuilder| {
        for name in &consume_after_call {
            b.line(&format!("{}.__consume();", name));
        }
    };

    if return_jvm == "void" {
        builder.line(&format!("{};", call));
        emit_consume(builder);
    } else if returns_self {
        // The C ABI returned a struct-by-value; the JNA shim returned a
        // ByValue Structure. We adopt its `Pointer` for the wrapper.
        builder.line(&format!("{} __raw = {};", return_jvm, call));
        emit_consume(builder);
        builder.line(&format!("return new {}(__raw.getPointer());", class_name));
    } else if let Some(ref wrapper) = returns_wrapper_other {
        // Non-self wrapper-class return: same shape as returns_self
        // but wraps in the return-type's wrapper class.
        builder.line(&format!("{} __raw = {};", return_jvm, call));
        emit_consume(builder);
        builder.line(&format!("return new {}(__raw.getPointer());", wrapper));
    } else {
        // The Option/Result type name for the _delete call lookup
        // (e.g. "OptionDom", "ResultIcuError"). Same string the IR
        // stored in `func.return_type`.
        let option_or_result_ty = func
            .return_type
            .as_deref()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        match &idiom {
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                builder.line(&format!("{} __ret = {};", return_jvm, call));
                emit_consume(builder);
                emit_option_return_body(builder, &raw, &option_or_result_ty, ir);
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                builder.line(&format!("{} __ret = {};", return_jvm, call));
                emit_consume(builder);
                emit_result_return_body(builder, &raw, &option_or_result_ty, ir);
            }
            ReturnIdiom::Plain => {
                if consume_after_call.is_empty() {
                    builder.line(&format!("return {};", call));
                } else {
                    // Capture result so consume calls can run between
                    // call and return without inlining side-effects.
                    builder.line(&format!("{} __ret = {};", return_jvm, call));
                    emit_consume(builder);
                    builder.line("return __ret;");
                }
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Phase I.5.1 — Option<T> / Result<T, E> idiomatic return bodies
// ============================================================================

/// Emit the body of a wrapper method whose return is `Optional<T>`. The
/// FFI `AzOption*.ByValue __ret` has already been declared; we call
/// `__ret.toNullable()` and wrap the result for the host idiom.
///
/// Three paths based on the raw FFI payload type:
/// 1. `AzString` — decode UTF-8 bytes into `java.lang.String` inline.
/// 2. `AzX` with a wrapper class — construct `new X(__nv.getPointer())`.
/// 3. Anything else (primitives, raw FFI structs without wrappers, `Pointer`) — return
///    `Optional.ofNullable(__ret.toNullable())` directly.
fn emit_option_return_body(
    builder: &mut CodeBuilder,
    raw_payload_jvm: &str,
    option_type_name: &str,
    ir: &CodegenIR,
) {
    let option_delete = format_option_delete_call_jvm(option_type_name, ir);
    if is_az_string_jvm(raw_payload_jvm, ir) {
        // AzString payload: decode UTF-8 into an independent
        // java.lang.String, then drop the Option so libazul frees the
        // embedded AzString's Vec.ptr buffer.
        builder.line(&format!("{} __nv = __ret.toNullable();", raw_payload_jvm));
        builder.line("if (__nv == null) {");
        builder.indent();
        if let Some(ref del) = option_delete {
            builder.line(&format!("{};", del));
        }
        builder.line("return java.util.Optional.empty();");
        builder.dedent();
        builder.line("}");
        builder.line("Pointer __sp = __nv.getPointer();");
        builder.line("Pointer __vecPtr = __sp.getPointer(0);");
        builder.line("long __vecLen = __sp.getLong(8);");
        builder.line("java.lang.String __out;");
        builder.line("if (__vecPtr == null || __vecLen <= 0) {");
        builder.indent();
        builder.line("__out = \"\";");
        builder.dedent();
        builder.line("} else {");
        builder.indent();
        builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
        builder.line(
            "__out = new java.lang.String(__bytes, java.nio.charset.StandardCharsets.UTF_8);",
        );
        builder.dedent();
        builder.line("}");
        if let Some(ref del) = option_delete {
            builder.line(&format!("{};", del));
        }
        builder.line("return java.util.Optional.of(__out);");
        return;
    }
    if let Some(unprefixed) = raw_payload_jvm.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            // Wrapper-class payload: clone the payload via the
            // type's `_clone` C export so the new wrapper owns
            // independent heap allocations, then drop the Option to
            // free the original payload's allocations. If the type
            // doesn't expose `_clone`, fall back to the borrow-from-
            // Option-Memory shape (small Option-shell leak but no
            // dangling pointers).
            let clone_call = format_clone_call_jvm(unprefixed, ir);
            builder.line(&format!("{} __nv = __ret.toNullable();", raw_payload_jvm));
            builder.line("if (__nv == null) {");
            builder.indent();
            if let Some(ref del) = option_delete {
                builder.line(&format!("{};", del));
            }
            builder.line("return java.util.Optional.empty();");
            builder.dedent();
            builder.line("}");
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "{} __cloned = {}(__nv.getPointer());",
                    raw_payload_jvm, clone
                ));
                builder.line("__cloned.write();");
                if let Some(ref del) = option_delete {
                    builder.line(&format!("{};", del));
                }
                builder.line(&format!(
                    "return java.util.Optional.of(new {}(__cloned.getPointer()));",
                    unprefixed
                ));
            } else {
                builder.line(&format!(
                    "return java.util.Optional.of(new {}(__nv.getPointer()));",
                    unprefixed
                ));
            }
            return;
        }
    }
    // Primitive / Pointer payload: independent value, safely drop
    // the Option. (For primitives _delete is essentially a no-op but
    // we still issue it for consistency and so future heap-bearing
    // primitives don't silently leak.)
    builder.line(&format!(
        "{} __opt = __ret.toNullable();",
        java_boxed(&payload_display_type(raw_payload_jvm, ir))
    ));
    if let Some(ref del) = option_delete {
        builder.line(&format!("{};", del));
    }
    builder.line("return java.util.Optional.ofNullable(__opt);");
}

/// Build a `<NativeClass>.INSTANCE.Az<OptionT>_delete(__ret.getPointer())`
/// call expression, or None when the IR has no matching _delete fn.
fn format_option_delete_call_jvm(option_type_name: &str, ir: &CodegenIR) -> Option<String> {
    if !has_delete_function(option_type_name, ir) {
        return None;
    }
    let native = super::functions::native_class_for_class(option_type_name, ir);
    let ffi_name = ffi_type_name(option_type_name);
    Some(format!(
        "{}.INSTANCE.{}_delete(__ret.getPointer())",
        native, ffi_name
    ))
}

/// Build a `<NativeClass>.INSTANCE.Az<T>_clone` expression (not yet
/// invoked) for the wrapper-class payload type, or None when no
/// `_clone` C export exists.
fn format_clone_call_jvm(payload_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == payload_type_name && matches!(f.kind, FunctionKind::DeepCopy));
    if !has_clone {
        return None;
    }
    let native = super::functions::native_class_for_class(payload_type_name, ir);
    let ffi_name = ffi_type_name(payload_type_name);
    Some(format!("{}.INSTANCE.{}_clone", native, ffi_name))
}

/// Emit the body of a wrapper method whose return is the bare Ok
/// payload of a `Result<T, E>` (throws `RuntimeException` on Err — the
/// FFI struct's `unwrap()` does that lift). Same three cases as
/// [`emit_option_return_body`].
fn emit_result_return_body(
    builder: &mut CodeBuilder,
    raw_payload_jvm: &str,
    result_type_name: &str,
    ir: &CodegenIR,
) {
    let result_delete = format_option_delete_call_jvm(result_type_name, ir);
    if is_az_string_jvm(raw_payload_jvm, ir) {
        builder.line(&format!("{} __u = __ret.unwrap();", raw_payload_jvm));
        builder.line("Pointer __sp = __u.getPointer();");
        builder.line("Pointer __vecPtr = __sp.getPointer(0);");
        builder.line("long __vecLen = __sp.getLong(8);");
        builder.line("java.lang.String __out;");
        builder.line("if (__vecPtr == null || __vecLen <= 0) {");
        builder.indent();
        builder.line("__out = \"\";");
        builder.dedent();
        builder.line("} else {");
        builder.indent();
        builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
        builder.line(
            "__out = new java.lang.String(__bytes, java.nio.charset.StandardCharsets.UTF_8);",
        );
        builder.dedent();
        builder.line("}");
        if let Some(ref del) = result_delete {
            builder.line(&format!("{};", del));
        }
        builder.line("return __out;");
        return;
    }
    if let Some(unprefixed) = raw_payload_jvm.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            let clone_call = format_clone_call_jvm(unprefixed, ir);
            builder.line(&format!("{} __u = __ret.unwrap();", raw_payload_jvm));
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "{} __cloned = {}(__u.getPointer());",
                    raw_payload_jvm, clone
                ));
                builder.line("__cloned.write();");
                if let Some(ref del) = result_delete {
                    builder.line(&format!("{};", del));
                }
                builder.line(&format!(
                    "return new {}(__cloned.getPointer());",
                    unprefixed
                ));
            } else {
                builder.line(&format!("return new {}(__u.getPointer());", unprefixed));
            }
            return;
        }
    }
    builder.line(&format!(
        "{} __u = __ret.unwrap();",
        java_boxed(&payload_display_type(raw_payload_jvm, ir))
    ));
    if let Some(ref del) = result_delete {
        builder.line(&format!("{};", del));
    }
    builder.line("return __u;");
}

// ============================================================================
// Tagged-union helper class
// ============================================================================

fn emit_union_helper(
    builder: &mut CodeBuilder,
    e: &EnumDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let class_name = wrapper_class_name(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        builder.line("/**");
        for d in &e.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!("public final class {}Helpers {{", class_name));
    builder.indent();
    builder.line(&format!("private {}Helpers() {{}}", class_name));
    builder.blank();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let mname = idiomatic_method_name(&v.name);
                let variant_ident = sanitize_identifier(&v.name);
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                // libazul exports a constructor for EVERY variant, payload or
                // not (`AzAccessibilityAction_blur()`). Calling it keeps the
                // tag encoding the engine's business — assembling the tag byte
                // here only worked for as long as the C layout stayed
                // `repr(C, u8)` with the tag first. The hand-rolled fallback
                // below is for the destructor/cloner unions, whose variant
                // constructors the FFI layer deliberately does not declare.
                match ir
                    .variant_constructor(&e.name, &v.name)
                    .filter(|ctor| super::functions::should_emit_function(*ctor, ir, config))
                {
                    Some(ctor) => {
                        builder.line(&format!(
                            "public static {} {}() {{",
                            super::map_jvm_type_byvalue(&e.name, ir),
                            mname
                        ));
                        builder.indent();
                        builder.line(&format!(
                            "return {}.{}();",
                            super::functions::native_class_for_func(ctor, ir),
                            super::super::managed_host_invoker::managed_c_symbol(ctor)
                        ));
                        builder.dedent();
                        builder.line("}");
                    }
                    None => {
                        builder.line(&format!("public static {} {}() {{", ffi_name, mname));
                        builder.indent();
                        builder.line(&format!("{} u = new {}();", ffi_name, ffi_name));
                        builder.line(&format!(
                            "u.{}.tag = (byte) {}_Tag.{}.value;",
                            variant_ident, ffi_name, variant_ident
                        ));
                        builder.line(&format!("u.setType(\"{}\");", variant_ident));
                        builder.line("return u;");
                        builder.dedent();
                        builder.line("}");
                    }
                }
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                // A payload variant: the C constructor libazul exports for it
                // (`Az{Enum}_{variant}(payload)`) builds the tagged union.
                let Some(ctor) = ir
                    .variant_constructor(&e.name, &v.name)
                    .filter(|ctor| super::functions::should_emit_function(*ctor, ir, config))
                else {
                    continue;
                };
                let params: Vec<String> = ctor
                    .args
                    .iter()
                    .map(|a| {
                        format!(
                            "{} {}",
                            super::map_jvm_type_byvalue(&a.type_name, ir),
                            sanitize_identifier(&a.name)
                        )
                    })
                    .collect();
                let names: Vec<String> = ctor.args.iter().map(|a| sanitize_identifier(&a.name)).collect();
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "public static {} {}({}) {{",
                    super::map_jvm_type_byvalue(&e.name, ir),
                    idiomatic_method_name(&v.name),
                    params.join(", ")
                ));
                builder.indent();
                builder.line(&format!(
                    "return {}.{}({});",
                    super::functions::native_class_for_func(ctor, ir),
                    super::super::managed_host_invoker::managed_c_symbol(ctor),
                    names.join(", ")
                ));
                builder.dedent();
                builder.line("}");
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
}

// ============================================================================
// Helpers
// ============================================================================

pub(super) fn wrapper_class_name(raw: &str) -> String {
    // The codegen-emitted `String` wrapper (AzString backing) collides
    // with `java.lang.String` inside `package com.azul`. Java has no
    // verbatim-identifier syntax, so users were forced into qualified
    // calls like `java.lang.String.valueOf(m.counter)` every time they
    // touched the JDK-bundled string. Rename the wrapper to `AzulString`
    // — preserves the type's purpose (Azul-managed UTF-8 string) and
    // restores the unqualified `String` for the JDK class.
    //
    // The comparison has to spell the JDK type's own name, and that name also
    // happens to be an api.json class name; no IR property says "the host
    // language already defines a type called this". Only this one is renamed:
    // `Thread`, the only other api.json class `java.lang` also defines, is
    // never written unqualified by generated code, so renaming it would churn
    // a public class name for nothing.
    // allow-api-name: the JDK class this wrapper shadows is itself named String
    if raw == "String" {
        return "AzulString".to_string();
    }
    sanitize_identifier(raw)
}

pub(super) fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    let camel = if method_name.contains('_') {
        snake_to_lower_camel(method_name)
    } else {
        // Already lowerCamel (most common in api.json) or PascalCase;
        // ensure first letter is lowercase for Java methods.
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // `default`, `class`, `case`, etc. can't be method names. Java has
    // no verbatim-identifier syntax, so append `_`.
    // Also rename `close` because every wrapper implements AutoCloseable
    // with its own `close()` for resource cleanup — a user-API method
    // also named `close` would be a duplicate-definition error. The
    // SvgPath bug (an SVG path's "close path" segment vs the lifecycle
    // close) showed up first; the rule generalises.
    if super::is_java_reserved(&camel) {
        format!("{}_", camel)
    } else if camel == "close" {
        "closeInner".to_string()
    } else if matches!(
        camel.as_str(),
        "toString" | "hashCode" | "equals" | "getClass" | "clone" | "finalize"
    ) {
        // Methods declared on java.lang.Object have fixed signatures.
        // A user-API method called `toString` returning `AzString.ByValue`
        // cannot legally override `Object.toString()` (which returns
        // `java.lang.String`), so suffix it. Same family covers
        // hashCode / equals / clone / finalize / getClass.
        format!("{}_", camel)
    } else {
        camel
    }
}
