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
/// `OpenLayers` / Mapbox conventions (Web Mercator, origin top-left).
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
/// vector layer. Additional layers (heatmaps, custom `GeoJSON`) compose
/// as further `MapWidget` instances stacked atop.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct MapTileLayer {
    /// `{z}` / `{x}` / `{y}` placeholders are substituted at fetch
    /// time. Matches Leaflet's `tileLayer(url_template)`.
    pub url_template: AzString,
    /// Minimum integer zoom this layer supports.
    pub min_zoom: u8,
    /// Maximum integer zoom this layer supports.
    pub max_zoom: u8,
    /// Attribution string the user MUST display (`ODbL` "© OpenStreetMap
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
            // OpenFreeMap's public planet vector tiles (full-detail OSM, z0–14, no
            // API key). The tile path is VERSIONED by planet-build date — the
            // unversioned `/planet/{z}/{x}/{y}.pbf` returns empty tiles. The version
            // below is the current build from the TileJSON at
            // `https://tiles.openfreemap.org/planet` (`tiles[0]`); when OpenFreeMap
            // rebuilds the planet this goes stale, so the proper long-term path is to
            // resolve it on the background thread by fetching that TileJSON first (a
            // follow-up to the Leaflet-style layer work). Raster relief is also
            // available at `…/natural_earth/ne2sr/{z}/{x}/{y}.png` (z0–6).
            url_template: AzString::from(
                "https://tiles.openfreemap.org/planet/20260531_080002_pt/{z}/{x}/{y}.pbf",
            ),
            min_zoom: 0,
            max_zoom: 14,
            attribution: AzString::from(
                "© OpenFreeMap © OpenMapTiles · Data © OpenStreetMap contributors",
            ),
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

/// A geographic coordinate in degrees. Returned by
/// [`MapWidget::latlon_at_px`] and (P3) the map's `on_pin_tap` hook.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MapLatLon {
    pub lat_deg: f64,
    pub lon_deg: f64,
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
    /// Optional hook fired when the user taps the map, with the tapped
    /// lat/lon. FFI-exposed; re-set on each fresh build.
    pub on_pin_tap: OptionMapPinTap,
}

