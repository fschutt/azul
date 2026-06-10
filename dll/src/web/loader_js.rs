//! Generate the azul-loader.js bootstrap script.
//!
//! M8.6 loader: instantiates `azul-mini.wasm` + every per-callback
//! WASM into a shared `WebAssembly.Table` (funcref), wires native
//! event listeners on `document.body`, marshals events into a fixed
//! 256-byte buffer + calls `AzStartup_dispatchEvent`, then applies
//! the patch byte-stream returned by the eventloop.
//!
//! Architecture (M8.5 confirmed end-to-end via Node):
//!   - `azul-mini.wasm` exports `AzStartup_init`, `_dispatchEvent`,
//!     `_alloc`, `_free`, `_registerStateDeserializer` + `memory`.
//!     Imports `env.__indirect_function_table` + `env.__az_resolve_callback`.
//!   - Per-callback WASMs (`/az/cb/<sym>.<hash>.wasm`) export
//!     `callback(i64, i64, i32) -> i32`. JS instantiates each at a
//!     known table slot.
//!   - JS maps `node_idx → table_idx` (M8.6 stub: identity, since
//!     dispatchEvent currently passes `node_idx` to
//!     `__az_resolve_callback`; M8.5c will swap to real fn-addrs
//!     looked up from a hydrated StyledDom inside WASM).
//!
//! Per the user direction (`default = client-side; no server
//! fallback in loader.js`), the loader does NOT call POST
//! `/az/exec/`. Failures log to console + silently no-op.

use super::CallbackWasm;

/// Generate the loader JavaScript.
///
/// Args are kept for forward compatibility — the M8.6 loader pulls
/// the mini.wasm URL from the `<link rel="preload">` hint in the
/// HTML head + the per-cb URLs from `[data-az-wasm]` attributes.
pub fn generate_loader_js(
    _mini_wasm_hash: &str,
    _callbacks: &[CallbackWasm],
) -> String {
    generate_m8_loader()
}

