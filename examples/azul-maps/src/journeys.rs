//! Public transport in the travel panel: the journeys a timetable service (Transitous by
//! default, `--transit`) plans between the panel's two places at the time asked for. They are
//! asked for on a worker thread ([`journey_worker`]) the way a route is, a newer question
//! superseding an older one; the panel lists them with their legs and the picked one is drawn
//! on the map leg by leg.
//!
//! The words and times the panel shows are plain functions here, tested without a window.

use std::{
    sync::{atomic::AtomicU64, Arc},
    time::Instant,
};

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::l10n::{t_args, Arg};
use azul_routing::transit::{Journey, JourneyPage, JourneyQuery, Leg, LegKind, TransitError, TransitProvider, When};
use chrono::{DateTime, NaiveDate, TimeZone};

use crate::{model::distance_text, route::Stop};

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
    let Some((id, query, provider, latest, on_done)) = init.downcast_ref::<JourneyRequest>().map(|r| {
        (r.id, r.query.clone(), Arc::clone(&r.provider), Arc::clone(&r.latest), r.on_done)
    }) else {
        return;
    };
    let mut stop = Stop::new(id, &latest, &mut receiver);
    if stop.now() {
        return;
    }
    let started = Instant::now();
    let answer = match provider.plan(&query, &mut || stop.now()) {
        Ok(Some(page)) => Ok(page),
        Ok(None) => return,
        Err(why) => Err(why),
    };
    if stop.now() {
        return;
    }
    let reply = JourneyReply {
        id,
        query,
        answer,
        credit: provider.credit(),
        credit_url: provider.credit_url(),
        compute_ms: started.elapsed().as_secs_f64() * 1000.0,
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(reply),
    )));
}

/// `unix` as the clock in `tz` shows it: `07:55`.
#[must_use]
pub fn clock_in<Tz: TimeZone>(unix: i64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    tz.timestamp_opt(unix, 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// `8:30`, `08:30`, `0830` or `830` as hours and minutes.
fn hours_minutes(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    let (h, m) = match text.split_once(':') {
        Some(parts) => parts,
        None if (3..=4).contains(&text.len()) && text.is_ascii() => text.split_at(text.len() - 2),
        None => return None,
    };
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
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
    let text = text.trim();
    let zone = now.timezone();
    if let Some((date, time)) = text.split_once(' ') {
        if let (Ok(day), Some((h, m))) = (NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d"), hours_minutes(time)) {
            let local = day.and_hms_opt(h, m, 0)?;
            return zone.from_local_datetime(&local).earliest().map(|t| t.timestamp());
        }
    }
    let (h, m) = hours_minutes(text)?;
    let local = now.date_naive().and_hms_opt(h, m, 0)?;
    let today = zone.from_local_datetime(&local).earliest()?.timestamp();
    // A time that passed more than an hour ago means tomorrow's.
    Some(if now.timestamp() - today > 3_600 { today + 86_400 } else { today })
}

/// The panel's time as a question's: now, or the typed time to leave at or arrive by.
#[must_use]
pub fn when_of<Tz: TimeZone>(kind: WhenKind, text: &str, now: &DateTime<Tz>) -> Option<When> {
    match kind {
        WhenKind::Now => Some(When::Now),
        WhenKind::DepartAt => parse_clock(text, now).map(When::DepartAt),
        WhenKind::ArriveBy => parse_clock(text, now).map(When::ArriveBy),
    }
}

/// A ride's delay as the panel shows it beside the time: `+3`; nothing when it is on time.
#[must_use]
pub fn delay_text(minutes: i64) -> Option<String> {
    match minutes {
        0 => None,
        m if m > 0 => Some(format!("+{m}")),
        m => Some(m.to_string()),
    }
}

/// How long a journey takes, exactly: `1 h 17 min`, `48 min`.
#[must_use]
pub fn span_text(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 30) / 60;
    let (h, m) = (minutes / 60, minutes % 60);
    let n = |v: i64| Arg::from(v.to_string());
    match (h, m) {
        (0, m) => t_args("azmaps-span-min", &[("min", n(m))]),
        (h, 0) => t_args("azmaps-span-h", &[("h", n(h))]),
        (h, m) => t_args("azmaps-span-h-min", &[("h", n(h)), ("min", n(m))]),
    }
}

/// How often a journey changes: `direct`, `1 change`, `2 changes`.
#[must_use]
pub fn transfers_text(transfers: u32) -> String {
    match transfers {
        0 => t_args("azmaps-transfers-none", &[]),
        1 => t_args("azmaps-transfers-one", &[]),
        n => t_args("azmaps-transfers-many", &[("count", Arg::from(n.to_string()))]),
    }
}

/// A leg's line in the list: `ICE 597 → München Hbf`, or `Walk 260 m`.
#[must_use]
pub fn leg_title(leg: &Leg) -> String {
    let own = match leg.kind {
        LegKind::Walk => Some("azmaps-leg-walk"),
        LegKind::Bike => Some("azmaps-leg-bike"),
        LegKind::Car => Some("azmaps-leg-car"),
        _ => None,
    };
    if let Some(key) = own {
        let distance = leg.distance_m.map(|m| distance_text(m / 1000.0)).unwrap_or_default();
        return t_args(key, &[("distance", Arg::from(distance))]).trim().to_string();
    }
    match (leg.line.trim(), leg.headsign.trim()) {
        ("", headsign) => headsign.to_string(),
        (line, "") => line.to_string(),
        (line, headsign) => format!("{line} \u{2192} {headsign}"),
    }
}

/// The journeys that have not left yet (a minute's grace), in the order of departure.
#[must_use]
pub fn upcoming(page: &JourneyPage, now: i64) -> Vec<&Journey> {
    let mut shown: Vec<&Journey> = page.journeys.iter().filter(|j| j.departure >= now - 60).collect();
    shown.sort_by_key(|j| j.departure);
    shown
}

/// The line `AZMAPS_JOURNEYS <n> <first departure> <first arrival> <transfers> <origin>` for
/// scripts, and `AZMAPS_LEG <kind> <line>` per leg of the first journey.
#[must_use]
pub fn script_lines(page: &JourneyPage) -> Vec<String> {
    let Some(first) = page.journeys.first() else {
        return vec![format!("AZMAPS_JOURNEYS 0 - - - {}", page.origin.key())];
    };
    let mut lines = vec![format!(
        "AZMAPS_JOURNEYS {} {} {} {} {}",
        page.journeys.len(),
        first.departure,
        first.arrival,
        first.transfers,
        page.origin.key()
    )];
    lines.extend(first.legs.iter().map(|leg| {
        let line = leg.line.trim();
        format!(
            "AZMAPS_LEG {} {}",
            leg.kind.key(),
            if line.is_empty() { "-".to_string() } else { line.replace(' ', "_") }
        )
    }));
    lines
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
