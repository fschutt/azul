//! Web backend for Azul (`AZ_BACKEND=web://ip:port`).
//!
//! When enabled, Azul runs as an HTTP server instead of opening a native
//! window. The layout callback executes natively and the resulting DOM is
//! rendered to HTML. In Phase 0 (stub transpiler), callbacks execute
//! server-side with page updates sent back as HTML fragments.
//!
//! Future phases will transpile callbacks to WASM (via remill) for fully
//! client-side interaction with zero server round-trips.
//!
//! # Architecture
//!
//! ```text
//! AZ_BACKEND=web://127.0.0.1:8080
//!   → AzBackend::Web(addr)
//!   → run_web(app_data, config, fc_cache, font_registry, root_window, addr)
//!     → Phase A: classify API functions (stubbed)
//!     → Phase B: generate azul-mini.wasm (stubbed)
//!     → Phase C: discover + transpile callbacks (stubbed)
//!     → Phase D: run layout() → render DOM to HTML
//!     → Phase E: start HTTP server, serve pages
//! ```

pub mod config;
pub mod server;
pub mod html_render;
pub mod loader_js;
pub mod classify;
pub mod transpiler;
pub mod cb_gen;
pub mod mini_gen;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::LayoutCallbackInfo;
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::shell2::common::WindowError;

/// Run the web backend — called from `run()` when `AzBackend::Web(addr)`.
///
/// This function blocks (like `run_headless`) serving HTTP requests until
/// the process is terminated.
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    bind_addr: SocketAddr,
) -> Result<(), WindowError> {

    eprintln!("[azul-web] Starting web backend...");

    // Phase A: Classify API functions (stubbed for now)
    let classification = classify::classify_api_functions();
    eprintln!(
        "[azul-web] Classified {} API functions ({} framework, {} excluded)",
        classification.total(),
        classification.framework_count(),
        classification.excluded_count(),
    );

    // Phase B: Generate azul-mini.wasm (stubbed)
    let mini_wasm = mini_gen::generate_mini_wasm(&classification);
    eprintln!(
        "[azul-web] azul-mini.wasm: {} bytes (stub)",
        mini_wasm.len()
    );

    // Phase C: Discover + transpile user callbacks (stubbed)
    // In Phase 0, we skip transpilation — callbacks run server-side
    let cb_wasms = cb_gen::discover_and_transpile_callbacks();
    eprintln!(
        "[azul-web] Discovered {} user callbacks (server-side execution mode)",
        cb_wasms.len()
    );

    // Phase D: Run layout() to get the initial DOM, render to HTML
    let window_state = root_window.window_state.clone();
    let layout_callback = root_window.window_state.layout_callback;

    eprintln!("[azul-web] Running initial layout...");
    let initial_html = html_render::render_initial_page(
        &app_data,
        &layout_callback,
        &window_state,
        &fc_cache,
        font_registry.as_deref(),
        &mini_wasm,
        &cb_wasms,
    );
    eprintln!(
        "[azul-web] Initial HTML: {} bytes",
        initial_html.len()
    );

    // Phase E: Start HTTP server
    eprintln!("[azul-web] Listening on http://{}", bind_addr);

    let state = server::WebServerState {
        app_data: Arc::new(Mutex::new(app_data)),
        config,
        fc_cache,
        font_registry,
        window_state,
        initial_html,
        mini_wasm,
        cb_wasms,
        layout_callback,
    };

    server::run_server(bind_addr, state)
        .map_err(|e| WindowError::PlatformError(format!("Web server error: {}", e)))
}
