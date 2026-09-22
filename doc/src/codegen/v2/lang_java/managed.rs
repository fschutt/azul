//! Java managed-FFI runtime helpers (host-invoker pattern).
//!
//! JNA's `com.sun.jna.Callback` interface synthesises C-callable
//! trampolines from JVM method handles, so unlike LuaJIT / koffi /
//! ruby-ffi, Java doesn't *need* the host-invoker pattern. We still
//! apply it uniformly because the wrapper layer is simpler when every
//! managed-FFI host shares one shape.
//!
//! ## What this emits
//!
//! Two new Java source files (under the multi-file `// ==FILE:` scheme
//! `lang_java/mod.rs` already uses):
//!
//! 1. **`AzulNativeManaged.java`** — `interface AzulNativeManaged extends Library` carrying the
//!    host-invoker C-ABI imports (`AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`,
//!    `AzRefAny_getHostHandle`, plus per-kind invoker setters and `Az<Kind>_createFromHostHandle`
//!    constructors).
//! 2. **`AzulHostInvoker.java`** — `public class AzulHostInvoker` with static state (id→Object
//!    dictionary, GC pin list, init flag), `ensureInitialized()`, public
//!    `register<Kind>Callback(callback)` factories per kind, and `refanyCreate(Object)` /
//!    `refanyGet(Pointer)` user-data helpers.
//!
//! Per-kind callback interfaces are defined inline as nested static
//! interfaces extending `Callback` so users can `implements
//! AzulHostInvoker.CallbackHandler` etc.

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        ir::{ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionArg, FunctionKind, TypeCategory},
        managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name},
        managed_lang_helpers::{has_wrapper_class, is_refany_type},
    },
    emit_file, ffi_type_name, map_jvm_type, map_jvm_type_byvalue, sanitize_identifier,
    types::should_include_struct, user_enum_type_name,
    wrappers::wrapper_class_name, LIBRARY_NAME,
};

