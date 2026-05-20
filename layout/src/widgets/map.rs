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
use azul_css::impl_option_inner; // for impl_widget_callback!'s impl_option!
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
    /// MapCSS-style stylesheet driving per-layer fill / stroke /
    /// stroke-width. Empty = use the built-in default palette. Each
    /// rule is `selector { fill: …; stroke: …; stroke-width: …; }`
    /// where the selector's trailing token is matched against the MVT
    /// layer name (e.g. `water { fill: #9ecae1; }`, `.buildings { … }`).
    /// Parsed by `azul_dll::desktop::extra::map`'s tile decoder.
    pub style_css: AzString,
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
            style_css: AzString::from(""),
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

// NOTE: `MapWidget` mirrors the api.json struct field-for-field so the
// codegen FFI transmute stays sound. Callback fields (e.g.
// `on_viewport_changed`) ARE allowed: codegen keeps `AzMapWidget` in sync
// (the Button / Camera pattern). The Rust-only tile-fetch worker stays in
// the FFI-opaque `MapTileCache` dataset (supplied via `dom_with_fetch`).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MapWidget {
    pub layer: MapTileLayer,
    pub viewport: MapViewport,
    pub container_style: CssPropertyWithConditionsVec,
    /// Optional hook fired when the user pans / zooms (effects / persist
    /// the viewport). FFI-exposed; re-set on each fresh build.
    pub on_viewport_changed: OptionMapViewportChanged,
}

