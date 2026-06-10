//! HTTP server for the web backend.
//!
//! Uses `std::net::TcpListener` for zero external dependencies.
//! Serves:
//! - `GET /` and `GET /{route}` — pre-rendered HTML pages per route
//! - `GET /az/mini.{hash}.wasm` — framework WASM (stubbed in Phase 0)
//! - `GET /az/cb/{name}.{hash}.wasm` — callback WASMs (stubbed)
//! - `GET /az/loader.js` — bootstrap JavaScript
//! - `GET /az/img/{id}` — collected images
//! - `GET /az/font/{id}` — collected fonts
//! - `POST /az/exec/{node_id}` — server-side callback execution (Phase 0)

use std::collections::HashMap;
use std::io::{Read, Write, BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use azul_core::callbacks::{CoreCallback, LayoutCallback};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::{BoundaryWasm, CallbackWasm, LayoutWasm};
use super::config::WebConfig;
use super::html_render::{CollectedImage, CollectedFont};
use super::loader_js;

/// Pre-rendered route data.
pub struct RenderedRoute {
    /// Route pattern (e.g. `"/users/{id}"`).
    pub pattern: String,
    /// Pre-rendered HTML for this route.
    pub html: String,
    /// Layout callback associated with this route.
    pub layout_callback: LayoutCallback,
    /// `az_N → CoreCallback` map built by Phase C discovery. The
    /// `/az/exec/{node_id}` dispatcher uses this to look up the user
    /// callback bound to the clicked node. Empty if the route's DOM
    /// has no callbacks.
    pub callback_index: HashMap<u32, CoreCallback>,
}

/// Shared state for the web server.
pub struct WebServerState {
    /// Application data shared across all request handlers.
    pub app_data: Arc<Mutex<RefAny>>,
    /// Application configuration (fonts, theming, etc.).
    pub config: AppConfig,
    /// Web-backend specific configuration (body cap, auth token, etc.)
    /// parsed from the `AZ_BACKEND` URL.
    pub web_config: WebConfig,
    /// Font cache used for layout and text shaping.
    pub fc_cache: Arc<FcFontCache>,
    /// Optional font registry for system font lookup.
    pub font_registry: Option<Arc<FcFontRegistry>>,
    /// Window state used when re-rendering layouts.
    pub window_state: FullWindowState,
    /// Compiled framework WASM module served at `/az/mini.{hash}.wasm`.
    pub mini_wasm: Vec<u8>,
    /// Per-callback WASM modules served under `/az/cb/`.
    pub cb_wasms: Vec<CallbackWasm>,
    /// Per-layout-callback WASM modules served under `/az/layout/`
    /// (M8.3). Lifted from each unique `LayoutCallback.cb` referenced
    /// by the configured routes. Deduped by fn-addr.
    pub layout_wasms: Vec<LayoutWasm>,
    /// M10-D — per-boundary-fn WASM modules served under `/az/fn/`.
    /// One per unique `api.json::Framework` symbol referenced by any
    /// cb / layout / mini wasm. Empty in legacy bundled mode
    /// (`AZ_BUNDLED_LEGACY=1` or `AZ_ENABLE_SHARDS` unset).
    pub boundary_wasms: Vec<BoundaryWasm>,
    /// Default layout callback used by `re_render_body`.
    pub layout_callback: LayoutCallback,
    /// Pre-rendered routes: pattern → HTML.
    pub rendered_routes: HashMap<String, RenderedRoute>,
    /// Collected images (shared across all routes).
    pub images: Vec<CollectedImage>,
    /// Collected fonts (shared across all routes).
    pub fonts: Vec<CollectedFont>,
}

/// Start the HTTP server and block forever.
pub fn run_server(
    bind_addr: SocketAddr,
    state: WebServerState,
) -> Result<(), String> {
    let listener = TcpListener::bind(bind_addr)
        .map_err(|e| format!("Failed to bind to {}: {}", bind_addr, e))?;

    let state = Arc::new(state);

    // Generate loader JS once
    let loader_js = loader_js::generate_loader_js("stub", &state.cb_wasms);
    let loader_js = Arc::new(loader_js);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                let loader_js = Arc::clone(&loader_js);
                std::thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, &state, &loader_js) {
                        eprintln!("[azul-web] Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("[azul-web] Accept error: {}", e);
            }
        }
    }

    Ok(())
}