/// Generate `AzulNativeManaged.java` + `AzulHostInvoker.java` and append
/// them to `out` using the same `FILE_MARKER` / `END_LINE` framing every
/// other Java file uses.
pub fn emit_files(out: &mut String, ir: &CodegenIR, config: &CodegenConfig) -> Result<()> {
    out.push_str(&emit_file(
        "AzulNativeManaged.java",
        |b| {
            // `emit_file` already imported Library/Native/Pointer/Structure.
            b.line("import com.sun.jna.Callback;");
            b.blank();
            b.line("/**");
            b.line(" * P/Invoke surface for libazul's host-invoker C-ABI exports.");
            b.line(" * Kept in a separate Library interface from `AzulNative` so the");
            b.line(" * regular function-binding emitter stays linear.");
            b.line(" */");
            b.line("public interface AzulNativeManaged extends Library {");
            b.indent();
            b.line(&format!(
                "AzulNativeManaged INSTANCE = Native.load(\"{}\", AzulNativeManaged.class);",
                LIBRARY_NAME
            ));
            b.blank();

            // Releaser + RefAny new/get
            b.line("interface HostHandleReleaserCallback extends Callback {");
            b.indent();
            b.line("void invoke(long id);");
            b.dedent();
            b.line("}");
            b.line("void AzApp_setHostHandleReleaser(HostHandleReleaserCallback fn);");
            b.line("AzRefAny.ByValue AzRefAny_newHostHandle(long id);");
            b.line("long AzRefAny_getHostHandle(Pointer refanyPtr);");
            b.blank();

            for cb in host_invoker_kinds(ir) {
                let wrapper = wrapper_name(cb);
                let cb_has_return = has_return(cb);
                // Inline JNA Callback interface for the per-kind invoker.
                b.line(&format!(
                    "interface {}InvokerCallback extends Callback {{",
                    wrapper
                ));
                b.indent();
                let mut params = vec!["long id".to_string()];
                for (i, a) in cb.args.iter().enumerate() {
                    let nm = if a.name.is_empty() {
                        format!("arg{}", i)
                    } else {
                        a.name.clone()
                    };
                    params.push(format!("Pointer {}", nm));
                }
                if cb_has_return {
                    params.push("Pointer outPtr".to_string());
                }
                b.line(&format!("void invoke({});", params.join(", ")));
                b.dedent();
                b.line("}");
                b.line(&format!(
                    "void AzApp_set{w}Invoker({w}InvokerCallback fn);",
                    w = wrapper
                ));
                b.line(&format!(
                    "Az{w}.ByValue Az{w}_createFromHostHandle(long id);",
                    w = wrapper
                ));
                b.blank();
            }

            b.dedent();
            b.line("}");
            Ok(())
        },
        config,
    )?);

    out.push_str(&emit_file(
        "AzulHostInvoker.java",
        |b| {
            // `emit_file` already imported Pointer/Structure/java.util.List.
            b.line("import java.util.ArrayList;");
            b.line("import java.util.HashMap;");
            b.line("import java.util.Map;");
            b.blank();

            b.line("/**");
            b.line(" * Managed-FFI runtime: host-invoker public surface.");
            b.line(" *");
            b.line(" * `register<Kind>Callback(...)` wraps a JNA callback in the");
            b.line(" * matching `Az<Kind>` cdata struct so a native call site can");
            b.line(" * store it. `refanyCreate(Object)` / `refanyGet(Pointer)` share");
            b.line(" * the same id-keyed table — the framework's RefAny destructor");
            b.line(" * calls back through the registered releaser to drop entries.");
            b.line(" */");
            b.line("public final class AzulHostInvoker {");
            b.indent();

            b.line("private static final Map<Long, Object> handles = new HashMap<>();");
            b.line("private static long nextHandleId = 0;");
            b.line("private static final List<Object> livePins = new ArrayList<>();");
            // `volatile`: the double-checked-locking fast path in
            // ensureInitialized() reads it outside the lock.
            b.line("private static volatile boolean initialized = false;");
            b.line("private static final Object initLock = new Object();");
            b.blank();

            b.line("private AzulHostInvoker() {}");
            b.blank();

            b.line("private static void ensureInitialized() {");
            b.indent();
            b.line("if (initialized) return;");
            b.line("synchronized (initLock) {");
            b.indent();
            b.line("if (initialized) return;");
            b.line("initialized = true;");
            b.blank();
            b.line("// Releaser");
            b.line("AzulNativeManaged.HostHandleReleaserCallback releaser = (long id) -> {");
            b.indent();
            // Same boundary rule as every other trampoline: libazul calls this
            // one while dropping a RefAny, and a Throwable unwinding out of a
            // JNA callback frame into Rust is undefined behaviour. There is no
            // CallbackInfo here, so stderr is the only sink.
            b.line("try {");
            b.indent();
            b.line("synchronized (handles) { handles.remove(id); }");
            b.dedent();
            b.line("} catch (Throwable __e) {");
            b.indent();
            b.line(
                "System.err.println(\"azul: host-handle releaser threw \" + \
                 __e.getClass().getName() + \": \" + __e.getMessage());",
            );
            b.line("__e.printStackTrace();");
            b.dedent();
            b.line("}");
            b.dedent();
            b.line("};");
            b.line("livePins.add(releaser);");
            b.line("AzulNativeManaged.INSTANCE.AzApp_setHostHandleReleaser(releaser);");
            b.blank();

            for cb in host_invoker_kinds(ir) {
                emit_per_kind_init(b, cb, ir);
            }

            b.dedent();
            b.line("}");
            b.dedent();
            b.line("}");
            b.blank();

            // Per-kind RegisterCallback
            for cb in host_invoker_kinds(ir) {
                let wrapper = wrapper_name(cb);
                b.line("/**");
                b.line(&format!(
                    " * Wrap a {} handler in the matching Az{} cdata struct.",
                    wrapper, wrapper
                ));
                b.line(
                    " * @param fn user callback (must implement the kind's `*InvokerCallback` \
                     interface).",
                );
                b.line(" */");
                b.line(&format!(
                    "public static Az{w}.ByValue register{w}(Object fn) {{",
                    w = wrapper
                ));
                b.indent();
                b.line("ensureInitialized();");
                b.line("long id;");
                b.line("synchronized (handles) {");
                b.indent();
                b.line("nextHandleId++;");
                b.line("id = nextHandleId;");
                b.line("handles.put(id, fn);");
                b.dedent();
                b.line("}");
                b.line(&format!(
                    "return AzulNativeManaged.INSTANCE.Az{}_createFromHostHandle(id);",
                    wrapper
                ));
                b.dedent();
                b.line("}");
                b.blank();
            }

            // RefanyCreate / RefanyGet
            b.line("/**");
            b.line(" * Wrap an arbitrary Java object in an AzRefAny held alive by the");
            b.line(" * framework's refcount.");
            b.line(" */");
            b.line("public static AzRefAny.ByValue refanyCreate(Object value) {");
            b.indent();
            b.line("ensureInitialized();");
            b.line("long id;");
            b.line("synchronized (handles) {");
            b.indent();
            b.line("nextHandleId++;");
            b.line("id = nextHandleId;");
            b.line("handles.put(id, value);");
            b.dedent();
            b.line("}");
            b.line("return AzulNativeManaged.INSTANCE.AzRefAny_newHostHandle(id);");
            b.dedent();
            b.line("}");
            b.blank();

            b.line("public static Object refanyGet(Pointer refanyPtr) {");
            b.indent();
            b.line("long id = AzulNativeManaged.INSTANCE.AzRefAny_getHostHandle(refanyPtr);");
            b.line("if (id == 0) return null;");
            b.line("synchronized (handles) {");
            b.indent();
            b.line("return handles.get(id);");
            b.dedent();
            b.line("}");
            b.dedent();
            b.line("}");
            b.blank();

            // Phase CC-5: wrap an Object directly in the `RefAny`
            // wrapper class (rather than the raw `AzRefAny.ByValue`
            // FFI struct). Saves the user from doing
            // `new RefAny(refanyCreate(model).getPointer())` at every
            // `App.create(...)` call site.
            b.line("/**");
            b.line(" * Wrap an arbitrary Java object in a `RefAny` wrapper class.");
            b.line(" * Convenience over `refanyCreate(Object)` which returns the");
            b.line(" * raw `AzRefAny.ByValue` FFI struct; this is the form most");
            b.line(" * wrapper-class call sites (`App.create`, etc.) accept.");
            b.line(" */");
            b.line("public static RefAny refanyWrap(Object value) {");
            b.indent();
            b.line("AzRefAny.ByValue raw = refanyCreate(value);");
            b.line("return new RefAny(raw.getPointer());");
            b.dedent();
            b.line("}");
            b.blank();

            // Phase CC-2: typed-SAM bridge per kind with wrapper-class
            // return. Iterates `host_invoker_kinds(ir)`; for each kind
            // whose return is a struct with an emitted wrapper class,
            // emit a typed `<Wrapper>Callback` interface (returns the
            // wrapper) and a `register<Wrapper>(<Wrapper>Callback)`
            // overload that wraps typed → raw with the
            // Structure.newInstance + read + outPtr.write splice.
            // Everything driven by IR metadata; no class names or
            // ABI symbols hardcoded.
            for cb in host_invoker_kinds(ir) {
                emit_typed_invoker_sam(b, cb, ir);
            }

            // Phase CC-1: Data<T> typed-SAM bridge per kind. Emits a
            // `<Wrapper>WithData<T>` SAM whose first arg is the
            // deref'd-and-cast `T` payload of the RefAny, and whose
            // remaining args are the natural wrapper-class types
            // (e.g. `LayoutCallbackInfo`, `CallbackInfo`) instead of
            // raw `Pointer`. The matching
            // `register<Wrapper>(Class<T>, <Wrapper>WithData<T>)`
            // overload wires the refanyGet + cast + arg-wrap +
            // outPtr-write plumbing internally. Per the user-locked
            // CC-1 scope: iterate ALL HOST_INVOKER_KINDS at once;
            // fall back per-kind (skip emit) if the signature is
            // non-conforming (first arg not RefAny / non-wrappable
            // arg type / return is neither void nor enum nor wrapper
            // struct). Don't abort the whole arc on one mismatch.
            for cb in host_invoker_kinds(ir) {
                emit_data_typed_invoker_sam(b, cb, ir, config);
            }

            b.dedent();
            b.line("}");
            Ok(())
        },
        config,
    )?);

    Ok(())
}

