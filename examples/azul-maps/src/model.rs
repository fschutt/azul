//! AzMaps' model without azul types: the viewport kept across restarts, the
//! pins file in the data tree, the travel panel's places and distances, the
//! stdout lines for scripts. Tested without a window; its words are keys of AzMaps'
//! resources (`azul_appkit::l10n`).

use azul_appkit::l10n::{t_args, Arg};

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
        serde_json::from_str(text).map_err(|e| t_args("azmaps-not-pins-file", &[("why", Arg::from(e.to_string()))]))?;
    pairs
        .into_iter()
        .map(|[lat, lon]| {
            if lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0 {
                Ok((lat, lon))
            } else {
                Err(t_args(
                    "azmaps-not-a-place",
                    &[("lat", Arg::from(lat.to_string())), ("lon", Arg::from(lon.to_string()))],
                ))
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

/// A travel field: the start or the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    From,
    To,
}

impl End {
    /// The word scripts read (`AZMAPS_SUGGEST from ...`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
        }
    }
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

    /// The button's accessible name (it shows no text): a key of AzMaps' resources.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Car => "azmaps-mode-drive",
            Self::Walk => "azmaps-mode-walk",
            Self::Bike => "azmaps-mode-cycle",
            Self::Transit => "azmaps-mode-transit",
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

/// A distance the way the travel panel shows it: `850 m`, `4.2 km` (`4,2 km` in German),
/// `412 km`.
#[must_use]
pub fn distance_text(km: f64) -> String {
    if km < 1.0 {
        format!("{:.0} m", km * 1000.0)
    } else if km < 10.0 {
        format!("{} km", azul_appkit::l10n::decimal(&format!("{km:.1}")))
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

/// The marker ids of the travel line's ends and of where you are. A pin's
/// marker id is its index in the pins file's order, far below these.
pub const FROM_MARKER: u64 = u64::MAX - 2;
pub const TO_MARKER: u64 = u64::MAX - 1;
pub const HERE_MARKER: u64 = u64::MAX;

/// The pin a marker id names; `None` for the travel line's ends and for
/// where you are.
#[must_use]
pub fn pin_of_marker(id: u64) -> Option<usize> {
    if id >= FROM_MARKER {
        return None;
    }
    usize::try_from(id).ok()
}

/// The travel line on the map: the road route once the route worker found
/// one, else the straight line between the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct TravelLine {
    /// The line's places in order, `(lat, lon)`, from the start to the
    /// destination.
    pub points: Vec<(f64, f64)>,
    /// Whether it follows the roads (else it is the straight preview).
    pub road: bool,
}

/// A pin on the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinMark {
    /// The pin's index in the pins file's order: its marker id.
    pub index: usize,
    /// Its place, `(lat, lon)`.
    pub at: (f64, f64),
    /// Whether its card is open above it.
    pub open: bool,
}

/// What AzMaps puts on the map, in places, not pixels: the map widget draws
/// it and moves it with every pan, pinch, turn and tilt, so nothing here
/// depends on where the map looks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlays {
    pub line: Option<TravelLine>,
    /// The start's dot.
    pub from: Option<(f64, f64)>,
    /// The destination's dot.
    pub to: Option<(f64, f64)>,
    /// Where you are, while the location is shown and known.
    pub here: Option<(f64, f64)>,
    pub pins: Vec<PinMark>,
}

/// What the map shows of the pins (the one `selected` with its card open),
/// the travel panel's ends with their line - the `road` route between them
/// when there is one - and where you are.
#[must_use]
pub fn plan_overlays(
    pins: &[(f64, f64)],
    selected: Option<usize>,
    travel: Option<((f64, f64), (f64, f64))>,
    road: Option<&[(f64, f64)]>,
    here: Option<(f64, f64)>,
) -> Overlays {
    let (line, from, to) = match travel {
        Some((a, b)) => {
            let points = match road.filter(|r| r.len() >= 2) {
                // The road route starts and ends where the ends meet the
                // roads: the line joins it to their dots.
                Some(road) => {
                    let mut points = Vec::with_capacity(road.len() + 2);
                    points.push(a);
                    points.extend_from_slice(road);
                    points.push(b);
                    TravelLine { points, road: true }
                }
                None => TravelLine {
                    points: vec![a, b],
                    road: false,
                },
            };
            (Some(points), Some(a), Some(b))
        }
        None => (None, None, None),
    };
    Overlays {
        line,
        from,
        to,
        here,
        pins: pins
            .iter()
            .enumerate()
            .map(|(index, &at)| PinMark {
                index,
                at,
                open: selected == Some(index),
            })
            .collect(),
    }
}

/// How much longer a way by road or path is than the crow flies: ROUTING.md's
/// first step, the straight-line estimate.
pub const DETOUR_FACTOR: f64 = 1.3;

