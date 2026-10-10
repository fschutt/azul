//! GeoJSON `Feature` → SVG string conversion for `MapWidget` tiles.
//!
//! Step 4 of the MVT pipeline (per `MOBILE_SESSION_LOG.md`):
//!
//! ```text
//! 1. fetch     → bytes      (HTTP)
//! 2. decode    → Vec<Feature>  (td::parse_mvt_tile — landed in P3.2e)
//! 3. style     → SVG attrs  (MapCSS — future)
//! 4. emit SVG  → String     (THIS MODULE)
//! 5. svg→DOM   → child Dom  (framework's existing svg-to-dom path)
//! ```
//!
//! This module is the pure-data half — no I/O, no async. Given a tile
//! id and the `geojson::Feature`s the decoder returned for it, produce
//! a self-contained `<svg>` document sized to the tile's 256 × 256
//! pixel bounding box, with one SVG primitive per feature.
//!
//! WGS-84 → tile-local pixel projection is done inline via the Web
//! Mercator forward equations — no `proj4rs` call needed since both
//! source and target use the same Mercator family. Conversion fits in
//! ~10 lines and matches the formula `MapWidget::map_widget_render`
//! already uses for the tile-grid math.
//!
//! Styling is intentionally minimal in this tick: a small per-layer
//! lookup picks fill/stroke colours based on the GeoJSON property
//! `"layer"` (the MVT layer name — e.g. `"water"`, `"buildings"`,
//! `"roads"`). MapCSS-driven styling lands in the next tick.
//!
//! LABELS. A point feature with a name (a place, a POI, a lake, a peak, a
//! house number) and a named road / river line become `<text>` elements in
//! one block at the end of the document, `<g id="labels">…</g>` (the
//! widget's `TILE_LABELS_OPEN`). The tile rasteriser draws no text, so the
//! widget cuts that block out again (`azul_layout::widgets::map::
//! split_tile_svg`), rasterises the geometry and lays the labels out as real
//! text over the tiles - upright, at a constant size, decluttered across
//! tiles. Each label is emitted by the ONE tile its anchor lies in (MVT
//! tiles repeat their neighbours' features in a buffer), so a label is never
//! doubled at a tile seam.

#![cfg(feature = "map-tiles")]

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

use azul_layout::widgets::map::{MapTileId, TILE_LABELS_OPEN};

const TILE_PX: f64 = 256.0;

/// The most labels one tile carries (by priority): a street-level tile holds
/// hundreds of POIs and house numbers, and a frame shows a few hundred labels
/// in all.
const MAX_TILE_LABELS: usize = 160;

/// Resolved per-layer styling — owned so it can hold either a built-in
/// default or a MapCSS-parsed value.
#[derive(Clone)]
struct LayerStyle {
    fill: String,
    stroke: String,
    stroke_width: f32,
}

impl LayerStyle {
    fn make(fill: &str, stroke: &str, stroke_width: f32) -> Self {
        Self {
            fill: fill.to_string(),
            stroke: stroke.to_string(),
            stroke_width,
        }
    }
}

/// Built-in fallback palette, loose-matched against the standard
/// OpenMapTiles / OpenFreeMap layer names. Used for any layer the
/// user's MapCSS doesn't cover (or when no MapCSS was supplied).
fn default_style(layer_name: &str) -> LayerStyle {
    let lower = layer_name.to_ascii_lowercase();
    if lower.contains("water") {
        LayerStyle::make("#9ecae1", "#75a8c8", 0.5)
    } else if lower.contains("building") {
        LayerStyle::make("#e0d8c8", "#b8ad99", 0.3)
    } else if lower.contains("transportation_name") || lower.contains("highway") {
        LayerStyle::make("none", "#ffffff", 1.6)
    } else if lower.contains("transportation") || lower.contains("road") {
        LayerStyle::make("none", "#f0e8d8", 0.8)
    } else if lower.contains("park")
        || lower.contains("landcover")
        || lower.contains("landuse_grass")
    {
        LayerStyle::make("#c8e0c0", "#a8c89c", 0.4)
    } else if lower.contains("boundary") || lower.contains("admin") {
        LayerStyle::make("none", "#9a8aa0", 0.6)
    } else {
        LayerStyle::make("#d6d8db", "#a8acb1", 0.4)
    }
}

/// A rule's label paint: `text-color`, `text-halo-color`, `font-size`.
/// Each is optional, so `place.city { font-size: 15; }` changes the size
/// alone and keeps the colours of the `place` rule (or the built-in ones).
#[derive(Clone, Default)]
struct LabelRule {
    color: Option<String>,
    halo: Option<String>,
    size: Option<f32>,
}

/// A label's resolved paint.
#[derive(Clone, Debug, PartialEq)]
struct LabelStyle {
    color: String,
    halo: String,
    size: f32,
}

impl LabelStyle {
    fn apply(&mut self, rule: &LabelRule) {
        if let Some(color) = &rule.color {
            self.color.clone_from(color);
        }
        if let Some(halo) = &rule.halo {
            self.halo.clone_from(halo);
        }
        if let Some(size) = rule.size {
            self.size = size;
        }
    }
}

