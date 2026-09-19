//! Lua FFI binding generator — LuaJIT `ffi` and PUC Lua via cffi-lua.
//!
//! Emits ONE `azul.lua` source file that exposes the C ABI of `libazul`
//! through an FFI module. The file is dual-runtime: it loads under LuaJIT
//! 2.0+ (built-in `ffi`) and under vanilla PUC Lua 5.1+ with the
//! `cffi-lua` rock (`require('cffi')`), whose API mirrors LuaJIT's.
//! `generator.rs` writes the same bytes a second time as `azul_cffi.lua`
//! (the file name the vanilla-Lua install instructions download).
//!
//! Layers of the generated file, top to bottom:
//!
//! 1. Prologue — FFI module resolution (`ffi`, else `cffi`) plus two runtime shims,
//!    `_is_cdata` / `_tonum`, that hide the two implementations' differences.
//! 2. `ffi.cdef[[ ... ]]` — the TYPES of `azul.h` with preprocessor directives stripped (see
//!    [`cdef::strip_for_cdef`]). Function declarations are registered lazily (ctype budget).
//! 3. `local C = <memoizing proxy over ffi.load('azul')>` — resolves each function on first use
//!    and routes every call that moves a struct / tagged union by value through its generated
//!    `<fn>Byref` twin (see [`byref_routes`]).
//! 4. Managed prelude ([`managed`]) — host-invoker registrations, RefAny helpers, `_consume`.
//! 5. Idiomatic wrappers ([`wrappers::generate_wrappers`]) — methods tables + metatypes + an
//!    `azul` namespace where method names drop the `Az<TypeName>_` prefix.
//! 6. Postlude — `azul.String.from_lua`, `azul._apply_opts`.
//!
//! ## Dual-runtime rules for emitted Lua
//!
//! * No LuaJIT-only literals (`0ULL`, `1LL`): host-handle ids are plain Lua numbers (exact up
//!   to 2^53; the C side takes `uint64_t`).
//! * Never `type(x) == 'cdata'` — cffi-lua cdata are userdata to `type()`; use `_is_cdata(x)`
//!   or `ffi.istype(ct, x)`. Never `tonumber(cdata)` — use `_tonum(x)`.
//! * No `if not jit` guard: any module that provides the LuaJIT `ffi` API under the name `ffi`
//!   (luaffi, luaffifb) is picked up by the same probe.
//!
//! ## Cross-module call to the C generator
//!
//! The cdef payload comes from the production C-header generator with the
//! standard `c_header` configuration, piped through `strip_for_cdef`, so
//! the Lua cdef stays in lockstep with `azul.h` — including the `<fn>Byref`
//! twins the routing below relies on.

pub mod cdef;
pub mod managed;
pub mod rockspec;
pub mod wrappers;

use std::collections::HashMap;

use anyhow::Result;

use super::{
    config::CodegenConfig,
    generator::{CodeBuilder, LanguageGenerator},
    ir::CodegenIR,
    lang_c::CGenerator,
};