impl TravelMode {
    /// A typical door-to-door speed for a way of `km`, km/h: the estimate's divisor. A car
    /// crosses a town at town speed and a country on the motorway; a long trip by public
    /// transport is a train.
    #[must_use]
    pub fn speed_kmh(self, km: f64) -> f64 {
        match self {
            Self::Car if km < 15.0 => 35.0,
            Self::Car if km < 80.0 => 70.0,
            Self::Car => 95.0,
            Self::Walk => 5.0,
            Self::Bike => 16.0,
            Self::Transit if km < 30.0 => 25.0,
            Self::Transit if km < 150.0 => 60.0,
            Self::Transit => 110.0,
        }
    }
}

/// A route's estimate: how long the way is and how long it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteEstimate {
    pub km: f64,
    pub minutes: f64,
}

/// The straight-line estimate of a route (ROUTING.md, step 1): the
/// great-circle distance times [`DETOUR_FACTOR`], at the mode's speed. What
/// the route worker answers until the routing tiles and A* exist.
#[must_use]
pub fn estimate_route(from: (f64, f64), to: (f64, f64), mode: TravelMode) -> RouteEstimate {
    let km = distance_km(from, to) * DETOUR_FACTOR;
    RouteEstimate {
        km,
        minutes: km / mode.speed_kmh(km) * 60.0,
    }
}

/// Where `(lat, lon)` lies on the Web Mercator world at `zoom`, in pixels (256 per tile, as
/// the map draws it): `x` east from the antimeridian, `y` south from the top edge.
#[must_use]
pub fn mercator_px(lat: f64, lon: f64, zoom: f32) -> (f64, f64) {
    use std::f64::consts::PI;
    let world = 256.0 * 2f64.powf(f64::from(zoom));
    let lat_rad = lat.clamp(-85.0, 85.0).to_radians();
    let x = (lon + 180.0) / 360.0 * world;
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * world;
    (x, y)
}

/// The view that frames a route from `a` to `b` in a `size` window whose left `left_inset`
/// pixels the sidebar covers: both ends in the free part, a margin from its edges.
#[must_use]
pub fn fit_view(a: (f64, f64), b: (f64, f64), size: (f32, f32), left_inset: f32) -> (f64, f64, f32) {
    use std::f64::consts::PI;
    /// Space kept between an end and the edge of the free part.
    const MARGIN: f64 = 40.0;
    /// The deepest zoom a route is framed at: one place shows its town, not a street.
    const PLACE_ZOOM: f32 = 13.0;
    let (w, h, inset) = (f64::from(size.0), f64::from(size.1), f64::from(left_inset));
    let free_w = (w - inset - 2.0 * MARGIN).max(1.0);
    let free_h = (h - 2.0 * MARGIN).max(1.0);
    let (pa, pb) = (mercator_px(a.0, a.1, 0.0), mercator_px(b.0, b.1, 0.0));
    let (dx, dy) = ((pa.0 - pb.0).abs(), (pa.1 - pb.1).abs());
    let fit = (free_w / dx.max(1e-9)).min(free_h / dy.max(1e-9));
    let zoom = (fit.log2() as f32).clamp(MIN_ZOOM, PLACE_ZOOM);
    // The ends' midpoint goes to the middle of the free part: the window's centre lies half
    // the sidebar to the left of it.
    let scale = 2f64.powf(f64::from(zoom));
    let world = 256.0 * scale;
    let x = (pa.0 + pb.0) / 2.0 * scale - inset / 2.0;
    let y = (pa.1 + pb.1) / 2.0 * scale;
    let lon = (x / world * 360.0 - 180.0 + 540.0).rem_euclid(360.0) - 180.0;
    let lat = (PI * (1.0 - 2.0 * y / world)).sinh().atan().to_degrees();
    (lat.clamp(-85.0, 85.0), lon, zoom)
}

/// A travel time the way the panel shows it: `~ 25 min`, `~ 5 h 10 min`,
/// `~ 2 h`; at least a minute.
#[must_use]
pub fn duration_text(minutes: f64) -> String {
    let total = if minutes.is_finite() {
        (minutes.round() as u64).max(1)
    } else {
        1
    };
    let (h, m) = (total / 60, total % 60);
    let n = |v: u64| Arg::from(v.to_string());
    match (h, m) {
        (0, m) => t_args("azmaps-duration-min", &[("min", n(m))]),
        (h, 0) => t_args("azmaps-duration-h", &[("h", n(h))]),
        (h, m) => t_args("azmaps-duration-h-min", &[("h", n(h)), ("min", n(m))]),
    }
}

