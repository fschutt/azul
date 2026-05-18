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

// node_idx → table_idx (M8.6 stub: identity since dispatchEvent uses
// node_idx as the fn-addr-lookup key. M8.5c+ will swap to real
// fn-addrs harvested from a hydrated StyledDom.)
var azFnAddrToTableIdx = new Map();

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
    };
    var handler = {
        get: function(_target, prop) {
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
// M8.7c-3 hydration state — wasm-side AzRefAny constructed at boot.
// =====================================================================
var azRefAnyPtr = 0;    // wasm offset of the 24B AzRefAny aggregate
var azModelPtr  = 0;    // wasm offset of the user-data struct
                          // (hello-world: 4B holding the u32 counter)
// Per-node callback fns, keyed by node_idx — populated when each
// per-cb wasm is instantiated. The direct-invoke click path looks
// these up + calls them with (azRefAnyPtr, 0, info_ptr).
var azNodeCbFns = new Map();

// M9-2: Layout-cb instance + reserved table slot. The layout cb's
// `callback` export has the M9-1 wrapper shape
// `(refany_lo: i64, refany_hi: i64, info_ptr: i32, out_ptr: i32) -> i32`
// — last arg is the caller-allocated destination buffer for the
// returned AzDom (X8 hidden-ptr return). M9-3 wires the actual
// invocation via __az_call_indirect inside AzStartup_initLayoutCache;
// for M9-2 we just instantiate + reserve the table slot.
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
    //    __az_resolve_callback.
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

    // 2. Initialize App. AzStartup_init returns the eventloop state ptr.
    //    M8.7c-3 ignores the JSON payload args (the AArch64-layout
    //    RefAny is constructed entirely on the JS side below — see
    //    azHydrate); a future pass will run the user's lifted
    //    fromJson via __az_call_indirect instead of building the
    //    aggregate by hand.
    azState = azMini.AzStartup_init(0, 0);
    if (!azState) {
        console.error('[azul-web] AzStartup_init returned 0');
        return;
    }
    console.debug('[azul-web] AzStartup_init → state ptr', azState);

    // 3. Hydrate the wasm-side RefAny from the server-embedded
    //    az-hydrate block.
    azHydrate();

    // 4. Discover + instantiate per-callback WASMs. Each gets put at
    //    table[node_idx] AND recorded in azNodeCbFns so the
    //    direct-invoke click handler below can call them without
    //    going through AzStartup_dispatchEvent.
    var cbs = document.querySelectorAll('[data-az-cb][data-az-wasm]');
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
            azNodeCbFns.set(nodeIdx, cbFn);
            // M9-4: tell mini about this cb-bearing node so the
            // wasm-side AzStartup_hitTest can return it without a
            // JS-side DOM-id-regex round-trip.
            if (typeof azMini.AzStartup_registerCbNode === 'function') {
                azMini.AzStartup_registerCbNode(azState, nodeIdx);
            }
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

                // M9-3: hand the table_idx + refany off to mini, then
                // drive the first layout pass. The viewport size is
                // window.innerWidth/Height for now; M9-5 will reflow
                // on resize events.
                if (typeof azMini.AzStartup_setLayoutCbTableIdx === 'function') {
                    azMini.AzStartup_setLayoutCbTableIdx(azState, azLayoutCbTableIdx);
                }
                if (typeof azMini.AzStartup_setRefAny === 'function' && azRefAnyPtr) {
                    azMini.AzStartup_setRefAny(azState, azRefAnyPtr);
                }
                if (typeof azMini.AzStartup_initLayoutCache === 'function') {
                    var viewportW = (typeof window !== 'undefined' && window.innerWidth) || 800;
                    var viewportH = (typeof window !== 'undefined' && window.innerHeight) || 600;
                    var initRc = azMini.AzStartup_initLayoutCache(azState, viewportW, viewportH, 0);
                    var domPtr = (typeof azMini.AzStartup_getCurrentDomPtr === 'function')
                        ? azMini.AzStartup_getCurrentDomPtr(azState) : 0;
                    console.info('[azul-web] initLayoutCache rc=' + initRc +
                                 ' current_dom_ptr=' + domPtr);
                }
            }
        } catch (e) {
            console.warn('[azul-web] failed to instantiate ' + layoutUrl + ':', e);
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
    console.info('[azul-web] hydrate ok: refany=' + azRefAnyPtr +
                 ' model=' + azModelPtr +
                 ' counter=' + counter + ' type_id=' + payload.type_id);
}

