//! Node.js managed-FFI runtime helpers (host-invoker pattern).
//!
//! Node's koffi binding is a libffi-based FFI, which means a JS function
//! cannot be cast to a C-callable pointer whose signature has aggregate
//! args by value — the same constraint Lua/PHP/Ruby share. The
//! host-invoker pattern works around this by routing user callbacks
//! through a single libffi closure per kind whose signature is
//! pointer-args + an out-pointer for the return.
//!
//! ## Bun / Deno
//!
//! Bun's `JSCallback` and Deno's `UnsafeCallback` *can* synthesize
//! struct-by-value trampolines, so they don't strictly need the
//! host-invoker pattern. We still go through it on every runtime so the
//! generated `azul.js` is uniform. The pointer-arg path costs one extra
//! `lib.Az<Kind>_createFromHostHandle(id)` C call per registration, which
//! is irrelevant for human-speed UI events.
//!
//! ## What this module emits
//!
//! Three pieces wired into `azul.js`:
//!
//! 1. **Function bindings** for the host-invoker C-ABI exports — one
//!    `lib.AzApp_setHostHandleReleaser` plus one
//!    `lib.AzApp_set<Kind>Invoker` and one
//!    `lib.Az<Kind>_createFromHostHandle` per supported callback kind.
//! 2. **Type registrations** for the per-kind invoker prototypes
//!    (`koffi.proto('AzCallbackInvoker', ...)`, mirrored on Bun/Deno via
//!    the uniform `azulFFI.proto(...)` adapter).
//! 3. **`azul.registerCallback(kind, fn)`** factory that allocates a host
//!    handle, stashes the user fn in a process-wide map, and returns the
//!    matching `Az<Kind>` cdata struct from
//!    `Az<Kind>_createFromHostHandle`. Plus `azul.refanyCreate(value)` /
//!    `azul.refanyGet(refany)` user-data helpers that share the same map.
//!
//! Future work (deferred):
//!
//! * Wrapper-emitter substitution in `wrappers.rs`. Until that lands,
//!   user code calls `azul.registerCallback('Callback', fn)` explicitly
//!   before passing the result to e.g. `button.setOnClick(...)`.
//! * Aggregate-return marshalling for kinds whose return type is a
//!   struct (LayoutCallback returns AzDom). Today the integer return
//!   path (Update enum) works directly via `koffi.encode`; struct
//!   returns need per-runtime out-pointer writeback support.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// Emit the host-invoker block. Insertion order: AFTER the existing
/// `types::generate_type_registrations` and `functions::generate_function_bindings`
/// (so it can reference both), BEFORE `wrappers::generate_wrappers` and
/// `emit_exports` (so user code via `azul.registerCallback` resolves).
pub fn emit_managed(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Managed-FFI runtime helpers (host-invoker pattern).");
    b.line("//");
    b.line("// libazul exports per supported callback kind:");
    b.line("//   * a static thunk (the `cb` field of the callback wrapper),");
    b.line("//   * Az<Kind>_createFromHostHandle(u64) -> Az<Kind> constructor,");
    b.line("//   * AzApp_set<Kind>Invoker(fn) setter.");
    b.line("//");
    b.line("// We register one libffi closure per kind via `azulFFI.callback()`.");
    b.line("// Pointer-arg signatures only, so even libffi-restricted runtimes");
    b.line("// (koffi) handle the cast without aggregate trampolines.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    emit_extra_function_bindings(b, ir);
    emit_dispatch_state(b);
    emit_init_block(b, ir);
    emit_register_callback(b, ir);
    emit_refany_helpers(b);
}

fn emit_extra_function_bindings(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// Host-invoker C-ABI bindings. The production C header generator");
    b.line("// filters these out (they're azul-core runtime helpers, not part of");
    b.line("// api.json), so we declare them here.");
    b.line("lib.AzApp_setHostHandleReleaser = azulFFI.func({");
    b.indent();
    b.line("name: 'AzApp_setHostHandleReleaser',");
    b.line("decl: 'void AzApp_setHostHandleReleaser(void (*)(uint64_t))',");
    b.line("parameters: ['void *'], returns: 'void',");
    b.dedent();
    b.line("});");
    b.line("lib.AzRefAny_newHostHandle = azulFFI.func({");
    b.indent();
    b.line("name: 'AzRefAny_newHostHandle',");
    b.line("decl: 'AzRefAny AzRefAny_newHostHandle(uint64_t)',");
    b.line("parameters: ['uint64_t'], returns: 'AzRefAny',");
    b.dedent();
    b.line("});");
    b.line("lib.AzRefAny_getHostHandle = azulFFI.func({");
    b.indent();
    b.line("name: 'AzRefAny_getHostHandle',");
    b.line("decl: 'uint64_t AzRefAny_getHostHandle(const AzRefAny *)',");
    b.line("parameters: ['void *'], returns: 'uint64_t',");
    b.dedent();
    b.line("});");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        b.line(&format!(
            "lib.AzApp_set{}Invoker = azulFFI.func({{",
            wrapper
        ));
        b.indent();
        b.line(&format!("name: 'AzApp_set{}Invoker',", wrapper));
        b.line(&format!(
            "decl: 'void AzApp_set{}Invoker(void *)',",
            wrapper
        ));
        b.line("parameters: ['void *'], returns: 'void',");
        b.dedent();
        b.line("});");
        b.line(&format!(
            "lib.Az{}_createFromHostHandle = azulFFI.func({{",
            wrapper
        ));
        b.indent();
        b.line(&format!("name: 'Az{}_createFromHostHandle',", wrapper));
        b.line(&format!(
            "decl: 'Az{w} Az{w}_createFromHostHandle(uint64_t)',",
            w = wrapper
        ));
        b.line(&format!(
            "parameters: ['uint64_t'], returns: 'Az{}',",
            wrapper
        ));
        b.dedent();
        b.line("});");
    }
    b.blank();
}

