use std::collections::BTreeMap;

use geo_types::Geometry;
use geojson::{Feature, FeatureCollection};
#[cfg(feature = "log")]
use log::{trace, warn};
use mvt_reader::{Reader as TileReader, error::ParserError, feature::Feature as MvtFeature};
use proj4rs::Proj;
use serde_json::{Number, Value as JsonValue};

/// Placeholder logging macros when the `logging` feature is disabled
#[cfg(not(feature = "log"))]
#[macro_use]
pub mod logging {
    macro_rules! trace {
        ($($arg:tt)*) => {};
    }
    macro_rules! warn {
        ($($arg:tt)*) => {};
    }
}

/// Represents the geographical bounding box of the map.
/// Coordinates are in WGS84 (EPSG:4326).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub north: f32,
    pub east: f32,
    pub south: f32,
    pub west: f32,
    pub epsg: u32, // Always 4326 for WGS84 geographic coordinates
}

/// Represents a single tile's Z/X/Y coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileCoord {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

// --- Public API Functions ---

/// Calculates the geographical extent (bounding box) of a map given its center,
/// physical dimensions, scale, and projection type.
///
/// The `longitude` and `latitude` define the WGS84 center of the map.
/// The `scale` is the map scale (e.g., 25000 for 1:25,000).
/// The `width_mm` and `height_mm` are the physical dimensions of the map in millimeters.
/// The `projection` can be "utm" or "mercator" (for web Mercator like behavior).
///
/// # Returns
/// A `Result` containing an `Extent` struct on success, or an error if projection
/// transformation fails.
///
/// # Errors
/// Returns an error if `proj4rs` fails to parse projection strings or transform points.
pub fn calculate_extent(
    longitude: f64,
    latitude: f64,
    scale: u32,
    width_mm: u32,
    height_mm: u32,
    projection: &str,
) -> Result<Extent, String> {
    let wgs84 = get_cached_proj("+proj=longlat +datum=WGS84 +no_defs +type=crs")?;

    let target_proj_str = get_target_proj_str_for_extent_calc(longitude, latitude, projection);
    let target_proj = get_cached_proj(&target_proj_str)?;

    let (width_m, height_m) = get_map_dimensions_m(scale, width_mm, height_mm);

    let half_width = width_m / 2.0;
    let half_height = height_m / 2.0;

    // Convert center point to radians before transformation for `proj4rs +proj=longlat` input
    let mut center = (
        longitude.to_radians(),
        latitude.to_radians(),
        0.0, // Z coordinate, typically 0
    );

    // Transform center from WGS84 to the target projection (e.g., UTM or Mercator)
    proj4rs::transform::transform(&wgs84, &target_proj, &mut center)
        .map_err(|e| format!("Failed to transform center point: {:?}", e))?;

    // Calculate corner points in the target projection's meter-based coordinates
    let corners_meters = [
        (center.0 - half_width, center.1 + half_height, 0.0), // North-West
        (center.0 + half_width, center.1 + half_height, 0.0), // North-East
        (center.0 + half_width, center.1 - half_height, 0.0), // South-East
        (center.0 - half_width, center.1 - half_height, 0.0), // South-West
    ];

    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;

    // Transform each corner back to WGS84 (lat/lng) and find the min/max
    for mut corner in corners_meters {
        proj4rs::transform::transform(&target_proj, &wgs84, &mut corner)
            .map_err(|e| format!("Failed to transform corner: {:?}", e))?;

        // Convert back to degrees from radians for WGS84 output
        let lon_deg = corner.0.to_degrees();
        let lat_deg = corner.1.to_degrees();

        min_lon = min_lon.min(lon_deg);
        max_lon = max_lon.max(lon_deg);
        min_lat = min_lat.min(lat_deg);
        max_lat = max_lat.max(lat_deg);
    }

    Ok(Extent {
        north: max_lat as f32,
        east: max_lon as f32,
        south: min_lat as f32,
        west: min_lon as f32,
        epsg: 4326, // WGS84
    })
}

