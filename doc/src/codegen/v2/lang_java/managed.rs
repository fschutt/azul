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