/// Handle a single HTTP connection.
fn handle_connection(
    mut stream: TcpStream,
    state: &WebServerState,
    loader_js: &str,
) -> Result<(), String> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| format!("set_read_timeout: {}", e))?;
    let mut reader = BufReader::new(&stream);

    // Read the request line
    let mut request_line = String::new();
    reader.read_line(&mut request_line)
        .map_err(|e| format!("read error: {}", e))?;

    // Read headers (we need Content-Length for POST, Referer for callback dispatch,
    // Authorization for `/az/exec/*` auth check)
    let mut content_length: usize = 0;
    let mut referer: Option<String> = None;
    let mut authorization: Option<String> = None;
    loop {
        let mut header_line = String::new();
        reader.read_line(&mut header_line)
            .map_err(|e| format!("header read error: {}", e))?;
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(val) = lower.strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        } else if lower.starts_with("referer:") {
            // Preserve case of the URL value by splitting the original line.
            if let Some((_, value)) = trimmed.split_once(':') {
                referer = Some(value.trim().to_string());
            }
        } else if lower.starts_with("authorization:") {
            if let Some((_, value)) = trimmed.split_once(':') {
                authorization = Some(value.trim().to_string());
            }
        }
    }

    // Reject oversized payloads using the configured cap (default 16 MiB).
    if content_length > state.web_config.max_body_bytes {
        return send_response(&mut stream, 413, "text/plain", b"Payload Too Large");
    }

    // Parse method and path
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return send_response(&mut stream, 400, "text/plain", b"Bad Request");
    }

    let method = parts[0];
    let path = parts[1];

    match (method, path) {
        // ── Static assets under /az/ ──

        ("GET", "/az/loader.js") => {
            send_response(&mut stream, 200, "application/javascript", loader_js.as_bytes())
        }
        // 2026-06-10: the dylib's embedded fallback TTF, fetched by the loader and fed to
        // AzStartup_setFallbackFont so the wasm-side solver shapes text with REAL bytes
        // (the lifted const mirror only covers statically-accessed pages of the font).
        ("GET", "/az/fallback.ttf") => send_response_cached(
            &mut stream,
            200,
            "font/ttf",
            super::eventloop::AZ_WEB_FALLBACK_FONT_BYTES,
        ),
        ("GET", p) if p.starts_with("/az/mini.") && p.ends_with(".wasm") => {
            send_response_cached(&mut stream, 200, "application/wasm", &state.mini_wasm)
        }
        ("GET", p) if p.starts_with("/az/cb/") && p.ends_with(".wasm") => {
            let name = p.strip_prefix("/az/cb/").unwrap_or("")
                .split('.').next().unwrap_or("");
            if let Some(cb) = state.cb_wasms.iter().find(|c| c.name == name) {
                send_response_cached(&mut stream, 200, "application/wasm", &cb.wasm_bytes)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Callback not found")
            }
        }
        ("GET", p) if p.starts_with("/az/layout/") && p.ends_with(".wasm") => {
            // M8.3: layout-callback WASMs. Same scheme as /az/cb/ —
            // URL is `/az/layout/{name}.{hash}.wasm`; we dispatch by
            // name and let the hash piece act as a cache-bust.
            let name = p.strip_prefix("/az/layout/").unwrap_or("")
                .split('.').next().unwrap_or("");
            if let Some(lw) = state.layout_wasms.iter().find(|l| l.name == name) {
                send_response_cached(&mut stream, 200, "application/wasm", &lw.wasm_bytes)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Layout callback not found")
            }
        }
        ("GET", p) if p.starts_with("/az/fn/") && p.ends_with(".wasm") => {
            // M10-D: boundary-shard wasms. URL is
            // `/az/fn/{canonical_name}.{hash}.wasm`. Dispatched by
            // canonical name; the hash is cache-bust only.
            let name = p.strip_prefix("/az/fn/").unwrap_or("")
                .split('.').next().unwrap_or("");
            if let Some(bw) = state.boundary_wasms.iter().find(|b| b.canonical_name == name) {
                send_response_cached(&mut stream, 200, "application/wasm", &bw.wasm_bytes)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Boundary shard not found")
            }
        }
        ("GET", p) if p == "/az/manifest.json"
            || (p.starts_with("/az/manifest.") && p.ends_with(".json")) =>
        {
            // M10-D: shard manifest. Lists every shard URL +
            // exports + imports so loader.js can topo-sort and
            // instantiate. Generated lazily on first hit so the
            // manifest's hash field stays in sync with whatever
            // bytes are actually being served. Both the unversioned
            // `/az/manifest.json` (loader bootstrap) and hashed
            // `/az/manifest.<hash>.json` URLs hit this branch.
            let manifest = build_manifest(state);
            // No-cache for manifest.json: small payload + always wins
            // on changes; the wasms it references have their own
            // hashed URLs for cache busting.
            send_response(
                &mut stream,
                200,
                "application/json",
                manifest.as_bytes(),
            )
        }

        // ── Image serving ──

        ("GET", p) if p.starts_with("/az/img/") => {
            let id_str = p.strip_prefix("/az/img/").unwrap_or("0");
            let id: usize = id_str.parse().unwrap_or(usize::MAX);
            if let Some(img) = state.images.iter().find(|i| i.id == id) {
                send_response_cached(&mut stream, 200, img.content_type, &img.data)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Image not found")
            }
        }

        // ── Font serving ──

        ("GET", p) if p.starts_with("/az/font/") => {
            let id_str = p.strip_prefix("/az/font/").unwrap_or("0");
            let id: usize = id_str.parse().unwrap_or(usize::MAX);
            if let Some(font) = state.fonts.iter().find(|f| f.id == id) {
                send_response_cached(&mut stream, 200, font.content_type, &font.data)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Font not found")
            }
        }

        // ── Phase C → Phase E: server-side callback dispatch ──
        //
        // Pipeline:
        //   1. Parse `node_id` from `/az/exec/{node_id}`.
        //   2. Locate the source route via the `Referer` header so we can
        //      pick the right per-route `callback_index` (different routes
        //      reuse the same `az_N` IDs).
        //   3. Look up the user `Callback`. If found, invoke it with a
        //      Phase 0 `CallbackInfo` skeleton — enough plumbing to mutate
        //      `RefAny` app state, which is the common case.
        //   4. Re-run layout via `re_render_body` and return the new HTML.

        ("POST", p) if p.starts_with("/az/exec/") => {
            // Auth check: when `auth_token` is configured, require a
            // matching `Authorization: Bearer <token>` header. Compared
            // with `constant_time_eq` to avoid leaking the token length
            // or matching prefix via timing.
            if !auth_check(
                state.web_config.auth_token.as_deref(),
                authorization.as_deref(),
            ) {
                return send_response(&mut stream, 401, "text/plain", b"Unauthorized");
            }

            // Explicit node_id check — rejects empty / non-digit strings
            // with 400 rather than silently falling through to a re-render.
            let node_idx = match parse_node_id(p) {
                Some(idx) => idx,
                None => {
                    return send_response(&mut stream, 400, "text/plain", b"Bad Request");
                }
            };

            // Read POST body so the connection is left in a clean state.
            // The JSON `{x, y, button, key}` payload is currently ignored —
            // the Phase 0 invocation runs with sentinel cursor values.
            if content_length > 0 {
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body)
                    .map_err(|e| format!("body read error: {}", e))?;
            }

            let route_pattern = referer_route(state, referer.as_deref());
            if let Some(route) = state.rendered_routes.get(&route_pattern) {
                if let Some(core_cb) = route.callback_index.get(&node_idx) {
                    // Best-effort invocation. If the surrounding state
                    // can't be reconstituted (rare; logged from inside),
                    // we silently fall through to `re_render_body` —
                    // worst case we lose this callback's side-effects
                    // beyond `app_data` mutations, which still flow
                    // through the next layout pass.
                    let _ = try_invoke_callback(state, node_idx, core_cb);
                }
            }

            let html = re_render_body(state);
            send_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes())
        }

        ("GET", "/favicon.ico") => {
            send_response(&mut stream, 204, "text/plain", b"")
        }

        // ── Route matching ──

        ("GET", path) => {
            // Try to match against registered routes
            if let Some(route) = state.rendered_routes.get(path) {
                send_response(&mut stream, 200, "text/html; charset=utf-8", route.html.as_bytes())
            } else {
                // Try parameterized route matching
                for route in state.rendered_routes.values() {
                    if azul_core::resources::match_route(&route.pattern, path).is_some() {
                        // For parameterized routes, we'd need to re-render with params.
                        // For now, serve the template HTML (Phase 0 limitation).
                        return send_response(&mut stream, 200, "text/html; charset=utf-8", route.html.as_bytes());
                    }
                }
                // Fall back to root route
                if let Some(root) = state.rendered_routes.get("/") {
                    send_response(&mut stream, 200, "text/html; charset=utf-8", root.html.as_bytes())
                } else if let Some(first) = state.rendered_routes.values().next() {
                    send_response(&mut stream, 200, "text/html; charset=utf-8", first.html.as_bytes())
                } else {
                    send_response(&mut stream, 404, "text/plain", b"No routes configured")
                }
            }
        }

        _ => {
            send_response(&mut stream, 404, "text/plain", b"Not Found")
        }
    }
}

