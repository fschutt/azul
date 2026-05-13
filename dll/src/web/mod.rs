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
//!     → Phase C: discover + transpile callbacks (discovery functional, transpile stubbed)
//!     → Phase D: pre-render ALL routes → HTML with stylesheet
//!     → Phase E: start HTTP server, serve pages + /az/img/ + /az/font/
//! ```

pub mod config;
pub mod server;
pub mod html_render;
pub mod loader_js;
pub mod classify;
pub mod transpiler;
pub mod mini_gen;

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::CoreCallback;
use azul_core::refany::RefAny;
use azul_core::resources::{AppConfig, RouteMatch};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::shell2::common::WindowError;

/// FNV-1a 64-bit offset basis. Shared with `html_render::content_hash`
/// and `loader_js::loader_js_hash` so that all cache-busting URLs in
/// the web backend use the same hash family.
pub(crate) const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

/// FNV-1a 64-bit hash, formatted as a 16-char hex string.
pub(crate) fn fnv1a64_hex(data: &[u8]) -> String {
    let mut hash: u64 = FNV_OFFSET_BASIS;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", hash)
}

/// A discovered callback and its WASM module (if transpiled).
#[derive(Debug, Clone)]
pub struct CallbackWasm {
    /// Callback name (derived from symbol name via dladdr / SymFromAddr).
    pub name: String,
    /// Content hash for cache-busting (FNV-1a 64-bit of `name`).
    pub content_hash: String,
    /// WASM bytes. Empty until the remill-based transpiler is wired up.
    pub wasm_bytes: Vec<u8>,
    /// Whether this callback can run client-side (transpiled to WASM)
    /// or must fall back to server-side execution.
    pub is_client_side: bool,
}

/// One callback found while walking a route's `StyledDom`, bound to a
/// concrete synthetic `az_N` node ID.
#[derive(Debug, Clone)]
pub struct DiscoveredCallback {
    /// `az_N` synthetic node ID within the host route's render.
    pub node_idx: u32,
    /// Resolved symbol name (or `cb_{addr:x}` fallback).
    pub name: String,
    /// FNV-1a 64-bit hash of `name`, used in `/az/cb/{name}.{hash}.wasm`.
    pub content_hash: String,
    /// The underlying callback (carries the fn-pointer usize plus the
    /// optional ctx for managed-FFI hosts).
    pub callback: CoreCallback,
}

