//! Public transport: journeys from a place to a place at a time, as a timetable service plans
//! them. A journey is a list of legs - walk to the stop, the S5 to Berlin Hbf, change platforms,
//! the ICE to Leipzig, walk on - each with its line, its stops, their platforms and times, live
//! where the service knows better than the timetable.
//!
//! A [`TransitProvider`] plans journeys. [`transitous::Transitous`] (the community-run MOTIS
//! instance of transitous.org, Europe-wide and beyond from public GTFS feeds, Deutsche Bahn's
//! trains included) is the default; other providers plug in behind the same trait. Only the two
//! places, the time and the language are sent.

pub mod transitous;

use crate::fetch::Origin;

/// When a journey is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum When {
    /// Leaving now.
    #[default]
    Now,
    /// Leaving at this time, unix seconds.
    DepartAt(i64),
    /// Arriving by this time, unix seconds.
    ArriveBy(i64),
}

/// A question for a provider.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JourneyQuery {
    /// The start, `(lat, lon)`.
    pub from: (f64, f64),
    /// The destination, `(lat, lon)`.
    pub to: (f64, f64),
    pub when: When,
    /// The provider's cursor of an earlier or a later page of the same question
    /// ([`JourneyPage::earlier`], [`JourneyPage::later`]).
    pub page: Option<String>,
    /// The language stop names and notes come in (`de`, `en`).
    pub language: String,
}

/// How a leg goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegKind {
    Walk,
    Bike,
    Car,
    /// ICE, TGV, ...
    HighSpeedTrain,
    /// IC, EC, ...
    LongDistanceTrain,
    NightTrain,
    /// RE, RB, ...
    RegionalTrain,
    /// S-Bahn, RER, ...
    Suburban,
    Subway,
    Tram,
    Bus,
    Coach,
    Ferry,
    Airplane,
    /// Funiculars and aerial lifts.
    CableCar,
    /// On-demand rides, shared rides, flexible services.
    OnDemand,
    Other,
}

impl LegKind {
    /// The kind of a mode in MOTIS' words (`HIGHSPEED_RAIL`, `WALK`, ...).
    #[must_use]
    pub fn of_mode(mode: &str) -> Self {
        match mode {
            "WALK" => Self::Walk,
            "BIKE" | "RENTAL" => Self::Bike,
            "CAR" | "CAR_PARKING" | "CAR_DROPOFF" | "HGV" => Self::Car,
            "HIGHSPEED_RAIL" => Self::HighSpeedTrain,
            "LONG_DISTANCE" => Self::LongDistanceTrain,
            "NIGHT_RAIL" => Self::NightTrain,
            "RAIL" | "REGIONAL_RAIL" | "REGIONAL_FAST_RAIL" => Self::RegionalTrain,
            "SUBURBAN" => Self::Suburban,
            "SUBWAY" | "METRO" => Self::Subway,
            "TRAM" => Self::Tram,
            "BUS" => Self::Bus,
            "COACH" => Self::Coach,
            "FERRY" => Self::Ferry,
            "AIRPLANE" => Self::Airplane,
            "FUNICULAR" | "AERIAL_LIFT" | "AREAL_LIFT" | "CABLE_CAR" => Self::CableCar,
            "ODM" | "RIDE_SHARING" | "FLEX" => Self::OnDemand,
            _ => Self::Other,
        }
    }

    /// Whether it is a ride on a timetable (else the traveller moves on their own).
    #[must_use]
    pub const fn is_transit(self) -> bool {
        !matches!(self, Self::Walk | Self::Bike | Self::Car)
    }

