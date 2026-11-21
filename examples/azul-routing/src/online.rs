//! Routing on a Valhalla server, for where no routing tiles are at hand:
//! the places and the travel mode go to the server's `/route`, and the
//! road's shape, its length and time and the turn-by-turn steps come back.
//!
//! The default server is the public one FOSSGIS runs for openstreetmap.org
//! ([`FOSSGIS_VALHALLA`]). Its usage policy asks for light use, a user agent
//! that names the app, and no more than one request a second, so
//! [`Valhalla`] keeps its requests a second apart and answers a repeated
//! route from memory without asking again. Only the places and the mode
//! are sent.
//!
//! The request and the answer are plain functions ([`request_body`],
//! [`parse_route`], [`decode_polyline6`]) so they are tested without a
//! network; the client (feature `http`) only moves the bytes.

use serde_json::{json, Value};

use crate::Mode;

/// The route endpoint of the Valhalla server FOSSGIS runs for
/// openstreetmap.org's directions.
pub const FOSSGIS_VALHALLA: &str = "https://valhalla1.openstreetmap.de/route";

/// How far apart two requests to one server are at least: the FOSSGIS
/// servers allow one request a second.
pub const MIN_GAP: std::time::Duration = std::time::Duration::from_secs(1);

/// How long a road route stays fresh in the cache, seconds: a week (roads change slowly; a
/// stale route still answers when the server cannot be reached).
pub const ROUTE_TTL_S: u64 = 7 * 24 * 3600;

/// The cache key of a route question: the mode, the language and the places to about 10 m, so
/// a route asked again from a few metres away is the same question. The server is not part of
/// it: a route kept from one server answers offline when another is named.
#[must_use]
pub fn route_key(places: &[(f64, f64)], mode: Mode, language: &str) -> String {
    let places: Vec<String> = places
        .iter()
        .map(|&p| crate::cache::place_key(p, 4))
        .collect();
    format!(
        "valhalla {} {} {}",
        costing(mode),
        language_tag(language),
        places.join(" ")
    )
}

/// A route the server worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineRoute {
    /// Along the roads, in metres.
    pub distance_m: f64,
    /// The server's travel time, in seconds.
    pub duration_s: f64,
    /// The road's shape, `(lat, lon)`, from the start to the destination.
    pub points: Vec<(f64, f64)>,
    /// The turn-by-turn steps, in order.
    pub steps: Vec<Step>,
}

/// One turn-by-turn step of an [`OnlineRoute`].
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// What to do, in the language the request asked for.
    pub instruction: String,
    /// How far the step goes before the next one, in metres.
    pub distance_m: f64,
    /// What kind of turn it is (Valhalla's maneuver type), for its icon.
    pub kind: Maneuver,
    /// The step's part of [`OnlineRoute::points`]: from this index ...
    pub first_point: usize,
    /// ... to this one, both included.
    pub last_point: usize,
}

/// The kinds of turn a step shows an arrow for (Valhalla's maneuver types,
/// grouped by the arrow they take).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maneuver {
    Start,
    Destination,
    Straight,
    SlightRight,
    Right,
    SharpRight,
    UTurnRight,
    UTurnLeft,
    SharpLeft,
    Left,
    SlightLeft,
    RampRight,
    RampLeft,
    ForkRight,
    ForkLeft,
    Merge,
    Roundabout,
    Ferry,
}

impl Maneuver {
    /// The arrow of Valhalla's maneuver type `kind` (its API's numbering).
    #[must_use]
    pub const fn of_type(kind: u64) -> Self {
        match kind {
            1..=3 => Self::Start,
            4..=6 => Self::Destination,
            9 => Self::SlightRight,
            10 => Self::Right,
            11 => Self::SharpRight,
            12 => Self::UTurnRight,
            13 => Self::UTurnLeft,
            14 => Self::SharpLeft,
            15 => Self::Left,
            16 => Self::SlightLeft,
            18 | 20 => Self::RampRight,
            19 | 21 => Self::RampLeft,
            23 => Self::ForkRight,
            24 => Self::ForkLeft,
            25 | 37 | 38 => Self::Merge,
            26 | 27 => Self::Roundabout,
            28 | 29 => Self::Ferry,
            _ => Self::Straight,
        }
    }

