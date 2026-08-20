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
//!   subtree) on top of the map widget - the widget doesn't bake in
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

/// Configuration of one map tile layer - usually the base raster /
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
    #[must_use]
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
    #[must_use]
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
    /// No tile-fetch worker is wired - tiles render as placeholders.
    /// Use [`dom_with_fetch`](Self::dom_with_fetch) to supply one.
    ///
    /// The FFI `MapWidget::dom()` does NOT land here: api.json routes it to
    /// `azul_dll::unified::map::map_widget_dom`, which calls `dom_with_fetch`
    /// with the built-in worker. It used to route here, which is why the map
    /// panned but never painted a tile on every desktop platform.
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
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_map_tile_cache))
            // AfterMount fires once when the widget first appears (and
            // again after a DOM-structure change re-mounts it). It's the
            // earliest point with a `CallbackInfo`, so we kick the
            // initial tile fetches here — without it the first frame's
            // tiles would stay `Pending` until the user panned/tapped.
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_after_mount),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_down),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_move),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_up),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_up),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchStart),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_down),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchMove),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_move),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_up),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchCancel),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_up),
            )
            // Native gesture events (UIPinchGestureRecognizer on iOS,
            // ScaleGestureDetector on Android, NSMagnificationGestureRecognizer
            // on macOS) — fire through the same map_on_pointer_move handler
            // which reads `info.get_pinch()` and applies the zoom delta.
            .with_callback(
                EventFilter::Hover(HoverEventFilter::PinchIn),
                dataset.clone(),
                crate::callbacks::Callback::from_ptr(map_on_pointer_move),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::PinchOut),
                dataset,
                crate::callbacks::Callback::from_ptr(map_on_pointer_move),
            )
            .with_child(
                Dom::create_virtual_view(
                    virtual_view_data,
                    azul_core::callbacks::VirtualViewCallback::create(map_widget_render),
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
    /// Pinch reference distance (pixels) - the two-finger separation
    /// the last time a pinch event was observed for this widget.
    /// `Some` while a pinch is in flight, `None` between gestures.
    /// On each subsequent pinch update we compute
    /// `dz = log2(current_distance / pinch_anchor)` and add it to
    /// `viewport.zoom`, then reset the anchor to the current
    /// distance - so the gesture stays continuous across many frames.
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
    /// Without this, `tiles` grows without limit - panning across the world or
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
    /// re-try the same URL - caller can choose to clear failed
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
                old_g.fetch_callback.clone_from(&new_g.fetch_callback);
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
/// about the user's overall state `RefAny` - only its own dataset -
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
/// pan branch is skipped and the move event drives zoom instead -
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
/// so the framework's queued wheel deltas would otherwise be wasted - drain them
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
/// fails - the caller then falls back to the placeholder glyph.
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
    // A tile/page SVG arrived and this build cannot turn it into a DOM — the
    // caller sees a permanent None, indistinguishable from a bad SVG.
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][svg] svg_string_to_dom called, but this build has no `xml` \
             feature — SVG-to-DOM always returns None. Rebuild azul-layout with \
             the `xml` feature"
        );
    });
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
            // The dataset handed to this callback is not a MapTileCache. Every
            // tile stays Pending forever and the map shows placeholders — the
            // exact symptom of "panning works, tiles never paint".
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                std::eprintln!("[map] spawn_pending: ABORT — dataset is not a MapTileCache");
            }
            return;
        };
        if cache.fetch_callback.is_none() {
            // No worker wired. Either the build has no `map-tiles` feature, or
            // the callback was lost when the cache RefAny was rebuilt (the
            // merge callback is what preserves it across relayout).
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                std::eprintln!(
                    "[map] spawn_pending: ABORT — no fetch_callback on the cache \
                     ({} tiles held)",
                    cache.tiles.len()
                );
            }
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
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                std::eprintln!("[map] spawn_pending: ABORT — dataset vanished before spawn");
            }
            return;
        };
        match cache.fetch_callback.as_ref() {
            Some(cb) => cb.clone(),
            None => {
                #[cfg(feature = "std")]
                if std::env::var("AZ_MAP_DEBUG").is_ok() {
                    std::eprintln!("[map] spawn_pending: ABORT — fetch_callback gone at spawn");
                }
                return;
            }
        }
    };

    #[cfg(feature = "std")]
    let spawn_count = to_spawn.len();
    // Distinguish "nothing to do" from "never got here" — a quiet log and an
    // aborted one look identical otherwise.
    #[cfg(feature = "std")]
    if spawn_count == 0 && std::env::var("AZ_MAP_DEBUG").is_ok() {
        std::eprintln!("[map] spawn_pending: 0 tiles were Pending (nothing to spawn)");
    }
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
/// `VirtualView` marked since the last spawn - the path that the
/// `pointer/scroll/after_mount` handlers can't cover (a rebuild-driven viewport
/// change marks tiles `Pending` in the `VirtualView` render, which has no
/// `add_thread`). Installed once in `map_on_after_mount`. The `data` clone
/// tracks the persistent dataset, so writebacks land in the rendered cache.
/// Cheap no-op when nothing is `Pending`; never `RefreshDom`s (that would
/// orphan the cache the workers write to - tile writebacks drive re-render).
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
/// (the widget can't reach the dll, so it's duplicated here - trivial).
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
        None => {
            // The worker sent something that is not a TileReadyMsg: the tile
            // arrived and is dropped on the floor here.
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                std::eprintln!("[map] writeback: DROPPED — payload is not a TileReadyMsg");
            }
            return Update::DoNothing;
        }
    };
    {
        let Some(mut cache) = cache_dataset.downcast_mut::<MapTileCache>() else {
            // The tile came back but the dataset it targets is no longer a
            // MapTileCache — typically a rebuild replaced it. The fetch
            // succeeded and the pixels are still discarded.
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                std::eprintln!(
                    "[map] writeback: DROPPED tile=({},{},{}) — target dataset is not a MapTileCache",
                    msg.0.z, msg.0.x, msg.0.y
                );
            }
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
/// `width_px x height_px` viewport centred at tile-space `(centre_x,
/// centre_y)`, at fractional `zoom_scale` and integer `tile_count` (2^z).
/// A one-tile margin (`+ 1.0`) is added each side so a tile scrolling into
/// view is already requested; the result is clamped to the valid
/// `0..=tile_count-1` grid. The pure core of `map_widget_render`'s grid
/// loop - what decides which tiles get fetched.
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
/// `0..tile_count` band - the horizontal world-wrap. `rem_euclid` (not `%`)
/// so columns west of the antimeridian map to the east side: at `tile_count`
/// = 4, column `-1` → `3`, column `4` → `0`.
#[allow(clippy::cast_possible_wrap)] // bounded layout/render numeric cast
fn wrap_tile_x(x: i32, tile_count: u32) -> u32 {
    x.rem_euclid(tile_count.max(1) as i32) as u32
}

/// `f(view)` - the tile ids a `viewport` needs to fill a `bounds`-sized widget.
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
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
            materialized: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
            virtual_rect: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
        };
    }

    let (layer, viewport) = match data.downcast_ref::<MapTileCache>() {
        Some(c) => (c.layer.clone(), c.viewport),
        None => {
            return VirtualViewReturn {
                dom: OptionDom::None,
                materialized: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
                virtual_rect: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
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
    let states: BTreeMap<MapTileId, TileDisplay> = data
        .downcast_ref::<MapTileCache>()
        .map_or_else(BTreeMap::new, |c| {
            c.tiles
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
                .collect()
        });

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
                Callback::from_ptr(map_on_pointer_down),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                data.clone(),
                Callback::from_ptr(map_on_pointer_move),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                Callback::from_ptr(map_on_pointer_up),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                data.clone(),
                Callback::from_ptr(map_on_pointer_up),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Scroll),
                data.clone(),
                Callback::from_ptr(map_on_scroll),
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
            // Derive each tile's on-screen box from the ROUNDED origins of THIS
            // tile and the NEXT one along each axis, so neighbours always share an
            // exact edge — no gaps, no overlaps — at fractional zoom too. A fixed
            // `tile_px.round()` size drifts out of step with the per-tile rounded
            // origin the moment `tile_px` isn't a whole number (any non-integer
            // zoom, e.g. a scroll-wheel notch), scattering the tiles into a
            // disconnected grid. At integer zoom `tile_px` is exactly 256, so each
            // span is exactly 256 and this is identical to the previous behaviour.
            let proj = |coord: f32, centre: f32, span_px: f32| {
                ((coord - centre) * tile_px + span_px * 0.5).round() as i32
            };
            let screen_x = proj(x as f32, centre_x, width_px);
            let screen_y = proj(y as f32, centre_y, height_px);
            // `saturating_sub`: `proj` ends in `as i32`, which SATURATES a
            // non-finite or out-of-range float to i32::MIN / i32::MAX. A viewport
            // whose zoom is `f32::INFINITY` therefore puts the two projected
            // origins at opposite ends of i32, and a plain `-` overflows — an
            // abort in an overflow-checked build (these run inside an `extern "C"`
            // render callback, so the panic does not unwind) and a wrapped,
            // nonsensical tile size in release.
            let size_w = proj(x as f32 + 1.0, centre_x, width_px)
                .saturating_sub(screen_x)
                .max(1);
            let size_h = proj(y as f32 + 1.0, centre_y, height_px)
                .saturating_sub(screen_y)
                .max(1);

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
                 width: {size_w}px; height: {size_h}px; {chrome}"
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
                            Dom::create_p_with_text(alloc::format!("✓? z{z_int}/{x}/{y}"))
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
                        Dom::create_p_with_text(alloc::format!("{state_tag} z{z_int}/{x}/{y}"))
                            .with_css("position: absolute; left: 4px; top: 4px; font-size: 11px; color: #888;"),
                    );
                }
            }

            grid = grid.with_child(tile_div);
        }
    }

    VirtualViewReturn {
        dom: OptionDom::Some(grid),
        materialized: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
        virtual_rect: azul_core::geom::LogicalRect::new(azul_core::geom::LogicalPosition::zero(), bounds_logical),
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
        // 512x512 viewport at zoom-scale 1 (256 px tiles) = 2 tiles across;
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

// ────────── Adversarial autotest coverage ────────────────────────────
//
// Boundary / malformed / overflow probes for the widget's pure numeric core,
// its builders, and its callback surface. Everything here is deliberately fed
// values a real app can produce (a zero-size container, a NaN viewport, a
// tile id past the antimeridian, a garbage tile payload) and asserts the
// function *contains* them rather than panicking.
#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        callbacks::{HidpiAdjustedBounds, VirtualViewCallbackReason},
        dom::{DomId, DomNodeId},
        geom::{LogicalPosition, LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::{DpiScaleFactor, ImageCache, RendererResources},
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle, WindowTheme},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use azul_core::task::ThreadReceiver;

    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{ThreadCallback, ThreadCallbackType, ThreadSender},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn close(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() <= eps, "expected {a} ≈ {b} (within {eps})");
    }

    fn layer_zoom(min_zoom: u8, max_zoom: u8) -> MapTileLayer {
        MapTileLayer {
            url_template: AzString::from("https://tiles.invalid/{z}/{x}/{y}.pbf"),
            min_zoom,
            max_zoom,
            attribution: AzString::from("attr"),
            style_css: AzString::from(""),
        }
    }

    fn view(lat: f64, lon: f64, zoom: f32) -> MapViewport {
        MapViewport {
            centre_lat_deg: lat,
            centre_lon_deg: lon,
            zoom,
            bearing_deg: 0.0,
            pitch_deg: 0.0,
        }
    }

    fn cache_at(lat: f64, lon: f64, zoom: f32) -> MapTileCache {
        MapTileCache::new(layer_zoom(0, 19), view(lat, lon, zoom))
    }

    /// Records everything the widget's user hooks are handed.
    #[derive(Default)]
    struct HookLog {
        viewports: Vec<MapViewport>,
        coords: Vec<MapLatLon>,
    }

    extern "C" fn record_viewport(
        mut data: RefAny,
        _: CallbackInfo,
        viewport: MapViewport,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<HookLog>() {
            log.viewports.push(viewport);
        }
        Update::DoNothing
    }

    extern "C" fn record_pin(mut data: RefAny, _: CallbackInfo, coord: MapLatLon) -> Update {
        if let Some(mut log) = data.downcast_mut::<HookLog>() {
            log.coords.push(coord);
        }
        Update::RefreshDom
    }

    /// A worker that returns immediately - enough to exercise the spawn path
    /// without any I/O.
    extern "C" fn noop_worker(_: RefAny, _: ThreadSender, _: ThreadReceiver) {}
    extern "C" fn other_noop_worker(_: RefAny, _: ThreadSender, _: ThreadReceiver) {}

    fn hook_log(data: &mut RefAny) -> (usize, usize) {
        let log = data
            .downcast_ref::<HookLog>()
            .expect("payload must still be a HookLog");
        (log.viewports.len(), log.coords.len())
    }

    /// Runs `f` against a real `CallbackInfo` over an empty `LayoutWindow`,
    /// with `cursor` reported as the cursor position relative to the hit node.
    /// Returns `f`'s value plus every `CallbackChange` the callback recorded.
    fn with_callback_info_at<R>(
        cursor: OptionLogicalPosition,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<
            DomId,
            BTreeMap<NodeHierarchyItemId, ScrollPosition>,
        > = BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            cursor,
            OptionLogicalPosition::None,
        );

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    fn with_callback_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> (R, Vec<CallbackChange>) {
        with_callback_info_at(OptionLogicalPosition::None, f)
    }

    fn cursor_at(x: f32, y: f32) -> OptionLogicalPosition {
        OptionLogicalPosition::Some(LogicalPosition::new(x, y))
    }

    /// Runs `f` against a `VirtualViewCallbackInfo` reporting `w x h` bounds.
    fn with_virtual_view_info<R>(
        w: f32,
        h: f32,
        f: impl FnOnce(VirtualViewCallbackInfo) -> R,
    ) -> R {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let size = LogicalSize::new(w, h);
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            &fonts,
            &images,
            WindowTheme::LightMode,
            HidpiAdjustedBounds {
                logical_size: size,
                hidpi_factor: DpiScaleFactor::new(1.0),
            },
            azul_core::geom::LogicalRect::new(LogicalPosition::zero(), size),
            azul_core::geom::LogicalRect::new(LogicalPosition::zero(), size),
            LogicalPosition::zero(),
        );
        f(info)
    }

    fn rendered_child_count(ret: &VirtualViewReturn) -> Option<usize> {
        match &ret.dom {
            OptionDom::Some(d) => Some(d.children.as_slice().len()),
            OptionDom::None => None,
        }
    }

    // ==================================================================
    // wrap_lon  (numeric)
    // ==================================================================

    #[test]
    fn wrap_lon_zero_and_negative_zero_are_zero() {
        assert_eq!(wrap_lon(0.0), 0.0);
        assert_eq!(wrap_lon(-0.0), 0.0);
    }

    #[test]
    fn wrap_lon_nan_and_infinities_are_nan_not_panic() {
        // `rem_euclid` on a non-finite dividend is NaN (fmod(inf, x) == NaN);
        // the documented, non-panicking outcome.
        assert!(wrap_lon(f64::NAN).is_nan());
        assert!(wrap_lon(f64::INFINITY).is_nan());
        assert!(wrap_lon(f64::NEG_INFINITY).is_nan());
    }

    #[test]
    fn wrap_lon_extreme_finite_inputs_stay_bounded_and_finite() {
        for raw in [
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            1.0e300,
            -1.0e300,
            1.0e18,
            -1.0e18,
            360.0 * 1.0e9,
        ] {
            let w = wrap_lon(raw);
            assert!(w.is_finite(), "{raw} → {w} is not finite");
            assert!((-180.0..=180.0).contains(&w), "{raw} → {w} out of range");
        }
    }

    #[test]
    fn wrap_lon_is_idempotent_on_representative_inputs() {
        for (raw, expected) in [
            (0.0_f64, 0.0_f64),
            (45.0, 45.0),
            (-45.0, -45.0),
            (181.0, -179.0),
            (-181.0, 179.0),
            (720.0, 0.0),
            (-720.0, 0.0),
            (1.0e6, -80.0),
        ] {
            let once = wrap_lon(raw);
            close(once, expected, 1e-9);
            close(wrap_lon(once), once, 1e-9);
        }
    }

    // ==================================================================
    // lon_to_tile_x / tile_x_to_lon  (numeric)
    // ==================================================================

    #[test]
    fn lon_to_tile_x_zero_tile_count_collapses_to_zero() {
        for lon in [-180.0, -1.0, 0.0, 1.0, 180.0] {
            assert_eq!(lon_to_tile_x(lon, 0.0), 0.0, "lon {lon} at tile_count 0");
        }
    }

    #[test]
    fn lon_to_tile_x_nan_inf_are_defined_not_panics() {
        assert!(lon_to_tile_x(f64::NAN, 4.0).is_nan());
        assert!(lon_to_tile_x(0.0, f64::NAN).is_nan());
        assert_eq!(lon_to_tile_x(f64::INFINITY, 4.0), f64::INFINITY);
        assert_eq!(lon_to_tile_x(f64::NEG_INFINITY, 4.0), f64::NEG_INFINITY);
        // inf * 0 is the one genuinely undefined product → NaN, not a panic.
        assert!(lon_to_tile_x(f64::INFINITY, 0.0).is_nan());
    }

    #[test]
    fn lon_to_tile_x_is_monotonic_and_saturates_on_huge_counts() {
        let mut prev = f64::NEG_INFINITY;
        for lon in [-180.0, -90.0, -0.5, 0.0, 0.5, 90.0, 180.0] {
            let x = lon_to_tile_x(lon, 256.0);
            assert!(x > prev, "lon_to_tile_x must increase with longitude");
            prev = x;
        }
        assert_eq!(lon_to_tile_x(180.0, f64::MAX), f64::MAX);
        assert!(lon_to_tile_x(180.0, f64::INFINITY).is_infinite());
    }

    #[test]
    fn tile_x_to_lon_degenerate_tile_counts_do_not_panic() {
        // 0/0 is the only NaN; a non-zero column over a zero-wide world is +inf.
        assert!(tile_x_to_lon(0.0, 0.0).is_nan());
        assert!(tile_x_to_lon(1.0, 0.0).is_infinite());
        assert!(tile_x_to_lon(f64::NAN, 4.0).is_nan());
        assert!(tile_x_to_lon(f64::MAX, f64::MIN_POSITIVE).is_infinite());
    }

    #[test]
    fn lon_tile_x_round_trips_across_zooms_and_edges() {
        for z in [0u32, 1, 5, 14, 22] {
            let tc = f64::from(1u32 << z);
            for lon in [-180.0, -179.999, -122.4194, 0.0, 0.1, 151.2093, 180.0] {
                let x = lon_to_tile_x(lon, tc);
                close(tile_x_to_lon(x, tc), lon, 1e-9);
            }
        }
    }

    // ==================================================================
    // lat_to_tile_y / tile_y_to_lat  (numeric)
    // ==================================================================

    #[test]
    fn lat_to_tile_y_inside_the_mercator_band_is_finite_and_ordered() {
        let tc = 256.0;
        let mut prev = f64::NEG_INFINITY;
        // y grows southward, so iterate north → south and expect a rise.
        for lat in [85.0, 60.0, 30.0, 0.0, -30.0, -60.0, -85.0] {
            let y = lat_to_tile_y(lat, tc);
            assert!(y.is_finite(), "lat {lat} → {y}");
            assert!((0.0..=tc).contains(&y), "lat {lat} → {y} outside the grid");
            assert!(y > prev, "tile-y must increase as latitude decreases");
            prev = y;
        }
    }

    #[test]
    fn lat_to_tile_y_nan_and_infinite_latitudes_are_nan() {
        assert!(lat_to_tile_y(f64::NAN, 4.0).is_nan());
        // tan(±inf) is NaN, so the whole Mercator term degrades to NaN.
        assert!(lat_to_tile_y(f64::INFINITY, 4.0).is_nan());
        assert!(lat_to_tile_y(f64::NEG_INFINITY, 4.0).is_nan());
        assert!(lat_to_tile_y(0.0, f64::NAN).is_nan());
    }

    #[test]
    fn lat_to_tile_y_past_the_poles_does_not_panic() {
        // Beyond ±85.05° the projection is undefined; assert only that every
        // one of these returns (reaching the length check means no panic).
        let outs: Vec<f64> = [90.0, -90.0, 89.9999, -89.9999, 180.0, -180.0, 1.0e9]
            .iter()
            .map(|lat| lat_to_tile_y(*lat, 4.0))
            .collect();
        assert_eq!(outs.len(), 7);
    }

    #[test]
    fn tile_y_to_lat_saturates_at_the_poles_for_out_of_range_rows() {
        for (y, expected) in [(-1.0e9_f64, 90.0_f64), (1.0e9, -90.0)] {
            let lat = tile_y_to_lat(y, 4.0);
            close(lat, expected, 1e-9);
        }
        // No finite row can ever escape ±90°.
        for y in [-1.0e300, -1000.0, -1.0, 0.0, 2.0, 1000.0, 1.0e300] {
            let lat = tile_y_to_lat(y, 4.0);
            assert!(
                (-90.0 - 1e-9..=90.0 + 1e-9).contains(&lat),
                "row {y} → {lat} outside ±90"
            );
        }
    }

    #[test]
    fn tile_y_to_lat_degenerate_tile_counts_do_not_panic() {
        assert!(tile_y_to_lat(0.0, 0.0).is_nan()); // 0/0
        close(tile_y_to_lat(1.0, 0.0), -90.0, 1e-9); // +inf rows south
        assert!(tile_y_to_lat(f64::NAN, 4.0).is_nan());
    }

    #[test]
    fn lat_tile_y_round_trips_at_the_mercator_edges() {
        for z in [0u32, 3, 14, 22] {
            let tc = f64::from(1u32 << z);
            for lat in [-85.05, -45.0, -0.0001, 0.0, 0.0001, 45.0, 85.05] {
                let y = lat_to_tile_y(lat, tc);
                close(tile_y_to_lat(y, tc), lat, 1e-6);
            }
        }
    }

    // ==================================================================
    // pan_viewport  (numeric)
    // ==================================================================

    #[test]
    fn pan_viewport_nan_inputs_propagate_without_panicking() {
        let (lon, lat) = pan_viewport(f64::NAN, 0.0, 2.0, 10.0, 10.0);
        assert!(lat.is_nan(), "NaN centre latitude must stay NaN, got {lat}");
        assert!(lon.is_nan() || (-180.0..=180.0).contains(&lon));

        let (lon, _) = pan_viewport(0.0, f64::NAN, 2.0, 10.0, 10.0);
        assert!(lon.is_nan());

        let (lon, lat) = pan_viewport(0.0, 0.0, 2.0, f64::NAN, f64::NAN);
        assert!(lon.is_nan() && lat.is_nan());
    }

    #[test]
    fn pan_viewport_infinite_zoom_is_a_no_op() {
        // world_px = inf → every pixel delta maps to a zero angular delta.
        let (lon, lat) = pan_viewport(37.0, -122.0, f64::INFINITY, 1.0e6, -1.0e6);
        close(lon, -122.0, 1e-9);
        close(lat, 37.0, 1e-9);
    }

    #[test]
    fn pan_viewport_negative_infinite_zoom_saturates_latitude_not_panics() {
        // world_px underflows to 0 → the longitude delta is ±inf (→ NaN through
        // wrap_lon) and the latitude delta saturates against the ±85 clamp.
        let (lon, lat) = pan_viewport(0.0, 0.0, f64::NEG_INFINITY, 100.0, 100.0);
        assert!(lon.is_nan(), "expected NaN longitude, got {lon}");
        close(lat, 85.0, 1e-9);
        let (_, lat_south) = pan_viewport(0.0, 0.0, f64::NEG_INFINITY, 0.0, -100.0);
        close(lat_south, -85.0, 1e-9);
    }

    #[test]
    fn pan_viewport_extreme_pixel_deltas_stay_inside_the_world() {
        for dx in [-1.0e18_f64, -1.0e9, -1.0, 0.0, 1.0, 1.0e9, 1.0e18] {
            for dy in [-1.0e18_f64, 0.0, 1.0e18] {
                let (lon, lat) = pan_viewport(37.0, -122.0, 0.0, dx, dy);
                assert!(lon.is_finite(), "dx {dx} dy {dy} → lon {lon}");
                assert!((-180.0..=180.0).contains(&lon), "lon {lon} out of range");
                assert!((-85.0..=85.0).contains(&lat), "lat {lat} out of range");
            }
        }
    }

    #[test]
    fn pan_viewport_zero_zoom_and_zero_delta_is_the_identity() {
        let (lon, lat) = pan_viewport(0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!((lon, lat), (0.0, 0.0));
    }

    #[test]
    fn pan_viewport_latitude_step_shrinks_towards_the_poles() {
        // d_lat carries a cos(lat) factor, so the same drag moves less near a pole.
        let (_, at_equator) = pan_viewport(0.0, 0.0, 2.0, 0.0, 100.0);
        let (_, at_80) = pan_viewport(80.0, 0.0, 2.0, 0.0, 100.0);
        assert!(
            (at_80 - 80.0).abs() < at_equator.abs(),
            "pole-adjacent pan {at_80} must move less than equatorial {at_equator}"
        );
    }

    // ==================================================================
    // MapWidget::latlon_at_px / px_at_latlon  (numeric)
    // ==================================================================

    #[test]
    fn latlon_at_px_centre_pixel_is_the_viewport_centre() {
        let viewport = view(51.5074, -0.1278, 11.0);
        let container = LogicalSize::new(800.0, 600.0);
        let coord = MapWidget::latlon_at_px(
            viewport,
            LogicalPosition::new(400.0, 300.0),
            container,
        );
        close(coord.lat_deg, 51.5074, 1e-9);
        close(coord.lon_deg, -0.1278, 1e-9);
    }

    #[test]
    fn latlon_at_px_result_is_always_clamped_or_nan() {
        let container = LogicalSize::new(800.0, 600.0);
        for zoom in [0.0_f32, 2.0, 11.0, 22.0] {
            for px in [
                LogicalPosition::new(0.0, 0.0),
                LogicalPosition::new(-1.0e9, -1.0e9),
                LogicalPosition::new(1.0e9, 1.0e9),
                LogicalPosition::new(f32::MAX, f32::MIN),
            ] {
                let c = MapWidget::latlon_at_px(view(0.0, 0.0, zoom), px, container);
                assert!(
                    (-180.0..=180.0).contains(&c.lon_deg),
                    "zoom {zoom} px {px:?} → lon {}",
                    c.lon_deg
                );
                assert!(
                    (-85.0..=85.0).contains(&c.lat_deg),
                    "zoom {zoom} px {px:?} → lat {}",
                    c.lat_deg
                );
            }
        }
    }

    #[test]
    fn latlon_at_px_non_finite_zoom_does_not_panic() {
        let container = LogicalSize::new(800.0, 600.0);
        let px = LogicalPosition::new(10.0, 10.0);
        // +inf zoom → infinitely large world → the centre pixel wins.
        let c = MapWidget::latlon_at_px(view(0.0, 0.0, f32::INFINITY), px, container);
        close(c.lon_deg, 0.0, 1e-9);
        close(c.lat_deg, 0.0, 1e-9);
        // -inf zoom → zero-size world → the clamp saturates instead of overflowing.
        let c = MapWidget::latlon_at_px(view(0.0, 0.0, f32::NEG_INFINITY), px, container);
        assert!((-180.0..=180.0).contains(&c.lon_deg));
        assert!((-85.0..=85.0).contains(&c.lat_deg));
        // NaN zoom → NaN out, never a panic.
        let c = MapWidget::latlon_at_px(view(0.0, 0.0, f32::NAN), px, container);
        assert!(c.lon_deg.is_nan() && c.lat_deg.is_nan());
    }

    #[test]
    fn latlon_at_px_zero_sized_container_is_still_defined() {
        let c = MapWidget::latlon_at_px(
            view(10.0, 20.0, 4.0),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(0.0, 0.0),
        );
        close(c.lat_deg, 10.0, 1e-9);
        close(c.lon_deg, 20.0, 1e-9);
    }

    #[test]
    fn px_at_latlon_centre_coord_is_the_container_centre() {
        let viewport = view(37.7749, -122.4194, 12.0);
        let container = LogicalSize::new(1024.0, 768.0);
        let p = MapWidget::px_at_latlon(
            viewport,
            MapLatLon {
                lat_deg: viewport.centre_lat_deg,
                lon_deg: viewport.centre_lon_deg,
            },
            container,
        );
        close(f64::from(p.x), 512.0, 1e-3);
        close(f64::from(p.y), 384.0, 1e-3);
    }

    #[test]
    fn px_at_latlon_saturates_instead_of_overflowing_f32() {
        // world = 256 * 2^f32::MAX overflows to +inf; the f64→f32 cast must
        // saturate (Rust's `as` is saturating) rather than trap.
        let container = LogicalSize::new(800.0, 600.0);
        let p = MapWidget::px_at_latlon(
            view(0.0, 0.0, f32::MAX),
            MapLatLon {
                lat_deg: 10.0,
                lon_deg: 10.0,
            },
            container,
        );
        assert!(!p.x.is_finite(), "expected a saturated x, got {}", p.x);
        assert!(!p.y.is_finite(), "expected a saturated y, got {}", p.y);
    }

    #[test]
    fn px_at_latlon_at_a_pole_centre_does_not_panic() {
        // cos(90°) is ~6e-17, not exactly 0 — the division is huge but finite.
        let container = LogicalSize::new(800.0, 600.0);
        let p = MapWidget::px_at_latlon(
            view(90.0, 0.0, 2.0),
            MapLatLon {
                lat_deg: 0.0,
                lon_deg: 0.0,
            },
            container,
        );
        assert!(p.x.is_finite(), "x should stay finite, got {}", p.x);
        assert!(!p.y.is_nan(), "y must be a number or an infinity, got {}", p.y);
    }

    #[test]
    fn projection_px_round_trips_within_the_clamped_band() {
        let container = LogicalSize::new(800.0, 600.0);
        for zoom in [2.0_f32, 8.0, 14.0] {
            let viewport = view(37.7749, -122.4194, zoom);
            for (dlat, dlon) in [(0.0, 0.0), (0.01, 0.02), (-0.03, 0.04)] {
                let coord = MapLatLon {
                    lat_deg: viewport.centre_lat_deg + dlat,
                    lon_deg: viewport.centre_lon_deg + dlon,
                };
                let px = MapWidget::px_at_latlon(viewport, coord, container);
                let back = MapWidget::latlon_at_px(viewport, px, container);
                close(back.lat_deg, coord.lat_deg, 1e-4);
                close(back.lon_deg, coord.lon_deg, 1e-4);
            }
        }
    }

    // ==================================================================
    // visible_tile_range  (numeric)
    // ==================================================================

    #[test]
    fn visible_tile_range_zero_zoom_scale_saturates_to_the_i32_extremes() {
        // tile_px = 0 → the half-extent is +inf → the floor/ceil casts saturate.
        // x is unclamped (world wrap), so the caller receives the FULL i32 span.
        let (x0, x1, y0, y1) = visible_tile_range(8.0, 8.0, 800.0, 600.0, 0.0, 16);
        assert_eq!((x0, x1), (i32::MIN, i32::MAX));
        assert_eq!((y0, y1), (0, 15));
    }

    #[test]
    fn visible_tile_range_infinite_dimensions_saturate_the_same_way() {
        let (x0, x1, y0, y1) =
            visible_tile_range(8.0, 8.0, f32::INFINITY, f32::INFINITY, 1.0, 16);
        assert_eq!((x0, x1), (i32::MIN, i32::MAX));
        assert_eq!((y0, y1), (0, 15));
    }

    #[test]
    fn visible_tile_range_nan_inputs_collapse_to_a_single_cell() {
        // `NaN as i32` is 0 in Rust (saturating cast), so a non-finite viewport
        // degenerates to the (0,0) cell instead of an unbounded loop.
        assert_eq!(
            visible_tile_range(8.0, 8.0, f32::NAN, f32::NAN, 1.0, 16),
            (0, 0, 0, 0)
        );
        assert_eq!(
            visible_tile_range(f32::NAN, f32::NAN, 512.0, 512.0, 1.0, 16),
            (0, 0, 0, 0)
        );
        assert_eq!(
            visible_tile_range(8.0, 8.0, 512.0, 512.0, f32::NAN, 16),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn visible_tile_range_negative_dimensions_are_taken_absolutely() {
        let positive = visible_tile_range(8.0, 8.0, 512.0, 384.0, 1.0, 16);
        let negative = visible_tile_range(8.0, 8.0, -512.0, -384.0, 1.0, 16);
        assert_eq!(positive, negative);
    }

    #[test]
    fn visible_tile_range_zero_tile_count_yields_an_empty_row_span() {
        // max_idx = -1, so y_min (>= 0) is above y_max — the caller's
        // `for y in y_min..=y_max` loop body never runs. No tiles, no panic.
        let (_, _, y0, y1) = visible_tile_range(0.0, 0.0, 512.0, 512.0, 1.0, 0);
        assert!(y0 > y1, "expected an empty row span, got {y0}..={y1}");
    }

    #[test]
    fn visible_tile_range_u32_max_tile_count_wraps_to_an_empty_row_span() {
        // `u32::MAX as i32` is -1 → max_idx -2 → again an empty (safe) span.
        let (_, _, y0, y1) = visible_tile_range(0.0, 0.0, 512.0, 512.0, 1.0, u32::MAX);
        assert!(y0 > y1, "expected an empty row span, got {y0}..={y1}");
    }

    #[test]
    fn visible_tile_range_always_keeps_a_one_tile_margin() {
        // Even a 1x1-pixel viewport must over-scan by a whole tile each side.
        let (x0, x1, y0, y1) = visible_tile_range(8.0, 8.0, 1.0, 1.0, 1.0, 16);
        assert!(x0 <= 7 && x1 >= 9, "x span {x0}..={x1} lost its margin");
        assert!(y0 <= 7 && y1 >= 9, "y span {y0}..={y1} lost its margin");
    }

    #[test]
    fn visible_tile_range_extreme_zoom_scale_shrinks_to_the_margin() {
        // A gigantic tile_px makes the viewport sub-tile: only the margin remains.
        let (x0, x1, y0, y1) = visible_tile_range(8.0, 8.0, 800.0, 600.0, f32::MAX, 16);
        assert_eq!((x0, x1), (7, 9));
        assert_eq!((y0, y1), (7, 9));
    }

    // ==================================================================
    // wrap_tile_x  (numeric)
    // ==================================================================

    #[test]
    fn wrap_tile_x_zero_tile_count_is_zero_never_a_division_by_zero() {
        for x in [i32::MIN, -7, -1, 0, 1, 7, i32::MAX] {
            assert_eq!(wrap_tile_x(x, 0), 0, "column {x} at tile_count 0");
        }
    }

    #[test]
    fn wrap_tile_x_extremes_stay_inside_the_band() {
        for tile_count in [1u32, 2, 4, 256, 65_536, 1 << 30, 1 << 31] {
            for x in [i32::MIN, -1_000_000, -1, 0, 1, 1_000_000, i32::MAX] {
                let wrapped = wrap_tile_x(x, tile_count);
                assert!(
                    wrapped < tile_count,
                    "column {x} at tile_count {tile_count} → {wrapped} (out of band)"
                );
            }
        }
    }

    #[test]
    fn wrap_tile_x_matches_rem_euclid_for_realistic_zooms() {
        for z in 0u32..=20 {
            let tile_count = 1u32 << z;
            for x in [i32::MIN, -3, -1, 0, 1, 3, i32::MAX] {
                assert_eq!(
                    wrap_tile_x(x, tile_count),
                    x.rem_euclid(tile_count as i32) as u32
                );
            }
        }
    }

    #[test]
    fn wrap_tile_x_is_periodic_in_tile_count() {
        for tile_count in [1u32, 2, 4, 16, 1024] {
            for x in [-9i32, -1, 0, 1, 9] {
                let shifted = x
                    .checked_add(tile_count as i32)
                    .expect("shift must stay in range");
                assert_eq!(wrap_tile_x(x, tile_count), wrap_tile_x(shifted, tile_count));
            }
        }
    }

    // ==================================================================
    // map_visible_tiles  (numeric)
    // ==================================================================

    #[test]
    fn map_visible_tiles_zero_bounds_still_covers_the_centre() {
        let layer = MapTileLayer::default();
        let tiles = map_visible_tiles(&view(0.0, 0.0, 2.0), LogicalSize::new(0.0, 0.0), &layer);
        assert!(!tiles.is_empty(), "the one-tile margin must survive 0x0 bounds");
        for t in &tiles {
            assert_eq!(t.z, 2);
            assert!(t.x < 4 && t.y < 4, "tile {t:?} escaped the z2 grid");
        }
    }

    #[test]
    fn map_visible_tiles_non_finite_bounds_degenerate_to_one_tile() {
        let layer = MapTileLayer::default();
        let tiles = map_visible_tiles(
            &view(0.0, 0.0, 2.0),
            LogicalSize::new(f32::NAN, f32::NAN),
            &layer,
        );
        assert_eq!(tiles.len(), 1, "NaN bounds must not enumerate a grid");
    }

    #[test]
    fn map_visible_tiles_nan_zoom_degenerates_to_one_tile() {
        let layer = MapTileLayer::default();
        let tiles = map_visible_tiles(
            &view(0.0, 0.0, f32::NAN),
            LogicalSize::new(800.0, 600.0),
            &layer,
        );
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].z, 0, "a NaN zoom clamps to the layer minimum");
    }

    #[test]
    fn map_visible_tiles_positive_infinite_zoom_stays_bounded() {
        let layer = MapTileLayer::default();
        let tiles = map_visible_tiles(
            &view(0.0, 0.0, f32::INFINITY),
            LogicalSize::new(800.0, 600.0),
            &layer,
        );
        assert!(!tiles.is_empty());
        assert!(tiles.len() < 64, "+inf zoom produced {} tiles", tiles.len());
        for t in &tiles {
            assert_eq!(t.z, layer.max_zoom, "+inf zoom must clamp to max_zoom");
        }
    }

    #[test]
    fn map_visible_tiles_negative_bounds_match_positive_bounds() {
        let layer = MapTileLayer::default();
        let viewport = view(48.0, 11.0, 6.0);
        let positive = map_visible_tiles(&viewport, LogicalSize::new(640.0, 480.0), &layer);
        let negative = map_visible_tiles(&viewport, LogicalSize::new(-640.0, -480.0), &layer);
        assert_eq!(positive, negative);
    }

    #[test]
    fn map_visible_tiles_ids_always_live_inside_the_grid() {
        for (min_zoom, max_zoom) in [(0u8, 14u8), (3, 5), (0, 0)] {
            let layer = layer_zoom(min_zoom, max_zoom);
            for zoom in [-1.0_f32, 0.0, 2.5, 7.0, 99.0] {
                for (lat, lon) in [(0.0, 0.0), (85.0, 180.0), (-85.0, -180.0), (60.0, 179.99)] {
                    let viewport = view(lat, lon, zoom);
                    let tiles =
                        map_visible_tiles(&viewport, LogicalSize::new(800.0, 600.0), &layer);
                    let expected_z = (zoom.floor() as i32)
                        .clamp(i32::from(min_zoom), i32::from(max_zoom))
                        as u8;
                    let tile_count = 1u32 << u32::from(expected_z);
                    for t in &tiles {
                        assert_eq!(t.z, expected_z, "zoom {zoom} produced z {}", t.z);
                        assert!(t.x < tile_count, "column {} >= {tile_count}", t.x);
                        assert!(t.y < tile_count, "row {} >= {tile_count}", t.y);
                    }
                }
            }
        }
    }

    #[test]
    fn map_visible_tiles_wraps_columns_across_the_antimeridian() {
        // A viewport pinned to +180° over-scans past the east edge; every id
        // must still be a legal column (the wrap), never tile_count or above.
        let layer = layer_zoom(0, 14);
        let tiles = map_visible_tiles(
            &view(0.0, 180.0, 3.0),
            LogicalSize::new(1024.0, 256.0),
            &layer,
        );
        assert!(!tiles.is_empty());
        assert!(
            tiles.iter().any(|t| t.x == 0),
            "panning past +180° must surface the west-edge column"
        );
        for t in &tiles {
            assert!(t.x < 8, "column {} escaped the z3 grid", t.x);
        }
    }

    // ==================================================================
    // MapWidget builders  (constructors)
    // ==================================================================

    #[test]
    fn create_uses_the_given_layer_and_neutral_defaults() {
        let layer = layer_zoom(2, 9);
        let widget = MapWidget::create(layer.clone());
        assert_eq!(widget.layer, layer);
        assert_eq!(widget.viewport, MapViewport::default());
        assert!(widget.container_style.as_slice().is_empty());
        assert!(matches!(
            widget.on_viewport_changed,
            OptionMapViewportChanged::None
        ));
        assert!(matches!(widget.on_pin_tap, OptionMapPinTap::None));
    }

    #[test]
    fn create_accepts_degenerate_layers_without_panicking() {
        // An inverted zoom band and an empty template are nonsense but must
        // still build - the widget validates nothing at construction time.
        let widget = MapWidget::create(MapTileLayer {
            url_template: AzString::from(""),
            min_zoom: 30,
            max_zoom: 0,
            attribution: AzString::from(""),
            style_css: AzString::from(""),
        });
        assert_eq!(widget.layer.min_zoom, 30);
        assert_eq!(widget.layer.max_zoom, 0);
    }

    #[test]
    fn with_viewport_stores_extreme_values_verbatim() {
        let widget = MapWidget::create(MapTileLayer::default())
            .with_viewport(view(1.0e300, -1.0e300, f32::MAX));
        assert_eq!(widget.viewport.centre_lat_deg, 1.0e300);
        assert_eq!(widget.viewport.centre_lon_deg, -1.0e300);
        assert_eq!(widget.viewport.zoom, f32::MAX);

        let widget = MapWidget::create(MapTileLayer::default())
            .with_viewport(view(f64::NAN, f64::INFINITY, f32::NEG_INFINITY));
        assert!(widget.viewport.centre_lat_deg.is_nan());
        assert!(widget.viewport.centre_lon_deg.is_infinite());
        assert!(widget.viewport.zoom.is_infinite());
    }

    #[test]
    fn with_viewport_is_last_write_wins() {
        let widget = MapWidget::create(MapTileLayer::default())
            .with_viewport(view(1.0, 2.0, 3.0))
            .with_viewport(view(4.0, 5.0, 6.0));
        assert_eq!(widget.viewport, view(4.0, 5.0, 6.0));
    }

    #[test]
    fn with_container_style_replaces_the_style_vec() {
        let css = CssPropertyWithConditionsVec::parse("width: 100px; height: 50px;");
        let parsed_len = css.as_slice().len();
        assert!(parsed_len > 0, "positive control: the style must parse");
        let widget = MapWidget::create(MapTileLayer::default()).with_container_style(css);
        assert_eq!(widget.container_style.as_slice().len(), parsed_len);

        // An unparseable style yields an empty vec, and the builder stores it.
        let widget = MapWidget::create(MapTileLayer::default())
            .with_container_style(CssPropertyWithConditionsVec::parse(""));
        assert!(widget.container_style.as_slice().is_empty());
    }

    #[test]
    fn with_container_style_tolerates_garbage_and_unicode() {
        for style in [
            "\u{1F600}: \u{1F600};",
            "   ",
            ";;;;",
            "width",
            "width: ;",
            "}{",
            "color: \u{0301}\u{0301};",
        ] {
            let widget = MapWidget::create(MapTileLayer::default())
                .with_container_style(CssPropertyWithConditionsVec::parse(style));
            // Reaching here means neither the parser nor the builder panicked.
            let _ = widget.container_style.as_slice().len();
        }
    }

    #[test]
    fn with_on_viewport_changed_installs_and_replaces_the_hook() {
        let mut widget = MapWidget::create(MapTileLayer::default()).with_on_viewport_changed(
            RefAny::new(HookLog::default()),
            record_viewport as MapViewportChangedCallbackType,
        );
        assert!(matches!(
            widget.on_viewport_changed,
            OptionMapViewportChanged::Some(_)
        ));
        // Re-setting must overwrite, not accumulate.
        widget.set_on_viewport_changed(
            RefAny::new(HookLog::default()),
            record_viewport as MapViewportChangedCallbackType,
        );
        assert!(matches!(
            widget.on_viewport_changed,
            OptionMapViewportChanged::Some(_)
        ));
        assert!(matches!(widget.on_pin_tap, OptionMapPinTap::None));
    }

    #[test]
    fn with_on_pin_tap_installs_and_replaces_the_hook() {
        let mut widget = MapWidget::create(MapTileLayer::default())
            .with_on_pin_tap(RefAny::new(HookLog::default()), record_pin as MapPinTapCallbackType);
        assert!(matches!(widget.on_pin_tap, OptionMapPinTap::Some(_)));
        widget.set_on_pin_tap(
            RefAny::new(HookLog::default()),
            record_pin as MapPinTapCallbackType,
        );
        assert!(matches!(widget.on_pin_tap, OptionMapPinTap::Some(_)));
        assert!(matches!(
            widget.on_viewport_changed,
            OptionMapViewportChanged::None
        ));
    }

    #[test]
    fn builder_chain_order_does_not_matter_for_independent_fields() {
        let layer = layer_zoom(1, 12);
        let viewport = view(10.0, 20.0, 5.0);
        let a = MapWidget::create(layer.clone())
            .with_viewport(viewport)
            .with_container_style(CssPropertyWithConditionsVec::parse("width: 10px;"));
        let b = MapWidget::create(layer)
            .with_container_style(CssPropertyWithConditionsVec::parse("width: 10px;"))
            .with_viewport(viewport);
        assert_eq!(a, b);
    }

    // ==================================================================
    // MapWidget::dom / dom_with_fetch / build_dom
    // ==================================================================

    #[test]
    fn dom_builds_a_single_virtual_view_child_with_a_dataset() {
        let mut dom = MapWidget::create(MapTileLayer::default())
            .with_viewport(view(0.0, 0.0, 3.0))
            .dom();
        assert_eq!(dom.children.as_slice().len(), 1, "one VirtualView child");
        let dataset = dom
            .root
            .get_dataset_mut()
            .expect("the widget div must carry a MapTileCache dataset");
        let cache = dataset
            .downcast_ref::<MapTileCache>()
            .expect("the dataset must be a MapTileCache");
        assert_eq!(cache.viewport.zoom, 3.0);
        assert!(cache.tiles.is_empty());
        assert!(cache.fetch_callback.is_none(), "dom() wires no worker");
    }

    #[test]
    fn dom_survives_degenerate_viewports_and_layers() {
        for viewport in [
            view(f64::NAN, f64::NAN, f32::NAN),
            view(1.0e300, -1.0e300, f32::INFINITY),
            view(0.0, 0.0, f32::NEG_INFINITY),
        ] {
            let dom = MapWidget::create(layer_zoom(200, 1))
                .with_viewport(viewport)
                .dom();
            assert_eq!(dom.children.as_slice().len(), 1);
        }
    }

    #[test]
    fn dom_with_a_container_style_still_builds_the_grid() {
        let dom = MapWidget::create(MapTileLayer::default())
            .with_container_style(CssPropertyWithConditionsVec::parse(
                "position: relative; width: 320px; height: 240px;",
            ))
            .dom();
        assert_eq!(dom.children.as_slice().len(), 1);
    }

    #[test]
    fn dom_with_fetch_records_the_worker_in_the_dataset() {
        let mut dom = MapWidget::create(MapTileLayer::default())
            .dom_with_fetch(ThreadCallback::new(noop_worker));
        let dataset = dom.root.get_dataset_mut().expect("dataset");
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        let cb = cache
            .fetch_callback
            .as_ref()
            .expect("dom_with_fetch must record the worker");
        assert_eq!(cb.cb as usize, noop_worker as ThreadCallbackType as usize);
    }

    #[test]
    fn dom_carries_the_user_hooks_into_the_cache() {
        let mut dom = MapWidget::create(MapTileLayer::default())
            .with_on_viewport_changed(
                RefAny::new(HookLog::default()),
                record_viewport as MapViewportChangedCallbackType,
            )
            .with_on_pin_tap(
                RefAny::new(HookLog::default()),
                record_pin as MapPinTapCallbackType,
            )
            .dom();
        let dataset = dom.root.get_dataset_mut().expect("dataset");
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(matches!(
            cache.on_viewport_changed,
            OptionMapViewportChanged::Some(_)
        ));
        assert!(matches!(cache.on_pin_tap, OptionMapPinTap::Some(_)));
    }

    // ==================================================================
    // MapTileCache  (constructor + mutators)
    // ==================================================================

    #[test]
    fn tile_cache_new_starts_empty_and_idle() {
        let cache = MapTileCache::new(layer_zoom(0, 14), view(1.0, 2.0, 3.0));
        assert!(cache.tiles.is_empty());
        assert!(cache.fetch_callback.is_none());
        assert!(cache.drag_anchor.is_none());
        assert!(cache.pinch_anchor.is_none());
        assert!(cache.press_origin.is_none());
        assert_eq!(cache.viewport, view(1.0, 2.0, 3.0));
        assert_eq!(cache.layer.max_zoom, 14);
    }

    #[test]
    fn mark_tile_ready_and_failed_overwrite_each_other() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        let tile = MapTileId { z: 4, x: 8, y: 8 };
        cache.mark_tile_ready(tile, AzString::from("<svg/>"));
        assert!(matches!(cache.tiles.get(&tile), Some(TileEntry::Ready { .. })));
        cache.mark_tile_failed(tile, AzString::from("boom"));
        assert!(matches!(cache.tiles.get(&tile), Some(TileEntry::Failed { .. })));
        cache.mark_tile_ready(tile, AzString::from(""));
        assert!(matches!(cache.tiles.get(&tile), Some(TileEntry::Ready { .. })));
        assert_eq!(cache.tiles.len(), 1, "the same id must not duplicate");
    }

    #[test]
    fn mark_tile_ready_accepts_empty_unicode_and_huge_payloads() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        let payloads = [
            AzString::from(""),
            AzString::from("\u{1F600}\u{4F60}\u{597D}e\u{0301}"),
            AzString::from("\0\u{FFFD}\r\n\t"),
            AzString::from("x".repeat(1_000_000)),
        ];
        for (i, svg) in payloads.into_iter().enumerate() {
            cache.mark_tile_ready(
                MapTileId {
                    z: 4,
                    x: i as u32,
                    y: 0,
                },
                svg,
            );
        }
        assert_eq!(cache.tiles.len(), 4);
    }

    #[test]
    fn mark_tile_ready_accepts_out_of_range_tile_ids() {
        // Nothing validates an id at insert time - a bogus id from an FFI
        // caller must land in the map rather than panic.
        let mut cache = cache_at(0.0, 0.0, 4.0);
        for tile in [
            MapTileId { z: 0, x: 0, y: 0 },
            MapTileId {
                z: 31,
                x: u32::MAX,
                y: u32::MAX,
            },
            MapTileId {
                z: 4,
                x: u32::MAX,
                y: 0,
            },
        ] {
            cache.mark_tile_failed(tile, AzString::from("e"));
        }
        assert_eq!(cache.tiles.len(), 3);
    }

    // ==================================================================
    // MapTileCache::prune_distant_tiles
    // ==================================================================

    fn fill_ready_grid(cache: &mut MapTileCache, z: u8, side: u32) {
        for x in 0..side {
            for y in 0..side {
                cache
                    .tiles
                    .insert(MapTileId { z, x, y }, TileEntry::Ready { svg: AzString::from("<svg/>") });
            }
        }
    }

    #[test]
    fn prune_never_evicts_in_flight_tiles_even_far_over_the_cap() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        for x in 0..16u32 {
            for y in 0..16u32 {
                cache.tiles.insert(
                    MapTileId { z: 4, x, y },
                    if (x + y) % 2 == 0 {
                        TileEntry::Pending
                    } else {
                        TileEntry::Fetching
                    },
                );
            }
        }
        assert_eq!(cache.tiles.len(), 256);
        cache.prune_distant_tiles();
        assert_eq!(
            cache.tiles.len(),
            256,
            "in-flight tiles are unevictable, so the cap can be exceeded"
        );
    }

    #[test]
    fn prune_with_a_nan_viewport_centre_still_bounds_the_cache() {
        let mut cache = MapTileCache::new(layer_zoom(0, 19), view(f64::NAN, f64::NAN, 4.0));
        fill_ready_grid(&mut cache, 4, 16);
        assert_eq!(cache.tiles.len(), 256);
        // Every score is NaN → `partial_cmp` returns None → the sort falls back
        // to Equal. The eviction count must still be honoured.
        cache.prune_distant_tiles();
        assert_eq!(cache.tiles.len(), 192);
    }

    #[test]
    fn prune_with_non_finite_zoom_clamps_to_the_layer_band() {
        for zoom in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, 1.0e30, -1.0e30] {
            let mut cache = MapTileCache::new(layer_zoom(0, 19), view(0.0, 0.0, zoom));
            fill_ready_grid(&mut cache, 4, 16);
            cache.prune_distant_tiles();
            assert_eq!(cache.tiles.len(), 192, "zoom {zoom} must still bound the cache");
        }
    }

    #[test]
    fn prune_is_idempotent_once_under_the_cap() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        fill_ready_grid(&mut cache, 4, 16);
        cache.prune_distant_tiles();
        let first = cache.tiles.len();
        let survivors: Vec<MapTileId> = cache.tiles.keys().copied().collect();
        cache.prune_distant_tiles();
        assert_eq!(cache.tiles.len(), first);
        assert_eq!(
            cache.tiles.keys().copied().collect::<Vec<_>>(),
            survivors,
            "a second prune under the cap must change nothing"
        );
    }

    #[test]
    fn prune_drops_the_farthest_tiles_first_across_zoom_levels() {
        // A mixed-zoom cache: same-zoom near tiles must outlive a wrong-zoom
        // tile (the score adds 10_000 per zoom level of mismatch).
        let mut cache = cache_at(0.0, 0.0, 4.0);
        fill_ready_grid(&mut cache, 4, 16);
        let wrong_zoom = MapTileId { z: 9, x: 256, y: 256 };
        cache
            .tiles
            .insert(wrong_zoom, TileEntry::Ready { svg: AzString::from("<svg/>") });
        let near = MapTileId { z: 4, x: 8, y: 8 };
        cache.prune_distant_tiles();
        assert!(cache.tiles.len() <= 192);
        assert!(
            !cache.tiles.contains_key(&wrong_zoom),
            "a zoom-mismatched tile must be evicted before same-zoom neighbours"
        );
        assert!(cache.tiles.contains_key(&near), "the centre tile must survive");
    }

    // ==================================================================
    // merge_map_tile_cache
    // ==================================================================

    #[test]
    fn merge_with_a_wrong_typed_new_dataset_returns_the_old_one_intact() {
        let mut old_cache = cache_at(0.0, 0.0, 5.0);
        let tile = MapTileId { z: 5, x: 1, y: 1 };
        old_cache.mark_tile_ready(tile, AzString::from("<svg/>"));
        let mut merged = merge_map_tile_cache(RefAny::new(0u32), RefAny::new(old_cache));
        let cache = merged.downcast_ref::<MapTileCache>().expect("old cache");
        assert_eq!(cache.viewport.zoom, 5.0, "no adoption from a bogus new dataset");
        assert!(cache.tiles.contains_key(&tile));
    }

    #[test]
    fn merge_with_a_wrong_typed_old_dataset_returns_it_unchanged() {
        let new_cache = cache_at(0.0, 0.0, 9.0);
        let mut merged = merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(7u64));
        assert!(
            merged.downcast_ref::<MapTileCache>().is_none(),
            "the merge must not fabricate a cache out of a wrong-typed dataset"
        );
        assert_eq!(*merged.downcast_ref::<u64>().expect("u64 payload"), 7);
    }

    #[test]
    fn merge_of_two_aliases_of_one_dataset_does_not_panic() {
        // Both handles share one allocation, so the shared borrow taken for
        // `new_data` blocks the exclusive borrow for `old_data`. The merge must
        // degrade to "no adoption" rather than deadlock or panic.
        let dataset = RefAny::new(cache_at(0.0, 0.0, 5.0));
        let mut merged = merge_map_tile_cache(dataset.clone(), dataset);
        let cache = merged.downcast_ref::<MapTileCache>().expect("cache");
        assert_eq!(cache.viewport.zoom, 5.0);
    }

    #[test]
    fn merge_adopts_the_worker_only_when_the_old_cache_has_none() {
        // Old has no worker → adopt the build's.
        let old_cache = cache_at(0.0, 0.0, 5.0);
        let mut new_cache = cache_at(0.0, 0.0, 6.0);
        new_cache.fetch_callback = Some(ThreadCallback::new(noop_worker));
        let mut merged = merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        {
            let cache = merged.downcast_ref::<MapTileCache>().expect("cache");
            let cb = cache.fetch_callback.as_ref().expect("adopted worker");
            assert_eq!(cb.cb as usize, noop_worker as ThreadCallbackType as usize);
        }

        // Old already has one → keep it (the workers already hold its handle).
        let mut old_cache = cache_at(0.0, 0.0, 5.0);
        old_cache.fetch_callback = Some(ThreadCallback::new(noop_worker));
        let mut new_cache = cache_at(0.0, 0.0, 6.0);
        new_cache.fetch_callback = Some(ThreadCallback::new(other_noop_worker));
        let mut merged = merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        let cache = merged.downcast_ref::<MapTileCache>().expect("cache");
        let cb = cache.fetch_callback.as_ref().expect("kept worker");
        assert_eq!(cb.cb as usize, noop_worker as ThreadCallbackType as usize);
    }

    #[test]
    fn merge_adopts_the_build_layer_and_viewport_even_when_degenerate() {
        let old_cache = MapTileCache::new(layer_zoom(0, 19), view(10.0, 20.0, 5.0));
        let new_cache =
            MapTileCache::new(layer_zoom(3, 7), view(f64::NAN, f64::INFINITY, f32::NAN));
        let mut merged = merge_map_tile_cache(RefAny::new(new_cache), RefAny::new(old_cache));
        let cache = merged.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.viewport.centre_lat_deg.is_nan());
        assert!(cache.viewport.zoom.is_nan());
        assert_eq!(cache.layer.min_zoom, 3);
        assert_eq!(cache.layer.max_zoom, 7);
    }

    // ==================================================================
    // build_tile_url  (parser-ish substitution)
    // ==================================================================

    #[test]
    fn build_tile_url_without_placeholders_is_the_identity() {
        let tile = MapTileId { z: 1, x: 2, y: 3 };
        assert_eq!(build_tile_url("", tile), "");
        assert_eq!(build_tile_url("https://t.example/fixed", tile), "https://t.example/fixed");
        // Unknown placeholders are left verbatim, not eaten.
        assert_eq!(build_tile_url("{q}/{Z}/{ x }", tile), "{q}/{Z}/{ x }");
    }

    #[test]
    fn build_tile_url_substitutes_extreme_tile_ids() {
        let tile = MapTileId {
            z: u8::MAX,
            x: u32::MAX,
            y: 0,
        };
        assert_eq!(build_tile_url("{z}/{x}/{y}", tile), "255/4294967295/0");
    }

    #[test]
    fn build_tile_url_handles_unicode_and_unbalanced_braces() {
        let tile = MapTileId { z: 7, x: 8, y: 9 };
        assert_eq!(
            build_tile_url("\u{1F600}/{z}/\u{4F60}\u{597D}/{y}", tile),
            "\u{1F600}/7/\u{4F60}\u{597D}/9"
        );
        assert_eq!(build_tile_url("{{z}}", tile), "{7}");
        assert_eq!(build_tile_url("{z", tile), "{z");
        assert_eq!(build_tile_url("z}", tile), "z}");
    }

    #[test]
    fn build_tile_url_substitution_is_not_re_scanned() {
        // `{z}` expands to a number, so no expansion can create a new
        // placeholder — but a template that already spells one out must not be
        // touched twice either.
        let tile = MapTileId { z: 1, x: 2, y: 3 };
        assert_eq!(build_tile_url("{z}{x}{y}{z}", tile), "1231");
    }

    #[test]
    fn build_tile_url_extremely_long_template_does_not_hang() {
        let template = "{z}/".repeat(100_000);
        let url = build_tile_url(&template, MapTileId { z: 14, x: 0, y: 0 });
        assert_eq!(url.len(), 100_000 * 3);
        assert!(url.starts_with("14/14/"));
    }

    // ==================================================================
    // svg_string_to_dom  (parser)
    // ==================================================================

    // --- xml + cpurender: the SVG is rasterised into an image node ---

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    const MINIMAL_TILE_SVG: &str =
        r#"<svg viewBox="0 0 16 16"><rect x="0" y="0" width="16" height="16" fill="red"/></svg>"#;

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_valid_minimal_is_the_positive_control() {
        assert!(
            svg_string_to_dom(MINIMAL_TILE_SVG).is_some(),
            "a well-formed <svg> must rasterise into a Dom"
        );
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_empty_whitespace_and_garbage_return_none() {
        for bad in [
            "",
            " ",
            "   \t\n\r ",
            "garbage",
            "<<<>>>",
            "<svg",
            "</svg>",
            "<html><body/></html>",
            "\u{0}\u{1}\u{2}",
        ] {
            assert!(
                svg_string_to_dom(bad).is_none(),
                "{bad:?} must be rejected, not rendered"
            );
        }
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_boundary_numeric_attributes_do_not_panic() {
        for svg in [
            r#"<svg viewBox="0 0 16 16"><rect width="0" height="-0" fill="red"/></svg>"#,
            r#"<svg viewBox="0 0 16 16"><rect width="NaN" height="inf" fill="red"/></svg>"#,
            r#"<svg viewBox="0 0 16 16"><rect width="1e400" height="1e-400" fill="red"/></svg>"#,
            r#"<svg viewBox="0 0 16 16"><rect width="9223372036854775807" height="8"/></svg>"#,
            r#"<svg viewBox="0 0 0 0"><rect width="8" height="8" fill="red"/></svg>"#,
            r#"<svg viewBox="NaN NaN NaN NaN"><rect width="8" height="8" fill="red"/></svg>"#,
        ] {
            // Reaching the next iteration means the rasteriser did not panic.
            let _ = svg_string_to_dom(svg).is_some();
        }
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_unicode_content_does_not_panic() {
        let svg = "<svg viewBox=\"0 0 16 16\"><title>\u{1F600} \u{4F60}\u{597D} \
                   e\u{0301} \u{202E}</title><rect width=\"16\" height=\"16\" \
                   fill=\"red\"/></svg>";
        assert!(svg_string_to_dom(svg).is_some());
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_leading_and_trailing_junk_is_deterministic() {
        for svg in [
            "  <svg viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\"/></svg>  ",
            "<svg viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\"/></svg>;garbage",
            "junk<svg viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\"/></svg>",
        ] {
            // Whatever the verdict, it must be stable across calls.
            assert_eq!(
                svg_string_to_dom(svg).is_some(),
                svg_string_to_dom(svg).is_some(),
                "{svg:?} parsed non-deterministically"
            );
        }
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_extremely_long_input_does_not_hang() {
        let svg = alloc::format!(
            "<svg viewBox=\"0 0 8 8\"><desc>{}</desc><rect width=\"8\" height=\"8\" \
             fill=\"red\"/></svg>",
            "a".repeat(1_000_000)
        );
        assert!(svg_string_to_dom(&svg).is_some());
    }

    #[cfg(all(feature = "xml", feature = "cpurender"))]
    #[test]
    fn svg_raster_deeply_nested_groups_do_not_stack_overflow() {
        // The rasteriser recurses per group; give it a generous stack so a real
        // 2000-deep document is a clean test rather than a crash.
        let ok = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(|| {
                const DEPTH: usize = 2_000;
                let svg = alloc::format!(
                    "<svg viewBox=\"0 0 8 8\">{}<rect width=\"8\" height=\"8\" \
                     fill=\"red\"/>{}</svg>",
                    "<g>".repeat(DEPTH),
                    "</g>".repeat(DEPTH)
                );
                svg_string_to_dom(&svg).is_some()
            })
            .expect("spawn")
            .join()
            .expect("2000-deep nesting must not overflow the stack");
        assert!(ok);
    }

    // --- xml without cpurender: the SVG goes through the XML→DOM path ---

    #[cfg(all(feature = "xml", not(feature = "cpurender")))]
    #[test]
    fn svg_dom_valid_minimal_is_the_positive_control() {
        assert!(svg_string_to_dom("<svg><g/></svg>").is_some());
    }

    #[cfg(all(feature = "xml", not(feature = "cpurender")))]
    #[test]
    fn svg_dom_malformed_markup_returns_none() {
        for bad in ["<<<>>>", "<svg", "</svg>", "<a></b>", "\u{0}\u{1}"] {
            assert!(svg_string_to_dom(bad).is_none(), "{bad:?} must be rejected");
        }
    }

    #[cfg(all(feature = "xml", not(feature = "cpurender")))]
    #[test]
    fn svg_dom_empty_whitespace_and_unicode_do_not_panic() {
        for input in ["", " ", "   \t\n\r ", "\u{1F600}", "e\u{0301}"] {
            assert_eq!(
                svg_string_to_dom(input).is_some(),
                svg_string_to_dom(input).is_some()
            );
        }
    }

    #[cfg(all(feature = "xml", not(feature = "cpurender")))]
    #[test]
    fn svg_dom_extremely_long_input_does_not_hang() {
        let svg = alloc::format!("<svg><desc>{}</desc></svg>", "a".repeat(1_000_000));
        let _ = svg_string_to_dom(&svg).is_some();
    }

    // --- no xml feature: the stub always declines ---

    #[cfg(not(feature = "xml"))]
    #[test]
    fn svg_stub_returns_none_for_every_input() {
        for input in ["", "   ", "<svg/>", "<svg><g/></svg>", "\u{1F600}", "<<<>>>"] {
            assert!(svg_string_to_dom(input).is_none());
        }
        let long = "a".repeat(1_000_000);
        assert!(svg_string_to_dom(&long).is_none());
    }

    // ==================================================================
    // User-hook invocation  (invoke_viewport_changed / invoke_pin_tap)
    // ==================================================================

    #[test]
    fn invoke_viewport_changed_without_a_hook_is_do_nothing() {
        let (update, changes) = with_callback_info(|info| {
            invoke_viewport_changed(&OptionMapViewportChanged::None, &info, view(0.0, 0.0, 2.0))
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn invoke_viewport_changed_forwards_even_a_nan_viewport() {
        let mut log = RefAny::new(HookLog::default());
        let hook = OptionMapViewportChanged::Some(MapViewportChanged {
            refany: log.clone(),
            callback: (record_viewport as MapViewportChangedCallbackType).into(),
        });
        let viewport = view(f64::NAN, f64::INFINITY, f32::NAN);
        let (update, _) = with_callback_info(|info| invoke_viewport_changed(&hook, &info, viewport));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(hook_log(&mut log), (1, 0));
    }

    #[test]
    fn invoke_pin_tap_without_a_hook_is_do_nothing() {
        let (update, _) = with_callback_info(|info| {
            invoke_pin_tap(
                &OptionMapPinTap::None,
                &info,
                MapLatLon {
                    lat_deg: 0.0,
                    lon_deg: 0.0,
                },
            )
        });
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn invoke_pin_tap_returns_the_users_update_verbatim() {
        let mut log = RefAny::new(HookLog::default());
        let hook = OptionMapPinTap::Some(MapPinTap {
            refany: log.clone(),
            callback: (record_pin as MapPinTapCallbackType).into(),
        });
        let (update, _) = with_callback_info(|info| {
            invoke_pin_tap(
                &hook,
                &info,
                MapLatLon {
                    lat_deg: f64::NAN,
                    lon_deg: -1.0e300,
                },
            )
        });
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(hook_log(&mut log), (0, 1));
    }

    // ==================================================================
    // Pointer / scroll callbacks
    // ==================================================================

    #[test]
    fn pointer_down_without_a_cursor_is_a_no_op() {
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let (update, _) =
            with_callback_info(|info| map_on_pointer_down(dataset.clone(), info));
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.drag_anchor.is_none());
        assert!(cache.press_origin.is_none());
    }

    #[test]
    fn pointer_down_records_both_anchors() {
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let (update, _) = with_callback_info_at(cursor_at(120.0, 80.0), |info| {
            map_on_pointer_down(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        let anchor = cache.drag_anchor.expect("drag anchor");
        let press = cache.press_origin.expect("press origin");
        assert_eq!((anchor.x, anchor.y), (120.0, 80.0));
        assert_eq!((press.x, press.y), (120.0, 80.0));
    }

    #[test]
    fn pointer_down_on_a_wrong_typed_dataset_is_a_no_op() {
        let dataset = RefAny::new(0u16);
        let (update, _) = with_callback_info_at(cursor_at(1.0, 1.0), |info| {
            map_on_pointer_down(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn pointer_move_without_an_anchor_does_not_pan() {
        let mut dataset = RefAny::new(cache_at(37.0, -122.0, 4.0));
        let (update, _) = with_callback_info_at(cursor_at(500.0, 500.0), |info| {
            map_on_pointer_move(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert_eq!(cache.viewport.centre_lon_deg, -122.0);
        assert_eq!(cache.viewport.centre_lat_deg, 37.0);
    }

    #[test]
    fn pointer_move_pans_by_the_exact_mercator_delta_and_re_anchors() {
        let mut cache = cache_at(0.0, 0.0, 2.0);
        cache.drag_anchor = Some(LogicalPosition::new(100.0, 100.0));
        let mut dataset = RefAny::new(cache);
        let (_, changes) = with_callback_info_at(cursor_at(150.0, 100.0), |info| {
            map_on_pointer_move(dataset.clone(), info)
        });
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        // world_px = 256 * 2^2 = 1024 → d_lon = -50 * 360 / 1024.
        close(cache.viewport.centre_lon_deg, -50.0 * 360.0 / 1024.0, 1e-9);
        close(cache.viewport.centre_lat_deg, 0.0, 1e-9);
        let anchor = cache.drag_anchor.expect("anchor must follow the cursor");
        assert_eq!((anchor.x, anchor.y), (150.0, 100.0));
        drop(cache);
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::UpdateAllVirtualViews)),
            "a pan must request a virtual-view re-render"
        );
    }

    #[test]
    fn pointer_move_ignores_sub_half_pixel_jitter() {
        let mut cache = cache_at(10.0, 20.0, 4.0);
        cache.drag_anchor = Some(LogicalPosition::new(100.0, 100.0));
        let mut dataset = RefAny::new(cache);
        let (update, _) = with_callback_info_at(cursor_at(100.4, 99.7), |info| {
            map_on_pointer_move(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert_eq!(cache.viewport.centre_lon_deg, 20.0, "jitter must not pan");
        assert_eq!(cache.viewport.centre_lat_deg, 10.0);
    }

    #[test]
    fn pointer_move_with_a_nan_viewport_does_not_panic() {
        let mut cache = MapTileCache::new(layer_zoom(0, 19), view(f64::NAN, f64::NAN, f32::NAN));
        cache.drag_anchor = Some(LogicalPosition::new(0.0, 0.0));
        let mut dataset = RefAny::new(cache);
        let (update, _) = with_callback_info_at(cursor_at(400.0, 400.0), |info| {
            map_on_pointer_move(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.viewport.centre_lon_deg.is_nan());
    }

    #[test]
    fn pointer_move_fires_the_viewport_hook_once_per_pan() {
        let mut log = RefAny::new(HookLog::default());
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.drag_anchor = Some(LogicalPosition::new(0.0, 0.0));
        cache.on_viewport_changed = OptionMapViewportChanged::Some(MapViewportChanged {
            refany: log.clone(),
            callback: (record_viewport as MapViewportChangedCallbackType).into(),
        });
        let dataset = RefAny::new(cache);
        let _ = with_callback_info_at(cursor_at(60.0, 60.0), |info| {
            map_on_pointer_move(dataset.clone(), info)
        });
        assert_eq!(hook_log(&mut log), (1, 0));
    }

    #[test]
    fn pointer_up_clears_every_gesture_anchor() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.drag_anchor = Some(LogicalPosition::new(5.0, 5.0));
        cache.pinch_anchor = Some(120.0);
        cache.press_origin = Some(LogicalPosition::new(5.0, 5.0));
        let mut dataset = RefAny::new(cache);
        let (update, _) = with_callback_info_at(cursor_at(5.0, 5.0), |info| {
            map_on_pointer_up(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.drag_anchor.is_none());
        assert!(cache.pinch_anchor.is_none());
        assert!(cache.press_origin.is_none());
    }

    #[test]
    fn pointer_up_fires_pin_tap_for_a_tap_but_not_for_a_drag() {
        // A release within 6px of the press point is a tap.
        let mut log = RefAny::new(HookLog::default());
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.press_origin = Some(LogicalPosition::new(10.0, 10.0));
        cache.on_pin_tap = OptionMapPinTap::Some(MapPinTap {
            refany: log.clone(),
            callback: (record_pin as MapPinTapCallbackType).into(),
        });
        let dataset = RefAny::new(cache);
        let _ = with_callback_info_at(cursor_at(12.0, 12.0), |info| {
            map_on_pointer_up(dataset.clone(), info)
        });
        assert_eq!(hook_log(&mut log), (0, 1), "a 2px release is a tap");

        // A release 90px away is a drag, not a tap.
        let mut log = RefAny::new(HookLog::default());
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.press_origin = Some(LogicalPosition::new(10.0, 10.0));
        cache.on_pin_tap = OptionMapPinTap::Some(MapPinTap {
            refany: log.clone(),
            callback: (record_pin as MapPinTapCallbackType).into(),
        });
        let dataset = RefAny::new(cache);
        let _ = with_callback_info_at(cursor_at(100.0, 100.0), |info| {
            map_on_pointer_up(dataset.clone(), info)
        });
        assert_eq!(hook_log(&mut log), (0, 0), "a 90px release is a drag");
    }

    #[test]
    fn pointer_up_without_a_press_origin_never_taps() {
        let mut log = RefAny::new(HookLog::default());
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.on_pin_tap = OptionMapPinTap::Some(MapPinTap {
            refany: log.clone(),
            callback: (record_pin as MapPinTapCallbackType).into(),
        });
        let dataset = RefAny::new(cache);
        let _ = with_callback_info_at(cursor_at(10.0, 10.0), |info| {
            map_on_pointer_up(dataset.clone(), info)
        });
        assert_eq!(hook_log(&mut log), (0, 0));
    }

    #[test]
    fn pointer_up_on_a_wrong_typed_dataset_is_a_no_op() {
        // A `MapViewport` is the most plausible mix-up: same widget, wrong payload.
        let dataset = RefAny::new(MapViewport::default());
        let (update, _) = with_callback_info_at(cursor_at(1.0, 1.0), |info| {
            map_on_pointer_up(dataset.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn scroll_without_a_wheel_delta_is_a_no_op() {
        // The harness has no hit node, so `get_scroll_delta` yields 0 - the
        // handler must bail before touching the viewport.
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let (update, changes) = with_callback_info(|info| map_on_scroll(dataset.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a zero-delta scroll must queue nothing");
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert_eq!(cache.viewport.zoom, 4.0);
        assert!(cache.tiles.is_empty());
    }

    #[test]
    fn scroll_on_a_wrong_typed_dataset_is_a_no_op() {
        let dataset = RefAny::new(0u8);
        let (update, _) = with_callback_info(|info| map_on_scroll(dataset.clone(), info));
        assert_eq!(update, Update::DoNothing);
    }

    // ==================================================================
    // Fetch spawning + writeback
    // ==================================================================

    #[test]
    fn spawn_pending_tile_fetches_is_a_no_op_without_a_worker() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        for x in 0..4u32 {
            cache.tiles.insert(MapTileId { z: 4, x, y: 8 }, TileEntry::Pending);
        }
        let mut dataset = RefAny::new(cache);
        let (_, changes) = with_callback_info(|info| {
            let mut info = info;
            spawn_pending_tile_fetches(&mut dataset.clone(), &mut info);
        });
        assert!(changes.is_empty(), "no worker → no threads queued");
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(
            cache.tiles.values().all(|e| matches!(e, TileEntry::Pending)),
            "tiles must stay Pending so the placeholder grid renders"
        );
    }

    #[test]
    fn spawn_pending_tile_fetches_caps_the_burst_at_sixteen() {
        let mut cache = cache_at(0.0, 0.0, 4.0);
        cache.fetch_callback = Some(ThreadCallback::new(noop_worker));
        for x in 0..20u32 {
            cache.tiles.insert(MapTileId { z: 4, x, y: 8 }, TileEntry::Pending);
        }
        let mut dataset = RefAny::new(cache);

        // `(fetching, pending)` counts, as a plain fn so the two call sites
        // don't share one inferred closure borrow.
        fn count_states(ds: &mut RefAny) -> (usize, usize) {
            let cache = ds.downcast_ref::<MapTileCache>().expect("cache");
            let fetching = cache
                .tiles
                .values()
                .filter(|e| matches!(e, TileEntry::Fetching))
                .count();
            let pending = cache
                .tiles
                .values()
                .filter(|e| matches!(e, TileEntry::Pending))
                .count();
            (fetching, pending)
        }

        let (_, changes) = with_callback_info(|info| {
            let mut info = info;
            spawn_pending_tile_fetches(&mut dataset.clone(), &mut info);
        });
        assert_eq!(count_states(&mut dataset), (16, 4), "one call spawns at most 16");
        assert_eq!(
            changes
                .iter()
                .filter(|c| matches!(c, CallbackChange::AddThread { .. }))
                .count(),
            16
        );

        // The second call drains the remainder - the cap bounds a burst, it
        // does not drop work.
        let _ = with_callback_info(|info| {
            let mut info = info;
            spawn_pending_tile_fetches(&mut dataset.clone(), &mut info);
        });
        assert_eq!(count_states(&mut dataset), (20, 0));
    }

    #[test]
    fn spawn_pending_tile_fetches_on_a_wrong_typed_dataset_is_a_no_op() {
        let mut dataset = RefAny::new(1234u32);
        let (_, changes) = with_callback_info(|info| {
            let mut info = info;
            spawn_pending_tile_fetches(&mut dataset, &mut info);
        });
        assert!(changes.is_empty());
    }

    #[test]
    fn tile_writeback_marks_ready_on_an_empty_error_and_failed_otherwise() {
        let tile = MapTileId { z: 4, x: 1, y: 2 };
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));

        let ok = RefAny::new(TileReadyMsg {
            tile,
            svg: AzString::from("<svg/>"),
            error: AzString::from(""),
        });
        let (update, changes) = with_callback_info(|info| {
            map_tile_writeback(dataset.clone(), ok.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes
            .iter()
            .any(|c| matches!(c, CallbackChange::UpdateAllVirtualViews)));
        {
            let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
            assert!(matches!(cache.tiles.get(&tile), Some(TileEntry::Ready { .. })));
        }

        let failed = RefAny::new(TileReadyMsg {
            tile,
            svg: AzString::from(""),
            error: AzString::from("404"),
        });
        let (update, _) = with_callback_info(|info| {
            map_tile_writeback(dataset.clone(), failed.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(matches!(cache.tiles.get(&tile), Some(TileEntry::Failed { .. })));
    }

    #[test]
    fn tile_writeback_accepts_a_huge_payload_and_an_out_of_range_id() {
        let tile = MapTileId {
            z: 31,
            x: u32::MAX,
            y: u32::MAX,
        };
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let msg = RefAny::new(TileReadyMsg {
            tile,
            svg: AzString::from("<svg/>".repeat(50_000)),
            error: AzString::from(""),
        });
        let (update, _) =
            with_callback_info(|info| map_tile_writeback(dataset.clone(), msg.clone(), info));
        assert_eq!(update, Update::DoNothing);
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.tiles.contains_key(&tile));
    }

    #[test]
    fn tile_writeback_with_a_wrong_typed_message_is_a_no_op() {
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let (update, changes) = with_callback_info(|info| {
            map_tile_writeback(dataset.clone(), RefAny::new(0u32), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a bogus message must not force a re-render");
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        assert!(cache.tiles.is_empty());
    }

    #[test]
    fn tile_writeback_with_a_wrong_typed_cache_is_a_no_op() {
        let msg = RefAny::new(TileReadyMsg {
            tile: MapTileId { z: 1, x: 0, y: 0 },
            svg: AzString::from("<svg/>"),
            error: AzString::from(""),
        });
        let (update, changes) = with_callback_info(|info| {
            map_tile_writeback(RefAny::new(9u64), msg.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn after_mount_installs_the_sweep_timer_and_asks_for_a_re_render() {
        let dataset = RefAny::new(cache_at(0.0, 0.0, 4.0));
        let (update, changes) =
            with_callback_info(|info| map_on_after_mount(dataset.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            changes
                .iter()
                .filter(|c| matches!(c, CallbackChange::AddTimer { .. }))
                .count(),
            1,
            "exactly one sweep timer per mount"
        );
        assert!(changes
            .iter()
            .any(|c| matches!(c, CallbackChange::UpdateAllVirtualViews)));
    }

    #[test]
    fn after_mount_on_a_wrong_typed_dataset_still_installs_the_timer() {
        // The timer is unconditional; only the fetch spawn depends on the cache.
        let dataset = RefAny::new(0u8);
        let (update, changes) =
            with_callback_info(|info| map_on_after_mount(dataset.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert!(changes
            .iter()
            .any(|c| matches!(c, CallbackChange::AddTimer { .. })));
    }

    // ==================================================================
    // map_widget_render  (VirtualView callback)
    // ==================================================================

    #[test]
    fn render_with_non_finite_or_empty_bounds_emits_no_dom() {
        let dataset = RefAny::new(cache_at(0.0, 0.0, 3.0));
        for (w, h) in [
            (0.0_f32, 0.0_f32),
            (0.0, 600.0),
            (800.0, 0.0),
            (-800.0, -600.0),
            (f32::NAN, 600.0),
            (800.0, f32::NAN),
            (f32::INFINITY, 600.0),
            (800.0, f32::NEG_INFINITY),
        ] {
            let ret =
                with_virtual_view_info(w, h, |info| map_widget_render(dataset.clone(), info));
            assert!(
                rendered_child_count(&ret).is_none(),
                "bounds {w}x{h} must render nothing until layout settles"
            );
        }
    }

    #[test]
    fn render_with_a_wrong_typed_dataset_emits_no_dom() {
        let dataset = RefAny::new(0u32);
        let ret =
            with_virtual_view_info(800.0, 600.0, |info| map_widget_render(dataset.clone(), info));
        assert!(rendered_child_count(&ret).is_none());
    }

    #[test]
    fn render_marks_every_visible_tile_pending_and_emits_one_div_each() {
        let mut dataset = RefAny::new(cache_at(0.0, 0.0, 2.0));
        let expected = map_visible_tiles(
            &view(0.0, 0.0, 2.0),
            LogicalSize::new(800.0, 600.0),
            &layer_zoom(0, 19),
        );
        let ret =
            with_virtual_view_info(800.0, 600.0, |info| map_widget_render(dataset.clone(), info));
        assert_eq!(
            rendered_child_count(&ret),
            Some(expected.len()),
            "one div per visible tile"
        );
        let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
        for tile in &expected {
            assert!(
                matches!(cache.tiles.get(tile), Some(TileEntry::Pending)),
                "tile {tile:?} must be queued by the render pass"
            );
        }
    }

    #[test]
    fn render_reports_the_bounds_back_as_the_scroll_size() {
        let dataset = RefAny::new(cache_at(0.0, 0.0, 2.0));
        let ret =
            with_virtual_view_info(640.0, 480.0, |info| map_widget_render(dataset.clone(), info));
        assert_eq!(ret.materialized.size.width, 640.0);
        assert_eq!(ret.materialized.size.height, 480.0);
        assert_eq!(ret.virtual_rect.size.width, 640.0);
        assert_eq!(ret.virtual_rect.size.height, 480.0);
        assert_eq!((ret.materialized.origin.x, ret.materialized.origin.y), (0.0, 0.0));
        assert_eq!(
            (ret.virtual_rect.origin.x, ret.virtual_rect.origin.y),
            (0.0, 0.0)
        );
    }

    #[test]
    fn render_falls_back_to_a_glyph_when_a_ready_tile_holds_garbage() {
        // A worker can hand back anything; an unparseable payload must degrade
        // to the placeholder text child, not panic the render pass.
        let mut cache = cache_at(0.0, 0.0, 2.0);
        for tile in map_visible_tiles(
            &view(0.0, 0.0, 2.0),
            LogicalSize::new(512.0, 512.0),
            &layer_zoom(0, 19),
        ) {
            cache.mark_tile_ready(tile, AzString::from("not xml at all <<<"));
        }
        let dataset = RefAny::new(cache);
        let ret =
            with_virtual_view_info(512.0, 512.0, |info| map_widget_render(dataset.clone(), info));
        assert!(
            rendered_child_count(&ret).is_some_and(|n| n > 0),
            "garbage tiles still render their placeholder"
        );
    }

    #[test]
    fn render_handles_mixed_tile_states_including_failures() {
        let mut cache = cache_at(0.0, 0.0, 2.0);
        let tiles = map_visible_tiles(
            &view(0.0, 0.0, 2.0),
            LogicalSize::new(512.0, 512.0),
            &layer_zoom(0, 19),
        );
        for (i, tile) in tiles.iter().enumerate() {
            match i % 4 {
                0 => {
                    cache.tiles.insert(*tile, TileEntry::Pending);
                }
                1 => {
                    cache.tiles.insert(*tile, TileEntry::Fetching);
                }
                2 => cache.mark_tile_failed(*tile, AzString::from("\u{1F600} failed")),
                _ => cache.mark_tile_ready(*tile, AzString::from("")),
            }
        }
        let dataset = RefAny::new(cache);
        let ret =
            with_virtual_view_info(512.0, 512.0, |info| map_widget_render(dataset.clone(), info));
        assert_eq!(rendered_child_count(&ret), Some(tiles.len()));
    }

    #[test]
    fn render_clamps_an_out_of_band_zoom_and_stays_bounded() {
        // A single-zoom layer with a viewport far outside it: the grid must
        // collapse onto the one supported zoom, not enumerate a huge range.
        //
        // NOTE: `min_zoom > max_zoom` is deliberately NOT exercised here - the
        // `i32::clamp(min, max)` in `map_widget_render` panics on an inverted
        // band (see the report accompanying these tests).
        for zoom in [0.0_f32, 1.0, 3.0, 25.0, f32::INFINITY] {
            let cache = MapTileCache::new(layer_zoom(3, 3), view(0.0, 0.0, zoom));
            let dataset = RefAny::new(cache);
            let ret = with_virtual_view_info(800.0, 600.0, |info| {
                map_widget_render(dataset.clone(), info)
            });
            let n = rendered_child_count(&ret).expect("a finite box must render");
            assert!(n > 0 && n < 4096, "zoom {zoom} produced {n} tiles");
        }
    }

    #[test]
    fn render_is_stable_across_repeated_invocations() {
        let dataset = RefAny::new(cache_at(48.1372, 11.5756, 5.0));
        let first =
            with_virtual_view_info(800.0, 600.0, |info| map_widget_render(dataset.clone(), info));
        let second =
            with_virtual_view_info(800.0, 600.0, |info| map_widget_render(dataset.clone(), info));
        assert_eq!(rendered_child_count(&first), rendered_child_count(&second));
    }
}
