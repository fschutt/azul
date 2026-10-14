//! Transitous (<https://transitous.org>): a community-run MOTIS server with the public timetables
//! of most of Europe and beyond, Deutsche Bahn's long-distance and regional trains included
//! through the German GTFS feeds, with live delays and platforms where the feeds have them.
//!
//! Three questions are asked of it: journeys (`/api/v6/plan`), places by name - stations,
//! addresses, points of interest - for the search (`/api/v1/geocode`), and the address of a
//! place (`/api/v1/reverse-geocode`). Its usage policy asks for open-source, non-commercial,
//! light use, a User-Agent naming the app, its version and a contact URL, and a visible link to
//! its sources page; so every answer is kept ([`crate::fetch`]) and the requests are spaced.
//! Only the places, the time, the typed text and the language are sent.
//!
//! The questions and the answers are plain functions, tested without a network.

use serde_json::Value;

use super::{
    format_time, parse_colour, parse_time, Journey, JourneyPage, JourneyQuery, Leg, LegKind,
    Stop, TransitError, When,
};
use crate::{cache::place_key, fetch::Origin};

/// The Transitous server.
pub const TRANSITOUS: &str = "https://api.transitous.org";

/// Its sources page, which its usage policy asks apps to link to.
pub const SOURCES: &str = "https://transitous.org/sources/";

/// The credit under journeys and search results from it.
pub const CREDIT: &str = "Transitous \u{b7} \u{a9} OpenStreetMap contributors";

/// How long journeys stay fresh, seconds: delays and platforms change by the minute.
pub const PLAN_TTL_S: u64 = 300;

/// How wide a time bucket of a journey question is, seconds: departures asked for within it
/// share one answer.
pub const PLAN_BUCKET_S: i64 = 300;

/// How long place searches and addresses stay fresh, seconds: thirty days.
pub const PLACES_TTL_S: u64 = 30 * 24 * 3600;

/// The least time between two requests to the server.
pub const GAP: std::time::Duration = std::time::Duration::from_millis(500);

/// The query parameters of a journey question. A question for "now" asks for the start of the
/// current time bucket, so asking again a minute later is the same question.
#[must_use]
pub fn plan_query(query: &JourneyQuery, now: i64) -> Vec<(String, String)> {
    let _ = (query, now, format_time(0));
    Vec::new()
}

/// The cache key of a journey question: the places to about 10 m, the time bucket, the page,
/// the language.
#[must_use]
pub fn plan_key(query: &JourneyQuery, now: i64) -> String {
    let _ = (query, now, place_key((0.0, 0.0), 4));
    String::new()
}

/// The journeys of a `/api/v6/plan` answer, the cursors of the pages before and after it.
///
/// # Errors
///
/// An answer that is no plan, the server's error, or a plan without any journey.
pub fn parse_plan(json: &str) -> Result<JourneyPage, TransitError> {
    let _ = json;
    Err(TransitError::Server("not built".to_string()))
}

/// What a found place is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundKind {
    /// A station or stop.
    Stop,
    /// A street address.
    Address,
    /// A point of interest, a town.
    Place,
}

/// A place the server found.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub name: String,
    pub kind: FoundKind,
    pub at: (f64, f64),
    /// The town or region it is in, as the server would show it with the name.
    pub area: Option<String>,
    /// Its country's code (`DE`).
    pub country: Option<String>,
}

impl Found {
    /// What a travel field shows for it: `Berlin Hbf, Berlin`; the area is left out when the
    /// name says it already.
    #[must_use]
    pub fn label(&self) -> String {
        self.name.clone()
    }
}

/// The query parameters of a place search: the text, a bias towards where the map looks.
#[must_use]
pub fn geocode_query(text: &str, near: Option<(f64, f64)>, language: &str) -> Vec<(String, String)> {
    let _ = (text, near, language);
    Vec::new()
}

/// The cache key of a place search: the text without case or surrounding space, the bias to
/// about 10 km, the language.
#[must_use]
pub fn geocode_key(text: &str, near: Option<(f64, f64)>, language: &str) -> String {
    let _ = (text, near, language);
    String::new()
}