impl MapWidget {
    #[must_use] pub fn create(layer: MapTileLayer) -> Self {
        Self {
            layer,
            viewport: MapViewport::default(),
            container_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            on_viewport_changed: OptionMapViewportChanged::None,
            on_pin_tap: OptionMapPinTap::None,
        }
    }

    #[must_use] pub const fn with_viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    #[must_use] pub fn with_container_style(mut self, css: CssPropertyWithConditionsVec) -> Self {
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

    /// Set a hook fired when the user taps the map (a press + release at ~the
    /// same point, no drag), with the tapped lat/lon. The backreference DI
    /// pattern (architecture.md).
    pub fn set_on_pin_tap<C: Into<MapPinTapCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_pin_tap = Some(MapPinTap {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_pin_tap`](Self::set_on_pin_tap).
    pub fn with_on_pin_tap<C: Into<MapPinTapCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_pin_tap(data, callback);
        self
    }

    /// Project a screen pixel `px` (relative to the map node's top-left, in a
    /// node of size `container`) to a lat/lon on the map at `viewport`. Small-
    /// angle Mercator (accurate at city zooms). Inverse of
    /// [`px_at_latlon`](Self::px_at_latlon). Exposed so apps don't reimplement
    /// the projection (e.g. to drop a pin where the user tapped).
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn latlon_at_px(
        viewport: MapViewport,
        px: azul_core::geom::LogicalPosition,
        container: azul_core::geom::LogicalSize,
    ) -> MapLatLon {
        let world = 256.0_f64 * 2.0_f64.powf(f64::from(viewport.zoom));
        let dx = f64::from(px.x - container.width * 0.5);
        let dy = f64::from(px.y - container.height * 0.5);
        let lon = (viewport.centre_lon_deg + dx * 360.0 / world).clamp(-180.0, 180.0);
        let cos_lat = viewport.centre_lat_deg.to_radians().cos();
        let lat = (viewport.centre_lat_deg - dy * 360.0 / world * cos_lat).clamp(-85.0, 85.0);
        MapLatLon {
            lat_deg: lat,
            lon_deg: lon,
        }
    }

    /// Inverse of [`latlon_at_px`](Self::latlon_at_px): where `coord` lands in
    /// container pixels at `viewport`.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    #[must_use] pub fn px_at_latlon(
        viewport: MapViewport,
        coord: MapLatLon,
        container: azul_core::geom::LogicalSize,
    ) -> azul_core::geom::LogicalPosition {
        let world = 256.0_f64 * 2.0_f64.powf(f64::from(viewport.zoom));
        let cos_lat = viewport.centre_lat_deg.to_radians().cos();
        let px = f64::from(container.width) * 0.5
            + (coord.lon_deg - viewport.centre_lon_deg) * world / 360.0;
        let py = f64::from(container.height) * 0.5
            - (coord.lat_deg - viewport.centre_lat_deg) * world / (360.0 * cos_lat);
        azul_core::geom::LogicalPosition::new(px as f32, py as f32)
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
    #[must_use] pub fn dom(self) -> Dom {
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
    #[must_use] pub fn dom_with_fetch(self, cb: crate::thread::ThreadCallback) -> Dom {
        self.build_dom(Some(cb))
    }

    fn build_dom(self, fetch_cb: Option<crate::thread::ThreadCallback>) -> Dom {
        use azul_core::dom::{ComponentEventFilter, EventFilter, HoverEventFilter};

        let mut cache = MapTileCache::new(self.layer.clone(), self.viewport);
        cache.fetch_callback = fetch_cb;
        cache.on_viewport_changed = self.on_viewport_changed;
        cache.on_pin_tap = self.on_pin_tap;
        let dataset = RefAny::new(cache);
        let virtual_view_data = dataset.clone();

        let root = Dom::create_div()
            // Fill the container (the Leaflet contract) via absolute inset:0 rather
            // than height:100%. A percentage height only resolves against a parent
            // with a DEFINITE height; the usual map container is a `flex-grow` item
            // whose height is not definite for percentage children, so height:100%
            // there resolves to INFINITY → the VirtualView gets infinite bounds and
            // positions every tile at y=∞ (off-screen → blank map). Absolute inset:0
            // instead sizes against the container's final, finite content box. The
            // container MUST be a positioned box (the demo's `position: relative`);
            // a non-empty `container_style` (via `with_container_style`) overrides.
            .with_css("position: absolute; top: 0; left: 0; right: 0; bottom: 0; overflow: hidden;")
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
            .with_child(
                Dom::create_virtual_view(
                    virtual_view_data,
                    map_widget_render as azul_core::callbacks::VirtualViewCallbackType,
                )
                // Fill the widget div with a PERCENTAGE box (not absolute). The
                // outer div above is absolutely sized, so its height IS definite —
                // height:100% here resolves against it (441px), giving the
                // VirtualView a finite box. (Absolute-against-absolute collapses to
                // 0 in the solver; percentage-against-a-definite-parent does not.)
                .with_css("width: 100%; height: 100%; overflow: hidden;"),
            );

        // A caller-supplied container style replaces the default fill above
        // (`with_css_props` replaces the inline style) — the caller then owns sizing.
        if self.container_style.as_slice().is_empty() {
            root
        } else {
            root.with_css_props(self.container_style)
        }
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
    /// Pixel position of the last pointer-down (the original press point, not
    /// overwritten by pan moves). Used to tell a tap from a drag in pointer-up.
    pub press_origin: Option<azul_core::geom::LogicalPosition>,
    /// The user's `on_pin_tap` hook, copied from the builder so pointer-up can
    /// fire it. Carried across relayout.
    pub on_pin_tap: OptionMapPinTap,
}

impl MapTileCache {
    #[must_use] pub const fn new(layer: MapTileLayer, viewport: MapViewport) -> Self {
        Self {
            layer,
            viewport,
            tiles: BTreeMap::new(),
            fetch_callback: None,
            drag_anchor: None,
            pinch_anchor: None,
            press_origin: None,
            on_viewport_changed: OptionMapViewportChanged::None,
            on_pin_tap: OptionMapPinTap::None,
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

    /// Bound the tile cache by evicting tiles far from the current viewport.
    ///
    /// Without this, `tiles` grows without limit — panning across the world or
    /// zooming in and out keeps every tile ever fetched (each decoded SVG is
    /// tens-to-hundreds of KB), so a long session leaks memory. Called after a
    /// viewport change once the new view's tiles are queued.
    ///
    /// Eviction is viewport-distance based (the right policy for spatial data,
    /// stronger than plain LRU): each tile is scored by zoom mismatch + squared
    /// distance from the viewport centre (projected into the current zoom's tile
    /// space), and the farthest are dropped first. IN-FLIGHT tiles
    /// (`Pending`/`Fetching`) are never evicted (their worker would write into a
    /// gone entry), and on-screen tiles score near-zero so they survive.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
    pub fn prune_distant_tiles(&mut self) {
        const MAX_CACHED_TILES: usize = 192;
        if self.tiles.len() <= MAX_CACHED_TILES {
            return;
        }

        let z = (self.viewport.zoom.floor() as i32)
            .clamp(i32::from(self.layer.min_zoom), i32::from(self.layer.max_zoom))
            as u8;
        let tile_count = 1u32 << u32::from(z);
        let cx = lon_to_tile_x(self.viewport.centre_lon_deg, f64::from(tile_count));
        let cy = lat_to_tile_y(self.viewport.centre_lat_deg, f64::from(tile_count));

        // Higher score = evict sooner.
        let score = |id: &MapTileId| -> f64 {
            let zt_count = 1u32 << u32::from(id.z);
            // Project the tile's centre into the CURRENT zoom's tile space so
            // distances across zoom levels are comparable.
            let scale = f64::from(tile_count) / f64::from(zt_count);
            let tx = (f64::from(id.x) + 0.5) * scale;
            let ty = (f64::from(id.y) + 0.5) * scale;
            let dz = f64::from((i32::from(id.z) - i32::from(z)).abs());
            let dx = tx - cx;
            let dy = ty - cy;
            dz * 10_000.0 + dx * dx + dy * dy
        };

        let mut evictable: Vec<(f64, MapTileId)> = self
            .tiles
            .iter()
            .filter(|(_, e)| !matches!(e, TileEntry::Pending | TileEntry::Fetching))
            .map(|(id, _)| (score(id), *id))
            .collect();
        // Farthest first.
        evictable.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));

        let mut to_remove = self.tiles.len().saturating_sub(MAX_CACHED_TILES);
        for (_, id) in evictable {
            if to_remove == 0 {
                break;
            }
            self.tiles.remove(&id);
            to_remove -= 1;
        }
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
    /// string for now; the `VirtualView` callback will feed it
    /// through the framework's svg-to-dom pipeline on the next
    /// re-render.
    Ready { svg: AzString },
    /// Fetch failed. Held so the framework doesn't immediately
    /// re-try the same URL — caller can choose to clear failed
    /// entries on retry.
    Failed { error: AzString },
}

/// Worker-thread input: which tile to fetch, the resolved URL, and the
/// `MapCSS` stylesheet to apply when converting features to SVG. Boxed
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
    // SHARE the previous cache across the relayout — do NOT copy its tiles into
    // the freshly-built one. The tile-fetch worker threads each hold a clone of
    // THIS very `RefAny` (handed to them at spawn time); returning it keeps their
    // writebacks landing in the same cache the VirtualView reads. The reconcile
    // pass re-points the VirtualView node's `refany` at this returned dataset
    // (core::diff::transfer_states), so the pure content callback reads it too.
    //
    // The old behaviour returned a fresh `new_data` with the old tiles *copied*
    // in. That orphaned the workers' clone after the first relayout: every tile
    // arriving later was written into the old, no-longer-rendered cache, so the
    // map stayed blank. Returning the persistent (old) cache fixes it at the root
    // — workers, dataset and VirtualView all reference one underlying allocation.
    //
    // The freshly-built `new_data` carries the layout-callback-controlled
    // CONFIG: the fetch worker the `.dom()` shim wired, and — critically — the
    // viewport/layer the app passed to `with_viewport()` / `create()` for THIS
    // build. Adopt those into the persistent cache: app callbacks (zoom
    // buttons, Recentre, Locate) mutate app state and return RefreshDom, and
    // the merge previously discarded that new viewport ("viewport intact"),
    // so external viewport changes never took effect — only the widget's
    // internal drag/wheel (which mutate the persistent cache directly)
    // worked. Widget-internal changes stay consistent because every build's
    // `with_viewport()` receives the app state, which the on_viewport_changed
    // hook keeps in sync with internal pans/zooms.
    {
        let new_g = new_data.downcast_ref::<MapTileCache>();
        let old_guard = old_data.downcast_mut::<MapTileCache>();
        if let (Some(new_g), Some(mut old_g)) = (new_g, old_guard) {
            if old_g.fetch_callback.is_none() {
                old_g.fetch_callback = new_g.fetch_callback.clone();
            }
            old_g.viewport = new_g.viewport;
            old_g.layer = new_g.layer.clone();
            old_g.on_viewport_changed = new_g.on_viewport_changed.clone();
        }
    }
    old_data
}

// ────────── Pan + zoom callbacks ─────────────────────────────────────

use crate::callbacks::CallbackInfo;
use azul_core::callbacks::Update;
use azul_core::callbacks::TimerCallbackReturn;
use azul_core::task::{Duration, SystemTimeDiff, TerminateTimer, TimerId};
use crate::timer::{Timer, TimerCallback, TimerCallbackInfo};

// --- User hook: on_viewport_changed (backreference DI, FFI-exposed) ---

/// User hook fired when the user pans or zooms the map.
///
/// Lets app code observe
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
            (h.callback.cb)(h.refany.clone(), *info, viewport)
        }
        OptionMapViewportChanged::None => Update::DoNothing,
    }
}

// --- User hook: on_pin_tap (backreference DI, FFI-exposed) ---

/// User hook fired when the user taps the map (a press + release at ~the same
/// point, no pan/pinch).
///
/// Receives the tapped [`MapLatLon`] (projected via
/// [`MapWidget::latlon_at_px`]) so apps can drop a pin without wiring their own
/// tap handling + projection. The backreference DI pattern (architecture.md).
pub type MapPinTapCallbackType = extern "C" fn(RefAny, CallbackInfo, MapLatLon) -> Update;
impl_widget_callback!(
    MapPinTap,
    OptionMapPinTap,
    MapPinTapCallback,
    MapPinTapCallbackType
);
azul_core::impl_managed_callback! {
    wrapper:        MapPinTapCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MAP_PIN_TAP_INVOKER,
    invoker_ty:     AzMapPinTapCallbackInvoker,
    thunk_fn:       az_map_pin_tap_callback_thunk,
    setter_fn:      AzApp_setMapPinTapCallbackInvoker,
    from_handle_fn: AzMapPinTapCallback_createFromHostHandle,
    extra_args:     [ coord: MapLatLon ],
}

/// Invoke a map widget's optional `on_pin_tap` hook with the tapped coordinate.
fn invoke_pin_tap(hook: &OptionMapPinTap, info: &CallbackInfo, coord: MapLatLon) -> Update {
    match hook {
        OptionMapPinTap::Some(h) => (h.callback.cb)(h.refany.clone(), *info, coord),
        OptionMapPinTap::None => Update::DoNothing,
    }
}

/// Pointer down → record the drag anchor. The widget knows nothing
/// about the user's overall state `RefAny` — only its own dataset —
/// so the anchor lives in `MapTileCache::drag_anchor`.
extern "C" fn map_on_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    #[cfg(feature = "std")]
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map] pointer_down fired");
    }
    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => azul_core::geom::LogicalPosition::new(p.x, p.y),
        None => return Update::DoNothing,
    };
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        cache.drag_anchor = Some(pos);
        cache.press_origin = Some(pos);
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
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
extern "C" fn map_on_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    #[cfg(feature = "std")]
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        let dragging = data
            .downcast_ref::<MapTileCache>()
            .is_some_and(|c| c.drag_anchor.is_some());
        eprintln!("[map] pointer_move fired (dragging={dragging})");
    }
    // Active pinch wins over single-finger pan.
    if let Some(pinch) = info.get_pinch().into_option() {
        let Some(mut cache) = data.downcast_mut::<MapTileCache>() else {
            return Update::DoNothing;
        };
        let anchor = *cache.pinch_anchor.get_or_insert(pinch.current_distance);
        if anchor > 1.0 && pinch.current_distance > 1.0 {
            let dz = (pinch.current_distance / anchor).log2();
            let min = f32::from(cache.layer.min_zoom);
            let max = f32::from(cache.layer.max_zoom);
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
        // Re-render the VirtualView in place so the new zoom's tiles compute
        // immediately, without a DOM rebuild. (See map_tile_writeback for why
        // RefreshDom is avoided.)
        info.trigger_all_virtual_view_rerender();
        return Update::DoNothing;
    }

    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => azul_core::geom::LogicalPosition::new(p.x, p.y),
        None => return Update::DoNothing,
    };
    let Some(mut cache_guard) = data.downcast_mut::<MapTileCache>() else {
        return Update::DoNothing;
    };
    let Some(anchor) = cache_guard.drag_anchor else {
        return Update::DoNothing; // no active drag
    };

    let dx_px = f64::from(pos.x - anchor.x);
    let dy_px = f64::from(pos.y - anchor.y);
    if dx_px.abs() < 0.5 && dy_px.abs() < 0.5 {
        return Update::DoNothing;
    }

    let (new_lon, new_lat) = pan_viewport(
        cache_guard.viewport.centre_lat_deg,
        cache_guard.viewport.centre_lon_deg,
        f64::from(cache_guard.viewport.zoom),
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
    // Pan moved the viewport — re-render the VirtualView in place so the newly
    // visible tiles are computed (and marked Pending) right away. No RefreshDom.
    info.trigger_all_virtual_view_rerender();
    Update::DoNothing
}

/// Pointer up / pointer leave → end the drag *and* the pinch. Either
/// can be in flight (and pinch supersedes pan in the move handler);
/// clear both anchors on release.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
extern "C" fn map_on_pointer_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // Cursor + container size for tap projection (read before borrowing data).
    let up_pos = info
        .get_cursor_relative_to_node()
        .into_option()
        .map(|p| azul_core::geom::LogicalPosition::new(p.x, p.y));
    let container = info
        .get_hit_node_rect()
        .map_or(azul_core::geom::LogicalSize::new(0.0, 0.0), |r| r.size);
    let (press, viewport, hook) = data.downcast_mut::<MapTileCache>().map_or_else(|| (None, MapViewport::default(), OptionMapPinTap::None), |mut cache| {
            let out = (cache.press_origin, cache.viewport, cache.on_pin_tap.clone());
            cache.drag_anchor = None;
            cache.pinch_anchor = None;
            cache.press_origin = None;
            out
        });
    // A press + release at ~the same point (no pan/pinch) is a tap: project it
    // to lat/lon and fire the user's on_pin_tap hook.
    if let (Some(origin), Some(up)) = (press, up_pos) {
        let dx = f64::from(up.x - origin.x);
        let dy = f64::from(up.y - origin.y);
        if dx * dx + dy * dy < 36.0 {
            let coord = MapWidget::latlon_at_px(viewport, up, container);
            invoke_pin_tap(&hook, &info, coord);
        }
    }
    // After a pan / pinch settles, kick off fetches for any tiles the new
    // viewport needs. (Only a `CallbackInfo`-bearing callback can spawn them.)
    spawn_pending_tile_fetches(&mut data, &mut info);
    // Re-render in place so Fetching/Ready states show as tiles arrive. The
    // worker writebacks will trigger further re-renders themselves. No RefreshDom.
    info.trigger_all_virtual_view_rerender();
    Update::DoNothing
}

/// Mouse-wheel / trackpad scroll over the map = ZOOM (Leaflet / Google-Maps
/// convention), not content scroll. The map's `VirtualView` has no scroll overflow,
/// so the framework's queued wheel deltas would otherwise be wasted — drain them
/// and apply as a zoom step, then queue + spawn the tiles the new zoom needs and
/// re-render in place.
extern "C" fn map_on_scroll(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // Wheel delta that triggered this Scroll callback (sign = direction). The map
    // is not a scroll container, so this comes from the per-pass wheel delta, not
    // the scroll-physics input queue (which only feeds scrollable nodes).
    let dy: f32 = {
        let hn = info.get_hit_node();
        hn.node.into_crate_internal().map_or(0.0, |nid| info.get_scroll_delta(hn.dom, nid).map_or(0.0, |d| d.y))
    };
    #[cfg(feature = "std")]
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map] scroll fired dy={dy}");
    }
    if dy == 0.0 {
        return Update::DoNothing;
    }
    // The grid's on-screen rect is the widget size (needed to recompute the tiles
    // the new zoom needs).
    let bounds = info
        .get_hit_node_rect()
        .map_or(azul_core::geom::LogicalSize::new(0.0, 0.0), |r| r.size);
    let (vp, hook) = {
        let Some(mut cache) = data.downcast_mut::<MapTileCache>() else {
            return Update::DoNothing;
        };
        let min = f32::from(cache.layer.min_zoom);
        let max = f32::from(cache.layer.max_zoom);
        // ~0.5 zoom levels per wheel notch. X11 delivers wheel-up as dy > 0;
        // wheel-up zooms IN, wheel-down zooms OUT (Leaflet / Google-Maps).
        let dz = dy.signum() * 0.5;
        cache.viewport.zoom = (cache.viewport.zoom + dz).clamp(min, max);
        let vp = cache.viewport;
        let layer = cache.layer.clone();
        for t in map_visible_tiles(&vp, bounds, &layer) {
            cache.tiles.entry(t).or_insert(TileEntry::Pending);
        }
        (vp, cache.on_viewport_changed.clone())
    };
    invoke_viewport_changed(&hook, &info, vp);
    spawn_pending_tile_fetches(&mut data, &mut info);
    info.trigger_all_virtual_view_rerender();
    Update::DoNothing
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
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
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
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
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
/// the framework's existing XML→DOM path.
///
/// The SVG is wrapped in a
/// minimal `<html><body>` envelope because `str_to_dom_unstyled`
/// expects a document root; the wrapper divs are zero-impact in
/// layout. Returns `None` if the `xml` feature is off or parsing
/// fails — the caller then falls back to the placeholder glyph.
// Render the decoded tile SVG to a COLOUR image node, reusing the framework's
// `render_svg_group` rasteriser (the one that renders the tiger), which honours
// the SVG `fill`/`stroke` attrs that `features_to_svg` emits. The DOM SVG path
// (`str_to_dom_unstyled` → `SvgNodeData::Path`) only produces a clip mask, so it
// cannot paint the feature colours — hence the tiles rendered grey.
#[cfg(all(feature = "xml", feature = "cpurender"))]
#[must_use] pub fn svg_string_to_dom(svg: &str) -> Option<Dom> {
    let img = crate::cpurender::render_svg_to_imageref(svg.as_bytes(), 256, 256).ok()?;
    Some(
        Dom::create_image(img)
            .with_css("position: absolute; left: 0; top: 0; width: 100%; height: 100%;"),
    )
}

#[cfg(all(feature = "xml", not(feature = "cpurender")))]
pub fn svg_string_to_dom(svg: &str) -> Option<Dom> {
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
/// (The `VirtualView` marks the viewport's tiles `Pending` during the
/// layout pass that precedes mount-event dispatch; this handler then
/// spawns the workers for them.) Returns `RefreshDom` so the
/// `Fetching` state shows immediately.
extern "C" fn map_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    #[cfg(feature = "std")]
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map] after_mount fired");
    }
    spawn_pending_tile_fetches(&mut data, &mut info);
    // Install a low-frequency sweep timer. Pointer/scroll/after_mount spawn
    // fetches directly, but a viewport change that originates from a *rebuild*
    // (an app's zoom/recentre button → with_viewport) marks new tiles `Pending`
    // in the VirtualView render, which has no `add_thread` — so without this
    // sweep the map would sit grey after a button-zoom until the next
    // drag/wheel. The timer's cache clone tracks the persistent dataset
    // `transfer_states` keeps across rebuilds, so it stays unified.
    let sweep = Timer::create(
        data.clone(),
        TimerCallback::create(map_fetch_sweep_tick),
        info.get_system_time_fn(),
    )
    .with_interval(Duration::System(SystemTimeDiff::from_millis(250)));
    info.add_timer(TimerId::unique(), sweep);
    // Re-render the VirtualView IN PLACE (not RefreshDom). RefreshDom would
    // rebuild the DOM, allocate a fresh MapTileCache, and orphan the clone of
    // the cache we just handed the worker threads — their tiles would then write
    // to a cache nobody renders. The dataset is shared via the construction-time
    // RefAny::clone(), so re-invoking in place lets the workers' writes land in
    // the same cache the VirtualView reads.
    info.trigger_all_virtual_view_rerender();
    Update::DoNothing
}

/// Scan the cache for `Pending` tiles and spawn one framework `Thread`
/// per tile (capped per call so a big viewport jump doesn't spawn
/// hundreds at once). Each thread gets:
/// - init `RefAny` = `TileFetchInit { tile, url }`
/// - writeback `RefAny` = a clone of the cache dataset, so
///   `map_tile_writeback` mutates the same cache the `VirtualView` reads.
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
        let Some(mut cache) = data.downcast_mut::<MapTileCache>() else {
            return;
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
        // Now that the current view's tiles are queued (Fetching, so eviction
        // protects them), bound the cache by dropping tiles far from the
        // viewport — otherwise panning/zooming grows it without limit.
        cache.prune_distant_tiles();
    }

    let cb = {
        let Some(cache) = data.downcast_ref::<MapTileCache>() else {
            return;
        };
        match cache.fetch_callback.as_ref() {
            Some(cb) => cb.clone(),
            None => return,
        }
    };

    #[cfg(feature = "std")]
    let spawn_count = to_spawn.len();
    for init in to_spawn {
        let init_data = RefAny::new(init);
        let writeback_data = data.clone(); // same cache dataset
        let thread = Thread::create(init_data, writeback_data, cb.clone());
        info.add_thread(ThreadId::unique(), thread);
    }
    #[cfg(feature = "std")]
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map] spawn_pending: {spawn_count} thread(s) spawned");
    }
}

/// Low-frequency timer that spawns fetches for any `Pending` tiles the
/// `VirtualView` marked since the last spawn — the path that the
/// `pointer/scroll/after_mount` handlers can't cover (a rebuild-driven viewport
/// change marks tiles `Pending` in the `VirtualView` render, which has no
/// `add_thread`). Installed once in `map_on_after_mount`. The `data` clone
/// tracks the persistent dataset, so writebacks land in the rendered cache.
/// Cheap no-op when nothing is `Pending`; never `RefreshDom`s (that would
/// orphan the cache the workers write to — tile writebacks drive re-render).
extern "C" fn map_fetch_sweep_tick(
    mut data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    spawn_pending_tile_fetches(&mut data, &mut info.callback_info);
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Continue,
    }
}

/// `{z}/{x}/{y}` substitution. Mirrors `azul_dll`'s `build_tile_url`
/// (the widget can't reach the dll, so it's duplicated here — trivial).
fn build_tile_url(template: &str, tile: MapTileId) -> String {
    use alloc::string::ToString;
    template
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Worker-thread → main-thread writeback.
///
/// `cache_dataset` is the
/// `writeback_data` handed to `Thread::create` (the same
/// `MapTileCache` the widget reads); `incoming` is the `TileReadyMsg`
/// the worker sent. Stamps the tile `Ready` (or `Failed`) and asks for
/// a relayout so the `VirtualView` renders the new content.
#[must_use] pub extern "C" fn map_tile_writeback(
    mut cache_dataset: RefAny,
    mut incoming: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let msg = match incoming.downcast_ref::<TileReadyMsg>() {
        Some(m) => (m.tile, m.svg.clone(), m.error.clone()),
        None => return Update::DoNothing,
    };
    {
        let Some(mut cache) = cache_dataset.downcast_mut::<MapTileCache>() else {
            return Update::DoNothing;
        };
        #[cfg(feature = "std")]
        if std::env::var("AZ_MAP_DEBUG").is_ok() {
            eprintln!(
                "[map] writeback tile=({},{},{}) ok={} svg_len={} err={:?}",
                msg.0.z, msg.0.x, msg.0.y,
                msg.2.as_str().is_empty(), msg.1.as_str().len(), msg.2.as_str()
            );
        }
        if msg.2.as_str().is_empty() {
            cache.mark_tile_ready(msg.0, msg.1);
        } else {
            cache.mark_tile_failed(msg.0, msg.2);
        }
    } // drop the cache borrow before touching `info`

    // Re-render the VirtualView(s) IN PLACE so the pure content callback re-reads
    // the shared cache we just mutated. NOT `RefreshDom`: a DOM rebuild would
    // allocate a fresh `MapTileCache` and orphan THIS worker's clone of it (the
    // VirtualView's `refany`, the node dataset and the worker's writeback handle
    // are all clones of one `RefAny` — same underlying data — only while the DOM
    // is not rebuilt). Re-invoking in place keeps that share intact, so this tile
    // and every later one reach the rendered view.
    info.trigger_all_virtual_view_rerender();
    Update::DoNothing
}

/// Inclusive `(x_min, x_max, y_min, y_max)` tile range covering a
/// `width_px × height_px` viewport centred at tile-space `(centre_x,
/// centre_y)`, at fractional `zoom_scale` and integer `tile_count` (2^z).
/// A one-tile margin (`+ 1.0`) is added each side so a tile scrolling into
/// view is already requested; the result is clamped to the valid
/// `0..=tile_count-1` grid. The pure core of `map_widget_render`'s grid
/// loop — what decides which tiles get fetched.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded layout/render numeric cast
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
    // x is NOT clamped: the map wraps horizontally. Callers take the tile id mod
    // `tile_count` (so a column past the antimeridian shows the far side of the
    // world) while positioning the div at the un-wrapped column — seamless pan
    // across ±180° with no empty gutter. y IS clamped: there is no data beyond
    // the Web-Mercator poles, so vertical over-scan must not request bogus rows.
    let x_min = (centre_x - half_w).floor() as i32;
    let x_max = (centre_x + half_w).ceil() as i32;
    let y_min = ((centre_y - half_h).floor() as i32).max(0);
    let y_max = ((centre_y + half_h).ceil() as i32).min(max_idx);
    (x_min, x_max, y_min, y_max)
}

/// Wrap a (possibly negative or over-range) tile column into the valid
/// `0..tile_count` band — the horizontal world-wrap. `rem_euclid` (not `%`)
/// so columns west of the antimeridian map to the east side: at `tile_count`
/// = 4, column `-1` → `3`, column `4` → `0`.
#[allow(clippy::cast_possible_wrap)] // bounded layout/render numeric cast
fn wrap_tile_x(x: i32, tile_count: u32) -> u32 {
    x.rem_euclid(tile_count.max(1) as i32) as u32
}

/// `f(view)` — the tile ids a `viewport` needs to fill a `bounds`-sized widget.
/// Shared by the `VirtualView` render and the pan/zoom handlers so a handler can
/// mark + spawn the NEW viewport's tiles immediately, rather than waiting for the
/// next render pass to discover them. Mirrors `map_widget_render`'s grid math.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn map_visible_tiles(
    viewport: &MapViewport,
    bounds: azul_core::geom::LogicalSize,
    layer: &MapTileLayer,
) -> Vec<MapTileId> {
    let z_int =
        (viewport.zoom.floor() as i32).clamp(i32::from(layer.min_zoom), i32::from(layer.max_zoom)) as u8;
    let tile_count = 1u32 << u32::from(z_int);
    let frac_zoom = viewport.zoom - f32::from(z_int);
    let zoom_scale = 2.0_f32.powf(frac_zoom);
    let centre_x = lon_to_tile_x(viewport.centre_lon_deg, f64::from(tile_count)) as f32;
    let centre_y = lat_to_tile_y(viewport.centre_lat_deg, f64::from(tile_count)) as f32;
    let (x_min, x_max, y_min, y_max) =
        visible_tile_range(centre_x, centre_y, bounds.width, bounds.height, zoom_scale, tile_count);
    let mut tiles = Vec::new();
    for x in x_min..=x_max {
        for y in y_min..=y_max {
            tiles.push(MapTileId { z: z_int, x: wrap_tile_x(x, tile_count), y: y as u32 });
        }
    }
    tiles
}

// ────────── VirtualView callback — visible-tile rendering ─────────────

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)] // bounded layout/render numeric cast
extern "C" fn map_widget_render(
    data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    enum TileDisplay {
        Glyph(&'static str),
        Svg(AzString),
    }
    let mut data = data;
    let bounds = info.get_bounds();
    let bounds_logical = bounds.get_logical_size();
    let width_px = bounds_logical.width;
    let height_px = bounds_logical.height;

    // Defensive: if the widget was placed in a container that gives it no definite
    // size, the bounds come through as 0 or non-finite. Computing a tile grid then
    // positions tiles at NaN/∞ (off-screen → blank) and can allocate unboundedly, so
    // render nothing until the layout settles to a finite box.
    if !width_px.is_finite() || !height_px.is_finite() || width_px <= 0.0 || height_px <= 0.0 {
        if std::env::var("AZ_MAP_DEBUG").is_ok() {
            eprintln!("[map] non-finite bounds {width_px}x{height_px} — skipping render");
        }
        return VirtualViewReturn {
            dom: OptionDom::None,
            scroll_size: bounds_logical,
            scroll_offset: azul_core::geom::LogicalPosition::zero(),
            virtual_scroll_size: bounds_logical,
            virtual_scroll_offset: azul_core::geom::LogicalPosition::zero(),
        };
    }

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
        .clamp(i32::from(layer.min_zoom), i32::from(layer.max_zoom))
        as u8;
    let tile_count = 1u32 << u32::from(z_int);
    let frac_zoom = viewport.zoom - f32::from(z_int);
    let zoom_scale = 2.0_f32.powf(frac_zoom);

    // Convert WGS-84 → Web-Mercator-XYZ tile-space via the shared
    // projection helpers (the single source of truth, unit-tested below).
    let centre_x = lon_to_tile_x(viewport.centre_lon_deg, f64::from(tile_count)) as f32;
    let centre_y = lat_to_tile_y(viewport.centre_lat_deg, f64::from(tile_count)) as f32;

    // 256 is the Mercator tile pixel size at integer zoom; tile_px is also
    // used below to position each tile div.
    let tile_px = 256.0 * zoom_scale;
    let (x_min, x_max, y_min, y_max) =
        visible_tile_range(centre_x, centre_y, width_px, height_px, zoom_scale, tile_count);

    // Opt-in render trace (`AZ_MAP_DEBUG=1`): the VirtualView callback fires only
    // when the framework finds this node with real bounds — so seeing this line at
    // all confirms invocation, and the values reveal a zero / infinite / off-screen
    // grid (the usual causes of a blank map).
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!(
            "[map] render bounds={:.0}x{:.0} z={} centre_tile=({:.2},{:.2}) tiles x{}..{} y{}..{} = {}",
            width_px, height_px, z_int, centre_x, centre_y, x_min, x_max, y_min, y_max,
            (x_max - x_min + 1).max(0) * (y_max - y_min + 1).max(0)
        );
    }

    // Patch in any missing tiles as `Pending`. Real fetch dispatch
    // lands in the follow-up tick that adds the HTTP client; for now
    // we just track which tiles the viewport needs.
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        for x in x_min..=x_max {
            for y in y_min..=y_max {
                let id = MapTileId {
                    z: z_int,
                    x: wrap_tile_x(x, tile_count),
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

    // Pan / zoom handlers live HERE, on the VirtualView content — NOT on the
    // outer widget div. The VirtualView renders as a separate DomId painted on
    // top of the outer div, so pointer events hit-test to these tiles and never
    // bubble to the outer div's handlers (which is why mouse-drag panning did
    // nothing). `data` is the shared cache the handlers mutate; the in-place
    // re-render they trigger re-reads it.
    {
        use crate::callbacks::{Callback, CallbackType};
        use azul_core::dom::{EventFilter, HoverEventFilter};
        grid = grid
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                data.clone(),
                Callback::from(map_on_pointer_down as CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                data.clone(),
                Callback::from(map_on_pointer_move as CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                Callback::from(map_on_pointer_up as CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                data.clone(),
                Callback::from(map_on_pointer_up as CallbackType),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Scroll),
                data.clone(),
                Callback::from(map_on_scroll as CallbackType),
            );
    }

    for x in x_min..=x_max {
        for y in y_min..=y_max {
            // Tile id wraps horizontally (the column past ±180° shows the far
            // side of the world); the *screen* position uses the raw un-wrapped
            // column so the wrapped tile lands seamlessly in the gutter.
            let id = MapTileId {
                z: z_int,
                x: wrap_tile_x(x, tile_count),
                y: y as u32,
            };
            let screen_x =
                ((x as f32 - centre_x) * tile_px + width_px * 0.5).round() as i32;
            let screen_y =
                ((y as f32 - centre_y) * tile_px + height_px * 0.5).round() as i32;
            let size_px = tile_px.round().max(1.0) as i32;

            // Placeholder (still-loading) tiles show the loading grid — a grey
            // background + 1px border — so fetch state is visible. A LOADED tile
            // drops that chrome entirely: the decoded SVG covers the tile, and
            // keeping the per-tile border would draw a grey seam-grid over the
            // whole map (user-reported "small grey borders around the tiles").
            let is_ready = matches!(states.get(&id), Some(TileDisplay::Svg(_)));
            let chrome = if is_ready {
                ""
            } else {
                "background: #e7e9ec; border: 1px solid #d0d4d9;"
            };
            let style = alloc::format!(
                "position: absolute; left: {screen_x}px; top: {screen_y}px; \
                 width: {size_px}px; height: {size_px}px; {chrome}"
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
                            Dom::create_text(alloc::format!("✓? z{z_int}/{x}/{y}"))
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
                        Dom::create_text(alloc::format!("{state_tag} z{z_int}/{x}/{y}"))
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
    #[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
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
        assert!((-85.0..=85.0).contains(&lat_north));
        let (_, lat_south) = pan_viewport(-84.0, 0.0, 0.0, 0.0, -1.0e6);
        assert!((-85.0..=85.0).contains(&lat_south));
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
    fn merge_shares_old_cache_so_worker_writebacks_survive_relayout() {
        // THE regression behind the blank map: the merge must SHARE the previous
        // cache (the very `RefAny` the fetch-worker threads cloned at spawn), not
        // copy its tiles into a freshly-built one. With a copy, a tile that writes
        // back AFTER a relayout lands in the orphaned old cache and never renders.
        // Here we prove a post-merge writeback through a retained handle is
        // visible in the merged cache — i.e. they are one shared allocation.
        let tile = MapTileId { z: 5, x: 1, y: 2 };
        let old_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(5.0));
        let old_ref = RefAny::new(old_cache);
        // A worker thread keeps THIS clone and writes into it after the relayout.
        let mut worker_handle = old_ref.clone();
        // dom() rebuilds a fresh, empty cache (default viewport) each relayout.
        let new_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(9.0));

        let mut merged = merge_map_tile_cache(RefAny::new(new_cache), old_ref);

        // Worker finishes a fetch AFTER the merge and stamps the tile Ready on its
        // retained handle...
        worker_handle
            .downcast_mut::<MapTileCache>()
            .unwrap()
            .mark_tile_ready(tile, AzString::from("<svg/>"));

        // ...and it IS visible through the merged cache (shared storage). With the
        // old copy-merge this assertion failed — the tile was stranded.
        let g = merged.downcast_ref::<MapTileCache>().unwrap();
        assert!(
            g.tiles.contains_key(&tile),
            "a worker writeback after relayout must reach the rendered cache"
        );
    }

    #[test]
    fn merge_adopts_build_viewport_but_keeps_tiles() {
        // CONTRACT (changed 2026-06-10): `with_viewport()` is authoritative on
        // every rebuild. App callbacks (zoom buttons / Recentre / Locate)
        // mutate app state and RefreshDom; the old merge kept the persistent
        // cache's viewport "intact", silently discarding those changes — the
        // demo's +/− buttons fired but did nothing. Widget-internal drags stay
        // consistent because the on_viewport_changed hook mirrors them into
        // app state, which the next build passes back via with_viewport().
        // Tiles and the fetch worker stay with the persistent cache: workers
        // hold clones of that very RefAny, so writebacks keep landing in it.
        let mut old_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(5.0));
        old_cache.viewport.zoom = 7.0; // internal state from previous frames
        let tile = MapTileId { z: 2, x: 1, y: 1 };
        old_cache.tiles.insert(tile, TileEntry::Ready { svg: "<svg/>".into() });

        let new_cache = MapTileCache::new(MapTileLayer::default(), viewport_at(2.0));

        let mut merged =
            merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        let g = merged.downcast_ref::<MapTileCache>().unwrap();
        // The build's viewport wins…
        approx(f64::from(g.viewport.zoom), f64::from(viewport_at(2.0).zoom), 1e-6);
        // …while the fetched tiles survive in the same allocation.
        assert!(
            g.tiles.contains_key(&tile),
            "fetched tiles must survive the merge (workers write into this cache)"
        );
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
    fn wrap_tile_x_wraps_both_directions() {
        // rem_euclid semantics: west of the antimeridian wraps to the east side.
        assert_eq!(wrap_tile_x(-1, 4), 3);
        assert_eq!(wrap_tile_x(0, 4), 0);
        assert_eq!(wrap_tile_x(3, 4), 3);
        assert_eq!(wrap_tile_x(4, 4), 0);
        assert_eq!(wrap_tile_x(-5, 4), 3);
        // Single-tile world: every column resolves to the one tile.
        assert_eq!(wrap_tile_x(7, 1), 0);
        assert_eq!(wrap_tile_x(-3, 1), 0);
    }

    #[test]
    fn tile_range_y_clamps_but_x_wraps_at_zoom0() {
        // zoom 0 → tile_count 1. y stays pinned to row 0 (no data past the
        // poles); x is unclamped (the column over-scans to fill the width) but
        // every column wraps to the single tile.
        let (x0, x1, y0, y1) = visible_tile_range(0.5, 0.5, 256.0, 256.0, 1.0, 1);
        assert_eq!((y0, y1), (0, 0));
        for x in x0..=x1 {
            assert_eq!(wrap_tile_x(x, 1), 0);
        }
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
    fn tile_range_clamps_y_but_wraps_x_at_edges() {
        // y is clamped to the valid band at both poles (no over-scan past the
        // Web-Mercator edges)…
        let (x0, _, y0, _) = visible_tile_range(0.0, 0.0, 512.0, 512.0, 1.0, 16);
        assert!(y0 >= 0);
        let (_, x1, _, y1) = visible_tile_range(15.0, 15.0, 512.0, 512.0, 1.0, 16);
        assert!(y1 <= 15);
        // …but x is unclamped so the world wraps: a west-edge viewport over-scans
        // into negative columns and an east-edge one past tile_count-1; both wrap
        // back into 0..tile_count via wrap_tile_x.
        assert!(x0 < 0, "west-edge viewport should over-scan into wrapped columns");
        assert!(x1 > 15, "east-edge viewport should over-scan into wrapped columns");
        assert_eq!(wrap_tile_x(x0, 16), x0.rem_euclid(16) as u32);
        assert_eq!(wrap_tile_x(x1, 16), x1.rem_euclid(16) as u32);
    }

    fn test_cache() -> MapTileCache {
        let layer = MapTileLayer {
            url_template: AzString::from("{z}/{x}/{y}"),
            min_zoom: 0,
            max_zoom: 19,
            attribution: AzString::from(""),
            style_css: AzString::from(""),
        };
        let viewport = MapViewport {
            centre_lat_deg: 0.0,
            centre_lon_deg: 0.0,
            zoom: 4.0,
            bearing_deg: 0.0,
            pitch_deg: 0.0,
        };
        MapTileCache::new(layer, viewport)
    }

    #[test]
    fn prune_evicts_distant_tiles_keeps_near_and_inflight() {
        let mut cache = test_cache();
        // Centre at z4 is tile (8, 8). Fill a big z4 grid (Ready) — far more than
        // the 192 cap — plus a near Pending tile and a near Ready tile.
        for x in 0..20u32 {
            for y in 0..20u32 {
                cache
                    .tiles
                    .insert(MapTileId { z: 4, x, y }, TileEntry::Ready { svg: AzString::from("<svg/>") });
            }
        }
        // A near, in-flight tile (must NEVER be evicted).
        cache.tiles.insert(MapTileId { z: 4, x: 8, y: 8 }, TileEntry::Pending);
        // A near, ready tile (should survive — low distance score).
        cache.tiles.insert(MapTileId { z: 4, x: 9, y: 8 }, TileEntry::Ready { svg: AzString::from("<svg/>") });
        // A very far ready tile (should be evicted first).
        cache.tiles.insert(MapTileId { z: 4, x: 0, y: 0 }, TileEntry::Ready { svg: AzString::from("<svg/>") });

        assert!(cache.tiles.len() > 192, "precondition: over the cap");
        cache.prune_distant_tiles();

        assert!(cache.tiles.len() <= 192, "cache must be bounded after prune");
        // In-flight tile survives.
        assert!(matches!(
            cache.tiles.get(&MapTileId { z: 4, x: 8, y: 8 }),
            Some(TileEntry::Pending)
        ));
        // Near tile survives; the corner tile is gone.
        assert!(cache.tiles.contains_key(&MapTileId { z: 4, x: 9, y: 8 }));
        assert!(!cache.tiles.contains_key(&MapTileId { z: 4, x: 0, y: 0 }));
    }

    #[test]
    fn prune_is_noop_under_cap() {
        let mut cache = test_cache();
        for x in 0..4u32 {
            cache
                .tiles
                .insert(MapTileId { z: 4, x, y: 8 }, TileEntry::Ready { svg: AzString::from("<svg/>") });
        }
        cache.prune_distant_tiles();
        assert_eq!(cache.tiles.len(), 4, "under the cap → nothing evicted");
    }
}
