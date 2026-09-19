//! Lua-side runtime helpers emitted into `azul.lua` (LuaJIT `ffi` and cffi-lua).
//!
//! Two layers of host machinery the wrappers depend on:
//!
//! 1. **Host-invoker registration** (the design Python uses, ported to a C-ABI plug-in point). At
//!    module load we register one FFI callback per callback kind (`AzCallbackInvoker`,
//!    `AzLayoutCallbackInvoker`, every widget callback, …) plus a single shared releaser. Each
//!    invoker has a *pointer-arg* signature, which both FFI implementations can build a callback
//!    for — by-value plumbing happens inside libazul's static thunks (see
//!    `azul_core::host_invoker`).
//!
//! 2. **`azul.refany_create` / `azul.refany_get` / `azul._refany_arg`** — keep the user's data
//!    alive for as long as a `RefAny` clone exists, mirroring Python's `PyDataWrapper` story.
//!    The RefAny destructor clears the entry via the same shared releaser used for callback
//!    handles.
//!
//! Plus the transient helpers every wrapper call site relies on: `azul._az_string` (plain Lua
//! string → unarmed `AzString`), `azul._consume` (detach a finalizer after Rust took the bytes)
//! and `azul.pin_callback` (callback kinds without a host invoker).
//!
//! Wrapper-emitted methods (e.g. `Button:with_on_click`, `WindowCreateOptions.create`, …) call
//! `azul._register_callback(<kind>, fn)` for each callback arg, which stashes `fn` in
//! `_lua_handles[id]` and returns the matching `Az<Kind>` wrapper struct produced by libazul's
//! `_createFromHostHandle`. The wrapper passes that struct straight to the C-ABI function.
//!
//! ## Why the prelude is data-driven from the IR
//!
//! Each callback typedef in api.json contributes one cdef line, one FFI
//! callback registration, one `_register_callback` branch. Hand-writing
//! that for ~25 widget callbacks would mean churn every time api.json
//! adds a new event hook. We walk `ir.callback_typedefs` and emit the
//! whole prelude programmatically; the only static piece is the
//! framework's RefAny / releaser plumbing, which is the same regardless
//! of which kinds are registered.
//!
//! ## Dual-runtime rules
//!
//! Everything emitted here must run under LuaJIT and cffi-lua alike: handle
//! ids are plain Lua numbers (no `ULL` literals), cdata checks go through the
//! `_is_cdata` / `_tonum` shims defined in the prologue (mod.rs), and every
//! per-role callback is built with `ffi.cast` on a pointer-arg signature.

use super::super::{
    ir::{CallbackTypedefDef, CodegenIR},
    managed_host_invoker::{c_typename, emit_cdef_block, host_invoker_kinds, wrapper_name},
    managed_lang_helpers::is_refany_type,
};

