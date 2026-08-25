# 06 — MVT vector tiles + map widget; printpdf integration (render + export)

**Scope:** research-only inventory for SUPER_PLAN_2 §1 items **#11 (MVT + `<MapWidget>` backed by openfreemap)** and **#12 (PDF via printpdf, both directions: render-inline + export-display-list)**. Produces the implementation brief for the next session. Web/W3C primitives are noted so a future WASM backend has a target.

**Architecture anchors (from §0 of SUPER_PLAN_2, verified in this branch):**
- `NodeType::Image(ImageRef)` precedent — `core/src/dom.rs:629`. New asset-bearing variants follow the same `BoxOrStatic` discipline (16 B inline tag, heap payload). For MVT we add `NodeType::MapTile(MapTileSource)`; for PDF we add `NodeType::Pdf(BoxOrStatic<PdfRef>)`.
- `DisplayListItem` — `layout/src/solver3/display_list.rs:584`. New variants slot here. **Critical pre-existing variant:** `DisplayListItem::TextLayout { layout: Arc<dyn Any+Send+Sync>, bounds, font_hash, font_size_px, color }` at `display_list.rs:631` — already commented `"This is pushed BEFORE the individual Text items"` and explicitly tagged "for PDF, accessibility, etc." in the producer at `display_list.rs:1605`. **The PDF export pipeline already has the high-fidelity text layout it needs**; `cpurender.rs:2711` skips this variant deliberately because pixel rendering doesn't need it. This is half of Direction-B already wired.
- CPU renderer — `layout/src/cpurender.rs`. Filled paths via `agg_fill_path_clipped` (cpurender.rs:1291) and stroked paths via `agg::ConvDash` (cpurender.rs:3365) are already used for SVG/box shadow / scrollbar arrows. MVT polygons/lines route through the same agg-rust path APIs; no new low-level primitive needed.
- WebRender GPU display list — `webrender/api/src/display_item.rs:163`. The `DisplayItem` enum is closed (rectangle, line, border, image, gradients, iframe, …); there is **no generic custom-draw-item / extension point**. New visual primitives need either (a) a new variant in this enum + scene_building + renderer shader, or (b) be expressed as composites of existing items (Image for a pre-rasterised map tile texture; Rectangle/Line/Border for individual MVT features). Path (b) is the lower-risk route and matches the precedent: SVG today is rendered to an `ImageRef` (raster) and embedded as `DisplayListItem::Image`; lyon-tessellated SVG primitives go through the same line/rect machinery.
- Manager pattern — `layout/src/managers/` (`gesture.rs`, `focus_cursor.rs`, `scroll_state.rs`, `selection.rs`, `text_input.rs`, `virtual_view.rs`, …). New: `map.rs`, `pdf.rs`.
- Injection seam — `dll/src/desktop/shell2/<platform>/mod.rs`. Both features stay platform-agnostic at the core (HTTP + parsing + agg-rust), with thin platform layers only for the PDF *save dialog* / *MediaStore* (uses File-Picker work from research/04).
- CallbackInfo — `layout/src/callbacks.rs`. New accessors: `get_map_state(node_id) -> Option<MapState>`, `get_map_features_at_lnglat(...)`, `export_to_pdf(path, options)`.
- HTTP — `ureq = "3.3"` is already a `layout/Cargo.toml` dep behind the `http` feature flag (`layout/Cargo.toml:83, 247`) with pure-Rust TLS via `rustls`+`rustls-rustcrypto`+`webpki-roots`. **No new HTTP stack needed.** Reuse the same dep for tile fetching.
- Glyph index machinery — `core/src/ui_solver.rs:42` `GlyphInstance { index: u32, point, size }`. `DisplayListItem::Text` (display_list.rs:639) already carries `Vec<GlyphInstance>` with positioned glyph indices. This is exactly what printpdf needs.

---

## PART 1 — MVT (Mapbox Vector Tile) + `<MapWidget>`

### 1.1 The MVT format (spec 2.1)

The Mapbox Vector Tile spec 2.1 — https://github.com/mapbox/vector-tile-spec/tree/master/2.1 — defines a tile as a protobuf-encoded message whose top-level type is `Tile { repeated Layer layers = 3; }`. The `.proto` is at https://github.com/mapbox/vector-tile-spec/blob/master/2.1/vector_tile.proto.

**Per layer:**
- `name: string` — source-layer name (e.g. `"water"`, `"road"`, `"place"`).
- `extent: uint32` — internal grid size; spec default 4096. Top-left origin, x→right, y→down.
- `keys: repeated string` — string pool for attribute keys.
- `values: repeated Value` — typed value pool (string / float / double / int / uint / sint / bool).
- `features: repeated Feature`.

**Per feature:**
- `id: uint64` (optional).
- `tags: repeated uint32` — flat pairs of `[key_index, value_index, key_index, value_index, …]` indexing into the layer's `keys`/`values` pools.
- `type: GeomType` — `UNKNOWN(0) | POINT(1) | LINESTRING(2) | POLYGON(3)`.
- `geometry: repeated uint32` — command stream (see below).

**Geometry encoding (verified against spec):**
- A `CommandInteger = (id & 0x7) | (count << 3)` — the low 3 bits are the command, the high 29 bits the parameter count.
- Commands: `MoveTo = 1` (2 params: dx, dy), `LineTo = 2` (2 params: dx, dy), `ClosePath = 7` (0 params).
- Parameters are **zigzag-encoded** signed deltas from the cursor: `ParameterInteger = (value << 1) ^ (value >> 31)` — same scheme as protobuf `sint32`.
- A POINT geometry is `MoveTo(N) [dx dy] × N`. LINESTRING is `MoveTo(1) dx dy LineTo(N) [dx dy] × N`. POLYGON is one or more linear rings, each `MoveTo(1) LineTo(K) ClosePath`. Polygon winding determines exterior vs interior (CCW = exterior in screen coords with y-down, per spec note).

### 1.2 The openfreemap tile endpoint (verified live)

- **TileJSON catalogue:** `GET https://tiles.openfreemap.org/planet` returns `tilejson` 3.0.0. Fields observed:
  - `tiles: ["https://tiles.openfreemap.org/planet/20260513_001001_pt/{z}/{x}/{y}.pbf"]` — date-pinned path with `YYYYMMDD_HHMMSS_pt` versioning. Refreshed weekly.
  - `minzoom: 0`, `maxzoom: 14`.
  - `bounds: [-180.0, -85.05113, 180.0, 85.05113]`.
  - `vector_layers`: 15 OpenMapTiles-spec layers (the canonical set):
    `aerodrome_label, aeroway, boundary, building, housenumber, landcover, landuse, mountain_peak, park, place, poi, transportation, transportation_name, water, water_name, waterway`. Each entry lists per-attribute types and min/max zooms.
  - `attribution: "<a href=\"https://openfreemap.org\">OpenFreeMap</a> <a href=\"https://www.openmaptiles.org/\">© OpenMapTiles</a> Data from <a href=\"https://www.openstreetmap.org/copyright\">OpenStreetMap</a>"` — must surface this string in the UI.

- **Single-tile fetch:** `GET https://tiles.openfreemap.org/planet/<date>/<z>/<x>/<y>.pbf`. Verified headers (z=0,x=0,y=0):
  - `HTTP/2 200`
  - `content-type: application/vnd.mapbox-vector-tile`
  - `access-control-allow-origin: *` (CORS open — important for the future Web/WASM backend)
  - `cache-control: public, max-age=315360000` (10 years; safe to cache aggressively client-side)
  - `etag: W/"..."` — supports conditional revalidation
  - `cf-cache-status: HIT` — served via Cloudflare CDN
  - `last-modified: Wed, 13 May 2026 06:06:36 GMT`