    /// The Material icon of the kind.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Walk => "directions_walk",
            Self::Bike => "directions_bike",
            Self::Car => "directions_car",
            Self::HighSpeedTrain | Self::LongDistanceTrain | Self::NightTrain | Self::RegionalTrain => "train",
            Self::Suburban => "directions_railway",
            Self::Subway => "subway",
            Self::Tram => "tram",
            Self::Bus | Self::Coach => "directions_bus",
            Self::Ferry => "directions_boat",
            Self::Airplane => "flight",
            Self::OnDemand => "local_taxi",
            Self::CableCar | Self::Other => "directions_transit",
        }
    }

    /// The word scripts read (`AZMAPS_LEG <kind> ...`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Bike => "bike",
            Self::Car => "car",
            Self::HighSpeedTrain => "highspeed",
            Self::LongDistanceTrain => "longdistance",
            Self::NightTrain => "night",
            Self::RegionalTrain => "regional",
            Self::Suburban => "suburban",
            Self::Subway => "subway",
            Self::Tram => "tram",
            Self::Bus => "bus",
            Self::Coach => "coach",
            Self::Ferry => "ferry",
            Self::Airplane => "airplane",
            Self::CableCar => "cablecar",
            Self::OnDemand => "ondemand",
            Self::Other => "other",
        }
    }

    /// The line colour of a leg of this kind whose feed names none, `(r, g, b)`: trains red,
    /// suburban trains green, subways blue, trams a deep red, buses purple, walking grey.
    #[must_use]
    pub const fn colour(self) -> (u8, u8, u8) {
        match self {
            Self::Walk => (0x80, 0x86, 0x8b),
            Self::Bike => (0x2e, 0x7d, 0x32),
            Self::Car => (0x42, 0x42, 0x42),
            Self::HighSpeedTrain | Self::LongDistanceTrain | Self::NightTrain => (0xd3, 0x2f, 0x2f),
            Self::RegionalTrain => (0xc6, 0x28, 0x28),
            Self::Suburban => (0x00, 0x8d, 0x4f),
            Self::Subway => (0x11, 0x5d, 0xa8),
            Self::Tram => (0xb7, 0x1c, 0x1c),
            Self::Bus | Self::Coach => (0x8e, 0x24, 0xaa),
            Self::Ferry => (0x00, 0x83, 0x8f),
            Self::Airplane => (0x37, 0x47, 0x4f),
            Self::CableCar | Self::OnDemand | Self::Other => (0x5d, 0x40, 0x37),
        }
    }

    /// Whether its line on the map is dotted: the parts walked, not ridden.
    #[must_use]
    pub const fn dotted(self) -> bool {
        matches!(self, Self::Walk)
    }
}

/// A stop of a leg (or the start or the destination).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stop {
    pub name: String,
    pub at: (f64, f64),
    /// The platform or track, live where known.
    pub platform: Option<String>,
    /// The platform the timetable has.
    pub scheduled_platform: Option<String>,
}

impl Stop {
    /// Whether the train leaves from another platform than the timetable says.
    #[must_use]
    pub fn platform_changed(&self) -> bool {
        match (&self.platform, &self.scheduled_platform) {
            (Some(now), Some(planned)) => now.trim() != planned.trim(),
            _ => false,
        }
    }
}

/// One leg of a journey.
#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    pub kind: LegKind,
    /// The line as it is signed (`ICE 597`, `S5`); empty for walking.
    pub line: String,
    /// Where the vehicle goes (`München Hbf`); empty for walking.
    pub headsign: String,
    /// Who runs it (`DB Fernverkehr AG`).
    pub operator: String,
    pub from: Stop,
    pub to: Stop,
    /// When it leaves and arrives, unix seconds, live where known.
    pub departure: i64,
    pub arrival: i64,
    /// When the timetable has it leave and arrive.
    pub scheduled_departure: i64,
    pub scheduled_arrival: i64,
    /// Whether the times are live.
    pub realtime: bool,
    pub cancelled: bool,
    /// How far it goes, metres, where the provider says (walking).
    pub distance_m: Option<f64>,
    /// The line's own colour, where the feed has one.
    pub colour: Option<(u8, u8, u8)>,
    /// Its way on the map, `(lat, lon)`.
    pub points: Vec<(f64, f64)>,
}

impl Leg {
    /// How many minutes late it leaves (negative: early).
    #[must_use]
    pub fn departure_delay_min(&self) -> i64 {
        (self.departure - self.scheduled_departure).div_euclid(60)
    }