    /// The Material icon that shows it.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Start => "trip_origin",
            Self::Destination => "flag",
            Self::Straight => "straight",
            Self::SlightRight => "turn_slight_right",
            Self::Right => "turn_right",
            Self::SharpRight => "turn_sharp_right",
            Self::UTurnRight => "u_turn_right",
            Self::UTurnLeft => "u_turn_left",
            Self::SharpLeft => "turn_sharp_left",
            Self::Left => "turn_left",
            Self::SlightLeft => "turn_slight_left",
            Self::RampRight => "ramp_right",
            Self::RampLeft => "ramp_left",
            Self::ForkRight => "fork_right",
            Self::ForkLeft => "fork_left",
            Self::Merge => "merge",
            Self::Roundabout => "roundabout_right",
            Self::Ferry => "directions_boat",
        }
    }
}

/// Why the server gave no route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnlineError {
    /// The server could not be reached (no network, a timeout).
    Offline(String),
    /// The server asks to slow down (HTTP 429).
    Busy,
    /// The server knows no way between the places, or no road near one.
    NoRoute(String),
    /// Anything else the server answered.
    Server(String),
}

/// Valhalla's costing for a travel mode.
#[must_use]
pub const fn costing(mode: Mode) -> &'static str {
    match mode {
        Mode::Car => "auto",
        Mode::Bike => "bicycle",
        Mode::Walk => "pedestrian",
    }
}

/// The JSON a route request sends: the places `(lat, lon)` in order, the
/// mode's costing, kilometres, and the language the steps are written in
/// (`de`, `en`, ...). Nothing else.
#[must_use]
pub fn request_body(places: &[(f64, f64)], mode: Mode, language: &str) -> String {
    let locations: Vec<Value> = places
        .iter()
        .map(|&(lat, lon)| json!({ "lat": lat, "lon": lon }))
        .collect();
    json!({
        "locations": locations,
        "costing": costing(mode),
        "directions_options": { "units": "kilometers", "language": language_tag(language) },
    })
    .to_string()
}

/// Valhalla's name of a language: `de` -> `de-DE`, `en` -> `en-US`; a tag
/// with a region already is kept.
fn language_tag(language: &str) -> String {
    let language = language.trim();
    if language.contains('-') {
        return language.to_string();
    }
    match language {
        "de" => "de-DE".to_string(),
        "" | "en" => "en-US".to_string(),
        other => other.to_string(),
    }
}

/// A shape in Valhalla's encoding: Google's polyline algorithm with six
/// decimals, `(lat, lon)` pairs. A truncated string ends at its last whole
/// pair.
#[must_use]
pub fn decode_polyline6(encoded: &str) -> Vec<(f64, f64)> {
    let bytes = encoded.as_bytes();
    let mut points = Vec::new();
    let (mut at, mut lat, mut lon) = (0usize, 0i64, 0i64);
    let next = |at: &mut usize| -> Option<i64> {
        let (mut shift, mut value) = (0u32, 0i64);
        loop {
            let byte = i64::from(*bytes.get(*at)?) - 63;
            *at += 1;
            value |= (byte & 0x1f) << shift;
            shift += 5;
            if byte < 0x20 {
                break;
            }
            if shift > 60 {
                return None;
            }
        }
        Some(if value & 1 == 1 { !(value >> 1) } else { value >> 1 })
    };
    while at < bytes.len() {
        let (Some(dlat), Some(dlon)) = (next(&mut at), next(&mut at)) else {
            break;
        };
        lat += dlat;
        lon += dlon;
        #[allow(clippy::cast_precision_loss)] // six decimals of a degree fit an f64
        points.push((lat as f64 / 1e6, lon as f64 / 1e6));
    }
    points
}