// =====================================================================
// Direct cb invocation (skips AzStartup_dispatchEvent).
//
// The dispatch chain (mini's dispatchEvent → __az_call_indirect
// → cb) currently hardcodes FAKE_REFANY (0x101). To exercise the
// hydrated wasm-side RefAny we bypass it: on click, look up the
// per-node cb fn we registered at bootstrap and call it directly
// with (azRefAnyPtr, 0n, info_ptr).
//
// The wrapper expects two i64s (the refany aggregate halves) — we
// pass refany_ptr in the low one and 0 in the high one because the
// lifted code treats X0 as a 64-bit pointer to the 24B AzRefAny
// (the canonical_callback sig's GprI64Pair predates the realization
// that >16B aggregates are passed by hidden pointer in arm64 PCS).
// Reading X0 as a 64-bit ptr in wasm32 land just truncates to the
// low 32 — exactly the wasm offset we passed.
// =====================================================================
function azInvokeCbDirect(nodeIdx, domEvent) {
    var cbFn = azNodeCbFns.get(nodeIdx);
    if (!cbFn) return;
    if (!azRefAnyPtr) {
        console.warn('[azul-web] cb node=' + nodeIdx + ' invoked but refany not hydrated');
        return;
    }
    var infoPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var update = 0;
    try {
        update = cbFn(BigInt(azRefAnyPtr), 0n, infoPtr);
    } catch (e) {
        console.warn('[azul-web] cb trapped:', e.message);
    } finally {
        azMini.AzStartup_free(infoPtr, EVENT_BUFFER_SIZE);
    }
    // Update enum: 0 = DoNothing, 1 = RefreshDom (… see eventloop.rs).
    // M9-5: on RefreshDom, ask wasm to encode a SetText TLV patch
    // with the new counter value, then apply it via the existing
    // azApplyPatches decoder. Replaces the previous hardcoded
    // `el.textContent = newCounter.toString()` path.
    if (update >= 1 && azModelPtr) {
        var view = new DataView(azMemory.buffer);
        var newCounter = view.getUint32(azModelPtr, true);
        if (typeof azMini.AzStartup_buildCounterPatch === 'function') {
            var patchCap = 32;
            var patchBuf = azMini.AzStartup_alloc(patchCap);
            // node_idx 1 = the counter text node (id="az_1"); the
            // server-side discovery layer assigns it. M9-3b's
            // wasm-resident StyledDom will walk + locate this for
            // arbitrary DOMs.
            var used = azMini.AzStartup_buildCounterPatch(
                patchBuf, patchCap, 1, newCounter,
            );
            if (used > 0) {
                azApplyPatches(patchBuf, used);
            }
            azMini.AzStartup_free(patchBuf, patchCap);
        } else {
            // Legacy fallback (M9-5 drops this once mini.wasm always
            // exports buildCounterPatch).
            var el = document.getElementById('az_1');
            if (el) el.textContent = newCounter.toString();
        }
        console.info('[azul-web] cb node=' + nodeIdx +
                     ' → Update=' + update + ' counter=' + newCounter);
    } else {
        console.info('[azul-web] cb node=' + nodeIdx + ' → Update=' + update);
    }
}

// M9-4: prefer the wasm-side AzStartup_hitTest when available; fall
// back to the legacy DOM-id-regex walk only when mini's hitTest
// export isn't present (e.g. older mini.wasm without the M9-4 lift).
// The fallback path will be dropped entirely in M9-6 once the wasm
// side is the source of truth.
function azNodeIdxFromEvent(domEvent) {
    if (azMini && typeof azMini.AzStartup_hitTest === 'function' && azState) {
        var x = domEvent.clientX || 0;
        var y = domEvent.clientY || 0;
        // Pass coordinates as f32 bits so the JS-side u32 signature
        // stays clean. Float32Array helps us reinterpret the float
        // as its bit pattern.
        var f32 = new Float32Array(2);
        var u32 = new Uint32Array(f32.buffer);
        f32[0] = x;
        f32[1] = y;
        var nodeIdx = azMini.AzStartup_hitTest(azState, u32[0], u32[1]);
        if (nodeIdx !== 0xFFFFFFFF) {
            return nodeIdx;
        }
    }
    // Legacy fallback (M9-6 deletes this).
    var target = domEvent.target;
    while (target && target !== document.body) {
        if (target.id) {
            var m = target.id.match(/^az_(\d+)$/);
            if (m) return parseInt(m[1], 10);
        }
        target = target.parentNode;
    }
    return SENTINEL_NO_NODE;
}