/// Generate the full `azul.lua` source file.
///
/// `config` is currently unused for output shaping (the Lua binding has no
/// per-target dialects), but is taken by reference to match the signatures
/// of other language entry points and so future configuration hooks can be
/// threaded through without breaking callers.
pub fn generate(ir: &CodegenIR, _config: &CodegenConfig) -> Result<String> {
    // 1. Run the production C-header generator and strip preprocessor directives so the result is
    //    acceptable inside `ffi.cdef[[...]]`.
    let c_config = CodegenConfig::c_header();
    let c_header = CGenerator.generate(ir, &c_config)?;
    let cdef_payload = cdef::strip_for_cdef(&c_header);

    // 2. Build the Lua source.
    let mut builder = CodeBuilder::new("    ");
    builder.raw(PROLOGUE);

    // 3. cdef block — TYPES ONLY, functions go lazy.
    //
    // LuaJIT's ctype-ID space is 16-bit (~65 536 type IDs for the whole
    // VM). Declaring the entire API eagerly — ~4 300 structs plus ~14 700
    // function prototypes, each consuming IDs for its signature — OVERFLOWS
    // it: `luajit: table overflow` inside the single big `ffi.cdef`, before
    // one line of user code ran (the AZ_E2E scripting chronic). Types are
    // still declared eagerly here; every FUNCTION declaration is deferred
    // into `__az_fn_decls` below and `ffi.cdef`'d on FIRST USE, so the
    // ctype budget scales with what the program actually calls (a real app
    // touches hundreds of functions, not fourteen thousand).
    //
    // Function-POINTER typedefs (the callback kinds, `typedef R (*AzXType)(...)`)
    // are split out as well and cdef'd one by one with a fallback — see
    // CB_TYPEDEF_LOOP. Single-line `/* ... */` comments (the Byref twin
    // markers of the function region, ~9 000 lines) carry no declaration
    // and are dropped from the payload.
    let mut type_lines = String::new();
    let mut fn_decls: Vec<CFnDecl> = Vec::new();
    let mut cb_typedefs: Vec<(String, String)> = Vec::new();
    for line in cdef_payload.lines() {
        let t = line.trim();
        if t.starts_with("/*") && t.ends_with("*/") {
            continue;
        }
        if let Some(name) = cdef::parse_callback_typedef_name(t) {
            debug_assert!(!t.contains('\'') && !t.contains('\\'));
            cb_typedefs.push((name, t.to_string()));
            continue;
        }
        match parse_c_fn_decl(line) {
            Some(decl) => fn_decls.push(decl),
            None => {
                type_lines.push_str(line);
                type_lines.push('\n');
            }
        }
    }

    builder.line("ffi.cdef[[");
    builder.raw(&type_lines);
    if !type_lines.ends_with('\n') {
        builder.raw("\n");
    }
    builder.line("]]");
    builder.blank();

    // 3a. Callback function-pointer typedefs, one cdef each with a fallback
    //     (rationale in CB_TYPEDEF_LOOP). Struct fields typed with them were
    //     rewritten to `void*` by cdef.rs, and every function declaration
    //     that names them is cdef'd lazily below, after this loop ran.
    builder.line("-- Callback function-pointer typedefs: { name, declaration } in header order.");
    builder.line("local __az_cb_typedefs = {");
    for (name, decl) in &cb_typedefs {
        builder.line(&format!("    {{ \"{name}\", '{decl}' }},"));
    }
    builder.line("}");
    builder.raw(CB_TYPEDEF_LOOP);

    // 3b. The lazy function-declaration registry (plain Lua strings — costs
    //     table slots, never ctype IDs, until a function is first called).
    builder.line("-- Function declarations, cdef'd lazily on first use (see the ctype");
    builder.line("-- budget note above). Keys are the C symbol names.");
    builder.line("local __az_fn_decls = {");
    for d in &fn_decls {
        // C declarations contain no quotes/backslashes; plain single-quoted
        // Lua strings are safe. Defensive assert keeps that true.
        debug_assert!(!d.decl.contains('\'') && !d.decl.contains('\\'));
        builder.line(&format!("    [\"{}\"] = '{}',", d.name, d.decl));
    }
    builder.line("}");
    builder.blank();

    // 3c. Byref routing table (the rationale lives in LOAD_PROXY, emitted
    //     right below it). Classification is the IR's `is_value_aggregate`
    //     — the same predicate lang_c.rs uses to shape the twins — and every
    //     twin is cross-checked against the classification before it is
    //     listed, so the Lua side can never pass a number where the twin
    //     wants a pointer or vice versa.
    let routes = byref_routes(ir, &fn_decls);
    builder.line("-- Byref routing table (see the proxy below): C symbol -> { ret = <C return");
    builder.line("-- type or nil>, agg = <return is a struct / tagged union> }. Only functions");
    builder.line("-- that move an aggregate by value (argument or return) AND have a Byref twin");
    builder.line("-- declared above are listed; everything else is called directly.");
    builder.line("local __az_fn_meta = {");
    for r in &routes {
        let body = match &r.ret {
            Some(ret) => format!(" ret = \"{ret}\", agg = {} ", r.agg_ret),
            None => String::new(),
        };
        builder.line(&format!("    [\"{}\"] = {{{body}}},", r.name));
    }
    builder.line("}");
    builder.blank();

    // 4. Load the native library behind a MEMOIZING PROXY: first access to a function cdefs its
    //    declaration, resolves it (through the Byref twin where the routing table says so) and
    //    caches the callable; enum constants and anything else fall through untouched.
    builder.raw(LOAD_PROXY);

    // 5. Public namespace table.
    builder.line("local azul = {}");
    builder.line("-- Re-export the raw C-ABI table for power users / advanced FFI casts.");
    builder.line("azul.C = C");
    builder.line("-- Runtime shims, exported for user code that inspects cdata portably.");
    builder.line("azul._is_cdata = _is_cdata");
    builder.line("azul._tonum = _tonum");
    builder.blank();

    // 6. Managed-FFI prelude: per-kind invoker registrations + RefAny user-data store +
    //    transient helpers (`_az_string`, `_refany_arg`, `_consume`, `pin_callback`). Must be
    //    emitted before the wrappers because wrapper methods reference them. The prelude is
    //    data-driven from the IR so adding a new callback kind to api.json contributes one cdef +
    //    one FFI callback registration automatically.
    let mut managed_buf = String::new();
    managed::emit_managed_prelude(&mut managed_buf, ir);
    builder.raw(&managed_buf);

    // 7. Wrapper layer.
    builder.raw(&wrappers::generate_wrappers(ir));

    // 8. Postlude: ergonomic helpers that need to attach to wrapper tables after the wrappers have
    //    created them.
    builder.raw(POSTLUDE);

    // 9. Trailer.
    builder.line("return azul");

    Ok(builder.finish())
}

