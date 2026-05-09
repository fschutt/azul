//! LuaJIT-side runtime helpers emitted into `azul.lua`.
//!
//! Two layers of host machinery the wrappers depend on:
//!
//! 1. **Host-invoker registration** (the design Python uses, ported to a
//!    C-ABI plug-in point). At module load we register one libffi closure
//!    per callback kind (`AzCallbackInvoker`, `AzLayoutCallbackInvoker`,
//!    every widget callback, …) plus a single shared releaser. Each
//!    invoker has a *pointer-arg* signature, which LuaJIT FFI can cast
//!    to without trouble — by-value plumbing happens inside libazul's
//!    static thunks (see `azul_core::host_invoker`).
//!
//! 2. **`azul.refany_create` / `azul.refany_get`** — keep the user's data
//!    table alive for as long as a `RefAny` clone exists, mirroring
//!    Python's `PyDataWrapper` story. RefAny destructor clears the entry
//!    via the same shared releaser used for callback handles.
//!
//! Wrapper-emitted methods (e.g. `Button:setOnClick`, `WindowCreateOptions
//! .create`, etc.) call `azul._register_callback(<kind>, fn)` for each
//! callback arg, which stashes `fn` in `_lua_cbs[id]` and returns the
//! matching `Az<Kind>` wrapper struct produced by libazul's
//! `_createFromHostHandle`. The user passes that wrapper directly to the
//! C-ABI function.
//!
//! ## Why the prelude is data-driven from the IR
//!
//! Each callback typedef in api.json contributes one cdef line, one libffi
//! closure registration, one `_register_callback` branch. Hand-writing
//! that for ~25 widget callbacks would mean churn every time api.json
//! adds a new event hook. We walk `ir.callback_typedefs` and emit the
//! whole prelude programmatically; the only static piece is the
//! framework's RefAny / releaser plumbing, which is the same regardless
//! of which kinds are registered.

use super::super::ir::{CallbackTypedefDef, CodegenIR};

/// Emit the LuaJIT prelude that registers all callback invokers + RefAny
/// helpers under the `azul` namespace.
///
/// Must be inserted *after* `local C = ffi.load('azul')` and *before* the
/// wrapper layer, because wrappers reference `azul._register_callback`.
pub fn emit_managed_prelude(out: &mut String, ir: &CodegenIR) {
    out.push_str(PRELUDE_HEADER);

    // Per-kind cdef declarations (invoker typedef + setter + constructor).
    out.push_str("ffi.cdef[[\n");
    out.push_str("    /* Host-handle releaser — called once per RefAny last-clone drop. */\n");
    out.push_str("    void AzApp_setHostHandleReleaser(void (*)(uint64_t));\n\n");
    out.push_str("    /* User-data RefAny on top of the host-handle path: one shared\n");
    out.push_str("       lifetime story for both callback registration and refany_create. */\n");
    out.push_str("    AzRefAny AzRefAny_newHostHandle(uint64_t);\n");
    out.push_str("    uint64_t AzRefAny_getHostHandle(const AzRefAny*);\n\n");
    out.push_str("    /* Per-kind invoker setters + pointer-arg signatures. The return\n");
    out.push_str("       value is an *out-parameter* so LuaJIT (which can't return\n");
    out.push_str("       aggregates > 8 bytes from callbacks) handles every kind uniformly. */\n");
    for cb in managed_callbacks(ir) {
        emit_cdef_for_kind(out, cb);
    }
    out.push_str("]]\n\n");

    out.push_str(PRELUDE_HANDLES);

    // Per-kind libffi closure registration.
    out.push_str("-- ── Per-kind invoker registrations ─────────────────────────────────────\n");
    for cb in managed_callbacks(ir) {
        emit_invoker_registration(out, cb);
    }
    out.push('\n');

    // _register_callback dispatch table.
    out.push_str("-- Wrapper-emitted methods call this to wrap a Lua function into a\n");
    out.push_str("-- callback wrapper struct the framework can store. The kind argument\n");
    out.push_str("-- is the *wrapper type name* (e.g. 'Callback', 'ButtonOnClickCallback');\n");
    out.push_str("-- the wrapper-method emitter passes the arg's IR-known wrapper.\n");
    out.push_str("function azul._register_callback(kind, fn)\n");
    out.push_str("    if fn == nil then return nil end\n");
    out.push_str("    if type(fn) ~= 'function' then\n");
    out.push_str("        error(\"azul._register_callback: expected function, got \"..type(fn), 2)\n");
    out.push_str("    end\n");
    out.push_str("    local id = _alloc_handle(fn)\n");
    let mut first = true;
    for cb in managed_callbacks(ir) {
        let wrapper = wrapper_name(cb);
        let lead = if first { "    if" } else { "    elseif" };
        out.push_str(&format!(
            "{lead} kind == '{wrapper}' then\n",
            lead = lead,
            wrapper = wrapper
        ));
        out.push_str(&format!(
            "        return C.Az{wrapper}_createFromHostHandle(id)\n",
            wrapper = wrapper
        ));
        first = false;
    }
    out.push_str("    else\n");
    out.push_str("        error(\"azul._register_callback: unknown kind '\"..tostring(kind)..\"'\", 2)\n");
    out.push_str("    end\n");
    out.push_str("end\n\n");

    out.push_str(PRELUDE_REFANY);
    out.push('\n');
}