function azModifierBits(e) {
    var bits = 0;
    if (e.shiftKey) bits |= 1;
    if (e.ctrlKey)  bits |= 2;
    if (e.altKey)   bits |= 4;
    if (e.metaKey)  bits |= 8;
    return bits;
}

function azDispatch(kind, domEvent) {
    var nodeIdx = azNodeIdxFromEvent(domEvent);
    if (nodeIdx === SENTINEL_NO_NODE) return;

    var evtPtr = azMini.AzStartup_alloc(EVENT_BUFFER_SIZE);
    var outLenPtr = azMini.AzStartup_alloc(OUT_LEN_SIZE);
    if (!evtPtr || !outLenPtr) {
        console.warn('[azul-web] alloc failed for event dispatch');
        return;
    }

    var view = new DataView(azMemory.buffer);
    // Layout matches event_offset in dll/src/web/eventloop.rs.
    view.setUint32(evtPtr + 0,  nodeIdx, true);
    view.setFloat32(evtPtr + 4, domEvent.clientX || 0, true);
    view.setFloat32(evtPtr + 8, domEvent.clientY || 0, true);
    view.setUint32(evtPtr + 12, domEvent.button || domEvent.keyCode || 0, true);
    view.setUint32(evtPtr + 16, azModifierBits(domEvent), true);

    var patchesPtr = azMini.AzStartup_dispatchEvent(
        azState, kind, evtPtr, EVENT_BUFFER_SIZE, outLenPtr
    );
    var patchesLen = view.getUint32(outLenPtr, true);
    console.debug('[azul-web] dispatch kind=' + kind + ' node=' + nodeIdx +
                  ' → patches_ptr=' + patchesPtr + ' patches_len=' + patchesLen);

    if (patchesPtr && patchesLen) {
        azApplyPatches(patchesPtr, patchesLen);
    }

    azMini.AzStartup_free(evtPtr, EVENT_BUFFER_SIZE);
    azMini.AzStartup_free(outLenPtr, OUT_LEN_SIZE);
}

// TLV patch-stream decoder. Layout per dll/src/web/eventloop.rs
// AzStartup_getPatches doc (M8.5d will populate this):
//   kind:u8 | node_idx:u32 | payload_len:u32 | payload:[u8; payload_len]
//
// M8.6 stub: dispatchEvent currently returns 0 (no patches). Once
// M8.5d wires real patches, this decoder applies them to the DOM.
function azApplyPatches(ptr, len) {
    var view = new DataView(azMemory.buffer);
    var off = 0;
    while (off + 9 <= len) {
        var kind        = view.getUint8(ptr + off + 0);
        var nodeIdx     = view.getUint32(ptr + off + 1, true);
        var payloadLen  = view.getUint32(ptr + off + 5, true);
        var payloadOff  = ptr + off + 9;
        switch (kind) {
            case 1: { // SetText
                var bytes = new Uint8Array(azMemory.buffer, payloadOff, payloadLen);
                var text = new TextDecoder().decode(bytes);
                var el = document.getElementById('az_' + nodeIdx);
                if (el) el.textContent = text;
                break;
            }
            default:
                console.debug('[azul-web] unknown patch kind:', kind);
        }
        off += 9 + payloadLen;
    }
}

function azWireListeners() {
    // M8.7c-3: click-only direct invocation. We bypass mini's
    // AzStartup_dispatchEvent (which still uses FAKE_REFANY) and
    // call the per-node cb fn directly with the hydrated
    // azRefAnyPtr. Other event kinds (mousedown, key, etc.) wait
    // on M8.5d's full dispatch loop.
    document.body.addEventListener('click', function(e) {
        var nodeIdx = azNodeIdxFromEvent(e);
        if (nodeIdx === SENTINEL_NO_NODE) return;
        azInvokeCbDirect(nodeIdx, e);
    });
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