/// Emit the Lua prelude that registers all callback invokers + RefAny
/// helpers under the `azul` namespace.
///
/// Must be inserted *after* `local C = ...` / `local azul = {}` and *before*
/// the wrapper layer, because wrappers reference `azul._register_callback`,
/// `azul._az_string`, `azul._refany_arg` and `azul._consume`.
pub fn emit_managed_prelude(out: &mut String, ir: &CodegenIR) {
    out.push_str(PRELUDE_HEADER);

    // Per-kind cdef declarations (invoker typedef + setter) come from the
    // shared host-invoker helper so every C-syntax host (LuaJIT, PHP FFI,
    // koffi, CFFI) emits the exact same block. ONE deviation: the helper
    // declares the by-value constructors `Az<Kind> Az<Kind>_createFromHostHandle(uint64_t)`,
    // which return a union-bearing struct by value — cffi-lua cannot even
    // DECLARE such a function (no libffi union-by-value support), so this
    // binding uses the out-pointer variants core exports for exactly this
    // case (`Az<Kind>_createFromHostHandleByref(handle, out)`,
    // `AzRefAny_newHostHandleByref(id, out)`; core/src/host_invoker.rs).
    // The by-value lines are dropped from the shared block and the Byref
    // ones declared here — each step is a no-op once the shared helper
    // emits the Byref forms itself.
    let mut shared = String::new();
    emit_cdef_block(&mut shared, ir);
    out.push_str("ffi.cdef[[\n");
    for line in shared.lines() {
        if line.trim().ends_with("_createFromHostHandle(uint64_t);") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !shared.contains("_createFromHostHandleByref") {
        out.push_str("    /* Out-pointer constructors (core/src/host_invoker.rs): no aggregate\n");
        out.push_str("       crosses by value, so both FFI implementations can declare them. */\n");
        for cb in host_invoker_kinds(ir) {
            let wrapper = wrapper_name(cb);
            out.push_str(&format!(
                "    void Az{wrapper}_createFromHostHandleByref(uint64_t, Az{wrapper}*);\n"
            ));
        }
    }
    if !shared.contains("AzRefAny_newHostHandleByref") {
        out.push_str("    void AzRefAny_newHostHandleByref(uint64_t, AzRefAny*);\n");
    }
    out.push_str("]]\n\n");

    out.push_str(PRELUDE_HANDLES);

    // Per-kind FFI callback registration.
    out.push_str("-- ── Per-kind invoker registrations ─────────────────────────────────────\n");
    for cb in host_invoker_kinds(ir) {
        emit_invoker_registration(out, cb, ir);
    }
    out.push('\n');

    // _register_callback: kind -> ctype table (data-driven from the IR) +
    // one out-pointer constructor path for every kind.
    out.push_str("-- Wrapper-emitted methods call this to wrap a Lua function into a\n");
    out.push_str("-- callback wrapper struct the framework can store. The kind argument\n");
    out.push_str("-- is the *wrapper type name* (e.g. 'Callback', 'ButtonOnClickCallback');\n");
    out.push_str("-- the wrapper-method emitter passes the arg's IR-known wrapper. The\n");
    out.push_str("-- returned struct is UNARMED like every C return: the C call it feeds\n");
    out.push_str("-- consumes it (or a struct field takes it over), and the engine releases\n");
    out.push_str("-- the host handle through the shared releaser when the last clone drops.\n");
    out.push_str("local _cb_ctypes = {\n");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        out.push_str(&format!("    {wrapper} = ffi.typeof('Az{wrapper}'),\n"));
    }
    out.push_str("}\n");
    out.push_str("function azul._register_callback(kind, fn)\n");
    out.push_str("    if fn == nil then return nil end\n");
    out.push_str("    if type(fn) ~= 'function' then\n");
    out.push_str(
        "        error(\"azul._register_callback: expected function, got \"..type(fn), 2)\n",
    );
    out.push_str("    end\n");
    out.push_str("    local ct = _cb_ctypes[kind]\n");
    out.push_str("    if ct == nil then\n");
    out.push_str(
        "        error(\"azul._register_callback: unknown kind '\"..tostring(kind)..\"'\", 2)\n",
    );
    out.push_str("    end\n");
    out.push_str("    local out = ffi.new(ct)\n");
    out.push_str("    ffi.gc(out, nil)\n");
    out.push_str("    C['Az' .. kind .. '_createFromHostHandleByref'](_alloc_handle(fn), out)\n");
    out.push_str("    return out\n");
    out.push_str("end\n\n");

    out.push_str(PRELUDE_REFANY);
    out.push_str(PRELUDE_TRANSIENTS);
    out.push('\n');
}

fn emit_invoker_registration(out: &mut String, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    let wrapper = wrapper_name(cb);
    let ret = cb.return_type.as_deref().unwrap_or("void");
    let has_return = ret != "void";

    // Lua function param list. The IR's args list mirrors the C-ABI args
    // (RefAny first, then info, then any widget state); each is passed
    // by pointer. Use the api.json arg names as Lua-side names.
    let arg_name = |i: usize, name: &str| {
        if name.is_empty() {
            format!("_arg{i}")
        } else {
            name.to_string()
        }
    };
    let mut lua_params: Vec<String> = vec!["id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        lua_params.push(arg_name(i, &a.name));
    }
    if has_return {
        lua_params.push("out_ptr".to_string());
    }

    // The user-fn call passes everything except `id` and `out_ptr`. id is
    // resolved to the user fn via _lua_handles; the user's fn signature
    // matches the typedef. RefAny args (IR category, not a name match)
    // are unwrapped to the Lua value they carry — or handed through raw
    // when they carry native data, see azul._refany_unwrap.
    let user_call_args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let n = arg_name(i, &a.name);
            if is_refany_type(&a.type_name, ir) {
                format!("azul._refany_unwrap({n})")
            } else {
                n
            }
        })
        .collect();

    out.push_str(&format!(
        "do\n    local invoker = ffi.cast('Az{wrapper}Invoker', function({params})\n",
        params = lua_params.join(", ")
    ));
    out.push_str("        local fn = _lua_handles[_tonum(id)]\n");
    out.push_str("        if fn == nil then return end\n");
    out.push_str(&format!(
        "        local ok, ret = pcall(fn, {})\n",
        user_call_args.join(", ")
    ));
    out.push_str("        if not ok then\n");
    out.push_str(&format!(
        "            io.stderr:write(\"[azul] {wrapper} error: \", tostring(ret), \"\\n\")\n"
    ));
    out.push_str("            return\n");
    out.push_str("        end\n");
    if has_return {
        // The framework reads the result from `out_ptr` (thunk default when
        // the callback returned nothing). An error while marshalling must
        // never escape an FFI callback (fatal under LuaJIT), hence the pcall.
        out.push_str("        if ret ~= nil then\n");
        out.push_str("            local ok_ret, err = pcall(function()\n");
        if ir.is_value_aggregate(ret) {
            // Aggregate return (Dom, ImageRef, …): store the struct into the
            // framework-owned slot through a typed pointer. A struct-to-struct
            // assignment (`p[0] = ret`) is the one form BOTH runtimes accept —
            // cffi-lua's `ffi.copy` rejects a struct cdata as its source
            // ("cannot convert 'struct AzDom' to 'void *'"), LuaJIT auto-takes
            // the address. Rust now owns the bytes and will drop them; detach
            // the finalizer the wrapper layer armed on `ret` so the Lua
            // cdata's collection cannot free them a second time.
            out.push_str(&format!(
                "                ffi.cast('{}*', out_ptr)[0] = ret\n",
                c_typename(ret)
            ));
            out.push_str("                azul._consume(ret)\n");
        } else {
            // Scalar / enum return (Update, …): plain store.
            out.push_str("                out_ptr[0] = ret\n");
        }
        out.push_str("            end)\n");
        out.push_str("            if not ok_ret then\n");
        out.push_str(&format!(
            "                io.stderr:write(\"[azul] {wrapper} return error: \", tostring(err), \"\\n\")\n"
        ));
        out.push_str("            end\n");
        out.push_str("        end\n");
    }
    out.push_str("    end)\n");
    out.push_str("    table.insert(_live_pins, invoker)\n");
    out.push_str(&format!("    C.AzApp_set{wrapper}Invoker(invoker)\n"));
    out.push_str("end\n");
}

