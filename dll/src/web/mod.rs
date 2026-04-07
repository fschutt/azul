//! Web backend for Azul (`AZ_BACKEND=web://ip:port`).
//!
//! When enabled, Azul runs as an HTTP server instead of opening a native
//! window. The layout callback executes natively and the resulting DOM is
//! rendered to HTML with a CSS stylesheet (per-node-ID rules).
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
//!     → Phase D: pre-render ALL routes → HTML with stylesheet
//!     → Phase E: start HTTP server, serve pages + /az/img/ + /az/font/
//! ```

pub mod config;
pub mod server;
pub mod html_render;
pub mod loader_js;
pub mod classify;
pub mod transpiler;
pub mod cb_gen;
pub mod mini_gen;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::LayoutCallbackInfo;
use azul_core::refany::RefAny;
use azul_core::resources::{AppConfig, RouteMatch};
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
    let cb_wasms = cb_gen::discover_and_transpile_callbacks();
    eprintln!(
        "[azul-web] Discovered {} user callbacks (server-side execution mode)",
        cb_wasms.len()
    );

    // Phase D: Pre-render all routes
    let window_state = root_window.window_state.clone();
    let default_layout_callback = root_window.window_state.layout_callback;

    let mut rendered_routes = HashMap::new();
    let mut all_images = Vec::new();
    let mut all_fonts = Vec::new();

    let routes = config.routes.as_ref();

    if routes.is_empty() {
        // No routes configured → use the root window's layout callback as "/"
        eprintln!("[azul-web] No routes configured, using root layout as /");
        let output = html_render::render_initial_page(
            &app_data,
            &default_layout_callback,
            &window_state,
            &fc_cache,
            font_registry.as_deref(),
            &mini_wasm,
            &cb_wasms,
            None,
            config.bundled_fonts.as_ref(),
        );
        eprintln!("[azul-web] Route / : {} bytes HTML, {} images, {} fonts",
            output.html.len(), output.images.len(), output.fonts.len());

        all_images.extend(output.images);
        all_fonts.extend(output.fonts);
        rendered_routes.insert("/".to_string(), server::RenderedRoute {
            pattern: "/".to_string(),
            html: output.html,
            layout_callback: default_layout_callback,
        });
    } else {
        // Pre-render each registered route
        for route in routes.iter() {
            let pattern = route.pattern.as_str();
            eprintln!("[azul-web] Pre-rendering route: {}", pattern);

            let route_match = RouteMatch {
                pattern: route.pattern.clone(),
                params: azul_core::window::StringPairVec::from_const_slice(&[]),
            };

            let output = html_render::render_initial_page(
                &app_data,
                &route.layout_callback,
                &window_state,
                &fc_cache,
                font_registry.as_deref(),
                &mini_wasm,
                &cb_wasms,
                Some(&route_match),
                config.bundled_fonts.as_ref(),
            );

            eprintln!("[azul-web] Route {} : {} bytes HTML, {} images, {} fonts",
                pattern, output.html.len(), output.images.len(), output.fonts.len());

            // Rebase image/font IDs to avoid collisions across routes
            let img_offset = all_images.len();
            let font_offset = all_fonts.len();
            let mut html = output.html;

            // Rewrite image IDs in HTML (simple string replace)
            for img in &output.images {
                let old = format!("/az/img/{}", img.id);
                let new = format!("/az/img/{}", img.id + img_offset);
                html = html.replace(&old, &new);
            }
            for font in &output.fonts {
                let old = format!("/az/font/{}", font.id);
                let new = format!("/az/font/{}", font.id + font_offset);
                html = html.replace(&old, &new);
            }

            for mut img in output.images {
                img.id += img_offset;
                all_images.push(img);
            }
            for mut font in output.fonts {
                font.id += font_offset;
                all_fonts.push(font);
            }

            rendered_routes.insert(pattern.to_string(), server::RenderedRoute {
                pattern: pattern.to_string(),
                html,
                layout_callback: route.layout_callback.clone(),
            });
        }
    }

    eprintln!(
        "[azul-web] Pre-rendered {} routes, {} total images, {} total fonts",
        rendered_routes.len(), all_images.len(), all_fonts.len(),
    );

    // Phase E: Start HTTP server
    eprintln!("[azul-web] Listening on http://{}", bind_addr);

    let state = server::WebServerState {
        app_data: Arc::new(Mutex::new(app_data)),
        config,
        fc_cache,
        font_registry,
        window_state,
        mini_wasm,
        cb_wasms,
        layout_callback: default_layout_callback,
        rendered_routes,
        images: all_images,
        fonts: all_fonts,
    };

    server::run_server(bind_addr, state)
        .map_err(|e| WindowError::PlatformError(format!("Web server error: {}", e)))
}