// ============================================================================
// C declaration parsing + Byref routing classification
// ============================================================================

/// One `extern <ret> <name>(<params>);` line of the cdef payload, split into
/// the parts the Byref routing needs. Param types are normalised —
/// `const` / `restrict` dropped, pointer-ness recorded separately — so
/// `const AzApp* app` becomes `("AzApp", true)` and `AzButtonType button_type`
/// becomes `("AzButtonType", false)`.
struct CFnDecl {
    /// C symbol name (`AzApp_create`).
    name: String,
    /// Return type text as written (`void`, `AzDom`, `const void*`).
    ret: String,
    /// `(base type, is_pointer)` per parameter; empty for `(void)`.
    params: Vec<(String, bool)>,
    /// The whole declaration, trimmed — what gets `ffi.cdef`'d lazily.
    decl: String,
}

fn parse_c_fn_decl(line: &str) -> Option<CFnDecl> {
    let t = line.trim();
    if !(t.starts_with("extern ") && t.ends_with(");") && t.contains('(')) {
        return None;
    }
    let open = t.find('(')?;
    let close = t.rfind(')')?;
    let before = &t[..open];
    let name = before
        .rsplit(|c: char| c.is_whitespace() || c == '*')
        .next()?
        .trim()
        .to_string();
    if name.is_empty() {
        // Unparseable: keep it eager rather than lose it.
        return None;
    }
    let ret = before["extern".len()..].trim();
    let ret = ret[..ret.len() - name.len()].trim().to_string();
    let mut params = Vec::new();
    let inner = t[open + 1..close].trim();
    if !inner.is_empty() && inner != "void" {
        for p in inner.split(',') {
            let is_ptr = p.contains('*');
            let ty = p
                .split(|c: char| c.is_whitespace() || c == '*')
                .map(str::trim)
                .find(|tok| !tok.is_empty() && *tok != "const" && *tok != "restrict")
                .unwrap_or("")
                .to_string();
            params.push((ty, is_ptr));
        }
    }
    Some(CFnDecl {
        name,
        ret,
        params,
        decl: t.to_string(),
    })
}

