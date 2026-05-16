//! Kotlin managed-FFI runtime helpers (host-invoker pattern).
//!
//! Kotlin emits a single `Azul.kt` file containing the JNA `Library`
//! interface, all `Structure` subclasses, all callback interfaces, and
//! the idiomatic wrapper classes. We append a Kotlin `object
//! AzulHostInvoker` to that file with the same surface as Java's
//! `AzulHostInvoker` class, plus `interface AzulNativeManaged` for the
//! host-invoker C-ABI imports.
//!
//! Why a per-language managed.rs (rather than reusing Java's class):
//! Kotlin's bindings are self-contained — there's no Java inter-op
//! requirement on the consumer side. Forcing them to compile a parallel
//! Java module just for the host-invoker would be a worse experience
//! than emitting a small Kotlin object.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};
use super::{ffi_type_name, LIBRARY_NAME};

/// Append the host-invoker block to the existing `Azul.kt` body.
pub fn emit(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// Managed-FFI runtime: host-invoker JNA imports + AzulHostInvoker");
    builder.line("// object. Mirrors the Java AzulNativeManaged + AzulHostInvoker pair.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    // AzulNativeManaged: separate JNA Library for host-invoker C-ABI exports.
    builder.line("interface AzulNativeManaged : Library {");
    builder.indent();
    builder.line("companion object {");
    builder.indent();
    builder.line(&format!(
        "@JvmField val INSTANCE: AzulNativeManaged = Native.load(\"{}\", AzulNativeManaged::class.java)",
        LIBRARY_NAME
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("fun interface HostHandleReleaserCallback : JnaCallback {");
    builder.indent();
    builder.line("fun invoke(id: Long)");
    builder.dedent();
    builder.line("}");
    builder.line("fun AzApp_setHostHandleReleaser(fn: HostHandleReleaserCallback)");
    builder.line("fun AzRefAny_newHostHandle(id: Long): AzRefAny.ByValue");
    builder.line("fun AzRefAny_getHostHandle(refanyPtr: Pointer?): Long");
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
            "fun AzApp_set{w}Invoker(fn: {w}InvokerCallback)",
            w = wrapper
        ));
        builder.line(&format!(
            "fun Az{w}_createFromHostHandle(id: Long): Az{w}.ByValue",
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
    builder.line("AzulNativeManaged.INSTANCE.AzApp_setHostHandleReleaser(releaser)");
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
        builder.line(&format!(
            "fn.invoke({})",
            forward_args.join(", ")
        ));
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.line(&format!("livePins.add({}Invoker)", lower_first(wrapper)));
        builder.line(&format!(
            "AzulNativeManaged.INSTANCE.AzApp_set{w}Invoker({l}Invoker)",
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
            "return AzulNativeManaged.INSTANCE.Az{}_createFromHostHandle(id)",
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
    builder.line("return AzulNativeManaged.INSTANCE.AzRefAny_newHostHandle(id)");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("fun refanyGet(refanyPtr: Pointer?): Any? {");
    builder.indent();
    builder.line("val id = AzulNativeManaged.INSTANCE.AzRefAny_getHostHandle(refanyPtr)");
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

    // Phase CC-1 (Kotlin): Data<T>-typed SAM bridge. Mirrors
    // `lang_java/managed::emit_data_typed_invoker_sam` (commit
    // 533df7ab5). The user writes
    //   (data: MyDataModel, info: LayoutCallbackInfo) -> Dom
    // instead of unpacking `Pointer dataPtr` themselves. Per the
    // user-locked CC-1 scope: iterate all HOST_INVOKER_KINDS at
    // once; fall back per-kind (skip emit) on non-conforming
    // signatures; don't abort the whole arc.
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
fn emit_kt_typed_invoker_sam(
    builder: &mut super::super::generator::CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &super::super::ir::CodegenIR,
) {
    use super::super::ir::FunctionKind;
    let wrapper = wrapper_name(cb);
    let cb_has_return = has_return(cb);
    if !cb_has_return {
        return;
    }
    let Some(ret_ty) = cb.return_type.as_deref() else {
        return;
    };
    let ret_ty = ret_ty.trim();
    let Some(ret_struct) = ir.find_struct(ret_ty) else {
        return;
    };
    if !ir.functions.iter().any(|f| {
        f.class_name == ret_ty && matches!(f.kind, FunctionKind::Delete)
    }) {
        return;
    }
    if matches!(
        ret_struct.category,
        super::super::ir::TypeCategory::Recursive
            | super::super::ir::TypeCategory::VecRef
            | super::super::ir::TypeCategory::DestructorOrClone
            | super::super::ir::TypeCategory::GenericTemplate
    ) {
        return;
    }

    let wrapper_class = ret_ty.to_string();
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
    builder.line(
        " * bridge handles the struct-byte splice into outPtr internally.",
    );
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
    builder.line(&format!(
        "val raw = {} {{",
        raw_sam
    ));
    builder.indent();
    builder.line(&format!("{} ->", raw_lambda_args.join(", ")));
    builder.line(&format!(
        "val result = fn.invoke({})",
        typed_args.join(", ")
    ));
    builder.line(&format!(
        "val rawStruct = Structure.newInstance({}.ByValue::class.java, result.rawPointer()) as {}.ByValue",
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

/// Phase CC-1 (Kotlin): emit `<Wrapper>WithData<T>` typed SAM +
/// generic `register<Wrapper>(klass: Class<T>, typed: ...)` overload.
/// Mirror of `lang_java/managed::emit_data_typed_invoker_sam`. Same
/// conformance probe: first arg must be `RefAny`; non-wrapper-class
/// args fall back to `Pointer?`; return must be void / enum /
/// wrapper struct (skip otherwise).
fn emit_kt_data_typed_invoker_sam(
    builder: &mut super::super::generator::CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &super::super::ir::CodegenIR,
) {
    use super::super::ir::FunctionKind;
    let wrapper = wrapper_name(cb);
    let cb_ffi = ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);

    // Probe #1: first arg = RefAny.
    let first = cb.args.first();
    if first.map(|a| a.type_name.trim() != "RefAny").unwrap_or(true) {
        return;
    }

    // Subsequent args: wrapper class when available, else raw Pointer?.
    enum ArgKind {
        Wrapper(String),
        RawPointer,
    }
    let mut extra_args: Vec<(ArgKind, String)> = Vec::new();
    for (i, a) in cb.args.iter().enumerate().skip(1) {
        let t = a.type_name.trim();
        let kind = if kt_managed_has_wrapper_class(t, ir) {
            ArgKind::Wrapper(t.to_string())
        } else {
            ArgKind::RawPointer
        };
        let name = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
        extra_args.push((kind, name));
    }

    // Probe #2: return type plumbing.
    enum RetShape {
        Void,
        Enum,
        WrapperStruct,
    }
    let (return_decl, ret_shape) = match cb.return_type.as_deref().map(str::trim) {
        None => ("Unit".to_string(), RetShape::Void),
        Some("void") => ("Unit".to_string(), RetShape::Void),
        Some(rt) => {
            if kt_managed_has_wrapper_class(rt, ir) {
                (rt.to_string(), RetShape::WrapperStruct)
            } else if ir.find_enum(rt).is_some() {
                (ffi_type_name(rt), RetShape::Enum)
            } else {
                return;
            }
        }
    };
    let _ = std::marker::PhantomData::<FunctionKind>;

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
    for (kind, name) in &extra_args {
        let ty = match kind {
            ArgKind::Wrapper(t) => t.clone(),
            ArgKind::RawPointer => "Pointer?".to_string(),
        };
        iface_params.push(format!("{}: {}", name, ty));
    }
    builder.line(&format!(
        "fun invoke({}): {}",
        iface_params.join(", "),
        return_decl
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
    builder.line(" * check, arg-wrap, and outPtr-write internally.");
    builder.line(" */");
    builder.line(&format!(
        "@JvmStatic fun <T : Any> register{}(klass: Class<T>, typed: {}WithData<T>): {}.ByValue {{",
        wrapper, wrapper, cb_ffi
    ));
    builder.indent();

    // Raw lambda param list mirrors `<Wrapper>InvokerCallback`'s SAM:
    // (id, arg0, ..., [outPtr]) — outPtr omitted on void-return kinds.
    let mut raw_lambda_args = vec!["id".to_string(), "arg0".to_string()];
    for (_kind, name) in &extra_args {
        raw_lambda_args.push(name.clone());
    }
    if has_return(cb) {
        raw_lambda_args.push("outPtr".to_string());
    }

    // Use an `inv@` label on the SAM lambda so the early-skip on
    // type mismatch can `return@inv` cleanly. Kotlin SAM lambdas
    // don't have an implicit name we can label-return to.
    builder.line(&format!("val raw = {} inv@{{", raw_sam));
    builder.indent();
    builder.line(&format!("{} ->", raw_lambda_args.join(", ")));
    builder.line("val __data = refanyGet(arg0)");
    // Kotlin's `Class<T>.isInstance(null)` returns false → null
    // payloads silently skip dispatch. Match Java's semantics.
    builder.line("if (__data != null && !klass.isInstance(__data)) return@inv");
    // Build wrapper-class args; pass Pointer args through.
    let mut call_args = vec!["__typed".to_string()];
    builder.line("@Suppress(\"UNCHECKED_CAST\")");
    builder.line("val __typed = __data as T");
    for (kind, name) in &extra_args {
        match kind {
            ArgKind::Wrapper(ty) => {
                // Wrapper class constructors take non-null `Pointer`;
                // the SAM args are platform-typed `Pointer?`. Force-
                // unwrap with `!!` — the C-side invoker thunk always
                // populates these slots; a null here would mean the
                // underlying libazul thunk crashed already.
                builder.line(&format!("val __{} = {}({}!!)", name, ty, name));
                call_args.push(format!("__{}", name));
            }
            ArgKind::RawPointer => {
                call_args.push(name.clone());
            }
        }
    }
    match ret_shape {
        RetShape::Void => {
            builder.line(&format!("typed.invoke({})", call_args.join(", ")));
        }
        RetShape::Enum => {
            builder.line(&format!(
                "val __result = typed.invoke({})",
                call_args.join(", ")
            ));
            // `enum class AzUpdate(val value: Int)` — `.value` is
            // already `Int`; no `.toLong()` conversion needed (and
            // `Pointer.setInt` rejects Long).
            builder.line("outPtr?.setInt(0, __result.value)");
        }
        RetShape::WrapperStruct => {
            let ffi_ret = ffi_type_name(&return_decl);
            builder.line(&format!(
                "val __result = typed.invoke({})",
                call_args.join(", ")
            ));
            builder.line(&format!(
                "val __raw = Structure.newInstance({}.ByValue::class.java, __result.rawPointer()) as {}.ByValue",
                ffi_ret, ffi_ret
            ));
            builder.line("__raw.read()");
            builder.line("val sz = __raw.size()");
            builder.line("outPtr?.write(0, __raw.pointer.getByteArray(0, sz), 0, sz)");
            builder.line("__result.__consume()");
        }
    }
    builder.dedent();
    builder.line("}");
    builder.line(&format!("return register{}(raw as Any)", wrapper));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Mirror of `lang_kotlin/wrappers.rs::has_kt_wrapper_class` kept
/// local to managed.rs so the helper there can stay private.
fn kt_managed_has_wrapper_class(
    type_name: &str,
    ir: &super::super::ir::CodegenIR,
) -> bool {
    use super::super::ir::{FunctionKind, TypeCategory};
    let Some(s) = ir.find_struct(type_name) else {
        return false;
    };
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && matches!(f.kind, FunctionKind::Delete))
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
