//! C# managed-FFI runtime helpers (host-invoker pattern).
//!
//! C# / .NET P/Invoke can natively marshal struct-by-value across the C-ABI
//! boundary, so it doesn't *need* the host-invoker pattern the way LuaJIT /
//! ruby-ffi / koffi do. We still apply it uniformly because the wrapper
//! generator + RefAny helpers are simpler when every managed-FFI host
//! shares one shape.
//!
//! ## What this emits
//!
//! 1. **`[DllImport]` declarations** for the host-invoker C-ABI exports
//!    inside `Azul.NativeMethods` (the same internal class the rest of
//!    the bindings live in).
//! 2. **Delegate types** for each per-kind invoker — pointer-arg
//!    signatures throughout, so `Marshal.GetFunctionPointerForDelegate`
//!    produces a stable thunk.
//! 3. **Static `Azul.HostInvoker` class** holding the id→Delegate
//!    dictionary, the GC-pinning list, the lazy `EnsureInitialized()`
//!    method, public `RegisterCallback(...)` factories per kind, and
//!    `RefanyCreate(object)` / `RefanyGet(IntPtr)` user-data helpers.
//!
//! User code looks like:
//!
//! ```csharp
//! var data = Azul.HostInvoker.RefanyCreate(model);
//!
//! AzUpdate OnClick(IntPtr dataPtr, IntPtr infoPtr) {
//!     var m = Azul.HostInvoker.RefanyGet(dataPtr) as MyModel;
//!     if (m == null) return AzUpdate.DoNothing;
//!     m.Counter++;
//!     return AzUpdate.RefreshDom;
//! }
//!
//! var cb = Azul.HostInvoker.RegisterCallback(OnClick);
//! Button.SetOnClick(button, dataClone, cb);
//! ```

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};
use super::DLL_NAME;