    /// How many minutes late it arrives.
    #[must_use]
    pub fn arrival_delay_min(&self) -> i64 {
        (self.arrival - self.scheduled_arrival).div_euclid(60)
    }

    /// How long it takes, seconds.
    #[must_use]
    pub fn duration_s(&self) -> i64 {
        self.arrival - self.departure
    }

    /// The colour its line is drawn in: the feed's, else its kind's.
    #[must_use]
    pub fn line_colour(&self) -> (u8, u8, u8) {
        self.colour.unwrap_or_else(|| self.kind.colour())
    }
}

/// A journey: its legs in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Journey {
    /// When it starts and ends, unix seconds.
    pub departure: i64,
    pub arrival: i64,
    pub transfers: u32,
    pub legs: Vec<Leg>,
}

impl Journey {
    /// How long it takes, seconds.
    #[must_use]
    pub fn duration_s(&self) -> i64 {
        self.arrival - self.departure
    }

    /// The legs ridden, not walked.
    pub fn rides(&self) -> impl Iterator<Item = &Leg> {
        self.legs.iter().filter(|leg| leg.kind.is_transit())
    }

    /// Whether a ride of it is late by `minutes` or more, cancelled, or moved to another platform.
    #[must_use]
    pub fn disrupted(&self, minutes: i64) -> bool {
        self.rides().any(|leg| {
            leg.cancelled
                || leg.departure_delay_min() >= minutes
                || leg.arrival_delay_min() >= minutes
                || leg.from.platform_changed()
                || leg.to.platform_changed()
        })
    }
}

/// The journeys a provider answered one question with.
#[derive(Debug, Clone, PartialEq)]
pub struct JourneyPage {
    pub journeys: Vec<Journey>,
    /// The cursor of the journeys before these, if there are any.
    pub earlier: Option<String>,
    /// The cursor of the journeys after these.
    pub later: Option<String>,
    /// Where the answer came from: a journey shown from the cache while offline says so.
    pub origin: Origin,
    /// How old the answer is, seconds.
    pub age_s: u64,
}

/// Why a provider has no journeys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitError {
    /// No network, no server, and nothing kept.
    Offline(String),
    /// The service asks to slow down, and nothing is kept.
    Busy,
    /// The service knows no journey (or no stop near a place).
    NoJourney(String),
    /// Anything else the service answered.
    Server(String),
}

/// A planner of public transport journeys.
pub trait TransitProvider: Send + Sync {
    /// The provider's word in scripts and settings (`transitous`).
    fn key(&self) -> &'static str;

    /// Who to credit for the journeys, as the panel shows it under them.
    fn credit(&self) -> &'static str;

    /// The page to link to from the credit (the provider's sources and licences).
    fn credit_url(&self) -> &'static str;

    /// The journeys for `query`; `Ok(None)` when `stop` said to give up first.
    ///
    /// # Errors
    ///
    /// [`TransitError`] when there are none to show.
    fn plan(
        &self,
        query: &JourneyQuery,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<JourneyPage>, TransitError>;
}

/// A time as MOTIS writes it (`2026-10-15T07:55:00Z`, RFC 3339) in unix seconds.
#[must_use]
pub fn parse_time(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|t| t.timestamp())
}

