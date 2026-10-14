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
        let _ = kind;
        Self::Straight
    }

    /// The Material icon that shows it.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        "straight"
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
    let _ = mode;
    "auto"
}

/// The JSON a route request sends: the places `(lat, lon)` in order, the
/// mode's costing, kilometres, and the language the steps are written in
/// (`de`, `en`, ...). Nothing else.
#[must_use]
pub fn request_body(places: &[(f64, f64)], mode: Mode, language: &str) -> String {
    let _ = (places, mode, language);
    String::new()
}

/// Valhalla's name of a language: `de` -> `de-DE`, `en` -> `en-US`; a tag
/// with a region already is kept.
#[allow(dead_code)]
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
    let _ = encoded;
    Vec::new()
}

/// The route in a Valhalla `/route` answer: every leg's shape joined, the
/// trip's summary, every leg's steps (their point ranges into the joined
/// shape).
///
/// # Errors
///
/// The answer's own error (`error_code`), or an answer that is no route.
pub fn parse_route(json: &str) -> Result<OnlineRoute, OnlineError> {
    let _ = json;
    Err(OnlineError::Server(String::new()))
}

/// What an answer that is not a route says: Valhalla's `error_code` 442
/// (no path) and 171 (no road near a place) are no route; HTTP 429 is
/// [`OnlineError::Busy`]; anything else is the server's.
#[must_use]
pub fn parse_error(status: u16, body: &str) -> OnlineError {
    let _ = (status, body);
    OnlineError::Server(String::new())
}

#[allow(dead_code)]
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
/// connection pool, requests at least [`MIN_GAP`] apart, and the routes of
/// the session in memory so a repeated one sends nothing.
#[cfg(feature = "http")]
pub struct Valhalla {
    /// The server's `/route` URL.
    pub url: String,
    /// What the requests name themselves: the app and its version, nothing
    /// about the user.
    pub user_agent: String,
    agent: ureq::Agent,
    /// When the last request went out: the next waits until [`MIN_GAP`]
    /// later.
    last_sent: std::sync::Mutex<Option<std::time::Instant>>,
    /// The latest routes by their request body, newest last.
    answered: std::sync::Mutex<std::collections::VecDeque<(String, OnlineRoute)>>,
}

#[cfg(feature = "http")]
impl Valhalla {
    /// How many routes it remembers.
    const REMEMBERED: usize = 32;
    /// The largest answer it reads, bytes: a cross-continent route with its
    /// steps is well under.
    const MAX_ANSWER: u64 = 16 * 1024 * 1024;

    /// The server at `url`, asked under the name `user_agent`.
    #[must_use]
    pub fn new(url: impl Into<String>, user_agent: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            user_agent: user_agent.into(),
            agent: crate::source::http_agent(),
            last_sent: std::sync::Mutex::new(None),
            answered: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    /// The route through `places` for `mode`, its steps in `language`.
    /// Waits for its turn after the last request ([`MIN_GAP`]), asking
    /// `stop` meanwhile; a route asked for before is answered from memory.
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
        let _ = (places, mode, language, stop, &self.agent, &self.last_sent, Self::MAX_ANSWER);
        Err(OnlineError::Offline(String::new()))
    }

    #[allow(dead_code)]
    fn remembered(&self, body: &str) -> Option<OnlineRoute> {
        let answered = self.answered.lock().ok()?;
        answered
            .iter()
            .rev()
            .find(|(asked, _)| asked == body)
            .map(|(_, route)| route.clone())
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
