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
// =====================================================================
function azMakeMiniImports() {
    azTable = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    return {
        env: {
            __indirect_function_table: azTable,
            __az_resolve_callback: function(fnAddr) {
                // BigInt → Number (safe; node_idx fits in u32).
                var n = Number(fnAddr);
                var idx = azFnAddrToTableIdx.get(n);
                return idx === undefined ? SENTINEL_NO_NODE : idx;
            },
        },
    };
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
            console.debug('[azul-web] cb node=' + nodeIdx + ' loaded from ' + url +
                          ' → table[' + nodeIdx + ']');
        } catch (e) {
            console.warn('[azul-web] failed to instantiate ' + url + ':', e);
        }
    }

    // 5. Wire native event listeners.
    azWireListeners();

    console.info('[azul-web] bootstrap complete');
}

// =====================================================================
// M8.7c-3 hydration: construct an AArch64-layout AzRefAny tree in
// wasm memory matching what the lifted cb expects to dereference.
//
// Layout (all sizes in bytes, all pointers 8B even in wasm32 — the
// lifted code was originally arm64 so it does 64-bit loads on the
// aggregates):
//
//   MyDataModel  @ azModelPtr    : { counter: u32 }            → 4B
//   RefCountInner @ innerPtr     : 112B with type_id at +56
//   AzRefAny @ azRefAnyPtr       : { sharing_info: RefCount,
//                                    instance_id: u64 }        → 24B
//   RefCount   = { ptr: u64, run_destructor: u64 (1B padded) } → 16B
//
// The 64-bit pointer fields are written with `setBigUint64` so the
// lifted `ldr x9, [x0]` reads a 64-bit value where the low 32 bits
// are the wasm offset and the high 32 bits are zero — wasm linear
// memory addressing ignores the high bits.
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
    var counter = (typeof payload.json === 'number') ? payload.json : 0;

    // Allocate the 3 blocks via the mini's bump allocator.
    azModelPtr   = azMini.AzStartup_alloc(4);
    var innerPtr = azMini.AzStartup_alloc(112);
    azRefAnyPtr  = azMini.AzStartup_alloc(24);
    if (!azModelPtr || !innerPtr || !azRefAnyPtr) {
        console.error('[azul-web] hydrate alloc failed', azModelPtr, innerPtr, azRefAnyPtr);
        return;
    }

    var view = new DataView(azMemory.buffer);

    // MyDataModel: just the counter.
    view.setUint32(azModelPtr, counter >>> 0, true);

    // RefCountInner — fields at offsets matching the desktop
    // `#[repr(C)]` layout (8B pointers, 8B usize on arm64).
    view.setBigUint64(innerPtr +   0, BigInt(azModelPtr), true);  // _internal_ptr
    view.setBigUint64(innerPtr +   8, 1n, true);                    // num_copies
    view.setBigUint64(innerPtr +  16, 0n, true);                    // num_refs
    view.setBigUint64(innerPtr +  24, 0n, true);                    // num_mutable_refs
    view.setBigUint64(innerPtr +  32, 4n, true);                    // _internal_len
    view.setBigUint64(innerPtr +  40, 4n, true);                    // _internal_layout_size
    view.setBigUint64(innerPtr +  48, 4n, true);                    // _internal_layout_align
    view.setBigUint64(innerPtr +  56, typeIdBigInt, true);          // type_id ★
    // type_name (AzString, 24B), custom_destructor (8B), serialize_fn
    // (8B), deserialize_fn (8B) — all zero is fine for the cb's
    // downcast path (is_type only reads type_id).
    view.setBigUint64(innerPtr +  64, 0n, true);
    view.setBigUint64(innerPtr +  72, 0n, true);
    view.setBigUint64(innerPtr +  80, 0n, true);
    view.setBigUint64(innerPtr +  88, 0n, true);
    view.setBigUint64(innerPtr +  96, 0n, true);
    view.setBigUint64(innerPtr + 104, 0n, true);

    // AzRefAny: { sharing_info: RefCount { ptr: u64, run_destructor: bool },
    //             instance_id: u64 }
    view.setBigUint64(azRefAnyPtr +  0, BigInt(innerPtr), true);  // sharing_info.ptr
    view.setBigUint64(azRefAnyPtr +  8, 0n, true);                  // sharing_info.run_destructor
    view.setBigUint64(azRefAnyPtr + 16, 0n, true);                  // instance_id

    console.info('[azul-web] hydrate ok: refany=' + azRefAnyPtr +
                 ' inner=' + innerPtr + ' model=' + azModelPtr +
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
    // For RefreshDom, read the counter back from wasm memory + apply
    // a SetText patch to the matching DOM node. The counter node id
    // is currently hardcoded to az_1 (the only text-containing node
    // in hello-world); a proper diff loop is M8.5d.
    if (update >= 1 && azModelPtr) {
        var view = new DataView(azMemory.buffer);
        var newCounter = view.getUint32(azModelPtr, true);
        var el = document.getElementById('az_1');
        if (el) el.textContent = newCounter.toString();
        console.info('[azul-web] cb node=' + nodeIdx +
                     ' → Update=' + update + ' counter=' + newCounter);
    } else {
        console.info('[azul-web] cb node=' + nodeIdx + ' → Update=' + update);
    }
}

function azNodeIdxFromEvent(domEvent) {
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