impl MapWidget {
    pub fn create(layer: MapTileLayer) -> Self {
        Self {
            layer,
            viewport: MapViewport::default(),
            container_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            on_viewport_changed: OptionMapViewportChanged::None,
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

    /// Set a hook fired when the user pans / zooms the map. The map owns its
    /// own pan/pinch state; this lets your app observe or persist the
    /// resulting `MapViewport`. The backreference DI pattern (architecture.md).
    pub fn set_on_viewport_changed<C: Into<MapViewportChangedCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_viewport_changed = Some(MapViewportChanged {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_viewport_changed`](Self::set_on_viewport_changed).
    pub fn with_on_viewport_changed<C: Into<MapViewportChangedCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_viewport_changed(data, callback);
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
    /// `cb` runs on a framework `Thread` per visible tile: it reads the
    /// `TileFetchInit`, fetches + decodes, then
    /// `sender.send(ThreadReceiveMsg::WriteBack(...))` a `TileReadyMsg`
    /// targeting `map_tile_writeback`. The standard worker is
    /// `azul_dll::desktop::extra::map::tile_fetch_worker`; wrap it in a
    /// `ThreadCallback` to pass it here. See the recipe in
    /// `MOBILE_SESSION_LOG.md`.
    pub fn dom_with_fetch(self, cb: crate::thread::ThreadCallback) -> Dom {
        self.build_dom(Some(cb))
    }

    fn build_dom(self, fetch_cb: Option<crate::thread::ThreadCallback>) -> Dom {
        use azul_core::dom::{ComponentEventFilter, EventFilter, HoverEventFilter};

        let mut cache = MapTileCache::new(self.layer.clone(), self.viewport);
        cache.fetch_callback = fetch_cb;
        cache.on_viewport_changed = self.on_viewport_changed;
        let dataset = RefAny::new(cache);
        let virtual_view_data = dataset.clone();

        Dom::create_div()
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_map_tile_cache as DatasetMergeCallbackType)
            // AfterMount fires once when the widget first appears (and
            // again after a DOM-structure change re-mounts it). It's the
            // earliest point with a `CallbackInfo`, so we kick the
            // initial tile fetches here — without it the first frame's
            // tiles would stay `Pending` until the user panned/tapped.
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset.clone(),
                crate::callbacks::Callback::from(map_on_after_mount as crate::callbacks::CallbackType),
            )
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
    /// Supplied by `MapWidget::dom_with_fetch` (the caller, usually
    /// `azul_dll`'s map-tiles glue, provides this because the MVT
    /// decoder lives in `azul-dll`, which `azul-layout` can't depend
    /// on). `None` means "no fetch wired": tiles stay `Pending` and
    /// the placeholder grid renders. The merge callback carries this
    /// across relayout. Held as the `ThreadCallback` wrapper (not the
    /// raw fn pointer) so it round-trips through the FFI codegen.
    pub fetch_callback: Option<crate::thread::ThreadCallback>,
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
    /// The user's `on_viewport_changed` hook, copied here from the builder
    /// so the pan / pinch callbacks can fire it. Carried across relayout.
    pub on_viewport_changed: OptionMapViewportChanged,
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
            on_viewport_changed: OptionMapViewportChanged::None,
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

/// Worker-thread input: which tile to fetch, the resolved URL, and the
/// MapCSS stylesheet to apply when converting features to SVG. Boxed
/// into the `Thread::create` init `RefAny`.
#[derive(Debug, Clone)]
pub struct TileFetchInit {
    pub tile: MapTileId,
    pub url: AzString,
    /// Copy of `MapTileLayer::style_css` (empty = default palette).
    pub style_css: AzString,
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
                new_g.fetch_callback = old_g.fetch_callback.clone();
            }
            if let OptionMapViewportChanged::None = new_g.on_viewport_changed {
                new_g.on_viewport_changed = old_g.on_viewport_changed.clone();
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

// --- User hook: on_viewport_changed (backreference DI, FFI-exposed) ---

/// User hook fired when the user pans or zooms the map. Lets app code observe
/// or persist the widget-driven `MapViewport` (which otherwise lives only in
/// the opaque `MapTileCache`). The backreference DI pattern (architecture.md).
pub type MapViewportChangedCallbackType =
    extern "C" fn(RefAny, CallbackInfo, MapViewport) -> Update;
impl_widget_callback!(
    MapViewportChanged,
    OptionMapViewportChanged,
    MapViewportChangedCallback,
    MapViewportChangedCallbackType
);
azul_core::impl_managed_callback! {
    wrapper:        MapViewportChangedCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MAP_VIEWPORT_CHANGED_INVOKER,
    invoker_ty:     AzMapViewportChangedCallbackInvoker,
    thunk_fn:       az_map_viewport_changed_callback_thunk,
    setter_fn:      AzApp_setMapViewportChangedCallbackInvoker,
    from_handle_fn: AzMapViewportChangedCallback_createFromHostHandle,
    extra_args:     [ viewport: MapViewport ],
}

/// Invoke a map widget's optional `on_viewport_changed` hook with the new
/// viewport, returning the user's `Update` (`DoNothing` if no hook is set).
fn invoke_viewport_changed(
    hook: &OptionMapViewportChanged,
    info: &CallbackInfo,
    viewport: MapViewport,
) -> Update {
    match hook {
        OptionMapViewportChanged::Some(h) => {
            (h.callback.cb)(h.refany.clone(), info.clone(), viewport)
        }
        OptionMapViewportChanged::None => Update::DoNothing,
    }
}

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
        let hook = cache.on_viewport_changed.clone();
        let vp = cache.viewport;
        drop(cache);
        invoke_viewport_changed(&hook, &info, vp);
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

    let (new_lon, new_lat) = pan_viewport(
        cache_guard.viewport.centre_lat_deg,
        cache_guard.viewport.centre_lon_deg,
        cache_guard.viewport.zoom as f64,
        dx_px,
        dy_px,
    );
    cache_guard.viewport.centre_lon_deg = new_lon;
    cache_guard.viewport.centre_lat_deg = new_lat;
    cache_guard.drag_anchor = Some(pos);

    let hook = cache_guard.on_viewport_changed.clone();
    let vp = cache_guard.viewport;
    drop(cache_guard);
    invoke_viewport_changed(&hook, &info, vp);
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
    // `rem_euclid` (not `%`) so even large negative deltas normalise:
    // `%` follows the dividend's sign and would leak values < -180.
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

// ────────── Web-Mercator (WGS-84 ↔ XYZ tile space) ───────────────────
//
// `tile_count` is `2^zoom`. Tile-space x grows east (0 at lon -180,
// `tile_count` at lon +180); y grows south (0 at the north edge
// ~85.05°, `tile_count` at the south edge). These four functions are
// exact inverses of each other and are the single source of truth for
// the widget's projection — `map_widget_render` forward-projects the
// viewport centre through them; tap-to-pin will inverse-project taps.

/// Longitude (deg) → fractional tile-x at the given `tile_count`.
fn lon_to_tile_x(lon_deg: f64, tile_count: f64) -> f64 {
    (lon_deg + 180.0) / 360.0 * tile_count
}

/// Latitude (deg) → fractional tile-y at the given `tile_count`.
fn lat_to_tile_y(lat_deg: f64, tile_count: f64) -> f64 {
    let lat_rad = lat_deg.to_radians();
    let mercator =
        (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / core::f64::consts::PI) / 2.0;
    mercator * tile_count
}

/// Fractional tile-x → longitude (deg). Inverse of [`lon_to_tile_x`].
/// Verified against the forward direction in the tests below; the
/// upcoming tap-to-pin handler reuses it to turn a tap into a lat/lon.
#[allow(dead_code)]
fn tile_x_to_lon(x: f64, tile_count: f64) -> f64 {
    x / tile_count * 360.0 - 180.0
}

/// Fractional tile-y → latitude (deg). Inverse of [`lat_to_tile_y`].
#[allow(dead_code)]
fn tile_y_to_lat(y: f64, tile_count: f64) -> f64 {
    let n = core::f64::consts::PI * (1.0 - 2.0 * y / tile_count);
    n.sinh().atan().to_degrees()
}

/// Apply a drag of `(dx_px, dy_px)` screen pixels to a viewport centre,
/// returning the new `(centre_lon_deg, centre_lat_deg)`. Dragging right
/// (+dx) pans the map content right, i.e. recentres on a *lower* longitude
/// (hence the minus). Latitude uses the small-angle Mercator approximation
/// (`d_lat ≈ dy·cos(lat)·360/world`), accurate to a few metres at city
/// zooms; the exact inverse only matters for very long drags near the
/// poles. Longitude wraps to [-180, 180); latitude clamps to the
/// Web-Mercator ±85.05° limit. The shared, unit-tested core of
/// `map_on_pointer_move`.
fn pan_viewport(
    centre_lat_deg: f64,
    centre_lon_deg: f64,
    zoom: f64,
    dx_px: f64,
    dy_px: f64,
) -> (f64, f64) {
    // World pixels at the current fractional zoom (256 px / tile).
    let world_px = 256.0 * (2.0_f64).powf(zoom);
    let d_lon = -dx_px * 360.0 / world_px;
    let d_lat = dy_px * 360.0 / world_px * centre_lat_deg.to_radians().cos();
    let new_lon = wrap_lon(centre_lon_deg + d_lon);
    let new_lat = (centre_lat_deg + d_lat).clamp(-85.0, 85.0);
    (new_lon, new_lat)
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

/// Fires once when the widget first mounts. Kicks the initial tile
/// fetches so the map populates without waiting for a user gesture.
/// (The VirtualView marks the viewport's tiles `Pending` during the
/// layout pass that precedes mount-event dispatch; this handler then
/// spawns the workers for them.) Returns `RefreshDom` so the
/// `Fetching` state shows immediately.
extern "C" fn map_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    spawn_pending_tile_fetches(&mut data, &mut info);
    Update::RefreshDom
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
        let style_css = cache.layer.style_css.clone();
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
                style_css: style_css.clone(),
            });
        }
    }

    let cb = {
        let cache = match data.downcast_ref::<MapTileCache>() {
            Some(c) => c,
            None => return,
        };
        match cache.fetch_callback.as_ref() {
            Some(cb) => cb.clone(),
            None => return,
        }
    };

    for init in to_spawn {
        let init_data = RefAny::new(init);
        let writeback_data = data.clone(); // same cache dataset
        let thread = Thread::create(init_data, writeback_data, cb.clone());
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

/// Inclusive `(x_min, x_max, y_min, y_max)` tile range covering a
/// `width_px × height_px` viewport centred at tile-space `(centre_x,
/// centre_y)`, at fractional `zoom_scale` and integer `tile_count` (2^z).
/// A one-tile margin (`+ 1.0`) is added each side so a tile scrolling into
/// view is already requested; the result is clamped to the valid
/// `0..=tile_count-1` grid. The pure core of `map_widget_render`'s grid
/// loop — what decides which tiles get fetched.
fn visible_tile_range(
    centre_x: f32,
    centre_y: f32,
    width_px: f32,
    height_px: f32,
    zoom_scale: f32,
    tile_count: u32,
) -> (i32, i32, i32, i32) {
    let tile_px = 256.0 * zoom_scale;
    let half_w = (width_px / tile_px).abs() * 0.5 + 1.0;
    let half_h = (height_px / tile_px).abs() * 0.5 + 1.0;
    let max_idx = tile_count as i32 - 1;
    let x_min = ((centre_x - half_w).floor() as i32).max(0);
    let x_max = ((centre_x + half_w).ceil() as i32).min(max_idx);
    let y_min = ((centre_y - half_h).floor() as i32).max(0);
    let y_max = ((centre_y + half_h).ceil() as i32).min(max_idx);
    (x_min, x_max, y_min, y_max)
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

    // Convert WGS-84 → Web-Mercator-XYZ tile-space via the shared
    // projection helpers (the single source of truth, unit-tested below).
    let centre_x = lon_to_tile_x(viewport.centre_lon_deg, tile_count as f64) as f32;
    let centre_y = lat_to_tile_y(viewport.centre_lat_deg, tile_count as f64) as f32;

    // 256 is the Mercator tile pixel size at integer zoom; tile_px is also
    // used below to position each tile div.
    let tile_px = 256.0 * zoom_scale;
    let (x_min, x_max, y_min, y_max) =
        visible_tile_range(centre_x, centre_y, width_px, height_px, zoom_scale, tile_count);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() < eps, "expected {a} ≈ {b} (within {eps})");
    }

    #[test]
    fn wrap_lon_keeps_in_range() {
        approx(wrap_lon(0.0), 0.0, 1e-9);
        approx(wrap_lon(179.0), 179.0, 1e-9);
        approx(wrap_lon(-179.0), -179.0, 1e-9);
        // Past the antimeridian wraps to the other side.
        approx(wrap_lon(181.0), -179.0, 1e-9);
        approx(wrap_lon(-181.0), 179.0, 1e-9);
        // 540° ≡ 180° ≡ -180° — the antimeridian normalises to -180.
        approx(wrap_lon(540.0), -180.0, 1e-9);
        // Anything fed in must come out within [-180, 180].
        for raw in [-1234.5, -360.0, 360.0, 999.9] {
            let w = wrap_lon(raw);
            assert!((-180.0..=180.0).contains(&w), "{raw} → {w} out of range");
        }
    }

    #[test]
    fn build_tile_url_substitutes_zxy() {
        let tile = MapTileId { z: 11, x: 327, y: 791 };
        assert_eq!(
            build_tile_url("https://t.example/{z}/{x}/{y}.pbf", tile),
            "https://t.example/11/327/791.pbf"
        );
        // Repeated and out-of-order placeholders both resolve.
        assert_eq!(
            build_tile_url("{y}-{x}-{z}-{z}", MapTileId { z: 3, x: 4, y: 5 }),
            "5-4-3-3"
        );
    }

    #[test]
    fn lon_tile_endpoints() {
        // At zoom 0 the world is one tile: -180° → 0, +180° → 1.
        approx(lon_to_tile_x(-180.0, 1.0), 0.0, 1e-9);
        approx(lon_to_tile_x(180.0, 1.0), 1.0, 1e-9);
        approx(lon_to_tile_x(0.0, 1.0), 0.5, 1e-9);
        // Greenwich at zoom 1 (2 tiles wide) sits on the seam.
        approx(lon_to_tile_x(0.0, 2.0), 1.0, 1e-9);
    }

    #[test]
    fn lat_tile_equator_and_symmetry() {
        // Equator maps to the vertical centre of the map.
        approx(lat_to_tile_y(0.0, 1.0), 0.5, 1e-9);
        // North is above (smaller y) and is mirror-symmetric to south.
        let north = lat_to_tile_y(45.0, 1.0);
        let south = lat_to_tile_y(-45.0, 1.0);
        assert!(north < 0.5 && south > 0.5);
        approx(north + south, 1.0, 1e-9);
    }

    #[test]
    fn projection_round_trips() {
        // Forward then inverse must return the original coordinate, for
        // a handful of real-world points across several zooms.
        let points = [
            (37.7749, -122.4194), // San Francisco
            (51.5074, -0.1278),   // London
            (-33.8688, 151.2093), // Sydney
            (0.0, 0.0),           // null island
        ];
        for z in [0u32, 5, 11, 18] {
            let tc = (1u64 << z) as f64;
            for (lat, lon) in points {
                let x = lon_to_tile_x(lon, tc);
                let y = lat_to_tile_y(lat, tc);
                approx(tile_x_to_lon(x, tc), lon, 1e-6);
                approx(tile_y_to_lat(y, tc), lat, 1e-6);
            }
        }
    }

    #[test]
    fn pan_zero_drag_is_identity() {
        // No movement → centre unchanged (lon/lat already in range).
        let (lon, lat) = pan_viewport(37.0, -122.0, 11.0, 0.0, 0.0);
        approx(lon, -122.0, 1e-9);
        approx(lat, 37.0, 1e-9);
    }

    #[test]
    fn pan_right_decreases_longitude() {
        // Dragging content right (+dx) recentres on a lower longitude.
        let (lon, _) = pan_viewport(0.0, 0.0, 0.0, 100.0, 0.0);
        assert!(lon < 0.0, "drag right should lower longitude, got {lon}");
        // Dragging left (-dx) is the mirror.
        let (lon_left, _) = pan_viewport(0.0, 0.0, 0.0, -100.0, 0.0);
        approx(lon_left, -lon, 1e-9);
    }

    #[test]
    fn pan_step_scales_inversely_with_zoom() {
        // Each extra zoom level doubles the world size, so the same pixel
        // drag should move the centre half as far in degrees.
        let (lon_z0, _) = pan_viewport(0.0, 0.0, 0.0, 50.0, 0.0);
        let (lon_z1, _) = pan_viewport(0.0, 0.0, 1.0, 50.0, 0.0);
        approx(lon_z1, lon_z0 / 2.0, 1e-9);
    }

    #[test]
    fn pan_clamps_latitude_to_mercator_limit() {
        // A huge vertical drag can't push the centre past ±85°.
        let (_, lat_north) = pan_viewport(84.0, 0.0, 0.0, 0.0, 1.0e6);
        assert!(lat_north <= 85.0 && lat_north >= -85.0);
        let (_, lat_south) = pan_viewport(-84.0, 0.0, 0.0, 0.0, -1.0e6);
        assert!(lat_south <= 85.0 && lat_south >= -85.0);
    }

    #[test]
    fn pan_wraps_longitude_across_antimeridian() {
        // Starting near +180 and panning further east wraps into negatives
        // rather than producing an out-of-range longitude.
        let (lon, _) = pan_viewport(0.0, 179.0, 0.0, -100.0, 0.0);
        assert!((-180.0..180.0).contains(&lon), "lon {lon} out of range");
    }

    fn viewport_at(zoom: f32) -> MapViewport {
        MapViewport {
            centre_lat_deg: 0.0,
            centre_lon_deg: 0.0,
            zoom,
            bearing_deg: 0.0,
            pitch_deg: 0.0,
        }
    }

    #[test]
    fn merge_preserves_old_tiles_and_keeps_new_viewport() {
        // The merge callback is what lets the tile cache survive relayout:
        // a tile downloaded last frame must still be present in the cache
        // the layout pass rebuilds this frame, without re-fetching.
        let tile = MapTileId { z: 5, x: 1, y: 2 };
        let mut old_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(5.0));
        old_cache.mark_tile_ready(tile, AzString::from("<svg/>"));
        // Fresh cache as rebuilt by dom() each relayout: new viewport, no tiles.
        let new_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(9.0));

        let mut merged =
            merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        let g = merged.downcast_ref::<MapTileCache>().unwrap();

        // Downloaded tile survived the relayout...
        assert!(g.tiles.contains_key(&tile), "old tile must survive relayout");
        // ...but the freshest viewport (just attached by the layout pass) wins.
        approx(g.viewport.zoom as f64, 9.0, 1e-6);
    }

    #[test]
    fn merge_keeps_new_tile_over_old() {
        // When both frames have the same tile, the new frame's entry wins
        // (or_insert_with must not clobber a freshly-stamped tile).
        let tile = MapTileId { z: 5, x: 1, y: 2 };
        let mut old_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(5.0));
        old_cache.mark_tile_ready(tile, AzString::from("OLD"));
        let mut new_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(5.0));
        new_cache.mark_tile_ready(tile, AzString::from("NEW"));

        let mut merged =
            merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        let g = merged.downcast_ref::<MapTileCache>().unwrap();

        match g.tiles.get(&tile) {
            Some(TileEntry::Ready { svg }) => {
                assert_eq!(svg.as_str(), "NEW", "new frame's tile must not be clobbered");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn tile_range_covers_centre_with_margin() {
        // 512×512 viewport at zoom-scale 1 (256 px tiles) = 2 tiles across;
        // half-extent 2 (incl. the +1 margin) → 5 tiles each axis, centred.
        let (x0, x1, y0, y1) = visible_tile_range(8.0, 8.0, 512.0, 512.0, 1.0, 16);
        assert_eq!((x0, x1), (6, 10));
        assert_eq!((y0, y1), (6, 10));
    }

    #[test]
    fn tile_range_clamps_to_single_tile_world_at_zoom0() {
        // zoom 0 → tile_count 1, so the only valid index is 0 regardless of
        // viewport size; the margin must not produce out-of-range indices.
        let (x0, x1, y0, y1) = visible_tile_range(0.5, 0.5, 256.0, 256.0, 1.0, 1);
        assert_eq!((x0, x1, y0, y1), (0, 0, 0, 0));
    }

    #[test]
    fn tile_range_widens_with_viewport() {
        let (nx0, nx1, ..) = visible_tile_range(8.0, 8.0, 512.0, 512.0, 1.0, 16);
        let (wx0, wx1, ..) = visible_tile_range(8.0, 8.0, 1024.0, 512.0, 1.0, 16);
        assert!(
            (wx1 - wx0) > (nx1 - nx0),
            "a wider viewport must request more columns"
        );
    }

    #[test]
    fn tile_range_clamps_at_grid_edges() {
        // Centre at the left/top edge: no negative indices.
        let (x0, _, y0, _) = visible_tile_range(0.0, 0.0, 512.0, 512.0, 1.0, 16);
        assert!(x0 >= 0 && y0 >= 0);
        // Centre at the right/bottom edge: never past tile_count-1.
        let (_, x1, _, y1) = visible_tile_range(15.0, 15.0, 512.0, 512.0, 1.0, 16);
        assert!(x1 <= 15 && y1 <= 15);
    }
}
