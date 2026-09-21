//! Kotlin managed-FFI runtime helpers (host-invoker pattern).
//!
//! Kotlin emits a single `Azul.kt` file containing the direct-mapped JNA
//! objects, all `Structure` subclasses, all callback interfaces, and
//! the idiomatic wrapper classes. We append a Kotlin `object
//! AzulHostInvoker` to that file with the same surface as Java's
//! `AzulHostInvoker` class, plus `object AzulNativeManaged` for the
//! host-invoker C-ABI imports (`@JvmStatic external`, `Native.register`).
//!
//! Why a per-language managed.rs (rather than reusing Java's class):
//! Kotlin's bindings are self-contained — there's no Java inter-op
//! requirement on the consumer side. Forcing them to compile a parallel
//! Java module just for the host-invoker would be a worse experience
//! than emitting a small Kotlin object.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionKind, TypeCategory},
        managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name},
        managed_lang_helpers::{has_delete_function, has_wrapper_class, is_refany_type},
    },
    ffi_type_name, kotlin_class_name, user_enum_type_name, LIBRARY_NAME,
};

/// Append the host-invoker block to the existing `Azul.kt` body.
pub fn emit(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// Managed-FFI runtime: host-invoker JNA imports + AzulHostInvoker");
    builder.line("// object. Mirrors the Java AzulNativeManaged + AzulHostInvoker pair.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    // AzulNativeManaged: separate direct-mapped object for the host-invoker
    // C-ABI exports (same `Native.register` shape as the per-module objects).
    builder.line("object AzulNativeManaged {");
    builder.indent();
    builder.line(&format!(
        "init {{ Native.register(AzulNativeManaged::class.java, \"{}\") }}",
        LIBRARY_NAME
    ));
    builder.blank();

    builder.line("fun interface HostHandleReleaserCallback : JnaCallback {");
    builder.indent();
    builder.line("fun invoke(id: Long)");
    builder.dedent();
    builder.line("}");
    builder.line(
        "@JvmStatic external fun AzApp_setHostHandleReleaser(fn: HostHandleReleaserCallback)",
    );
    builder.line("@JvmStatic external fun AzRefAny_newHostHandle(id: Long): AzRefAny.ByValue");
    builder.line("@JvmStatic external fun AzRefAny_getHostHandle(refanyPtr: Pointer?): Long");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let cb_has_return = has_return(cb);
        let mut params = vec!["id: Long".to_string()];
        for (i, a) in cb.args.iter().enumerate() {
            let nm = if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                a.name.clone()
            };
            params.push(format!("{}: Pointer?", nm));
        }
        if cb_has_return {
            params.push("outPtr: Pointer?".to_string());
        }
        builder.line(&format!(
            "fun interface {}InvokerCallback : JnaCallback {{",
            wrapper
        ));
        builder.indent();
        builder.line(&format!("fun invoke({})", params.join(", ")));
        builder.dedent();
        builder.line("}");
        builder.line(&format!(
            "@JvmStatic external fun AzApp_set{w}Invoker(fn: {w}InvokerCallback)",
            w = wrapper
        ));
        builder.line(&format!(
            "@JvmStatic external fun Az{w}_createFromHostHandle(id: Long): Az{w}.ByValue",
            w = wrapper
        ));
        builder.blank();
    }

    builder.dedent();
    builder.line("}");
    builder.blank();

    // AzulHostInvoker singleton.
    builder.line("object AzulHostInvoker {");
    builder.indent();
    builder.line("private val handles = java.util.HashMap<Long, Any>()");
    builder.line("private var nextHandleId: Long = 0");
    builder.line("private val livePins = java.util.ArrayList<Any>()");
    builder.line("@Volatile private var initialized: Boolean = false");
    builder.line("private val initLock = Any()");
    builder.blank();

    builder.line("private fun ensureInitialized() {");
    builder.indent();
    builder.line("if (initialized) return");
    builder.line("synchronized(initLock) {");
    builder.indent();
    builder.line("if (initialized) return");
    builder.line("initialized = true");
    builder.blank();
    builder.line("val releaser = AzulNativeManaged.HostHandleReleaserCallback { id ->");
    builder.indent();
    builder.line("synchronized(handles) { handles.remove(id) }");
    builder.dedent();
    builder.line("}");
    builder.line("livePins.add(releaser)");
    builder.line("AzulNativeManaged.AzApp_setHostHandleReleaser(releaser)");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let cb_has_return = has_return(cb);
        // Build named lambda params matching the SAM declared in
        // AzulNativeManaged: `id` + one Pointer per callback IR arg
        // + (if has_return) `outPtr`. We need named bindings so the
        // dispatch body can forward them through.
        let mut params: Vec<String> = vec!["id".to_string()];
        let mut forward_args: Vec<String> = vec!["id".to_string()];
        for (idx, _arg) in cb.args.iter().enumerate() {
            let n = format!("arg{}", idx);
            params.push(n.clone());
            forward_args.push(n);
        }
        if cb_has_return {
            params.push("outPtr".to_string());
            forward_args.push("outPtr".to_string());
        }
        builder.line(&format!("// {} invoker", wrapper));
        builder.line(&format!(
            "val {l}Invoker = AzulNativeManaged.{w}InvokerCallback {{ {p} ->",
            l = lower_first(wrapper),
            w = wrapper,
            p = params.join(", ")
        ));
        builder.indent();
        // Per-kind dispatch: look up the registered user callback by
        // id (it was stashed by `register<Wrapper>(fn)` below), then
        // if it implements the matching `<Wrapper>InvokerCallback`
        // SAM, call its `invoke(...)` with the same args we received
        // from libazul. Mirrors lang_java's dispatch shape.
        builder.line("val fn = synchronized(handles) { handles[id] }");
        builder.line(&format!(
            "if (fn is AzulNativeManaged.{w}InvokerCallback) {{",
            w = wrapper
        ));
        builder.indent();
        builder.line(&format!("fn.invoke({})", forward_args.join(", ")));
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.line(&format!("livePins.add({}Invoker)", lower_first(wrapper)));
        builder.line(&format!(
            "AzulNativeManaged.AzApp_set{w}Invoker({l}Invoker)",
            w = wrapper,
            l = lower_first(wrapper)
        ));
        builder.blank();
    }

    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!(
            "fun register{w}(fn: Any): Az{w}.ByValue {{",
            w = wrapper
        ));
        builder.indent();
        builder.line("ensureInitialized()");
        builder.line("val id = synchronized(handles) {");
        builder.indent();
        builder.line("nextHandleId += 1");
        builder.line("handles[nextHandleId] = fn");
        builder.line("nextHandleId");
        builder.dedent();
        builder.line("}");
        builder.line(&format!(
            "return AzulNativeManaged.Az{}_createFromHostHandle(id)",
            wrapper
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    builder.line("fun refanyCreate(value: Any): AzRefAny.ByValue {");
    builder.indent();
    builder.line("ensureInitialized()");
    builder.line("val id = synchronized(handles) {");
    builder.indent();
    builder.line("nextHandleId += 1");
    builder.line("handles[nextHandleId] = value");
    builder.line("nextHandleId");
    builder.dedent();
    builder.line("}");
    builder.line("return AzulNativeManaged.AzRefAny_newHostHandle(id)");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("fun refanyGet(refanyPtr: Pointer?): Any? {");
    builder.indent();
    builder.line("val id = AzulNativeManaged.AzRefAny_getHostHandle(refanyPtr)");
    builder.line("if (id == 0L) return null");
    builder.line("return synchronized(handles) { handles[id] }");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Phase CC-5 (Kotlin): wrap an Any in the `RefAny` wrapper class
    // directly. Convenience over `refanyCreate(Any)` which returns
    // the raw `AzRefAny.ByValue`.
    builder.line("/**");
    builder.line(" * Wrap an arbitrary Kotlin object in a `RefAny` wrapper.");
    builder.line(" * Convenience over `refanyCreate(Any)` which returns the raw");
    builder.line(" * `AzRefAny.ByValue` FFI struct.");
    builder.line(" */");
    builder.line("@JvmStatic fun refanyWrap(value: Any): RefAny {");
    builder.indent();
    builder.line("val raw = refanyCreate(value)");
    builder.line("return RefAny(raw.pointer)");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Phase CC-2 (Kotlin): typed-SAM bridge per kind with wrapper-class
    // return. Iterates `host_invoker_kinds(ir)`; for each kind whose
    // return is a struct with an emitted wrapper class, emit a typed
    // `<Wrapper>Callback` interface and a `register<Wrapper>` overload
    // that splices the wrapper's bytes into outPtr. Pure IR-driven —
    // no ABI symbols or class names hardcoded.
    for cb in host_invoker_kinds(ir) {
        emit_kt_typed_invoker_sam(builder, cb, ir);
    }

    // Phase CC-1 (Kotlin): Data<T>-typed SAM bridge. The user writes
    //   (data: MyDataModel, info: LayoutCallbackInfo) -> Dom
    // instead of unpacking `Pointer dataPtr` themselves. Kinds whose
    // signature does not fit `kt_data_typed_sam_shape` are skipped.
    for cb in host_invoker_kinds(ir) {
        emit_kt_data_typed_invoker_sam(builder, cb, ir);
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Emit the typed-SAM bridge for one host-invoker kind on the Kotlin
/// side. Mirrors `lang_java/managed::emit_typed_invoker_sam`; the
/// only differences are language syntax (`fun interface`, `as Any`
/// boxing) and Kotlin's strict-null requirement on the platform-type
/// Pointer args.
fn emit_kt_typed_invoker_sam(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    let wrapper = wrapper_name(cb);
    if !has_return(cb) {
        return;
    }
    let Some(ret_ty) = cb.return_type.as_deref().map(str::trim) else {
        return;
    };
    // The bridge splices the returned wrapper's bytes into `outPtr`, so
    // the return type must have a wrapper class — the same predicate that
    // decides whether `wrappers.rs` emits one.
    if !has_wrapper_class(ret_ty, ir) {
        return;
    }

    let wrapper_class = kotlin_class_name(ret_ty, ir);
    let ffi_ret = ffi_type_name(ret_ty);
    let cb_ffi = ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);

    let mut typed_params = vec!["id: Long".to_string()];
    let mut typed_args = vec!["id".to_string()];
    let mut raw_lambda_args = vec!["id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        let nm = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
        typed_params.push(format!("{}: Pointer?", nm));
        typed_args.push(nm.clone());
        raw_lambda_args.push(nm);
    }
    raw_lambda_args.push("outPtr".to_string());

    builder.line("/**");
    builder.line(&format!(
        " * Typed {} SAM. Returns a `{}` wrapper directly; the host-invoker",
        wrapper, wrapper_class
    ));
    builder.line(" * bridge handles the struct-byte splice into outPtr internally.");
    builder.line(" */");
    builder.line(&format!("fun interface {} {{", wrapper));
    builder.indent();
    builder.line(&format!(
        "fun invoke({}): {}",
        typed_params.join(", "),
        wrapper_class
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/**");
    builder.line(&format!(
        " * Register a typed `{}`. Wraps it in a raw",
        wrapper
    ));
    builder.line(&format!(
        " * `{}InvokerCallback` that performs the `{}`-byte splice.",
        wrapper, ret_ty
    ));
    builder.line(" */");
    builder.line(&format!(
        "@JvmStatic fun register{}(fn: {}): {}.ByValue {{",
        wrapper, wrapper, cb_ffi
    ));
    builder.indent();
    builder.line(&format!("val raw = {} {{", raw_sam));
    builder.indent();
    builder.line(&format!("{} ->", raw_lambda_args.join(", ")));
    builder.line(&format!(
        "val result = fn.invoke({})",
        typed_args.join(", ")
    ));
    builder.line(&format!(
        "val rawStruct = Structure.newInstance({}.ByValue::class.java, result.rawPointer()) as \
         {}.ByValue",
        ffi_ret, ffi_ret
    ));
    builder.line("rawStruct.read()");
    builder.line("val sz = rawStruct.size()");
    builder.line("outPtr?.write(0, rawStruct.pointer.getByteArray(0, sz), 0, sz)");
    // libazul takes ownership of the struct bytes via outPtr.
    builder.line("result.__consume()");
    builder.dedent();
    builder.line("}");
    builder.line(&format!("return register{}(raw as Any)", wrapper));
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line(&format!(
        "@JvmStatic fun register{}(fn: {}): {}.ByValue {{",
        wrapper, raw_sam, cb_ffi
    ));
    builder.indent();
    builder.line(&format!("return register{}(fn as Any)", wrapper));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// How one positional arg of a `<Kind>WithData<T>` SAM reaches the user.
/// Every host-invoker arg arrives as a `const T*` (see
/// `managed_host_invoker::invoker_c_arg_list`); the bridge dereferences it
/// into the most useful Kotlin shape the IR allows.
pub(super) enum KtSamArg {
    /// A wrapper class exists: `X(ptr)`. Non-owning (`owned = false`) when
    /// the type has a `_delete` — the engine owns the pointee for the
    /// duration of the callback, so the wrapper must never free it.
    Wrapper { class: String, has_delete: bool },
    /// Plain-old-data struct without a wrapper class: the raw JNA
    /// `Structure` read over the engine's memory (`AzNumberInputState`).
    PodStruct(String),
    /// Fieldless `repr(C)` enum: a C `int` on the wire → `X.fromInt(...)`.
    UnitEnum(String),
    /// Primitive: read the value at offset 0 (`getLong(0)` for `usize`).
    Primitive { kt: String, getter: String },
    /// Tagged unions and anything the IR does not know: the `Pointer?`.
    RawPointer,
}

/// How the SAM's return value is written back through `outPtr`.
pub(super) enum KtSamRet {
    Void,
    /// Fieldless enum → `outPtr.setInt(0, value)`.
    UnitEnum,
    /// Wrapper class → bytes spliced into `outPtr`, wrapper consumed (`Dom`).
    Wrapper(String),
    /// POD struct without a wrapper (`AzOnTextInputReturn`) → bytes spliced.
    PodStruct(String),
}

pub(super) struct KtDataSamShape {
    /// `(arg kind, SAM parameter name)` for `cb.args[1..]`.
    pub extra_args: Vec<(KtSamArg, String)>,
    pub ret: KtSamRet,
    /// Kotlin return type of the SAM's `invoke`.
    pub return_decl: String,
}


/// The first callback argument whose class can REPORT a failure, as
/// `(Kotlin variable, Error level expression)`.
///
/// Found by SHAPE, not by name: an instance method taking a severity (a
/// unit enum that has an `Error` level) and an owned message string.
/// Exactly one function in the whole API has that shape, and it is the
/// one we want. Spelling its name here instead would mean a rename in
/// api.json silently stopped every failing callback from reporting
/// anything — at runtime, with no build error to notice it by.
fn kt_failure_logger(
    cb: &CallbackTypedefDef,
    shape: &KtDataSamShape,
    ir: &CodegenIR,
) -> Option<(String, String)> {
    let is_message = |t: &str| {
        ir.find_struct(t.trim())
            .is_some_and(|s| matches!(s.category, TypeCategory::String))
    };
    let has_error_level = |t: &str| {
        ir.find_enum(t.trim())
            .filter(|e| !e.is_union)
            .is_some_and(|e| e.variants.iter().any(|v| v.name == "Error"))
    };
    for ((kind, name), a) in shape.extra_args.iter().zip(cb.args.iter().skip(1)) {
        if !matches!(kind, KtSamArg::Wrapper { .. }) {
            continue;
        }
        let ty = a.type_name.trim();
        let f = ir.functions.iter().find(|f| {
            f.class_name == ty
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                && f.args.len() == 3
                && f.is_receiver_arg(&f.args[0])
                && has_error_level(f.args[1].type_name.as_str())
                && is_message(f.args[2].type_name.as_str())
                && matches!(f.args[2].ref_kind, ArgRefKind::Owned)
        })?;
        let level_ty = f.args[1].type_name.trim();
        let e = ir.find_enum(level_ty).filter(|e| !e.is_union)?;
        let variant = e.variants.iter().find(|v| v.name == "Error")?;
        return Some((
            format!("__{}", name),
            format!("{}.{}.value", user_enum_type_name(level_ty), variant.name),
        ));
    }
    None
}

/// THE predicate for "a `<Kind>WithData<T>` typed SAM is emitted for this
/// callback kind": `args[0]` must be the engine's `RefAny` (the host-handle
/// carrier the bridge resolves to `T`) and the return must be void, a unit
/// enum, a wrapper class or a POD struct. The smart setters and the
/// application factory in `wrappers.rs` key on this same function, so they
/// can never reference a SAM that was not emitted.
pub(super) fn kt_data_typed_sam_shape(
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) -> Option<KtDataSamShape> {
    let first = cb.args.first()?;
    if !is_refany_type(&first.type_name, ir) {
        return None;
    }
    let mut extra_args = Vec::new();
    for (i, a) in cb.args.iter().enumerate().skip(1) {
        let t = a.type_name.trim();
        let kind = if has_wrapper_class(t, ir) {
            KtSamArg::Wrapper {
                class: kotlin_class_name(t, ir),
                has_delete: has_delete_function(t, ir),
            }
        } else if let Some((kt, getter)) = kt_primitive_pointer_read(t) {
            KtSamArg::Primitive {
                kt: kt.to_string(),
                getter: getter.to_string(),
            }
        } else if ir.find_enum(t).is_some_and(|e| !e.is_union) {
            KtSamArg::UnitEnum(user_enum_type_name(t))
        } else if ir.find_struct(t).is_some() {
            KtSamArg::PodStruct(ffi_type_name(t))
        } else {
            KtSamArg::RawPointer
        };
        let name = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
        extra_args.push((kind, name));
    }
    let (return_decl, ret) = match cb.return_type.as_deref().map(str::trim) {
        None | Some("void") | Some("()") => ("Unit".to_string(), KtSamRet::Void),
        Some(rt) => {
            if has_wrapper_class(rt, ir) {
                (kotlin_class_name(rt, ir), KtSamRet::Wrapper(ffi_type_name(rt)))
            } else if ir.find_enum(rt).is_some_and(|e| !e.is_union) {
                (user_enum_type_name(rt), KtSamRet::UnitEnum)
            } else if ir.find_struct(rt).is_some() {
                (ffi_type_name(rt), KtSamRet::PodStruct(ffi_type_name(rt)))
            } else {
                return None;
            }
        }
    };
    Some(KtDataSamShape {
        extra_args,
        ret,
        return_decl,
    })
}

/// `(Kotlin type, JNA Pointer getter)` for a primitive IR type that the
/// invoker passes by pointer. `bool` is read as a byte by the emitter.
fn kt_primitive_pointer_read(rust_type: &str) -> Option<(&'static str, &'static str)> {
    Some(match rust_type {
        "u8" | "i8" => ("Byte", "getByte"),
        "u16" | "i16" => ("Short", "getShort"),
        "u32" | "i32" => ("Int", "getInt"),
        "u64" | "i64" | "usize" | "isize" => ("Long", "getLong"),
        "f32" => ("Float", "getFloat"),
        "f64" => ("Double", "getDouble"),
        "bool" => ("Boolean", "getByte"),
        _ => return None,
    })
}

/// The Kotlin parameter type a [`KtSamArg`] shows to the user.
pub(super) fn kt_sam_arg_type(kind: &KtSamArg) -> String {
    match kind {
        KtSamArg::Wrapper { class, .. } => class.clone(),
        KtSamArg::PodStruct(ffi) => ffi.clone(),
        KtSamArg::UnitEnum(name) => name.clone(),
        KtSamArg::Primitive { kt, .. } => kt.clone(),
        KtSamArg::RawPointer => "Pointer?".to_string(),
    }
}

/// Emit `<Kind>WithData<T>` (typed SAM) + the generic
/// `register<Kind>(klass: Class<T>, typed: <Kind>WithData<T>)` overload for
/// one host-invoker kind, driven by [`kt_data_typed_sam_shape`]. The user
/// writes `(data: MyModel, info: CallbackInfo) -> Update` instead of
/// unpacking `Pointer`s; the bridge resolves the host handle, checks the
/// runtime class, wraps every arg, invalidates the borrowed wrappers after
/// the call (they alias engine memory that is gone once the callback
/// returns) and writes the result through `outPtr`.
fn emit_kt_data_typed_invoker_sam(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) {
    let wrapper = wrapper_name(cb);
    let cb_ffi = ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);
    let Some(shape) = kt_data_typed_sam_shape(cb, ir) else {
        return;
    };

    // === Typed SAM (fun interface) ===
    builder.line("/**");
    builder.line(&format!(
        " * Typed Data<T> SAM for {}: first arg is the deref'd-and-cast",
        wrapper
    ));
    builder.line(" * `T` payload of the RefAny; remaining args are wrapper-class");
    builder.line(" * types instead of raw `Pointer`. The matching `register` overload");
    builder.line(" * handles the refanyGet + isInstance check + arg-wrap + outPtr-write");
    builder.line(" * plumbing internally.");
    builder.line(" */");
    builder.line(&format!("fun interface {}WithData<T> {{", wrapper));
    builder.indent();
    let mut iface_params = vec!["data: T".to_string()];
    for (kind, name) in &shape.extra_args {
        iface_params.push(format!("{}: {}", name, kt_sam_arg_type(kind)));
    }
    builder.line(&format!(
        "fun invoke({}): {}",
        iface_params.join(", "),
        shape.return_decl
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    // === Register overload ===
    builder.line("/**");
    builder.line(&format!(
        " * Register a typed Data<T> `{}WithData<T>`. Wraps it in a raw",
        wrapper
    ));
    builder.line(&format!(
        " * `{}InvokerCallback` that performs refanyGet, runtime-class",
        wrapper
    ));
    builder.line(" * check, arg-wrap, and outPtr-write internally. Wrappers handed to the");
    builder.line(" * callback borrow engine memory and are invalidated when it returns.");
    builder.line(" */");
    builder.line(&format!(
        "@JvmStatic fun <T : Any> register{}(klass: Class<T>, typed: {}WithData<T>): {}.ByValue {{",
        wrapper, wrapper, cb_ffi
    ));
    builder.indent();

    // Raw lambda param list mirrors `<Wrapper>InvokerCallback`'s SAM:
    // (id, arg0, ..., [outPtr]) — outPtr omitted on void-return kinds.
    let mut raw_lambda_args = vec!["id".to_string(), "arg0".to_string()];
    for (_kind, name) in &shape.extra_args {
        raw_lambda_args.push(name.clone());
    }
    if has_return(cb) {
        raw_lambda_args.push("outPtr".to_string());
    }

    // `inv@` label: a model of the wrong class `return@inv`s out of the
    // SAM lambda after logging.
    builder.line(&format!("val raw = {} inv@{{", raw_sam));
    builder.indent();
    builder.line(&format!("{} ->", raw_lambda_args.join(", ")));
    builder.line("val __data = refanyGet(arg0)");
    // Wrapper args borrow engine memory for the duration of the call; they
    // are built first so a failure report can already log through them.
    let mut borrowed: Vec<String> = Vec::new();
    for (kind, name) in &shape.extra_args {
        if let KtSamArg::Wrapper { class, has_delete } = kind {
            // The SAM args are platform-typed `Pointer?`; the C thunk
            // always fills these slots, so `!!` documents the contract.
            if *has_delete {
                builder.line(&format!("val __{} = {}({}!!, owned = false)", name, class, name));
            } else {
                builder.line(&format!("val __{} = {}({}!!)", name, class, name));
            }
            borrowed.push(format!("__{}", name));
        }
    }
    // A model of the wrong class or a throwing callback is reported through
    // the first argument with a `log(level, message)` method (CallbackInfo),
    // else on stderr, and never reaches JNA: the engine pre-filled the out
    // slot with the kind's default, so a failed call leaves it alone.
    let logger = kt_failure_logger(cb, &shape, ir);
    let report = |builder: &mut CodeBuilder, msg: &str| match &logger {
        Some((arg, level)) => builder.line(&format!("{}.log({}, {})", arg, level, msg)),
        None => builder.line(&format!("System.err.println({})", msg)),
    };
    builder.line("try {");
    builder.indent();
    builder.line("if (__data == null || !klass.isInstance(__data)) {");
    builder.indent();
    report(
        builder,
        &format!(
            "\"azul: {} expected a model of class \" + klass.name + \", got \" + (__data?.javaClass?.name ?: \"null\")",
            wrapper
        ),
    );
    builder.line("return@inv");
    builder.dedent();
    builder.line("}");
    builder.line("val __typed: T = klass.cast(__data)");
    let mut call_args = vec!["__typed".to_string()];
    for (kind, name) in &shape.extra_args {
        match kind {
            KtSamArg::Wrapper { .. } => call_args.push(format!("__{}", name)),
            KtSamArg::PodStruct(ffi) => {
                builder.line(&format!(
                    "val __{n} = (Structure.newInstance({f}::class.java, {n}!!) as {f}).also {{ it.read() }}",
                    n = name,
                    f = ffi
                ));
                call_args.push(format!("__{}", name));
            }
            KtSamArg::UnitEnum(enum_name) => {
                builder.line(&format!(
                    "val __{n} = {e}.fromInt({n}!!.getInt(0))",
                    n = name,
                    e = enum_name
                ));
                call_args.push(format!("__{}", name));
            }
            KtSamArg::Primitive { kt, getter } => {
                if kt == "Boolean" {
                    builder.line(&format!(
                        "val __{n} = {n}!!.getByte(0) != 0.toByte()",
                        n = name
                    ));
                } else {
                    builder.line(&format!("val __{n} = {n}!!.{g}(0)", n = name, g = getter));
                }
                call_args.push(format!("__{}", name));
            }
            KtSamArg::RawPointer => {
                call_args.push(name.clone());
            }
        }
    }
    match &shape.ret {
        KtSamRet::Void => {
            builder.line(&format!("typed.invoke({})", call_args.join(", ")));
        }
        KtSamRet::UnitEnum => {
            builder.line(&format!(
                "val __result = typed.invoke({})",
                call_args.join(", ")
            ));
            // `enum class X(val value: Int)` — `.value` is already `Int`.
            builder.line("outPtr?.setInt(0, __result.value)");
        }
        KtSamRet::Wrapper(ffi_ret) => {
            builder.line(&format!(
                "val __result = typed.invoke({})",
                call_args.join(", ")
            ));
            builder.line(&format!(
                "val __raw = Structure.newInstance({}.ByValue::class.java, __result.rawPointer()) \
                 as {}.ByValue",
                ffi_ret, ffi_ret
            ));
            builder.line("__raw.read()");
            builder.line("val sz = __raw.size()");
            builder.line("outPtr?.write(0, __raw.pointer.getByteArray(0, sz), 0, sz)");
            // libazul takes ownership of the struct bytes via outPtr.
            builder.line("__result.__consume()");
        }
        KtSamRet::PodStruct(_) => {
            builder.line(&format!(
                "val __result = typed.invoke({})",
                call_args.join(", ")
            ));
            builder.line("__result.write()");
            builder.line("val sz = __result.size()");
            builder.line("outPtr?.write(0, __result.pointer.getByteArray(0, sz), 0, sz)");
        }
    }
    builder.dedent();
    builder.line("} catch (__e: Throwable) {");
    builder.indent();
    report(builder, &format!("\"azul: {} raised \" + __e", wrapper));
    builder.line("__e.printStackTrace()");
    builder.dedent();
    if borrowed.is_empty() {
        builder.line("}");
    } else {
        builder.line("} finally {");
        builder.indent();
        for b in &borrowed {
            builder.line(&format!("{}.__consume()", b));
        }
        builder.dedent();
        builder.line("}");
    }
    builder.dedent();
    builder.line("}");
    builder.line(&format!("return register{}(raw as Any)", wrapper));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `LayoutCallback` → `layoutCallback`: the invoker/field naming used by both
/// the host-invoker object and the wrapper emitter.
pub(super) fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