/// Filter to the callback typedefs that have `impl_managed_callback!`
/// applied on the Rust side (and therefore export the matching
/// `Az<Wrapper>_createFromHostHandle` + `AzApp_set<Wrapper>Invoker`
/// symbols from the dll).
///
/// This list grows as new `impl_managed_callback!` invocations are added
/// in `azul-core` / `azul-layout`. Entries here whose typedef isn't in
/// `ir.callback_typedefs` are silently ignored (handles api.json
/// renames). Entries in the IR that aren't in this list get the legacy
/// `pin_callback` path on the wrapper-emitter side, which compiles fine
/// even if the resulting cast won't actually fire on libffi-style hosts.
const HOST_INVOKER_KINDS: &[&str] = &[
    "Callback",
    "LayoutCallback",
    "VirtualViewCallback",
];

fn managed_callbacks(ir: &CodegenIR) -> impl Iterator<Item = &CallbackTypedefDef> {
    ir.callback_typedefs.iter().filter(|cb| {
        let wrapper = cb.name.strip_suffix("Type").unwrap_or(cb.name.as_str());
        HOST_INVOKER_KINDS.contains(&wrapper)
    })
}

/// `CallbackTypedefDef.name` is e.g. `"CallbackType"` — strip the trailing
/// "Type" to get the wrapper struct name (e.g. `"Callback"`).
fn wrapper_name(cb: &CallbackTypedefDef) -> &str {
    cb.name.strip_suffix("Type").unwrap_or(cb.name.as_str())
}

/// Render the C-ABI argument list for one callback kind's invoker.
///
/// All by-value aggregates are passed by pointer (libffi-friendly); the
/// return value is also an out-pointer so we never hit LuaJIT's "callbacks
/// can't return aggregates > 8 bytes" limit. Primitive arg types skip the
/// `Az` prefix — e.g. `usize`, `u32` stay as themselves.
fn invoker_arg_list(cb: &CallbackTypedefDef) -> String {
    let mut parts = vec!["uint64_t".to_string()]; // host handle id
    for arg in &cb.args {
        parts.push(format!("const {}*", c_typename(&arg.type_name)));
    }
    let ret = cb.return_type.as_deref().unwrap_or("void");
    if ret != "void" {
        parts.push(format!("{}*", c_typename(ret)));
    }
    parts.join(", ")
}

/// Map an IR type name to its cdef C-ABI name. Primitives pass through
/// unchanged; non-primitives get the `Az` prefix.
fn c_typename(rust_type: &str) -> String {
    match rust_type {
        "u8" => "uint8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "u32" => "uint32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i8" => "int8_t".to_string(),
        "i16" => "int16_t".to_string(),
        "i32" => "int32_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "ssize_t".to_string(),
        "bool" => "bool".to_string(),
        "()" | "void" => "void".to_string(),
        _ => format!("Az{}", rust_type),
    }
}

fn emit_cdef_for_kind(out: &mut String, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let arg_list = invoker_arg_list(cb);
    out.push_str(&format!(
        "    typedef void (*Az{w}Invoker)({args});\n",
        w = wrapper,
        args = arg_list
    ));
    out.push_str(&format!(
        "    void AzApp_set{w}Invoker(Az{w}Invoker);\n",
        w = wrapper
    ));
    out.push_str(&format!(
        "    Az{w} Az{w}_createFromHostHandle(uint64_t);\n",
        w = wrapper
    ));
}