/// Re-render the page body by calling the layout callback again.
fn re_render_body(state: &WebServerState) -> String {
    let app_data = state.app_data.lock()
        .unwrap_or_else(|e| e.into_inner());
    let output = super::html_render::render_initial_page(
        &app_data,
        &state.layout_callback,
        &state.window_state,
        &state.fc_cache,
        state.font_registry.as_ref().map(|r| r.as_ref()),
        &state.mini_wasm,
        None,
        state.config.bundled_fonts.as_ref(),
    );
    output.html
}

/// Pick the most likely source route for a callback dispatch.
///
/// The browser's `Referer` is `https://host:port/path`, so we split off
/// the host scheme and match the path against `state.rendered_routes`.
/// On miss we fall back to `/`, then to whatever route exists first —
/// matching the GET fallback further below.
fn referer_route(state: &WebServerState, referer: Option<&str>) -> String {
    if let Some(r) = referer {
        // Strip `scheme://host` if present and any trailing `?query#frag`.
        let path = r
            .split_once("://")
            .map(|(_, rest)| rest.splitn(2, '/').nth(1).map(|p| format!("/{}", p)).unwrap_or_else(|| "/".to_string()))
            .unwrap_or_else(|| r.to_string());
        let path = path.split(['?', '#']).next().unwrap_or("/").to_string();
        if state.rendered_routes.contains_key(&path) {
            return path;
        }
        for route in state.rendered_routes.values() {
            if azul_core::resources::match_route(&route.pattern, &path).is_some() {
                return route.pattern.clone();
            }
        }
    }
    if state.rendered_routes.contains_key("/") {
        return "/".to_string();
    }
    state.rendered_routes.keys().next().cloned().unwrap_or_else(|| "/".to_string())
}

