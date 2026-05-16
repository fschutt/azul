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

function azLoadWasm(url) {
    var existing = azWasmCache.get(url);
    if (existing) return existing;
    var p = WebAssembly.instantiateStreaming(fetch(url), {})
        .then(function(res) { return res.instance; })
        .catch(function(err) {
            console.warn('[azul-web] failed to load ' + url + ':', err);
            return null; // null-instance signals "skip dispatch"
        });
    azWasmCache.set(url, p);
    return p;
}

function azDispatch(instance, evt, cbId, url) {
    if (!instance) return;
    var fn = instance.exports && instance.exports.callback;
    if (typeof fn !== 'function') {
        console.warn('[azul-web] ' + url + ' has no `callback` export');
        return;
    }
    try {
        // (0, 0) placeholder args. M7+ will marshal the real
        // RefAny + CallbackInfo pointers. Return value (Update tag)
        // is ignored until M9 wires the client-side DOM patcher.
        var ret = fn(0, 0);
        if (ret !== 0) {
            // Non-DoNothing return surfaces in console so M5-M8
            // debugging can confirm the lifted body actually ran.
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
