//! Generate the azul-loader.js bootstrap script.
//!
//! This is the ONLY JavaScript in the system. It:
//! 1. Instantiates azul-mini.wasm (when available)
//! 2. Loads callback .wasm modules
//! 3. Registers global event listeners (mousemove, mousedown, keydown, etc.)
//! 4. Hydrates the server-rendered HTML into the WASM layout tree
//!
//! Phase 0: The loader simply sets up server-side callback execution
//! via fetch() POST requests, since no WASM is available yet.

use super::cb_gen::CallbackWasm;

/// Generate the loader JavaScript for the current phase.
///
/// Phase 0 (stub transpiler): The loader sends callback invocations
/// to the server via POST and replaces the page content with the response.
///
/// Phase 1+ (real transpiler): The loader instantiates WASM modules
/// and runs callbacks client-side.
pub fn generate_loader_js(
    mini_wasm_hash: &str,
    callbacks: &[CallbackWasm],
) -> String {
    // Phase 0: server-side execution fallback
    generate_phase0_loader()
}

/// Phase 0 loader: all interaction goes through the server.
///
/// Each element with an `data-az-cb` attribute gets a click handler
/// that POSTs to the server, which runs the callback natively and
/// returns updated HTML.
fn generate_phase0_loader() -> String {
    r#"(function(){
'use strict';

// Phase 0: Server-side callback execution
// All callbacks POST to the server, which runs them natively
// and returns the updated HTML for the page body.

function azInit() {
    document.querySelectorAll('[data-az-cb]').forEach(function(el) {
        var cbId = el.getAttribute('data-az-cb');
        var evType = el.getAttribute('data-az-ev') || 'click';
        el.addEventListener(evType, function(e) {
            e.preventDefault();
            fetch('/az/exec/' + cbId, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    x: e.clientX || 0,
                    y: e.clientY || 0,
                    button: e.button || 0,
                    key: e.key || '',
                })
            })
            .then(function(r) { return r.text(); })
            .then(function(html) {
                if (html) {
                    var body = document.getElementById('az-body');
                    if (body) { body.innerHTML = html; azInit(); }
                }
            })
            .catch(function(err) {
                console.error('[azul-web] callback error:', err);
            });
        });
    });
}

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
    // Simple hash: first 8 hex chars of a basic hash
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    format!("{:016x}", hash)
}