fn emit_invoker_registration(out: &mut String, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let ret = cb.return_type.as_deref().unwrap_or("void");
    let has_return = ret != "void";

    // Lua function param list. The IR's args list mirrors the C-ABI args
    // (RefAny first, then info, then any widget state); each is passed
    // by pointer. Use the api.json arg names as Lua-side names.
    let mut lua_params: Vec<String> = vec!["id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        lua_params.push(if a.name.is_empty() {
            format!("_arg{}", i)
        } else {
            a.name.clone()
        });
    }
    if has_return {
        lua_params.push("out_ptr".to_string());
    }

    // The user-fn call passes everything except `id` and `out_ptr`. id is
    // resolved to the user fn via _lua_handles; the user's fn signature
    // matches the typedef.
    let user_call_args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.name.is_empty() {
                format!("_arg{}", i)
            } else {
                a.name.clone()
            }
        })
        .collect();

    out.push_str(&format!(
        "do\n    local invoker = ffi.cast('Az{w}Invoker', function({params})\n",
        w = wrapper,
        params = lua_params.join(", ")
    ));
    out.push_str("        local fn = _lua_handles[tonumber(id)]\n");
    out.push_str("        if fn == nil then return end\n");
    out.push_str(&format!(
        "        local ok, ret = pcall(fn, {})\n",
        user_call_args.join(", ")
    ));
    out.push_str("        if not ok then\n");
    out.push_str(&format!(
        "            io.stderr:write(\"[azul] {} error: \", tostring(ret), \"\\n\")\n",
        wrapper
    ));
    out.push_str("            return\n");
    out.push_str("        end\n");
    if has_return {
        // For non-aggregate returns (Update, etc.), ret can be assigned
        // directly through the out-pointer indexed at [0]. For aggregate
        // returns (Dom, ImageRef, …), we copy bytes via ffi.copy.
        out.push_str("        if ret ~= nil then\n");
        out.push_str("            -- Try direct scalar assignment first; fall back to ffi.copy\n");
        out.push_str("            local ok_assign = pcall(function() out_ptr[0] = ret end)\n");
        out.push_str("            if not ok_assign then\n");
        out.push_str(&format!(
            "                ffi.copy(out_ptr, ret, ffi.sizeof('{}'))\n",
            c_typename(ret)
        ));
        out.push_str("            end\n");
        out.push_str("        end\n");
    }
    out.push_str("    end)\n");
    out.push_str("    table.insert(_live_pins, invoker)\n");
    out.push_str(&format!(
        "    C.AzApp_set{w}Invoker(invoker)\n",
        w = wrapper
    ));
    out.push_str("end\n");
}

const PRELUDE_HEADER: &str = r#"
-- ────────────────────────────────────────────────────────────────────────
-- Managed-FFI runtime helpers (host-invoker pattern)
--
-- libazul exports per callback kind:
--   * a static thunk (the `cb` field of the callback wrapper),
--   * `Az<Kind>_createFromHostHandle(u64) -> Az<Kind>` constructor,
--   * `AzApp_set<Kind>Invoker(fn)` setter.
--
-- We register one libffi closure per kind at module load (these have
-- *pointer-arg* signatures which LuaJIT FFI handles fine — the by-value
-- plumbing happens inside libazul's static thunk). User callbacks then
-- live in a Lua table keyed by integer id; the framework's RefAny
-- destructor calls back through `AzApp_setHostHandleReleaser` to clear
-- the entry.
-- ────────────────────────────────────────────────────────────────────────

"#;

const PRELUDE_HANDLES: &str = r#"-- One Lua table for every host handle libazul knows about — both user
-- data (`refany_create`) and per-callback closures live here keyed by id.
-- Lua refs are held strong; the releaser fires when the framework drops
-- the last RefAny clone tied to a given id.
local _lua_handles = {}
local _next_handle_id = 0ULL

-- Pin one libffi closure per role. These never die for the process
-- lifetime; we deliberately avoid putting their cdata in `_lua_handles`
-- so the host-invoker / releaser path can't accidentally clear itself.
local _live_pins = {}

local function _alloc_handle(value)
    _next_handle_id = _next_handle_id + 1ULL
    local id = _next_handle_id
    _lua_handles[tonumber(id)] = value
    return id
end

local _releaser = ffi.cast('void (*)(uint64_t)', function(id)
    _lua_handles[tonumber(id)] = nil
end)
table.insert(_live_pins, _releaser)
C.AzApp_setHostHandleReleaser(_releaser)

"#;

const PRELUDE_REFANY: &str = r#"-- ── RefAny user-data helpers ──────────────────────────────────────────
--
-- User data goes through the SAME host-handle RefAny path as registered
-- callbacks: one shared releaser, one id-keyed table. azul_core handles
-- the heap allocation + RTTI tagging in `AzRefAny_newHostHandle`; we
-- just stash the value at `_lua_handles[id]`.

-- Build an AzString from a plain Lua string (AzString_fromConstStr is a
-- preprocessor macro and not visible through FFI).
local function _az_make_string(s)
    return C.AzString_copyFromBytes(
        ffi.cast('const uint8_t*', s), 0, #s)
end
azul._make_string = _az_make_string

--- Wrap an arbitrary Lua value in an AzRefAny.
function azul.refany_create(value)
    local id = _alloc_handle(value)
    return C.AzRefAny_newHostHandle(id)
end

--- Recover the Lua value previously wrapped by `azul.refany_create`.
function azul.refany_get(refany)
    local id = C.AzRefAny_getHostHandle(refany)
    if id == 0ULL then return nil end
    return _lua_handles[tonumber(id)]
end
"#;
