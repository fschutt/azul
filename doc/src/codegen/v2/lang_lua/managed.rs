//! LuaJIT-side runtime helpers emitted into `azul.lua`.
//!
//! Two layers of host machinery the wrappers depend on:
//!
//! 1. **Host-invoker registration** (the design Python uses, ported to a
//!    C-ABI plug-in point). At module load we register one libffi closure
//!    per callback kind (`AzCallbackInvoker`, `AzLayoutCallbackInvoker`)
//!    plus a single shared releaser. Each invoker has a *pointer-arg*
//!    signature, which LuaJIT FFI can cast to without trouble. The
//!    by-value plumbing happens inside libazul's static thunks (see
//!    `azul_core::host_invoker`).
//!
//! 2. **`azul.refany_create` / `azul.refany_get`** — keep the user's data
//!    table alive for as long as a `RefAny` clone exists, mirroring
//!    Python's `PyDataWrapper` story. RefAny destructor clears the entry
//!    via the same shared releaser used for callback handles.
//!
//! Wrapper-emitted methods (e.g. `Button:setOnClick`, `WindowCreateOptions
//! .create`) call `azul._register_callback(<kind>, fn)` for each callback
//! arg, which stashes `fn` in `_lua_cbs[id]` and returns the matching
//! `AzCallback` / `AzLayoutCallback` produced by libazul's
//! `_createFromHostHandle`. The user passes that wrapper struct directly
//! to the C-ABI function.

/// Emit the LuaJIT prelude that registers callback invokers + RefAny
/// helpers under the `azul` namespace.
///
/// Must be inserted *after* `local C = ffi.load('azul')` and *before* the
/// wrapper layer, because wrappers reference `azul._register_callback`.
pub fn emit_managed_prelude(out: &mut String) {
    out.push_str(MANAGED_PRELUDE);
    out.push('\n');
}

/// Verbatim Lua source. Keep in sync with `super::wrappers`: the wrapper
/// emitter inlines `azul._register_callback('Callback', user_fn)` for
/// every callback-typed argument.
const MANAGED_PRELUDE: &str = r#"
-- ────────────────────────────────────────────────────────────────────────
-- Managed-FFI runtime helpers (host-invoker pattern)
--
-- libazul exports per callback kind:
--   * a static thunk (the `cb` field of the callback wrapper),
--   * `Az<Kind>_createFromHostHandle(u64) -> AzCallback`-style constructor,
--   * `AzApp_set<Kind>Invoker(fn)` setter.
--
-- We register one libffi closure per kind at module load (these have
-- *pointer-arg* signatures which LuaJIT FFI handles fine — the by-value
-- plumbing happens inside libazul's static thunk). User callbacks then
-- live in a Lua table keyed by integer id; the framework's RefAny
-- destructor calls back through `AzApp_setHostHandleReleaser` to clear
-- the entry.
-- ────────────────────────────────────────────────────────────────────────

ffi.cdef[[
    /* Host-handle releaser — called once per RefAny last-clone drop. */
    void AzApp_setHostHandleReleaser(void (*)(uint64_t));

    /* User-data RefAny on top of the host-handle path: one shared
       lifetime story for both callback registration and refany_create. */
    AzRefAny AzRefAny_newHostHandle(uint64_t);
    uint64_t AzRefAny_getHostHandle(const AzRefAny*);

    /* Per-kind invoker setters + pointer-arg signatures. The return
       value is an *out-parameter* so LuaJIT (which can't return
       aggregates > 8 bytes from callbacks) handles every kind uniformly. */
    typedef void (*AzCallbackInvoker)(
        uint64_t, const AzRefAny*, const AzCallbackInfo*, AzUpdate*);
    typedef void (*AzLayoutCallbackInvoker)(
        uint64_t, const AzRefAny*, const AzLayoutCallbackInfo*, AzDom*);

    void AzApp_setCallbackInvoker(AzCallbackInvoker);
    void AzApp_setLayoutCallbackInvoker(AzLayoutCallbackInvoker);

    AzCallback AzCallback_createFromHostHandle(uint64_t);
    AzLayoutCallback AzLayoutCallback_createFromHostHandle(uint64_t);
]]

-- One Lua table for every host handle libazul knows about — both user
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

-- ── Callback-kind invokers ─────────────────────────────────────────────
--
-- Each invoker re-enters Lua, looks up the user's fn by id, and runs it.
-- Both args are pointers (LuaJIT FFI can synthesize pointer-arg cdata
-- bodies in `ffi.cast`; struct-by-value args, which the original C-ABI
-- typedefs use, are what doesn't work). The static thunk in libazul
-- bridges the gap.

-- Each invoker writes its return value through `out_ptr`. The static
-- thunk in libazul pre-fills `out` with the kind's default value before
-- calling, so a buggy invoker that "forgets" to write still leaves the
-- framework with a sane fallback rather than uninitialized memory.

local _callback_invoker = ffi.cast('AzCallbackInvoker',
    function(id, data_ptr, info_ptr, out_ptr)
        local fn = _lua_handles[tonumber(id)]
        if fn == nil then return end -- thunk's pre-filled default takes over
        local ok, ret = pcall(fn, data_ptr, info_ptr)
        if not ok then
            -- Surfacing the error inside the framework would tear down
            -- the event loop. Print and bail; thunk's default stands.
            io.stderr:write("[azul] callback error: ", tostring(ret), "\n")
            return
        end
        if ret ~= nil then out_ptr[0] = ret end
    end)
table.insert(_live_pins, _callback_invoker)
C.AzApp_setCallbackInvoker(_callback_invoker)

local _layout_invoker = ffi.cast('AzLayoutCallbackInvoker',
    function(id, data_ptr, info_ptr, out_ptr)
        local fn = _lua_handles[tonumber(id)]
        if fn == nil then return end
        local ok, ret = pcall(fn, data_ptr, info_ptr)
        if not ok then
            io.stderr:write("[azul] layout callback error: ", tostring(ret), "\n")
            return
        end
        if ret ~= nil then
            -- AzDom is a struct, not a pointer; assign through the out pointer.
            ffi.copy(out_ptr, ret, ffi.sizeof('AzDom'))
        end
    end)
table.insert(_live_pins, _layout_invoker)
C.AzApp_setLayoutCallbackInvoker(_layout_invoker)

-- Wrapper-emitted methods call this to wrap a Lua function into a
-- callback wrapper struct the framework can store. Returns the AzCallback
-- (or AzLayoutCallback) by value; user passes it on to setOnClick/etc.
function azul._register_callback(kind, fn)
    if fn == nil then return nil end
    if type(fn) ~= 'function' then
        error("azul._register_callback: expected function, got "..type(fn), 2)
    end
    local id = _alloc_handle(fn)
    if kind == 'Callback' then
        return C.AzCallback_createFromHostHandle(id)
    elseif kind == 'LayoutCallback' then
        return C.AzLayoutCallback_createFromHostHandle(id)
    else
        error("azul._register_callback: unknown kind '"..tostring(kind).."'", 2)
    end
end

-- ── RefAny user-data helpers ──────────────────────────────────────────
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