- **Style URL:** `GET https://tiles.openfreemap.org/styles/liberty` returns MapLibre style spec v8 JSON pointing back at the planet TileJSON. Sister style endpoints: `/styles/bright`, `/styles/positron`, `/styles/3d`. Layer list inside `liberty.json` is a few hundred entries with `paint`/`layout`/`filter` per layer.

- **Fonts (label rendering):** style references `glyphs: "https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf"` — SDF glyph atlases keyed by font name + Unicode codepoint range (256 codepoints per range, e.g. `0-255`, `256-511`, …). MapLibre/Mapbox-GL-JS standard. **Skip in v1**; render labels with Azul's existing `text3` allsorts pipeline using whatever font the user picks. Glyph PBF support is post-v1.

- **Sprites (icons):** style references `sprite: "https://tiles.openfreemap.org/sprites/ofm_f384/ofm"` — pairs of `<sprite>.json` + `<sprite>.png` (and `@2x` variants). Used by `symbol` layers (POI icons). v1 skips symbol layers.

- **License:** code MIT, **data ODbL** (OpenStreetMap Open Database License). https://openstreetmap.org/copyright. Attribution required (string above). v1 ships an `AttributionLabel` overlay as a non-removable widget child of `MapWidget`.

### 1.3 Rust crates surveyed