/// A parsed MapCSS stylesheet: trailing-selector-token → style.
///
/// MapCSS is its own CSS dialect (`way`, `area`, `node` selectors,
/// `fill-color` / `casing-width` properties) that doesn't map onto the
/// framework's CSS property enum — so this is a focused subset parser
/// rather than a reuse of `azul_css::Css::from_string`. It accepts
/// rules of the form `selector { fill: <color>; stroke: <color>;
/// stroke-width: <num>; }` (also accepting MapCSS-isms `fill-color`,
/// `color`, `width`). The selector's trailing whitespace/`.`-stripped
/// token is the lookup key, matched against the MVT layer name.
///
/// Labels: `text-color` (or `text-fill`), `text-halo-color`, `font-size`
/// (or `text-size`) in a rule style the labels of that layer / class. A
/// rule that declares ONLY label properties styles no geometry, so
/// `place { text-color: … }` cannot paint anything but the place names.
struct MapCss {
    rules: BTreeMap<String, LayerStyle>,
    /// The label rules, by the same keys as `rules`.
    labels: BTreeMap<String, LabelRule>,
    /// `canvas { fill: … }` — the tile's base (land) colour. Themes set it
    /// (a dark theme needs a dark base under the water holes); absent, the
    /// built-in light land colour.
    canvas_fill: Option<String>,
}

/// The built-in base (land) colour behind every tile.
const DEFAULT_CANVAS_FILL: &str = "#d6d8db";

impl MapCss {
    fn parse(src: &str) -> Self {
        let mut rules = BTreeMap::new();
        let mut labels = BTreeMap::new();
        let mut canvas_fill = None;
        // Split into `selector { body }` chunks on `}`.
        for block in src.split('}') {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            let Some(brace) = block.find('{') else {
                continue;
            };
            let selector_raw = block[..brace].trim();
            let body = &block[brace + 1..];
            // Selector key: last token, leading `.`/`#` stripped, lowered.
            let key = selector_raw
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches(['.', '#'])
                .to_ascii_lowercase();
            if key.is_empty() {
                continue;
            }

            let mut fill = "none".to_string();
            let mut stroke = "none".to_string();
            let mut stroke_width = 0.5_f32;
            let mut label = LabelRule::default();
            let mut has_geometry = false;
            let mut has_label = false;
            for decl in body.split(';') {
                let Some(colon) = decl.find(':') else {
                    continue;
                };
                let prop = decl[..colon].trim().to_ascii_lowercase();
                let val = decl[colon + 1..].trim();
                match prop.as_str() {
                    "fill" | "fill-color" => {
                        fill = val.to_string();
                        has_geometry = true;
                    }
                    "stroke" | "color" | "casing-color" => {
                        stroke = val.to_string();
                        has_geometry = true;
                    }
                    "stroke-width" | "width" | "casing-width" => {
                        if let Ok(w) = val.trim_end_matches("px").trim().parse::<f32>() {
                            stroke_width = w;
                        }
                        has_geometry = true;
                    }
                    "text-color" | "text-fill" => {
                        label.color = Some(val.to_string());
                        has_label = true;
                    }
                    "text-halo-color" | "text-halo" => {
                        label.halo = Some(val.to_string());
                        has_label = true;
                    }
                    "font-size" | "text-size" => {
                        if let Ok(px) = val.trim_end_matches("px").trim().parse::<f32>() {
                            if px.is_finite() && px > 0.0 {
                                label.size = Some(px);
                            }
                        }
                        has_label = true;
                    }
                    _ => {}
                }
            }
            if key == "canvas" {
                if fill != "none" {
                    canvas_fill = Some(fill);
                }
                continue;
            }
            if has_label {
                labels.insert(key.clone(), label);
            }
            // A rule of label properties alone styles no geometry (it would
            // otherwise draw every line of its layer in `none`).
            if has_geometry || !has_label {
                rules.insert(
                    key,
                    LayerStyle {
                        fill,
                        stroke,
                        stroke_width,
                    },
                );
            }
        }
        Self {
            rules,
            labels,
            canvas_fill,
        }
    }

    /// Resolve a feature's style. Lookup order: `layer.class` (a rule like
    /// `transportation.motorway { … }`), then the layer name exactly, then
    /// a rule whose key is a substring of the layer name, then the built-in
    /// palette. A rule with a `.class` key never matches by substring, so
    /// `transportation.motorway` cannot capture a minor road.
    fn resolve(&self, layer_name: &str, class: Option<&str>) -> LayerStyle {
        if !self.rules.is_empty() {
            let lower = layer_name.to_ascii_lowercase();
            if let Some(class) = class {
                let key = format!("{lower}.{}", class.to_ascii_lowercase());
                if let Some(s) = self.rules.get(&key) {
                    return s.clone();
                }
            }
            if let Some(s) = self.rules.get(&lower) {
                return s.clone();
            }
            for (key, style) in &self.rules {
                if !key.contains('.') && lower.contains(key.as_str()) {
                    return style.clone();
                }
            }
        }
        default_style(layer_name)
    }

    fn canvas(&self) -> &str {
        self.canvas_fill.as_deref().unwrap_or(DEFAULT_CANVAS_FILL)
    }

    /// Whether the sheet's land is dark (a dark theme): the built-in label
    /// paint is then light ink on a dark halo.
    fn is_dark(&self) -> bool {
        azul_layout::widgets::map_themes::parse_hex_rgb(self.canvas())
            .is_some_and(|rgb| azul_layout::widgets::map_themes::luma(rgb) < 0.5)
    }

