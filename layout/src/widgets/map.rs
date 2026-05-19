//! AzulMaps map widget. The P3 goal-app's central primitive.
//!
//! Architecture (per the user's design in MOBILE_SESSION_LOG and the
//! follow-up clarification):
//!
//! - **Widget, not a NodeType.** `MapWidget` builds a regular `<div>`
//!   that owns a `MapTileCache` `RefAny` dataset. The cache holds
//!   decoded SVG bytes per `MapTileId`; the dataset is the unit of
//!   persistence across relayout.
//! - **Tile cache survives relayout** via a `DatasetMergeCallback`.
//!   Every relayout creates a fresh `MapTileCache` skeleton; the
//!   merge callback transfers all `Ready` / `Pending` entries from
//!   the old dataset into the new one, so in-flight fetches and
//!   already-decoded SVGs aren't dropped.
//! - **VirtualView drives lazy rendering.** The widget's body is a
//!   `VirtualView` callback that:
//!     1. Computes which tile XYZs are visible from the current
//!        viewport + viewport size.
//!     2. For each visible tile not yet in the cache, marks it
//!        `Pending` and (eventually) enqueues an HTTP fetch.
//!     3. Returns a `Dom` whose children are one `<div>` per visible
//!        tile, GPU-translated into screen space via
//!        `transform: translate(x, y) scale(z)`. Each tile div's
//!        inner content is the cached SVG DOM, or an empty
//!        placeholder while the fetch is in flight.
//! - **MVT + MapCSS → SVG → DOM.** The decode pipeline (MVT protobuf
//!   bytes + a MapCSS stylesheet → an `<svg>` tree → the framework's
//!   existing svg-to-dom path) lands in a follow-up tick. This tick
//!   provides the widget shell + the dataset / merge-callback / virtual-
//!   view wiring; tiles render as empty placeholders.
//! - **Geolocation dot composes on top.** Users stack a normal child
//!   `Dom` (with a `NodeType::GeolocationProbe` deeper in the
//!   subtree) on top of the map widget — the widget doesn't bake in
//!   any geolocation feature itself.
//!
//! Compile gate: no new HTTP / MVT / proj4 dependencies in this tick.
//! Those land alongside the actual decode pipeline.

use alloc::collections::btree_map::BTreeMap;

use azul_core::callbacks::{
    VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn,
};
use azul_core::dom::{DatasetMergeCallbackType, Dom, OptionDom};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_css::dynamic_selector::CssPropertyWithConditionsVec;
use azul_css::AzString;

// ────────── POD types (api.json + codegen surface) ─────────────────────

/// Identity of one tile in a tiled-map XYZ scheme. Matches Leaflet /
/// OpenLayers / Mapbox conventions (Web Mercator, origin top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct MapTileId {
    /// Zoom level. `0` = whole world in one tile, `~14` = street level
    /// for vector tiles, `~19` for raster.
    pub z: u8,
    /// Tile column at this zoom.
    pub x: u32,
    /// Tile row at this zoom.
    pub y: u32,
}

/// Configuration of one map tile layer — usually the base raster /
/// vector layer. Additional layers (heatmaps, custom GeoJSON) compose
/// as further `MapWidget` instances stacked atop.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MapTileLayer {
    /// `{z}` / `{x}` / `{y}` placeholders are substituted at fetch
    /// time. Matches Leaflet's `tileLayer(url_template)`.
    pub url_template: AzString,
    /// Minimum integer zoom this layer supports.
    pub min_zoom: u8,
    /// Maximum integer zoom this layer supports.
    pub max_zoom: u8,
    /// Attribution string the user MUST display (ODbL "© OpenStreetMap
    /// contributors" or similar). Most providers require it.
    pub attribution: AzString,
}

impl Default for MapTileLayer {
    fn default() -> Self {
        Self {
            url_template: AzString::from(
                "https://openfreemap.org/example/{z}/{x}/{y}.pbf",
            ),
            min_zoom: 0,
            max_zoom: 14,
            attribution: AzString::from("© OpenStreetMap contributors, ODbL"),
        }
    }
}

/// Centre + zoom + rotation state. The Leaflet shape
/// (`map.setView([lat, lon], zoom)`). `bearing_deg` + `pitch_deg` are
/// reserved for future 3D-camera work; most callers leave them at zero.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MapViewport {
    pub centre_lat_deg: f64,
    pub centre_lon_deg: f64,
    pub zoom: f32,
    pub bearing_deg: f32,
    pub pitch_deg: f32,
}