fn emit_per_kind_init(
    b: &mut super::super::generator::CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    let wrapper = wrapper_name(cb);
    let cb_has_return = has_return(cb);

    let mut params = vec!["long id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        params.push(format!("Pointer {}", invoker_arg_name(i, a)));
    }
    if cb_has_return {
        params.push("Pointer outPtr".to_string());
    }

    b.line(&format!("// {} invoker", wrapper));
    b.line(&format!(
        "AzulNativeManaged.{w}InvokerCallback {l}Invoker = ({p}) -> {{",
        w = wrapper,
        l = lower_first(wrapper),
        p = params.join(", ")
    ));
    b.indent();
    // THE boundary. libazul calls this lambda through JNA, and every
    // application callback of this kind — the typed SAMs below, a raw
    // `<Kind>InvokerCallback` the user wrote by hand — is dispatched from
    // inside it. A Throwable that unwinds from here into Rust is undefined
    // behaviour, not merely a crash, so everything is caught, reported and
    // swallowed. The engine pre-filled the out slot with this kind's default
    // before calling, so not writing it IS returning the fallback.
    b.line("try {");
    b.indent();
    b.line("Object fn;");
    b.line("synchronized (handles) { fn = handles.get(id); }");
    b.line("if (fn == null) return;");
    b.line("// Dispatch is left to the user-side handler — JNA does not");
    b.line("// expose Method.invoke through Callback. The user passes a");
    b.line("// concrete <Wrapper>InvokerCallback to register*Callback.");
    b.line("if (fn instanceof AzulNativeManaged.");
    b.line(&format!("    {}InvokerCallback) {{", wrapper));
    b.indent();
    let mut handler_args = vec!["id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        handler_args.push(invoker_arg_name(i, a));
    }
    if cb_has_return {
        handler_args.push("outPtr".to_string());
    }
    b.line(&format!(
        "((AzulNativeManaged.{}InvokerCallback) fn).invoke({});",
        wrapper,
        handler_args.join(", ")
    ));
    b.dedent();
    b.line("}");
    b.dedent();
    emit_boundary_catch(b, wrapper, &pointer_sink(cb, ir), &[]);
    b.dedent();
    b.line("};");
    b.line(&format!("livePins.add({}Invoker);", lower_first(wrapper)));
    b.line(&format!(
        "AzulNativeManaged.INSTANCE.AzApp_set{}Invoker({}Invoker);",
        wrapper,
        lower_first(wrapper)
    ));
    b.blank();
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Emit a typed-SAM bridge for one host-invoker callback kind:
///
///   interface <Wrapper>Callback {
///       <ReturnWrapper> invoke(long id, Pointer arg0, ..., Pointer argN);
///   }
///   public static Az<Wrapper>.ByValue register<Wrapper>(<Wrapper>Callback fn) { ... }
///
/// Skips kinds whose return is not a wrapper-class struct — the
/// caller still has the raw `<Wrapper>InvokerCallback` four-arg
/// outPtr-write SAM for those.
fn emit_typed_invoker_sam(
    b: &mut super::super::generator::CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &super::super::ir::CodegenIR,
) {
    let wrapper = wrapper_name(cb);
    if !has_return(cb) {
        return;
    }
    let Some(ret_ty) = cb.return_type.as_deref() else {
        return;
    };
    let ret_ty = ret_ty.trim();
    // Only emit when the return type is a struct with an emitted
    // wrapper class (the same predicate `wrappers.rs` uses to emit the
    // class, so the two can never drift). Primitive / enum returns
    // (e.g. Update for ButtonOnClickCallback) keep using the raw
    // outPtr-write path because the typed wrapper would just be a
    // boxed primitive without a meaningful splice savings.
    if !has_wrapper_class(ret_ty, ir) {
        return;
    }

    let wrapper_class = wrapper_class_name(ret_ty);
    let ffi_ret = ffi_type_name(ret_ty);
    let cb_ffi = ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);

    // Typed interface signature: `(long id, Pointer arg0, ..., Pointer argN) -> <Wrapper>`.
    let mut typed_params = vec!["long id".to_string()];
    let mut typed_args = vec!["id".to_string()];
    let mut raw_lambda_params = vec!["long id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        let nm = invoker_arg_name(i, a);
        typed_params.push(format!("Pointer {}", nm));
        typed_args.push(nm.clone());
        raw_lambda_params.push(format!("Pointer {}", nm));
    }
    raw_lambda_params.push("Pointer outPtr".to_string());

    b.line("/**");
    b.line(&format!(
        " * Typed {} SAM. Returns a `{}` wrapper directly; the host-invoker",
        wrapper, wrapper_class
    ));
    b.line(" * bridge handles the struct-byte splice into outPtr internally.");
    b.line(" */");
    b.line("@FunctionalInterface");
    b.line(&format!("public interface {} {{", wrapper));
    b.indent();
    b.line(&format!(
        "{} invoke({});",
        wrapper_class,
        typed_params.join(", ")
    ));
    b.dedent();
    b.line("}");
    b.blank();

    b.line("/**");
    b.line(&format!(
        " * Register a typed `{}`. Wraps it in a raw",
        wrapper
    ));
    b.line(&format!(
        " * `{}InvokerCallback` that performs the `{}`-byte splice",
        wrapper, ret_ty
    ));
    b.line(" * into outPtr; delegates to the generic Object overload.");
    b.line(" */");
    b.line(&format!(
        "public static {}.ByValue register{}({} fn) {{",
        cb_ffi, wrapper, wrapper
    ));
    b.indent();
    b.line(&format!(
        "{} raw = ({}) -> {{",
        raw_sam,
        raw_lambda_params.join(", ")
    ));
    b.indent();
    // Boundary: whatever the application's SAM throws is caught, reported to
    // its own log sink and swallowed. outPtr keeps the default libazul
    // pre-filled, which is this kind's fallback. See `emit_boundary_catch`.
    b.line("try {");
    b.indent();
    b.line(&format!(
        "{} result = fn.invoke({});",
        wrapper_class,
        typed_args.join(", ")
    ));
    b.line("if (result == null) return;");
    b.line(&format!("{}.ByValue raw_struct =", ffi_ret));
    b.indent();
    b.line(&format!(
        "({}.ByValue) Structure.newInstance({}.ByValue.class, result.rawPointer());",
        ffi_ret, ffi_ret
    ));
    b.dedent();
    b.line("raw_struct.read();");
    b.line("int sz = raw_struct.size();");
    b.line("outPtr.write(0, raw_struct.getPointer().getByteArray(0, sz), 0, sz);");
    // libazul takes ownership of the struct bytes via outPtr; the
    // user's wrapper would otherwise double-drop on GC.
    b.line("result.__consume();");
    b.dedent();
    emit_boundary_catch(b, wrapper, &pointer_sink(cb, ir), &[]);
    b.dedent();
    b.line("};");
    b.line(&format!("return register{}((Object) raw);", wrapper));
    b.dedent();
    b.line("}");
    b.blank();

    // Typed overload of register that takes the raw SAM so the smart
    // factory's overload resolution has an exact-type match.
    b.line(&format!(
        "public static {}.ByValue register{}({} fn) {{",
        cb_ffi, wrapper, raw_sam
    ));
    b.indent();
    b.line(&format!("return register{}((Object) fn);", wrapper));
    b.dedent();
    b.line("}");
    b.blank();
}

