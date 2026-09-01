//! The parts of the debug/E2E server that CANNOT live in `azul-layout`.
//!
//! The ~12k-line op dispatcher, the `DebugEvent` schema, the E2E scenario runner
//! and the assertion library now live once, in [`azul_layout::e2e`], and are
//! re-exported by the parent module. Two copies of that file existed until this
//! commit and had drifted 1,300 lines apart, so an assertion fixed in one was
//! silently not fixed in the other.
//!
//! What stays here is exactly what has a hard dependency on this crate:
//!
//! * the HTTP transport (`start_debug_server`, `serve_response`,
//!   `handle_http_connection`, `compile_and_send_zip`) — it serves the debugger
//!   UI from `include_bytes!(concat!(env!("OUT_DIR"), "/debugger.*.br"))`, and
//!   those assets are emitted by THIS crate's `build.rs`;
//! * `register_debug_timer`, which takes a `&mut dyn PlatformWindow` — a trait
//!   that only exists in the DLL.
//!
//! Everything else these functions need (`DebugRequest`, `DebugServerHandle`,
//! `handle_event_request`, the log queue, …) is public API of
//! `azul_layout::e2e`.

use alloc::string::String;
use alloc::vec::Vec;
use std::sync::{mpsc, Arc, Mutex};

use azul_layout::e2e::{
    create_debug_timer, debug_server_port, handle_event_request,
    is_debug_enabled, log, serialize_http_response, set_debug_server, take_logs, DebugHttpResponse,
    DebugHttpResponseError, DebugHttpResponseOk, DebugRequest, DebugServerHandle, HealthResponse,
    LogCategory, LogLevel, LogMessageJson, ResponseData,
};

/// Initialize and start the debug server.
///
/// This function:
/// 1. Creates an `spmc::channel` for debug requests
/// 2. Binds to the port (exits process if port is taken)
/// 3. Starts the HTTP server thread (captures the `spmc::Sender`)
/// 4. Blocks until the server is ready to accept connections
/// 5. Stores the handle in `DEBUG_SERVER` for global access
/// 6. Returns the handle AND the `spmc::Receiver` for window timers
///
/// Called once from `run()` when `AZ_DEBUG=<port>` is set.
/// Subsequent calls return the existing handle (without a new receiver).
#[cfg(feature = "std")]
#[cfg(feature = "debug-server")]
pub fn start_debug_server(port: u16) -> (Arc<DebugServerHandle>, spmc::Receiver<DebugRequest>) {
    // HTTP-only: registering the served port has no meaning for a script run.
    use azul_layout::e2e::init_debug_server_statics;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    // Initialize the server-side statics that live in `azul_layout::e2e`
    // (start time, log queue, port, enabled flag).
    init_debug_server_statics(port);

    // Create spmc channel for debug requests
    let (request_tx, request_rx) = spmc::channel::<DebugRequest>();
    let request_tx = Arc::new(Mutex::new(request_tx));
    let request_tx_for_thread = request_tx.clone();

    // Try to bind - exit if port is taken
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Debug server failed to bind to port {}: {}", port, e);
            std::process::exit(1);
        }
    };

    // Set a short timeout for accept() so we can check for shutdown
    listener.set_nonblocking(false).ok();

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    // Channel to signal when server is ready
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    // Start server thread
    let thread_handle = thread::Builder::new()
        .name("azul-debug-server".to_string())
        .spawn(move || {
            // Signal that we're ready
            let _ = ready_tx.send(());

            // Set a timeout on the listener so we can check for shutdown
            listener.set_nonblocking(true).ok();

            log(
                LogLevel::Info,
                LogCategory::DebugServer,
                format!("Debug server listening on http://127.0.0.1:{}", port),
                None,
            );

            loop {
                // Check for shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    log(
                        LogLevel::Info,
                        LogCategory::DebugServer,
                        "Debug server shutting down",
                        None,
                    );
                    break;
                }

                // Try to accept a connection (non-blocking)
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        // NOTE: Stream explicitly set to blocking mode
                        // The listener is non-blocking, but accepted streams may inherit this.
                        // This causes the final read loop to fail immediately with WouldBlock,
                        // closing the socket before the client has read all data.
                        stream.set_nonblocking(false).ok();
                        // Set read timeout
                        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                        // Increase write timeout to 30s for large screenshot transfers
                        stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
                        handle_http_connection(&mut stream, &request_tx_for_thread);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No connection pending, sleep a bit
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        // Other error, continue
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        })
        .expect("Failed to spawn debug server thread");

    // Wait for server to be ready
    let _ = ready_rx.recv_timeout(Duration::from_secs(5));

    // Verify server is actually accepting connections
    for _ in 0..10 {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    log(
        LogLevel::Info,
        LogCategory::DebugServer,
        format!("Debug server ready on http://127.0.0.1:{}", port),
        None,
    );

    let handle = Arc::new(DebugServerHandle {
        shutdown_tx,
        thread_handle: Mutex::new(Some(thread_handle)),
        port,
        request_tx,
    });
    set_debug_server(handle.clone());
    (handle, request_rx)
}

