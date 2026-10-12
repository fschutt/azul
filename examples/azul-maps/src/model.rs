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

/// Whether a mark at `p` (view pixels) shows in a `width` x `height` view: a
/// pin's head reaches 40 px left, right and up of its point and its box 60 px
/// below it (where the window draws a place, it draws it only then).
#[must_use]
pub fn mark_visible(p: (f32, f32), width: f32, height: f32) -> bool {
    p.0 > -40.0 && p.0 < width + 40.0 && p.1 > -40.0 && p.1 < height + 60.0
}

/// What the window draws AT A PLACE on the map, in view pixels for one viewport:
/// the travel line, its ends, where you are, the pins.
///
/// The build plans the marks it draws (a margin past the view, so a pan can
/// move them in place before one is missing) and every step of a pan plans the
/// marks that show (no margin). The same rules make the two comparable
/// ([`pan_step`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// The travel line from start to destination, cut to the planned area.
    pub route: Option<((f32, f32), (f32, f32))>,
    /// The start of the travel line, when it is a place in the planned area.
    pub from: Option<(f32, f32)>,
    /// The destination, when it is a place in the planned area.
    pub to: Option<(f32, f32)>,
    /// Where you are, while the location is shown and known.
    pub here: Option<(f32, f32)>,
    /// The pins in the planned area: the pin's index in the pins file's order
    /// and its point.
    pub pins: Vec<(usize, (f32, f32))>,
}

impl Marks {
    /// The marks of a `size` view whose projection is `project` (`(lat, lon)`
    /// to view pixels), planned `margin` px past every edge: `(0, 0)` gives the
    /// marks that show.
    #[must_use]
    pub fn plan(
        project: impl Fn(f64, f64) -> (f32, f32),
        size: (f32, f32),
        margin: (f32, f32),
        pins: &[(f64, f64)],
        travel: Option<((f64, f64), (f64, f64))>,
        here: Option<(f64, f64)>,
    ) -> Self {
        let (mx, my) = margin;
        let (w, h) = (size.0 + 2.0 * mx, size.1 + 2.0 * my);
        // The planned area as a view of its own, its origin `margin` up and left.
        let inside = |p: (f32, f32)| mark_visible((p.0 + mx, p.1 + my), w, h);
        let point = |(lat, lon): (f64, f64)| Some(project(lat, lon)).filter(|p| inside(*p));
        let (route, from, to) = match travel {
            Some((a, b)) => {
                let (pa, pb) = (project(a.0, a.1), project(b.0, b.1));
                let route = clip_segment((pa.0 + mx, pa.1 + my), (pb.0 + mx, pb.1 + my), w, h)
                    .map(|(p, q)| ((p.0 - mx, p.1 - my), (q.0 - mx, q.1 - my)));
                (route, Some(pa).filter(|p| inside(*p)), Some(pb).filter(|p| inside(*p)))
            }
            None => (None, None, None),
        };
        Self {
            route,
            from,
            to,
            here: here.and_then(point),
            pins: pins
                .iter()
                .enumerate()
                .filter_map(|(index, &pin)| point(pin).map(|p| (index, p)))
                .collect(),
        }
    }

    /// Nothing at a place: no line, no dot, no pin.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.route.is_none()
            && self.from.is_none()
            && self.to.is_none()
            && self.here.is_none()
            && self.pins.is_empty()
    }
}

/// What a step of a pan does to the marks the last build drew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanStep {
    /// Nothing of the app's is drawn or comes into view: the map moves its
    /// tiles itself and the window stays as it is.
    MapOnly,
    /// Every mark that shows after the step was drawn by the build and only
    /// moved by this many pixels: the drawn marks move in place by it.
    Shift(f32, f32),
    /// A mark shows that the build did not draw, or the step did more than
    /// move the map (a zoom, a turn): only a new build draws it right.
    Rebuild,
}

