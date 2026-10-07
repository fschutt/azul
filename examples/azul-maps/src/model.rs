//! AzMaps' model without azul types: the viewport kept across restarts, the
//! pins file in the data tree, the travel panel's places and distances, the
//! stdout lines for scripts. Tested without a window.

/// The pins file, in the app's folder of the data tree (`maps/pins.json`):
/// a JSON list of `[latitude, longitude]` pairs, oldest first.
pub const PINS_FILE: &str = "pins.json";

/// The settings key of the last viewport (`lat,lon,zoom`).
pub const VIEW_KEY: &str = "view";

/// The settings key of the sidebar (`open` / `closed`).
pub const SIDEBAR_KEY: &str = "sidebar";

/// Where the map opens without a remembered viewport (San Francisco).
pub const HOME: (f64, f64, f32) = (37.7749, -122.4194, 2.0);

/// The zoom range of the tile layer.
pub const MIN_ZOOM: f32 = 0.0;
pub const MAX_ZOOM: f32 = 19.0;

/// The pins as the pins file keeps them.
#[must_use]
pub fn pins_to_json(pins: &[(f64, f64)]) -> String {
    let pairs: Vec<[f64; 2]> = pins.iter().map(|(lat, lon)| [*lat, *lon]).collect();
    serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".to_string())
}

/// The pins of a pins file; a broken file is an error, never a panic.
///
/// # Errors
/// What is wrong with the file.
pub fn pins_from_json(text: &str) -> Result<Vec<(f64, f64)>, String> {
    let pairs: Vec<[f64; 2]> =
        serde_json::from_str(text).map_err(|e| format!("not a pins file: {e}"))?;
    pairs
        .into_iter()
        .map(|[lat, lon]| {
            if lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0 {
                Ok((lat, lon))
            } else {
                Err(format!("{lat}, {lon} is not a place on the map"))
            }
        })
        .collect()
}

/// The viewport as settings.json keeps it: `lat,lon,zoom`.
#[must_use]
pub fn view_value(lat: f64, lon: f64, zoom: f32) -> String {
    format!("{lat},{lon},{zoom}")
}

/// A kept viewport read back, clamped into the map (`None`: unreadable).
#[must_use]
pub fn parse_view(text: &str) -> Option<(f64, f64, f32)> {
    let mut parts = text.split(',').map(str::trim);
    let lat: f64 = parts.next()?.parse().ok()?;
    let lon: f64 = parts.next()?.parse().ok()?;
    let zoom: f32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(lat.is_finite() && lon.is_finite() && zoom.is_finite()) {
        return None;
    }
    // A longitude on the map stays as written (no float round trip); one
    // past it wraps around.
    let lon = if (-180.0..=180.0).contains(&lon) {
        lon
    } else {
        (lon + 540.0).rem_euclid(360.0) - 180.0
    };
    Some((lat.clamp(-85.0, 85.0), lon, zoom.clamp(MIN_ZOOM, MAX_ZOOM)))
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

/// How the traveller goes (the travel panel's options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TravelMode {
    #[default]
    Car,
    Walk,
    Bike,
    Transit,
}

impl TravelMode {
    pub const ALL: [Self; 4] = [Self::Car, Self::Walk, Self::Bike, Self::Transit];

    /// The mode's word in ids and stdout lines.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Walk => "walk",
            Self::Bike => "bike",
            Self::Transit => "transit",
        }
    }

    /// The Material icon of the mode's button.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Car => "directions_car",
            Self::Walk => "directions_walk",
            Self::Bike => "directions_bike",
            Self::Transit => "directions_transit",
        }
    }

    /// The button's accessible name (it shows no text).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Car => "Drive",
            Self::Walk => "Walk",
            Self::Bike => "Cycle",
            Self::Transit => "Public transport",
        }
    }
}

/// A place in a travel field: `lat, lon` (`37.7749, -122.4194`, the comma
/// optional) or the way the app writes a place (`37.7749° N 122.4194° W`).
/// `None` for anything else - there is no geocoder yet.
#[must_use]
pub fn parse_place(text: &str) -> Option<(f64, f64)> {
    let cleaned: String = text
        .chars()
        .map(|c| if c == ',' || c == ';' || c == '\u{b0}' { ' ' } else { c })
        .collect();
    let mut numbers = Vec::new();
    let mut tokens = cleaned.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        let mut value: f64 = token.parse().ok()?;
        let direction = tokens.peek().map(|t| t.to_ascii_uppercase());
        match direction.as_deref() {
            Some("N" | "E") => {
                tokens.next();
            }
            Some("S" | "W") => {
                value = -value;
                tokens.next();
            }
            _ => {}
        }
        numbers.push(value);
    }
    let (lat, lon) = match numbers.as_slice() {
        [lat, lon] => (*lat, *lon),
        _ => return None,
    };
    (lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0)
        .then_some((lat, lon))
}

