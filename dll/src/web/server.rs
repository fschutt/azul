//! HTTP server for the web backend.
//!
//! Uses `std::net::TcpListener` for zero external dependencies.
//! Serves:
//! - `GET /` — initial HTML page (pre-rendered from layout callback)
//! - `GET /az/mini.{hash}.wasm` — framework WASM (stubbed in Phase 0)
//! - `GET /az/cb/{name}.{hash}.wasm` — callback WASMs (stubbed)
//! - `GET /az/loader.js` — bootstrap JavaScript
//! - `POST /az/exec/{node_id}` — server-side callback execution (Phase 0)

use std::io::{Read, Write, BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use azul_core::callbacks::LayoutCallback;
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::cb_gen::CallbackWasm;
use super::loader_js;

/// Shared state for the web server.
pub struct WebServerState {
    pub app_data: Arc<Mutex<RefAny>>,
    pub config: AppConfig,
    pub fc_cache: Arc<FcFontCache>,
    pub font_registry: Option<Arc<FcFontRegistry>>,
    pub window_state: FullWindowState,
    pub initial_html: String,
    pub mini_wasm: Vec<u8>,
    pub cb_wasms: Vec<CallbackWasm>,
    pub layout_callback: LayoutCallback,
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
                // Handle each connection synchronously for simplicity.
                // Phase 1+ could use a thread pool or async runtime.
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
        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
        if let Some(val) = trimmed.strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
    }

    // Parse method and path
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return send_response(&mut stream, 400, "text/plain", b"Bad Request");
    }

    let method = parts[0];
    let path = parts[1];

    match (method, path) {
        ("GET", "/") => {
            send_response(&mut stream, 200, "text/html; charset=utf-8", state.initial_html.as_bytes())
        }
        ("GET", "/az/loader.js") => {
            send_response(&mut stream, 200, "application/javascript", loader_js.as_bytes())
        }
        ("GET", p) if p.starts_with("/az/mini.") && p.ends_with(".wasm") => {
            send_response(&mut stream, 200, "application/wasm", &state.mini_wasm)
        }
        ("GET", p) if p.starts_with("/az/cb/") && p.ends_with(".wasm") => {
            // Look up callback WASM by name
            let name = p.strip_prefix("/az/cb/").unwrap_or("")
                .split('.').next().unwrap_or("");
            if let Some(cb) = state.cb_wasms.iter().find(|c| c.name == name) {
                send_response(&mut stream, 200, "application/wasm", &cb.wasm_bytes)
            } else {
                send_response(&mut stream, 404, "text/plain", b"Callback not found")
            }
        }
        ("POST", p) if p.starts_with("/az/exec/") => {
            // Phase 0: server-side callback execution
            let node_id_str = p.strip_prefix("/az/exec/").unwrap_or("0");

            // Read POST body (if any)
            if content_length > 0 {
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body)
                    .map_err(|e| format!("body read error: {}", e))?;
            }

            // Re-run layout to produce updated HTML
            // In Phase 0, we can't execute individual callbacks server-side yet
            // (would need to match node_id to callback fn ptr). For now, just
            // re-render the full page body.
            let html = re_render_body(state);
            send_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes())
        }
        ("GET", "/favicon.ico") => {
            send_response(&mut stream, 204, "text/plain", b"")
        }
        _ => {
            send_response(&mut stream, 404, "text/plain", b"Not Found")
        }
    }
}

/// Re-render the page body by calling the layout callback again.
fn re_render_body(state: &WebServerState) -> String {
    let app_data = state.app_data.lock().unwrap();
    super::html_render::render_initial_page(
        &app_data,
        &state.layout_callback,
        &state.window_state,
        &state.fc_cache,
        state.font_registry.as_ref().map(|r| r.as_ref()),
        &state.mini_wasm,
        &state.cb_wasms,
    )
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