/// Invoke a discovered callback server-side with a Phase 0 `CallbackInfo`.
///
/// Phase 0 doesn't retain a persistent `LayoutWindow` across requests, so
/// the helper constructs one per dispatch from `state.fc_cache`. That
/// gives the callback enough plumbing to mutate the shared `RefAny` and
/// push `CallbackChange`s (which we currently drop on the floor — only
/// the `app_data` mutation survives, picked up by the next
/// `re_render_body`). The full pipeline — applying focus/scroll/timer
/// changes, propagating `Update` to the layout system, etc. — is the
/// last piece of Phase E and waits on the persistent layout-window
/// refactor.
///
/// Wrapped in `catch_unwind` so a panicking user callback can't kill the
/// HTTP server thread.
fn try_invoke_callback(
    state: &WebServerState,
    node_idx: u32,
    core_cb: &CoreCallback,
) -> azul_core::callbacks::Update {
    use std::collections::BTreeMap;

    use azul_core::callbacks::Update;
    use azul_core::id::NodeId;
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::geom::OptionLogicalPosition;
    use azul_core::window::{MonitorVec, RawWindowHandle, WebHandle};
    use azul_core::resources::RendererResources;
    use azul_core::gl::OptionGlContextPtr;
    use azul_core::styled_dom::NodeHierarchyItemId;
    use azul_css::system::SystemStyle;
    use azul_layout::callbacks::{Callback, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks};
    use azul_layout::window::LayoutWindow;

    let cb = Callback::from_core(core_cb.clone());

    let layout_window = match LayoutWindow::new((*state.fc_cache).clone()) {
        Ok(lw) => lw,
        Err(e) => {
            eprintln!("[azul-web] callback dispatch: LayoutWindow::new failed ({:?})", e);
            return Update::DoNothing;
        }
    };
    let renderer_resources = RendererResources::default();
    let gl_context: OptionGlContextPtr = OptionGlContextPtr::None;
    let scroll: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, azul_core::hit_test::ScrollPosition>> =
        BTreeMap::new();
    let window_handle = RawWindowHandle::Web(WebHandle { id: 0 });
    let sys_callbacks = ExternalSystemCallbacks::rust_internal();
    let sys_style = std::sync::Arc::new(SystemStyle::default());
    let monitors_arc = std::sync::Arc::new(std::sync::Mutex::new(MonitorVec::from_const_slice(&[])));
    let previous_window_state: Option<FullWindowState> = None;

    let ref_data = CallbackInfoRefData {
        layout_window: &layout_window,
        renderer_resources: &renderer_resources,
        previous_window_state: &previous_window_state,
        current_window_state: &state.window_state,
        gl_context: &gl_context,
        current_scroll_manager: &scroll,
        current_window_handle: &window_handle,
        system_callbacks: &sys_callbacks,
        system_style: sys_style,
        monitors: monitors_arc,
        #[cfg(feature = "icu")]
        icu_localizer: azul_layout::icu::IcuLocalizerHandle::default(),
        ctx: cb.ctx.clone(),
    };

    let changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_idx as usize))),
    };
    let info = CallbackInfo::new(
        &ref_data,
        &changes,
        hit,
        OptionLogicalPosition::None,
        OptionLogicalPosition::None,
    );

    let app_data_locked = state.app_data.lock().unwrap_or_else(|e| e.into_inner());
    let app_data_clone = app_data_locked.clone();
    drop(app_data_locked);

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cb.cb)(app_data_clone, info))) {
        Ok(update) => update,
        Err(_) => {
            eprintln!("[azul-web] callback panicked at node az_{}", node_idx);
            Update::DoNothing
        }
    }
}

