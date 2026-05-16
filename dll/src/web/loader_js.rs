//! Generate the azul-loader.js bootstrap script.
//!
//! M4 (default): client-side WASM dispatch. Each `data-az-cb` element
//! also carries `data-az-wasm="/az/cb/<sym>.<hash>.wasm"`; loader.js
//! fetches + instantiates the module on DOMContentLoaded and calls
//! `instance.exports.callback(0, 0)` on the listed event. Per the
//! user direction `default = client-side; no server fallback in
//! loader.js by default`, the loader does NOT fall back to
//! `POST /az/exec/` when a callback fails to load or runs as a no-op;
//! it logs to console and stays silent. The server-side path is
//! preserved (server.rs still serves it) for opt-in debugging.
//!
//! Routing (SPA nav via `<a>` interception, popstate handling) is
//! unchanged — those don't go through the callback dispatch path.

use super::CallbackWasm;

/// Generate the loader JavaScript for the current phase.
///
/// Args are kept for forward compatibility (M5+ will inline a
/// per-callback metadata table for richer features like signature
/// info, lift-success flags, etc.), but the M4 loader pulls the WASM
/// URL from each element's `data-az-wasm` attribute and doesn't need
/// the global list.
pub fn generate_loader_js(
    _mini_wasm_hash: &str,
    _callbacks: &[CallbackWasm],
) -> String {
    generate_m4_loader()
}