impl Default for MapViewport {
    fn default() -> Self {
        // A neutral "whole world, slightly zoomed in" default. Apps
        // care will replace this immediately.
        Self {
            centre_lat_deg: 0.0,
            centre_lon_deg: 0.0,
            zoom: 2.0,
            bearing_deg: 0.0,
            pitch_deg: 0.0,
        }
    }
}

// ────────── MapWidget builder ──────────────────────────────────────────

// NOTE: `MapWidget` mirrors the api.json struct exactly (3 fields) so
// the codegen FFI transmute stays sound — do NOT add a function-pointer
// field here. The tile-fetch worker is supplied through the Rust-only
// `dom_with_fetch` method and stored in the (FFI-opaque) `MapTileCache`
// dataset instead.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MapWidget {
    pub layer: MapTileLayer,
    pub viewport: MapViewport,
    pub container_style: CssPropertyWithConditionsVec,
}

impl MapWidget {
    pub fn create(layer: MapTileLayer) -> Self {
        Self {
            layer,
            viewport: MapViewport::default(),
            container_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
        }
    }

    pub fn with_viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn with_container_style(mut self, css: CssPropertyWithConditionsVec) -> Self {
        self.container_style = css;
        self
    }

    /// Construct the rendered `Dom`. The returned `Dom` is a single
    /// `<div>` with:
    /// - A `MapTileCache` `RefAny` dataset (initialised from this
    ///   widget's `viewport` + `layer`).
    /// - A `DatasetMergeCallback` so the cache survives relayout.
    /// - A `VirtualView` child that re-renders the visible-tile grid
    ///   on bounds change.
    /// - Mouse-down / mouse-move / mouse-up callbacks that pan the
    ///   viewport while a drag is active (the widget owns the
    ///   pan state via `MapTileCache::drag_anchor`, so user code
    ///   doesn't have to wire anything).
    /// - Pinch callbacks that zoom in / out.
    ///
    /// No tile-fetch worker is wired — tiles render as placeholders.
    /// Use [`dom_with_fetch`](Self::dom_with_fetch) to supply one.
    pub fn dom(self) -> Dom {
        self.build_dom(None)
    }

    /// Like [`dom`](Self::dom), but wires a tile-fetch worker thread.
    /// Rust-only (the fn-pointer arg doesn't round-trip through the
    /// FFI codegen). `cb` runs on a framework `Thread` per visible
    /// tile — read the `TileFetchInit`, fetch + decode, then
    /// `sender.send(ThreadReceiveMsg::WriteBack(...))` a `TileReadyMsg`
    /// targeting [`map_tile_writeback`]. See the recipe in
    /// `MOBILE_SESSION_LOG.md`.
    pub fn dom_with_fetch(self, cb: crate::thread::ThreadCallbackType) -> Dom {
        self.build_dom(Some(cb))
    }

    fn build_dom(self, fetch_cb: Option<crate::thread::ThreadCallbackType>) -> Dom {
        use azul_core::dom::{EventFilter, HoverEventFilter};

        let mut cache = MapTileCache::new(self.layer.clone(), self.viewport);
        cache.fetch_callback = fetch_cb;
        let dataset = RefAny::new(cache);
        let virtual_view_data = dataset.clone();

        Dom::create_div()
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_map_tile_cache as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_down as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_move as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_up as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_up as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchStart),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_down as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchMove),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_move as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_up as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchCancel),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_up as crate::callbacks::CallbackType),
            )
            // Native gesture events (UIPinchGestureRecognizer on iOS,
            // ScaleGestureDetector on Android, NSMagnificationGestureRecognizer
            // on macOS) — fire through the same map_on_pointer_move handler
            // which reads `info.get_pinch()` and applies the zoom delta.
            .with_callback(
                EventFilter::Hover(HoverEventFilter::PinchIn),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_pointer_move as crate::callbacks::CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::PinchOut),
                dataset,
                crate::callbacks::Callback::from(map_on_pointer_move as crate::callbacks::CallbackType),
            )
            .with_child(Dom::create_virtual_view(
                virtual_view_data,
                map_widget_render as azul_core::callbacks::VirtualViewCallbackType,
            ))
    }
}