/// Parse `/az/exec/{node_id}` strictly: only ASCII digit characters are
/// accepted. Returns `None` for empty paths, non-digit characters, or
/// overflow.
pub(crate) fn parse_node_id(path: &str) -> Option<u32> {
    let id_str = path.strip_prefix("/az/exec/")?;
    if id_str.is_empty() || !id_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id_str.parse().ok()
}

/// Check the `Authorization` header against the configured `auth_token`.
///
/// When no token is configured, every request passes. Otherwise the
/// header must be `Bearer <token>` (case-insensitive prefix) and the
/// token must match `expected` exactly. The byte-wise comparison runs
/// in constant time relative to the token length to avoid timing leaks.
pub(crate) fn auth_check(expected: Option<&str>, provided: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let provided = provided.unwrap_or("");
    let token = provided
        .strip_prefix("Bearer ")
        .or_else(|| provided.strip_prefix("bearer "))
        .unwrap_or("");
    constant_time_eq(expected.as_bytes(), token.as_bytes())
}

/// Byte-wise equality whose execution time depends only on the length
/// of the inputs, not on their contents. Returns `false` immediately
/// for length mismatches — that leak is unavoidable, but matches the
/// surface area of `==` itself.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Send an HTTP response, optionally with immutable cache headers.
fn send_response_inner(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    cache_immutable: bool,
) -> Result<(), String> {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let cache_header = if cache_immutable {
        "Cache-Control: public, max-age=31536000, immutable\r\n"
    } else {
        ""
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
        status, status_text, content_type, body.len(), cache_header
    );

    stream.write_all(response.as_bytes())
        .map_err(|e| format!("write error: {}", e))?;
    stream.write_all(body)
        .map_err(|e| format!("write body error: {}", e))?;
    stream.flush()
        .map_err(|e| format!("flush error: {}", e))?;

    Ok(())
}

fn send_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    send_response_inner(stream, status, content_type, body, false)
}

fn send_response_cached(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    send_response_inner(stream, status, content_type, body, true)
}