const PRELUDE_HEADER: &str = r#"
-- ────────────────────────────────────────────────────────────────────────
-- Managed-FFI runtime helpers (host-invoker pattern)
--
-- libazul exports per callback kind:
--   * a static thunk (the `cb` field of the callback wrapper),
--   * `Az<Kind>_createFromHostHandleByref(u64, Az<Kind>*)` constructor
--     (out-pointer form; no aggregate crosses by value),
--   * `AzApp_set<Kind>Invoker(fn)` setter.
--
-- We register one FFI callback per kind at module load (these have
-- *pointer-arg* signatures, which LuaJIT and cffi-lua handle alike — the
-- by-value plumbing happens inside libazul's static thunk). User callbacks
-- then live in a Lua table keyed by integer id; the framework's RefAny
-- destructor calls back through `AzApp_setHostHandleReleaser` to clear
-- the entry.
-- ────────────────────────────────────────────────────────────────────────

"#;

const PRELUDE_HANDLES: &str = r#"-- One Lua table for every host handle libazul knows about — both user
-- data (`refany_create` / auto-wrapped RefAny args) and per-callback
-- closures live here keyed by id. Lua refs are held strong; the releaser
-- fires when the framework drops the last RefAny clone tied to a given id.
-- Ids are plain Lua numbers (exact up to 2^53; the C side takes uint64_t)
-- so the same source runs under LuaJIT and PUC Lua.
local _lua_handles = {}
local _next_handle_id = 0

-- Pin one FFI callback per role. These never die for the process
-- lifetime; we deliberately avoid putting their cdata in `_lua_handles`
-- so the host-invoker / releaser path can't accidentally clear itself.
local _live_pins = {}

local function _alloc_handle(value)
    _next_handle_id = _next_handle_id + 1
    _lua_handles[_next_handle_id] = value
    return _next_handle_id
end

local _releaser = ffi.cast('void (*)(uint64_t)', function(id)
    _lua_handles[_tonum(id)] = nil
end)
table.insert(_live_pins, _releaser)
C.AzApp_setHostHandleReleaser(_releaser)

-- Diagnostics: number of live host handles (user data + callbacks).
function azul._debug_handles()
    local n = 0
    for _ in pairs(_lua_handles) do n = n + 1 end
    return n
end

"#;

const PRELUDE_REFANY: &str = r#"-- ── RefAny user-data helpers ──────────────────────────────────────────
--
-- User data goes through the SAME host-handle RefAny path as registered
-- callbacks: one shared releaser, one id-keyed table. azul_core handles
-- the heap allocation + RTTI tagging in `AzRefAny_newHostHandleByref`; we
-- just stash the value at `_lua_handles[id]`.
local _refany_ct = ffi.typeof('AzRefAny')
local _refany_ptr_ct = ffi.typeof('AzRefAny*')
local _refany_cptr_ct = ffi.typeof('const AzRefAny*')

-- Fresh host-handle RefAny for `id`, UNARMED like every C return (the
-- out-pointer constructor: no aggregate crosses by value).
local function _new_host_refany(id)
    local out = ffi.new(_refany_ct)
    ffi.gc(out, nil)
    C.AzRefAny_newHostHandleByref(id, out)
    return out
end

--- Wrap an arbitrary Lua value in an AzRefAny. The CALLER owns the result:
--- C-call returns are never armed by the FFI, so arm the finalizer
--- explicitly — a dropped RefAny then decrements its refcount and, at
--- zero, fires the host-handle releaser that clears _lua_handles[id].
--- Passing it to a wrapper method clones it (see azul._refany_arg), so the
--- value stays usable across relayouts.
function azul.refany_create(value)
    return ffi.gc(_new_host_refany(_alloc_handle(value)), C.AzRefAny_delete)
end

--- Recover the Lua value previously wrapped by `azul.refany_create` (or
--- auto-wrapped by a wrapper method); nil for a RefAny that carries native,
--- non-host data.
function azul.refany_get(refany)
    local id = _tonum(C.AzRefAny_getHostHandle(refany))
    if id == 0 then return nil end
    return _lua_handles[id]
end

-- Callback-side unwrap used by the invokers: like refany_get, but a RefAny
-- that carries no host handle (native data created by the engine or by
-- another language) is handed to the Lua callback untouched instead of nil.
function azul._refany_unwrap(refany)
    local id = _tonum(C.AzRefAny_getHostHandle(refany))
    if id == 0 then return refany end
    return _lua_handles[id]
end

-- Wrapper-side coercion of every OWNED `RefAny` argument. Returns an
-- UNARMED transient that the C call consumes (owned by-value argument =
-- bytes taken over by Rust), so the wrapper emits no azul._consume for it:
--   * a plain Lua value  -> fresh host-handle RefAny (new id; the engine's
--                           releaser clears it when the last clone drops);
--   * an AzRefAny cdata  -> a CLONE (refcount + 1): the caller keeps its own
--                           handle and may pass it again on the next relayout.
function azul._refany_arg(v)
    if ffi.istype(_refany_ct, v) or ffi.istype(_refany_ptr_ct, v)
        or ffi.istype(_refany_cptr_ct, v) then
        return C.AzRefAny_clone(v)
    end
    return _new_host_refany(_alloc_handle(v))
end

"#;

const PRELUDE_TRANSIENTS: &str = r#"-- ── Transients, consumption, legacy callbacks ─────────────────────────

--- Mark a cdata as consumed by the C ABI: detach its __gc finalizer so the
--- FFI won't call `AzX_delete` on bytes that Rust has already taken over.
--- Called by codegen-emitted bridges after any C call that takes the
--- wrapper by value (consumed self, owned by-value arg, callback-return
--- byte splice, values assigned into struct fields by `:with{}`).
--- ffi.gc(cdata, nil) is the documented primitive for clearing
--- per-instance finalizers even when the ctype's metatype declared __gc;
--- it rejects scalar boxes (enums, 64-bit ints), which never carry one —
--- hence the pcall.
function azul._consume(c)
    if _is_cdata(c) then
        pcall(ffi.gc, c, nil)
    end
end

-- Auto-AzString conversion. Wrapper methods route every OWNED `String`
-- argument through this so user code can pass plain Lua strings; cdata
-- values pass through untouched. The AzString produced here is an UNARMED
-- transient (C-call returns never carry a finalizer, on the direct and on
-- the Byref path alike) that the C call it feeds consumes — arming it would
-- double-free the buffer Rust takes over. `azul.String.from_lua` is the
-- caller-owned (armed) variant.
function azul._az_string(v)
    if type(v) == 'string' then
        return C.AzString_copyFromBytes(ffi.cast('const uint8_t*', v), 0, #v)
    end
    return v
end

-- Callback kinds WITHOUT a host invoker (everything outside the
-- generator's HOST_INVOKER_KINDS list): build the C function pointer
-- straight from the Lua function with ffi.cast and pin it for the process
-- lifetime (nothing tells us when the last DOM holding it is gone). Most of
-- these typedefs pass RefAny / CallbackInfo / widget state BY VALUE, which
-- LuaJIT cannot marshal into a callback ("NYI: cannot create callback");
-- that error is re-raised with the C type so the failure is attributable.
function azul.pin_callback(ctype, fn)
    if fn == nil then return nil end
    if _is_cdata(fn) then return fn end -- already a C function pointer
    if type(fn) ~= 'function' then
        error("azul.pin_callback: expected function, got " .. type(fn), 2)
    end
    local ok, cb = pcall(ffi.cast, ctype, fn)
    if not ok then
        error("azul.pin_callback: cannot build a C callback of type " .. tostring(ctype)
            .. " from a Lua function (this callback kind has no host invoker): "
            .. tostring(cb), 2)
    end
    table.insert(_live_pins, cb)
    return cb
end
"#;