// ────────── Tile cache (dataset RefAny payload) ───────────────────────

#[derive(Debug)]
pub struct MapTileCache {
    pub layer: MapTileLayer,
    pub viewport: MapViewport,
    /// `Ready(svg)` once the tile has been fetched + decoded;
    /// `Pending` while queued, `Fetching` while a worker thread is
    /// in flight; absent otherwise. `BTreeMap` for deterministic
    /// iteration so the debug log + e2e snapshots are stable.
    pub tiles: BTreeMap<MapTileId, TileEntry>,
    /// Worker thread entry point that fetches + decodes one tile.
    /// Supplied by `MapWidget::with_tile_fetch_callback` (the caller —
    /// usually `azul_dll`'s map-tiles glue — provides this because the
    /// MVT decoder lives in `azul-dll`, which `azul-layout` can't
    /// depend on). `None` means "no fetch wired" — tiles stay
    /// `Pending` forever and the placeholder grid renders. The
    /// merge callback carries this across relayout.
    pub fetch_callback: Option<crate::thread::ThreadCallbackType>,
    /// Pixel coordinates of the cursor at the last mouse-down /
    /// touch-down on the widget. `Some` while a drag is in flight,
    /// `None` between drags. The framework consults this on every
    /// mouse-move to derive the pixel delta, which then converts to a
    /// lat/lon delta via the Web Mercator inverse.
    pub drag_anchor: Option<azul_core::geom::LogicalPosition>,
    /// Pinch reference distance (pixels) — the two-finger separation
    /// the last time a pinch event was observed for this widget.
    /// `Some` while a pinch is in flight, `None` between gestures.
    /// On each subsequent pinch update we compute
    /// `dz = log2(current_distance / pinch_anchor)` and add it to
    /// `viewport.zoom`, then reset the anchor to the current
    /// distance — so the gesture stays continuous across many frames.
    pub pinch_anchor: Option<f32>,
}

impl MapTileCache {
    pub fn new(layer: MapTileLayer, viewport: MapViewport) -> Self {
        Self {
            layer,
            viewport,
            tiles: BTreeMap::new(),
            fetch_callback: None,
            drag_anchor: None,
            pinch_anchor: None,
        }
    }

    /// Worker-thread → main-thread write path. Set the decoded SVG for
    /// a tile (called from `map_tile_writeback`). Stamps `Ready`.
    pub fn mark_tile_ready(&mut self, tile: MapTileId, svg: AzString) {
        self.tiles.insert(tile, TileEntry::Ready { svg });
    }

    /// Mark a tile's fetch as failed so the grid doesn't re-spawn it
    /// every frame.
    pub fn mark_tile_failed(&mut self, tile: MapTileId, error: AzString) {
        self.tiles.insert(tile, TileEntry::Failed { error });
    }
}

#[derive(Debug, Clone)]
pub enum TileEntry {
    /// Needed by the viewport, fetch not yet spawned.
    Pending,
    /// A worker thread is fetching / decoding this tile right now.
    /// Distinct from `Pending` so the spawn pass doesn't double-fire.
    Fetching,
    /// Tile decoded into an SVG document. Held as the raw SVG
    /// string for now; the VirtualView callback will feed it
    /// through the framework's svg-to-dom pipeline on the next
    /// re-render.
    Ready { svg: AzString },
    /// Fetch failed. Held so the framework doesn't immediately
    /// re-try the same URL — caller can choose to clear failed
    /// entries on retry.
    Failed { error: AzString },
}

/// Worker-thread input: which tile to fetch and the resolved URL.
/// Boxed into the `Thread::create` init `RefAny`.
#[derive(Debug, Clone)]
pub struct TileFetchInit {
    pub tile: MapTileId,
    pub url: AzString,
}

/// Worker-thread output, sent back via `ThreadWriteBackMsg`. The
/// `map_tile_writeback` callback downcasts to this and stamps the
/// cache.
#[derive(Debug, Clone)]
pub struct TileReadyMsg {
    pub tile: MapTileId,
    /// Decoded SVG document for the tile, or empty on failure (with
    /// `error` set).
    pub svg: AzString,
    /// Empty on success; an error message on failure.
    pub error: AzString,
}

// ────────── Merge callback — cache survives relayout ─────────────────