/// Is the C type name (`AzButton`, `AzButtonType`, `AzGLuint`) a struct or
/// tagged union that crosses the ABI by value? Delegates to the IR's
/// [`CodegenIR::is_value_aggregate`] — the ONE predicate that also shapes
/// the `<fn>Byref` twins in lang_c.rs / the dll — so C enums, scalar
/// typedefs and function-pointer typedefs are never treated as aggregates.
fn is_c_value_aggregate(ir: &CodegenIR, c_type: &str) -> bool {
    c_type
        .strip_prefix("Az")
        .is_some_and(|name| ir.is_value_aggregate(name))
}

/// One entry of the emitted `__az_fn_meta` table.
struct ByrefRoute {
    name: String,
    /// C return type of the ORIGINAL function (`None` for void).
    ret: Option<String>,
    /// The return is a struct / tagged union (twin fills a cdata struct);
    /// otherwise a scalar / pointer / enum (twin fills a 1-element array).
    agg_ret: bool,
}

/// Decide which functions the proxy routes through their `<fn>Byref` twin.
///
/// Rule: a function is routed iff it moves at least one aggregate (struct
/// or tagged union, per [`is_c_value_aggregate`]) by value — as an owned
/// argument or as its return — AND a twin with the matching shape is
/// declared in the header. The twin's shape is verified position by
/// position (`__ret` out-pointer first when the original returns
/// non-void, then every original parameter, pointerised iff it was already
/// a pointer or is an aggregate); a mismatch is reported on stderr and the
/// function is called directly instead of emitting a wrapper that would
/// pass the wrong argument kinds at runtime.
fn byref_routes(ir: &CodegenIR, decls: &[CFnDecl]) -> Vec<ByrefRoute> {
    let by_name: HashMap<&str, &CFnDecl> = decls.iter().map(|d| (d.name.as_str(), d)).collect();
    let mut routes = Vec::new();
    for d in decls {
        if d.name.ends_with("Byref") {
            continue;
        }
        let agg_args: Vec<bool> = d
            .params
            .iter()
            .map(|(ty, is_ptr)| !is_ptr && is_c_value_aggregate(ir, ty))
            .collect();
        let has_ret = !d.ret.is_empty() && d.ret != "void";
        let agg_ret = has_ret && !d.ret.contains('*') && is_c_value_aggregate(ir, &d.ret);
        if !agg_ret && !agg_args.iter().any(|b| *b) {
            continue;
        }
        let twin_name = format!("{}Byref", d.name);
        let Some(twin) = by_name.get(twin_name.as_str()) else {
            // No twin declared: the direct by-value symbol is the only option.
            continue;
        };
        let mut expected: Vec<bool> = Vec::with_capacity(d.params.len() + 1);
        if has_ret {
            expected.push(true);
        }
        expected.extend(
            d.params
                .iter()
                .zip(&agg_args)
                .map(|((_, is_ptr), is_agg)| *is_ptr || *is_agg),
        );
        let actual: Vec<bool> = twin.params.iter().map(|(_, is_ptr)| *is_ptr).collect();
        if twin.ret != "void" || expected != actual {
            eprintln!(
                "[lua codegen] WARN: {twin_name} does not match the Byref routing \
                 classification of {} (expected pointer positions {expected:?}, twin has \
                 {actual:?}); calling the by-value symbol directly",
                d.name
            );
            continue;
        }
        routes.push(ByrefRoute {
            name: d.name.clone(),
            ret: has_ret.then(|| d.ret.clone()),
            agg_ret,
        });
    }
    routes
}