    /// A label's paint: the built-in one for its layer / class, then the
    /// sheet's `layer` rule, then its `layer.class` rule, each overriding
    /// only what it declares. A river (`waterway`) takes the `water_name`
    /// rule first, so a theme colours every water label in one place.
    fn label_style(&self, layer_name: &str, class: Option<&str>) -> LabelStyle {
        let lower = layer_name.to_ascii_lowercase();
        let mut style = default_label_style(&lower, class, self.is_dark());
        if lower == "waterway" {
            if let Some(rule) = self.labels.get("water_name") {
                style.apply(rule);
            }
        }
        if let Some(rule) = self.labels.get(&lower) {
            style.apply(rule);
        }
        if let Some(class) = class {
            let key = format!("{lower}.{}", class.to_ascii_lowercase());
            if let Some(rule) = self.labels.get(&key) {
                style.apply(rule);
            }
        }
        style
    }
}

/// The `OpenMapTiles` layers that only carry LABELS: their points are label
/// anchors, their lines (road names, lake centre lines) the paths labels
/// follow. None of them is drawn as geometry - `transportation_name` used to
/// be, as a thin line of the generic road colour painted down the middle of
/// every styled motorway.
fn is_label_layer(layer_lower: &str) -> bool {
    layer_lower.ends_with("_name")
        || matches!(
            layer_lower,
            "place" | "poi" | "housenumber" | "mountain_peak" | "aerodrome_label"
        )
}

/// The layers whose named LINES carry a label along them (road names,
/// rivers, the centre lines of large lakes).
fn is_line_label_layer(layer_lower: &str) -> bool {
    matches!(
        layer_lower,
        "transportation_name" | "waterway" | "water_name"
    )
}

/// The built-in label paint of a layer / class (light or dark land).
fn default_label_style(layer_lower: &str, class: Option<&str>, dark: bool) -> LabelStyle {
    let water = matches!(layer_lower, "water_name" | "waterway");
    let (color, halo) = match (water, layer_lower, dark) {
        (true, _, false) => ("#4a6f8a", "#ffffff"),
        (true, _, true) => ("#7f9fbf", "#101418"),
        (false, "poi" | "housenumber", false) => ("#5a5a5a", "#ffffff"),
        (false, "poi" | "housenumber", true) => ("#a8a8a8", "#141414"),
        (false, "mountain_peak", false) => ("#6b5a45", "#ffffff"),
        (false, "transportation_name", false) => ("#4a4a4a", "#ffffff"),
        (false, _, false) => ("#333333", "#ffffff"),
        (false, _, true) => ("#dddddd", "#111111"),
    };
    let size = match (layer_lower, class) {
        ("place", Some("continent")) => 14.0,
        ("place", Some("country")) => 13.0,
        ("place", Some("state" | "province")) => 11.0,
        ("place", Some("city")) => 15.0,
        ("place", Some("town")) => 13.0,
        ("place", Some("village")) => 12.0,
        ("place", _) => 11.0,
        ("water_name", Some("ocean")) => 13.0,
        ("water_name", Some("sea")) => 12.0,
        ("housenumber", _) => 9.0,
        ("transportation_name" | "poi" | "mountain_peak", _) => 10.5,
        _ => 11.0,
    };
    LabelStyle {
        color: color.to_string(),
        halo: halo.to_string(),
        size,
    }
}

/// The order labels claim space in, lowest first: countries and cities
/// before towns, towns before roads, roads before shops, house numbers last;
/// within a kind by the tile's own `rank`.
#[allow(clippy::cast_possible_truncation)] // clamped to 0..=99 first
fn label_priority(layer_lower: &str, class: Option<&str>, rank: Option<f64>) -> i32 {
    let base = match (layer_lower, class) {
        ("place", Some("continent")) => 0,
        ("place", Some("country")) => 1,
        ("place", Some("city")) => 2,
        ("water_name", Some("ocean")) => 2,
        ("water_name", Some("sea")) => 3,
        ("place", Some("state" | "province")) => 4,
        ("place", Some("town")) => 5,
        ("aerodrome_label", _) => 7,
        ("place", Some("village")) => 8,
        ("place", Some("suburb" | "quarter")) => 9,
        ("water_name" | "waterway", _) => 10,
        ("place", _) => 11,
        ("mountain_peak", _) => 12,
        ("transportation_name", _) => 13,
        ("poi", _) => 14,
        ("housenumber", _) => 30,
        _ => 20,
    };
    let within = match (layer_lower, class) {
        // Roads carry no rank: a motorway's name before a lane's.
        ("transportation_name", Some(c)) => match c {
            "motorway" => 0.0,
            "trunk" => 10.0,
            "primary" => 20.0,
            "secondary" => 30.0,
            "tertiary" => 40.0,
            "minor" => 60.0,
            _ => 80.0,
        },
        _ => rank.unwrap_or(50.0),
    };
    let within = if within.is_finite() { within.clamp(0.0, 99.0) } else { 50.0 };
    base * 100 + within as i32
}