/// Copy every entry from the previous frame's cache into the new
/// frame's cache. The next layout pass thus sees the same in-flight /
/// decoded set without re-fetching anything.
extern "C" fn merge_map_tile_cache(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard_opt = new_data.downcast_mut::<MapTileCache>();
        let old_guard_opt = old_data.downcast_ref::<MapTileCache>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard_opt, old_guard_opt) {
            for (id, entry) in old_g.tiles.iter() {
                new_g.tiles.entry(*id).or_insert_with(|| entry.clone());
            }
            // Inherit the worker callback the builder stored last
            // frame (the freshly-built cache from `dom()` has it too,
            // but be defensive in case a future caller drops it).
            if new_g.fetch_callback.is_none() {
                new_g.fetch_callback = old_g.fetch_callback;
            }
            // Keep the freshest viewport (the one the layout pass
            // just attached) — only inherit tile bytes + worker.
        }
    }
    new_data
}

// ────────── Pan + zoom callbacks ─────────────────────────────────────

use crate::callbacks::CallbackInfo;
use azul_core::callbacks::Update;

/// Pointer down → record the drag anchor. The widget knows nothing
/// about the user's overall state RefAny — only its own dataset —
/// so the anchor lives in `MapTileCache::drag_anchor`.
extern "C" fn map_on_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => azul_core::geom::LogicalPosition::new(p.x, p.y),
        None => return Update::DoNothing,
    };
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        cache.drag_anchor = Some(pos);
    }
    Update::DoNothing
}

/// Pointer move during an active drag → translate the pixel delta
/// into a lat/lon delta via the Web Mercator inverse and update
/// `viewport.centre_lat_deg / centre_lon_deg`. Updates the anchor so
/// the next move computes a fresh delta.
///
/// If a pinch gesture is in flight (two fingers on the widget), the
/// pan branch is skipped and the move event drives zoom instead —
/// `dz = log2(current_distance / pinch_anchor)`. The next move resets
/// the anchor to the current distance so the gesture stays
/// continuous across many frames.
extern "C" fn map_on_pointer_move(mut data: RefAny, info: CallbackInfo) -> Update {
    // Active pinch wins over single-finger pan.
    if let Some(pinch) = info.get_pinch().into_option() {
        let mut cache = match data.downcast_mut::<MapTileCache>() {
            Some(c) => c,
            None => return Update::DoNothing,
        };
        let anchor = *cache.pinch_anchor.get_or_insert(pinch.current_distance);
        if anchor > 1.0 && pinch.current_distance > 1.0 {
            let dz = (pinch.current_distance / anchor).log2();
            let min = cache.layer.min_zoom as f32;
            let max = cache.layer.max_zoom as f32;
            cache.viewport.zoom = (cache.viewport.zoom + dz).clamp(min, max);
        }
        cache.pinch_anchor = Some(pinch.current_distance);
        // Pinch is exclusive with pan — clear the drag anchor so the
        // pinch end doesn't accidentally drop into a pan.
        cache.drag_anchor = None;
        return Update::RefreshDom;
    }

    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => azul_core::geom::LogicalPosition::new(p.x, p.y),
        None => return Update::DoNothing,
    };
    let mut cache_guard = match data.downcast_mut::<MapTileCache>() {
        Some(c) => c,
        None => return Update::DoNothing,
    };
    let anchor = match cache_guard.drag_anchor {
        Some(a) => a,
        None => return Update::DoNothing, // no active drag
    };

    let dx_px = (pos.x - anchor.x) as f64;
    let dy_px = (pos.y - anchor.y) as f64;
    if dx_px.abs() < 0.5 && dy_px.abs() < 0.5 {
        return Update::DoNothing;
    }

    // World pixels at the current fractional zoom. Each tile is 256 px
    // wide at integer zoom; fractional zoom scales linearly.
    let z = cache_guard.viewport.zoom as f64;
    let world_px = 256.0 * (2.0_f64).powf(z);

    // dx_px → delta longitude. World is 360° wide ⇒ degrees per pixel
    // = 360 / world_px. Dragging right (positive dx) should move the
    // map content to the right, which is equivalent to centring on a
    // lower longitude → minus sign.
    let d_lon = -dx_px * 360.0 / world_px;

    // dy_px → delta latitude via Mercator inverse. For small drags
    // the linear approximation `d_lat ≈ dy * cos(lat) * 360 / world`
    // is accurate to within a few metres at city zooms; the exact
    // Mercator inverse would only matter for very long drags near
    // the poles.
    let current_lat_rad = cache_guard.viewport.centre_lat_deg.to_radians();
    let d_lat = dy_px * 360.0 / world_px * current_lat_rad.cos();

    cache_guard.viewport.centre_lon_deg = wrap_lon(
        cache_guard.viewport.centre_lon_deg + d_lon,
    );
    cache_guard.viewport.centre_lat_deg =
        (cache_guard.viewport.centre_lat_deg + d_lat).clamp(-85.0, 85.0);
    cache_guard.drag_anchor = Some(pos);

    Update::RefreshDom
}