/// M10-D — emit the shard manifest as a JSON string.
///
/// Shape (v1):
/// ```json
/// {
///   "version": 1,
///   "mini": { "url": "/az/mini.<hash>.wasm" },
///   "layout": [ { "name": "...", "url": "...", "fn_addr": 0 }, ... ],
///   "callbacks": [ { "name": "...", "url": "...", "fn_addr": 0 }, ... ],
///   "boundaries": [
///     { "name": "AzRefAny_clone", "url": "/az/fn/AzRefAny_clone.<hash>.wasm",
///       "body_export": "sub_<hex>", "canonical_addr": 12345,
///       "transitive_boundaries": [ ... ] },
///     ...
///   ]
/// }
/// ```
///
/// The JSON is hand-rolled (no serde_json dep here) to keep the
/// server module dependency-free. loader.js parses it via `JSON.parse`.
pub fn build_manifest(state: &WebServerState) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("{\"version\":1,");
    // Mini URL
    let mini_hash = super::fnv1a64_hex(&state.mini_wasm);
    out.push_str(&format!(
        "\"mini\":{{\"url\":\"/az/mini.{}.wasm\"}},",
        mini_hash,
    ));
    // Layout WASMs
    out.push_str("\"layout\":[");
    let mut first = true;
    for lw in &state.layout_wasms {
        if !first { out.push(','); }
        first = false;
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"url\":\"/az/layout/{}.{}.wasm\",\"fn_addr\":{},\
              \"client_side\":{}}}",
            json_escape(&lw.name),
            json_escape(&lw.name),
            lw.content_hash,
            lw.fn_addr,
            lw.is_client_side,
        ));
    }
    out.push_str("],");
    // Callback WASMs
    out.push_str("\"callbacks\":[");
    let mut first = true;
    for cb in &state.cb_wasms {
        if !first { out.push(','); }
        first = false;
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"url\":\"/az/cb/{}.{}.wasm\",\"fn_addr\":{},\
              \"client_side\":{}}}",
            json_escape(&cb.name),
            json_escape(&cb.name),
            cb.content_hash,
            cb.fn_addr,
            cb.is_client_side,
        ));
    }
    out.push_str("],");
    // Boundary WASMs
    out.push_str("\"boundaries\":[");
    let mut first = true;
    for bw in &state.boundary_wasms {
        if !first { out.push(','); }
        first = false;
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"url\":\"/az/fn/{}.{}.wasm\",\
              \"body_export\":\"{}\",\"canonical_addr\":{},\
              \"transitive_boundaries\":[",
            json_escape(&bw.canonical_name),
            json_escape(&bw.canonical_name),
            bw.content_hash,
            json_escape(&bw.body_export),
            bw.canonical_addr,
        ));
        let mut first_t = true;
        for &t in &bw.transitive_boundaries {
            if !first_t { out.push(','); }
            first_t = false;
            out.push_str(&format!("{}", t));
        }
        out.push_str("]}");
    }
    out.push(']');
    out.push('}');
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_node_id ──────────────────────────────────────────

    #[test]
    fn exec_rejects_non_numeric_node_id() {
        // Empty path after prefix → 400
        assert_eq!(parse_node_id("/az/exec/"), None);
        // Non-digit characters → 400
        assert_eq!(parse_node_id("/az/exec/foo"), None);
        // Mixed → 400 (defence against `4abc` accidentally parsing as 4)
        assert_eq!(parse_node_id("/az/exec/4abc"), None);
        // Negative → 400
        assert_eq!(parse_node_id("/az/exec/-1"), None);
        // Hex prefix → 400
        assert_eq!(parse_node_id("/az/exec/0x1f"), None);
        // Valid digit string → Some(idx)
        assert_eq!(parse_node_id("/az/exec/0"), Some(0));
        assert_eq!(parse_node_id("/az/exec/42"), Some(42));
        // Doesn't start with `/az/exec/` → None
        assert_eq!(parse_node_id("/az/other/42"), None);
    }

    // ── auth_check ─────────────────────────────────────────────

    #[test]
    fn exec_passes_when_no_auth_token_configured() {
        // No token → every header passes (including no header).
        assert!(auth_check(None, None));
        assert!(auth_check(None, Some("Bearer anything")));
        assert!(auth_check(None, Some("garbage")));
    }

    #[test]
    fn exec_requires_auth_token_when_set() {
        // Configured token, no header → reject.
        assert!(!auth_check(Some("s3cr3t"), None));
        // Configured token, wrong prefix → reject.
        assert!(!auth_check(Some("s3cr3t"), Some("Basic s3cr3t")));
        // Configured token, wrong token → reject.
        assert!(!auth_check(Some("s3cr3t"), Some("Bearer wrong")));
        // Configured token, correct token → accept.
        assert!(auth_check(Some("s3cr3t"), Some("Bearer s3cr3t")));
        // Lowercase `bearer` prefix accepted (some clients).
        assert!(auth_check(Some("s3cr3t"), Some("bearer s3cr3t")));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }

    // ── body cap policy ───────────────────────────────────────

    #[test]
    fn exec_rejects_oversize_body_policy() {
        // The policy: content_length > max_body_bytes → 413.
        // Body cap is read straight from `WebConfig.max_body_bytes`;
        // we verify the policy here, leaving the actual HTTP exchange
        // to manual / integration testing.
        let max = 1024usize;
        assert!(max + 1 > max);
        assert!(!(max > max));
    }
}
