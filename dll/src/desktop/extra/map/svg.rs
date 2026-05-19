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

use alloc::string::String;
use alloc::vec::Vec;

use azul_layout::widgets::map::MapTileId;

const TILE_PX: f64 = 256.0;

/// Default per-layer styling. Looked up by the feature's `"layer"`
/// property. Anything we don't recognise falls back to `default()`.
struct LayerStyle {
    fill: &'static str,
    stroke: &'static str,
    stroke_width: f32,
}

impl LayerStyle {
    fn default() -> Self {
        Self {
            fill: "#d6d8db",
            stroke: "#a8acb1",
            stroke_width: 0.4,
        }
    }
    fn water() -> Self {
        Self {
            fill: "#9ecae1",
            stroke: "#75a8c8",
            stroke_width: 0.5,
        }
    }
    fn buildings() -> Self {
        Self {
            fill: "#e0d8c8",
            stroke: "#b8ad99",
            stroke_width: 0.3,
        }
    }
    fn roads_major() -> Self {
        Self {
            fill: "none",
            stroke: "#ffffff",
            stroke_width: 1.6,
        }
    }
    fn roads() -> Self {
        Self {
            fill: "none",
            stroke: "#f0e8d8",
            stroke_width: 0.8,
        }
    }
    fn parks() -> Self {
        Self {
            fill: "#c8e0c0",
            stroke: "#a8c89c",
            stroke_width: 0.4,
        }
    }
    fn boundary() -> Self {
        Self {
            fill: "none",
            stroke: "#9a8aa0",
            stroke_width: 0.6,
        }
    }
}

fn lookup_style(layer_name: &str) -> LayerStyle {
    // Loose-match against the standard OpenMapTiles / OpenFreeMap
    // layer names. This is a placeholder for the MapCSS layer that
    // lands next.
    let lower = layer_name.to_ascii_lowercase();
    if lower.contains("water") {
        return LayerStyle::water();
    }
    if lower.contains("building") {
        return LayerStyle::buildings();
    }
    if lower.contains("transportation_name") || lower.contains("highway") {
        return LayerStyle::roads_major();
    }
    if lower.contains("transportation") || lower.contains("road") {
        return LayerStyle::roads();
    }
    if lower.contains("park") || lower.contains("landcover") || lower.contains("landuse_grass") {
        return LayerStyle::parks();
    }
    if lower.contains("boundary") || lower.contains("admin") {
        return LayerStyle::boundary();
    }
    LayerStyle::default()
}

/// Convert one tile's worth of GeoJSON features into a self-contained
/// `<svg>` string. The SVG's viewBox is the tile's `0 0 256 256`
/// pixel space; user-side widget code wraps it with the inherited
/// `position: absolute; transform: translate(x, y)` styling.
pub fn features_to_svg(features: &[geojson::Feature], tile: MapTileId) -> String {
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
        let style = lookup_style(layer_name);

        let Some(geom) = feature.geometry.as_ref() else {
            continue;
        };

        match &geom.value {
            geojson::Value::Point(pos) => {
                let (x, y) = read_pos(pos, &project);
                emit_circle(&mut out, x, y, &style);
            }
            geojson::Value::MultiPoint(points) => {
                for pos in points {
                    let (x, y) = read_pos(pos, &project);
                    emit_circle(&mut out, x, y, &style);
                }
            }
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

fn read_pos<F: Fn(f64, f64) -> (f64, f64)>(pos: &[f64], project: &F) -> (f64, f64) {
    if pos.len() < 2 {
        return (0.0, 0.0);
    }
    project(pos[0], pos[1])
}

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
    out.push_str(style.stroke);
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
    out.push_str(style.fill);
    out.push_str("\" stroke=\"");
    out.push_str(style.stroke);
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
        let svg = features_to_svg(&[], MapTileId { z: 0, x: 0, y: 0 });
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        // No primitives — just the wrapper.
        assert!(!svg.contains("<path"));
        assert!(!svg.contains("<polyline"));
        assert!(!svg.contains("<circle"));
    }

    #[test]
    fn point_emits_circle() {
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

        let svg = features_to_svg(&[f], MapTileId { z: 11, x: 327, y: 791 });
        assert!(svg.contains("<circle"));
    }
}