/// Determines the appropriate OSM/Web Mercator zoom level for a given map scale.
/// This is an empirical mapping often used for tile services.
///
/// # Arguments
/// * `scale` - The map scale (e.g., 25000 for 1:25,000).
///
/// # Returns
/// The suggested zoom level (u32).
pub fn calculate_zoom_level(scale: u32) -> u32 {
    match scale {
        // Adjust these ranges as per typical map service scaling.
        // These approximate common display resolutions for given scales.
        0..=50_000 => 14,
        50_001..=100_000 => 13,
        100_001..=250_000 => 12,
        250_001..=500_000 => 11,
        500_001..=1_000_000 => 10,
        1_000_001..=2_500_000 => 9,
        2_500_001..=5_000_000 => 8,
        5_000_001..=12_500_000 => 7,
        12_500_001..=25_000_000 => 6,
        25_000_001..=62_500_000 => 5,
        62_500_001..=125_000_000 => 4,
        125_000_001..=312_500_000 => 3,
        312_500_001..=625_000_000 => 2,
        _ => 1,
    }
}

/// Generates a list of unique `TileCoord`s that cover a given geographical extent
/// at a specific zoom level.
///
/// # Arguments
/// * `extent` - The geographical bounding box to cover.
/// * `zoom` - The desired zoom level for the tiles.
///
/// # Returns
/// A `Result` containing a `Vec<TileCoord>` on success, or an error.
pub fn get_tile_coordinates_for_extent(
    extent: &Extent,
    zoom: u32,
) -> Result<Vec<TileCoord>, String> {
    let min_tile = lat_lng_to_tile(extent.south as f64, extent.west as f64, zoom);
    let max_tile = lat_lng_to_tile(extent.north as f64, extent.east as f64, zoom);

    // The Y-axis in tile systems typically goes from north (0) to south (2^z - 1).
    // So, for a bounding box, the 'north' latitude will correspond to a smaller Y-tile index,
    // and 'south' latitude to a larger Y-tile index.
    // Ensure min_x/max_x and min_y/max_y correctly define the iteration range.
    let min_x = min_tile.x.min(max_tile.x);
    let max_x = min_tile.x.max(max_tile.x);
    let min_y = min_tile.y.min(max_tile.y);
    let max_y = min_tile.y.max(max_tile.y);

    let tiles = (min_x..=max_x)
        .flat_map(|x| (min_y..=max_y).map(move |y| TileCoord { z: zoom, x, y }))
        .collect();

    Ok(tiles)
}

/// Generates a list of URLs for a given set of `TileCoord`s and a base URL.
///
/// These URLs can then be used by an external downloader (e.g., `reqwest` in an async context,
/// or `fetch` in WASM/JS).
///
/// # Arguments
/// * `base_url` - The base URL of the tile service (e.g., "https://tiles.openfreemap.org/planet/20250528_001001_pt").
///                The format should be without the Z/X/Y path.
/// * `tile_coords` - A slice of `TileCoord`s for which to generate URLs.
///
/// # Returns
/// A `Vec<String>` containing the full URLs for each tile.
pub fn tile_coords_to_urls(
    base_url: &str,
    tile_coords: &[TileCoord],
) -> BTreeMap<TileCoord, String> {
    tile_coords
        .iter()
        .map(|coord| {
            (
                coord.clone(),
                format!("{}/{}/{}/{}.pbf", base_url, coord.z, coord.x, coord.y),
            )
        })
        .collect()
}

