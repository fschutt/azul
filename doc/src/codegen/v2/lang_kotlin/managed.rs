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
use super::LIBRARY_NAME;

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
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