/// A feature's label text: a house number's number, otherwise its name
/// (Latin script first, so every font can draw it), without control
/// characters, at most 48 characters.
fn label_text(feature: &geojson::Feature, layer_lower: &str) -> Option<String> {
    let prop = |key: &str| {
        feature
            .property(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let text = if layer_lower == "housenumber" {
        prop("housenumber")
    } else {
        prop("name:latin")
            .or_else(|| prop("name"))
            .or_else(|| prop("name_en"))
            .or_else(|| prop("name_int"))
    }?;
    let text: String = text.chars().filter(|c| !c.is_control()).take(48).collect();
    (!text.trim().is_empty()).then_some(text)
}

/// One label of a tile, in the tile's 0..256 pixel space.
struct TileLabelOut {
    x: f64,
    y: f64,
    /// Degrees, clockwise; 0 for a point label (always upright).
    angle: f64,
    text: String,
    style: LabelStyle,
    priority: i32,
    kind: String,
    italic: bool,
}

/// Where a label runs along a line: the point halfway along it, the
/// direction of the segment there (turned so the text never reads upside
/// down) - or `None` for a line too short to carry `min_len` pixels of text.
fn line_label_anchor(points: &[(f64, f64)], min_len: f64) -> Option<(f64, f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let dist = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).hypot(b.1 - a.1);
    let total: f64 = points.windows(2).map(|w| dist(w[0], w[1])).sum();
    if !total.is_finite() || total < min_len {
        return None;
    }
    let half = total / 2.0;
    let mut walked = 0.0;
    for w in points.windows(2) {
        let d = dist(w[0], w[1]);
        if d > 0.0 && walked + d >= half {
            let t = (half - walked) / d;
            let x = w[0].0 + (w[1].0 - w[0].0) * t;
            let y = w[0].1 + (w[1].1 - w[0].1) * t;
            let mut angle = (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0).to_degrees();
            if angle > 90.0 {
                angle -= 180.0;
            } else if angle < -90.0 {
                angle += 180.0;
            }
            return Some((x, y, angle));
        }
        walked += d;
    }
    None
}

/// `s` safe inside an XML attribute or text node.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Whether a projected point lies in this tile (not in the buffer it
/// shares with its neighbours, who emit their own labels).
fn in_tile(x: f64, y: f64) -> bool {
    (0.0..TILE_PX).contains(&x) && (0.0..TILE_PX).contains(&y)
}

/// Convert one tile's worth of GeoJSON features into a self-contained
/// `<svg>` string. The SVG's viewBox is the tile's `0 0 256 256`
/// pixel space; user-side widget code wraps it with the inherited
/// `position: absolute; transform: translate(x, y)` styling.
///
/// `mapcss` is the layer's `MapTileLayer::style_css` (empty = built-in
/// palette). It drives per-MVT-layer fill / stroke / stroke-width.
pub fn features_to_svg(features: &[geojson::Feature], tile: MapTileId, mapcss: &str) -> String {
    let style_sheet = MapCss::parse(mapcss);
    let mut out = String::with_capacity(features.len().saturating_mul(96) + 256);
    out.push_str(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 256 256\" width=\"256\" \
         height=\"256\">",
    );
    // Opaque base layer covering the whole tile. MVT encodes islands/lakes as
    // EVEN-ODD HOLES in a larger polygon (e.g. arctic islands are holes in the
    // ocean `water` polygon) — without a base, those holes are transparent and
    // show whatever is behind the tile (a dark parent → solid-black islands once
    // the placeholder tile background was removed). The base = the land colour,
    // so holes read as land and ocean is painted over it by the `water` polygons.
    // A theme's `canvas { fill: … }` rule drives this base colour (a dark
    // theme needs a dark land under the water holes); the built-in light
    // land colour otherwise.
    let _ = core::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "<rect x=\"0\" y=\"0\" width=\"256\" height=\"256\" fill=\"{}\" />",
            style_sheet.canvas()
        ),
    );

    // Tile bounding box in degrees. We project each Position back into
    // the 0..256 pixel range of *this* tile.
    let tile_count = 1u32 << tile.z;
    let tile_count_f = tile_count as f64;
    let lon_west = tile.x as f64 / tile_count_f * 360.0 - 180.0;
    // Use Web-Mercator forward transform for lat → world y, then
    // localise. Avoids a separate lat_north/south computation.
    // Clamp to the Web-Mercator latitude limit (±85.0511°, where the projection
    // is square). Beyond it tan(lat) explodes: a polygon vertex at the pole
    // (lat 90°) projects to local_y ≈ -5685 px — ~22 tiles off-tile — and such
    // extreme off-tile coordinates overflow the CPU SVG rasteriser's edge math
    // into solid-black fills (the user-reported high-arctic black polygons).
    // Clamping pins near-pole vertices to the tile's top/bottom edge, which is
    // the correct Web-Mercator behaviour anyway (the poles are at infinity).
    let mercator_y = |lat: f64| -> f64 {
        const MAX_MERCATOR_LAT: f64 = 85.051_128_779_806_59;
        let r = lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT).to_radians();
        (1.0 - (r.tan() + 1.0 / r.cos()).ln() / core::f64::consts::PI) / 2.0
    };
    let project = |lon: f64, lat: f64| -> (f64, f64) {
        let world_x = (lon + 180.0) / 360.0 * tile_count_f;
        let world_y = mercator_y(lat) * tile_count_f;
        let local_x = (world_x - tile.x as f64) * TILE_PX;
        let local_y = (world_y - tile.y as f64) * TILE_PX;
        (local_x, local_y)
    };

    let _ = lon_west; // referenced in comments; consumed implicitly by `project`.

    let mut labels: Vec<TileLabelOut> = Vec::new();

    for feature in features {
        let layer_name = feature
            .property("layer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let layer_lower = layer_name.to_ascii_lowercase();
        // OpenMapTiles puts the road / land kind in `class`
        // (`motorway`, `minor`, `grass`, `wood`, …): what lets a theme colour
        // a motorway differently from a lane.
        let class = feature.property("class").and_then(|v| v.as_str());
        let style = style_sheet.resolve(layer_name, class);
        let draws_geometry = !is_label_layer(&layer_lower);

        let Some(geom) = feature.geometry.as_ref() else {
            continue;
        };

        // The label this feature carries, if any, styled for its layer.
        let label = |x: f64, y: f64, angle: f64, text: String| TileLabelOut {
            x,
            y,
            angle,
            text,
            style: style_sheet.label_style(layer_name, class),
            priority: label_priority(
                &layer_lower,
                class,
                feature.property("rank").and_then(serde_json::Value::as_f64),
            ),
            kind: layer_lower.clone(),
            italic: matches!(layer_lower.as_str(), "water_name" | "waterway"),
        };
        // A line's label: halfway along the longest of its parts that can
        // carry the text, if that point lies in this tile.
        let line_label = |parts: &[&Vec<Vec<f64>>]| -> Option<TileLabelOut> {
            if !is_line_label_layer(&layer_lower) {
                return None;
            }
            let text = label_text(feature, &layer_lower)?;
            let size = f64::from(style_sheet.label_style(layer_name, class).size);
            let min_len = text.chars().count() as f64 * size * 0.62 + 8.0;
            let mut best: Option<(f64, (f64, f64, f64))> = None;
            for part in parts {
                let points: Vec<(f64, f64)> = part
                    .iter()
                    .filter(|p| p.len() >= 2)
                    .map(|p| project(p[0], p[1]))
                    .collect();
                let len: f64 = points
                    .windows(2)
                    .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                    .sum();
                if best.as_ref().is_some_and(|(l, _)| *l >= len) {
                    continue;
                }
                if let Some(anchor) = line_label_anchor(&points, min_len) {
                    best = Some((len, anchor));
                }
            }
            let (_, (x, y, angle)) = best?;
            in_tile(x, y).then(|| label(x, y, angle, text))
        };

        match &geom.value {
            // Point / MultiPoint features in an MVT tile are LABEL ANCHORS and POI
            // markers (place names, mountain peaks, POIs, housenumbers …) — not
            // shapes meant to be drawn (drawn as circles they scattered little grey
            // dots across every country). A named one becomes a label, in the ONE
            // tile its anchor lies in.
            geojson::Value::Point(pos) => {
                if pos.len() >= 2 {
                    let (x, y) = project(pos[0], pos[1]);
                    if in_tile(x, y) {
                        if let Some(text) = label_text(feature, &layer_lower) {
                            labels.push(label(x, y, 0.0, text));
                        }
                    }
                }
            }
            geojson::Value::MultiPoint(points) => {
                if let Some(pos) = points.iter().find(|p| p.len() >= 2) {
                    let (x, y) = project(pos[0], pos[1]);
                    if in_tile(x, y) {
                        if let Some(text) = label_text(feature, &layer_lower) {
                            labels.push(label(x, y, 0.0, text));
                        }
                    }
                }
            }
            geojson::Value::LineString(line) => {
                if draws_geometry {
                    emit_polyline(&mut out, line, &project, &style);
                }
                if let Some(l) = line_label(&[line]) {
                    labels.push(l);
                }
            }
            geojson::Value::MultiLineString(lines) => {
                if draws_geometry {
                    for line in lines {
                        emit_polyline(&mut out, line, &project, &style);
                    }
                }
                let parts: Vec<&Vec<Vec<f64>>> = lines.iter().collect();
                if let Some(l) = line_label(&parts) {
                    labels.push(l);
                }
            }
            geojson::Value::Polygon(rings) => {
                if draws_geometry {
                    emit_polygon(&mut out, rings, &project, &style);
                }
            }
            geojson::Value::MultiPolygon(polys) => {
                if draws_geometry {
                    for rings in polys {
                        emit_polygon(&mut out, rings, &project, &style);
                    }
                }
            }
            geojson::Value::GeometryCollection(_) => {
                // Rare; defer for next tick.
            }
        }
    }

    emit_labels(&mut out, labels);
    out.push_str("</svg>");
    out
}

