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
    pub fn dom(self) -> Dom {
        let cache = MapTileCache::new(self.layer.clone(), self.viewport);
        let dataset = RefAny::new(cache);
        let virtual_view_data = dataset.clone();

        Dom::create_div()
            .with_dataset(OptionRefAny::Some(dataset))
            .with_merge_callback(merge_map_tile_cache as DatasetMergeCallbackType)
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
    /// `Pending` while the fetch / decode is in flight; absent
    /// otherwise. `BTreeMap` for deterministic iteration so the
    /// debug log + e2e snapshots are stable.
    pub tiles: BTreeMap<MapTileId, TileEntry>,
}

impl MapTileCache {
    pub fn new(layer: MapTileLayer, viewport: MapViewport) -> Self {
        Self {
            layer,
            viewport,
            tiles: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TileEntry {
    /// Fetch / decode is in flight. The next tick may patch this
    /// in place to `Ready` or `Failed`.
    Pending,
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
            // Keep the freshest viewport (the one the layout pass
            // just attached) — only inherit tile bytes.
        }
    }
    new_data
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

    // Build the visible-tile grid. Each tile div is GPU-translated
    // into its screen position; the (CSS-driven) `transform` keeps
    // pan / zoom O(1) — no relayout per frame.
    let mut grid = Dom::create_div().with_css(
        "position: absolute; left: 0; top: 0; width: 100%; height: 100%; overflow: hidden;",
    );

    for x in x_min..=x_max {
        for y in y_min..=y_max {
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

            // Stamp the tile id as a child text so users can confirm
            // the grid math without a real renderer. Replaced with
            // the decoded SVG DOM in the follow-up tick.
            tile_div = tile_div.with_child(
                Dom::create_text(alloc::format!("z{}/{}/{}", z_int, x, y))
                    .with_css("position: absolute; left: 4px; top: 4px; font-size: 11px; color: #888;"),
            );

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