/// A place as the travel fields hold it: `37.77490, -122.41940`.
#[must_use]
pub fn place_text(lat: f64, lon: f64) -> String {
    format!("{lat:.5}, {lon:.5}")
}

/// The great-circle distance between two places, in km.
#[must_use]
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// A distance the way the travel panel shows it: `850 m`, `4.2 km`, `412 km`.
#[must_use]
pub fn distance_text(km: f64) -> String {
    if km < 1.0 {
        format!("{:.0} m", km * 1000.0)
    } else if km < 10.0 {
        format!("{km:.1} km")
    } else {
        format!("{km:.0} km")
    }
}

/// `AZMAPS_TRAVEL <mode> <from> <to>` for scripts: each end `lat,lon` to
/// four decimals, `-` while it is not a place.
#[must_use]
pub fn travel_line(mode: TravelMode, from: Option<(f64, f64)>, to: Option<(f64, f64)>) -> String {
    let end = |p: Option<(f64, f64)>| match p {
        Some((lat, lon)) => format!("{lat:.4},{lon:.4}"),
        None => "-".to_string(),
    };
    format!("AZMAPS_TRAVEL {} {} {}", mode.key(), end(from), end(to))
}

/// Whether a mark at `p` (view pixels) shows in a `width` x `height` view.
#[must_use]
pub fn mark_visible(p: (f32, f32), width: f32, height: f32) -> bool {
    let _ = (p, width, height);
    true
}

/// Whether the window draws anything at a place on the map at the view
/// `project` stands for.
#[must_use]
pub fn overlay_shows(
    project: impl Fn(f64, f64) -> (f32, f32),
    size: (f32, f32),
    pins: &[(f64, f64)],
    travel: Option<((f64, f64), (f64, f64))>,
    here: Option<(f64, f64)>,
) -> bool {
    let _ = (project, size, pins, travel, here);
    true
}

/// How much longer a way is than the crow flies.
pub const DETOUR_FACTOR: f64 = 1.3;

impl TravelMode {
    /// A typical door-to-door speed, km/h.
    #[must_use]
    pub const fn speed_kmh(self) -> f64 {
        let _ = self;
        1.0
    }
}

/// A route's estimate: how long the way is and how long it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteEstimate {
    pub km: f64,
    pub minutes: f64,
}

/// The straight-line estimate of a route.
#[must_use]
pub fn estimate_route(from: (f64, f64), to: (f64, f64), mode: TravelMode) -> RouteEstimate {
    let _ = (from, to, mode);
    todo!()
}

/// A travel time the way the panel shows it.
#[must_use]
pub fn duration_text(minutes: f64) -> String {
    let _ = minutes;
    todo!()
}

/// `AZMAPS_ROUTE <mode> <km> <minutes> <compute_ms>` for scripts.
#[must_use]
pub fn route_line(mode: TravelMode, route: RouteEstimate, compute_ms: f64) -> String {
    let _ = (mode, route, compute_ms);
    todo!()
}

