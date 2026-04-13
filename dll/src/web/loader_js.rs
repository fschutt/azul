//! Generate the azul-loader.js bootstrap script.
//!
//! Phase 0 = no WASM transpilation; all callbacks run server-side.
//! Future phases will move callbacks client-side via WASM.
//!
//! Phase 0: Server-side callback execution via fetch() POST.
//! Includes routing support: intercepts `<a>` clicks with `data-az-route`
//! and handles browser back/forward via `popstate`.

/// Generate the loader JavaScript for the current phase.
pub fn generate_loader_js() -> String {
    generate_phase0_loader()
}

/// Phase 0 loader: all interaction goes through the server.
///
/// Features:
/// - Callback execution via POST /az/exec/{node_id}
/// - Route navigation via GET /{path} (intercepts link clicks)
/// - Browser back/forward support via popstate
fn generate_phase0_loader() -> String {
    r#"(function(){
'use strict';

// Phase 0: Server-side callback + routing

function azInit() {
    // Attach callback handlers
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
                    document.open();
                    document.write(html);
                    document.close();
                }
            })
            .catch(function(err) {
                console.error('[azul-web] callback error:', err);
            });
        });
    });

    // Intercept internal link clicks for SPA-style navigation
    document.querySelectorAll('a[href^="/"]').forEach(function(el) {
        el.addEventListener('click', function(e) {
            var href = el.getAttribute('href');
            if (!href || href.startsWith('/az/')) return; // Don't intercept asset URLs
            e.preventDefault();
            azNavigate(href);
        });
    });
}

// Navigate to a route (SPA-style)
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

// Browser back/forward
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

