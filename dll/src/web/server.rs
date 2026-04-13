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

use azul_core::callbacks::LayoutCallback;
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

    // Read headers (we need Content-Length for POST)
    let mut content_length: usize = 0;
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

        // ── Phase 0: server-side callback execution ──

        ("POST", p) if p.starts_with("/az/exec/") => {
            let _node_id_str = p.strip_prefix("/az/exec/").unwrap_or("0");

            // Read POST body (if any)
            if content_length > 0 {
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body)
                    .map_err(|e| format!("body read error: {}", e))?;
            }

            // Re-run layout to produce updated HTML
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
        &state.cb_wasms,
        None,
        state.config.bundled_fonts.as_ref(),
    );
    output.html
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
