//! Public transport in the travel panel: the journeys a timetable service (Transitous by
//! default, `--transit`) plans between the panel's two places at the time asked for. They are
//! asked for on a worker thread ([`journey_worker`]) the way a route is, a newer question
//! superseding an older one; the panel lists them with their legs and the picked one is drawn
//! on the map leg by leg.
//!
//! The words and times the panel shows are plain functions here, tested without a window.

use std::sync::{atomic::AtomicU64, Arc};

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::l10n::{t_args, Arg};
use azul_routing::transit::{Journey, JourneyPage, JourneyQuery, Leg, LegKind, TransitError, TransitProvider, When};
use chrono::{DateTime, TimeZone};

use crate::route::Stop;

/// The kind of time the panel asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhenKind {
    #[default]
    Now,
    DepartAt,
    ArriveBy,
}

impl WhenKind {
    pub const ALL: [Self; 3] = [Self::Now, Self::DepartAt, Self::ArriveBy];

    /// The words of its choice in the panel (a key of AzMaps' resources).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Now => "azmaps-transit-now",
            Self::DepartAt => "azmaps-transit-depart",
            Self::ArriveBy => "azmaps-transit-arrive",
        }
    }
}

/// One journey question: what the panel asks.
pub struct JourneyRequest {
    /// This question's number; the newest one's is in `latest`.
    pub id: u64,
    pub query: JourneyQuery,
    /// Who plans the journeys.
    pub provider: Arc<dyn TransitProvider>,
    /// The newest question's number, shared with the app.
    pub latest: Arc<AtomicU64>,
    /// The write-back that hands the reply to the app.
    pub on_done: WriteBackCallbackType,
}

/// The journeys of a question, or why there are none.
#[derive(Debug, Clone)]
pub struct JourneyReply {
    /// The question's number.
    pub id: u64,
    pub query: JourneyQuery,
    pub answer: Result<JourneyPage, TransitError>,
    /// Who planned them, for the credit under them.
    pub credit: &'static str,
    /// The page the credit links to.
    pub credit_url: &'static str,
    /// How long the worker worked, ms.
    pub compute_ms: f64,
}

/// The journey worker, an azul `Thread`: asks the provider, sends a [`JourneyReply`] to
/// `on_done` unless a newer question came first.
pub extern "C" fn journey_worker(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let _ = (&mut init, &mut sender, &mut receiver, ThreadReceiveMsg::WriteBack, ThreadWriteBackMsg::create, Stop::new);
}

/// `unix` as the clock in `tz` shows it: `07:55`.
#[must_use]
pub fn clock_in<Tz: TimeZone>(unix: i64, tz: &Tz) -> String {
    let _ = (unix, tz);
    String::new()
}

/// `unix` as this device's clock shows it.
#[must_use]
pub fn clock(unix: i64) -> String {
    clock_in(unix, &chrono::Local)
}

/// The time a typed clock (`8:30`, `08:30`, `0830`, or `2026-10-15 08:30`) names, unix
/// seconds: today, or tomorrow when today's has passed by more than an hour; `None` for text
/// that is no time.
#[must_use]
pub fn parse_clock<Tz: TimeZone>(text: &str, now: &DateTime<Tz>) -> Option<i64> {
    let _ = (text, now);
    None
}

/// The panel's time as a question's: now, or the typed time to leave at or arrive by.
#[must_use]
pub fn when_of<Tz: TimeZone>(kind: WhenKind, text: &str, now: &DateTime<Tz>) -> Option<When> {
    let _ = (kind, text, now);
    None
}

/// A ride's delay as the panel shows it beside the time: `+3`; nothing when it is on time.
#[must_use]
pub fn delay_text(minutes: i64) -> Option<String> {
    let _ = minutes;
    None
}

/// How long a journey takes, exactly: `1 h 17 min`, `48 min`.
#[must_use]
pub fn span_text(seconds: i64) -> String {
    let _ = seconds;
    String::new()
}

/// How often a journey changes: `direct`, `1 change`, `2 changes`.
#[must_use]
pub fn transfers_text(transfers: u32) -> String {
    let _ = transfers;
    String::new()
}

/// A leg's line in the list: `ICE 597 → München Hbf`, or `Walk 260 m`.
#[must_use]
pub fn leg_title(leg: &Leg) -> String {
    let _ = (leg, LegKind::Walk);
    String::new()
}

/// The journeys that have not left yet (a minute's grace), in the order of departure.
#[must_use]
pub fn upcoming(page: &JourneyPage, now: i64) -> Vec<&Journey> {
    let _ = (page, now);
    Vec::new()
}

/// The line `AZMAPS_JOURNEYS <n> <first departure> <first arrival> <transfers> <origin>` for
/// scripts, and `AZMAPS_LEG <kind> <line>` per leg of the first journey.
#[must_use]
pub fn script_lines(page: &JourneyPage) -> Vec<String> {
    let _ = (page, t_args("azmaps-transit-now", &[]), Arg::from(String::new()));
    Vec::new()
}

