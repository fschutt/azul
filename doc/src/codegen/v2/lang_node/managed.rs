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
//!    `lib.AzApp_setHostHandleReleaser` plus one `lib.AzApp_set<Kind>Invoker` and one
//!    `lib.Az<Kind>_createFromHostHandle` per supported callback kind.
//! 2. **Type registrations** for the per-kind invoker prototypes (`koffi.proto('AzCallbackInvoker',
//!    ...)`, mirrored on Bun/Deno via the uniform `azulFFI.proto(...)` adapter).
//! 3. **`registerCallback(kind, fn)`** factory that allocates a host handle, stashes the user fn in
//!    a process-wide map, and returns the matching `Az<Kind>` cdata struct from
//!    `Az<Kind>_createFromHostHandle`. Plus `refanyCreate(value)` / `refanyGet(refany)` user-data
//!    helpers that share the same map.
//!
//! The wrapper classes (`wrappers.rs`) apply these automatically: any argument typed as a
//! host-invoker callback wrapper accepts a plain JS function, any `RefAny` argument accepts any JS
//! value, and callback invocations hand the JS value back (not the RefAny). Struct-returning kinds
//! (LayoutCallback → AzDom) are written back through the out-pointer via the adapter's
//! `encodeInto`; integer returns (Update) via `writeInt32`.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{CallbackTypedefDef, CodegenIR, EnumVariantKind, FunctionKind},
        managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name},
    },
    is_refany_type,
};

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
    emit_refany_helpers(b, ir);
}