// ==================== HTTP Server ====================

#[cfg(feature = "std")]
fn serve_response(stream: &mut std::net::TcpStream, header: &str, body: &[u8]) {
    use std::io::{Read, Write};
    stream.set_nodelay(true).ok();
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    for chunk in body.chunks(8192) {
        if stream.write_all(chunk).is_err() {
            return;
        }
    }
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut drain = [0u8; 512];
    while let Ok(n) = stream.read(&mut drain) {
        if n == 0 {
            break;
        }
    }
}

#[cfg(feature = "std")]
fn handle_http_connection(
    stream: &mut std::net::TcpStream,
    request_tx: &Arc<Mutex<spmc::Sender<DebugRequest>>>,
) {
    use std::io::{Read, Write};

    let mut buffer = [0u8; 16384];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Parse HTTP request
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return;
    }

    let first_line = lines[0];
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    // ── Route: GET /material-icons.ttf → serve embedded Material Icons font ──
    if method == "GET" && path == "/material-icons.ttf" {
        if let Some(font_bytes) = crate::desktop::material_icons::get_material_icons_font_bytes() {
            let header = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: font/ttf\r\nCache-Control: public, max-age=31536000\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                font_bytes.len()
            );
            serve_response(stream, &header, font_bytes);
        } else {
            let body = b"Material Icons font not available (icons feature not enabled)";
            let header = format!(
                "HTTP/1.0 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            serve_response(stream, &header, body);
        }
        return;
    }

    // Compressed debugger assets (gzip, built by build.rs)
    // Browsers decompress transparently via Content-Encoding: br.
    static DEBUGGER_CSS_BR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/debugger.css.br"));
    static DEBUGGER_JS_BR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/debugger.js.br"));
    static DEBUGGER_HTML_BR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/debugger.html.br"));

    // ── Route: GET /debugger.css → serve brotli-compressed CSS ──
    if method == "GET" && path == "/debugger.css" {
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/css; charset=utf-8\r\nContent-Encoding: br\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            DEBUGGER_CSS_BR.len()
        );
        serve_response(stream, &header, DEBUGGER_CSS_BR);
        return;
    }

    // ── Route: GET /debugger.js → serve brotli-compressed JS ──
    if method == "GET" && path == "/debugger.js" {
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/javascript; charset=utf-8\r\nContent-Encoding: br\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            DEBUGGER_JS_BR.len()
        );
        serve_response(stream, &header, DEBUGGER_JS_BR);
        return;
    }

    // ── Route: GET / → serve brotli-compressed debugger HTML ──
    if method == "GET" && (path == "/" || path == "/index.html") {
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Encoding: br\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            DEBUGGER_HTML_BR.len()
        );
        serve_response(stream, &header, DEBUGGER_HTML_BR);
        return;
    }

    // ── Route: POST /debug/compile?lang=<rust|cpp|python> → return generated project as ZIP ──
    if method == "POST" && path.starts_with("/debug/compile") {
        let lang = path
            .split_once('?')
            .and_then(|(_, q)| {
                q.split('&').find_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    if k == "lang" {
                        Some(v)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or("rust");

        let body_start = request
            .find("\r\n\r\n")
            .map(|i| i + 4)
            .or_else(|| request.find("\n\n").map(|i| i + 2));
        let css_source = body_start.map(|s| &request[s..]).unwrap_or("");

        compile_and_send_zip(stream, lang, css_source);
        return;
    }

    let response_json = match (method, path) {
        // Health check - GET /health
        ("GET", "/health") => {
            let logs = take_logs();
            let health = HealthResponse {
                port: debug_server_port(),
                pending_logs: logs.len(),
                logs: logs
                    .iter()
                    .map(|l| LogMessageJson {
                        timestamp_us: l.timestamp_us,
                        level: format!("{:?}", l.level),
                        category: format!("{:?}", l.category),
                        message: l.message.clone(),
                    })
                    .collect(),
            };
            serialize_http_response(&DebugHttpResponse::Ok(DebugHttpResponseOk {
                request_id: 0,
                window_state: None,
                data: Some(ResponseData::Health(health)),
            }))
        }

        // Event handling - POST /
        ("POST", "/") => {
            // Parse body
            let body_start = request
                .find("\r\n\r\n")
                .map(|i| i + 4)
                .or_else(|| request.find("\n\n").map(|i| i + 2));

            if let Some(start) = body_start {
                let body = &request[start..];
                handle_event_request(body, request_tx)
            } else {
                serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                    request_id: None,
                    message: "No request body".to_string(),
                }))
            }
        }

        _ => serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
            request_id: None,
            message: "GET / → debugger UI, GET /debugger.css → CSS, GET /debugger.js → JS, GET /material-icons.ttf → font, GET /health → status, POST / → debug commands (incl. run_e2e_tests), POST /debug/compile?lang=rust → standalone project ZIP".to_string(),
        })),
    };

    let body_bytes = response_json.as_bytes();
    let header = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    serve_response(stream, &header, body_bytes);
}