/// The tile's labels, most important first and at most
/// [`MAX_TILE_LABELS`], as `<text>` elements in the block the widget cuts
/// out (`TILE_LABELS_OPEN` … `</g>`). Standard SVG text - anchored at its
/// middle, the halo a stroke painted under the fill - so any other consumer
/// of the document draws them too.
fn emit_labels(out: &mut String, mut labels: Vec<TileLabelOut>) {
    use core::fmt::Write;
    if labels.is_empty() {
        return;
    }
    labels.sort_by_key(|l| l.priority);
    labels.truncate(MAX_TILE_LABELS);
    out.push_str(TILE_LABELS_OPEN);
    for l in &labels {
        let _ = write!(
            out,
            "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.1}\" fill=\"{}\" stroke=\"{}\" \
             stroke-width=\"2.5\" paint-order=\"stroke\" text-anchor=\"middle\" \
             dominant-baseline=\"central\" data-kind=\"{}\" data-priority=\"{}\"",
            l.x,
            l.y,
            l.style.size,
            xml_escape(&l.style.color),
            xml_escape(&l.style.halo),
            xml_escape(&l.kind),
            l.priority,
        );
        if l.italic {
            out.push_str(" font-style=\"italic\"");
        }
        if l.angle.abs() > 0.01 {
            let _ = write!(
                out,
                " transform=\"rotate({:.2} {:.2} {:.2})\"",
                l.angle, l.x, l.y
            );
        }
        out.push('>');
        out.push_str(&xml_escape(&l.text));
        out.push_str("</text>");
    }
    out.push_str("</g>");
}