/// The route in a Valhalla `/route` answer: every leg's shape joined, the
/// trip's summary, every leg's steps (their point ranges into the joined
/// shape).
///
/// # Errors
///
/// The answer's own error (`error_code`), or an answer that is no route.
pub fn parse_route(json: &str) -> Result<OnlineRoute, OnlineError> {
    let doc: Value =
        serde_json::from_str(json).map_err(|e| OnlineError::Server(format!("json: {e}")))?;
    if doc.get("error_code").is_some() {
        return Err(error_of(&doc));
    }
    let trip = doc
        .get("trip")
        .ok_or_else(|| OnlineError::Server("no trip in the answer".to_string()))?;
    let number = |v: &Value, key: &str| v.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let summary = trip.get("summary").unwrap_or(&Value::Null);
    let mut route = OnlineRoute {
        distance_m: number(summary, "length") * 1000.0,
        duration_s: number(summary, "time"),
        points: Vec::new(),
        steps: Vec::new(),
    };
    for leg in trip.get("legs").and_then(Value::as_array).into_iter().flatten() {
        // A leg's shape starts where the last one ended: the shared point
        // is kept once.
        let shape = decode_polyline6(leg.get("shape").and_then(Value::as_str).unwrap_or(""));
        let offset = route.points.len().saturating_sub(usize::from(!route.points.is_empty()));
        let skip = usize::from(!route.points.is_empty());
        route.points.extend(shape.iter().skip(skip));
        for m in leg.get("maneuvers").and_then(Value::as_array).into_iter().flatten() {
            let index = |key: &str| {
                usize::try_from(m.get(key).and_then(Value::as_u64).unwrap_or(0)).unwrap_or(0)
            };
            let last = route.points.len().saturating_sub(1);
            route.steps.push(Step {
                instruction: m
                    .get("instruction")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                distance_m: number(m, "length") * 1000.0,
                kind: Maneuver::of_type(m.get("type").and_then(Value::as_u64).unwrap_or(0)),
                first_point: (offset + index("begin_shape_index")).min(last),
                last_point: (offset + index("end_shape_index")).min(last),
            });
        }
    }
    if route.points.len() < 2 {
        return Err(OnlineError::Server("the route has no shape".to_string()));
    }
    Ok(route)
}

/// What an answer that is not a route says: Valhalla's `error_code` 442
/// (no path) and 171 (no road near a place) are no route; HTTP 429 is
/// [`OnlineError::Busy`]; anything else is the server's.
#[must_use]
pub fn parse_error(status: u16, body: &str) -> OnlineError {
    if status == 429 {
        return OnlineError::Busy;
    }
    match serde_json::from_str::<Value>(body) {
        Ok(doc) if doc.get("error_code").is_some() => error_of(&doc),
        _ => OnlineError::Server(format!("HTTP {status}")),
    }
}

fn error_of(doc: &Value) -> OnlineError {
    let code = doc.get("error_code").and_then(Value::as_u64).unwrap_or(0);
    let text = doc
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match code {
        171 | 442 | 443 => OnlineError::NoRoute(text),
        _ => OnlineError::Server(format!("{code}: {text}")),
    }
}

/// A Valhalla server, as the route workers of a session share it: one
/// connection pool, requests at least [`MIN_GAP`] apart, a pause after
/// failures, and the routes kept ([`crate::cache`]) so a repeated or nearby
/// route sends nothing and a kept one answers, marked offline, when the
/// server cannot be reached.
#[cfg(feature = "http")]
pub struct Valhalla {
    /// The server's `/route` URL.
    pub url: String,
    /// What the requests name themselves: the app and its version, nothing
    /// about the user.
    pub user_agent: String,
    /// The connection pool, the gate and the cache the requests go through.
    fetcher: crate::fetch::Fetcher,
}

#[cfg(feature = "http")]
impl Valhalla {
    /// How many routes a server without a cache of its own remembers.
    const REMEMBERED: usize = 32;

    /// The server at `url`, asked under the name `user_agent`, its routes
    /// remembered for the session.
    #[must_use]
    pub fn new(url: impl Into<String>, user_agent: impl Into<String>) -> Self {
        Self::with_cache(
            url,
            user_agent,
            std::sync::Arc::new(crate::cache::ResponseCache::in_memory(Self::REMEMBERED)),
        )
    }

    /// The server at `url`, its routes kept in `cache` (in memory and, with a folder, on disk
    /// across sessions).
    #[must_use]
    pub fn with_cache(
        url: impl Into<String>,
        user_agent: impl Into<String>,
        cache: std::sync::Arc<crate::cache::ResponseCache>,
    ) -> Self {
        let user_agent = user_agent.into();
        Self {
            url: url.into(),
            fetcher: crate::fetch::Fetcher::new(user_agent.clone(), MIN_GAP, cache),
            user_agent,
        }
    }