/// Pointer up / pointer leave → end the drag *and* the pinch. Either
/// can be in flight (and pinch supersedes pan in the move handler);
/// clear both anchors on release.
extern "C" fn map_on_pointer_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        cache.drag_anchor = None;
        cache.pinch_anchor = None;
    }
    // After a pan / pinch settles, kick off fetches for any tiles the
    // new viewport needs. (The VirtualView marked them `Pending` but
    // can't spawn threads itself — only a `CallbackInfo`-bearing
    // callback can.)
    spawn_pending_tile_fetches(&mut data, &mut info);
    Update::RefreshDom
}

fn wrap_lon(lon: f64) -> f64 {
    ((lon + 540.0) % 360.0) - 180.0
}

/// Parse a standalone `<svg>…</svg>` string into a `Dom` subtree via
/// the framework's existing XML→DOM path. The SVG is wrapped in a
/// minimal `<html><body>` envelope because `str_to_dom_unstyled`
/// expects a document root; the wrapper divs are zero-impact in
/// layout. Returns `None` if the `xml` feature is off or parsing
/// fails — the caller then falls back to the placeholder glyph.
#[cfg(feature = "xml")]
fn svg_string_to_dom(svg: &str) -> Option<Dom> {
    use azul_core::xml::{str_to_dom_unstyled, ComponentMap};

    let wrapped = alloc::format!("<html><body>{}</body></html>", svg);
    let nodes = crate::xml::parse_xml_string(&wrapped).ok()?;
    let component_map = ComponentMap::default();
    str_to_dom_unstyled(nodes.as_ref(), &component_map).ok()
}

#[cfg(not(feature = "xml"))]
fn svg_string_to_dom(_svg: &str) -> Option<Dom> {
    None
}

/// Scan the cache for `Pending` tiles and spawn one framework `Thread`
/// per tile (capped per call so a big viewport jump doesn't spawn
/// hundreds at once). Each thread gets:
/// - init `RefAny` = `TileFetchInit { tile, url }`
/// - writeback `RefAny` = a clone of the cache dataset, so
///   `map_tile_writeback` mutates the same cache the VirtualView reads.
///
/// Tiles transition `Pending → Fetching` here so they aren't
/// re-spawned next frame. No-op when the cache has no `fetch_callback`.
fn spawn_pending_tile_fetches(data: &mut RefAny, info: &mut CallbackInfo) {
    use crate::thread::Thread;
    use azul_core::task::ThreadId;

    // Per-call spawn cap — bounds the burst on a big viewport jump.
    const MAX_SPAWN_PER_CALL: usize = 16;

    // Collect the work first (URL build + state flip) under one borrow,
    // then spawn outside it so we don't hold the cache lock across
    // `info.add_thread`.
    let mut to_spawn: Vec<TileFetchInit> = Vec::new();
    {
        let mut cache = match data.downcast_mut::<MapTileCache>() {
            Some(c) => c,
            None => return,
        };
        if cache.fetch_callback.is_none() {
            return; // no worker wired — leave tiles Pending (placeholder grid)
        }
        let template = cache.layer.url_template.as_str().to_string();
        let pending: Vec<MapTileId> = cache
            .tiles
            .iter()
            .filter(|(_, e)| matches!(e, TileEntry::Pending))
            .map(|(id, _)| *id)
            .take(MAX_SPAWN_PER_CALL)
            .collect();
        for tile in pending {
            let url = build_tile_url(&template, tile);
            cache.tiles.insert(tile, TileEntry::Fetching);
            to_spawn.push(TileFetchInit {
                tile,
                url: AzString::from(url),
            });
        }
    }

    let cb = {
        let cache = match data.downcast_ref::<MapTileCache>() {
            Some(c) => c,
            None => return,
        };
        match cache.fetch_callback {
            Some(cb) => cb,
            None => return,
        }
    };

    for init in to_spawn {
        let init_data = RefAny::new(init);
        let writeback_data = data.clone(); // same cache dataset
        let thread = Thread::create(init_data, writeback_data, cb);
        info.add_thread(ThreadId::unique(), thread);
    }
}