/// The part of the segment `a`-`b` inside the `width` x `height` view
/// (Liang-Barsky), `None` when it misses the view: a line to a place a
/// continent away is drawn as long as the window, not a million pixels.
#[must_use]
pub fn clip_segment(
    a: (f32, f32),
    b: (f32, f32),
    width: f32,
    height: f32,
) -> Option<((f32, f32), (f32, f32))> {
    if ![a.0, a.1, b.0, b.1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    for (p, q) in [(-dx, a.0), (dx, width - a.0), (-dy, a.1), (dy, height - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then(|| {
        (
            (a.0 + t0 * dx, a.1 + t0 * dy),
            (a.0 + t1 * dx, a.1 + t1 * dy),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_read_as_typed_or_as_the_app_writes_it() {
        assert_eq!(parse_place("37.7749, -122.4194"), Some((37.7749, -122.4194)));
        assert_eq!(parse_place(" 48.2 16.37 "), Some((48.2, 16.37)));
        assert_eq!(
            parse_place("37.7749\u{b0} N 122.4194\u{b0} W"),
            Some((37.7749, -122.4194))
        );
        assert_eq!(parse_place("33.8688 s, 151.2093 e"), Some((-33.8688, 151.2093)));
        let (lat, lon) = parse_place(&place_text(52.52, 13.405)).expect("round trip");
        assert!((lat - 52.52).abs() < 1e-9 && (lon - 13.405).abs() < 1e-9);
        assert_eq!(parse_place("Berlin"), None, "no geocoder yet");
        assert_eq!(parse_place("91, 0"), None, "past the pole");
        assert_eq!(parse_place("1, 2, 3"), None);
        assert_eq!(parse_place(""), None);
    }

    #[test]
    fn distances_are_great_circles_and_read_short() {
        let berlin = (52.52, 13.405);
        let munich = (48.1351, 11.582);
        let km = distance_km(berlin, munich);
        assert!((km - 504.0).abs() < 5.0, "{km}");
        assert!(distance_km(berlin, berlin).abs() < 1e-9);
        assert_eq!(distance_text(0.85), "850 m");
        assert_eq!(distance_text(4.24), "4.2 km");
        assert_eq!(distance_text(412.4), "412 km");
        assert_eq!(
            travel_line(TravelMode::Transit, Some(berlin), None),
            "AZMAPS_TRAVEL transit 52.5200,13.4050 -"
        );
        assert_eq!(TravelMode::default(), TravelMode::Car);
    }

    #[test]
    fn a_route_line_is_clipped_to_the_view() {
        // Inside: unchanged.
        assert_eq!(
            clip_segment((10.0, 10.0), (90.0, 50.0), 100.0, 100.0),
            Some(((10.0, 10.0), (90.0, 50.0)))
        );
        // Crossing: cut at the edges.
        let ((x0, y0), (x1, y1)) =
            clip_segment((-100.0, 50.0), (300.0, 50.0), 100.0, 100.0).expect("crosses");
        assert!((x0 - 0.0).abs() < 1e-4 && (x1 - 100.0).abs() < 1e-4, "{x0} {x1}");
        assert!((y0 - 50.0).abs() < 1e-4 && (y1 - 50.0).abs() < 1e-4);
        // Missing the view, or not a number: nothing.
        assert_eq!(clip_segment((-50.0, -50.0), (-10.0, 200.0), 100.0, 100.0), None);
        assert_eq!(clip_segment((f32::NAN, 0.0), (10.0, 10.0), 100.0, 100.0), None);
    }

    #[test]
    fn a_pan_moves_the_window_only_while_something_of_ours_is_on_the_map() {
        // The map moves its tiles itself; the window is rebuilt for a pan only
        // to move what IT draws at a place. A flat stand-in for the
        // projection: 10 px per degree, (0, 0) in the middle of 800 x 600.
        let project = |lat: f64, lon: f64| (400.0 + lon as f32 * 10.0, 300.0 - lat as f32 * 10.0);
        let size = (800.0, 600.0);
        assert!(!overlay_shows(project, size, &[], None, None), "an empty map moves itself");
        assert!(overlay_shows(project, size, &[(0.0, 0.0)], None, None), "a pin in view");
        assert!(
            !overlay_shows(project, size, &[(80.0, 170.0)], None, None),
            "a pin far outside the view does not count"
        );
        assert!(
            overlay_shows(project, size, &[], Some(((0.0, -100.0), (0.0, 100.0))), None),
            "the travel line crossing the view counts, both ends outside"
        );
        assert!(!overlay_shows(project, size, &[], Some(((80.0, 170.0), (85.0, 175.0))), None));
        assert!(overlay_shows(project, size, &[], None, Some((1.0, 1.0))), "where you are");
        assert!(
            mark_visible((-30.0, 10.0), 800.0, 600.0) && !mark_visible((-50.0, 10.0), 800.0, 600.0),
            "a pin's head reaches 40 px past its point"
        );
        assert!(mark_visible((10.0, 650.0), 800.0, 600.0), "and 60 px below it");
    }

    #[test]
    fn a_route_is_first_estimated_from_the_crow_flies_distance_at_the_modes_speed() {
        let vienna = (48.2082, 16.3738);
        let munich = (48.1372, 11.5756);
        let crow = distance_km(vienna, munich);
        let car = estimate_route(vienna, munich, TravelMode::Car);
        assert!((car.km - crow * DETOUR_FACTOR).abs() < 1e-9, "{car:?}");
        assert!((car.minutes - car.km / TravelMode::Car.speed_kmh() * 60.0).abs() < 1e-9);
        let walk = estimate_route(vienna, munich, TravelMode::Walk);
        assert!(walk.minutes > car.minutes * 10.0, "walking takes far longer: {walk:?} {car:?}");
        assert_eq!(estimate_route(vienna, vienna, TravelMode::Bike).minutes, 0.0);
        assert_eq!(duration_text(0.4), "~ 1 min");
        assert_eq!(duration_text(25.2), "~ 25 min");
        assert_eq!(duration_text(310.0), "~ 5 h 10 min");
        assert_eq!(duration_text(120.0), "~ 2 h");
        assert_eq!(
            route_line(
                TravelMode::Walk,
                RouteEstimate {
                    km: 12.345,
                    minutes: 148.14
                },
                0.0123
            ),
            "AZMAPS_ROUTE walk 12.3 148 0.012"
        );
    }

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
