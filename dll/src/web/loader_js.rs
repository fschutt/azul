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

    // 2. Initialize App. (No JSON payload in M8.6; M8.7 wires this.)
    azState = azMini.AzStartup_init(0, 0);
    if (!azState) {
        console.error('[azul-web] AzStartup_init returned 0');
        return;
    }
    console.debug('[azul-web] AzStartup_init → state ptr', azState);

    // 3. Discover + instantiate per-callback WASMs. Each gets put at
    //    table[node_idx] (M8.6 stub: table-index == node_idx). The
    //    node_idx → table_idx map drives __az_resolve_callback.
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
            // Grow the table if needed.
            while (azTable.length <= nodeIdx) {
                azTable.grow(16);
            }
            azTable.set(nodeIdx, cbFn);
            azFnAddrToTableIdx.set(nodeIdx, nodeIdx);
            console.debug('[azul-web] cb node=' + nodeIdx + ' loaded from ' + url +
                          ' → table[' + nodeIdx + ']');
        } catch (e) {
            console.warn('[azul-web] failed to instantiate ' + url + ':', e);
        }
    }

    // 4. Wire native event listeners on the document root. WASM does
    //    not yet hit-test (M8.5c); JS extracts node_idx from
    //    event.target.id (="az_N").
    azWireListeners();

    console.info('[azul-web] bootstrap complete');
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
    var root = document.body;
    root.addEventListener('click',     function(e) { azDispatch(EVT_CLICK, e); });
    root.addEventListener('mousedown', function(e) { azDispatch(EVT_MOUSEDOWN, e); });
    root.addEventListener('mouseup',   function(e) { azDispatch(EVT_MOUSEUP, e); });
    root.addEventListener('dblclick',  function(e) { azDispatch(EVT_DBLCLICK, e); });
    root.addEventListener('wheel',     function(e) { azDispatch(EVT_WHEEL, e); });
    document.addEventListener('keydown', function(e) { azDispatch(EVT_KEYDOWN, e); });
    document.addEventListener('keyup',   function(e) { azDispatch(EVT_KEYUP, e); });
    root.addEventListener('focusin',   function(e) { azDispatch(EVT_FOCUSIN, e); });
    root.addEventListener('focusout',  function(e) { azDispatch(EVT_FOCUSOUT, e); });
    window.addEventListener('resize',  function(e) { azDispatch(EVT_RESIZE, e); });
    window.addEventListener('scroll',  function(e) { azDispatch(EVT_SCROLL, e); });
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