#[cfg(test)]
mod tests {
    use azul_routing::{fetch::Origin, transit::Stop as TransitStop};
    use chrono::FixedOffset;

    use super::*;

    fn berlin() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).expect("an offset")
    }

    /// 2026-10-15 09:00 in Berlin (07:00 UTC).
    fn nine() -> DateTime<FixedOffset> {
        berlin().timestamp_opt(1_792_047_600, 0).single().expect("a time")
    }

    fn leg(kind: LegKind, line: &str, headsign: &str, distance_m: Option<f64>) -> Leg {
        Leg {
            kind,
            line: line.to_string(),
            headsign: headsign.to_string(),
            operator: String::new(),
            from: TransitStop::default(),
            to: TransitStop::default(),
            departure: 0,
            arrival: 0,
            scheduled_departure: 0,
            scheduled_arrival: 0,
            realtime: false,
            cancelled: false,
            distance_m,
            colour: None,
            points: Vec::new(),
        }
    }

    #[test]
    fn a_time_shows_on_the_local_clock() {
        assert_eq!(clock_in(1_792_050_900, &berlin()), "09:55");
        assert_eq!(clock_in(1_792_050_900, &FixedOffset::east_opt(0).expect("utc")), "07:55");
    }

    #[test]
    fn a_typed_time_is_today_or_tomorrow() {
        let now = nine();
        assert_eq!(parse_clock("10:30", &now), Some(1_792_047_600 + 90 * 60), "later today");
        assert_eq!(parse_clock("8:30", &now), Some(1_792_047_600 - 30 * 60), "half an hour ago: today");
        assert_eq!(parse_clock("0630", &now), Some(1_792_047_600 + 21 * 3600 + 30 * 60), "hours ago: tomorrow");
        assert_eq!(parse_clock("2026-10-16 07:15", &now), Some(1_792_047_600 + 22 * 3600 + 15 * 60));
        assert_eq!(parse_clock("25:00", &now), None);
        assert_eq!(parse_clock("soon", &now), None);
        assert_eq!(parse_clock("", &now), None);
    }

    #[test]
    fn the_panels_time_is_the_questions() {
        let now = nine();
        assert_eq!(when_of(WhenKind::Now, "whatever", &now), Some(When::Now));
        assert_eq!(when_of(WhenKind::DepartAt, "10:30", &now), Some(When::DepartAt(1_792_047_600 + 5_400)));
        assert_eq!(when_of(WhenKind::ArriveBy, "12:00", &now), Some(When::ArriveBy(1_792_047_600 + 3 * 3600)));
        assert_eq!(when_of(WhenKind::ArriveBy, "noon", &now), None, "no time, no question");
    }

    #[test]
    fn delays_spans_and_changes_read_short() {
        crate::l10n::in_english();
        assert_eq!(delay_text(0), None);
        assert_eq!(delay_text(3).as_deref(), Some("+3"));
        assert_eq!(delay_text(-1).as_deref(), Some("-1"));
        assert_eq!(span_text(4_620), "1 h 17 min");
        assert_eq!(span_text(2_880), "48 min");
        assert_eq!(span_text(7_200), "2 h");
        assert_eq!(transfers_text(0), "direct");
        assert_eq!(transfers_text(1), "1 change");
        assert_eq!(transfers_text(2), "2 changes");
    }

    #[test]
    fn a_leg_is_titled_by_its_line_and_where_it_goes() {
        crate::l10n::in_english();
        assert_eq!(leg_title(&leg(LegKind::HighSpeedTrain, "ICE 597", "München Hbf", None)), "ICE 597 \u{2192} München Hbf");
        assert_eq!(leg_title(&leg(LegKind::Walk, "", "", Some(260.0))), "Walk 260 m");
        assert_eq!(leg_title(&leg(LegKind::Bus, "M41", "", None)), "M41");
    }

    #[test]
    fn journeys_that_left_are_not_shown_and_scripts_read_the_first() {
        let mut ice = leg(LegKind::HighSpeedTrain, "ICE 597", "München Hbf", None);
        ice.departure = 1_000;
        ice.arrival = 5_500;
        let journey = |departure: i64| Journey {
            departure,
            arrival: departure + 4_500,
            transfers: 0,
            legs: vec![ice.clone()],
        };
        let page = JourneyPage {
            journeys: vec![journey(2_000), journey(900), journey(1_000)],
            earlier: None,
            later: None,
            origin: Origin::Offline,
            age_s: 0,
        };
        let shown: Vec<i64> = upcoming(&page, 1_030).iter().map(|j| j.departure).collect();
        assert_eq!(shown, [1_000, 2_000], "left half a minute ago is still shown; left 2 min ago is not");
        let lines = script_lines(&page);
        assert!(lines[0].starts_with("AZMAPS_JOURNEYS 3 "), "{lines:?}");
        assert!(lines[0].ends_with(" offline"), "{lines:?}");
        assert_eq!(lines[1], "AZMAPS_LEG highspeed ICE_597");
    }
}