/// Compile a CSS source string into a standalone project for `lang` and stream
/// the resulting ZIP back over `stream`. Errors are surfaced as 4xx/5xx
/// responses rather than abrupt disconnects so the AZ_DEBUG webpage can render
/// a useful message.
#[cfg(feature = "std")]
fn compile_and_send_zip(stream: &mut std::net::TcpStream, lang: &str, css_source: &str) {
    use std::io::{Read, Write};

    use azul_css::codegen::backend_for;
    use azul_layout::zip::{ZipFileEntry, ZipWriteConfig};

    let backend = match backend_for(lang) {
        Some(b) => b,
        None => {
            let body = format!("Unknown lang: {lang}. Supported: rust, cpp, python.");
            let header = format!(
                "HTTP/1.0 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: \
                 {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.set_nodelay(true).ok();
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let mut drain = [0u8; 64];
            while let Ok(n) = stream.read(&mut drain) {
                if n == 0 {
                    break;
                }
            }
            return;
        }
    };

    let (parsed, _warnings) = azul_css::parser2::new_from_str(css_source);
    let files = backend.emit_project(&parsed);

    let entries: Vec<ZipFileEntry> = files
        .into_iter()
        .map(|f| ZipFileEntry::file(f.path, f.contents.into_bytes()))
        .collect();
    let archive = azul_layout::zip::ZipFile { entries };

    let zip_bytes = match archive.to_bytes(&ZipWriteConfig::default()) {
        Ok(b) => b,
        Err(e) => {
            let body = format!("ZIP write failed: {e:?}");
            let header = format!(
                "HTTP/1.0 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: \
                 {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.set_nodelay(true).ok();
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let mut drain = [0u8; 64];
            while let Ok(n) = stream.read(&mut drain) {
                if n == 0 {
                    break;
                }
            }
            return;
        }
    };

    let header = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: attachment; \
         filename=\"azul-generated-{lang}.zip\"\r\nContent-Length: {}\r\nConnection: \
         close\r\n\r\n",
        zip_bytes.len()
    );
    stream.set_nodelay(true).ok();
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    for chunk in zip_bytes.chunks(8192) {
        if stream.write_all(chunk).is_err() {
            return;
        }
    }
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut drain = [0u8; 64];
    while let Ok(n) = stream.read(&mut drain) {
        if n == 0 {
            break;
        }
    }
}

/// Register the debug timer on a window if `AZ_DEBUG` or E2E mode is active.
///
/// This is the single cross-platform entry point that replaces the
/// copy-pasted registration blocks in each platform window constructor.
/// It reads `app_data` and `window_id` from the window, then creates
/// a `DebugTimerData` with the given channel receiver and component map.
#[cfg(feature = "std")]
pub fn register_debug_timer(
    window: &mut dyn crate::desktop::shell2::common::event::PlatformWindow,
    request_rx: spmc::Receiver<DebugRequest>,
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
) {
    if !is_debug_enabled() {
        return;
    }

    log(
        LogLevel::Debug,
        LogCategory::DebugServer,
        "[Window Init] Registering debug timer",
        None,
    );

    /// Well-known timer ID for the debug server polling timer.
    /// Chosen to avoid collision with user-registered timer IDs.
    const DEBUG_TIMER_ID: usize = 0xDEBE;
    let timer_id: usize = DEBUG_TIMER_ID;
    let app_data_for_timer = window.get_app_data().borrow().clone();
    let window_id = window
        .get_current_window_state()
        .window_id
        .as_str()
        .to_string();
    let get_system_time_fn =
        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal().get_system_time_fn;
    let debug_timer = create_debug_timer(
        app_data_for_timer,
        get_system_time_fn,
        request_rx,
        component_map,
        window_id,
    );
    window.start_timer(timer_id, debug_timer);

    log(
        LogLevel::Debug,
        LogCategory::DebugServer,
        format!(
            "[Window Init] Debug timer registered with ID 0x{:X}",
            timer_id
        ),
        None,
    );
}

// ==================== Host hooks ====================

/// Install this crate's implementation for the one call site
/// `azul_layout::e2e` cannot satisfy on its own: the native OS screenshot
/// (see `azul_layout::e2e::hooks`).
///
/// Called once from `setup_debug_and_e2e` before any request can be dispatched.
/// Without it the `screenshot` op errors out — loudly, not silently: the
/// headless default is an `Err`, never a fake success.
#[cfg(feature = "std")]
pub fn install_e2e_host_hooks() {
    use azul_layout::e2e::hooks::{set_host_hooks, E2eHostHooks};

    fn screenshot(ci: &mut azul_layout::callbacks::CallbackInfo) -> Result<String, String> {
        use crate::desktop::native_screenshot::NativeScreenshotExt;
        // Explicitly the trait method, not the stubbed inherent method on CallbackInfo.
        NativeScreenshotExt::take_native_screenshot_base64(&*ci)
            .map(|s| s.as_str().to_string())
            .map_err(|e| e.as_str().to_string())
    }

    set_host_hooks(E2eHostHooks {
        take_native_screenshot_base64: Some(screenshot),
    });
}
