//! AzMaps' model without azul types: the viewport kept across restarts, the
//! pins file in the data tree, the stdout lines for scripts. Tested without
//! a window.

/// The pins file, in the app's folder of the data tree (`maps/pins.json`):
/// a JSON list of `[latitude, longitude]` pairs, oldest first.
pub const PINS_FILE: &str = "pins.json";

/// The settings key of the last viewport (`lat,lon,zoom`).
pub const VIEW_KEY: &str = "view";

/// Where the map opens without a remembered viewport (San Francisco).
pub const HOME: (f64, f64, f32) = (37.7749, -122.4194, 2.0);

/// The zoom range of the tile layer.
pub const MIN_ZOOM: f32 = 0.0;
pub const MAX_ZOOM: f32 = 19.0;

/// The pins as the pins file keeps them.
#[must_use]
pub fn pins_to_json(pins: &[(f64, f64)]) -> String {
    let _ = pins;
    todo!()
}

/// The pins of a pins file; a broken file is an error, never a panic.
///
/// # Errors
/// What is wrong with the file.
pub fn pins_from_json(text: &str) -> Result<Vec<(f64, f64)>, String> {
    let _ = text;
    todo!()
}

/// The viewport as settings.json keeps it: `lat,lon,zoom`.
#[must_use]
pub fn view_value(lat: f64, lon: f64, zoom: f32) -> String {
    let _ = (lat, lon, zoom);
    todo!()
}

/// A kept viewport read back, clamped into the map (`None`: unreadable).
#[must_use]
pub fn parse_view(text: &str) -> Option<(f64, f64, f32)> {
    let _ = text;
    todo!()
}

/// `AZMAPS_VIEW <lat> <lon> <zoom>`, four decimals and one, for scripts.
#[must_use]
pub fn view_line(lat: f64, lon: f64, zoom: f32) -> String {
    format!("AZMAPS_VIEW {lat:.4} {lon:.4} {zoom:.1}")
}

/// The compass point of a heading in degrees.
#[must_use]
pub fn cardinal(deg: f32) -> &'static str {
    const DIRS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    DIRS[((((deg + 22.5) % 360.0) / 45.0) as usize) % 8]
}

/// Pans the centre by whole tiles at `tile_count` tiles per axis: `dx` east,
/// `dy` south; the longitude wraps, the latitude stops at the Mercator edge.
#[must_use]
pub fn pan_tiles(lon_deg: f64, lat_deg: f64, tile_count: f64, dx_tiles: f64, dy_tiles: f64) -> (f64, f64) {
    use std::f64::consts::PI;
    let x = (lon_deg + 180.0) / 360.0 * tile_count + dx_tiles;
    let lon = ((x / tile_count * 360.0 - 180.0) + 540.0).rem_euclid(360.0) - 180.0;
    let lat_rad = lat_deg.to_radians();
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * tile_count;
    let y = (y + dy_tiles).clamp(0.0, tile_count);
    let lat = (PI * (1.0 - 2.0 * y / tile_count)).sinh().atan().to_degrees();
    (lon, lat.clamp(-85.0, 85.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_goes_north_in_both_hemispheres() {
        for lat in [37.7749, -33.8688, 0.0] {
            let (_, north) = pan_tiles(0.0, lat, 4.0, 0.0, -0.5);
            let (_, south) = pan_tiles(0.0, lat, 4.0, 0.0, 0.5);
            assert!(north > lat, "up must go north: {lat} -> {north}");
            assert!(south < lat, "down must go south: {lat} -> {south}");
        }
    }

    #[test]
    fn steps_are_exact_in_tile_space_and_east_is_positive() {
        let (lon, lat) = pan_tiles(0.0, 0.0, 4.0, 0.5, 0.0);
        assert!((lon - 45.0).abs() < 1e-9, "{lon}");
        assert!(lat.abs() < 1e-9, "{lat}");
        let (_, lat) = pan_tiles(0.0, 0.0, 2.0, 0.0, 1.0);
        assert!((lat - -85.0).abs() < 1e-9, "{lat}");
        let (lon, _) = pan_tiles(179.0, 0.0, 4.0, 0.5, 0.0);
        assert!((lon - -136.0).abs() < 1e-9, "{lon}");
    }

    #[test]
    fn the_pins_survive_the_trip_through_the_pins_file() {
        let pins = vec![(37.7749, -122.4194), (-33.8688, 151.2093)];
        let text = pins_to_json(&pins);
        assert_eq!(pins_from_json(&text).expect("read back"), pins);
        assert_eq!(pins_from_json("[]").expect("empty"), Vec::<(f64, f64)>::new());
        assert!(pins_from_json("{not json").is_err());
        assert!(pins_from_json("[[91.0, 0.0]]").is_err(), "a latitude past the pole");
    }

    #[test]
    fn the_viewport_is_kept_and_read_back_inside_the_map() {
        let v = view_value(48.2082, 16.3738, 11.5);
        assert_eq!(parse_view(&v), Some((48.2082, 16.3738, 11.5)));
        assert_eq!(parse_view("10,20,99"), Some((10.0, 20.0, MAX_ZOOM)), "the zoom is clamped");
        assert_eq!(parse_view("89,0,3"), Some((85.0, 0.0, 3.0)), "the Mercator edge");
        assert_eq!(parse_view("nonsense"), None);
        assert_eq!(parse_view("1,2"), None);
        assert_eq!(view_line(37.77491, -122.41942, 2.0), "AZMAPS_VIEW 37.7749 -122.4194 2.0");
        assert_eq!(cardinal(0.0), "N");
        assert_eq!(cardinal(225.0), "SW");
    }
}