/// `{z}/{x}/{y}` substitution. Mirrors `azul_dll`'s `build_tile_url`
/// (the widget can't reach the dll, so it's duplicated here — trivial).
fn build_tile_url(template: &str, tile: MapTileId) -> alloc::string::String {
    use alloc::string::ToString;
    template
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Worker-thread → main-thread writeback. `cache_dataset` is the
/// `writeback_data` handed to `Thread::create` (the same
/// `MapTileCache` the widget reads); `incoming` is the `TileReadyMsg`
/// the worker sent. Stamps the tile `Ready` (or `Failed`) and asks for
/// a relayout so the VirtualView renders the new content.
pub extern "C" fn map_tile_writeback(
    mut cache_dataset: RefAny,
    mut incoming: RefAny,
    _info: CallbackInfo,
) -> Update {
    let msg = match incoming.downcast_ref::<TileReadyMsg>() {
        Some(m) => (m.tile, m.svg.clone(), m.error.clone()),
        None => return Update::DoNothing,
    };
    let mut cache = match cache_dataset.downcast_mut::<MapTileCache>() {
        Some(c) => c,
        None => return Update::DoNothing,
    };
    if msg.2.as_str().is_empty() {
        cache.mark_tile_ready(msg.0, msg.1);
    } else {
        cache.mark_tile_failed(msg.0, msg.2);
    }
    Update::RefreshDom
}

// ────────── VirtualView callback — visible-tile rendering ─────────────

extern "C" fn map_widget_render(
    data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let mut data = data;
    let bounds = info.get_bounds();
    let bounds_logical = bounds.get_logical_size();
    let width_px = bounds_logical.width;
    let height_px = bounds_logical.height;

    let (layer, viewport) = match data.downcast_ref::<MapTileCache>() {
        Some(c) => (c.layer.clone(), c.viewport),
        None => {
            return VirtualViewReturn {
                dom: OptionDom::None,
                scroll_size: bounds_logical,
                scroll_offset: azul_core::geom::LogicalPosition::zero(),
                virtual_scroll_size: bounds_logical,
                virtual_scroll_offset: azul_core::geom::LogicalPosition::zero(),
            };
        }
    };

    // Round the requested fractional zoom down to the nearest integer
    // tile zoom the layer supports.
    let z_int = (viewport.zoom.floor() as i32)
        .clamp(layer.min_zoom as i32, layer.max_zoom as i32)
        as u8;
    let tile_count = 1u32 << z_int as u32;
    let frac_zoom = viewport.zoom - z_int as f32;
    let zoom_scale = 2.0_f32.powf(frac_zoom);

    // Convert WGS-84 → Web-Mercator-XYZ tile-space. The standard
    // formula (Bing tile system):
    //     x = (lon + 180) / 360
    //     y = (1 - ln(tan(lat) + sec(lat)) / pi) / 2
    let centre_x = ((viewport.centre_lon_deg + 180.0) / 360.0) as f32 * tile_count as f32;
    let centre_y = {
        let lat_rad = viewport.centre_lat_deg.to_radians();
        let mercator =
            (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / core::f64::consts::PI) / 2.0;
        mercator as f32 * tile_count as f32
    };

    // How many tiles fit on each side of centre, at fractional zoom.
    // 256 is the Mercator tile pixel size at integer zoom.
    let tile_px = 256.0 * zoom_scale;
    let half_w = (width_px / tile_px).abs() * 0.5 + 1.0;
    let half_h = (height_px / tile_px).abs() * 0.5 + 1.0;

    let x_min = ((centre_x - half_w).floor() as i32).max(0);
    let x_max = ((centre_x + half_w).ceil() as i32).min(tile_count as i32 - 1);
    let y_min = ((centre_y - half_h).floor() as i32).max(0);
    let y_max = ((centre_y + half_h).ceil() as i32).min(tile_count as i32 - 1);

    // Patch in any missing tiles as `Pending`. Real fetch dispatch
    // lands in the follow-up tick that adds the HTTP client; for now
    // we just track which tiles the viewport needs.
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        for x in x_min..=x_max {
            for y in y_min..=y_max {
                let id = MapTileId {
                    z: z_int,
                    x: x as u32,
                    y: y as u32,
                };
                cache.tiles.entry(id).or_insert(TileEntry::Pending);
            }
        }
    }

    // Snapshot the per-tile state under a short borrow, then drop it
    // before building DOM. `Ready` tiles carry their decoded SVG so the
    // render loop can parse it into a DOM child; the rest carry a glyph
    // (`…` Pending / `⟳` Fetching / `✗` Failed) so the fetch path stays
    // observable.
    enum TileDisplay {
        Glyph(&'static str),
        Svg(AzString),
    }
    let states: BTreeMap<MapTileId, TileDisplay> = match data.downcast_ref::<MapTileCache>() {
        Some(c) => c
            .tiles
            .iter()
            .map(|(id, e)| {
                let disp = match e {
                    TileEntry::Pending => TileDisplay::Glyph("…"),
                    TileEntry::Fetching => TileDisplay::Glyph("⟳"),
                    TileEntry::Ready { svg } => TileDisplay::Svg(svg.clone()),
                    TileEntry::Failed { .. } => TileDisplay::Glyph("✗"),
                };
                (*id, disp)
            })
            .collect(),
        None => BTreeMap::new(),
    };

    // Build the visible-tile grid. Each tile div is GPU-translated
    // into its screen position; the (CSS-driven) `transform` keeps
    // pan / zoom O(1) — no relayout per frame.
    let mut grid = Dom::create_div().with_css(
        "position: absolute; left: 0; top: 0; width: 100%; height: 100%; overflow: hidden;",
    );

    for x in x_min..=x_max {
        for y in y_min..=y_max {
            let id = MapTileId {
                z: z_int,
                x: x as u32,
                y: y as u32,
            };
            let screen_x =
                ((x as f32 - centre_x) * tile_px + width_px * 0.5).round() as i32;
            let screen_y =
                ((y as f32 - centre_y) * tile_px + height_px * 0.5).round() as i32;
            let size_px = tile_px.round().max(1.0) as i32;

            let style = alloc::format!(
                "position: absolute; left: {}px; top: {}px; \
                 width: {}px; height: {}px; \
                 background: #e7e9ec; border: 1px solid #d0d4d9;",
                screen_x, screen_y, size_px, size_px
            );

            let mut tile_div = Dom::create_div().with_css(style.as_str());

            // `Ready` tiles render their decoded SVG as a child DOM
            // tree (parsed via the framework's existing XML→DOM path);
            // everything else shows a state glyph + tile id so the grid
            // math + fetch state stay observable.
            match states.get(&id) {
                Some(TileDisplay::Svg(svg)) => match svg_string_to_dom(svg.as_str()) {
                    Some(svg_dom) => {
                        tile_div = tile_div.with_child(svg_dom);
                    }
                    None => {
                        tile_div = tile_div.with_child(
                            Dom::create_text(alloc::format!("✓? z{}/{}/{}", z_int, x, y))
                                .with_css("position: absolute; left: 4px; top: 4px; font-size: 11px; color: #888;"),
                        );
                    }
                },
                other => {
                    let state_tag = match other {
                        Some(TileDisplay::Glyph(g)) => *g,
                        _ => "",
                    };
                    tile_div = tile_div.with_child(
                        Dom::create_text(alloc::format!("{} z{}/{}/{}", state_tag, z_int, x, y))
                            .with_css("position: absolute; left: 4px; top: 4px; font-size: 11px; color: #888;"),
                    );
                }
            }

            grid = grid.with_child(tile_div);
        }
    }

    VirtualViewReturn {
        dom: OptionDom::Some(grid),
        scroll_size: bounds_logical,
        scroll_offset: azul_core::geom::LogicalPosition::zero(),
        virtual_scroll_size: bounds_logical,
        virtual_scroll_offset: azul_core::geom::LogicalPosition::zero(),
    }
}
