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

use super::CallbackWasm;
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

    // Read headers (we need Content-Length for POST, Referer for callback dispatch)
    let mut content_length: usize = 0;
    let mut referer: Option<String> = None;
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
        }
    }

    // Reject oversized payloads (16 MB limit)
    const MAX_BODY: usize = 16 * 1024 * 1024;
    if content_length > MAX_BODY {
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
            let node_id_str = p.strip_prefix("/az/exec/").unwrap_or("");
            let node_idx: Option<u32> = node_id_str.parse().ok();

            // Read POST body so the connection is left in a clean state.
            // The JSON `{x, y, button, key}` payload is currently ignored —
            // the Phase 0 invocation runs with sentinel cursor values.
            if content_length > 0 {
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body)
                    .map_err(|e| format!("body read error: {}", e))?;
            }

            if let Some(idx) = node_idx {
                let route_pattern = referer_route(state, referer.as_deref());
                if let Some(route) = state.rendered_routes.get(&route_pattern) {
                    if let Some(core_cb) = route.callback_index.get(&idx) {
                        // Best-effort invocation. If the surrounding state
                        // can't be reconstituted (rare; logged from inside),
                        // we silently fall through to `re_render_body` —
                        // worst case we lose this callback's side-effects
                        // beyond `app_data` mutations, which still flow
                        // through the next layout pass.
                        let _ = try_invoke_callback(state, idx, core_cb);
                    }
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
    use azul_core::window::{DomNodeId, DomId, MonitorVec, OptionLogicalPosition, RawWindowHandle, WebHandle};
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

/// Send an HTTP response.
fn send_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status, status_text, content_type, body.len()
    );

    stream.write_all(response.as_bytes())
        .map_err(|e| format!("write error: {}", e))?;
    stream.write_all(body)
        .map_err(|e| format!("write body error: {}", e))?;
    stream.flush()
        .map_err(|e| format!("flush error: {}", e))?;

    Ok(())
}

/// Send an HTTP response with cache headers for immutable assets.
fn send_response_cached(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let status_text = match status {
        200 => "OK",
        _ => "Unknown",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: public, max-age=31536000, immutable\r\nConnection: close\r\n\r\n",
        status, status_text, content_type, body.len()
    );

    stream.write_all(response.as_bytes())
        .map_err(|e| format!("write error: {}", e))?;
    stream.write_all(body)
        .map_err(|e| format!("write body error: {}", e))?;
    stream.flush()
        .map_err(|e| format!("flush error: {}", e))?;

    Ok(())
}