/// Parses raw Mapbox Vector Tile (MVT) PBF data into a vector of GeoJSON `Feature`s.
///
/// This function is typically called after downloading the tile data. It requires
/// the `TileCoord` for correct geometry transformation from tile-local coordinates
/// to global WGS84 (latitude/longitude).
///
/// # Arguments
/// * `tile_data` - A `Vec<u8>` containing the raw PBF data of the MVT.
/// * `tile_coord` - The `TileCoord` (Z/X/Y) of the tile being parsed.
///
/// # Returns
/// A `Result` containing a `Vec<Feature>` on success, or an error if the PBF data
/// is malformed or parsing fails.
///
/// # Errors
/// Returns an error if `mvt_reader` fails to parse the tile data or if unsupported
/// geometry types are encountered.
pub fn parse_mvt_tile(
    tile_data: Vec<u8>,
    tile_coord: &TileCoord,
) -> Result<Vec<Feature>, ParserError> {
    trace!("Parsing MVT tile at {:?}", tile_coord);
    let reader = TileReader::new(tile_data)?;
    let mut features = Vec::new();

    let layers = reader.get_layer_metadata().unwrap_or_default();
    trace!("Found {} layers in tile {:?}", layers.len(), tile_coord);
    let dbg = std::env::var("AZ_MAP_DEBUG").is_ok();
    let layer_count = layers.len();
    let mut raw_count = 0usize;

    for layer in layers {
        trace!(
            "Processing layer '{}' (index {})",
            layer.name, layer.layer_index
        );
        let tile_features = reader.get_features(layer.layer_index).unwrap_or_default();
        trace!(
            "Layer '{}' has {} features.",
            layer.name,
            tile_features.len()
        );
        raw_count += tile_features.len();

        for mvt_feature in tile_features {
            // Replaced `if let` with `match` to log errors explicitly.
            match convert_mvt_feature_to_geojson(&mvt_feature, tile_coord) {
                Ok(mut geojson_feature) => {
                    trace!("Successfully converted MVT feature to GeoJSON feature.");
                    // GeoJSON has no notion of a "layer", but the SVG styler keys
                    // its palette on the MVT layer name (water → blue, building →
                    // tan, …). Stash it as a property so `features_to_svg` can read
                    // `feature.property("layer")` — without this every feature fell
                    // through to the grey default and the whole tile rendered grey.
                    if let Some(props) = geojson_feature.properties.as_mut() {
                        props.insert(
                            "layer".to_string(),
                            JsonValue::String(layer.name.clone()),
                        );
                    }
                    features.push(geojson_feature);
                }
                Err(e) => {
                    warn!(
                        "Failed to convert MVT feature in tile {:?} (layer '{}'): {}. Skipping feature.",
                        tile_coord, layer.name, e
                    );
                }
            }
        }
    }

    trace!(
        "Finished parsing tile {:?}. Extracted {} GeoJSON features.",
        tile_coord,
        features.len()
    );
    if dbg {
        eprintln!(
            "[map] parse tile=({},{},{}) layers={} raw_features={} converted={}",
            tile_coord.z, tile_coord.x, tile_coord.y,
            layer_count, raw_count, features.len()
        );
    }
    Ok(features)
}

/// Combines a list of GeoJSON `Feature`s into a single `FeatureCollection`.
///
/// This is useful after parsing multiple tiles to create a unified GeoJSON object.
///
/// # Arguments
/// * `features` - A `Vec<Feature>` containing all the GeoJSON features to combine.
///
/// # Returns
/// A `FeatureCollection` containing all the provided features.
pub fn stitch_features_to_collection(features: Vec<Feature>) -> FeatureCollection {
    FeatureCollection {
        bbox: None, // Bounding box can be calculated if needed, but often left optional.
        features,
        foreign_members: None,
    }
}

// --- Internal Helper Functions ---

/// Gets the appropriate `proj4rs` projection string for extent calculation based on
/// the desired projection type and center coordinates.
fn get_target_proj_str_for_extent_calc(longitude: f64, latitude: f64, projection: &str) -> String {
    match projection {
        "utm" => {
            let zone = get_utm_zone(longitude);
            let south = if latitude < 0.0 { " +south" } else { "" };
            format!(
                "+proj=utm +zone={}{} +datum=WGS84 +units=m +no_defs +type=crs",
                zone, south
            )
        }
        // Default to a Mercator projection centered on the given longitude
        _ => format!(
            "+proj=merc +lat_0=0 +lon_0={} +k=1 +x_0=0 +y_0=0 +datum=WGS84 +units=m +no_defs +type=crs",
            longitude
        ),
    }
}

/// Calculates the width and height of the map in meters based on scale and mm dimensions.
fn get_map_dimensions_m(scale: u32, width_mm: u32, height_mm: u32) -> (f64, f64) {
    let width_m = width_mm as f64 * scale as f64 / 1000.0;
    let height_m = height_mm as f64 * scale as f64 / 1000.0;
    (width_m, height_m)
}

/// Calculates the UTM zone for a given longitude.
fn get_utm_zone(longitude: f64) -> u32 {
    ((longitude + 180.0) / 6.0).floor() as u32 + 1
}

/// Converts a latitude/longitude pair to a tile coordinate (Z/X/Y) at a given zoom level.
pub fn lat_lng_to_tile(lat: f64, lng: f64, zoom: u32) -> TileCoord {
    let lat_rad = lat.to_radians();
    let n = 2_u32.pow(zoom) as f64;

    let x = ((lng + 180.0) / 360.0 * n).floor() as u32;
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor() as u32;

    TileCoord { z: zoom, x, y }
}