// Retained for the future text/icon-on-map feature (see the Point arm above).
#[allow(dead_code)]
fn read_pos<F: Fn(f64, f64) -> (f64, f64)>(pos: &[f64], project: &F) -> (f64, f64) {
    if pos.len() < 2 {
        return (0.0, 0.0);
    }
    project(pos[0], pos[1])
}

#[allow(dead_code)]
fn emit_circle(out: &mut String, x: f64, y: f64, style: &LayerStyle) {
    use core::fmt::Write;
    let _ = write!(
        out,
        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"1.2\" fill=\"{}\" />",
        x, y, style.stroke
    );
}

fn emit_polyline<F: Fn(f64, f64) -> (f64, f64)>(
    out: &mut String,
    line: &[Vec<f64>],
    project: &F,
    style: &LayerStyle,
) {
    if line.len() < 2 {
        return;
    }
    out.push_str("<polyline points=\"");
    write_points(out, line, project);
    out.push_str("\" fill=\"none\" stroke=\"");
    out.push_str(&style.stroke);
    out.push_str("\" stroke-width=\"");
    let _ = core::fmt::Write::write_fmt(out, format_args!("{:.2}", style.stroke_width));
    out.push_str("\" stroke-linecap=\"round\" stroke-linejoin=\"round\" />");
}

fn emit_polygon<F: Fn(f64, f64) -> (f64, f64)>(
    out: &mut String,
    rings: &[Vec<Vec<f64>>],
    project: &F,
    style: &LayerStyle,
) {
    if rings.is_empty() {
        return;
    }
    out.push_str("<path d=\"");
    for (ring_idx, ring) in rings.iter().enumerate() {
        if ring.len() < 3 {
            continue;
        }
        let cmd = if ring_idx == 0 { 'M' } else { 'M' }; // SVG fills holes via even-odd; both rings
                                                         // start with M.
        for (i, p) in ring.iter().enumerate() {
            if p.len() < 2 {
                continue;
            }
            let (x, y) = project(p[0], p[1]);
            if i == 0 {
                let _ = core::fmt::Write::write_fmt(out, format_args!("{}{:.2},{:.2}", cmd, x, y));
            } else {
                let _ = core::fmt::Write::write_fmt(out, format_args!(" L{:.2},{:.2}", x, y));
            }
        }
        out.push('Z');
    }
    out.push_str("\" fill=\"");
    out.push_str(&style.fill);
    out.push_str("\" stroke=\"");
    out.push_str(&style.stroke);
    out.push_str("\" stroke-width=\"");
    let _ = core::fmt::Write::write_fmt(out, format_args!("{:.2}", style.stroke_width));
    out.push_str("\" fill-rule=\"evenodd\" />");
}