fn emit_extra_function_bindings(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// Host-invoker C-ABI bindings. The production C header generator");
    b.line("// filters these out (they're azul-core runtime helpers, not part of");
    b.line("// api.json), so we declare them here.");
    b.line("lib.AzApp_setHostHandleReleaser = azulFFI.func({");
    b.indent();
    b.line("name: 'AzApp_setHostHandleReleaser',");
    // koffi rejects raw `void (*)(uint64_t)` in its decl spec; pass the
    // function pointer as `void *` (opaque address).
    b.line("decl: 'void AzApp_setHostHandleReleaser(void *)',");
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
    b.line("_handles[('' + id)] = value;");
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
    b.line("delete _handles[('' + id)];");
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
        b.line("const fn = _handles[('' + id)];");
        b.line("if (!fn) return;");
        b.line("try {");
        b.indent();
        // Invoker args arrive as native pointers (`const Az<T>*`). A
        // `RefAny` arg is resolved back to the JS value it was created
        // from, so the user's callback sees `(data, info)` — never the
        // handle struct.
        let mut user_args = Vec::new();
        for (i, a) in cb.args.iter().enumerate() {
            let var_name = closure_args[i + 1].clone();
            if is_refany_type(&a.type_name, ir) {
                user_args.push(format!("_refanyFromPtr({})", var_name));
            } else {
                user_args.push(var_name);
            }
        }
        if cb_has_return {
            b.line(&format!("const ret = fn({});", user_args.join(", ")));
            b.line("if (ret === undefined || ret === null) return;");
            // Numeric returns (Update enum, etc.) → write as int32_t.
            // Expressible on ALL three runtimes via the adapter's
            // writeInt32 (koffi.encode / Bun toArrayBuffer / Deno
            // UnsafePointerView.getArrayBuffer), so enum-returning
            // callbacks (button on_click → Update) take effect on
            // Bun/Deno too instead of being silently dropped.
            //
            // Struct returns (AzDom from LayoutCallback,
            // VirtualViewReturn from VirtualViewCallback) → encode
            // through the registered koffi type so the bytes land in
            // the out-pointer's target memory; otherwise the framework
            // reads the pre-filled default value and the host's layout
            // is dropped. koffi-only: see the else-branch note below.
            b.line("if (typeof ret === 'number') {");
            b.indent();
            b.line("azulFFI.writeInt32(outPtr, ret);");
            b.dedent();
            // For struct returns, the IR knows the exact type name
            // (e.g. "Dom"). The koffi-registered type name is
            // prefix-mangled ("AzDom") — see `ffi_type_name`.
            let struct_branch_type: Option<String> = cb.return_type.as_deref().and_then(|rt| {
                let trimmed = rt.trim();
                let is_primitive = matches!(
                    trimmed,
                    "bool"
                        | "u8"
                        | "i8"
                        | "u16"
                        | "i16"
                        | "u32"
                        | "i32"
                        | "u64"
                        | "i64"
                        | "f32"
                        | "f64"
                        | "usize"
                        | "isize"
                        | "c_void"
                        | "()"
                        | "void"
                );
                if is_primitive {
                    None
                } else {
                    Some(super::ffi_type_name(trimmed))
                }
            });
            if let Some(koffi_type) = struct_branch_type {
                b.line("} else if (typeof ret === 'object') {");
                b.indent();
                // Unwrap wrapper-class instances back to their underlying
                // koffi struct value. Users return `Dom.create_body().with_child(...)`
                // which is a `Dom` wrapper instance; the koffi-side encode
                // wants the raw AzDom bytes from `_ptr`. The user-callback
                // also consumed the wrapper (returned ownership to libazul),
                // so we null the `_ptr` to keep the FinalizationRegistry
                // from double-freeing.
                b.line("const _raw = (ret && ret._ptr !== undefined) ? ret._ptr : ret;");
                // Through the adapter's `encodeInto`, not koffi directly:
                // every runtime implements it (koffi.encode on Node, the
                // by-value type layer's encoder over an UnsafePointerView /
                // toArrayBuffer on Deno and Bun), so a struct-returning
                // callback works on all three.
                b.line(&format!(
                    "azulFFI.encodeInto(outPtr, '{}', _raw);",
                    koffi_type
                ));
                b.line(
                    "if (ret && ret._ptr !== undefined && ret.constructor && \
                     ret.constructor._registry) {",
                );
                b.indent();
                b.line("ret.constructor._registry.unregister(ret);");
                b.line("ret._ptr = null;");
                b.dedent();
                b.line("}");
                b.dedent();
                b.dedent();
                b.line("}");
            } else {
                b.line("}");
            }
        } else {
            b.line(&format!("fn({});", user_args.join(", ")));
        }
        b.dedent();
        b.line("} catch (e) {");
        b.indent();
        b.line(&format!("console.error('[azul] {} error:', e);", wrapper));
        if cb_has_return {
            emit_catch_default_write(b, ir, cb);
        }
        b.dedent();
        b.line("}");
        b.dedent();
        b.line("});");
        b.line(&format!("_livePins.push({}Invoker);", lower_first(wrapper)));
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

/// Emit the catch-arm `outPtr` write for a throwing user callback.
///
/// libazul's static thunk (`impl_managed_callback!` in
/// `core/src/host_invoker.rs`) pre-fills `*outPtr` with the kind's
/// default (`Update::DoNothing`, `Dom::create_body()`, ...) before
/// dispatching to the host, so an untouched out-struct is already
/// well-defined. But the success path's `koffi.encode` of a nested
/// struct can throw halfway through and leave `*outPtr` torn;
/// re-writing a complete safe default here guarantees native code
/// never reads a half-written return value.
///
/// Classification of the return type:
/// - fieldless enum (`Update`, ...) → `int32_t 0` (the first variant, i.e. `Update.DoNothing`);
/// - struct with an IR `Default` factory (`Dom` → `AzDom_default`, `VirtualViewReturn` →
///   `AzVirtualViewReturn_default`) → encode a freshly-constructed default struct;
/// - anything else (e.g. `OnTextInputReturn`, which has no `_default` C export) → leave the native
///   pre-filled default in place (documented no-op).
fn emit_catch_default_write(b: &mut CodeBuilder, ir: &CodegenIR, cb: &CallbackTypedefDef) {
    let rt = cb.return_type.as_deref().map(str::trim).unwrap_or("");
    let unit_enum = ir
        .find_enum(rt)
        .map(|e| {
            !e.is_union
                && e.variants
                    .iter()
                    .all(|v| matches!(v.kind, EnumVariantKind::Unit))
        })
        .unwrap_or(false);
    let default_factory = ir
        .functions_for_class(rt)
        .find(|f| matches!(f.kind, FunctionKind::Default))
        .map(|f| f.c_name.clone());
    b.line("// The native thunk pre-filled *outPtr with this kind's default,");
    b.line("// but a koffi.encode that threw halfway above may have left it");
    b.line("// torn — re-write a complete safe default. (On Bun/Deno only the");
    b.line("// 4-byte writeInt32 path writes, which cannot tear, so re-writing");
    b.line("// the int default there is cheap belt-and-braces; struct encodes");
    b.line("// never happen there and the native pre-fill stands.)");
    if unit_enum {
        // Runtime-agnostic: writeInt32 exists on koffi/Bun/Deno alike, so
        // the enum default (0 == first, DoNothing-style variant) is safely
        // re-written on every runtime and cannot tear (single 4-byte store).
        b.line(&format!(
            "try {{ azulFFI.writeInt32(outPtr, 0); }} catch (_e2) {{ /* pre-fill stands */ }} // \
             0 == {}.DoNothing-style first variant",
            rt
        ));
    } else if let Some(c_name) = default_factory {
        // Struct default via koffi.encode — koffi-only. On Bun/Deno struct
        // encodes never run in the success path (they collapse to pointers),
        // so nothing can tear there and the native pre-fill already stands.
        b.line("if (azulFFI.runtime === 'node-koffi') {");
        b.indent();
        b.line(&format!(
            "try {{ azulFFI.koffi.encode(outPtr, '{}', lib.{}()); }} catch (_e2) {{ /* pre-fill \
             stands */ }}",
            super::ffi_type_name(rt),
            c_name
        ));
        b.dedent();
        b.line("}");
    } else {
        b.line(&format!(
            "// No Az{}_default C export — rely on the native pre-filled default.",
            rt
        ));
    }
}

fn emit_register_callback(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line("// Wrap a JS function in the matching Az<Kind> cdata struct so a");
    b.line("// native call site (e.g. button.setOnClick(...)) can store it.");
    b.line("function registerCallback(kind, fn) {");
    b.indent();
    b.line("// Pass through anything that isn't a plain function — in particular");
    b.line("// already-registered Az<Kind> callback structs. The smart on_* setters");
    b.line("// register the user's fn and then delegate to with_on_* / set_on_*,");
    b.line("// which route through registerCallback again; that second call must");
    b.line("// be a no-op, not a TypeError (double-registration regression,");
    b.line("// BINDINGS_REVIEW_2026_07_04).");
    b.line("if (typeof fn !== 'function') return fn;");
    b.line("_ensureHostInvokerInit();");
    b.line("const id = _allocHandle(fn);");
    b.line("switch (kind) {");
    b.indent();
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        b.line(&format!("case '{}':", wrapper));
        b.indent();
        b.line(&format!(
            "return lib.Az{}_createFromHostHandle(id);",
            wrapper
        ));
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

/// `refanyCreate` / `refanyGet` / `_refanyFromPtr`.
///
/// A RefAny *value* is recognised structurally: a koffi-decoded object
/// carrying every field of the IR's `RefAny` struct (`sharing_info`,
/// `instance_id`; names are read from the IR). Anything else is user
/// data. That guard matters: koffi zero-fills an arbitrary JS object
/// into a `const AzRefAny *` parameter, and `AzRefAny_getHostHandle` on
/// a zeroed struct aborts the process inside `RefCount::downcast`.
///
/// One handle per `refanyCreate` call (no identity cache): the engine
/// releases it through `AzApp_setHostHandleReleaser` when it drops the
/// last clone, so per-frame `withOnClick(data, ..)` handles stay
/// bounded (two per DOM generation in the counter example). A
/// `WeakMap` cache would have to hold a RefAny clone per JS object and
/// could never release it while the handle table keeps the object
/// alive — a permanent leak — so it is deliberately not done.
fn emit_refany_helpers(b: &mut CodeBuilder, ir: &CodegenIR) {
    let refany_fields: Vec<String> = ir
        .structs
        .iter()
        .find(|s| is_refany_type(&s.name, ir))
        .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();
    let shape_check = if refany_fields.is_empty() {
        "false".to_string()
    } else {
        refany_fields
            .iter()
            .map(|f| format!("('{}' in v)", f))
            .collect::<Vec<_>>()
            .join(" && ")
    };

    b.line("// User-data RefAny helpers — share the host-handle table with");
    b.line("// callbacks so the releaser frees both on last-clone drop.");
    b.line("// A RefAny value is recognised by its C field names; every other JS");
    b.line("// value is user data (never passed to the C side as a RefAny).");
    b.line("function _isRefAnyValue(v) {");
    b.indent();
    b.line(&format!(
        "return v !== null && typeof v === 'object' && {};",
        shape_check
    ));
    b.dedent();
    b.line("}");
    b.blank();
    b.line("// Wrap any JS value in a fresh RefAny handle. Pass-through for a");
    b.line("// value that already is a RefAny (or a wrapper holding one).");
    b.line("function refanyCreate(value) {");
    b.indent();
    b.line("if (_isRefAnyValue(value)) return value;");
    b.line("if (value && typeof value === 'object' && _isRefAnyValue(value._ptr)) return value._ptr;");
    b.line("_ensureHostInvokerInit();");
    b.line("const id = _allocHandle(value);");
    b.line("return lib.AzRefAny_newHostHandle(id);");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("// Resolve a native `const AzRefAny *` (callback invoker argument) or a");
    b.line("// RefAny value back to the JS value it was created from.");
    b.line("function _refanyFromPtr(ptr) {");
    b.indent();
    b.line("if (ptr == null) return null;");
    b.line("const id = lib.AzRefAny_getHostHandle(ptr);");
    b.line("if (id === 0n || id === 0) return null;");
    b.line("return _handles[('' + id)] ?? null;");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("// Public form: a RefAny value yields its JS payload; any other value is");
    b.line("// returned unchanged (callbacks already receive the unwrapped value, so");
    b.line("// `refanyGet(data)` inside a callback is a harmless no-op).");
    b.line("function refanyGet(refany) {");
    b.indent();
    b.line("if (_isRefAnyValue(refany)) return _refanyFromPtr(refany);");
    b.line("if (refany && typeof refany === 'object' && _isRefAnyValue(refany._ptr)) {");
    b.indent();
    b.line("return _refanyFromPtr(refany._ptr);");
    b.dedent();
    b.line("}");
    b.line("return refany;");
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