/// How a step of a pan treats the marks `drawn` by the last build, given the
/// marks that show after it (`now`, planned without a margin) and how far
/// every place moved on screen since the build (`translation`; `None` when
/// the step zoomed, turned or tilted the map).
#[must_use]
pub fn pan_step(
    drawn: &Marks,
    now: &Marks,
    translation: Option<(f32, f32)>,
    size: (f32, f32),
) -> PanStep {
    let _ = (translation, size);
    if drawn.is_empty() && now.is_empty() {
        PanStep::MapOnly
    } else {
        PanStep::Rebuild
    }
}

/// How much longer a way by road or path is than the crow flies: ROUTING.md's
/// first step, the straight-line estimate.
pub const DETOUR_FACTOR: f64 = 1.3;

impl TravelMode {
    /// A typical door-to-door speed, km/h: the estimate's divisor.
    #[must_use]
    pub const fn speed_kmh(self) -> f64 {
        match self {
            Self::Car => 80.0,
            Self::Walk => 5.0,
            Self::Bike => 16.0,
            Self::Transit => 40.0,
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
        minutes: km / mode.speed_kmh() * 60.0,
    }
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

    /// A flat stand-in for the map's projection: 10 px per degree, the view's
    /// centre at `(lat, lon)` in the middle of an 800 x 600 view.
    fn flat(lat0: f64, lon0: f64) -> impl Fn(f64, f64) -> (f32, f32) {
        move |lat, lon| (400.0 + ((lon - lon0) * 10.0) as f32, 300.0 - ((lat - lat0) * 10.0) as f32)
    }

    const SIZE: (f32, f32) = (800.0, 600.0);
    const SHOWS: (f32, f32) = (0.0, 0.0);

    #[test]
    fn the_marks_that_show_are_the_ones_inside_the_view() {
        let shows = |pins: &[(f64, f64)], travel, here| {
            !Marks::plan(flat(0.0, 0.0), SIZE, SHOWS, pins, travel, here).is_empty()
        };
        assert!(!shows(&[], None, None), "an empty map moves itself");
        assert!(shows(&[(0.0, 0.0)], None, None), "a pin in view");
        assert!(!shows(&[(80.0, 170.0)], None, None), "a pin far outside the view does not count");
        assert!(
            shows(&[], Some(((0.0, -100.0), (0.0, 100.0))), None),
            "the travel line crossing the view counts, both ends outside"
        );
        assert!(!shows(&[], Some(((80.0, 170.0), (85.0, 175.0))), None));
        assert!(shows(&[], None, Some((1.0, 1.0))), "where you are");
        assert!(
            mark_visible((-30.0, 10.0), 800.0, 600.0) && !mark_visible((-50.0, 10.0), 800.0, 600.0),
            "a pin's head reaches 40 px past its point"
        );
        assert!(mark_visible((10.0, 650.0), 800.0, 600.0), "and 60 px below it");
        let drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &[(0.0, 100.0)], None, None);
        assert_eq!(drawn.pins, vec![(0, (1400.0, 300.0))], "the build draws a window past the view");
    }