// ============================================================================
// Phase CC-1 — `<Wrapper>WithData<T>` typed SAMs
// ============================================================================

/// How one positional argument of a `<Wrapper>WithData<T>` SAM is
/// surfaced to the user. Derived purely from the IR shape of the callback
/// typedef's arg type — no type names:
///
/// * a struct with a wrapper class → the wrapper (`CallbackInfo`), built with its `(Pointer)`
///   constructor;
/// * any other struct that `types.rs` emits as a JNA `Structure` → the `Az<T>` FFI struct, built
///   with the `Az<T>(Pointer)` overlay constructor (a read-only snapshot);
/// * a unit enum → the user-facing Java enum, decoded with `X.fromInt(ptr.getInt(0))`;
/// * a primitive (after alias resolution) → the JVM primitive, read through `Pointer.getXxx(0)`;
/// * everything else (tagged unions, monomorphized aliases, callback typedefs, recursive types) →
///   the raw `Pointer` the invoker received.
pub(super) enum SamArg {
    Wrapper(String),
    Pod(String),
    Enum(String),
    Prim { jvm: String, getter: &'static str },
    RawPointer,
}

/// How the SAM's return value is written back through the invoker's
/// out-pointer.
pub(super) enum SamRet {
    Void,
    /// Unit enum: `outPtr.setInt(0, result.value)`.
    Enum(String),
    /// Struct with a wrapper class: byte-splice of the wrapped value, then `__consume()`.
    Wrapper { class: String, ffi: String },
    /// Plain JNA `Structure` without a wrapper (e.g. `AzOnTextInputReturn`): byte-splice.
    Pod(String),
}

impl SamArg {
    pub(super) fn java_type(&self) -> String {
        match self {
            SamArg::Wrapper(t) | SamArg::Pod(t) | SamArg::Enum(t) => t.clone(),
            SamArg::Prim { jvm, .. } => jvm.clone(),
            SamArg::RawPointer => "Pointer".to_string(),
        }
    }
}

impl SamRet {
    pub(super) fn java_type(&self) -> String {
        match self {
            SamRet::Void => "void".to_string(),
            SamRet::Enum(t) | SamRet::Pod(t) => t.clone(),
            SamRet::Wrapper { class, .. } => class.clone(),
        }
    }
}

/// IR-derived description of the `<Wrapper>WithData<T>` SAM for one
/// host-invoker kind, or `None` when the kind is not conformant (its
/// first arg is not the engine's `RefAny`, or its return type has no
/// out-pointer encoding). Shared by the `AzulHostInvoker` emitter and by
/// the wrapper-class emitter (`wrappers.rs`), which emits the typed `<T>`
/// builder siblings and `App.create(T, fn)` only for kinds that got their
/// SAM — the two decisions cannot drift apart.
pub(super) struct DataTypedSam {
    /// Callback wrapper kind (`"ButtonOnClickCallback"`).
    pub wrapper: String,
    /// Its FFI struct (`"AzButtonOnClickCallback"`).
    pub cb_ffi: String,
    /// Every arg after the leading `RefAny`, with its Java parameter name.
    pub args: Vec<(SamArg, String)>,
    pub ret: SamRet,
}

pub(super) fn data_typed_sam_info(
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<DataTypedSam> {
    let wrapper = wrapper_name(cb);
    // Conformance probe #1: the first arg is the engine's RefAny (the
    // data slot typed via `<T>`).
    let first = cb.args.first()?;
    if !is_refany_type(&first.type_name, ir) {
        return None;
    }
    let is_pod = |t: &str| {
        ir.find_struct(t)
            .is_some_and(|s| should_include_struct(s, config))
    };
    let is_unit_enum = |t: &str| ir.find_enum(t).is_some_and(|e| !e.is_union);

    let mut args = Vec::new();
    for (i, a) in cb.args.iter().enumerate().skip(1) {
        let t = a.type_name.trim();
        let kind = if has_wrapper_class(t, ir) {
            SamArg::Wrapper(wrapper_class_name(t))
        } else if is_pod(t) {
            SamArg::Pod(ffi_type_name(t))
        } else if is_unit_enum(t) {
            SamArg::Enum(user_enum_type_name(t))
        } else {
            let jvm = map_jvm_type(t, ir);
            let getter = match jvm.as_str() {
                "byte" => Some("getByte"),
                "short" => Some("getShort"),
                "int" => Some("getInt"),
                "long" => Some("getLong"),
                "float" => Some("getFloat"),
                "double" => Some("getDouble"),
                _ => None,
            };
            match getter {
                Some(getter) => SamArg::Prim { jvm, getter },
                None => SamArg::RawPointer,
            }
        };
        args.push((kind, invoker_arg_name(i, a)));
    }

    // Conformance probe #2: the return type must have an out-pointer
    // encoding we know how to write.
    let ret = if !has_return(cb) {
        SamRet::Void
    } else {
        let rt = cb.return_type.as_deref().map(str::trim).unwrap_or("void");
        if has_wrapper_class(rt, ir) {
            SamRet::Wrapper {
                class: wrapper_class_name(rt),
                ffi: ffi_type_name(rt),
            }
        } else if is_unit_enum(rt) {
            SamRet::Enum(user_enum_type_name(rt))
        } else if is_pod(rt) {
            SamRet::Pod(ffi_type_name(rt))
        } else {
            return None;
        }
    };

    Some(DataTypedSam {
        wrapper: wrapper.to_string(),
        cb_ffi: ffi_type_name(wrapper),
        args,
        ret,
    })
}

/// [`data_typed_sam_info`] looked up by callback wrapper kind
/// (`"ButtonOnClickCallback"`). `None` when the kind is not a
/// host-invoker kind or is not conformant.
pub(super) fn data_typed_sam_for_kind(
    kind: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<DataTypedSam> {
    host_invoker_kinds(ir)
        .find(|cb| wrapper_name(cb) == kind)
        .and_then(|cb| data_typed_sam_info(cb, ir, config))
}

/// Phase CC-1 (Java): emit `<Wrapper>WithData<T>` typed SAM + a
/// `register<Wrapper>(Class<T> klass, <Wrapper>WithData<T> typed)`
/// overload that handles refanyGet + cast + arg-decode + outPtr-write
/// internally. Non-conformant kinds (see [`data_typed_sam_info`]) are
/// skipped individually; the arc never aborts on one mismatch.
fn emit_data_typed_invoker_sam(
    b: &mut super::super::generator::CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let Some(sam) = data_typed_sam_info(cb, ir, config) else {
        return;
    };
    let wrapper = sam.wrapper.as_str();
    let cb_ffi = sam.cb_ffi.as_str();
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);
    let return_decl = sam.ret.java_type();

    // === Typed SAM interface ===
    b.line("/**");
    b.line(&format!(
        " * Typed Data&lt;T&gt; SAM for {}: first arg is the deref'd-and-cast",
        wrapper
    ));
    b.line(" * `T` payload of the RefAny; remaining args are wrapper-class /");
    b.line(" * FFI-struct / enum / primitive values instead of raw `Pointer`s.");
    b.line(" * The matching `register` overload handles the refanyGet +");
    b.line(" * isInstance check + arg decoding + outPtr-write plumbing.");
    b.line(" */");
    b.line("@FunctionalInterface");
    b.line(&format!("public interface {}WithData<T> {{", wrapper));
    b.indent();
    let mut iface_params = vec!["T data".to_string()];
    for (kind, name) in &sam.args {
        iface_params.push(format!("{} {}", kind.java_type(), name));
    }
    b.line(&format!(
        "{} invoke({});",
        return_decl,
        iface_params.join(", ")
    ));
    b.dedent();
    b.line("}");
    b.blank();

    // === Register overload ===
    b.line("/**");
    b.line(&format!(
        " * Register a typed Data&lt;T&gt; `{}WithData&lt;T&gt;`. Wraps the typed SAM",
        wrapper
    ));
    b.line(" * into the raw invoker; performs the refanyGet, runtime-class");
    b.line(" * check, arg decoding, and outPtr-write internally. If the deref'd");
    b.line(" * payload doesn't match `klass.isInstance`, the mismatch is logged");
    b.line(" * and {@code typed} is not called; an exception thrown by {@code typed}");
    b.line(" * is logged too. Either way the engine keeps the kind's default return.");
    b.line(" */");
    b.line(&format!(
        "public static <T> {}.ByValue register{}(Class<T> klass, {}WithData<T> typed) {{",
        cb_ffi, wrapper, wrapper
    ));
    b.indent();

    // Raw lambda param list mirrors the `<Wrapper>InvokerCallback` SAM:
    // `(long id, Pointer arg0, ..., [Pointer outPtr])` — the trailing
    // out-pointer exists only for non-void kinds (`has_return`).
    let mut raw_lambda_params = vec!["long id".to_string(), "Pointer arg0".to_string()];
    for (_kind, name) in &sam.args {
        raw_lambda_params.push(format!("Pointer {}", name));
    }
    if has_return(cb) {
        raw_lambda_params.push("Pointer outPtr".to_string());
    }

    b.line(&format!(
        "{} raw = ({}) -> {{",
        raw_sam,
        raw_lambda_params.join(", ")
    ));
    b.indent();
    // Failures inside the bridge (a model of the wrong class, an exception
    // thrown by `typed`) are reported through the first argument whose
    // wrapper has a `log(level, String)` method (`CallbackInfo`), else on
    // stderr. They never propagate to JNA; the engine pre-filled the out
    // slot with the kind's default, so a failed call just leaves it alone.
    let logger = failure_logger(cb, &sam, ir);
    let report = |b: &mut super::super::generator::CodeBuilder, msg: &str| match &logger {
        Some((arg, level)) => b.line(&format!("{}.log({}, {});", arg, level, msg)),
        None => b.line(&format!("System.err.println({});", msg)),
    };
    b.line("Object __data = refanyGet(arg0);");
    // Wrapper-class args are built over ENGINE-OWNED memory (`const
    // T*` for the duration of the callback): non-owning wrappers, never
    // `_delete`d, invalidated in `finally` once the callback returns.
    // Built before the `try` so the mismatch report can already log.
    let mut borrowed: Vec<String> = Vec::new();
    for (kind, name) in &sam.args {
        if let SamArg::Wrapper(ty) = kind {
            b.line(&format!("{} __{} = {}.__borrow({});", ty, name, ty, name));
            borrowed.push(format!("__{}", name));
        }
    }
    b.line("try {");
    b.indent();
    // `klass.isInstance(null)` is false — a null payload (refany freed /
    // unset) is passed through as `null` rather than rejected.
    b.line("if (__data != null && !klass.isInstance(__data)) {");
    b.indent();
    report(
        b,
        &format!(
            "\"azul: {} expected a model of class \" + klass.getName() + \", got \" + __data.getClass().getName()",
            wrapper
        ),
    );
    b.line("return;");
    b.dedent();
    b.line("}");
    b.line("@SuppressWarnings(\"unchecked\")");
    b.line("T __typed = (T) __data;");
    let mut call_args = vec!["__typed".to_string()];
    for (kind, name) in &sam.args {
        match kind {
            SamArg::Wrapper(_) => {
                call_args.push(format!("__{}", name));
            }
            SamArg::Pod(ty) => {
                b.line(&format!("{} __{} = new {}({});", ty, name, ty, name));
                call_args.push(format!("__{}", name));
            }
            SamArg::Enum(ty) => {
                // Unit enums travel as a C `int`; the invoker hands us a
                // pointer to it.
                b.line(&format!(
                    "{} __{} = {}.fromInt({}.getInt(0));",
                    ty, name, ty, name
                ));
                call_args.push(format!("__{}", name));
            }
            SamArg::Prim { jvm, getter } => {
                b.line(&format!("{} __{} = {}.{}(0);", jvm, name, name, getter));
                call_args.push(format!("__{}", name));
            }
            SamArg::RawPointer => {
                call_args.push(name.clone());
            }
        }
    }
    match &sam.ret {
        SamRet::Void => {
            b.line(&format!("typed.invoke({});", call_args.join(", ")));
        }
        SamRet::Enum(_) => {
            b.line(&format!(
                "{} __result = typed.invoke({});",
                return_decl,
                call_args.join(", ")
            ));
            // Unit-only enums emit a `.value` field of type `int`
            // (`Update.RefreshDom.value == 1`). A `null` return is
            // treated as ordinal 0 — same as the legacy behaviour where
            // the user forgot to write outPtr.
            b.line("outPtr.setInt(0, __result == null ? 0 : __result.value);");
        }
        SamRet::Wrapper { ffi, .. } => {
            b.line(&format!(
                "{} __result = typed.invoke({});",
                return_decl,
                call_args.join(", ")
            ));
            b.line("if (__result == null) return;");
            b.line(&format!("{}.ByValue __raw =", ffi));
            b.indent();
            b.line(&format!(
                "({}.ByValue) Structure.newInstance({}.ByValue.class, __result.rawPointer());",
                ffi, ffi
            ));
            b.dedent();
            b.line("__raw.read();");
            b.line("int sz = __raw.size();");
            b.line("outPtr.write(0, __raw.getPointer().getByteArray(0, sz), 0, sz);");
            // libazul takes ownership of the struct bytes via outPtr;
            // the user's wrapper would otherwise double-drop on GC.
            b.line("__result.__consume();");
        }
        SamRet::Pod(_) => {
            b.line(&format!(
                "{} __result = typed.invoke({});",
                return_decl,
                call_args.join(", ")
            ));
            b.line("if (__result == null) return;");
            // A plain value struct built by the user in Java: push its
            // fields into its backing memory, then copy the bytes out.
            b.line("__result.write();");
            b.line("int sz = __result.size();");
            b.line("outPtr.write(0, __result.getPointer().getByteArray(0, sz), 0, sz);");
        }
    }
    b.dedent();
    // Boundary: see `emit_boundary_catch`. The log sink is the wrapper this
    // bridge already borrowed for the logging-capable argument, so the catch
    // has to run before the `finally` that invalidates those borrows.
    let sink = match &logger {
        Some((var, level)) => FailureSink::Wrapper {
            var: var.clone(),
            level: level.clone(),
        },
        None => FailureSink::Stderr,
    };
    let finally_lines: Vec<String> = borrowed
        .iter()
        .map(|name| format!("{}.close();", name))
        .collect();
    emit_boundary_catch(b, wrapper, &sink, &finally_lines);
    b.dedent();
    b.line("};");
    b.line(&format!("return register{}((Object) raw);", wrapper));
    b.dedent();
    b.line("}");
    b.blank();
}

// ============================================================================
// The callback boundary: nothing the application throws may unwind into Rust
// ============================================================================

/// The argument of one callback kind that can report a failure to the
/// application: the first whose IR type has a wrapper class AND an instance
/// method of the shape `(self, <unit enum> level, owned <string> message) ->
/// ()`.
///
/// That shape is what a bridge needs to report a failure from inside a firing
/// callback, and it is matched by SHAPE — a binding that keyed on the method
/// being spelled `log` would lose the capability the day api.json renames it,
/// and would pick up an unrelated method that happened to share the name.
/// Reaching the application's own log sink is the point: a logged failure
/// shows up wherever the app already sends its logs, so a broken callback is
/// visible instead of fatal.
struct LogSink {
    /// Index into `cb.args`.
    index: usize,
    /// The wrapper class that carries the logging method.
    class: String,
    /// The level expression for the enum's `Error` variant (first variant if
    /// there is none), spelled the way the wrapper method takes it
    /// (`AppLogLevel.Error.value` for an `int`).
    level: String,
}

fn log_sink(cb: &CallbackTypedefDef, ir: &CodegenIR) -> Option<LogSink> {
    for (i, a) in cb.args.iter().enumerate() {
        let ty = a.type_name.trim();
        if !has_wrapper_class(ty, ir) {
            continue;
        }
        let Some(f) = ir.functions.iter().find(|f| {
            f.class_name == ty
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                // Reports, never answers: a logging call returns nothing.
                && f.return_type.is_none()
                // (self, level, message)
                && f.args.len() == 3
                && ir
                    .find_enum(f.args[1].type_name.trim())
                    .is_some_and(|e| !e.is_union && !e.variants.is_empty())
                // The engine's UTF-8 string type, by IR category.
                && matches!(f.args[2].ref_kind, ArgRefKind::Owned)
                && ir
                    .find_struct(f.args[2].type_name.trim())
                    .is_some_and(|st| matches!(st.category, TypeCategory::String))
        }) else {
            continue;
        };
        let level_ty = f.args[1].type_name.trim();
        let e = ir.find_enum(level_ty)?;
        let variant = e
            .variants
            .iter()
            .find(|v| v.name == "Error")
            .or_else(|| e.variants.first())?;
        let constant = format!(
            "{}.{}",
            user_enum_type_name(level_ty),
            sanitize_identifier(&variant.name)
        );
        let level = if map_jvm_type_byvalue(level_ty, ir) == "int" {
            format!("{}.value", constant)
        } else {
            constant
        };
        return Some(LogSink {
            index: i,
            class: wrapper_class_name(ty),
            level,
        });
    }
    None
}

/// Where a trampoline sends the text of a failure it caught at the boundary.
enum FailureSink {
    /// Through the log sink, on a wrapper the trampoline already built over
    /// the engine's pointer (the typed-`T` bridge borrows its args up front).
    Wrapper { var: String, level: String },
    /// Through the log sink, over a raw `Pointer` parameter the trampoline has
    /// to wrap first — non-owning, and closed again immediately.
    Pointer {
        class: String,
        expr: String,
        level: String,
    },
    /// The kind has no argument that can log (a layout callback, the
    /// host-handle releaser): stderr is all there is.
    Stderr,
}

/// The Java parameter name the raw `<Kind>InvokerCallback` lambda gives
/// `cb.args[i]`: api.json's own name, or `arg<i>` when it has none. One
/// definition, so the lambda's parameter list and everything that refers back
/// to a parameter cannot drift.
fn invoker_arg_name(i: usize, a: &FunctionArg) -> String {
    if a.name.is_empty() {
        format!("arg{}", i)
    } else {
        a.name.clone()
    }
}

/// The sink for a trampoline that has the raw invoker's `Pointer` parameters
/// in scope.
fn pointer_sink(cb: &CallbackTypedefDef, ir: &CodegenIR) -> FailureSink {
    match log_sink(cb, ir) {
        Some(s) => FailureSink::Pointer {
            expr: invoker_arg_name(s.index, &cb.args[s.index]),
            class: s.class,
            level: s.level,
        },
        None => FailureSink::Stderr,
    }
}

/// Close a trampoline's `try` with the boundary catch.
///
/// `Throwable`, not `Exception`: an `Error` crossing the FFI boundary is
/// exactly as undefined as a checked exception, and a `StackOverflowError`
/// out of a deep layout callback is the likeliest one of all. The failure is
/// reported and swallowed; the engine keeps the default it pre-filled for this
/// kind, so the application carries on.
///
/// The caller is left at the `try` body's indentation; on return the whole
/// `catch` (and `finally_lines`, when there are any) is closed and the builder
/// is back at the statement level, ready for the closing `};` of the lambda.
fn emit_boundary_catch(
    b: &mut super::super::generator::CodeBuilder,
    kind: &str,
    sink: &FailureSink,
    finally_lines: &[String],
) {
    b.line("} catch (Throwable __e) {");
    b.indent();
    b.line(&format!(
        "java.lang.String __msg = \"azul: {} threw \" + __e.getClass().getName() + \": \" \
         + __e.getMessage();",
        kind
    ));
    match sink {
        FailureSink::Stderr => b.line("System.err.println(__msg);"),
        FailureSink::Wrapper { var, level } => emit_log_call(b, var, level, None),
        FailureSink::Pointer { class, expr, level } => {
            // Non-owning: the engine owns this pointer for the duration of the
            // call, so the wrapper must never `_delete` it.
            b.line(&format!("{} __log = {}.__borrow({});", class, class, expr));
            emit_log_call(b, "__log", level, Some("__log"));
        }
    }
    // The stack trace is the only thing that says WHERE the callback broke,
    // and the log sink takes a single line of text.
    b.line("__e.printStackTrace();");
    b.dedent();
    if finally_lines.is_empty() {
        b.line("}");
    } else {
        b.line("} finally {");
        b.indent();
        for l in finally_lines {
            b.line(l);
        }
        b.dedent();
        b.line("}");
    }
}

/// `var.log(<level>, __msg)`, itself guarded: this is the last line of
/// defence, so a log sink that fails (the engine is already tearing down, say)
/// must still not let anything escape into Rust.
fn emit_log_call(
    b: &mut super::super::generator::CodeBuilder,
    var: &str,
    level: &str,
    close: Option<&str>,
) {
    b.line("try {");
    b.indent();
    b.line(&format!("{}.log({}, __msg);", var, level));
    b.dedent();
    b.line("} catch (Throwable __logFailed) {");
    b.indent();
    b.line("System.err.println(__msg);");
    b.dedent();
    match close {
        Some(v) => {
            b.line("} finally {");
            b.indent();
            b.line(&format!("{}.close();", v));
            b.dedent();
            b.line("}");
        }
        None => b.line("}"),
    }
}

/// [`log_sink`] resolved against a typed-`T` bridge, which has already built a
/// borrowed wrapper (`__<name>`) for every wrapper-class argument.
fn failure_logger(
    cb: &CallbackTypedefDef,
    sam: &DataTypedSam,
    ir: &CodegenIR,
) -> Option<(String, String)> {
    let sink = log_sink(cb, ir)?;
    // The typed SAM drops the leading RefAny, so `cb.args[i]` is `sam.args[i - 1]`.
    let (kind, name) = sam.args.get(sink.index.checked_sub(1)?)?;
    matches!(kind, SamArg::Wrapper(_)).then(|| (format!("__{}", name), sink.level))
}