/// Converts a MVT feature to a GeoJSON feature, performing coordinate transformation.
fn convert_mvt_feature_to_geojson(
    mvt_feature: &MvtFeature,
    coord: &TileCoord,
) -> Result<Feature, String> {
    trace!(
        "Attempting to convert MVT feature with ID: {:?}",
        mvt_feature.id
    );
    if let Some(props) = &mvt_feature.properties {
        trace!("Feature properties: {:?}", props);
    }

    let geometry = convert_mvt_geometry_to_geojson(mvt_feature.get_geometry(), coord)?;

    let mut properties = serde_json::Map::new();
    if let Some(props) = &mvt_feature.properties {
        for (key, value) in props {
            properties.insert(key.clone(), translate_mvt_value_to_json(value));
        }
    }

    Ok(Feature {
        bbox: None,
        geometry: Some(geometry),
        id: mvt_feature
            .id
            .map(|id| geojson::feature::Id::Number(id.into())),
        properties: Some(properties),
        foreign_members: None,
    })
}

/// Translates an `mvt_reader::feature::Value` to a `serde_json::Value`.
fn translate_mvt_value_to_json(m: &mvt_reader::feature::Value) -> JsonValue {
    use mvt_reader::feature::Value;
    match m {
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Float(s) => JsonValue::Number(Number::from_f64(*s as f64).unwrap()),
        Value::Double(s) => JsonValue::Number(Number::from_f64(*s).unwrap()),
        Value::Int(s) => JsonValue::Number(Number::from_i128((*s).into()).unwrap()),
        Value::UInt(s) => JsonValue::Number(Number::from_u128((*s).into()).unwrap()),
        Value::SInt(s) => JsonValue::Number(Number::from_i128((*s).into()).unwrap()),
        Value::Bool(s) => JsonValue::Bool(*s),
        Value::Null => JsonValue::Null,
    }
}

/// Converts a `geo_types::Geometry` (from MVT) to a `geojson::Geometry`,
/// applying tile coordinate transformations.
fn convert_mvt_geometry_to_geojson(
    geometry: &Geometry<f32>,
    tile_coord: &TileCoord,
) -> Result<geojson::Geometry, String> {
    use geo_types::Geometry as GeoGeometry;

    let geometry_type_name = match geometry {
        GeoGeometry::Point(_) => "Point",
        GeoGeometry::LineString(_) => "LineString",
        GeoGeometry::Polygon(_) => "Polygon",
        GeoGeometry::MultiPoint(_) => "MultiPoint",
        GeoGeometry::MultiLineString(_) => "MultiLineString",
        GeoGeometry::MultiPolygon(_) => "MultiPolygon",
        _ => "Other/Unsupported",
    };
    trace!("Converting MVT geometry of type: {}", geometry_type_name);

    let geom = match geometry {
        GeoGeometry::Point(point) => {
            let (lng, lat) = tile_pixel_to_lat_lng(
                point.x(),
                point.y(),
                tile_coord.z,
                tile_coord.x,
                tile_coord.y,
            );
            geojson::Geometry::new(geojson::Value::Point(vec![lng.into(), lat.into()]))
        }
        GeoGeometry::LineString(line) => {
            let coords = translate_linestring_for_tile(line, tile_coord);
            geojson::Geometry::new(geojson::Value::LineString(coords))
        }
        GeoGeometry::Polygon(polygon) => {
            let exterior = translate_linestring_for_tile(polygon.exterior(), tile_coord);
            let holes = polygon
                .interiors()
                .iter()
                .map(|l| translate_linestring_for_tile(l, tile_coord))
                .collect::<Vec<_>>();
            let mut rings = vec![exterior];
            rings.extend(holes);
            geojson::Geometry::new(geojson::Value::Polygon(rings))
        }
        // MVT features at low zoom are overwhelmingly Multi* (merged continental
        // water/landuse → MultiPolygon, boundary networks → MultiLineString).
        // Without these arms every such feature was rejected as "unsupported",
        // which is why a z2 planet tile decoded to ~0 GeoJSON features.
        GeoGeometry::MultiPoint(points) => {
            let coords = points
                .iter()
                .map(|point| {
                    let (lng, lat) = tile_pixel_to_lat_lng(
                        point.x(),
                        point.y(),
                        tile_coord.z,
                        tile_coord.x,
                        tile_coord.y,
                    );
                    vec![lng.into(), lat.into()]
                })
                .collect::<Vec<_>>();
            geojson::Geometry::new(geojson::Value::MultiPoint(coords))
        }
        GeoGeometry::MultiLineString(lines) => {
            let lines = lines
                .iter()
                .map(|line| translate_linestring_for_tile(line, tile_coord))
                .collect::<Vec<_>>();
            geojson::Geometry::new(geojson::Value::MultiLineString(lines))
        }
        GeoGeometry::MultiPolygon(polygons) => {
            let polys = polygons
                .iter()
                .map(|polygon| {
                    let exterior =
                        translate_linestring_for_tile(polygon.exterior(), tile_coord);
                    let holes = polygon
                        .interiors()
                        .iter()
                        .map(|l| translate_linestring_for_tile(l, tile_coord))
                        .collect::<Vec<_>>();
                    let mut rings = vec![exterior];
                    rings.extend(holes);
                    rings
                })
                .collect::<Vec<_>>();
            geojson::Geometry::new(geojson::Value::MultiPolygon(polys))
        }
        unsupported_geom => {
            warn!(
                "Unsupported MVT geometry type encountered in tile {:?}: {:?}",
                tile_coord, unsupported_geom
            );
            return Err(format!("Unsupported geometry type: {:?}", unsupported_geom));
        }
    };

    Ok(geom)
}