/// `AZMAPS_ROUTE <mode> <km> <minutes> <compute_ms>`: the route the travel
/// panel took, and how long the worker worked it out, for scripts.
#[must_use]
pub fn route_line(mode: TravelMode, route: RouteEstimate, compute_ms: f64) -> String {
    format!(
        "AZMAPS_ROUTE {} {:.1} {:.0} {:.3}",
        mode.key(),
        route.km,
        route.minutes,
        compute_ms
    )
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
        crate::l10n::in_english();
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
    fn the_map_shows_the_straight_line_until_the_road_route_arrives() {
        let (a, b) = ((48.2082, 16.3738), (48.1372, 11.5756));
        let straight = plan_overlays(&[], None, Some((a, b)), None, None);
        assert_eq!(
            straight.line,
            Some(TravelLine {
                points: vec![a, b],
                road: false
            })
        );
        assert_eq!((straight.from, straight.to), (Some(a), Some(b)));
        let road = [(48.2, 16.3), (48.0, 14.0), (48.14, 11.58)];
        let routed = plan_overlays(&[], None, Some((a, b)), Some(&road), None);
        let line = routed.line.expect("the travel line");
        assert!(line.road, "the road route replaces the preview");
        assert_eq!(line.points.first(), Some(&a), "it starts at the start's dot");
        assert_eq!(line.points.last(), Some(&b), "and ends at the destination's");
        assert_eq!(&line.points[1..4], &road[..], "along the roads between them");
        assert_eq!(plan_overlays(&[], None, None, Some(&road), None).line, None, "no ends, no line");
    }

    #[test]
    fn every_pin_is_a_marker_known_by_its_index_and_the_selected_one_shows_its_card() {
        let pins = [(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)];
        let plan = plan_overlays(&pins, Some(1), None, None, Some((7.0, 8.0)));
        assert_eq!(plan.pins.len(), 3);
        for (i, pin) in plan.pins.iter().enumerate() {
            assert_eq!((pin.index, pin.at, pin.open), (i, pins[i], i == 1));
            assert_eq!(pin_of_marker(pin.index as u64), Some(i));
        }
        assert_eq!(plan.here, Some((7.0, 8.0)));
        for other in [FROM_MARKER, TO_MARKER, HERE_MARKER] {
            assert_eq!(pin_of_marker(other), None, "not a pin");
        }
    }

    #[test]
    fn a_route_is_first_estimated_from_the_crow_flies_distance_at_the_modes_speed() {
        crate::l10n::in_english();
        let vienna = (48.2082, 16.3738);
        let munich = (48.1372, 11.5756);
        let crow = distance_km(vienna, munich);
        let car = estimate_route(vienna, munich, TravelMode::Car);
        assert!((car.km - crow * DETOUR_FACTOR).abs() < 1e-9, "{car:?}");
        assert!((car.minutes - car.km / TravelMode::Car.speed_kmh(car.km) * 60.0).abs() < 1e-9);
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
    fn a_car_route_from_frankfurt_to_berlin_is_estimated_at_motorway_speed() {
        let frankfurt = (50.1109, 8.6821);
        let berlin = (52.5200, 13.4050);
        let car = estimate_route(frankfurt, berlin, TravelMode::Car);
        assert!((500.0..600.0).contains(&car.km), "{car:?}");
        assert!((300.0..360.0).contains(&car.minutes), "five to six hours, not {car:?}");
        let offenbach = (50.0956, 8.7761);
        let town = estimate_route(frankfurt, offenbach, TravelMode::Car);
        let speed = town.km / (town.minutes / 60.0);
        assert!(speed < 60.0, "a trip across town is not at motorway speed: {speed} km/h");
    }

    #[test]
    fn a_planned_route_is_framed_in_the_part_of_the_window_the_sidebar_leaves_free() {
        let frankfurt = (50.1109, 8.6821);
        let berlin = (52.5200, 13.4050);
        let (size, inset) = ((1100.0_f32, 720.0_f32), 310.0_f32);
        let (lat, lon, zoom) = fit_view(frankfurt, berlin, size, inset);
        assert!((5.0..=8.0).contains(&zoom), "zoom {zoom}");
        let centre = mercator_px(lat, lon, zoom);
        for (end_lat, end_lon) in [frankfurt, berlin] {
            let p = mercator_px(end_lat, end_lon, zoom);
            let x = p.0 - centre.0 + f64::from(size.0) / 2.0;
            let y = p.1 - centre.1 + f64::from(size.1) / 2.0;
            assert!(
                x > f64::from(inset) + 20.0 && x < f64::from(size.0) - 20.0,
                "x {x} at zoom {zoom}"
            );
            assert!(y > 20.0 && y < f64::from(size.1) - 20.0, "y {y} at zoom {zoom}");
        }
        let (_, _, same) = fit_view(berlin, berlin, size, inset);
        assert!((10.0..=16.0).contains(&same), "one place shows its town, at zoom {same}");
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