/// M8.6 loader.
fn generate_m8_loader() -> String {
    r#"(function(){
'use strict';

// =====================================================================
// Event-kind constants — must match `event_kind` module in
// dll/src/web/eventloop.rs.
// =====================================================================
var EVT_CLICK     = 0;
var EVT_MOUSEDOWN = 1;
var EVT_MOUSEUP   = 2;
var EVT_MOUSEMOVE = 3;
var EVT_DBLCLICK  = 4;
var EVT_WHEEL     = 5;
var EVT_KEYDOWN   = 6;
var EVT_KEYUP     = 7;
var EVT_FOCUSIN   = 8;
var EVT_FOCUSOUT  = 9;
var EVT_RESIZE    = 10;
var EVT_SCROLL    = 11;

var EVENT_BUFFER_SIZE = 256;
var OUT_LEN_SIZE = 4;
var SENTINEL_NO_NODE = 0xFFFFFFFF;

// =====================================================================
// Shared state (populated by azBootstrap).
// =====================================================================
var azMini = null;       // mini.wasm instance.exports
var azState = 0;          // App state ptr from AzStartup_init
var azMemory = null;      // mini's WebAssembly.Memory (shared via export)
var azTable = null;       // WebAssembly.Table for indirect callback dispatch

// __multi3 = compiler-rt 128-bit multiply, a LEAKED wasm import (same class as the
// fmaxf/fminf/roundf libcall leak). LLVM lowers Rust u128/i128 multiply (Vec/Layout::array
// overflow checks, ratio math) to a `__multi3` call. Unprovided it stubs to 0/garbage AND
// never writes its sret → corrupt alloc sizes/lengths. Real impl: result(128) → [sret].
// wasm sig (i32 sret, i64 aLo, i64 aHi, i64 bLo, i64 bHi) -> nil.
function azMulti3(sret, aLo, aHi, bLo, bHi) {
    var dv = new DataView(azMemory.buffer);
    var mask = 0xFFFFFFFFFFFFFFFFn;
    var a = ((BigInt.asUintN(64, BigInt(aHi))) << 64n) | BigInt.asUintN(64, BigInt(aLo));
    var b = ((BigInt.asUintN(64, BigInt(bHi))) << 64n) | BigInt.asUintN(64, BigInt(bLo));
    var p = BigInt.asUintN(128, a * b);
    dv.setBigUint64(Number(sret), p & mask, true);
    dv.setBigUint64(Number(sret) + 8, (p >> 64n) & mask, true);
}

// node_idx → table_idx (M8.6 stub: identity since dispatchEvent uses
// node_idx as the fn-addr-lookup key. M8.5c+ will swap to real
// fn-addrs harvested from a hydrated StyledDom.)
var azFnAddrToTableIdx = new Map();

// M10-D: parsed manifest + boundary symbol table. In sharded mode,
// every Az* framework symbol referenced by a cb / layout / mini wasm
// becomes an env-import named `sub_<canonical_synth_hex>`. The
// boundary shard at `/az/fn/<name>.<hash>.wasm` exports the matching
// `sub_<canonical_synth_hex>` body. Once loaded, the export goes
// into azBoundarySymbols and azCallbackImports() routes env-imports
// through it before falling back to stub-noops.
var azManifest = null;
var azBoundarySymbols = new Map();   // sub_<synth_hex> → exported wasm fn

// =====================================================================
// Imports given to mini.wasm at instantiate-time.
// `__indirect_function_table` + `__az_resolve_callback` are the
// load-bearing ones; everything else (remill FPU helpers, lift
// dedup artifacts like `sub_<hex>.1`) falls through to a Proxy
// that returns shape-appropriate no-ops. Without the Proxy, any
// new import added by a future eventloop lift fails instantiation.
// =====================================================================
function azMakeMiniImports() {
    azTable = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    var i64_noop  = function() { return 0n; };
    var i32_noop  = function() { return 0; };
    var void_noop = function() { /* no return */ };
    var realEnv = {
        __indirect_function_table: azTable,
        __az_resolve_callback: function(fnAddr) {
            var n = Number(fnAddr);
            var idx = azFnAddrToTableIdx.get(n);
            return idx === undefined ? SENTINEL_NO_NODE : idx;
        },
        // M12.7: libc math libcalls. LLVM's wasm backend lowers Rust f32::max/min
        // (`@llvm.maxnum/minnum.f32`) and .round() to `fmaxf`/`fminf`/`roundf` calls
        // (their NaN/sign-of-zero semantics differ from wasm's native f32.max/min).
        // These MUST be real — the layout solver floors every used size with
        // `.max(0.0)`, so a 0-returning stub zeroes ALL widths/heights. fmaxf/fminf
        // follow IEEE maxNum/minNum (a NaN operand yields the other).
        fmaxf: function(a, b) { return a !== a ? b : (b !== b ? a : Math.max(a, b)); },
        fminf: function(a, b) { return a !== a ? b : (b !== b ? a : Math.min(a, b)); },
        fmax:  function(a, b) { return a !== a ? b : (b !== b ? a : Math.max(a, b)); },
        fmin:  function(a, b) { return a !== a ? b : (b !== b ? a : Math.min(a, b)); },
        roundf: function(x) { return Math.sign(x) * Math.round(Math.abs(x)); },
        round:  function(x) { return Math.sign(x) * Math.round(Math.abs(x)); },
        fabsf: Math.abs, fabs: Math.abs,
        sqrtf: Math.sqrt, sqrt: Math.sqrt,
        floorf: Math.floor, floor: Math.floor,
        ceilf: Math.ceil, ceil: Math.ceil,
        truncf: Math.trunc, trunc: Math.trunc,
        powf: Math.pow, pow: Math.pow,
        __multi3: azMulti3,
    };
    function stubFor(name) {
        if (name.indexOf('write_memory') !== -1 ||
            name.indexOf('barrier') !== -1 ||
            name.indexOf('exception_clear') !== -1) return void_noop;
        if (/(?:_64|_f64)\b/.test(name)) return i64_noop;
        return i32_noop;
    }
    var handler = {
        get: function(_t, prop) {
            if (typeof prop !== 'string') return undefined;
            if (Object.prototype.hasOwnProperty.call(realEnv, prop)) {
                return realEnv[prop];
            }
            return stubFor(prop);
        },
        has: function() { return true; },
    };
    return { env: new Proxy({}, handler) };
}

// =====================================================================
// Per-callback / per-layout WASM imports.
//
// Per-cb wasms are linked with `--import-memory` + `--import-table`
// so they share linear address space + the indirect funcref table
// with mini.wasm. JS routes `env.memory` to mini's exported memory
// + `env.__indirect_function_table` to the table created in
// azMakeMiniImports.
//
// `--allow-undefined` means any other unresolved symbol becomes an
// import too — for the lifted remill code that's
// `__remill_fpu_exception_clear/test` (FP exception helpers) plus
// any `sub_<hex>` the recursive walk left as a stub. We satisfy
// them all with a Proxy that returns shape-appropriate no-ops:
// 0n for *_64 (i64), undefined for void-shaped helpers, 0 for the
// rest (i32). Surfacing as a Proxy means new mangled imports
// added by future lift work don't need a per-name JS shim.
// =====================================================================
function azCallbackImports() {
    var i64_noop  = function() { return 0n; };
    var i32_noop  = function() { return 0; };
    var void_noop = function() { /* no return */ };
    function stubFor(name) {
        if (name.indexOf('write_memory') !== -1 ||
            name.indexOf('barrier') !== -1 ||
            name.indexOf('exception_clear') !== -1) return void_noop;
        if (/_64\b/.test(name)) return i64_noop;
        return i32_noop;
    }
    // Real bindings: shared memory + shared table. Everything else
    // falls through to the Proxy.
    var realEnv = {
        memory: azMemory,
        __indirect_function_table: azTable,
        __multi3: azMulti3,
    };
    var handler = {
        get: function(_target, prop) {
            if (typeof prop !== 'string') return undefined;
            if (Object.prototype.hasOwnProperty.call(realEnv, prop)) {
                return realEnv[prop];
            }
            // M10-D: route every `sub_<canonical_synth_hex>` env-import
            // through the boundary symbol table. In sharded mode the
            // boundary shard exports the matching body; in legacy
            // mode the map is empty and we drop to stub-noop, which
            // matches the pre-M10-D behavior (the cb's wasm bundles
            // the body so the env-import was never reached).
            if (azBoundarySymbols.has(prop)) {
                return azBoundarySymbols.get(prop);
            }
            return stubFor(prop);
        },
        has: function() { return true; },
    };
    return { env: new Proxy({}, handler) };
}

// M10-D: fetch the manifest (best-effort) + pre-load every boundary
// shard. Each shard's `sub_<canonical_synth_hex>` export gets routed
// into `azBoundarySymbols` so subsequent cb / layout instantiations
// (which import `env.sub_<canonical_synth_hex>`) resolve to the real
// boundary body instead of a stub-noop.
//
// Skips silently when the manifest endpoint returns 404 / non-JSON
// (legacy bundled mode = sharded build off; per-cb wasms ship every
// boundary body inline, no shards needed). Errors loading individual
// shards downgrade to "stub-noop for that boundary" so a single
// missing shard doesn't break the whole page.
async function azLoadBoundaryShards() {
    try {
        var resp = await fetch('/az/manifest.json');
        if (!resp.ok) {
            console.debug('[azul-web] no manifest available (legacy mode)');
            return;
        }
        azManifest = await resp.json();
    } catch (e) {
        console.debug('[azul-web] manifest fetch failed:', e);
        return;
    }
    if (!azManifest || !Array.isArray(azManifest.boundaries) ||
        azManifest.boundaries.length === 0) {
        console.debug('[azul-web] manifest has no boundary shards (legacy mode)');
        return;
    }
    console.info('[azul-web] loading ' + azManifest.boundaries.length +
                 ' boundary shards...');
    // Parallel-fetch every shard. Each shard's wasm imports
    // env.memory + env.__indirect_function_table (same wiring as
    // cb wasms) but exports `sub_<canonical_synth_hex>` (the raw
    // remill-shape body) for downstream wasms to import.
    var imports = azCallbackImports();
    var loads = azManifest.boundaries.map(async function(b) {
        try {
            var mod = await WebAssembly.instantiateStreaming(fetch(b.url), imports);
            var bodyFn = mod.instance.exports[b.body_export];
            if (typeof bodyFn !== 'function') {
                console.warn('[azul-web] boundary ' + b.name + ' missing export ' +
                             b.body_export);
                return;
            }
            azBoundarySymbols.set(b.body_export, bodyFn);
            console.debug('[azul-web] boundary loaded: ' + b.name + ' → ' +
                          b.body_export);
        } catch (e) {
            console.warn('[azul-web] boundary load failed for ' + b.name + ':', e);
        }
    });
    await Promise.all(loads);
    console.info('[azul-web] boundary shards ready: ' +
                 azBoundarySymbols.size + '/' + azManifest.boundaries.length);
}

// =====================================================================
// M8.7c-3 hydration state — wasm-side AzRefAny constructed at boot.
// =====================================================================
var azRefAnyPtr = 0;    // wasm offset of the 24B AzRefAny aggregate
var azModelPtr  = 0;    // wasm offset of the user-data struct
                          // (hello-world: 4B holding the u32 counter)
// M9-2: Layout-cb instance + reserved table slot. The layout cb's
// `callback` export has the M9-1 wrapper shape
// `(refany_lo: i64, refany_hi: i64, info_ptr: i32, out_ptr: i32) -> i32`
// — last arg is the caller-allocated destination buffer for the
// returned AzDom (X8 hidden-ptr return). M9-3 wires the actual
// invocation via __az_call_indirect inside AzStartup_initLayoutCache.
var azLayoutCb = null;
var azLayoutCbTableIdx = -1;

// =====================================================================
// Bootstrap.
// =====================================================================
async function azBootstrap() {
    var miniLink = document.querySelector('link[rel="preload"][href*="/az/mini."]');
    if (!miniLink) {
        console.error('[azul-web] no /az/mini.<hash>.wasm preload hint found');
        return;
    }
    var miniUrl = miniLink.getAttribute('href');

    // 1. Fetch + instantiate mini.wasm with shared table + JS-side
    //    __az_resolve_callback. Mini OWNS the shared memory so it
    //    must instantiate before any wasm that imports memory.
    var imports = azMakeMiniImports();
    try {
        var miniMod = await WebAssembly.instantiateStreaming(fetch(miniUrl), imports);
        azMini = miniMod.instance.exports;
    } catch (e) {
        console.error('[azul-web] failed to instantiate mini.wasm:', e);
        return;
    }
    azMemory = azMini.memory;
    console.debug('[azul-web] mini exports:', Object.keys(azMini));

    // 1.5. M10-D: load every boundary shard (api.json Framework
    //      symbols factored out of the cb / layout / mini bundles).
    //      Their `sub_<canonical_synth_hex>` exports populate
    //      azBoundarySymbols so subsequent cb / layout instantiations
    //      can route their env-imports through them. Skips silently
    //      in legacy bundled mode (no manifest endpoint or empty
    //      boundaries array).
    await azLoadBoundaryShards();

    // 2./3. (2026-06-10 BOOTSTRAP-ORDER FIX) init + hydrate MOVED BELOW the cb/layout wasm
    //    instantiations. Every module instantiation re-runs its DATA SEGMENTS over the
    //    shared memory; the layout wasm carries the multi-MiB lifted-data mirror whose
    //    segments span the same band the bump heap allocates from (0x110000..~0x8664000 ∋
    //    0x6000000). Init/hydrate before it → EventloopState/RefAny/model bytes are
    //    CLOBBERED by the mirror → the cb's type_id check fails → every click = DoNothing.
    //    (Same ordering the node harness has always used — see layout-flexbox.js.)

    // 4. Discover + instantiate per-callback WASMs. Each gets put at
    //    table[node_idx] AND recorded in azNodeCbFns so the
    //    direct-invoke click handler below can call them without
    //    going through AzStartup_dispatchEvent.
    var cbs = document.querySelectorAll('[data-az-cb][data-az-wasm]');
    var azCbNodeIdxs = [];
    for (var i = 0; i < cbs.length; i++) {
        var el = cbs[i];
        var nodeIdxStr = el.getAttribute('data-az-cb');
        var nodeIdx = parseInt(nodeIdxStr, 10);
        if (isNaN(nodeIdx)) continue;
        var url = el.getAttribute('data-az-wasm');
        if (!url) continue;

        try {
            var cbMod = await WebAssembly.instantiateStreaming(fetch(url), azCallbackImports());
            var cbFn = cbMod.instance.exports.callback;
            if (typeof cbFn !== 'function') {
                console.warn('[azul-web] ' + url + ' has no `callback` export');
                continue;
            }
            while (azTable.length <= nodeIdx) {
                azTable.grow(16);
            }
            azTable.set(nodeIdx, cbFn);
            azFnAddrToTableIdx.set(nodeIdx, nodeIdx);
            // M9-4: remember the cb-bearing node; AzStartup_registerCbNode
            // is called AFTER init (azState doesn't exist yet — see the
            // bootstrap-order fix below).
            azCbNodeIdxs.push(nodeIdx);
            console.debug('[azul-web] cb node=' + nodeIdx + ' loaded from ' + url +
                          ' → table[' + nodeIdx + ']');
        } catch (e) {
            console.warn('[azul-web] failed to instantiate ' + url + ':', e);
        }
    }

    // 4.5. M9-2/M9-3: instantiate the layout wasm + run initLayoutCache.
    //      html_render.rs emits `<link rel="preload" href="/az/layout/...">`
    //      per route; we discover it the same way we found the mini
    //      wasm. The module shares memory + table with mini via the
    //      standard azCallbackImports() wiring. Reserve a table slot,
    //      tell mini about it via AzStartup_setLayoutCbTableIdx, then
    //      drive the first layout pass via AzStartup_initLayoutCache —
    //      from there everything lives in the WASM-resident DOM (M9-4+
    //      hit-tests and diff-patches against it).
    var layoutLink = document.querySelector('link[rel="preload"][href*="/az/layout/"]');
    if (layoutLink) {
        var layoutUrl = layoutLink.getAttribute('href');
        try {
            var layoutMod = await WebAssembly.instantiateStreaming(fetch(layoutUrl), azCallbackImports());
            var cbFn = layoutMod.instance.exports.callback;
            if (typeof cbFn !== 'function') {
                console.warn('[azul-web] layout wasm has no `callback` export');
            } else {
                azLayoutCbTableIdx = azTable.length;
                azTable.grow(1);
                azTable.set(azLayoutCbTableIdx, cbFn);
                azLayoutCb = cbFn;
                console.info('[azul-web] layout cb loaded from ' + layoutUrl +
                             ' → table[' + azLayoutCbTableIdx + ']');
            }
        } catch (e) {
            console.warn('[azul-web] failed to instantiate ' + layoutUrl + ':', e);
        }
    }

    // 4.6. (2026-06-10 BOOTSTRAP-ORDER FIX) Every wasm is instantiated — the shared
    //      memory's data segments are final. NOW seed the bump heap (each module's
    //      segments can clobber @__az_bump_ptr's linear-memory copy), build the state,
    //      hydrate the RefAny/model, and only then run the layout pipeline.
    if (typeof azMini.AzStartup_resetBumpHeap === 'function') {
        azMini.AzStartup_resetBumpHeap(96 * 1024 * 1024);
    }
    azState = azMini.AzStartup_init(0, 0);
    if (!azState) {
        console.error('[azul-web] AzStartup_init returned 0');
        return;
    }
    console.debug('[azul-web] AzStartup_init → state ptr', azState);
    azHydrate();
    for (var ri = 0; ri < azCbNodeIdxs.length; ri++) {
        if (typeof azMini.AzStartup_registerCbNode === 'function') {
            azMini.AzStartup_registerCbNode(azState, azCbNodeIdxs[ri]);
        }
    }
    if (azLayoutCb) {
        if (typeof azMini.AzStartup_setLayoutCbTableIdx === 'function') {
            azMini.AzStartup_setLayoutCbTableIdx(azState, azLayoutCbTableIdx);
        }
        if (typeof azMini.AzStartup_setRefAny === 'function' && azRefAnyPtr) {
            azMini.AzStartup_setRefAny(azState, azRefAnyPtr);
        }

        // Feed the fallback font (server route /az/fallback.ttf serves the dylib's
        // embedded TTF). Without real font bytes the wasm-side solver can't shape
        // text → text nodes get no rects → bbox hit-testing can't see the button.
        if (typeof azMini.AzStartup_setFallbackFont === 'function') {
            try {
                var fontResp = await fetch('/az/fallback.ttf');
                if (fontResp.ok) {
                    var fontBytes = new Uint8Array(await fontResp.arrayBuffer());
                    var fontPtr = azMini.AzStartup_alloc(fontBytes.length);
                    new Uint8Array(azMemory.buffer).set(fontBytes, fontPtr);
                    azMini.AzStartup_setFallbackFont(fontPtr, fontBytes.length);
                    console.debug('[azul-web] fallback font registered (' + fontBytes.length + ' bytes)');
                }
            } catch (e) {
                console.warn('[azul-web] fallback font fetch failed:', e);
            }
        }

        if (typeof azMini.AzStartup_initLayoutCache === 'function') {
            var viewportW = (typeof window !== 'undefined' && window.innerWidth) || 800;
            var viewportH = (typeof window !== 'undefined' && window.innerHeight) || 600;
            var initRc = azMini.AzStartup_initLayoutCache(azState, viewportW, viewportH, 0);
            var domPtr = (typeof azMini.AzStartup_getCurrentDomPtr === 'function')
                ? azMini.AzStartup_getCurrentDomPtr(azState) : 0;
            console.info('[azul-web] initLayoutCache rc=' + initRc +
                         ' current_dom_ptr=' + domPtr);

            // M11 Sprint 1: hydrate the wasm-side StyledDom + run the layout
            // solver. Failures log but don't abort — hit-test falls back to
            // the last registered cb node when the rects cache is empty.
            if (initRc === 0 && domPtr &&
                typeof azMini.AzStartup_hydrateStyledDom === 'function') {
                var hydrateRc = azMini.AzStartup_hydrateStyledDom(azState);
                var hydrated = (typeof azMini.AzStartup_isStyledDomHydrated === 'function')
                    ? azMini.AzStartup_isStyledDomHydrated(azState) : 0;
                var nodeCount = (typeof azMini.AzStartup_getDomNodeCount === 'function')
                    ? azMini.AzStartup_getDomNodeCount(azState) : 0;
                console.info('[azul-web] hydrateStyledDom rc=' + hydrateRc +
                             ' hydrated=' + hydrated +
                             ' node_count=' + nodeCount);
                // Prefer the REAL solver (the one the node e2e harness exercises);
                // fall back to the legacy partial solve.
                var solveFn = azMini.AzStartup_solveLayoutReal || azMini.AzStartup_solveLayout;
                if (hydrateRc === 0 && typeof solveFn === 'function') {
                    var solveRc = solveFn(azState, viewportW, viewportH);
                    var solved = (typeof azMini.AzStartup_isLayoutSolved === 'function')
                        ? azMini.AzStartup_isLayoutSolved(azState) : 0;
                    var rectsLen = (typeof azMini.AzStartup_getPositionedRectsLen === 'function')
                        ? azMini.AzStartup_getPositionedRectsLen(azState) : 0;
                    console.info('[azul-web] solveLayout rc=' + solveRc +
                                 ' solved=' + solved + ' rects_len=' + rectsLen);
                }
            }
        }
    }

    // M9-2 probe hook: expose the layout cb + buildLayoutInfo on the
    // window so /tmp/layout-probe.js can drive an end-to-end test
    // from a Node fetch without bootstrapping the full DOM. Harmless
    // in production (no JS reads `window.__azProbe`).
    if (typeof window !== 'undefined') {
        window.__azProbe = {
            mini: azMini,
            layoutCb: azLayoutCb,
            layoutCbTableIdx: azLayoutCbTableIdx,
            refAnyPtr: azRefAnyPtr,
            modelPtr: azModelPtr,
            state: azState,
            table: azTable,
            memory: azMemory,
        };
    }

    // 5. Wire native event listeners.
    azWireListeners();

    console.info('[azul-web] bootstrap complete');
}

// =====================================================================
// M8.7c-3 hydration. We read the server-embedded az-hydrate block
// for the user's type_id + initial data, then call mini's lifted
// `AzStartup_hydrate(type_id_lo, type_id_hi, data_ptr, data_size)`
// to build the wasm-side RefAny tree. The hydrate fn allocates
// RefCountInner + RefAny via Box::new (routed through __rust_alloc,
// our bump allocator) — so the field offsets / pointer widths
// automatically match what the lifted cb expects.
//
// We only have to hand JS-layout the *user's data* (which is
// type-specific and can't be Rust-side without per-type codegen).
// For hello-world's MyDataModel that's a single u32 counter at
// offset 0; future types might serialize via postcard or json and
// the lifted user `_fromJson` would take over (lifting that adds a
// hidden-return wrapper variant — out of scope today).
// =====================================================================
function azHydrate() {
    var script = document.getElementById('az-hydrate');
    if (!script) {
        console.warn('[azul-web] no #az-hydrate block in HTML — cb path will get a null refany');
        return;
    }
    var payload;
    try {
        payload = JSON.parse(script.textContent);
    } catch (e) {
        console.error('[azul-web] az-hydrate JSON parse failed:', e);
        return;
    }
    var typeIdBigInt = BigInt(payload.type_id);
    var typeIdLo = Number(typeIdBigInt & 0xFFFFFFFFn);
    var typeIdHi = Number((typeIdBigInt >> 32n) & 0xFFFFFFFFn);
    var counter = (typeof payload.json === 'number') ? payload.json : 0;

    // Allocate user-data slot + write counter (hello-world's
    // MyDataModel = { counter: u32 }).
    azModelPtr = azMini.AzStartup_alloc(4);
    if (!azModelPtr) {
        console.error('[azul-web] hydrate alloc(4) failed');
        return;
    }
    new DataView(azMemory.buffer).setUint32(azModelPtr, counter >>> 0, true);

    // Hand to AzStartup_hydrate — the mini-side fn does the
    // RefCountInner + RefAny construction in lifted Rust code, no
    // hand-laid-out JS bytes.
    azRefAnyPtr = azMini.AzStartup_hydrate(typeIdLo, typeIdHi, azModelPtr, 4);
    if (!azRefAnyPtr) {
        console.error('[azul-web] AzStartup_hydrate returned 0');
        return;
    }
    // M9-6: hand model_ptr + display node_idx to mini so the
    // wasm-side AzStartup_dispatchEvent can read the updated
    // counter + emit SetText patches without JS round-trips.
    if (typeof azMini.AzStartup_setRefAny === 'function') {
        azMini.AzStartup_setRefAny(azState, azRefAnyPtr);
    }
    if (typeof azMini.AzStartup_setModelPtr === 'function') {
        azMini.AzStartup_setModelPtr(azState, azModelPtr);
    }
    if (typeof azMini.AzStartup_setDisplayNode === 'function') {
        // node_idx 1 = the counter text node (id="az_1"). Hardcoded
        // for hello-world; M9-3b's wasm-resident StyledDom walk
        // will discover this automatically per route.
        azMini.AzStartup_setDisplayNode(azState, 1);
    }
    console.info('[azul-web] hydrate ok: refany=' + azRefAnyPtr +
                 ' model=' + azModelPtr +
                 ' counter=' + counter + ' type_id=' + payload.type_id);
}

// M9-6: azInvokeCbDirect + azNodeIdxFromEvent regex removed.
//   - Hit-test now happens wasm-side via AzStartup_hitTest, called
//     from AzStartup_dispatchEvent when it sees node_idx=SENTINEL.
//   - Cb dispatch happens wasm-side via __az_call_indirect inside
//     AzStartup_dispatchEvent, using the hydrated refany_ptr.
//   - Patch emission happens wasm-side via AzStartup_buildCounterPatch
//     inside the same call; JS just applies the returned TLV stream.
// The DOM id="az_N" attributes are now decorative (CSS only).

function azModifierBits(e) {
    var bits = 0;
    if (e.shiftKey) bits |= 1;
    if (e.ctrlKey)  bits |= 2;
    if (e.altKey)   bits |= 4;
    if (e.metaKey)  bits |= 8;
    return bits;
}

function azDispatch(kind, domEvent) {
    // (2026-06-10) AzStartup_dispatchEvent does not yet filter by the callback's
    // registered EventFilter (it invokes the hit node's cb for ANY kind), so a single
    // physical click would fire the cb three times (mousedown + mouseup + click).
    // Until per-filter dispatch lands wasm-side, only the gesture-complete events
    // invoke callbacks; the rest return early.
    if (kind === EVT_MOUSEDOWN || kind === EVT_MOUSEUP || kind === EVT_DBLCLICK ||
        kind === EVT_MOUSEMOVE || kind === EVT_FOCUSIN || kind === EVT_FOCUSOUT) {
        return;
    }
    var evtPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var outLenPtr = azMini.AzStartup_alloc(OUT_LEN_SIZE);
    if (!evtPtr || !outLenPtr) {
        console.warn('[azul-web] alloc failed for event dispatch');
        return;
    }

    var view = new DataView(azMemory.buffer);
    // Layout matches event_offset in dll/src/web/eventloop.rs.
    // M9-6: encode SENTINEL_NO_NODE so the wasm-side hit-test runs
    // (no more JS-side `id="az_N"` regex walk).
    // M11 Sprint 2: x/y now encoded as integer pixels (Math.floor)
    // so the wasm-side hitTest can compare directly against the
    // positioned-rect cache (also stored as u32 pixels).
    // f32::from_bits proved unreliable through the remill lift —
    // integer coords sidestep that conversion entirely.
    view.setUint32(evtPtr + 0,  SENTINEL_NO_NODE, true);
    view.setUint32(evtPtr + 4, Math.max(0, Math.floor(domEvent.clientX || 0)), true);
    view.setUint32(evtPtr + 8, Math.max(0, Math.floor(domEvent.clientY || 0)), true);
    view.setUint32(evtPtr + 12, domEvent.button || domEvent.keyCode || 0, true);
    view.setUint32(evtPtr + 16, azModifierBits(domEvent), true);

    var patchesPtr = azMini.AzStartup_dispatchEvent(
        azState, kind, evtPtr, EVENT_BUFFER_SIZE, outLenPtr
    );
    var patchesLen = view.getUint32(outLenPtr, true);
    console.debug('[azul-web] dispatch kind=' + kind +
                  ' → patches_ptr=' + patchesPtr + ' patches_len=' + patchesLen);

    if (patchesPtr && patchesLen) {
        azApplyPatches(patchesPtr, patchesLen);
    }

    azMini.AzStartup_free(evtPtr, EVENT_BUFFER_SIZE);
    azMini.AzStartup_free(outLenPtr, OUT_LEN_SIZE);
}

// TLV patch-stream decoder. M11 Sprint 3 schema:
//   kind:u8 | node_idx:u32 LE | payload_len:u32 LE | payload[payload_len]
//
// Kinds — keep in sync with eventloop.rs PATCH_KIND_* constants:
//   1  SetText           — payload = UTF-8 text bytes
//   2  SetAttr           — payload = name:cstr | value:cstr
//   3  RemoveAttr        — payload = name:cstr
//   4  SetInlineStyle    — payload = css_text bytes
//   5  RemoveNode        — payload empty
//   6  InsertNode        — payload = parent_node_idx:u32 | html_or_blob bytes
//   7  MoveNode          — payload = new_parent_idx:u32 | new_sibling_idx:u32
//   8  ReplaceSubtree    — payload = new_subtree_html bytes
//   9  Focus             — payload empty
//   10 ScrollTo          — payload = x:i32 | y:i32
//   11 AddClass          — payload = class name bytes
//   12 RemoveClass       — payload = class name bytes
function azDecodeCstr(view, payloadOff, payloadEnd) {
    // Read NUL-terminated bytes starting at payloadOff; returns
    // (string, bytes_consumed_incl_NUL). If no NUL found before
    // payloadEnd, returns the rest of the slice.
    var end = payloadOff;
    while (end < payloadEnd && view.getUint8(end) !== 0) end++;
    var bytes = new Uint8Array(azMemory.buffer, payloadOff, end - payloadOff);
    var s = new TextDecoder().decode(bytes);
    return [s, (end < payloadEnd ? (end - payloadOff + 1) : (end - payloadOff))];
}

function azApplyPatches(ptr, len) {
    var view = new DataView(azMemory.buffer);
    var off = 0;
    while (off + 9 <= len) {
        var kind        = view.getUint8(ptr + off + 0);
        var nodeIdx     = view.getUint32(ptr + off + 1, true);
        var payloadLen  = view.getUint32(ptr + off + 5, true);
        var payloadOff  = ptr + off + 9;
        var payloadEnd  = payloadOff + payloadLen;
        switch (kind) {
            case 1: { // SetText
                var bytes = new Uint8Array(azMemory.buffer, payloadOff, payloadLen);
                var text = new TextDecoder().decode(bytes);
                var el = document.getElementById('az_' + nodeIdx);
                if (el) el.textContent = text;
                break;
            }
            case 2: { // SetAttr — name\0value\0
                var pair = azDecodeCstr(view, payloadOff, payloadEnd);
                var name = pair[0];
                var valuePair = azDecodeCstr(view, payloadOff + pair[1], payloadEnd);
                var value = valuePair[0];
                var el2 = document.getElementById('az_' + nodeIdx);
                if (el2) el2.setAttribute(name, value);
                break;
            }
            case 3: { // RemoveAttr — name\0
                var name3 = azDecodeCstr(view, payloadOff, payloadEnd)[0];
                var el3 = document.getElementById('az_' + nodeIdx);
                if (el3) el3.removeAttribute(name3);
                break;
            }
            case 4: { // SetInlineStyle — css_text
                var css = new TextDecoder().decode(
                    new Uint8Array(azMemory.buffer, payloadOff, payloadLen));
                var el4 = document.getElementById('az_' + nodeIdx);
                if (el4) el4.setAttribute('style', css);
                break;
            }
            case 5: { // RemoveNode — payload empty
                var el5 = document.getElementById('az_' + nodeIdx);
                if (el5 && el5.parentNode) el5.parentNode.removeChild(el5);
                break;
            }
            case 6: { // InsertNode — parent_idx:u32 | html bytes
                if (payloadLen < 4) break;
                var parentIdx = view.getUint32(payloadOff, true);
                var htmlBytes = new Uint8Array(azMemory.buffer, payloadOff + 4, payloadLen - 4);
                var html = new TextDecoder().decode(htmlBytes);
                var parent = document.getElementById('az_' + parentIdx);
                if (parent) {
                    var tmpl = document.createElement('template');
                    tmpl.innerHTML = html;
                    if (tmpl.content.firstElementChild) {
                        parent.appendChild(tmpl.content.firstElementChild);
                    }
                }
                break;
            }
            case 9: { // Focus
                var el9 = document.getElementById('az_' + nodeIdx);
                if (el9 && typeof el9.focus === 'function') el9.focus();
                break;
            }
            case 10: { // ScrollTo — x:i32 | y:i32
                if (payloadLen < 8) break;
                var sx = view.getInt32(payloadOff, true);
                var sy = view.getInt32(payloadOff + 4, true);
                var el10 = document.getElementById('az_' + nodeIdx);
                if (el10) el10.scrollTo(sx, sy);
                break;
            }
            case 11: { // AddClass
                var cn = new TextDecoder().decode(
                    new Uint8Array(azMemory.buffer, payloadOff, payloadLen));
                var el11 = document.getElementById('az_' + nodeIdx);
                if (el11) el11.classList.add(cn);
                break;
            }
            case 12: { // RemoveClass
                var cn12 = new TextDecoder().decode(
                    new Uint8Array(azMemory.buffer, payloadOff, payloadLen));
                var el12 = document.getElementById('az_' + nodeIdx);
                if (el12) el12.classList.remove(cn12);
                break;
            }
            default:
                console.debug('[azul-web] unknown patch kind:', kind);
        }
        off += 9 + payloadLen;
    }
}

function azWireListeners() {
    // M11 Sprint 4: wire the event kinds the bench (Sprint 6) needs.
    // Each listener encodes its event into the fixed 256-byte buffer
    // via azDispatch + dispatches to mini.AzStartup_dispatchEvent.
    // The wasm-side dispatcher honors the `kind` arg and routes to
    // the matching cb (input → on_input, keydown → on_keydown, …).
    //
    // Skipping touch/drag/composition/wheel for now — those need
    // variable-width TLV payloads beyond the fixed 256-byte header
    // (the M11 plan's Stage A.6 deferred work).
    document.body.addEventListener('click',     function(e) { azDispatch(EVT_CLICK,     e); });
    document.body.addEventListener('mousedown', function(e) { azDispatch(EVT_MOUSEDOWN, e); });
    document.body.addEventListener('mouseup',   function(e) { azDispatch(EVT_MOUSEUP,   e); });
    document.body.addEventListener('dblclick',  function(e) { azDispatch(EVT_DBLCLICK,  e); });
    document.body.addEventListener('keydown',   function(e) { azDispatch(EVT_KEYDOWN,   e); });
    document.body.addEventListener('keyup',     function(e) { azDispatch(EVT_KEYUP,     e); });
    // Focus events bubble via `focusin`/`focusout` (the bubbling
    // variants — `focus`/`blur` don't bubble to body).
    document.body.addEventListener('focusin',   function(e) { azDispatch(EVT_FOCUSIN,   e); });
    document.body.addEventListener('focusout',  function(e) { azDispatch(EVT_FOCUSOUT,  e); });
    // `input` fires on every <input>/<textarea>/[contenteditable]
    // mutation — bench's row-edit cells need this.
    document.body.addEventListener('input', function(e) {
        // Encode the new text value into the event_bytes scratch
        // region past the fixed header so the cb can read it.
        azDispatchWithText(EVT_KEYDOWN, e, e.target && e.target.value || '');
    });
    // Scroll on window for the page-level cb; scroll on body for
    // overflow containers.
    window.addEventListener('scroll', function(e) {
        azDispatchScroll(EVT_SCROLL, window.scrollX, window.scrollY);
    });
    window.addEventListener('resize', function(e) {
        azDispatchResize(window.innerWidth, window.innerHeight);
    });
}

// M11 Sprint 4 — kind-specific encoders.
//
// All extend the fixed header with payload bytes past offset 20:
//   bytes 20..24 = payload_len (u32 LE)
//   bytes 24..   = payload[payload_len]

function azDispatchWithText(kind, domEvent, text) {
    var evtPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var outLenPtr = azMini.AzStartup_alloc(OUT_LEN_SIZE);
    if (!evtPtr || !outLenPtr) return;
    var view = new DataView(azMemory.buffer);
    view.setUint32(evtPtr + 0,  SENTINEL_NO_NODE, true);
    view.setUint32(evtPtr + 4,  Math.max(0, Math.floor(domEvent.clientX || 0)), true);
    view.setUint32(evtPtr + 8,  Math.max(0, Math.floor(domEvent.clientY || 0)), true);
    view.setUint32(evtPtr + 12, domEvent.keyCode || 0, true);
    view.setUint32(evtPtr + 16, azModifierBits(domEvent), true);
    // Pack text bytes at offset 20+ (u32 length followed by UTF-8).
    var bytes = new TextEncoder().encode(text);
    var maxText = EVENT_BUFFER_SIZE - 24;
    var n = Math.min(bytes.length, maxText);
    view.setUint32(evtPtr + 20, n, true);
    new Uint8Array(azMemory.buffer, evtPtr + 24, n).set(bytes.subarray(0, n));

    var patchesPtr = azMini.AzStartup_dispatchEvent(
        azState, kind, evtPtr, EVENT_BUFFER_SIZE, outLenPtr,
    );
    var patchesLen = view.getUint32(outLenPtr, true);
    if (patchesPtr && patchesLen) azApplyPatches(patchesPtr, patchesLen);
    azMini.AzStartup_free(evtPtr, EVENT_BUFFER_SIZE);
    azMini.AzStartup_free(outLenPtr, OUT_LEN_SIZE);
}

function azDispatchScroll(kind, scrollX, scrollY) {
    var evtPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var outLenPtr = azMini.AzStartup_alloc(OUT_LEN_SIZE);
    if (!evtPtr || !outLenPtr) return;
    var view = new DataView(azMemory.buffer);
    view.setUint32(evtPtr + 0,  SENTINEL_NO_NODE, true);
    view.setUint32(evtPtr + 4,  Math.max(0, Math.floor(scrollX)), true);
    view.setUint32(evtPtr + 8,  Math.max(0, Math.floor(scrollY)), true);
    view.setUint32(evtPtr + 12, 0, true);
    view.setUint32(evtPtr + 16, 0, true);
    view.setUint32(evtPtr + 20, 0, true);
    var patchesPtr = azMini.AzStartup_dispatchEvent(
        azState, kind, evtPtr, EVENT_BUFFER_SIZE, outLenPtr,
    );
    var patchesLen = view.getUint32(outLenPtr, true);
    if (patchesPtr && patchesLen) azApplyPatches(patchesPtr, patchesLen);
    azMini.AzStartup_free(evtPtr, EVENT_BUFFER_SIZE);
    azMini.AzStartup_free(outLenPtr, OUT_LEN_SIZE);
}

function azDispatchResize(w, h) {
    var evtPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var outLenPtr = azMini.AzStartup_alloc(OUT_LEN_SIZE);
    if (!evtPtr || !outLenPtr) return;
    var view = new DataView(azMemory.buffer);
    view.setUint32(evtPtr + 0,  SENTINEL_NO_NODE, true);
    view.setUint32(evtPtr + 4,  Math.max(0, Math.floor(w)), true);
    view.setUint32(evtPtr + 8,  Math.max(0, Math.floor(h)), true);
    view.setUint32(evtPtr + 12, 0, true);
    view.setUint32(evtPtr + 16, 0, true);
    var patchesPtr = azMini.AzStartup_dispatchEvent(
        azState, EVT_RESIZE, evtPtr, EVENT_BUFFER_SIZE, outLenPtr,
    );
    var patchesLen = view.getUint32(outLenPtr, true);
    if (patchesPtr && patchesLen) azApplyPatches(patchesPtr, patchesLen);
    azMini.AzStartup_free(evtPtr, EVENT_BUFFER_SIZE);
    azMini.AzStartup_free(outLenPtr, OUT_LEN_SIZE);
    // Re-run layout against the new viewport.
    if (typeof azMini.AzStartup_solveLayout === 'function') {
        azMini.AzStartup_solveLayout(azState, Math.floor(w), Math.floor(h));
    }
}

// =====================================================================
// Internal-link navigation (SPA-style, unchanged from M4).
// =====================================================================
function azNavigate(path) {
    fetch(path)
    .then(function(r) { return r.text(); })
    .then(function(html) {
        if (html) {
            history.pushState(null, '', path);
            document.open();
            document.write(html);
            document.close();
        }
    })
    .catch(function(err) { console.error('[azul-web] navigation error:', err); });
}

document.querySelectorAll('a[href^="/"]').forEach(function(el) {
    el.addEventListener('click', function(e) {
        var href = el.getAttribute('href');
        if (!href || href.startsWith('/az/')) return;
        e.preventDefault();
        azNavigate(href);
    });
});

window.addEventListener('popstate', function() {
    fetch(location.pathname)
    .then(function(r) { return r.text(); })
    .then(function(html) {
        if (html) {
            document.open();
            document.write(html);
            document.close();
        }
    });
});

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', azBootstrap);
} else {
    azBootstrap();
}
})();
"#.to_string()
}

/// Content hash for the loader JS (for cache-busting).
pub fn loader_js_hash(content: &str) -> String {
    super::fnv1a64_hex(content.as_bytes())
}