fn write_points<F: Fn(f64, f64) -> (f64, f64)>(out: &mut String, line: &[Vec<f64>], project: &F) {
    for (i, p) in line.iter().enumerate() {
        if p.len() < 2 {
            continue;
        }
        let (x, y) = project(p[0], p[1]);
        if i > 0 {
            out.push(' ');
        }
        let _ = core::fmt::Write::write_fmt(out, format_args!("{:.2},{:.2}", x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_feature(layer: &str, class: Option<&str>) -> geojson::Feature {
        let mut props = serde_json::Map::new();
        props.insert(
            "layer".to_string(),
            serde_json::Value::String(layer.to_string()),
        );
        if let Some(c) = class {
            props.insert(
                "class".to_string(),
                serde_json::Value::String(c.to_string()),
            );
        }
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(vec![
                vec![-122.40, 37.78],
                vec![-122.41, 37.77],
            ]))),
            id: None,
            properties: Some(props),
            foreign_members: None,
        }
    }

    #[test]
    fn a_theme_drives_the_canvas_and_colours_a_motorway_by_class() {
        use azul_layout::widgets::map::MapLook;
        let tile = MapTileId {
            z: 11,
            x: 327,
            y: 791,
        };
        // Dark Matter: dark base, motorway lighter than a minor road.
        let dark = MapLook::DarkMatter.stylesheet();
        let dark = dark.as_str();
        let svg = features_to_svg(
            &[line_feature("transportation", Some("motorway"))],
            tile,
            dark,
        );
        assert!(
            svg.contains("fill=\"#0c0c0c\""),
            "the theme's canvas fill is the base rect: {svg}"
        );
        assert!(
            svg.contains("stroke=\"#333333\""),
            "transportation.motorway rule applies: {svg}"
        );
        let svg = features_to_svg(&[line_feature("transportation", Some("minor"))], tile, dark);
        assert!(
            svg.contains("stroke=\"#222222\""),
            "no `.minor` rule: the layer rule applies: {svg}"
        );
        // No theme: the built-in light base and palette, unchanged.
        let svg = features_to_svg(
            &[line_feature("transportation", Some("motorway"))],
            tile,
            "",
        );
        assert!(svg.contains("fill=\"#d6d8db\""), "{svg}");
        assert!(svg.contains("stroke=\"#f0e8d8\""), "{svg}");
    }

    #[test]
    fn every_preset_parses_into_rules_with_a_canvas() {
        use azul_layout::widgets::map::MapLook;
        for theme in [
            MapLook::Positron,
            MapLook::Bright,
            MapLook::Liberty,
            MapLook::DarkMatter,
            MapLook::GoogleLight,
            MapLook::GoogleNight,
            MapLook::AppleLight,
            MapLook::AppleDark,
        ] {
            let sheet = MapCss::parse(theme.stylesheet().as_str());
            assert!(
                sheet.canvas_fill.is_some(),
                "{theme:?}: no canvas rule parsed"
            );
            assert!(
                sheet.rules.contains_key("water"),
                "{theme:?}: no water rule"
            );
            assert!(
                sheet.rules.contains_key("transportation.motorway"),
                "{theme:?}: no motorway rule"
            );
            let water = sheet.resolve("water", None);
            assert!(
                water.fill.starts_with('#'),
                "{theme:?}: water fill {:?}",
                water.fill
            );
            // a `.class` key never captures a plain layer by substring
            assert_eq!(
                sheet.resolve("transportation", None).stroke,
                sheet.rules["transportation"].stroke
            );
        }
    }

    #[test]
    fn empty_features_emit_empty_svg() {
        let svg = features_to_svg(&[], MapTileId { z: 0, x: 0, y: 0 }, "");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        // No primitives — just the wrapper.
        assert!(!svg.contains("<path"));
        assert!(!svg.contains("<polyline"));
        assert!(!svg.contains("<circle"));
    }

    #[test]
    fn point_features_are_skipped_not_drawn_as_dots() {
        let mut f = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![
                -122.4194, 37.7749, // San Francisco
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        // Attach a "layer" property so the style lookup runs.
        let mut props = serde_json::Map::new();
        props.insert(
            "layer".to_string(),
            serde_json::Value::String("place".to_string()),
        );
        f.properties = Some(props);

        // Point features (place/POI label anchors) must NOT be drawn as dots.
        let svg = features_to_svg(
            &[f],
            MapTileId {
                z: 11,
                x: 327,
                y: 791,
            },
            "",
        );
        assert!(!svg.contains("<circle"));
    }

    #[test]
    fn high_latitude_polygon_coords_stay_bounded() {
        // A polygon ring reaching the North Pole (lat 90°). Without the
        // Web-Mercator latitude clamp in `mercator_y`, the pole vertex projects
        // to local_y ≈ -5685 px (~22 tiles above this tile); such extreme
        // off-tile coordinates overflow the CPU SVG rasteriser into solid-black
        // fills (reported high-arctic black polygons). The clamp pins near-pole
        // vertices to the tile edge, so every projected coordinate stays sane.
        let ring = vec![
            vec![-100.0, 80.0],
            vec![-60.0, 83.0],
            vec![-80.0, 90.0], // North Pole — the Mercator singularity
            vec![-100.0, 80.0],
        ];
        let mut f = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![ring]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let mut props = serde_json::Map::new();
        props.insert(
            "layer".to_string(),
            serde_json::Value::String("ice".to_string()),
        );
        f.properties = Some(props);

        let svg = features_to_svg(&[f], MapTileId { z: 2, x: 1, y: 0 }, "");
        assert!(
            !svg.contains("inf") && !svg.contains("NaN"),
            "non-finite coordinate leaked into SVG: {svg}"
        );
        // Bound every coordinate inside the <path d="..."> data. A clamped 90°
        // vertex yields local_y ≈ 0; an unclamped one yields ~-5685. Scope the
        // scan to the path data only (the xmlns URL contains a literal "2000").
        let mut rest = svg.as_str();
        let mut checked = 0usize;
        while let Some(start) = rest.find("d=\"") {
            let data = &rest[start + 3..];
            let end = data.find('"').expect("unterminated d attribute");
            for tok in data[..end].split(|c: char| matches!(c, 'M' | 'L' | 'Z' | ' ' | ',')) {
                if let Ok(v) = tok.trim().parse::<f64>() {
                    checked += 1;
                    assert!(
                        v.abs() < 2000.0,
                        "projected coordinate {v} is out of bounds (unclamped Mercator?): {svg}"
                    );
                }
            }
            rest = &data[end..];
        }
        assert!(
            checked >= 8,
            "expected path coordinates to check, got {checked}: {svg}"
        );
    }

    // ---- labels ----

    /// The z11 tile San Francisco (-122.4194, 37.7749) lies in, at about
    /// (146, 102) of its 256 pixels.
    const SF_TILE: MapTileId = MapTileId {
        z: 11,
        x: 327,
        y: 791,
    };

    fn props(
        layer: &str,
        class: Option<&str>,
        name: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut props = serde_json::Map::new();
        props.insert("layer".to_string(), serde_json::Value::String(layer.to_string()));
        props.insert("name".to_string(), serde_json::Value::String(name.to_string()));
        if let Some(c) = class {
            props.insert("class".to_string(), serde_json::Value::String(c.to_string()));
        }
        props
    }

    fn named_point(
        layer: &str,
        class: Option<&str>,
        name: &str,
        lon: f64,
        lat: f64,
    ) -> geojson::Feature {
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![lon, lat]))),
            id: None,
            properties: Some(props(layer, class, name)),
            foreign_members: None,
        }
    }

    fn named_line(
        layer: &str,
        class: Option<&str>,
        name: &str,
        coords: Vec<Vec<f64>>,
    ) -> geojson::Feature {
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(coords))),
            id: None,
            properties: Some(props(layer, class, name)),
            foreign_members: None,
        }
    }

    /// The angle of the first rotated label.
    fn label_angle(svg: &str) -> Option<f64> {
        let key = "transform=\"rotate(";
        let at = svg.find(key)? + key.len();
        svg[at..].split(' ').next()?.parse().ok()
    }

    #[test]
    fn a_named_place_becomes_a_label_after_the_geometry() {
        let svg = features_to_svg(
            &[named_point("place", Some("city"), "San Francisco & Co <", -122.4194, 37.7749)],
            SF_TILE,
            "",
        );
        let block = svg.find(TILE_LABELS_OPEN).expect("a label block");
        assert!(svg[..block].contains("<rect"), "the geometry comes first: {svg}");
        assert!(svg.ends_with("</g></svg>"), "the block closes before the svg: {svg}");
        assert!(svg.contains(">San Francisco &amp; Co &lt;</text>"), "escaped text: {svg}");
        assert!(svg.contains("font-size=\"15.0\""), "a city's built-in size: {svg}");
        assert!(svg.contains("data-kind=\"place\""), "{svg}");
        assert!(!svg.contains("transform="), "a point label is upright: {svg}");
    }

    #[test]
    fn a_label_in_the_neighbours_buffer_is_left_to_the_neighbour() {
        // -122.0 is two tiles east of this one: in its buffer at most.
        let svg = features_to_svg(
            &[named_point("place", Some("town"), "Elsewhere", -122.0, 37.7749)],
            SF_TILE,
            "",
        );
        assert!(!svg.contains("<text"), "{svg}");
        assert!(!svg.contains(TILE_LABELS_OPEN), "no empty block either: {svg}");
    }

    #[test]
    fn a_label_rule_paints_labels_and_no_geometry() {
        let sheet = "canvas { fill: #f0f0f0; } place { text-color: #123456; text-halo-color: \
                     #fedcba; } place.city { font-size: 17; } transportation { stroke: #ff0000; }";
        let svg = features_to_svg(
            &[named_point("place", Some("city"), "Oakland", -122.4194, 37.7749)],
            SF_TILE,
            sheet,
        );
        assert!(svg.contains("fill=\"#123456\""), "{svg}");
        assert!(svg.contains("stroke=\"#fedcba\""), "{svg}");
        assert!(svg.contains("font-size=\"17.0\""), "the class rule changes the size alone: {svg}");
        let css = MapCss::parse(sheet);
        assert!(!css.rules.contains_key("place"), "a label rule styles no geometry");
        assert!(css.rules.contains_key("transportation"));
        assert!(css.labels.contains_key("place.city"));
    }

    #[test]
    fn a_road_name_runs_along_its_road_and_never_upside_down() {
        let up = vec![vec![-122.44, 37.765], vec![-122.40, 37.785]];
        let down: Vec<Vec<f64>> = up.iter().rev().cloned().collect();
        let a = features_to_svg(
            &[named_line("transportation_name", Some("primary"), "Main", up)],
            SF_TILE,
            "",
        );
        let b = features_to_svg(
            &[named_line("transportation_name", Some("primary"), "Main", down)],
            SF_TILE,
            "",
        );
        assert!(!a.contains("<polyline"), "a name layer draws no line of its own: {a}");
        assert!(a.contains(">Main</text>"), "{a}");
        let (ea, wa) = (label_angle(&a).expect("rotated"), label_angle(&b).expect("rotated"));
        assert!(ea < -10.0 && ea > -60.0, "a road rising to the north-east reads upwards: {ea}");
        assert!((ea - wa).abs() < 0.1, "either direction reads the same way: {ea} {wa}");
        let short = named_line(
            "transportation_name",
            None,
            "A very long street name",
            vec![vec![-122.4194, 37.7749], vec![-122.4190, 37.7749]],
        );
        assert!(
            !features_to_svg(&[short], SF_TILE, "").contains("<text"),
            "a road too short for its name carries none"
        );
    }

    #[test]
    fn labels_come_most_important_first() {
        let svg = features_to_svg(
            &[
                named_point("poi", Some("cafe"), "Cafe", -122.4194, 37.7749),
                named_point("place", Some("city"), "City", -122.4180, 37.7740),
            ],
            SF_TILE,
            "",
        );
        let city = svg.find(">City<").expect("the city");
        let cafe = svg.find(">Cafe<").expect("the cafe");
        assert!(city < cafe, "{svg}");
    }

    #[test]
    fn a_dark_theme_gets_light_label_ink_and_a_light_one_dark_ink() {
        use azul_layout::widgets::{map::MapLook, map_themes::{luma, parse_hex_rgb}};
        let dark = MapLook::DarkMatter.stylesheet();
        let s = MapCss::parse(dark.as_str()).label_style("place", Some("town"));
        assert!(luma(parse_hex_rgb(&s.color).expect("hex ink")) > 0.5, "{s:?}");
        let s = MapCss::parse("").label_style("place", Some("town"));
        assert!(luma(parse_hex_rgb(&s.color).expect("hex ink")) < 0.5, "{s:?}");
        let s = MapCss::parse("canvas { fill: #101010; }").label_style("poi", None);
        let ink = parse_hex_rgb(&s.color).expect("hex ink");
        assert!(luma(ink) > 0.5, "no rules, dark land: {s:?}");
    }
}