// ============================================================================
// Static Lua chunks
// ============================================================================

const PROLOGUE: &str = r#"-- azul.lua — Lua FFI bindings for the Azul GUI framework
-- WARNING: autogenerated by azul-doc codegen v2 — DO NOT EDIT
--
-- Runs unchanged under LuaJIT 2.0+ (built-in `ffi`) and under PUC Lua 5.1+
-- with the cffi-lua module (`luarocks install cffi-lua`), whose API mirrors
-- LuaJIT's. Any other module that provides the LuaJIT `ffi` API under the
-- name `ffi` (luaffi, luaffifb) is picked up by the same probe.

local ffi
do
    local ok, mod = pcall(require, 'ffi')
    if ok then
        ffi = mod
    else
        local ok2, mod2 = pcall(require, 'cffi')
        if not ok2 then
            error("azul.lua needs an FFI module: run it under LuaJIT (built-in 'ffi') "
                .. "or install cffi-lua for PUC Lua (`luarocks install cffi-lua`).\n"
                .. "  require('ffi'): " .. tostring(mod) .. "\n"
                .. "  require('cffi'): " .. tostring(mod2), 0)
        end
        ffi = mod2
    end
end

-- Runtime shims over the two FFI implementations:
--   _is_cdata(x)  cffi-lua cdata are userdata to type(), so `type(x) == 'cdata'`
--                 is LuaJIT-only; cffi-lua exposes ffi.type() with LuaJIT's answer.
--   _tonum(x)     tonumber() on a boxed 64-bit integer / enum cdata is LuaJIT-only;
--                 cffi-lua exposes ffi.tonumber().
local _ffi_type = ffi.type or type
local function _is_cdata(x) return _ffi_type(x) == 'cdata' end
local _tonum = ffi.tonumber or tonumber

"#;

const CB_TYPEDEF_LOOP: &str = r#"-- cdef'd one at a time: an FFI that cannot marshal a typedef's by-value
-- aggregates rejects the declaration (cffi-lua refuses any struct that
-- contains a union by value where libffi has no union support — which is
-- most callback signatures, e.g. `AzCallbackInfo` by value). Such a name
-- is bound to an opaque pointer-sized alias instead: struct fields typed
-- with it are already `void*`, function parameters typed with it still
-- declare, and azul.pin_callback reports the kind as unbuildable rather
-- than the whole module failing to load. LuaJIT accepts every declaration
-- here (it only reports NYI when a callback is actually created).
for _, td in ipairs(__az_cb_typedefs) do
    if not pcall(ffi.cdef, td[2]) then
        ffi.cdef('typedef void* ' .. td[1] .. ';')
    end
end
__az_cb_typedefs = nil

"#;

const LOAD_PROXY: &str = r#"-- Byref routing.
-- LuaJIT's C-call frame has a fixed budget for STACK-passed argument words
-- (CCALL_MAXSTACK = 32 slots = 256 bytes on 64-bit targets). On x86-64 SysV
-- every aggregate larger than 16 bytes is passed by value ON THE STACK, so
-- the SUM of by-value aggregate bytes decides: `AzApp_create(AzRefAny,
-- AzAppConfig)` alone needs ~2.4 KiB and fails with "NYI: cannot call this
-- C function (yet)". arm64 passes large aggregates indirectly, which is why
-- the same file used to run on Apple Silicon only. Rather than estimating
-- sizes per ABI, every call that moves a struct / tagged union by value —
-- as an argument OR as the return value — goes through the generated
-- `<fn>Byref` twin: owned aggregates by pointer (CONSUMED exactly like the
-- by-value call), C enums and scalars unchanged, the result written to a
-- leading out-pointer. cdata structs auto-convert to `T*` at the FFI
-- boundary, so wrapper call sites are identical on both paths. A twin the
-- loaded libazul does not export (older build) falls back to the direct
-- by-value symbol.
local function __az_byref_wrap(raw_fn, meta)
    local retT = meta.ret
    if retT == nil then
        return raw_fn
    end
    if meta.agg then
        -- Aggregate return: the twin fills a caller-owned cdata. ffi.new would
        -- arm the metatype __gc; detach it so the value comes back UNARMED,
        -- exactly like a by-value C return — the wrapper layer arms owned
        -- returns itself via ffi.gc(v, Az<T>_delete), and transients that
        -- the next C call consumes must never carry a finalizer.
        local ct = ffi.typeof(retT)
        return function(...)
            local out = ffi.new(ct)
            ffi.gc(out, nil)
            raw_fn(out, ...)
            return out
        end
    end
    -- Scalar / pointer / enum return: 1-element out array, unboxed on return
    -- (numbers stay numbers, booleans stay booleans, enums come back exactly
    -- as the FFI returns them from a direct call).
    local ct = ffi.typeof(retT .. '[1]')
    return function(...)
        local out = ffi.new(ct)
        raw_fn(out, ...)
        return out[0]
    end