    #[test]
    fn a_pan_with_the_route_on_the_map_moves_its_marks_in_place_instead_of_rebuilding_the_window() {
        // A line across the view whose ends are far outside it, and a pin.
        let travel = Some(((0.0, -100.0), (10.0, 100.0)));
        let pins = [(2.0, 3.0)];
        let drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &pins, travel, None);
        // One step: the view's centre goes 3 degrees east, every place 30 px left.
        let now = Marks::plan(flat(0.0, 3.0), SIZE, SHOWS, &pins, travel, None);
        assert_eq!(pan_step(&drawn, &now, Some((-30.0, 0.0)), SIZE), PanStep::Shift(-30.0, 0.0));
    }

    #[test]
    fn a_pin_the_build_did_not_draw_coming_into_view_rebuilds_the_window() {
        // 150 degrees east is 1500 px right of the middle: past the drawn margin.
        let pins = [(0.0, 150.0)];
        let drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &pins, None, None);
        assert!(drawn.is_empty());
        let now = Marks::plan(flat(0.0, 120.0), SIZE, SHOWS, &pins, None, None);
        assert_eq!(pan_step(&drawn, &now, Some((-1200.0, 0.0)), SIZE), PanStep::Rebuild);
    }

    #[test]
    fn a_zoom_or_a_turn_rebuilds_the_window_while_a_mark_shows() {
        let pins = [(0.0, 0.0)];
        let drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &pins, None, None);
        let now = Marks::plan(flat(0.0, 0.0), SIZE, SHOWS, &pins, None, None);
        assert_eq!(pan_step(&drawn, &now, None, SIZE), PanStep::Rebuild);
    }

    #[test]
    fn a_mark_that_leaves_the_view_is_moved_out_with_the_map() {
        let empty = Marks::default();
        assert_eq!(pan_step(&empty, &empty, Some((5.0, 0.0)), SIZE), PanStep::MapOnly);
        // The pin was drawn in view; one step later it is past the left edge.
        let pins = [(0.0, -35.0)];
        let drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &pins, None, None);
        let now = Marks::plan(flat(0.0, 10.0), SIZE, SHOWS, &pins, None, None);
        assert!(now.is_empty() && !drawn.is_empty());
        assert_eq!(pan_step(&drawn, &now, Some((-100.0, 0.0)), SIZE), PanStep::Shift(-100.0, 0.0));
    }

    #[test]
    fn a_long_pan_with_a_route_rebuilds_the_window_once_per_window_not_once_per_move() {
        // The window's cost while panning is its rebuilds (each a full layout
        // of the window); moving the drawn marks is a relayout of their boxes.
        // 400 moves of 5 px east along a line far longer than the window, two
        // pins near it: a rebuild only when the drawn part of the line runs out.
        let travel = Some(((0.0, -1000.0), (20.0, 1000.0)));
        let pins = [(1.0, 20.0), (-2.0, 60.0)];
        let mut built_at = 0.0_f64;
        let mut drawn = Marks::plan(flat(0.0, 0.0), SIZE, SIZE, &pins, travel, None);
        let mut rebuilds = 0;
        let started = std::time::Instant::now();
        for step in 1..=400 {
            let lon = f64::from(step) * 0.5;
            let now = Marks::plan(flat(0.0, lon), SIZE, SHOWS, &pins, travel, None);
            let mut shift = (-((lon - built_at) * 10.0) as f32, 0.0);
            match pan_step(&drawn, &now, Some(shift), SIZE) {
                PanStep::Rebuild => {
                    rebuilds += 1;
                    built_at = lon;
                    drawn = Marks::plan(flat(0.0, lon), SIZE, SIZE, &pins, travel, None);
                    shift = (0.0, 0.0);
                }
                PanStep::Shift(..) | PanStep::MapOnly => {}
            }
            // What the window shows after the step is what it should show.
            let (a, b) = drawn.route.expect("the line crosses every view of this pan");
            let shown = clip_segment(
                (a.0 + shift.0, a.1 + shift.1),
                (b.0 + shift.0, b.1 + shift.1),
                SIZE.0,
                SIZE.1,
            );
            let (p, q) = shown.expect("the drawn line covers the view");
            let (np, nq) = now.route.expect("the line shows");
            assert!(
                (p.0 - np.0).abs() < 0.5 && (p.1 - np.1).abs() < 0.5,
                "step {step}: the line starts at {p:?}, not {np:?}"
            );
            assert!(
                (q.0 - nq.0).abs() < 0.5 && (q.1 - nq.1).abs() < 0.5,
                "step {step}: the line ends at {q:?}, not {nq:?}"
            );
        }
        assert!(
            (1..=3).contains(&rebuilds),
            "a 2000 px pan rebuilt the window {rebuilds} times (one per window is enough)"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "planning 400 steps took {:?}: a step must cost microseconds",
            started.elapsed()
        );
    }

    #[test]
    fn a_route_is_first_estimated_from_the_crow_flies_distance_at_the_modes_speed() {
        crate::l10n::in_english();
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
