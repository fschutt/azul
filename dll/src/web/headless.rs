//! `HeadlessApp` — the web backend's analogue of the desktop
//! `HeadlessWindow`. Wraps the same logical state (RefAny + config +
//! font cache + current StyledDom + window state) plus web-specific
//! bits needed for client-side dispatch (the lifted layout-cb's
//! symbol name + hash for URL composition).
//!
//! Per `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md`, this is the
//! source-of-truth that gets serialized to JSON + embedded in the
//! HTML head for the wasm client to deserialize at bootstrap.

use std::sync::Arc;

use azul_core::callbacks::LayoutCallback;
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_core::styled_dom::StyledDom;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;

/// Browser-side app state. Constructed in [`crate::web::run_web`]
/// after the initial layout-callback run produces the first
/// `StyledDom`; serialized to JSON via
/// [`headless_app_to_json`](super::hydration::headless_app_to_json)
/// (M8.7b) + embedded in the rendered HTML.
pub struct HeadlessApp {
    /// User's root RefAny — the App's data model.
    pub app_data: RefAny,
    /// App-level config (font_loading, accessibility, etc.).
    pub config: AppConfig,
    /// FcFontCache: same layout as desktop, but the JSON serializer
    /// rewrites `path` fields to `/az/font/<id>` URLs for the wasm
    /// side.
    pub font_cache: Arc<FcFontCache>,
    /// Window state at the time of initial render. The wasm client
    /// uses this for window-level event dispatch (resize, focus,
    /// scroll) + as the starting baseline for any window-state
    /// mutations the cb returns.
    pub window_state: FullWindowState,
    /// Most recent layout output. Hit-tested against incoming events
    /// + reconciled against on RefreshDom (M8.5c/d).
    pub current_dom: StyledDom,
    /// The root layout callback. Held so we can dladdr-resolve its
    /// symbol name + serve the lifted wasm under that name.
    pub layout_callback: LayoutCallback,
}

/// Server-startup validation result.
#[derive(Debug)]
pub enum ValidationError {
    /// User's root RefAny has no JSON serializer registered. The
    /// web backend can't hydrate state on the wasm client without
    /// one. See the `AZ_REFLECT_JSON` macro in `dll/azul.h`.
    RefAnyNotSerializable,
    /// Layout callback's fn-pointer couldn't be resolved to a real
    /// symbol via dladdr. The fallback `cb_<addr>` name still works
    /// for URL purposes but is opaque to users — flagged so they can
    /// add `-rdynamic` or static linkage to fix.
    LayoutCallbackUnresolvable,
}

impl HeadlessApp {
    /// Construct from the pieces `run_web` already has at hand.
    pub fn new(
        app_data: RefAny,
        config: AppConfig,
        font_cache: Arc<FcFontCache>,
        window_state: FullWindowState,
        current_dom: StyledDom,
        layout_callback: LayoutCallback,
    ) -> Self {
        Self {
            app_data,
            config,
            font_cache,
            window_state,
            current_dom,
            layout_callback,
        }
    }

    /// Validate the App can be hydrated on the wasm client. Called
    /// once at `run_web` startup, before any HTTP traffic.
    ///
    /// Returns `Ok(())` if the App is hydratable. `Err(...)` lists
    /// the FATAL issues that would prevent the web backend from
    /// shipping a working demo. The caller (`run_web`) prints a
    /// detailed message + aborts.
    ///
    /// Non-fatal warnings (lossy serializer roundtrip, etc.) are
    /// `eprintln!`'d directly and don't appear in the result.
    pub fn validate(&self) -> Result<(), ValidationError> {
        // 1. RefAny must have a registered JSON serializer.
        let serialized = azul_layout::json::refany_serialize_to_json(&self.app_data);
        match serialized {
            azul_core::json::OptionJson::None => {
                return Err(ValidationError::RefAnyNotSerializable);
            }
            azul_core::json::OptionJson::Some(ref json) => {
                eprintln!(
                    "[azul-web] RefAny JSON-roundtrip check: serialized → {}",
                    json
                );
                // M8.7a doesn't attempt the deserialize half (needs the
                // user's `_fromJson` fn-ptr lookup, which is in the
                // RefAny's internal RefCount). The roundtrip check
                // is deferred to M8.7b when we have the full
                // hydration pipeline + can run it server-side as a
                // dry-run.
            }
        }

        // 2. Layout callback should be dladdr-resolvable. Non-fatal.
        let sym = super::resolve_fn_ptr(self.layout_callback.cb as usize);
        if sym.name.starts_with("cb_") {
            eprintln!(
                "[azul-web] WARN: layout callback fn-ptr 0x{:016x} resolves to \
                 fallback `{}` (not a real symbol). The wasm client will still \
                 fetch /az/layout/{}.<hash>.wasm but the name is opaque — \
                 consider linking with `-rdynamic` or making the layout fn \
                 `pub extern \"C\"` so dladdr can recover its name.",
                self.layout_callback.cb as usize,
                sym.name,
                sym.name,
            );
        } else {
            eprintln!(
                "[azul-web] layout callback resolved: {} (addr=0x{:016x})",
                sym.name, sym.addr,
            );
        }

        Ok(())
    }
}