| Crate | Version | Purpose | License | Verdict |
|---|---|---|---|---|
| [`mvt`](https://crates.io/crates/mvt) (`DougLau`) | 0.13.0 | Encode + decode MVT; types `Tile`, `Layer`, `Feature`, `GeomEncoder`, `GeomData`, `GeomType` | MIT/Apache-2.0 | **Decoder + encoder.** Uses `prost ^0.14`. Preferred — fewest deps, both directions. https://docs.rs/mvt/latest/mvt/ |
| [`mvt-reader`](https://crates.io/crates/mvt-reader) | 2.3.0 | Decode-only; `Reader::new(&[u8]) -> Reader` with `get_layer_names()`, `get_features(idx)`. Uses `prost`, exposes `geo-types` geometries | MIT | Lighter (decode-only); ships WASM target. Use if we want a `geo-types` boundary instead of mvt's own types. https://docs.rs/mvt-reader/ |
| [`geozero`](https://crates.io/crates/geozero) | (latest) | Vector-data translation layer; sinks MVT into GeoJSON / WKT / GeoArrow / Postgres / SVG. TODO: verify current version | MIT/Apache-2.0 | Heavy. Unneeded for our render path — `mvt` decoder → agg-rust path is enough. Skip. |
| [`prost`](https://crates.io/crates/prost) | 0.14 | Protobuf runtime (already pulled transitively by `mvt`) | Apache-2.0 | Transitive only. No direct dep. |
| [`tile-grid`](https://crates.io/crates/tile-grid) | (TODO: verify) | Slippy-map tile addressing helpers (lat/lon ↔ tile xy at zoom) | Apache-2.0 | Useful but a 20-line module of our own is enough; skip the dep. |
| [`map-tile`](https://crates.io/crates/map-tile) / [`slippy-map-tiles`](https://crates.io/crates/slippy-map-tiles) | various | Same as above | various | Skip. |

**Recommended pick:** `mvt = "0.13"` for decode (and re-use for encode if we add a "save current tile cache" path later).

### 1.4 Style spec subset

Full MapLibre style spec — https://maplibre.org/maplibre-style-spec/ — has top-level keys `version, name, metadata, center, zoom, pitch, bearing, sources, layers, sprite, glyphs, light, sky, terrain, projection, transition, …`. Layers carry `id, type, source, source-layer, filter, paint, layout, minzoom, maxzoom`. Layer types: `background, fill, line, symbol, circle, raster, fill-extrusion, heatmap, hillshade`.

**v1 supported subset (smallest deliverable that renders openfreemap/liberty acceptably):**

| Style feature | v1 status | Notes |
|---|---|---|
| `type: "background"` | yes | Single fill colour for the canvas; one `DisplayListItem::Rect`. |
| `type: "fill"` | yes | Map MVT `Polygon` features → `agg_fill_path` (CPU) / WR `Rectangle`-or-tessellated `Border` per ring (GPU stopgap; see §1.6 GPU). |
| `type: "line"` | yes | `Linestring` features → agg stroked paths with `line-width` from paint; dash patterns via `ConvDash`. |
| `type: "symbol"` | **no** (v1) | Requires sprite + SDF glyph PBF loader. Defer to v2; punt POI/place names. |
| `type: "circle"` | yes (cheap) | Each MVT point → fill circle of `circle-radius` px. |
| `type: "raster"` | yes | OpenFreeMap's natural-earth shading layer uses a `tileSize: 256` raster source `ne2sr`; render as `DisplayListItem::Image`. |
| `type: "fill-extrusion"` | no (v1) | Defer — needs perspective. |
| `type: "heatmap"` / `hillshade` | no | Out of scope. |
| Paint `interpolate` expressions | partial | Support `["interpolate", ["linear", …]]` and `["interpolate", ["exponential", base], …]` over `["zoom"]`; that covers most of openfreemap's paint. https://maplibre.org/maplibre-style-spec/expressions/ |
| Paint `step` expressions | yes | Zoom-stepped colour/width. |
| Paint `case` / `match` | partial | Support `["==", ["get", "class"], "value"]` filters and case-based paint colour. |
| Paint `get` / `feature-state` | yes (get) | Read feature attributes from the decoded `tags` pool. |
| Layout `text-field`, `text-font` | no (v1) | Symbol-only; out of scope. |

**Realistic estimate:** subset above is ~10–15% of the full spec, but it's the part Liberty/Bright/Positron actually use for polygon + line layers. POIs and labels (symbol layers) account for ~30% of any production map's visual complexity and are deferred.

### 1.5 `<MapWidget>` integration sketch

#### 1.5.1 New types

In `core/src/dom.rs` next to `NodeType::Image`:

```rust
// core/src/dom.rs additions
#[repr(C, u8)]
pub enum NodeType {
    // ... existing variants ...
    /// Vector map widget. Payload references a MapTileSource (URL template + style)
    /// and carries a viewport (lat/lon center + zoom + bearing + pitch).
    MapTile(BoxOrStatic<MapTileSource>),
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MapTileSource {
    pub tile_url_template: AzString,    // "https://tiles.openfreemap.org/planet/20260513_001001_pt/{z}/{x}/{y}.pbf"
    pub style: MapStyle,                // inline JSON or AzString URL to fetch
    pub viewport: MapViewport,
    pub min_zoom: u8,                   // openfreemap planet: 0
    pub max_zoom: u8,                   // openfreemap planet: 14
    pub attribution: AzString,          // required surface
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MapViewport {
    pub center_lng: f64,    // degrees
    pub center_lat: f64,
    pub zoom: f32,          // fractional zoom (Web Mercator)
    pub bearing_deg: f32,   // rotation around center, 0 = north up
    pub pitch_deg: f32,     // tilt, 0 = flat, max 60
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum MapStyle {
    InlineJson(AzString),               // full MapLibre style spec JSON
    StyleUrl(AzString),                 // fetched lazily; e.g. ".../styles/liberty"
    Builtin(BuiltinStyle),              // OpenFreeMap presets; zero network
}

#[repr(C)]
pub enum BuiltinStyle { Liberty, Bright, Positron, Dark }
```

`MapTileSource` is heap-only (no static variant needed); `BoxOrStatic::heap(...)`.

#### 1.5.2 Manager

`layout/src/managers/map.rs` (new):

```rust
pub struct MapTileManager {
    /// Per-MapWidget node state. Keyed by DomNodeId.
    states: BTreeMap<DomNodeId, MapState>,
    /// LRU cache of decoded tiles; key = (source_id, z, x, y, date_string).
    tile_cache: lru::LruCache<TileKey, Arc<DecodedTile>>,
    /// In-flight fetches; key = TileKey, value = JoinHandle / completion sentinel.
    inflight: BTreeMap<TileKey, FetchState>,
    /// HTTP client (built once, lifetime = manager).
    http: ureq::Agent,                  // already in layout/Cargo.toml under http feature
    /// Decoded styles, keyed by source URL or builtin id.
    style_cache: BTreeMap<StyleKey, Arc<ParsedStyle>>,
}

pub struct MapState {
    pub current_viewport: MapViewport,
    /// Drag accumulator for pan; consumed once per frame to mutate viewport.
    pub pending_pan_dx_px: f32,
    pub pending_pan_dy_px: f32,
    /// Wheel/pinch zoom accumulator.
    pub pending_zoom_delta: f32,
    /// Visible tiles this frame (z,x,y).
    pub visible_tiles: Vec<TileKey>,
}

pub struct DecodedTile {
    pub layers: Vec<DecodedLayer>,
    pub fetched_at: Instant,
}

pub struct DecodedLayer {
    pub name: AzString,
    pub features: Vec<DecodedFeature>,
}

pub struct DecodedFeature {
    pub id: Option<u64>,
    pub geom_type: GeomType,            // re-export of mvt::GeomType
    pub geometry: Vec<Vec<LogicalPosition>>,  // outer Vec = sub-paths, inner = points in tile coords (0..4096)
    pub attrs: BTreeMap<AzString, FeatureValue>,
}
```

`MapTileManager::tick(viewport, viewport_px_size)` computes the visible tile set:

1. Convert viewport center + zoom to Web Mercator pixel coords via the standard formula:
   - `n = 2^z`
   - `x_tile = (lng + 180) / 360 * n`
   - `y_tile = (1 - asinh(tan(lat_rad)) / π) / 2 * n`
2. Tile range = `floor(x_tile - half_w/256) ..= floor(x_tile + half_w/256)` (similar for y).
3. For each tile, check cache; on miss, enqueue fetch.
4. Drop tiles outside the LRU horizon (default 256 tiles ≈ 2× a 1080p viewport at z=12).

Tile fetches go through `ureq::Agent::get(url).call()` on a small dedicated thread pool (rayon or a one-shot `std::thread::spawn` per fetch with a bounded semaphore). On completion, decode via `mvt::Tile::decode(&bytes)` and slot into `tile_cache`. The next layout pass picks them up — same lazy-update pattern as `IFrameManager`'s `lazy loading` (`layout/src/managers/iframe.rs`, see also `IFRAME_INVESTIGATION_REPORT.md`).

#### 1.5.3 Display-list emission

Three options for rendering map content; recommendation: **option B for v1**.

**Option A — pre-rasterise to ImageRef.** Render the whole MapWidget's content (all visible MVT features through agg-rust) into a single `AzulPixmap`, wrap in `ImageRef::new_rawimage`, emit one `DisplayListItem::Image`. Pros: zero new display-list variants; reuses entire ImageRef machinery (texture upload, WebRender pipeline, hit-test). Cons: lose vector-perfect scaling on transforms; full re-rasterisation on viewport change.

**Option B — emit features as existing primitives (recommended for v1).** In the MapWidget's layout pass, walk visible tiles → visible features → emit:
- `DisplayListItem::Rect` for fill polygons (with no border-radius) per tessellated polygon. Tessellation via `lyon::tessellation::FillTessellator` (already a dep behind `svg` feature in `layout/Cargo.toml:36`), feeding `lyon::path::Path` from the decoded MVT geometry. Each output triangle/fan becomes a small Rect set; or push as a single tessellated mesh via a new variant — see Option C.
- `DisplayListItem::LinearGradient` for stripe/dash patterns — not great fit. Better: a small new variant.
- New variant `DisplayListItem::FilledPath { bounds, path: Arc<Path>, color, transform }` — adds one enum tag in display_list.rs. CPU renderer routes to `agg_fill_path_clipped`; GPU stopgap rasterises into an ImageRef offscreen and falls back to Image.

  Actually clearer: introduce a single new variant **`DisplayListItem::MapTileLayer { bounds, clip_rect, features: Arc<Vec<DecodedFeature>>, style_layer: Arc<StyleLayer>, viewport: MapViewport, dpi_factor: f32 }`** and let the renderer do the per-feature loop. This keeps display-list emission O(#tiles × #style layers) instead of O(#features); critical because a single z=14 tile can carry 10k features and emitting a `DisplayListItem` per feature is unreasonable.

**Option C — full GPU path with a custom WebRender primitive.** Add a new `DisplayItem::MvtTile` variant in `webrender/api/src/display_item.rs` and wire it through scene_building / frame_builder / a new shader stage. Cost: weeks of WebRender work; benefit unclear vs offscreen-render-to-texture (the SVG path is the precedent here — `layout/src/xml/svg.rs` rasterises SVG to `ImageRef` and never adds a custom WR variant). **Defer.**

**Therefore:** v1 ships with `DisplayListItem::MapTileLayer { … }` (or **`MapTileBlock`** for clarity), where:
- CPU path (`cpurender.rs`): iterates `features`, applies the `style_layer` paint, calls `agg_fill_path_clipped` for fills and `agg`'s stroked-path machinery for lines. Already-existing primitives.
- GPU path: a method `MapTileBlock::rasterise_to_image_ref(dpi, font_manager) -> ImageRef` is called by the WR translator (`dll/src/desktop/compositor2.rs`), result is uploaded as a normal `WrImage` and emitted as a WR `Image` display item. Same pattern as SVG today.

The downside is the GPU path doesn't keep vector crispness on transforms — but a map widget is panned/zoomed by re-laying-out (changing `MapViewport.zoom` triggers a new tile-set + a new MapTileBlock), so this is acceptable.

#### 1.5.4 Style resolver

`layout/src/map/style.rs` (new submodule under a new `pub mod map;` in `layout/src/lib.rs`):

```rust
pub struct ParsedStyle {
    pub layers: Vec<StyleLayer>,
    pub background_color: Option<ColorU>,
    pub sources: BTreeMap<AzString, StyleSource>,
}

pub struct StyleLayer {
    pub id: AzString,
    pub source_layer: AzString,         // e.g. "water"
    pub kind: StyleLayerKind,           // Fill / Line / Symbol / Circle
    pub filter: Option<StyleFilter>,
    pub paint: StylePaint,
    pub min_zoom: Option<f32>,
    pub max_zoom: Option<f32>,
}

pub enum StyleLayerKind { Background, Fill, Line, Symbol, Circle, Raster }

pub struct StylePaint {
    pub fill_color: Option<ResolvedExpr<ColorU>>,
    pub fill_opacity: Option<ResolvedExpr<f32>>,
    pub line_color: Option<ResolvedExpr<ColorU>>,
    pub line_width: Option<ResolvedExpr<f32>>,
    pub line_dasharray: Option<Vec<f32>>,
    // ... v1 subset only
}

pub enum ResolvedExpr<T> {
    Const(T),
    InterpolateZoom { kind: InterpolateKind, stops: Vec<(f32, T)> },  // f32 = zoom
    Step { input: ExprInput, stops: Vec<(f32, T)> },
    Case { branches: Vec<(StyleFilter, T)>, default: T },
}
```

Filter language subset: `["==", ["get", "<key>"], "<value>"]`, `["!=", …]`, `["in", ["get", "<key>"], [<values>]]`, `["all", filter, filter, …]`, `["any", …]`. The full filter language is ~20 op codes — https://maplibre.org/maplibre-style-spec/expressions/.

`serde_json` is already a layout dep (`layout/Cargo.toml:92`); parsing the style JSON is straightforward.

#### 1.5.5 Events

New `On` variants (`core/src/dom.rs:1124` family):
- `On::MapMove` — fires after viewport changes (pan / zoom / rotate). Payload accessor: `callback_info.get_map_viewport(node_id) -> Option<MapViewport>`.
- `On::MapClick` — fires on click within MapWidget; payload accessor: `callback_info.get_map_click_features(node_id) -> Option<MapClickInfo>` where:
  ```rust
  pub struct MapClickInfo {
      pub lng: f64,
      pub lat: f64,
      pub features: Vec<DecodedFeature>,   // hit-tested features under cursor
  }
  ```
- `On::MapTilesLoaded` — fires when the visible tile set finishes loading (useful for tests / screenshot triggers).

The pan/zoom interaction reuses `GestureManager` — pinch → zoom delta, drag → pan delta, mouse wheel → zoom delta — all already wired via the existing `inject_native_gesture` machinery from Sprint M. No new platform code needed.

Hit-test feature lookup: at the moment of `MapClick`, the manager iterates visible tiles, transforms cursor (px) into tile coords (0..4096) per tile, runs point-in-polygon for `Polygon` features and a tolerance-distance for `Linestring`/`Point`, returns the top-N features above z-index. Standard `geo` algorithms; manageable code or a small `geo` dep.

#### 1.5.6 CallbackInfo accessors

Added in `layout/src/callbacks.rs`:
```rust
pub fn get_map_state(&self, node_id: DomNodeId) -> Option<MapState>
pub fn get_map_viewport(&self, node_id: DomNodeId) -> Option<MapViewport>
pub fn set_map_viewport(&mut self, node_id: DomNodeId, vp: MapViewport)
pub fn pan_map(&mut self, node_id: DomNodeId, dx_px: f32, dy_px: f32)
pub fn zoom_map(&mut self, node_id: DomNodeId, delta: f32, anchor_px: LogicalPosition)
pub fn get_features_at_lnglat(&self, node_id: DomNodeId, lng: f64, lat: f64) -> Vec<DecodedFeature>
pub fn get_map_click_info(&self) -> Option<MapClickInfo>   // valid inside On::MapClick
pub fn get_loaded_tile_count(&self, node_id: DomNodeId) -> usize
```

#### 1.5.7 Platform considerations

All five platforms use the same `ureq` + rustls HTTP stack. Per-platform deltas:

- **Tiles are HTTPS** → no `NSAppTransportSecurity` plist entry needed on iOS/macOS.
- **Android:** declare `<uses-permission android:name="android.permission.INTERNET" />` in `AndroidManifest.xml` (standard for any networked app).
- **No file-system or photos permissions** anywhere — tile bytes are public.
- **Cache root:** `dirs::cache_dir()` (already a layout dep, `layout/Cargo.toml:95`) returns the platform-correct path: `~/Library/Caches/<bundle>/azul-map-tiles/` on Apple, `/data/data/<pkg>/cache/azul-map-tiles/` on Android (thread `Context.getCacheDir()` through JNI to override), `$XDG_CACHE_HOME/azul-map-tiles/` on Linux, `%LOCALAPPDATA%\<app>\Cache\azul-map-tiles\` on Windows.

#### 1.5.8 Web/W3C equivalent

A `<canvas>` element + MapLibre GL JS (FOSS fork of mapbox-gl-js, BSD-3) is the web mapping standard. https://maplibre.org/maplibre-gl-js/docs/

When the WASM backend lands:
- `NodeType::MapTile(...)` maps to `<div>` hosting a MapLibre GL JS map instance.
- `MapViewport` ↔ `Map.setCenter() / setZoom() / setBearing() / setPitch()`.
- `On::MapClick` ↔ MapLibre's `map.on('click', evt => evt.features)`.
- `On::MapMove` ↔ `map.on('moveend', …)`.

Architectural shape stays identical; the WASM impl swaps the Rust `mvt` decoder + agg-rust path for MapLibre's WebGL rendering and treats `MapTileManager` as a passthrough to the JS instance.

### 1.6 Risks (MVT side)

1. **Tile loading is async.** Display-list builder needs a "placeholder" emission for tiles still in flight. v1: emit a grey-grid placeholder via `DisplayListItem::Rect` per missing tile, schedule a repaint when the fetch completes. The existing `VirtualView` placeholder pattern (`display_list.rs:715`) is the precedent — same "emit a marker, replace it post-hoc" shape.
2. **Memory pressure at deep zoom-out.** At z=0 a single tile covers the world (small). Bigger concern is z=14 where a full screen at 1080p × DPR 2 needs ~64 tiles, each potentially 50 KB encoded + 5-20× decoded. LRU bound at 256 tiles ≈ ~40 MB peak; tune per-platform (mobile lower).
3. **Label conflict resolution.** Real maps deduplicate labels (no overlapping POI names). Postponed since symbol layers are v2.
4. **Antimeridian + datelines.** `mvt` returns raw tile coords; longitude-wrap math is on us (when panning past ±180° tiles must wrap). Standard issue; a 30-line fix per the slippy-map docs https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames.
5. **Vector polygons with holes.** MVT encodes outer rings (CCW) + inner rings (CW). agg-rust's even-odd fill rule already handles this if rings are concatenated; if not, switch to non-zero with explicit winding correction. **TODO: verify the agg-rust path API matches MVT winding semantics.**
6. **Tile re-projection.** OpenMapTiles ships pre-projected to Web Mercator (EPSG:3857). If we ever support non-Mercator projections (globe / Albers), reprojection becomes very expensive on the client. Deferred.
7. **MapLibre style spec expression depth.** The full `interpolate` / `step` / `case` / `let` / `var` machinery is ~80 ops. v1 subset will hit limitations on community styles. Plan: gate "unsupported expression" warnings into the existing `LayoutDebugMessage` channel rather than failing layout.
8. **Glyph PBF / sprite loader missing.** Labels and icons (symbol layers) require both. Skipping v1 means OpenFreeMap maps render as "pretty geography, no place names." Probably fine for the first demo; user-facing roadmap should flag this.
9. **Date-pinned URL versioning.** The TileJSON tile URL embeds a date (`20260513_001001_pt`). If the user hardcodes a `MapTileSource::tile_url_template`, that pin can go stale on the server when OpenFreeMap rotates the planet. Recommendation: **always** resolve via TileJSON first (cheap, cached), pull `tiles[0]` out of the response; only fall back to a static template if the TileJSON fetch fails.
10. **Self-hosting story is mature.** OpenFreeMap docs (https://github.com/hyperknot/openfreemap) explicitly support a "self-host the planet" workflow. Users can substitute their own URL; our `MapTileSource::tile_url_template` is the only knob.

### 1.7 MVT integration sketch — surface summary

| Artifact | Location | Status |
|---|---|---|
| New `NodeType::MapTile(MapTileSource)` | `core/src/dom.rs` next to `Image`, `VirtualView` | new |
| `MapTileSource`, `MapViewport`, `MapStyle`, `MapClickInfo` | `core/src/dom.rs` (or `core/src/map.rs` module) | new |
| New `DisplayListItem::MapTileBlock { … }` | `layout/src/solver3/display_list.rs:584` | new variant |
| `MapTileManager` | `layout/src/managers/map.rs` | new file |
| MapLibre style parser | `layout/src/map/style.rs`, `mod map;` from `lib.rs` | new module |
| MVT decoder dep | `mvt = "0.13"` in `layout/Cargo.toml`, behind a new `map` feature | new feature flag |
| `On::MapClick`, `On::MapMove`, `On::MapTilesLoaded` | `core/src/dom.rs` `pub enum On` | new variants |
| New `HoverEventFilter::MapClick` (etc.) | `core/src/events.rs:1512` | new variants |
| CPU render | `layout/src/cpurender.rs` after `Image` handling | new arm |
| GPU render (offscreen→Image) | `dll/src/desktop/compositor2.rs` translator | new arm |
| `get_map_*` accessors | `layout/src/callbacks.rs` | new methods |
| Tile HTTP fetch + cache | `MapTileManager` uses existing `ureq` (`layout/Cargo.toml:83`) | reuse |
| Attribution surface | renderer always emits `attribution` string at bottom-right of MapWidget bounds | new bit of `MapTileBlock` rendering |
| Sample test | `scripts/mobile/golden/map_widget.png` via `mobile-snapshot.sh` | new test |
| api.json + codegen | `azul-doc autofix add MapTileSource.*` + `codegen all` | required |

**Implementation estimate:** ~3 sprint-days (decoder + Z/X/Y math + Liberty paint subset rendering correctly for a static viewport), +1–2 days for pan/zoom interaction, +∞ for symbol layers (deferred).

---

## PART 2 — PDF via printpdf (both directions)

### 2.1 The printpdf crate (verified)

- **Crate:** `printpdf` — https://docs.rs/printpdf — current latest **0.9.1** on docs.rs (Felix Schütt's own crate; **TODO: verify** if a newer master branch should be pinned instead of crates.io 0.9.1 — README mentions ops/variants not present in 0.9.1 docs, e.g. `WriteCodepoints` / `WriteCodepointsWithKerning`).
- **License:** MIT.
- **Direction:** **emission only on the canonical 0.9.1 API** — `printpdf` *generates* PDFs. It does **not** rasterise an existing PDF to pixels. It has a `render_to_svg` / `render_to_svg_async` for converting *parsed* PDFs into SVG (which Azul could then re-rasterise via resvg), but no built-in raster path.

#### 2.1.1 The Op enum (printpdf 0.9.1, verified from `github.com/fschutt/printpdf/src/ops.rs`)

51 variants covering all canonical PDF page operators. Key variants we'll use:

- **Drawing:** `DrawLine { line: Line }`, `DrawRectangle { rectangle: Rect }`, `DrawPolygon { polygon: Polygon }`, `UseXobject { id: XObjectId, transform: XObjectTransform }` (for images/forms).
- **Text:** `StartTextSection`/`EndTextSection` (`BT`/`ET`), `SetFont { font_id, size }` (`Tf`), `SetTextCursor { x, y }` (bottom-left origin), `SetTextMatrix`, `ShowText { items: Vec<TextItem> }` (`Tj`/`TJ`), `SetCharacterSpacing`, `SetWordSpacing`, `SetLineHeight`, `SetLineOffset` (superscript), `MoveToNextLineShowText` (`'`), `SetSpacingMoveAndShowText` (`"`), `MoveTextCursorAndSetLeading` (`TD`), `SetHorizontalScaling` (`Tz`).
- **Colour / state:** `SetFillColor`, `SetOutlineColor`, `SetColorSpaceStroke`/`Fill`, `SetOutlineThickness`, `SetLineDashPattern`, `SetLineJoinStyle`, `SetLineCapStyle`, `SetMiterLimit`, `SetTextRenderingMode`, `SetRenderingIntent`, `SetTransformationMatrix`.
- **Graphics state stack:** `SaveGraphicsState`, `RestoreGraphicsState`, `LoadGraphicsState`.
- **Structure:** `BeginLayer`/`EndLayer`, `BeginMarkedContent*`/`EndMarkedContent*`, `BeginOptionalContent`/`EndOptionalContent`, `BeginCompatibilitySection`/`EndCompatibilitySection`, `BeginInlineImage`/`Data`/`EndInlineImage`, `DefineMarkedContentPoint`.
- **Annotations:** `LinkAnnotation`.
- **Misc:** `Marker { id }`, `Unknown { key, value }`.

#### 2.1.2 Text API shape

`Op::ShowText { items: Vec<TextItem> }` is the standard text-rendering verb. `TextItem` is an enum (`printpdf::text::TextItem`) with two variants:
- Text segment (decoded as a UTF-8 String) — for ASCII / Latin-1 / Unicode characters; printpdf maps to glyphs via the font's cmap.
- Spacing adjustment — number-of-thousandths-of-em adjustment between glyphs (the standard PDF `TJ` array shape for kerning).

The `printpdf::text` module also exposes `decode_tj_string_as_glyph_ids`, `decode_tj_operands_as_glyph_ids` — i.e. **round-trip glyph-ID support is present on the reader side**, and `Codepoint { glyph_id, codepoint }` is a public struct. **The writer side accepts text by character string** as documented. **TODO: verify** whether the project's master branch has `Op::WriteCodepoints { codepoints: Vec<Codepoint> }` (the README mentions it; docs.rs 0.9.1 doesn't expose it). If yes, that's the ideal entry for our text-export path (we already have positioned glyph IDs in `DisplayListItem::Text { glyphs: Vec<GlyphInstance> }`). If not, we have two options:

- **Path A (clean):** add a `WriteCodepoints` op to printpdf master and bump the dep. Felix maintains both; this is a 1-2 day printpdf-side change (build the encoded byte string from glyph IDs + emit as a `Tj` with the right encoding).
- **Path B (workaround):** for each `GlyphInstance`, reverse-look up the glyph→unicode via the font's cmap (allsorts exposes this) and emit `ShowText` with the unicode string. This works for any font where the cmap is monotonic; breaks for ligatures (the glyph index points to a glyph that has no single unicode codepoint) and for emoji / complex scripts.

**Recommendation: Path A.** Felix owns printpdf, the change is mechanically simple, and Path B has correctness bugs that bite on day-1 if a user types an `fi` ligature.

#### 2.1.3 Font loading

`ParsedFont::from_bytes(&ttf_bytes, font_index)` is the embedding entry point. printpdf parses TTF/OTF, extracts the subset actually used, embeds the subset in the PDF. Allsorts (Azul's shaper, `layout/Cargo.toml:50`) already parses TTF/OTF; we either (a) pass the same TTF bytes to printpdf (cheap; printpdf re-parses) or (b) refactor printpdf to accept an allsorts `Font<'_>` (cleaner, future work).

For **embedded fonts in PDF**, printpdf assigns a `FontId` (`PdfDocument::add_font(parsed_font)`). Subsequent `Op::SetFont { font_id, size }` selects it for following text.

#### 2.1.4 Image embedding

`RawImage::decode_from_bytes(&bytes)` decodes any image format printpdf supports (PNG / JPEG / TIFF / BMP). `PdfDocument::add_image(raw_image) -> XObjectId`. Embed via `Op::UseXobject { id, transform }` where `XObjectTransform` carries position + scale + rotation.

For **PDF inline rendering** (Direction A), we'd want the reverse: load a PDF file → render page → embed as image. printpdf doesn't do this; see §2.3.

#### 2.1.5 Graphics primitives

`Op::DrawLine { line: Line }`, `Op::DrawRectangle { rectangle: Rect }`, `Op::DrawPolygon { polygon: Polygon }`. Polygon supports holes (multiple rings with even-odd or non-zero fill rule). `Op::SetFillColor` / `SetOutlineColor` accept `printpdf::Color::Rgb(Rgb { r, g, b })` / Cmyk / Greyscale.

**Gradients**: not explicitly in the Op list. Workaround: render the gradient to a raster image (PNG/JPEG) → embed via `UseXobject`. Real PDF gradients use Shading patterns (PDF spec §8.7); printpdf does not currently expose them. **TODO: verify** with the printpdf changelog.

### 2.2 Direction A — *render PDF inline* as `NodeType::Pdf(PdfRef)`

#### 2.2.1 The rasterisation problem

Since printpdf cannot rasterise, we need a backing crate that can. Survey:

| Crate | Engine | License | Cross-platform | Verdict |
|---|---|---|---|---|
| [`pdfium-render`](https://docs.rs/pdfium-render) | Google Chromium's PDFium (C++) | Apache-2.0 (binding); PDFium itself is Apache-2.0 + BSD-3 | iOS ✓, Android ✓, macOS ✓, Linux ✓, Windows ✓, WASM ✓ | **Recommended.** Production-grade rasteriser. Runtime requires a `libpdfium.{dylib,so,dll}` next to the binary or statically linked. https://crates.io/crates/pdfium-render |
| [`mupdf-rs`](https://crates.io/crates/mupdf) | Artifex MuPDF (C) | AGPL-3.0 / commercial | desktop ✓, iOS ✓ (limited), Android ✓ | AGPL contagion is a deal-breaker for closed-source mobile apps shipped through stores. Skip. |
| [`pdf`](https://crates.io/crates/pdf) (pure-Rust) | own renderer | MIT/Apache-2.0 | ✓ all | Parses PDFs but rendering is incomplete; many real-world PDFs render blank or with corrupted text. **TODO: verify** the rasterisation quality in 2026; last assessment was ~2024. |
| [`lopdf`](https://crates.io/crates/lopdf) | parser only | MIT | ✓ | Doesn't render. Used internally by printpdf for low-level PDF object writes. |
| Custom (Pure-Rust PDF rasteriser) | — | — | — | A multi-month engineering project. Skip. |

**Recommendation:** **pdfium-render with a statically-linked libpdfium** for desktop + Android, and **pdfium-render with the system PDF service** on iOS (iOS has CGPDFDocument in Quartz; alternatively bundle libpdfium for parity).

On iOS specifically, the *cheaper* option is Quartz: `CGPDFDocument` (`CoreGraphics.framework`) + `CGContext` rendering. Wrap in Rust via `objc2-core-graphics` (Azul already pulls `core-foundation` and `core-graphics` for font rendering at `layout/Cargo.toml:118-119`). Same on macOS. This saves the ~10 MB libpdfium binary on Apple platforms; whether worth the extra ifdef is a future decision. v1 ships with `pdfium-render` everywhere.

#### 2.2.2 NodeType + types

```rust
// core/src/dom.rs additions
#[repr(C, u8)]
pub enum NodeType {
    // ... existing ...
    /// Embedded PDF document. Renders the chosen page rasterised to the node's bounds.
    Pdf(BoxOrStatic<PdfRef>),
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct PdfRef {
    pub source: PdfSource,
    pub page: u32,                  // 0-indexed; clamp to page count
    pub dpi: f32,                   // rasterisation DPI; default 144 (2× screen)
    pub fit: PdfFit,                // FitToBounds | OriginalSize
    /// Optional pre-rasterised image. None on first frame; populated by PdfManager.
    pub rendered: Option<ImageRef>,
}

#[repr(C, u8)]
pub enum PdfSource {
    File(AzString),                  // path on disk
    Bytes(U8Vec),                    // for in-memory PDFs
    Url(AzString),                   // fetched via ureq (lazy)
}

#[repr(C)]
pub enum PdfFit { FitToBounds, OriginalSize }
```

`PdfRef` is small (a few words + the optional `ImageRef`). `BoxOrStatic<PdfRef>` mirrors `BoxOrStatic<ImageRef>` of `NodeType::Image`.

#### 2.2.3 Manager

`layout/src/managers/pdf.rs` (new):

```rust
pub struct PdfManager {
    pdfium: pdfium_render::Pdfium,                 // pdfium binding handle
    parsed_documents: BTreeMap<PdfSourceKey, Arc<ParsedPdf>>,
    rasterised_pages: lru::LruCache<RenderKey, ImageRef>,
    inflight: BTreeMap<RenderKey, FetchOrRenderState>,
    /// HTTP for PdfSource::Url; reuses the same ureq agent as the map manager
    http: ureq::Agent,
}

struct PdfSourceKey { source: PdfSource }   // hashable
struct RenderKey { source_hash: u64, page: u32, dpi_quantised: u32 }
```

`PdfManager::rasterise_page(key) -> Result<ImageRef>` uses pdfium-render's `PdfRenderConfig::new().set_target_width(…)` and stores the resulting BGRA buffer as a `RawImage` wrapped in `ImageRef::new_rawimage`. From that point on, the node renders just like any other `Image` — both CPU (`cpurender.rs::render_image`) and WebRender (existing `ImageDisplayItem` path) work unchanged.

#### 2.2.4 Display-list emission

**Zero new variants required.** During display-list generation, `NodeType::Pdf(pdf_ref)` is translated into `DisplayListItem::Image { bounds, image: pdf_ref.rendered.clone()_or_placeholder, border_radius }`. If `rendered` is `None`, emit a grey placeholder Rect (same pattern as in-flight map tiles) and schedule a render via `PdfManager`.

#### 2.2.5 Events

`On::PdfPageLoaded` (one variant; not strictly necessary but useful for "show spinner then PDF" UX).

No custom hit-test surface in v1 — clicks on a PDF report which page was clicked but not which text/annotation. (Direction A2: PDF text selection / annotation hit-test is a separate future sprint.)

### 2.3 Direction B — *export the current display list to PDF*

This is the more interesting direction and the deeper integration. The key fact: **the display list already carries everything printpdf needs.**

#### 2.3.1 The `DisplayListItem::TextLayout` precedent (already present)

`layout/src/solver3/display_list.rs:631`:
```rust
TextLayout {
    layout: Arc<dyn std::any::Any + Send + Sync>,    // type-erased UnifiedLayout
    bounds: WindowLogicalRect,
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
},
```

Producer at `display_list.rs:1605` pushes this *before* the per-line `DisplayListItem::Text` items, so a PDF exporter has access to the full `text3::UnifiedLayout` (text3/cache.rs:3985) containing bidirectional analysis, shaped clusters, baselines, all positioned glyphs. `cpurender.rs:2718` deliberately skips `TextLayout` because pixel rendering doesn't need it; the comment is `"TextLayout is metadata for PDF/accessibility - skip in CPU rendering"`.

**The hard work is already done.** The PDF exporter downcasts `Arc<dyn Any>` → `Arc<UnifiedLayout>` and walks `unified.items: Vec<PositionedItem>` to emit text with high-fidelity glyph positions.

#### 2.3.2 The mapping table

| DisplayListItem variant | printpdf `Op` sequence |
|---|---|
| `Rect { bounds, color, border_radius }` | `SetFillColor(color); DrawRectangle(bounds)` (rounded → `DrawPolygon` with bezier-approximated arcs) |
| `LinearGradient`/`RadialGradient`/`ConicGradient` | Rasterise to PNG, `UseXobject` (real PDF Shading is post-v1) |
| `Border { widths, colors, styles, … }` | Per-side `SetOutlineThickness` + `DrawLine`, or `DrawPolygon` for rounded |
| `Text { glyphs, font_hash, font_size, color }` | `SetFont; SetFillColor; SetTextCursor; ShowText` with `TextItem::Text`, *or* with `WriteCodepoints` (positioned glyph IDs) if printpdf master ships that op — preferred, preserves shaping |
| `TextLayout { layout, bounds, font, color }` | Downcast `Arc<Any>` → `Arc<UnifiedLayout>`; walk items; per cluster emit `StartTextSection; SetFont; SetTextMatrix(pos); ShowText(items); EndTextSection` |
| `Underline` / `Strikethrough` / `Overline` | `SetOutlineColor + DrawLine` |
| `Image { bounds, image, … }` | `add_image(raw_image) → XObjectId` once; `UseXobject { id, transform }` |
| `BoxShadow` | Skip in v1 (no native shadow; would need bitmap embed) |
| `PushClip` / `PopClip` | `SaveGraphicsState; <clip path>; W; content; RestoreGraphicsState` |
| `PushStackingContext` / `PopStackingContext` | Ignore (display-list order preserves z) |
| `PushReferenceFrame { transform }` / `PopReferenceFrame` | `SetTransformationMatrix(t); content; SetTransformationMatrix(inverse)` |
| `PushScrollFrame` / `PopScrollFrame` | Ignore — PDF has no scrolling; we crop via existing `PushClip` |
| `HitTestArea` | No PDF equivalent (future: emit `LinkAnnotation` for hyperlink hit areas) |
| `PushOpacity` / `PopOpacity` | Ignore in v1; full impl needs SMask (post-v1) |
| `PushFilter` / `PushBackdropFilter` | No PDF equivalent; rasterise affected region (post-v1) |
| `VirtualView` / `VirtualViewPlaceholder` | Recurse into child DOM's display list inline |
| `ScrollBar` / `ScrollBarStyled` | Skip — UI chrome, not document content |
| `PushTextShadow` / `PopTextShadow` | Skip in v1 |

#### 2.3.3 Export API

`layout/src/extra.rs` (or a new `layout/src/pdf_export.rs`):

```rust
pub struct PdfExportOptions {
    pub page_size: PageSize,             // A4, Letter, Custom { w_mm, h_mm }
    pub orientation: PageOrientation,    // Portrait | Landscape
    pub margins_mm: PdfMargins,
    pub title: AzString,
    pub author: AzString,
    pub subject: AzString,
    pub keywords: AzString,
    pub embed_metadata: bool,
    /// If the laid-out content overflows one page, split into multiple pages.
    pub multi_page: PdfMultiPage,
}

pub enum PdfMultiPage {
    SinglePageStretch,                   // resize to fit one page
    SinglePageCrop,                      // single page, crop overflow
    MultiPage,                           // split by Y; uses fragmentation engine (layout/src/fragmentation.rs)
}

impl App {
    pub fn export_pdf(&self, path: &Path, options: PdfExportOptions) -> Result<(), PdfExportError>;
    pub fn export_pdf_to_bytes(&self, options: PdfExportOptions) -> Result<Vec<u8>, PdfExportError>;
}
```

The implementation lives in `layout/src/pdf_export.rs` and walks the *most recent* `DisplayList` (kept by `LayoutWindow`) emitting printpdf `Op`s into a `Vec<Op>`, packaging via `PdfPage::new(width, height, vec_of_ops)` and `PdfDocument::with_pages(vec_of_pages).save(...)`.

Multi-page is the interesting case: the existing `layout/src/fragmentation.rs` already implements CSS `break-inside`/`page-break-after` semantics for printing. We reuse it: configure the layout pass with `page_size_mm`, the fragmentation engine splits the layout tree into pages, each page produces its own DisplayList → its own `PdfPage`.

#### 2.3.4 Font embedding

For each unique `FontHash` referenced in the display list:
1. Look up the font bytes via `FontManager::get_font_bytes(font_hash)` — already exposed for the SVG rendering path.
2. Call `printpdf::ParsedFont::from_bytes(&bytes, 0)` → `add_font(parsed_font)` → `FontId`.
3. Cache the `FontId` keyed by `FontHash` for this export run.

printpdf handles the TTF subsetting automatically (only embeds glyphs actually used). For PDF/A-2 compliance (TODO: verify printpdf has a PDF/A export flag), full embedding is required.

#### 2.3.5 Image embedding

For each unique `ImageRef::get_hash()` in the display list:
1. Look up via `RendererResources::get_image_data(hash)`.
2. Convert to `printpdf::RawImage` (sniff the format; if already PNG-encoded keep, else re-encode).
3. `PdfDocument::add_image(raw_image) -> XObjectId`. Cache.

For animated GIFs / video frames — skip; export the *current* frame only.

#### 2.3.6 Where the save dialog goes (mobile)

- **Desktop:** `tfd::save_file_dialog("Export PDF", "document.pdf", "PDF")` (already a layout dep, `layout/Cargo.toml:98`).
- **iOS:** present `UIActivityViewController` (the share sheet) with the PDF data — the most flexible UX. Alternative: `UIDocumentPickerViewController(.exportToService)` for a save-as flow. Both via the file-picker work tracked in `scripts/research/04_system_integration.md`.
- **Android:** `Intent.ACTION_CREATE_DOCUMENT` with `setType("application/pdf")` + `EXTRA_TITLE`. Returns a scoped `content://` URI; no `WRITE_EXTERNAL_STORAGE` needed on API 29+. Same SAF route as 04.

All three route into the planned `FileDialog::save_file_dialog` API; PDF export is one of its first users.

### 2.4 PDF integration sketch — surface summary

| Artifact | Location | Status |
|---|---|---|
| New `NodeType::Pdf(BoxOrStatic<PdfRef>)` | `core/src/dom.rs` | new |
| `PdfRef`, `PdfSource`, `PdfFit` | `core/src/dom.rs` (or new `core/src/pdf.rs`) | new |
| `PdfManager` | `layout/src/managers/pdf.rs` | new file |
| `pdfium-render` dep (Direction A) | `layout/Cargo.toml`, behind `pdf_render` feature | new |
| `printpdf` dep (Direction B) | `layout/Cargo.toml`, behind `pdf_export` feature | new |
| `App::export_pdf` | `dll/src/desktop/app.rs` (and `layout/src/pdf_export.rs`) | new |
| `PdfExportOptions`, `PdfMultiPage`, `PdfMargins` | `layout/src/pdf_export.rs` | new |
| `On::PdfPageLoaded` | `core/src/dom.rs` | new (small surface) |
| New `DisplayListItem::*` variants for PDF | **none** — reuse existing `Image`, `Rect`, `TextLayout` | reuse |
| Recursion into `VirtualView` children | already supported by display-list walker | reuse |
| Fragmentation for multi-page | `layout/src/fragmentation.rs` already exists | reuse |
| CallbackInfo accessor | `callback_info.export_pdf(path, options)` | new |
| Mobile save dialog | `FileDialog::save_file_dialog(default_name, filter)` from research/04 | depends-on |
| Sample test | `scripts/mobile/golden/pdf_export.pdf` byte-stable hash check | new |
| api.json + codegen | `azul-doc autofix add PdfRef.*` + `codegen all` | required |

**Implementation estimate:**
- Direction A (PDF render inline) — 2-3 days. The hard work is the platform-specific libpdfium binary distribution; the rendering code is ~200 LOC.
- Direction B (export display list) — 5-7 days. The dispatch table in §2.3.2 is ~600 LOC; font/image embedding caches add ~200 LOC; multi-page integration with the fragmentation engine adds ~300 LOC. The win is high — every Azul app gets `App::export_pdf` for free.

### 2.5 W3C equivalent (Web/WASM target)

- **Direction A (render PDF inline):** `<embed type="application/pdf" src="…">` or `<object>` element. The browser's built-in PDF viewer (PDF.js on Firefox, native on Chrome/Safari) handles rendering. https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-embed-element
  - Alternative: PDF.js as a JS dependency, render to `<canvas>`.
- **Direction B (export display list as PDF):** Browser-side, generate the PDF in Rust→WASM via the same `printpdf` crate (printpdf compiles to WASM; **TODO: verify** that the `printpdf` dependency tree builds clean for `wasm32-unknown-unknown` — lopdf may need a feature flag), call `.save_to_bytes()`, then trigger a browser download:
  ```js
  const blob = new Blob([pdfBytes], {type: 'application/pdf'});
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url; a.download = 'document.pdf'; a.click();
  URL.revokeObjectURL(url);
  ```
  The same `App::export_pdf` API maps to this; the platform layer differs only in the final byte-disposition step.

### 2.6 Risks (PDF side)

1. **libpdfium binary distribution.** PDFium isn't on every platform's stock SDK. We must either statically link it (Android: doable with NDK; iOS: requires xcframework build), bundle a dylib, or fall back to system PDF support (CGPDFDocument on Apple; nothing on Linux/Windows). v1 ships `pdfium-render` everywhere; document the libpdfium acquisition step in `scripts/check-prereqs-mobile.sh`.
2. **License audit on libpdfium.** PDFium is Apache-2.0 + BSD-3 (Chromium-friendly). Mobile-app-store-compatible. **No GPL/AGPL contagion** — that was the reason we rejected mupdf.
3. **Glyph-ID round-trip on text export.** printpdf 0.9.1's `Op::ShowText` accepts strings, not glyph IDs. If we rely on the cmap-reverse path (Path B in §2.1.2), we lose ligature correctness. Fix: PR a `WriteCodepoints` op to printpdf master. Felix owns both crates.
4. **Multi-page fragmentation correctness.** `layout/src/fragmentation.rs` is exercised today by the print stylesheet path; PDF export will be its first heavy user. Expect bug discovery on real-world layouts (tables that break across pages, sticky headers, page-break-inside: avoid on flex containers).
5. **Gradient fidelity.** Rasterising gradients to PNG-then-embed is fine for static fills but bloats the PDF and limits zoom resolution. Eventual fix: emit PDF Shading patterns (PDF spec §8.7); printpdf has no API for that today.
6. **Animations / transitions.** A PDF can't represent live state. Export captures the *current frame*; document this in the API.
7. **Opacity (`PushOpacity`).** PDF has soft masks (SMask) for transparency groups, but printpdf doesn't expose them. v1 drops opacity (renders fully opaque); v2 adds SMask support.
8. **Filters (`PushFilter` / `PushBackdropFilter`).** No PDF equivalent (PDF has Transparency Groups + Blend Modes but the CSS `filter` family is much richer). Rasterise affected region as a PNG → embed. Post-v1.
9. **iOS sandbox path.** `Documents/` is the only user-visible location without a `UIDocumentPickerViewController`. Default to `UIActivityViewController` for the best UX. Tie into research/04 file picker work.
10. **Android scoped storage.** API ≥29 forces `MediaStore` or `Intent.ACTION_CREATE_DOCUMENT`. Don't write to `/sdcard/Download/` directly — that's blocked on Android 11+.
11. **PDF/A compliance** (long-term archival format). printpdf may not generate PDF/A-conforming output by default (need to set `/OutputIntent`, embed colour profiles, prohibit transparency, …). Out of scope for v1; flag as future work.
12. **TODO: verify** printpdf's current state on PDF encryption (password-protected PDFs); we likely skip in v1.

---

## Cross-cutting integration notes

### A. Permissions surface (unifies with research/04)

- **MVT:** internet only (already universal).
- **PDF export:** filesystem write — share intent or save-as picker, both shared with research/04.

### B. Feature flags in `layout/Cargo.toml`

```toml
[features]
# existing ...
map = ["dep:mvt", "dep:serde_json", "http"]                # MVT decode + style parse + tile fetch
pdf_export = ["dep:printpdf"]                              # display-list → PDF
pdf_render = ["dep:pdfium-render"]                         # PDF → ImageRef
```

`http` is already a feature flag (`layout/Cargo.toml:247`). `map` reuses it.

### C. Codegen impact

Every public type added (`MapTileSource`, `MapViewport`, `PdfRef`, `PdfExportOptions`, etc.) flows through `azul-doc autofix add <Type>.<method>` + `codegen all` so all 35 binding languages get the API for free. Per the §0 architecture seams, no special handling needed.

### D. WebRender custom display item — final word

The WebRender display-item enum is closed; there is no extension point for third-party draw kinds. Both features in this brief avoid the problem by **going through the existing `Image` path** (MVT via offscreen agg-rust → ImageRef; PDF inline via pdfium-render → ImageRef). The display-list emission side adds new `DisplayListItem` variants in `layout/src/solver3/display_list.rs`, but the `compositor2.rs` translator that hands off to WebRender flattens those into existing WR primitives. Same pattern SVG uses today.

If we ever need vector-perfect maps on the GPU (no offscreen rasterisation), the right path is to **fork the WebRender vendor code** and add an `MvtTile` display item with custom shaders. That's a future sprint; v1 doesn't need it.

### E. Test surface

- `scripts/mobile/golden/map_widget.png` — Liberty style on tile (51, 33) at z=8 (Berlin); deterministic given the date-pinned URL. Run weekly to catch tile-server schema drift.
- `scripts/mobile/golden/pdf_export.pdf` — hash-only check on the byte output of `export_pdf(some_dom, A4)`. printpdf's output should be byte-deterministic given a fixed creation timestamp and embedded font subset.
- `scripts/mobile/golden/pdf_render.png` — load a checked-in 3-page PDF, render page 1 at 144 DPI, compare to golden.

### F. Acceptance gates (sketch for the impl session)

1. `cargo build -p azul-layout --features map` succeeds.
2. `cargo build -p azul-layout --features pdf_export` succeeds.
3. `cargo build -p azul-layout --features pdf_render` succeeds with libpdfium on `LIBPDFIUM_PATH`.
4. `cargo test -p azul-layout --features "map,pdf_export,pdf_render"` passes.
5. Headless render of a 800×600 MapWidget centred on Berlin (lng 13.40, lat 52.52, z=12) → tile fetch succeeds → polygons + lines render → golden PNG matches.
6. Headless render of `hello-world.c`'s DOM → `App::export_pdf("hello.pdf", default_options)` produces a non-empty valid PDF; opening it in any reader shows the same layout.
7. Headless render of a 1-page PDF embedded as `NodeType::Pdf` → page 1 visible inside the bounds.

---

## References (URLs cited above, consolidated)

- MVT spec 2.1 — https://github.com/mapbox/vector-tile-spec/tree/master/2.1
- MVT proto file — https://github.com/mapbox/vector-tile-spec/blob/master/2.1/vector_tile.proto
- OpenFreeMap home — https://openfreemap.org/
- OpenFreeMap GitHub — https://github.com/hyperknot/openfreemap
- OpenFreeMap planet TileJSON — https://tiles.openfreemap.org/planet  (live verified 2026-05-19)
- OpenFreeMap Liberty style — https://tiles.openfreemap.org/styles/liberty  (live verified 2026-05-19)
- OpenStreetMap copyright — https://www.openstreetmap.org/copyright
- MapLibre style spec — https://maplibre.org/maplibre-style-spec/
- MapLibre vector sources — https://maplibre.org/maplibre-style-spec/sources/
- MapLibre expressions — https://maplibre.org/maplibre-style-spec/expressions/
- MapLibre GL JS — https://maplibre.org/maplibre-gl-js/docs/
- mvt crate — https://docs.rs/mvt/latest/mvt/
- mvt-reader crate — https://docs.rs/mvt-reader/latest/mvt_reader/
- Slippy-map tile naming — https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames
- printpdf 0.9.1 docs — https://docs.rs/printpdf/0.9.1/printpdf/
- printpdf ops.rs source — https://github.com/fschutt/printpdf/blob/master/src/ops.rs
- printpdf text module — https://docs.rs/printpdf/0.9.1/printpdf/text/index.html
- pdfium-render — https://crates.io/crates/pdfium-render, https://docs.rs/pdfium-render
- mupdf-rs — https://crates.io/crates/mupdf (rejected: AGPL)
- PDFium upstream — https://pdfium.googlesource.com/pdfium/
- W3C `<embed>` — https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-embed-element

---

## Open questions for the implementation session

1. **Pin printpdf to 0.9.1 (crates.io) or to a master commit?** Master may carry `WriteCodepoints` for glyph-precise text export.
2. **pdfium binary distribution policy.** Ship in repo? Fetch on first build? Document via `scripts/check-prereqs-mobile.sh`.
3. **Multi-page PDF export v1 — reuse `fragmentation.rs` or single-canvas-stretch?** Likely the latter for v1.
4. **`App::export_pdf` sync or async?** Lean sync v1; an image-heavy export can take seconds.
5. **`On::MapClick` hit-test — extend `core/hit_test.rs` or keep inside `MapTileManager`?** Latter for v1 (hit-test infra is over a static DisplayList; map hit-test is over feature attributes).