/// Unix seconds as MOTIS reads them (`2026-10-15T07:55:00Z`).
#[must_use]
pub fn format_time(unix: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// A colour as GTFS writes it (`eb7405`, `#EB7405`).
#[must_use]
pub fn parse_colour(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(name: &str) -> Stop {
        Stop {
            name: name.to_string(),
            ..Stop::default()
        }
    }

    fn ride(departure: i64, scheduled: i64) -> Leg {
        Leg {
            kind: LegKind::HighSpeedTrain,
            line: "ICE 91".to_string(),
            headsign: "Nürnberg Hbf".to_string(),
            operator: String::new(),
            from: stop("Berlin Hbf"),
            to: stop("Leipzig Hbf"),
            departure,
            arrival: departure + 4_620,
            scheduled_departure: scheduled,
            scheduled_arrival: scheduled + 4_620,
            realtime: true,
            cancelled: false,
            distance_m: None,
            colour: None,
            points: Vec::new(),
        }
    }

    #[test]
    fn motis_modes_are_leg_kinds() {
        assert_eq!(LegKind::of_mode("WALK"), LegKind::Walk);
        assert_eq!(LegKind::of_mode("HIGHSPEED_RAIL"), LegKind::HighSpeedTrain);
        assert_eq!(LegKind::of_mode("LONG_DISTANCE"), LegKind::LongDistanceTrain);
        assert_eq!(LegKind::of_mode("REGIONAL_FAST_RAIL"), LegKind::RegionalTrain);
        assert_eq!(LegKind::of_mode("REGIONAL_RAIL"), LegKind::RegionalTrain);
        assert_eq!(LegKind::of_mode("SUBURBAN"), LegKind::Suburban);
        assert_eq!(LegKind::of_mode("METRO"), LegKind::Subway);
        assert_eq!(LegKind::of_mode("TRAM"), LegKind::Tram);
        assert_eq!(LegKind::of_mode("COACH"), LegKind::Coach);
        assert_eq!(LegKind::of_mode("AERIAL_LIFT"), LegKind::CableCar);
        assert_eq!(LegKind::of_mode("ODM"), LegKind::OnDemand);
        assert_eq!(LegKind::of_mode("SOMETHING_NEW"), LegKind::Other);
        assert!(!LegKind::Walk.is_transit() && LegKind::Bus.is_transit());
        assert!(LegKind::Walk.dotted() && !LegKind::Tram.dotted());
    }

    #[test]
    fn a_late_ride_on_another_platform_is_a_disruption() {
        let on_time = ride(1_000, 1_000);
        assert_eq!(on_time.departure_delay_min(), 0);
        let late = ride(1_000 + 180, 1_000);
        assert_eq!(late.departure_delay_min(), 3);
        assert_eq!(late.arrival_delay_min(), 3);
        assert_eq!(late.duration_s(), 4_620);
        let journey = |leg: Leg| Journey {
            departure: leg.departure,
            arrival: leg.arrival,
            transfers: 0,
            legs: vec![leg],
        };
        assert!(!journey(on_time.clone()).disrupted(5));
        assert!(journey(late.clone()).disrupted(3));
        assert!(!journey(late).disrupted(5));
        let mut moved = on_time.clone();
        moved.from.platform = Some("5".to_string());
        moved.from.scheduled_platform = Some("3".to_string());
        assert!(moved.from.platform_changed());
        assert!(journey(moved).disrupted(5));
        let mut cancelled = on_time;
        cancelled.cancelled = true;
        assert!(journey(cancelled).disrupted(5));
    }

    #[test]
    fn a_platform_without_a_timetable_platform_has_not_changed() {
        let mut s = stop("Leipzig Hbf");
        s.platform = Some("12".to_string());
        assert!(!s.platform_changed());
        s.scheduled_platform = Some("12".to_string());
        assert!(!s.platform_changed());
    }

    #[test]
    fn times_and_colours_are_read_as_motis_and_gtfs_write_them() {
        assert_eq!(parse_time("2026-10-15T07:55:00Z"), Some(1_792_050_900));
        assert_eq!(parse_time("2026-10-15T09:55:00+02:00"), Some(1_792_050_900));
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(format_time(1_792_050_900), "2026-10-15T07:55:00Z");
        assert_eq!(parse_colour("eb7405"), Some((0xeb, 0x74, 0x05)));
        assert_eq!(parse_colour("#0066AD"), Some((0x00, 0x66, 0xad)));
        assert_eq!(parse_colour(""), None);
        assert_eq!(parse_colour("red"), None);
        let mut leg = ride(0, 0);
        assert_eq!(leg.line_colour(), LegKind::HighSpeedTrain.colour());
        leg.colour = Some((1, 2, 3));
        assert_eq!(leg.line_colour(), (1, 2, 3));
    }
}