/// Converts a `geo_types::LineString` (from MVT) to a GeoJSON coordinate list,
/// applying tile coordinate transformations.
fn translate_linestring_for_tile(
    l: &geo_types::LineString<f32>,
    tile_coord: &TileCoord,
) -> Vec<Vec<f64>> {
    l.coords()
        .map(|coord_in_tile| {
            let (lng, lat) = tile_pixel_to_lat_lng(
                coord_in_tile.x,
                coord_in_tile.y,
                tile_coord.z,
                tile_coord.x,
                tile_coord.y,
            );
            vec![lng, lat]
        })
        .collect()
}

/// Converts pixel coordinates within an MVT tile to WGS84 latitude and longitude.
///
/// MVT geometries are typically encoded using a 0-4096 (or 0-256 for older specs)
/// grid within each tile. This function converts those internal pixel coordinates
/// to global WGS84 coordinates.
///
/// # Arguments
/// * `tile_x_pixel` - X coordinate within the MVT tile (e.g., 0-4095).
/// * `tile_y_pixel` - Y coordinate within the MVT tile (e.g., 0-4095).
/// * `z` - Global zoom level of the tile.
/// * `tile_global_x` - Global X index of the tile.
/// * `tile_global_y` - Global Y index of the tile.
///
/// # Returns
/// A tuple `(longitude, latitude)` in degrees.
pub fn tile_pixel_to_lat_lng(
    tile_x_pixel: f32,
    tile_y_pixel: f32,
    z: u32,
    tile_global_x: u32,
    tile_global_y: u32,
) -> (f64, f64) {
    // Compute in f64, NOT f32: the global pixel coordinate reaches
    // `2^z * extent_size` (≈ 6.7e7 at z14 with a 4096 extent), which is far past
    // f32's exact-integer ceiling (2^24 ≈ 1.6e7). In f32 the per-pixel `+tile_*_pixel`
    // term and the tile-boundary multiples round to a coarse grid, so a feature's
    // vertices snap inconsistently and adjacent tiles' shared edges stop lining up —
    // coastlines/borders fracture across seams and the map reads as a disconnected
    // jumble at street zooms. f64 keeps every coordinate exact across the whole
    // zoom range. (Returns f64 so the downstream SVG projection, which is already
    // f64, doesn't re-truncate.)
    let n = 2_f64.powi(z as i32);
    let extent_size = 4096.0_f64; // Default MVT extent, adjust if your tiles use a different one

    // Calculate global pixel coordinates (from 0 to (n * extent_size) - 1)
    let global_pixel_x = (tile_global_x as f64 * extent_size) + tile_x_pixel as f64;
    let global_pixel_y = (tile_global_y as f64 * extent_size) + tile_y_pixel as f64;

    // Normalize coordinates to a 0.0-1.0 range (mercator projection space)
    let x_norm = global_pixel_x / (n * extent_size);
    let y_norm = global_pixel_y / (n * extent_size);

    // Convert normalized mercator coordinates to WGS84 lat/lng
    let lng = x_norm * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y_norm)).sinh().atan();
    let lat = lat_rad.to_degrees();

    (lng, lat)
}