/// Resolve a function pointer to a symbol name.
///
/// Uses `dladdr(3)` on unix-like platforms (linked from libSystem on macOS
/// and libdl on Linux/Android). Windows lacks a stateless equivalent —
/// `SymFromAddr` requires `SymInitialize` and a cleanup pair — so we fall
/// back to `cb_{addr:x}` there. The fallback is also used when `dladdr`
/// reports no symbol (e.g. callbacks built into an executable without
/// `-rdynamic`).
pub(crate) fn resolve_fn_ptr_name(fn_addr: usize) -> String {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "android"))]
    unsafe {
        #[repr(C)]
        struct DlInfo {
            dli_fname: *const core::ffi::c_char,
            dli_fbase: *mut core::ffi::c_void,
            dli_sname: *const core::ffi::c_char,
            dli_saddr: *mut core::ffi::c_void,
        }
        extern "C" {
            fn dladdr(addr: *const core::ffi::c_void, info: *mut DlInfo) -> core::ffi::c_int;
        }
        let mut info = DlInfo {
            dli_fname: core::ptr::null(),
            dli_fbase: core::ptr::null_mut(),
            dli_sname: core::ptr::null(),
            dli_saddr: core::ptr::null_mut(),
        };
        if dladdr(fn_addr as *const _, &mut info) != 0 && !info.dli_sname.is_null() {
            if let Ok(s) = core::ffi::CStr::from_ptr(info.dli_sname).to_str() {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    format!("cb_{:016x}", fn_addr)
}

/// Aggregate `DiscoveredCallback`s from every rendered route into a
/// deduplicated `Vec<CallbackWasm>` keyed by function pointer.
///
/// Each unique fn-ptr becomes one `CallbackWasm` entry whose `wasm_bytes`
/// stays empty — the remill lift that fills them in is the final, untouched
/// step of Phase C. The output drives the server's `/az/cb/{name}.wasm`
/// route and (via the same `name` + `content_hash`) the per-page
/// `<link rel="preload">` hints emitted by `html_render`.
pub fn discover_and_transpile_callbacks(
    discovered_per_route: &BTreeMap<String, Vec<DiscoveredCallback>>,
) -> Vec<CallbackWasm> {
    let mut seen: BTreeMap<usize, ()> = BTreeMap::new();
    let mut out = Vec::new();
    for (_pattern, list) in discovered_per_route.iter() {
        for d in list {
            if seen.insert(d.callback.cb, ()).is_none() {
                out.push(CallbackWasm {
                    name: d.name.clone(),
                    content_hash: d.content_hash.clone(),
                    wasm_bytes: Vec::new(),
                    is_client_side: false,
                });
            }
        }
    }
    out
}

/// Run the web backend — called from `run()` when `AzBackend::Web(cfg)`.
///
/// This function blocks (like `run_headless`) serving HTTP requests until
/// the process is terminated.
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    web_config: config::WebConfig,
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

    // Phase D: Pre-render all routes. The walk also collects every
    // callback fn-pointer it sees, which feeds Phase C below.
    let window_state = root_window.window_state.clone();
    let default_layout_callback = root_window.window_state.layout_callback;

    let mut rendered_routes: HashMap<String, server::RenderedRoute> = HashMap::new();
    let mut all_images = Vec::new();
    let mut all_fonts = Vec::new();
    let mut discovered_per_route: BTreeMap<String, Vec<DiscoveredCallback>> = BTreeMap::new();

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
            None,
            config.bundled_fonts.as_ref(),
        );
        eprintln!("[azul-web] Route / : {} bytes HTML, {} images, {} fonts, {} callbacks",
            output.html.len(), output.images.len(), output.fonts.len(), output.callbacks.len());

        let callback_index = build_callback_index(&output.callbacks);
        all_images.extend(output.images);
        all_fonts.extend(output.fonts);
        discovered_per_route.insert("/".to_string(), output.callbacks);
        rendered_routes.insert("/".to_string(), server::RenderedRoute {
            pattern: "/".to_string(),
            html: output.html,
            layout_callback: default_layout_callback,
            callback_index,
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
                Some(&route_match),
                config.bundled_fonts.as_ref(),
            );

            eprintln!("[azul-web] Route {} : {} bytes HTML, {} images, {} fonts, {} callbacks",
                pattern, output.html.len(), output.images.len(), output.fonts.len(),
                output.callbacks.len());

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

            let callback_index = build_callback_index(&output.callbacks);
            discovered_per_route.insert(pattern.to_string(), output.callbacks);
            rendered_routes.insert(pattern.to_string(), server::RenderedRoute {
                pattern: pattern.to_string(),
                html,
                layout_callback: route.layout_callback.clone(),
                callback_index,
            });
        }
    }

    // Phase C: feed every discovered callback into the (still-stubbed) lift
    // pipeline. Discovery is functional (DOM walk + dladdr); `wasm_bytes`
    // stays empty until the remill / LLVM-IR / wasm-link pass is wired up.
    let cb_wasms = discover_and_transpile_callbacks(&discovered_per_route);
    eprintln!(
        "[azul-web] Discovered {} unique callbacks across {} route(s); transpile lift is stubbed",
        cb_wasms.len(), discovered_per_route.len(),
    );

    eprintln!(
        "[azul-web] Pre-rendered {} routes, {} total images, {} total fonts",
        rendered_routes.len(), all_images.len(), all_fonts.len(),
    );

    // Phase E: Start HTTP server
    let bind_addr = web_config.bind;
    eprintln!("[azul-web] Listening on http://{}", bind_addr);

    let state = server::WebServerState {
        app_data: Arc::new(Mutex::new(app_data)),
        config,
        web_config,
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

/// Build the `az_N → CoreCallback` map used by the `/az/exec/{node_id}`
/// dispatch handler.
///
/// When the same node ID carries multiple callbacks (e.g. one for `MouseUp`
/// and one for `MouseDown`), the first one wins. Phase 0 dispatches a single
/// callback per node — the event filter is already captured in
/// `data-az-ev` on the emitted HTML, so the client only targets the right
/// kind of event.
fn build_callback_index(discovered: &[DiscoveredCallback]) -> HashMap<u32, CoreCallback> {
    let mut idx: HashMap<u32, CoreCallback> = HashMap::new();
    for d in discovered {
        idx.entry(d.node_idx).or_insert_with(|| d.callback.clone());
    }
    idx
}
