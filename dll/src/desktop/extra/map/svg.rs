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

#![cfg(feature = "map-tiles")]

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use azul_layout::widgets::map::MapTileId;

const TILE_PX: f64 = 256.0;

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
    } else if lower.contains("park") || lower.contains("landcover") || lower.contains("landuse_grass") {
        LayerStyle::make("#c8e0c0", "#a8c89c", 0.4)
    } else if lower.contains("boundary") || lower.contains("admin") {
        LayerStyle::make("none", "#9a8aa0", 0.6)
    } else {
        LayerStyle::make("#d6d8db", "#a8acb1", 0.4)
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
struct MapCss {
    rules: BTreeMap<String, LayerStyle>,
}

impl MapCss {
    fn parse(src: &str) -> Self {
        let mut rules = BTreeMap::new();
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
            for decl in body.split(';') {
                let Some(colon) = decl.find(':') else { continue };
                let prop = decl[..colon].trim().to_ascii_lowercase();
                let val = decl[colon + 1..].trim();
                match prop.as_str() {
                    "fill" | "fill-color" => fill = val.to_string(),
                    "stroke" | "color" | "casing-color" => stroke = val.to_string(),
                    "stroke-width" | "width" | "casing-width" => {
                        if let Ok(w) = val.trim_end_matches("px").trim().parse::<f32>() {
                            stroke_width = w;
                        }
                    }
                    _ => {}
                }
            }
            rules.insert(key, LayerStyle { fill, stroke, stroke_width });
        }
        Self { rules }
    }

    /// Resolve a layer's style: a MapCSS rule whose key is a substring
    /// of (or equal to) the layer name wins; otherwise the built-in
    /// palette. Empty stylesheet → always the palette.
    fn resolve(&self, layer_name: &str) -> LayerStyle {
        if !self.rules.is_empty() {
            let lower = layer_name.to_ascii_lowercase();
            // Exact match first, then substring.
            if let Some(s) = self.rules.get(&lower) {
                return s.clone();
            }
            for (key, style) in &self.rules {
                if lower.contains(key.as_str()) {
                    return style.clone();
                }
            }
        }
        default_style(layer_name)
    }
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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"0 0 256 256\" width=\"256\" height=\"256\">",
    );

    // Tile bounding box in degrees. We project each Position back into
    // the 0..256 pixel range of *this* tile.
    let tile_count = 1u32 << tile.z;
    let tile_count_f = tile_count as f64;
    let lon_west = tile.x as f64 / tile_count_f * 360.0 - 180.0;
    // Use Web-Mercator forward transform for lat → world y, then
    // localise. Avoids a separate lat_north/south computation.
    let mercator_y = |lat: f64| -> f64 {
        let r = lat.to_radians();
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

    for feature in features {
        let layer_name = feature
            .property("layer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let style = style_sheet.resolve(layer_name);

        let Some(geom) = feature.geometry.as_ref() else {
            continue;
        };

        match &geom.value {
            // Point / MultiPoint features in an MVT tile are LABEL ANCHORS and POI
            // markers (place names, mountain peaks, POIs, housenumbers …) — not
            // shapes meant to be drawn. Rendering them as raw circles scattered
            // little grey dots across every country (user-reported). A real map
            // renders these as text labels / icons (a future text-on-map feature);
            // until then they are skipped rather than drawn as dots. `emit_circle`
            // is retained for that future use.
            geojson::Value::Point(_) | geojson::Value::MultiPoint(_) => {}
            geojson::Value::LineString(line) => {
                emit_polyline(&mut out, line, &project, &style);
            }
            geojson::Value::MultiLineString(lines) => {
                for line in lines {
                    emit_polyline(&mut out, line, &project, &style);
                }
            }
            geojson::Value::Polygon(rings) => {
                emit_polygon(&mut out, rings, &project, &style);
            }
            geojson::Value::MultiPolygon(polys) => {
                for rings in polys {
                    emit_polygon(&mut out, rings, &project, &style);
                }
            }
            geojson::Value::GeometryCollection(_) => {
                // Rare; defer for next tick.
            }
        }
    }

    out.push_str("</svg>");
    out
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
        let cmd = if ring_idx == 0 { 'M' } else { 'M' }; // SVG fills holes via even-odd; both rings start with M.
        for (i, p) in ring.iter().enumerate() {
            if p.len() < 2 {
                continue;
            }
            let (x, y) = project(p[0], p[1]);
            if i == 0 {
                let _ = core::fmt::Write::write_fmt(
                    out,
                    format_args!("{}{:.2},{:.2}", cmd, x, y),
                );
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

fn write_points<F: Fn(f64, f64) -> (f64, f64)>(
    out: &mut String,
    line: &[Vec<f64>],
    project: &F,
) {
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
        let svg = features_to_svg(&[f], MapTileId { z: 11, x: 327, y: 791 }, "");
        assert!(!svg.contains("<circle"));
    }
}