end

local __az_raw = ffi.load('azul')
local C = setmetatable({}, {
    __index = function(cache, name)
        local value
        local meta = __az_fn_meta[name]
        if meta then
            local byname = name .. 'Byref'
            local bydecl = __az_fn_decls[byname]
            if bydecl then
                ffi.cdef(bydecl)
                __az_fn_decls[byname] = nil -- cdef exactly once
                local ok, raw_fn = pcall(function() return __az_raw[byname] end)
                if ok then
                    value = __az_byref_wrap(raw_fn, meta)
                end
            end
        end
        if value == nil then
            local decl = __az_fn_decls[name]
            if decl then
                ffi.cdef(decl)
                __az_fn_decls[name] = nil -- cdef exactly once
            end
            value = __az_raw[name]
        end
        rawset(cache, name, value) -- memoize: __index fires once per name
        return value
    end,
})

"#;

const POSTLUDE: &str = r#"
-- Postlude: convenience helpers that hang off generated wrappers.
azul.String.from_lua = function(s)
    -- User-facing factory: the CALLER owns the returned AzString. C-call
    -- returns are never armed by the FFI, so arm the finalizer explicitly —
    -- a dropped result then frees its heap buffer. A consuming C call
    -- disarms it via azul._consume (owned AzString args are always
    -- consumed by the C ABI).
    return ffi.gc(azul._az_string(s), C.AzString_delete)
end

-- Recursive opts-table applier (mirrors Ruby's `_apply_opts` and Node's
-- `_applyOpts`). Every struct wrapper's `:with(opts)` method routes
-- through this so users can replace
--   window.window_state.title = azul._az_string('Hello World')
--   window.window_state.size.dimensions.width = 400.0
-- with
--   window:with({ window_state = { title = 'Hello World',
--                  size = { dimensions = { width = 400.0 } } } })
-- The FFI lets us assign nested cdata struct fields directly
-- (`struct.field = value`); we only need to recurse into Lua tables and
-- auto-convert strings. Everything else (wrapper instances, enum
-- constants, numbers, booleans) direct-assigns — the FFI does the byte
-- copy. A cdata value byte-copied into a field is thereby OWNED by the
-- containing struct (Rust drops it with the struct), so its own finalizer
-- is detached like for any other consumed by-value argument.
azul._apply_opts = function(struct, opts)
    -- `opts` is always a plain Lua table; `not opts` is safe to falsy-
    -- check. We deliberately do NOT check `struct == nil`: cdata types
    -- with an __eq metamethod invoke it with `nil` as the second operand.
    if not opts then return end
    for k, v in pairs(opts) do
        local t = type(v)
        if t == 'string' then
            struct[k] = azul._az_string(v)
        elseif t == 'table' then
            azul._apply_opts(struct[k], v)
        else
            struct[k] = v
            azul._consume(v)
        end
    end
end

"#;
