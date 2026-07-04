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
//! 1. **`AzulNativeManaged.java`** — `interface AzulNativeManaged extends
//!    Library` carrying the host-invoker C-ABI imports
//!    (`AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`,
//!    `AzRefAny_getHostHandle`, plus per-kind invoker setters and
//!    `Az<Kind>_createFromHostHandle` constructors).
//! 2. **`AzulHostInvoker.java`** — `public class AzulHostInvoker` with
//!    static state (id→Object dictionary, GC pin list, init flag),
//!    `ensureInitialized()`, public `register<Kind>Callback(callback)`
//!    factories per kind, and `refanyCreate(Object)` / `refanyGet(Pointer)`
//!    user-data helpers.
//!
//! Per-kind callback interfaces are defined inline as nested static
//! interfaces extending `Callback` so users can `implements
//! AzulHostInvoker.CallbackHandler` etc.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};
use super::{emit_file, LIBRARY_NAME};

/// Generate `AzulNativeManaged.java` + `AzulHostInvoker.java` and append
/// them to `out` using the same `FILE_MARKER` / `END_LINE` framing every
/// other Java file uses.
pub fn emit_files(out: &mut String, ir: &CodegenIR, config: &CodegenConfig) -> Result<()> {
    out.push_str(&emit_file(
        "AzulNativeManaged.java",
        |b| {
            b.line("import com.sun.jna.Library;");
            b.line("import com.sun.jna.Native;");
            b.line("import com.sun.jna.Pointer;");
            b.line("import com.sun.jna.Callback;");
            b.line("import com.sun.jna.Structure;");
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
                b.line(&format!("interface {}InvokerCallback extends Callback {{", wrapper));
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
            b.line("import com.sun.jna.Pointer;");
            b.line("import java.util.HashMap;");
            b.line("import java.util.Map;");
            b.line("import java.util.ArrayList;");
            b.line("import java.util.List;");
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
            b.line("private static boolean initialized = false;");
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
            b.line("synchronized (handles) { handles.remove(id); }");
            b.dedent();
            b.line("};");
            b.line("livePins.add(releaser);");
            b.line("AzulNativeManaged.INSTANCE.AzApp_setHostHandleReleaser(releaser);");
            b.blank();

            for cb in host_invoker_kinds(ir) {
                emit_per_kind_init(b, cb);
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
                b.line(" * @param fn user callback (must implement the kind's `*InvokerCallback` interface).");
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
                emit_data_typed_invoker_sam(b, cb, ir);
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
) {
    let wrapper = wrapper_name(cb);
    let cb_has_return = has_return(cb);

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
    let _user_args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                a.name.clone()
            }
        })
        .collect();

    b.line(&format!("// {} invoker", wrapper));
    b.line(&format!(
        "AzulNativeManaged.{w}InvokerCallback {l}Invoker = ({p}) -> {{",
        w = wrapper,
        l = lower_first(wrapper),
        p = params.join(", ")
    ));
    b.indent();
    b.line("Object fn;");
    b.line("synchronized (handles) { fn = handles.get(id); }");
    b.line("if (fn == null) return;");
    b.line("// Dispatch is left to the user-side handler — JNA does not");
    b.line("// expose Method.invoke through Callback. The user passes a");
    b.line("// concrete <Wrapper>InvokerCallback to register*Callback.");
    b.line("if (fn instanceof AzulNativeManaged." );
    let _ = wrapper; // future: refine dispatch
    b.line(&format!("    {}InvokerCallback) {{", wrapper));
    b.indent();
    let mut handler_args = vec!["id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        handler_args.push(if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        });
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
    // Only emit when the return type is a struct with an emitted
    // wrapper class — i.e. there's a `<ReturnType>_delete` function
    // and the struct isn't filtered out. Primitive / enum returns
    // (e.g. Update for ButtonOnClickCallback) keep using the raw
    // outPtr-write path because the typed wrapper would just be a
    // boxed primitive without a meaningful splice savings.
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
    let ffi_ret = super::ffi_type_name(ret_ty);
    let cb_ffi = super::ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);

    // Typed interface signature: `(long id, Pointer arg0, ..., Pointer argN) -> <Wrapper>`.
    let mut typed_params = vec!["long id".to_string()];
    let mut typed_args = vec!["id".to_string()];
    let mut raw_lambda_params = vec!["long id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        let nm = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
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
    b.line(
        " * bridge handles the struct-byte splice into outPtr internally.",
    );
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
    b.line(&format!(
        "{} result = fn.invoke({});",
        wrapper_class,
        typed_args.join(", ")
    ));
    b.line("if (result == null) return;");
    b.line(&format!(
        "{}.ByValue raw_struct =",
        ffi_ret
    ));
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

/// Phase CC-1 (Java): emit `<Wrapper>WithData<T>` typed SAM + a
/// `register<Wrapper>(Class<T> klass, <Wrapper>WithData<T> typed)`
/// overload that handles refanyGet + cast + arg-wrap + outPtr-write
/// internally. Per-kind conformability check — skip emit when any of:
///
///   - First callback arg is not `RefAny` (no data slot to type)
///   - Any subsequent arg's type isn't a struct with an emitted
///     wrapper class (`(Pointer)` constructor needed for arg-wrap)
///   - Return type is neither void, an enum (writes `result.value`
///     to outPtr), nor a wrapper struct (bytes-splice into outPtr)
///
/// Per the user-locked decision: iterate ALL HOST_INVOKER_KINDS, skip
/// non-conformers individually; never abort the whole arc.
fn emit_data_typed_invoker_sam(
    b: &mut super::super::generator::CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &super::super::ir::CodegenIR,
) {
    use super::super::ir::FunctionKind;
    let wrapper = wrapper_name(cb);
    let cb_ffi = super::ffi_type_name(wrapper);
    let raw_sam = format!("AzulNativeManaged.{}InvokerCallback", wrapper);

    // Conformance probe #1: first arg must be `RefAny` (the data slot
    // we'll type via `<T>`).
    let first = cb.args.first();
    let first_is_refany = first
        .map(|a| a.type_name.trim() == "RefAny")
        .unwrap_or(false);
    if !first_is_refany {
        return;
    }

    // Subsequent args: prefer wrapper-class types (`new <Wrapper>(ptr)`),
    // fall back to raw `Pointer` per arg when no wrapper class exists.
    // This keeps Callback / ButtonOnClick / etc. conforming despite
    // `CallbackInfo` being POD-Copy-no-_delete (so no wrapper class) —
    // the typed Data<T> win on the data slot is still worth it even
    // when the info slot remains a Pointer. ArgKind tags the form so
    // the emitter knows whether to construct or pass through.
    enum ArgKind {
        Wrapper(String), // wrapper class name; emit `new <ty>(ptr)`
        RawPointer,      // emit the raw `Pointer` (no wrap)
    }
    let mut extra_args: Vec<(ArgKind, String)> = Vec::new();
    for (i, a) in cb.args.iter().enumerate().skip(1) {
        let t = a.type_name.trim();
        let kind = if managed_has_wrapper_class(t, ir) {
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

    // Conformance probe #3: return type must be void / enum / wrapper
    // struct (we know how to plumb each to outPtr).
    enum RetShape {
        Void,
        Enum,
        WrapperStruct,
    }
    let (return_decl, ret_shape) = match cb.return_type.as_deref().map(str::trim) {
        None => ("void".to_string(), RetShape::Void),
        Some("void") => ("void".to_string(), RetShape::Void),
        Some(rt) => {
            if managed_has_wrapper_class(rt, ir) {
                (rt.to_string(), RetShape::WrapperStruct)
            } else if ir.find_enum(rt).is_some() {
                // Surface the unprefixed enum name (e.g. `Update`) —
                // that's what JVM users actually call `.value` on
                // (unit enums are emitted unprefixed; see
                // `user_enum_type_name` in mod.rs).
                (super::user_enum_type_name(rt), RetShape::Enum)
            } else {
                return;
            }
        }
    };
    // Avoid unused-warning when probe #3 yields neither Enum nor
    // WrapperStruct (the `FunctionKind` import only matters once we
    // add per-arg consume logic; keep the import path explicit so a
    // future arg-consume extension lights up without re-import).
    let _ = std::marker::PhantomData::<FunctionKind>;

    // === Typed SAM interface ===
    b.line("/**");
    b.line(&format!(
        " * Typed Data<T> SAM for {}: first arg is the deref'd-and-cast",
        wrapper
    ));
    b.line(" * `T` payload of the RefAny; remaining args are wrapper-class");
    b.line(" * types instead of raw `Pointer`. The matching `register` overload");
    b.line(" * handles the refanyGet + isInstance check + arg-wrap + outPtr-write");
    b.line(" * plumbing. Use this when you already know the concrete data type.");
    b.line(" */");
    b.line("@FunctionalInterface");
    b.line(&format!(
        "public interface {}WithData<T> {{",
        wrapper
    ));
    b.indent();
    let mut iface_params = vec!["T data".to_string()];
    for (kind, name) in &extra_args {
        let ty = match kind {
            ArgKind::Wrapper(t) => t.clone(),
            ArgKind::RawPointer => "Pointer".to_string(),
        };
        iface_params.push(format!("{} {}", ty, name));
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
        " * Register a typed Data<T> `{}WithData<T>`. Wraps the typed SAM",
        wrapper
    ));
    b.line(" * into the raw invoker; performs the refanyGet, runtime-class");
    b.line(" * check, arg-wrap, and outPtr-write internally. If the deref'd");
    b.line(" * payload doesn't match `klass.isInstance`, the invocation is");
    b.line(" * silently skipped (no-op default for non-matching data).");
    b.line(" */");
    b.line(&format!(
        "public static <T> {}.ByValue register{}(Class<T> klass, {}WithData<T> typed) {{",
        cb_ffi, wrapper, wrapper
    ));
    b.indent();

    // Raw lambda param list mirrors the existing `<Wrapper>InvokerCallback`
    // SAM: `(long id, Pointer arg0, ..., Pointer outPtr)` for non-void
    // returns; outPtr is omitted in the void case (the underlying SAM
    // still has it but we ignore by-pattern).
    let mut raw_lambda_params = vec!["long id".to_string(), "Pointer arg0".to_string()];
    for (_kind, name) in &extra_args {
        // Raw invoker always takes Pointer per arg; the wrapper-class
        // construction happens inside the lambda body.
        raw_lambda_params.push(format!("Pointer {}", name));
    }
    // `<Wrapper>InvokerCallback` only carries the trailing
    // `Pointer outPtr` when the callback has a non-void return
    // (`lang_java/managed.rs` line 88 / 315 — the `has_return(cb)`
    // gate). Mirror it here so the typed Data<T> lambda signature
    // lines up with the SAM exactly. ThreadCallback (void) is the
    // canonical case that exposed this.
    if has_return(cb) {
        raw_lambda_params.push("Pointer outPtr".to_string());
    }

    b.line(&format!(
        "{} raw = ({}) -> {{",
        raw_sam,
        raw_lambda_params.join(", ")
    ));
    b.indent();
    b.line("Object __data = refanyGet(arg0);");
    // `klass.isInstance(null)` returns false — so the null-payload
    // case (refany freed / unset) silently skips dispatch. Mirrors
    // the existing untyped-handler patterns in HelloWorld.java.
    b.line("if (__data != null && !klass.isInstance(__data)) return;");
    b.line("@SuppressWarnings(\"unchecked\")");
    b.line("T __typed = (T) __data;");
    let mut call_args = vec!["__typed".to_string()];
    for (kind, name) in &extra_args {
        match kind {
            ArgKind::Wrapper(ty) => {
                b.line(&format!(
                    "{} __{} = new {}({});",
                    ty, name, ty, name
                ));
                call_args.push(format!("__{}", name));
            }
            ArgKind::RawPointer => {
                // No-wrap path: pass the raw Pointer straight through.
                // User wraps via Structure.newInstance themselves if
                // they need to deref the underlying C struct.
                call_args.push(name.clone());
            }
        }
    }
    match ret_shape {
        RetShape::Void => {
            b.line(&format!("typed.invoke({});", call_args.join(", ")));
        }
        RetShape::Enum => {
            b.line(&format!(
                "{} __result = typed.invoke({});",
                return_decl,
                call_args.join(", ")
            ));
            // Unit-only enums emit a `.value` field of type `int`
            // (`AzUpdate.RefreshDom.value == 1`). Defensive null-check:
            // a `null` return from the SAM is treated as "default
            // value 0" — matches the legacy behaviour where the user
            // forgot to write outPtr.
            b.line("outPtr.setInt(0, __result == null ? 0 : __result.value);");
        }
        RetShape::WrapperStruct => {
            b.line(&format!(
                "{} __result = typed.invoke({});",
                return_decl,
                call_args.join(", ")
            ));
            b.line("if (__result == null) return;");
            let ffi_ret = super::ffi_type_name(&return_decl);
            b.line(&format!(
                "{}.ByValue __raw =",
                ffi_ret
            ));
            b.indent();
            b.line(&format!(
                "({}.ByValue) Structure.newInstance({}.ByValue.class, __result.rawPointer());",
                ffi_ret, ffi_ret
            ));
            b.dedent();
            b.line("__raw.read();");
            b.line("int sz = __raw.size();");
            b.line("outPtr.write(0, __raw.getPointer().getByteArray(0, sz), 0, sz);");
            // libazul takes ownership of the struct bytes via outPtr;
            // the user's wrapper would otherwise double-drop on GC.
            b.line("__result.__consume();");
        }
    }
    b.dedent();
    b.line("};");
    b.line(&format!(
        "return register{}((Object) raw);",
        wrapper
    ));
    b.dedent();
    b.line("}");
    b.blank();
}

/// Mirror of `lang_java/wrappers.rs::has_wrapper_class` (kept local
/// to avoid making the helper `pub` solely for this caller).
fn managed_has_wrapper_class(
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