/// The places of a `/api/v1/geocode` answer, best first.
///
/// # Errors
///
/// An answer that is no list of places.
pub fn parse_geocode(json: &str) -> Result<Vec<Found>, TransitError> {
    let _ = json;
    Err(TransitError::Server("not built".to_string()))
}

/// The query parameters of an address question.
#[must_use]
pub fn reverse_query(at: (f64, f64)) -> Vec<(String, String)> {
    let _ = at;
    Vec::new()
}

/// The cache key of an address question: the place to about 10 m.
#[must_use]
pub fn reverse_key(at: (f64, f64)) -> String {
    let _ = at;
    String::new()
}

/// The nearest address of a `/api/v1/reverse-geocode` answer.
///
/// # Errors
///
/// An answer that is no list of places.
pub fn parse_reverse(json: &str) -> Result<Option<Found>, TransitError> {
    let _ = json;
    Err(TransitError::Server("not built".to_string()))
}

/// A place of a geocode answer.
fn found_of(m: &Value) -> Option<Found> {
    let _ = m;
    None
}

/// A leg of a plan answer.
fn leg_of(leg: &Value) -> Option<Leg> {
    let _ = (leg, LegKind::Other, parse_time(""), parse_colour(""));
    None
}

/// A stop of a leg.
fn stop_of(place: &Value) -> Stop {
    let _ = place;
    Stop::default()
}

/// A journey of a plan answer.
fn journey_of(itinerary: &Value) -> Option<Journey> {
    let _ = (itinerary, When::Now);
    None
}

#[cfg(feature = "http")]
pub use client::Transitous;