    /// The same server asked trusting exactly the roots in `pems` (PEM text: the app's
    /// set) instead of the Mozilla roots ([`crate::fetch::Fetcher::trusting`]).
    #[must_use]
    pub fn trusting(mut self, pems: &[String]) -> Self {
        self.fetcher = self.fetcher.trusting(pems);
        self
    }

    /// The fetcher the requests go through (offline by choice is set there).
    #[must_use]
    pub fn fetcher(&self) -> &crate::fetch::Fetcher {
        &self.fetcher
    }

    /// The route through `places` for `mode`, its steps in `language`.
    /// Waits for its turn after the last request ([`MIN_GAP`]), asking
    /// `stop` meanwhile; a route asked for before is answered from the cache.
    ///
    /// # Errors
    ///
    /// [`OnlineError`]; `Ok(None)` when `stop` said to give up first.
    pub fn route(
        &self,
        places: &[(f64, f64)],
        mode: Mode,
        language: &str,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<OnlineRoute>, OnlineError> {
        Ok(self
            .route_with_origin(places, mode, language, stop)?
            .map(|(route, _)| route))
    }

    /// [`Valhalla::route`], and where the route came from: the server, the cache, or - the
    /// server out of reach - a kept route marked offline.
    ///
    /// # Errors
    ///
    /// [`OnlineError`]; `Ok(None)` when `stop` said to give up first.
    pub fn route_with_origin(
        &self,
        places: &[(f64, f64)],
        mode: Mode,
        language: &str,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<(OnlineRoute, crate::fetch::Origin)>, OnlineError> {
        use crate::fetch::{FetchError, Request};
        let body = request_body(places, mode, language);
        let request = Request::PostJson {
            url: &self.url,
            body: &body,
        };
        let key = route_key(places, mode, language);
        match self.fetcher.fetch(&request, &key, ROUTE_TTL_S, stop) {
            Ok(None) => Ok(None),
            Ok(Some(fetched)) => Ok(Some((parse_route(&fetched.body)?, fetched.origin))),
            Err(FetchError::Offline(why)) => Err(OnlineError::Offline(why)),
            Err(FetchError::Busy) => Err(OnlineError::Busy),
            Err(FetchError::Refused(status, body)) => Err(parse_error(status, &body)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Google's example polyline, re-encoded with six decimals: 38.5,-120.2
    /// / 40.7,-120.95 / 43.252,-126.453.
    const SHAPE: &str = "_izlhA~rlgdF_{geC~ywl@_kwzCn`{nI";

    #[test]
    fn a_request_sends_the_places_the_mode_and_the_language_and_nothing_else() {
        let body = request_body(&[(52.5163, 13.3777), (52.52, 13.405)], Mode::Bike, "de");
        let doc: Value = serde_json::from_str(&body).expect("json");
        assert_eq!(doc["costing"], "bicycle");
        assert_eq!(doc["locations"][0]["lat"], 52.5163);
        assert_eq!(doc["locations"][1]["lon"], 13.405);
        assert_eq!(doc["directions_options"]["language"], "de-DE");
        assert_eq!(doc["directions_options"]["units"], "kilometers");
        let mut keys: Vec<&str> = doc.as_object().expect("an object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["costing", "directions_options", "locations"]);
        assert_eq!(costing(Mode::Car), "auto");
        assert_eq!(costing(Mode::Walk), "pedestrian");
    }

    #[test]
    fn a_six_decimal_polyline_decodes_to_its_places() {
        let points = decode_polyline6(SHAPE);
        let want = [(38.5, -120.2), (40.7, -120.95), (43.252, -126.453)];
        assert_eq!(points.len(), want.len(), "{points:?}");
        for (got, want) in points.iter().zip(want) {
            assert!((got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9, "{got:?}");
        }
        assert!(decode_polyline6("").is_empty());
        assert_eq!(decode_polyline6(&SHAPE[..SHAPE.len() - 3]).len(), 2, "a cut string keeps its whole pairs");
    }

    fn answer() -> String {
        json!({
            "trip": {
                "summary": { "length": 2.5, "time": 330.0 },
                "legs": [{
                    "shape": SHAPE,
                    "maneuvers": [
                        { "type": 1, "instruction": "Drive north.", "length": 1.2, "begin_shape_index": 0, "end_shape_index": 1 },
                        { "type": 10, "instruction": "Turn right.", "length": 1.3, "begin_shape_index": 1, "end_shape_index": 2 },
                        { "type": 4, "instruction": "You have arrived.", "length": 0.0, "begin_shape_index": 2, "end_shape_index": 2 }
                    ]
                }]
            }
        })
        .to_string()
    }

    #[test]
    fn an_answer_gives_the_shape_the_summary_and_the_steps() {
        let route = parse_route(&answer()).expect("a route");
        assert_eq!(route.points.len(), 3);
        assert!((route.distance_m - 2500.0).abs() < 1e-6);
        assert!((route.duration_s - 330.0).abs() < 1e-6);
        assert_eq!(route.steps.len(), 3);
        assert_eq!(route.steps[1].instruction, "Turn right.");
        assert_eq!(route.steps[1].kind, Maneuver::Right);
        assert_eq!(route.steps[1].kind.icon(), "turn_right");
        assert!((route.steps[1].distance_m - 1300.0).abs() < 1e-6);
        assert_eq!((route.steps[1].first_point, route.steps[1].last_point), (1, 2));
        assert_eq!(route.steps[0].kind, Maneuver::Start);
        assert_eq!(route.steps[2].kind, Maneuver::Destination);
    }

    #[test]
    fn a_route_asked_again_from_a_few_metres_away_is_the_same_question() {
        let a = route_key(&[(52.516_30, 13.377_70), (52.52, 13.405)], Mode::Car, "en");
        let b = route_key(&[(52.516_32, 13.377_72), (52.52, 13.405)], Mode::Car, "en");
        assert_eq!(a, b);
        assert_ne!(a, route_key(&[(52.516_30, 13.377_70), (52.52, 13.405)], Mode::Walk, "en"));
        assert_ne!(a, route_key(&[(52.516_30, 13.377_70), (52.52, 13.405)], Mode::Car, "de"));
        assert_ne!(a, route_key(&[(52.5263, 13.3777), (52.52, 13.405)], Mode::Car, "en"));
    }

    #[test]
    fn the_servers_refusals_are_told_apart() {
        let no_path = r#"{"error_code":442,"error":"No path could be found for input","status_code":400}"#;
        assert!(matches!(parse_error(400, no_path), OnlineError::NoRoute(_)));
        assert!(matches!(parse_route(no_path), Err(OnlineError::NoRoute(_))));
        let no_road = r#"{"error_code":171,"error":"No suitable edges near location","status_code":400}"#;
        assert!(matches!(parse_error(400, no_road), OnlineError::NoRoute(_)));
        assert_eq!(parse_error(429, "Too many requests"), OnlineError::Busy);
        assert!(matches!(parse_error(502, "<html>"), OnlineError::Server(_)));
    }

    #[cfg(feature = "http")]
    mod server {
        use std::{
            io::{BufRead, BufReader, Read, Write},
            net::TcpListener,
            sync::{Arc, Mutex},
            time::Instant,
        };

        use super::*;

        /// A one-route server on a local port: every request it read, and
        /// how many it answered.
        fn mock(status: u16, body: String) -> (String, Arc<Mutex<Vec<String>>>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
            let url = format!("http://{}/route", listener.local_addr().expect("an address"));
            let seen = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
                    let mut head = String::new();
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                        head.push_str(&line);
                    }
                    let mut sent = vec![0u8; length];
                    let _ = reader.read_exact(&mut sent);
                    head.push_str(&String::from_utf8_lossy(&sent));
                    log.lock().expect("the log").push(head);
                    let reason = if status == 200 { "OK" } else { "Error" };
                    let mut stream = stream;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            });
            (url, seen)
        }

        #[test]
        fn a_route_is_asked_once_and_a_repeat_is_answered_from_memory() {
            let (url, seen) = mock(200, answer());
            let server = Valhalla::new(url, "AzMaps-test/1 (+https://github.com/fschutt/azul)");
            let places = [(52.5163, 13.3777), (52.52, 13.405)];
            let route = server
                .route(&places, Mode::Car, "en", &mut || false)
                .expect("an answer")
                .expect("not stopped");
            assert_eq!(route.points.len(), 3);
            let again = server.route(&places, Mode::Car, "en", &mut || false).expect("an answer");
            assert_eq!(again, Some(route));
            let seen = seen.lock().expect("the log");
            assert_eq!(seen.len(), 1, "the repeat sent nothing");
            let request = seen[0].to_ascii_lowercase();
            assert!(request.starts_with("post /route"), "{request}");
            assert!(request.contains("user-agent: azmaps-test/1"), "{request}");
            assert!(request.contains("\"costing\":\"auto\""), "{request}");
        }

        #[test]
        fn two_routes_are_asked_a_second_apart() {
            let (url, seen) = mock(200, answer());
            let server = Valhalla::new(url, "AzMaps-test/1");
            let started = Instant::now();
            let _ = server.route(&[(1.0, 1.0), (1.1, 1.1)], Mode::Car, "en", &mut || false);
            let _ = server.route(&[(1.0, 1.0), (1.2, 1.2)], Mode::Car, "en", &mut || false);
            assert!(started.elapsed() >= MIN_GAP, "{:?}", started.elapsed());
            assert_eq!(seen.lock().expect("the log").len(), 2);
        }

        #[test]
        fn a_stopped_request_waiting_for_its_turn_sends_nothing() {
            let (url, seen) = mock(200, answer());
            let server = Valhalla::new(url, "AzMaps-test/1");
            let _ = server.route(&[(1.0, 1.0), (1.1, 1.1)], Mode::Car, "en", &mut || false);
            let stopped = server.route(&[(1.0, 1.0), (1.3, 1.3)], Mode::Car, "en", &mut || true);
            assert_eq!(stopped, Ok(None));
            assert_eq!(seen.lock().expect("the log").len(), 1);
        }

        #[test]
        fn a_route_of_an_earlier_session_answers_from_the_disk_marked_offline() {
            use crate::{cache::ResponseCache, fetch::Origin};
            let dir = std::env::temp_dir().join(format!("azul-routing-valhalla-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let places = [(52.5163, 13.3777), (52.52, 13.405)];
            let (url, _) = mock(200, answer());
            let first = Valhalla::with_cache(url, "AzMaps-test/1", Arc::new(ResponseCache::new(Some(dir.clone()), 8, 1 << 20)));
            let (route, origin) = first
                .route_with_origin(&places, Mode::Car, "en", &mut || false)
                .expect("an answer")
                .expect("not stopped");
            assert_eq!(origin, Origin::Network);
            // The next session: the kept route is old and the server is gone.
            let cache = Arc::new(ResponseCache::new(Some(dir.clone()), 8, 1 << 20));
            let key = route_key(&places, Mode::Car, "en");
            let mut kept = cache.get(&key).expect("kept on disk");
            kept.stored_at = 0;
            cache.put(&key, kept);
            let closed = TcpListener::bind("127.0.0.1:0").expect("a port");
            let gone = format!("http://{}/route", closed.local_addr().expect("an address"));
            drop(closed);
            let offline = Valhalla::with_cache(gone, "AzMaps-test/1", cache);
            let nearby = [(52.516_31, 13.377_71), (52.52, 13.405)];
            let (again, origin) = offline
                .route_with_origin(&nearby, Mode::Car, "en", &mut || false)
                .expect("the kept route")
                .expect("not stopped");
            assert_eq!(origin, Origin::Offline);
            assert_eq!(again, route);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_busy_server_and_no_server_say_so() {
            let (url, _) = mock(429, "Too many requests".to_string());
            let busy = Valhalla::new(url, "AzMaps-test/1");
            assert_eq!(busy.route(&[(1.0, 1.0), (1.1, 1.1)], Mode::Car, "en", &mut || false), Err(OnlineError::Busy));
            // A port nothing listens on.
            let closed = TcpListener::bind("127.0.0.1:0").expect("a port");
            let url = format!("http://{}/route", closed.local_addr().expect("an address"));
            drop(closed);
            let offline = Valhalla::new(url, "AzMaps-test/1");
            assert!(matches!(
                offline.route(&[(1.0, 1.0), (1.1, 1.1)], Mode::Car, "en", &mut || false),
                Err(OnlineError::Offline(_))
            ));
        }
    }
}