/// M4 loader: client-side WASM dispatch, no server fallback.
fn generate_m4_loader() -> String {
    r#"(function(){
'use strict';

// M4: client-side WASM dispatch (no server fallback by default).
// Each interactive element carries:
//   data-az-cb    — synthetic node ID (debug/diagnostic only here)
//   data-az-ev    — DOM event name to bind under
//   data-az-wasm  — URL of the per-callback WASM module
//
// On DOMContentLoaded, every distinct `data-az-wasm` URL is fetched
// + instantiated via WebAssembly.instantiateStreaming, then cached
// in a Map keyed by URL. On the listed event, we call
// `instance.exports.callback(0, 0)`. The return value is currently
// ignored (placeholder until M7 marshals real args + return → DOM
// patches). A missing/failed instance logs to console and silently
// no-ops — we don't fall back to `POST /az/exec/` per user direction.

var azWasmCache = new Map(); // URL -> Promise<WebAssembly.Instance>

// M5: import stubs for remill-lifted modules. The lifted IR contains
// declared-but-undefined symbols that wasm-ld passes through as
// imports (via `--allow-undefined`):
//
//   - `__remill_*` intrinsics — memory/function/atomic ops the lift
//     models opaquely.
//   - `sub_<hex>` — branch destinations to addresses outside the
//     byte map we handed to remill. These are real call targets in
//     the host binary that the lift saw but didn't follow.
//
// We satisfy every import with a JS no-op. The lifted callback
// can't compute anything meaningful (every memory access returns
// 0, every external call is dropped), but it WILL instantiate and
// run — that's the M5 validation goal. M6's IR-passes (intrinsic
// lowering + signature rewrite) and M7's symbol-intercept pass
// give bodies to these on the Rust side; the imports disappear
// from the lifted module's import list and the callback computes
// real results.
//
// A Proxy backs the import object so any name the lifter produces
// resolves to a noop — beats enumerating every `__remill_*`
// variant manually and handles the variable `sub_<hex>` set.
function azRemillImports() {
    var i64_noop = function() { return 0n; };
    var i32_noop = function() { return 0; };
    var void_noop = function() { /* no return */ };
    // Per-name policy: read_memory_64 / fetch_and_add_64 / similar
    // return i64; write_memory_* and barrier_* return void;
    // everything else returns i32. Pattern-match the import name
    // to pick the right return type so wasm validation passes.
    function stubFor(name) {
        if (name.indexOf('write_memory') !== -1 ||
            name.indexOf('barrier') !== -1) return void_noop;
        if (/_64\b/.test(name)) return i64_noop;
        return i32_noop;
    }
    var handler = {
        get: function(_target, prop) {
            if (typeof prop !== 'string') return undefined;
            return stubFor(prop);
        },
        has: function() { return true; },
    };
    var proxy = new Proxy({}, handler);
    return { env: proxy, remill: proxy };
}

function azLoadWasm(url) {
    var existing = azWasmCache.get(url);
    if (existing) return existing;
    var imports = azRemillImports();
    var p = WebAssembly.instantiateStreaming(fetch(url), imports)
        .then(function(res) { return res.instance; })
        .catch(function(err) {
            console.warn('[azul-web] failed to load ' + url + ':', err);
            return null; // null-instance signals "skip dispatch"
        });
    azWasmCache.set(url, p);
    return p;
}

// Pick the callback entry point from the instance's exports.
// Hand-rolled M3 modules use `callback`. M5+ lifted modules use
// `sub_<hex>` (remill's auto-generated name) — pick the first
// function-typed export when `callback` isn't present.
function azFindCallbackExport(exports) {
    if (typeof exports.callback === 'function') return exports.callback;
    for (var k in exports) {
        if (typeof exports[k] === 'function') return exports[k];
    }
    return null;
}

function azDispatch(instance, evt, cbId, url) {
    if (!instance) return;
    var fn = azFindCallbackExport(instance.exports);
    if (!fn) {
        console.warn('[azul-web] ' + url + ' has no callable export');
        return;
    }
    try {
        // M3 no-op: `(i32, i32) -> i32`; called with `(0, 0)`.
        // M5 lifted: `(ptr, i64, ptr) -> ptr` per remill convention.
        //            JS numeric args are coerced to the expected
        //            wasm types; `0n` for the i64. Extra args are
        //            ignored by WASM if the fn takes fewer.
        // Either way the return is opaque and ignored until M9
        // wires the client-side DOM patcher.
        var ret = fn(0, 0n, 0);
        if (ret !== 0 && ret !== 0n) {
            console.debug('[azul-web] cb=' + cbId + ' returned ' + ret);
        }
    } catch (e) {
        console.warn('[azul-web] cb=' + cbId + ' threw:', e);
    }
}

function azInit() {
    // Preload every distinct callback WASM up front. The browser
    // already started the fetches via `<link rel="preload">` in the
    // HTML head; instantiateStreaming reuses those connections.
    var urls = new Set();
    document.querySelectorAll('[data-az-wasm]').forEach(function(el) {
        var url = el.getAttribute('data-az-wasm');
        if (url) urls.add(url);
    });
    urls.forEach(function(u) { azLoadWasm(u); });

    // Bind per-element listeners.
    document.querySelectorAll('[data-az-cb]').forEach(function(el) {
        var cbId = el.getAttribute('data-az-cb');
        var evType = el.getAttribute('data-az-ev') || 'click';
        var url = el.getAttribute('data-az-wasm');
        el.addEventListener(evType, function(e) {
            e.preventDefault();
            if (!url) return; // no WASM bound → silent no-op
            azLoadWasm(url).then(function(inst) {
                azDispatch(inst, e, cbId, url);
            });
        });
    });

    // Intercept internal link clicks for SPA-style navigation.
    document.querySelectorAll('a[href^="/"]').forEach(function(el) {
        el.addEventListener('click', function(e) {
            var href = el.getAttribute('href');
            if (!href || href.startsWith('/az/')) return;
            e.preventDefault();
            azNavigate(href);
        });
    });
}

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
    .catch(function(err) {
        console.error('[azul-web] navigation error:', err);
    });
}

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
    document.addEventListener('DOMContentLoaded', azInit);
} else {
    azInit();
}
})();
"#.to_string()
}

/// Content hash for the loader JS (for cache-busting).
pub fn loader_js_hash(content: &str) -> String {
    super::fnv1a64_hex(content.as_bytes())
}