#[cfg(feature = "http")]
mod client {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::ResponseCache,
        fetch::{FetchError, Fetcher, Request},
        transit::TransitProvider,
    };

    /// The server as a session asks it: one connection pool, one gate, one cache.
    pub struct Transitous {
        /// The server's base URL (`https://api.transitous.org`).
        pub base: String,
        fetcher: Fetcher,
    }

    impl Transitous {
        /// The server at `base`, asked under `user_agent`, its answers kept in `cache`.
        #[must_use]
        pub fn new(base: &str, user_agent: &str, cache: Arc<ResponseCache>) -> Self {
            Self {
                base: base.trim().trim_end_matches('/').to_string(),
                fetcher: Fetcher::new(user_agent, GAP, cache),
            }
        }

        /// The fetcher its questions go through (offline by choice is set there).
        #[must_use]
        pub fn fetcher(&self) -> &Fetcher {
            &self.fetcher
        }

        /// The places called `text`, the ones near `near` first, and where the answer came
        /// from; `Ok(None)` when `stop` said to give up first.
        ///
        /// # Errors
        ///
        /// [`TransitError`] when neither the server nor the cache has an answer.
        pub fn geocode(
            &self,
            text: &str,
            near: Option<(f64, f64)>,
            language: &str,
            stop: &mut dyn FnMut() -> bool,
        ) -> Result<Option<(Vec<Found>, Origin)>, TransitError> {
            let _ = (text, near, language, stop);
            Err(TransitError::Server("not built".to_string()))
        }

        /// The address nearest to `at`, and where the answer came from.
        ///
        /// # Errors
        ///
        /// [`TransitError`] when neither the server nor the cache has an answer.
        pub fn reverse(
            &self,
            at: (f64, f64),
            stop: &mut dyn FnMut() -> bool,
        ) -> Result<Option<(Option<Found>, Origin)>, TransitError> {
            let _ = (at, stop);
            Err(TransitError::Server("not built".to_string()))
        }
    }

    impl TransitProvider for Transitous {
        fn key(&self) -> &'static str {
            "transitous"
        }

        fn credit(&self) -> &'static str {
            CREDIT
        }

        fn credit_url(&self) -> &'static str {
            SOURCES
        }

        fn plan(
            &self,
            query: &JourneyQuery,
            stop: &mut dyn FnMut() -> bool,
        ) -> Result<Option<JourneyPage>, TransitError> {
            let _ = (query, stop, Request::Get { url: "", query: &[] });
            Err(error_of(FetchError::Busy))
        }
    }

    /// A fetch error in the transit words.
    pub(super) fn error_of(error: FetchError) -> TransitError {
        let _ = error;
        TransitError::Busy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = include_str!("fixtures/plan_berlin_leipzig.json");
    const GEOCODE: &str = include_str!("fixtures/geocode_berlin_hbf.json");
    const REVERSE: &str = include_str!("fixtures/reverse_pariser_platz.json");

    const BERLIN_HBF: (f64, f64) = (52.52498, 13.369114);
    const LEIPZIG_HBF: (f64, f64) = (51.3455, 12.3821);

    fn query(when: When) -> JourneyQuery {
        JourneyQuery {
            from: BERLIN_HBF,
            to: LEIPZIG_HBF,
            when,
            page: None,
            language: "de".to_string(),
        }
    }

    fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
        params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    #[test]
    fn a_journey_question_sends_the_places_the_time_and_the_language_and_nothing_else() {
        let now = 1_792_047_700;
        let params = plan_query(&query(When::DepartAt(1_792_047_600)), now);
        assert_eq!(param(&params, "fromPlace"), Some("52.524980,13.369114"));
        assert_eq!(param(&params, "toPlace"), Some("51.345500,12.382100"));
        assert_eq!(param(&params, "time"), Some("2026-10-15T07:00:00Z"));
        assert_eq!(param(&params, "arriveBy"), None);
        assert_eq!(param(&params, "language"), Some("de"));
        let mut names: Vec<&str> = params.iter().map(|(k, _)| k.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["fromPlace", "language", "time", "toPlace"]);
        let arrive = plan_query(&query(When::ArriveBy(1_792_054_800)), now);
        assert_eq!(param(&arrive, "arriveBy"), Some("true"));
        assert_eq!(param(&arrive, "time"), Some("2026-10-15T09:00:00Z"));
        let mut later = query(When::Now);
        later.page = Some("LATER|1792056360".to_string());
        assert_eq!(param(&plan_query(&later, now), "pageCursor"), Some("LATER|1792056360"));
    }

    #[test]
    fn a_question_for_now_asked_again_a_minute_later_is_the_same_question() {
        let now = 1_792_047_700;
        let first = plan_query(&query(When::Now), now);
        assert_eq!(param(&first, "time"), Some("2026-10-15T07:00:00Z"), "the bucket's start");
        assert_eq!(plan_key(&query(When::Now), now), plan_key(&query(When::Now), now + 60));
        assert_ne!(plan_key(&query(When::Now), now), plan_key(&query(When::Now), now + 600));
        let mut nearby = query(When::Now);
        nearby.from = (52.52501, 13.36913);
        assert_eq!(plan_key(&query(When::Now), now), plan_key(&nearby, now), "a few metres away");
        let mut arrive = query(When::ArriveBy(now));
        assert_ne!(plan_key(&query(When::DepartAt(now)), now), plan_key(&arrive, now));
        arrive.language = "en".to_string();
        assert_ne!(plan_key(&query(When::ArriveBy(now)), now), plan_key(&arrive, now));
    }

    #[test]
    fn a_plan_answer_gives_the_journeys_with_their_legs_lines_platforms_and_times() {
        let page = parse_plan(PLAN).expect("a plan");
        assert_eq!(page.journeys.len(), 2);
        assert_eq!(page.earlier.as_deref(), Some("EARLIER|1792047600"));
        assert_eq!(page.later.as_deref(), Some("LATER|1792056360"));
        let first = &page.journeys[0];
        assert_eq!(first.transfers, 1);
        assert_eq!(first.legs.len(), 5);
        let kinds: Vec<LegKind> = first.legs.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [LegKind::Walk, LegKind::Suburban, LegKind::Walk, LegKind::HighSpeedTrain, LegKind::Walk]
        );
        let s5 = &first.legs[1];
        assert_eq!(s5.line, "S5");
        assert_eq!(s5.headsign, "S Westkreuz (Berlin)");
        assert_eq!(s5.operator, "S-Bahn Berlin GmbH");
        assert_eq!(s5.colour, Some((0xeb, 0x74, 0x05)));
        assert_eq!(s5.from.name, "S+U Alexanderplatz Bhf (Berlin)");
        assert_eq!(s5.to.platform.as_deref(), Some("16"));
        assert!(s5.points.len() >= 2, "drawn along its way");
        let ice = &first.legs[3];
        assert_eq!(ice.line, "ICE 597");
        assert_eq!(ice.from.platform.as_deref(), Some("4"));
        assert_eq!(ice.departure, parse_time("2026-10-15T07:27:00Z").expect("a time"));
        assert_eq!(ice.arrival, parse_time("2026-10-15T08:42:00Z").expect("a time"));
        assert!((ice.points[0].0 - 52.525).abs() < 0.01, "starts at Berlin Hbf: {:?}", ice.points[0]);
        assert_eq!(first.legs[0].from.name, "", "the trip's own start keeps the field's name");
        assert_eq!(first.legs[0].distance_m, Some(260.0));
        assert_eq!(first.duration_s(), 6_120);
        assert!(!first.disrupted(3));
    }

    #[test]
    fn a_late_train_on_a_changed_platform_is_read_as_such() {
        let page = parse_plan(PLAN).expect("a plan");
        let ice = &page.journeys[1].legs[1];
        assert_eq!(ice.line, "ICE 91");
        assert_eq!(ice.departure_delay_min(), 3);
        assert_eq!(ice.from.platform.as_deref(), Some("5"));
        assert_eq!(ice.from.scheduled_platform.as_deref(), Some("3"));
        assert!(ice.from.platform_changed());
        assert!(ice.realtime);
        assert!(page.journeys[1].disrupted(3));
    }

    #[test]
    fn an_empty_plan_or_an_error_is_no_journey() {
        let empty = r#"{"from":{},"to":{},"direct":[],"itineraries":[],"previousPageCursor":"","nextPageCursor":""}"#;
        assert!(matches!(parse_plan(empty), Err(TransitError::NoJourney(_))));
        assert!(matches!(parse_plan(r#"{"error":"unknown place"}"#), Err(TransitError::NoJourney(_))));
        assert!(matches!(parse_plan("<html>"), Err(TransitError::Server(_))));
    }

    #[test]
    fn a_place_search_finds_the_station_first_with_its_town() {
        let found = parse_geocode(GEOCODE).expect("places");
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].kind, FoundKind::Stop);
        assert_eq!(found[0].name, "Berlin Hbf");
        assert_eq!(found[0].area.as_deref(), Some("Berlin"));
        assert_eq!(found[0].country.as_deref(), Some("DE"));
        assert_eq!(found[0].label(), "Berlin Hbf, Berlin");
        assert!((found[0].at.0 - 52.52498).abs() < 1e-6);
        assert_eq!(found[1].kind, FoundKind::Place);
        let berlin = Found {
            name: "Berlin".to_string(),
            kind: FoundKind::Place,
            at: (52.52, 13.405),
            area: Some("Berlin".to_string()),
            country: None,
        };
        assert_eq!(berlin.label(), "Berlin", "the area is not said twice");
        assert!(parse_geocode("{}").is_err());
    }

    #[test]
    fn a_place_search_sends_the_text_the_bias_and_the_language() {
        let params = geocode_query("Berlin Hbf", Some((52.5163, 13.3777)), "de");
        assert_eq!(param(&params, "text"), Some("Berlin Hbf"));
        assert_eq!(param(&params, "place"), Some("52.5163,13.3777"));
        assert_eq!(param(&params, "language"), Some("de"));
        assert_eq!(geocode_query("x", None, "en").len(), 2, "no bias without a place");
        assert_eq!(
            geocode_key(" Berlin HBF ", Some((52.51, 13.37)), "de"),
            geocode_key("berlin hbf", Some((52.53, 13.39)), "de"),
            "the same search near the same town"
        );
        assert_ne!(geocode_key("berlin hbf", None, "de"), geocode_key("berlin hbf", None, "en"));
    }

    #[test]
    fn the_address_of_a_place_is_its_nearest_one() {
        let found = parse_reverse(REVERSE).expect("an answer").expect("an address");
        assert_eq!(found.kind, FoundKind::Address);
        assert_eq!(found.label(), "Pariser Platz 1, Berlin");
        assert_eq!(parse_reverse("[]").expect("an answer"), None);
        let params = reverse_query((52.5163, 13.3777));
        assert_eq!(param(&params, "place"), Some("52.5163,13.3777"));
        assert_eq!(param(&params, "type"), Some("ADDRESS"));
        assert_eq!(reverse_key((52.51631, 13.37771)), reverse_key((52.51629, 13.37769)));
    }

    #[cfg(feature = "http")]
    mod server {
        use std::sync::Arc;

        use super::*;
        use crate::{cache::ResponseCache, test_server::Server, transit::TransitProvider};

        const AGENT: &str = "AzMaps-test/1 (+https://github.com/fschutt/azul)";

        #[test]
        fn journeys_are_asked_once_and_answered_from_the_cache_the_second_time() {
            let server = Server::start(|seen| {
                if seen.target.starts_with("/api/v6/plan?") {
                    (200, vec![], PLAN.to_string())
                } else {
                    (404, vec![], String::new())
                }
            });
            let transitous = Transitous::new(&server.base, AGENT, Arc::new(ResponseCache::in_memory(16)));
            let ask = query(When::DepartAt(1_792_047_600));
            let page = transitous.plan(&ask, &mut || false).expect("journeys").expect("not stopped");
            assert_eq!(page.journeys.len(), 2);
            assert_eq!(page.origin, Origin::Network);
            let again = transitous.plan(&ask, &mut || false).expect("journeys").expect("not stopped");
            assert_eq!(again.origin, Origin::Cache);
            assert_eq!(server.count(), 1);
            let seen = &server.requests()[0];
            assert_eq!(seen.header("user-agent"), Some(AGENT));
            assert_eq!(seen.query("fromPlace").as_deref(), Some("52.524980,13.369114"));
            assert_eq!(transitous.key(), "transitous");
            assert!(transitous.credit().contains("Transitous"));
            assert_eq!(transitous.credit_url(), SOURCES);
        }

        #[test]
        fn offline_the_kept_journeys_answer_marked_offline() {
            let server = Server::start(|_| (200, vec![], PLAN.to_string()));
            let cache = Arc::new(ResponseCache::in_memory(16));
            let online = Transitous::new(&server.base, AGENT, cache.clone());
            let ask = query(When::DepartAt(1_792_047_600));
            let _ = online.plan(&ask, &mut || false);
            // The next session's server is gone, and the kept answer is old.
            let key = plan_key(&ask, crate::cache::now_s() as i64);
            let mut entry = cache.get(&key).expect("kept");
            entry.stored_at = 0;
            cache.put(&key, entry);
            let offline = Transitous::new(&crate::test_server::closed_port(), AGENT, cache);
            let page = offline.plan(&ask, &mut || false).expect("kept journeys").expect("not stopped");
            assert_eq!(page.origin, Origin::Offline);
            assert_eq!(page.journeys.len(), 2);
        }

        #[test]
        fn a_place_search_and_an_address_are_asked_once() {
            let server = Server::start(|seen| {
                if seen.target.starts_with("/api/v1/geocode?") {
                    (200, vec![], GEOCODE.to_string())
                } else if seen.target.starts_with("/api/v1/reverse-geocode?") {
                    (200, vec![], REVERSE.to_string())
                } else {
                    (404, vec![], String::new())
                }
            });
            let transitous = Transitous::new(&server.base, AGENT, Arc::new(ResponseCache::in_memory(16)));
            let (found, origin) = transitous
                .geocode("Berlin Hbf", None, "de", &mut || false)
                .expect("places")
                .expect("not stopped");
            assert_eq!(found[0].name, "Berlin Hbf");
            assert_eq!(origin, Origin::Network);
            let (_, again) = transitous
                .geocode("berlin hbf", None, "de", &mut || false)
                .expect("places")
                .expect("not stopped");
            assert_eq!(again, Origin::Cache);
            let (address, _) = transitous
                .reverse((52.5163, 13.3777), &mut || false)
                .expect("an address")
                .expect("not stopped");
            assert_eq!(address.map(|a| a.label()).as_deref(), Some("Pariser Platz 1, Berlin"));
            assert_eq!(server.count(), 2);
        }
    }
}