/// Emit a separate `NativeMethodsManaged` class with `[DllImport]`
/// declarations for the host-invoker exports. Lives next to `NativeMethods`
/// rather than inside it because we don't want to surgery into
/// `functions::generate_native_methods`.
///
/// Inserted from `mod.rs` AFTER `functions::generate_native_methods`.
pub fn emit_native_method_imports(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// NativeMethodsManaged: P/Invoke imports for the host-invoker C-ABI exports.");
    builder.line("// (Kept separate from NativeMethods so the regular function-binding emitter");
    builder.line("// stays linear; same DLL, same calling convention.)");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    builder.line("internal static class NativeMethodsManaged");
    builder.line("{");
    builder.indent();
    builder.line(&format!("public const string DllName = \"{}\";", DLL_NAME));
    builder.blank();

    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern void AzApp_setHostHandleReleaser(IntPtr fn);");
    builder.blank();
    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern AzRefAny AzRefAny_newHostHandle(ulong id);");
    builder.blank();
    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern ulong AzRefAny_getHostHandle(IntPtr refanyPtr);");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
        builder.line(&format!(
            "public static extern void AzApp_set{}Invoker(IntPtr fn);",
            wrapper
        ));
        builder.blank();
        builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
        builder.line(&format!(
            "public static extern Az{w} Az{w}_createFromHostHandle(ulong id);",
            w = wrapper
        ));
        builder.blank();
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Emit the public `Azul.HostInvoker` static class. Inserted from
/// `mod.rs` AFTER `wrappers::generate_wrappers` (so user-facing wrappers
/// are visible) but inside `namespace Azul { ... }`.
pub fn emit_host_invoker_class(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// Managed-FFI runtime: host-invoker public surface.");
    builder.line("//");
    builder.line("// Wraps user delegates so the framework's static thunk in libazul");
    builder.line("// can dispatch them by id. Storage is a process-global dictionary;");
    builder.line("// the framework's RefAny destructor calls back through the registered");
    builder.line("// releaser so we drop the entry on last-clone collection.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    builder.line("public static class HostInvoker");
    builder.line("{");
    builder.indent();

    // Per-kind delegate types
    builder.line("public delegate void HostHandleReleaserDelegate(ulong id);");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let cb_has_return = has_return(cb);
        let mut params = vec!["ulong id".to_string()];
        for (i, a) in cb.args.iter().enumerate() {
            let nm = if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                a.name.clone()
            };
            params.push(format!("IntPtr {}", nm));
        }
        if cb_has_return {
            params.push("IntPtr outPtr".to_string());
        }
        builder.line(&format!(
            "public delegate void {}InvokerDelegate({});",
            wrapper,
            params.join(", ")
        ));
    }
    builder.blank();

    // Storage
    builder.line("private static readonly System.Collections.Generic.Dictionary<ulong, object> _handles = new();");
    builder.line("private static ulong _nextHandleId = 0;");
    builder.line("private static readonly System.Collections.Generic.List<Delegate> _livePins = new();");
    builder.line("private static readonly object _initLock = new();");
    builder.line("private static bool _initialized = false;");
    builder.blank();

    // EnsureInitialized
    builder.line("private static void EnsureInitialized()");
    builder.line("{");
    builder.indent();
    builder.line("if (_initialized) return;");
    builder.line("lock (_initLock)");
    builder.line("{");
    builder.indent();
    builder.line("if (_initialized) return;");
    builder.line("_initialized = true;");
    builder.blank();

    builder.line("// Releaser");
    builder.line("HostHandleReleaserDelegate releaser = (ulong id) =>");
    builder.line("{");
    builder.indent();
    builder.line("lock (_handles) { _handles.Remove(id); }");
    builder.dedent();
    builder.line("};");
    builder.line("_livePins.Add(releaser);");
    builder.line("NativeMethodsManaged.AzApp_setHostHandleReleaser(System.Runtime.InteropServices.Marshal.GetFunctionPointerForDelegate(releaser));");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker_init(builder, cb);
    }

    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Per-kind RegisterCallback methods
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line("/// <summary>");
        builder.line(&format!(
            "/// Wrap a {} delegate in an Az{} cdata struct so a native call site can store it.",
            wrapper, wrapper
        ));
        builder.line("/// </summary>");
        builder.line(&format!(
            "public static Az{} Register{}(Delegate fn)",
            wrapper, wrapper
        ));
        builder.line("{");
        builder.indent();
        builder.line("EnsureInitialized();");
        builder.line("ulong id;");
        builder.line("lock (_handles)");
        builder.line("{");
        builder.indent();
        builder.line("_nextHandleId++;");
        builder.line("id = _nextHandleId;");
        builder.line("_handles[id] = fn;");
        builder.dedent();
        builder.line("}");
        builder.line(&format!(
            "return NativeMethodsManaged.Az{}_createFromHostHandle(id);",
            wrapper
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // RefanyCreate / RefanyGet
    builder.line("/// <summary>");
    builder.line("/// Wrap an arbitrary managed object in an AzRefAny held alive by the");
    builder.line("/// framework's refcount.");
    builder.line("/// </summary>");
    builder.line("public static AzRefAny RefanyCreate(object value)");
    builder.line("{");
    builder.indent();
    builder.line("EnsureInitialized();");
    builder.line("ulong id;");
    builder.line("lock (_handles)");
    builder.line("{");
    builder.indent();
    builder.line("_nextHandleId++;");
    builder.line("id = _nextHandleId;");
    builder.line("_handles[id] = value;");
    builder.dedent();
    builder.line("}");
    builder.line("return NativeMethodsManaged.AzRefAny_newHostHandle(id);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>");
    builder.line("/// Recover the managed object previously wrapped via RefanyCreate.");
    builder.line("/// Returns null if the RefAny is not a host-handle RefAny.");
    builder.line("/// </summary>");
    // Use plain `object` (not `object?`) so the C# source compiles
    // under a non-nullable context — PowerShell's `Add-Type` embed
    // doesn't enable `#nullable`, and the nullable-annotation form
    // raises CS8632 there.
    builder.line("public static object RefanyGet(IntPtr refanyPtr)");
    builder.line("{");
    builder.indent();
    builder.line("ulong id = NativeMethodsManaged.AzRefAny_getHostHandle(refanyPtr);");
    builder.line("if (id == 0) return null;");
    builder.line("lock (_handles)");
    builder.line("{");
    builder.indent();
    builder.line("return _handles.TryGetValue(id, out var v) ? v : null;");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_per_kind_invoker_init(
    builder: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
) {
    let wrapper = wrapper_name(cb);
    let cb_has_return = has_return(cb);

    let mut params = vec!["ulong id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        let nm = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
        params.push(format!("IntPtr {}", nm));
    }
    if cb_has_return {
        params.push("IntPtr outPtr".to_string());
    }
    let user_args: Vec<String> = cb
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

    builder.line(&format!("// {} invoker", wrapper));
    builder.line(&format!(
        "{w}InvokerDelegate {l}Invoker = ({p}) =>",
        w = wrapper,
        l = lower_first(wrapper),
        p = params.join(", ")
    ));
    builder.line("{");
    builder.indent();
    // Use plain `Delegate` (not `Delegate?`) — embed-friendly under
    // Add-Type's non-nullable C# context.
    builder.line("Delegate fn;");
    builder.line("lock (_handles)");
    builder.line("{");
    builder.indent();
    builder.line("fn = _handles.TryGetValue(id, out var v) ? (Delegate)v : null;");
    builder.dedent();
    builder.line("}");
    builder.line("if (fn == null) return;");
    builder.line("try");
    builder.line("{");
    builder.indent();
    if cb_has_return {
        builder.line(&format!(
            "var ret = fn.DynamicInvoke(new object[] {{ {} }});",
            user_args.join(", ")
        ));
        builder.line("if (ret is null) return;");
        builder.line("// Best-effort writeback: int / uint / enum write directly,");
        builder.line("// other ValueTypes (struct returns like AzDom) marshal via");
        builder.line("// StructureToPtr, ref types are silently dropped.");
        builder.line("if (ret is int i32)");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.WriteInt32(outPtr, i32);");
        builder.dedent();
        builder.line("}");
        builder.line("else if (ret is uint u32)");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.WriteInt32(outPtr, unchecked((int)u32));");
        builder.dedent();
        builder.line("}");
        builder.line("else if (ret is Enum e)");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.WriteInt32(outPtr, Convert.ToInt32(e));");
        builder.dedent();
        builder.line("}");
        builder.line("else if (ret is ValueType vt)");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(vt, outPtr, false);");
        builder.dedent();
        builder.line("}");
        // Wrapper-class return: extract its `Raw` property (now public)
        // and write the underlying struct bytes. This is what lets users
        // return `Dom.CreateBody().WithChild(...)` directly from a
        // LayoutCallback delegate.
        builder.line("else if (ret != null)");
        builder.line("{");
        builder.indent();
        builder.line("var __rawProp = ret.GetType().GetProperty(\"Raw\");");
        builder.line("if (__rawProp != null)");
        builder.line("{");
        builder.indent();
        builder.line("var __rawValue = __rawProp.GetValue(ret);");
        builder.line("if (__rawValue is ValueType __rvt)");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(__rvt, outPtr, false);");
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
    } else {
        builder.line(&format!(
            "fn.DynamicInvoke(new object[] {{ {} }});",
            user_args.join(", ")
        ));
    }
    builder.dedent();
    builder.line("}");
    builder.line("catch (Exception e)");
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "Console.Error.WriteLine($\"[azul] {} error: {{e.Message}}\");",
        wrapper
    ));
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("};");
    builder.line(&format!("_livePins.Add({}Invoker);", lower_first(wrapper)));
    builder.line(&format!(
        "NativeMethodsManaged.AzApp_set{}Invoker(System.Runtime.InteropServices.Marshal.GetFunctionPointerForDelegate({}Invoker));",
        wrapper,
        lower_first(wrapper)
    ));
    builder.blank();
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