fn emit_dispatch_state(b: &mut CodeBuilder) {
    b.line("// One process-global table maps a host-handle id to the user's");
    b.line("// JS value (callback or refany payload). Released through the");
    b.line("// shared releaser when the framework drops the last RefAny clone.");
    b.line("const _handles = Object.create(null);");
    b.line("let _nextHandleId = 0n;");
    b.line("// Pinned cdata for libffi closures (must outlive any C-side");
    b.line("// reference). Process-lifetime today.");
    b.line("const _livePins = [];");
    b.line("let _hostInvokerInitialized = false;");
    b.blank();
    b.line("function _allocHandle(value) {");
    b.indent();
    b.line("_nextHandleId += 1n;");
    b.line("const id = _nextHandleId;");
    b.line("_handles[String(id)] = value;");
    b.line("return id;");
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_init_block(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("function _ensureHostInvokerInit() {");
    b.indent();
    b.line("if (_hostInvokerInitialized) return;");
    b.line("_hostInvokerInitialized = true;");
    b.blank();
    b.line("// Releaser: framework calls this with a host-handle id when the");
    b.line("// last RefAny clone tied to it is dropped.");
    b.line("const releaserProto = azulFFI.proto(");
    b.indent();
    b.line("'AzHostReleaser', 'void', ['uint64_t']");
    b.dedent();
    b.line(");");
    b.line("const releaser = azulFFI.callback(releaserProto, (id) => {");
    b.indent();
    b.line("delete _handles[String(id)];");
    b.dedent();
    b.line("});");
    b.line("_livePins.push(releaser);");
    b.line("lib.AzApp_setHostHandleReleaser(releaser);");
    b.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let cb_has_return = has_return(cb);
        // Per-kind proto: pointer args throughout. Out-param appended when
        // the callback returns non-void.
        let mut params = vec!["'uint64_t'".to_string()];
        for _ in &cb.args {
            params.push("'void *'".to_string());
        }
        if cb_has_return {
            params.push("'void *'".to_string());
        }
        b.line(&format!("// {} invoker", wrapper));
        b.line(&format!(
            "const {}Proto = azulFFI.proto(",
            lower_first(wrapper)
        ));
        b.indent();
        b.line(&format!(
            "'Az{}Invoker', 'void', [{}]",
            wrapper,
            params.join(", ")
        ));
        b.dedent();
        b.line(");");

        let mut closure_args = vec!["id".to_string()];
        for (i, a) in cb.args.iter().enumerate() {
            closure_args.push(if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                a.name.clone()
            });
        }
        if cb_has_return {
            closure_args.push("outPtr".to_string());
        }

        b.line(&format!(
            "const {}Invoker = azulFFI.callback({}Proto, ({}) => {{",
            lower_first(wrapper),
            lower_first(wrapper),
            closure_args.join(", ")
        ));
        b.indent();
        b.line("const fn = _handles[String(id)];");
        b.line("if (!fn) return;");
        b.line("try {");
        b.indent();
        let user_args: Vec<String> = closure_args
            .iter()
            .skip(1)
            .take(cb.args.len())
            .cloned()
            .collect();
        if cb_has_return {
            b.line(&format!("const ret = fn({});", user_args.join(", ")));
            b.line("if (ret === undefined || ret === null) return;");
            b.line("// Best-effort writeback. Numeric returns (Update enum) write");
            b.line("// directly via koffi.encode; struct returns need per-runtime");
            b.line("// support and are silently dropped today.");
            b.line("if (typeof ret === 'number' && azulFFI.runtime === 'node-koffi') {");
            b.indent();
            b.line("azulFFI.koffi.encode(outPtr, 'int32_t', ret);");
            b.dedent();
            b.line("}");
        } else {
            b.line(&format!("fn({});", user_args.join(", ")));
        }
        b.dedent();
        b.line("} catch (e) {");
        b.indent();
        b.line(&format!(
            "console.error('[azul] {} error:', e);",
            wrapper
        ));
        b.dedent();
        b.line("}");
        b.dedent();
        b.line("});");
        b.line(&format!(
            "_livePins.push({}Invoker);",
            lower_first(wrapper)
        ));
        b.line(&format!(
            "lib.AzApp_set{}Invoker({}Invoker);",
            wrapper,
            lower_first(wrapper)
        ));
        b.blank();
    }

    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_register_callback(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// Wrap a JS function in the matching Az<Kind> cdata struct so a");
    b.line("// native call site (e.g. button.setOnClick(...)) can store it.");
    b.line("function registerCallback(kind, fn) {");
    b.indent();
    b.line("if (typeof fn !== 'function') {");
    b.indent();
    b.line(
        "throw new TypeError(`azul.registerCallback: expected function, got ${typeof fn}`);",
    );
    b.dedent();
    b.line("}");
    b.line("_ensureHostInvokerInit();");
    b.line("const id = _allocHandle(fn);");
    b.line("switch (kind) {");
    b.indent();
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        b.line(&format!("case '{}':", wrapper));
        b.indent();
        b.line(&format!("return lib.Az{}_createFromHostHandle(id);", wrapper));
        b.dedent();
    }
    b.line("default:");
    b.indent();
    b.line("throw new Error(`azul.registerCallback: unknown kind '${kind}'`);");
    b.dedent();
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_refany_helpers(b: &mut CodeBuilder) {
    b.line("// User-data RefAny helpers — share the host-handle table with");
    b.line("// callbacks so the releaser frees both on last-clone drop.");
    b.line("function refanyCreate(value) {");
    b.indent();
    b.line("_ensureHostInvokerInit();");
    b.line("const id = _allocHandle(value);");
    b.line("return lib.AzRefAny_newHostHandle(id);");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("function refanyGet(refany) {");
    b.indent();
    b.line("// Accept either an AzRefAny by value or a pointer to one. koffi");
    b.line("// surfaces by-value structs as objects with a `__addr` accessor.");
    b.line("const ptr = (refany && typeof refany === 'object' && '__addr' in refany)");
    b.indent();
    b.line("? refany.__addr : refany;");
    b.dedent();
    b.line("const id = lib.AzRefAny_getHostHandle(ptr);");
    b.line("if (id === 0n || id === 0) return null;");
    b.line("return _handles[String(id)] ?? null;");
    b.dedent();
    b.line("}");
    b.blank();
}

/// `"Callback"` → `"callback"`, `"LayoutCallback"` → `"layoutCallback"`
/// — JavaScript convention for the local invoker variable.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