/// Caches or creates a `proj4rs::Proj` object from a PROJ.4 string.
/// In a real application, consider a proper global cache (e.g., using `once_cell::sync::Lazy`)
/// if this function is called many times with the same string.
fn get_cached_proj(proj_str: &str) -> Result<Proj, String> {
    Ok(Proj::from_user_string(proj_str).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod geometry_conversion_tests {
    //! REGRESSION coverage for the MVT geometry converter. Low-zoom MVT tiles
    //! are overwhelmingly Multi* (merged continental water/landuse →
    //! `MultiPolygon`, boundary networks → `MultiLineString`). Those arms were
    //! missing, so `convert_mvt_geometry_to_geojson` returned `Err(unsupported)`
    //! for them and a z2 planet tile decoded to ~0 GeoJSON features (the map
    //! rendered blank). These tests pin every geometry kind to a successful,
    //! correctly-shaped GeoJSON conversion.
    use super::*;
    use geo_types::{
        Coord, Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
    };

    fn tc() -> TileCoord {
        TileCoord { z: 2, x: 1, y: 1 }
    }

    /// A closed 5-point square ring in tile-local pixel space.
    fn square_ring() -> LineString<f32> {
        LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 100.0, y: 0.0 },
            Coord { x: 100.0, y: 100.0 },
            Coord { x: 0.0, y: 100.0 },
            Coord { x: 0.0, y: 0.0 },
        ])
    }

    #[test]
    fn multipolygon_is_converted_not_rejected() {
        let geom =
            Geometry::MultiPolygon(MultiPolygon(vec![Polygon::new(square_ring(), vec![])]));
        let out =
            convert_mvt_geometry_to_geojson(&geom, &tc()).expect("MultiPolygon must convert");
        match out.value {
            geojson::Value::MultiPolygon(polys) => {
                assert_eq!(polys.len(), 1, "one polygon");
                assert_eq!(polys[0].len(), 1, "one ring (exterior, no holes)");
                assert_eq!(polys[0][0].len(), 5, "five coords in the ring");
            }
            other => panic!("expected MultiPolygon, got {other:?}"),
        }
    }

    #[test]
    fn multilinestring_is_converted_not_rejected() {
        let geom = Geometry::MultiLineString(MultiLineString(vec![square_ring()]));
        let out = convert_mvt_geometry_to_geojson(&geom, &tc())
            .expect("MultiLineString must convert");
        match out.value {
            geojson::Value::MultiLineString(lines) => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].len(), 5);
            }
            other => panic!("expected MultiLineString, got {other:?}"),
        }
    }

    #[test]
    fn multipoint_is_converted_not_rejected() {
        let geom = Geometry::MultiPoint(MultiPoint(vec![
            Point::new(10.0f32, 20.0),
            Point::new(30.0, 40.0),
        ]));
        let out =
            convert_mvt_geometry_to_geojson(&geom, &tc()).expect("MultiPoint must convert");
        match out.value {
            geojson::Value::MultiPoint(pts) => assert_eq!(pts.len(), 2),
            other => panic!("expected MultiPoint, got {other:?}"),
        }
    }

    #[test]
    fn single_geometries_still_convert() {
        // Guard against a regression that breaks the original single-geometry arms.
        let poly = Geometry::Polygon(Polygon::new(square_ring(), vec![]));
        assert!(matches!(
            convert_mvt_geometry_to_geojson(&poly, &tc()).unwrap().value,
            geojson::Value::Polygon(_)
        ));
        let line = Geometry::LineString(square_ring());
        assert!(matches!(
            convert_mvt_geometry_to_geojson(&line, &tc()).unwrap().value,
            geojson::Value::LineString(_)
        ));
        let point = Geometry::Point(Point::new(50.0f32, 50.0));
        assert!(matches!(
            convert_mvt_geometry_to_geojson(&point, &tc()).unwrap().value,
            geojson::Value::Point(_)
        ));
    }
}
