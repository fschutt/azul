//! The world clock: cities with their IANA zones (chrono-tz's database, so
//! every city follows its own daylight-saving rules - the plan's "fixed
//! sample offsets" are gone), how far each is from here, whether it is day
//! or night there, and the search the "+ City" dialog runs over every zone.
//!
//! The list is `clock/world.json`: `{"cities":[{"name":"Tokyo","zone":"Asia/Tokyo"}]}`
//! (order = the list's order).

use chrono::{DateTime, Offset, TimeZone, Timelike, Utc};
use chrono_tz::{OffsetName, Tz, TZ_VARIANTS};
use serde::{Deserialize, Serialize};

/// The zone areas the search offers (not `Etc/GMT+5`, `US/Eastern`, `EST5EDT`).
const AREAS: [&str; 9] = [
    "Africa/",
    "America/",
    "Antarctica/",
    "Asia/",
    "Atlantic/",
    "Australia/",
    "Europe/",
    "Indian/",
    "Pacific/",
];

/// One city of the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct City {
    /// "New York" (the user may rename it).
    pub name: String,
    /// "America/New_York".
    pub zone: String,
}

impl City {
    /// The city a zone names: `America/New_York` is "New York".
    #[must_use]
    pub fn of_zone(zone: &str) -> City {
        City {
            name: city_name(zone),
            zone: zone.to_string(),
        }
    }
}

/// The world clock's file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldFile {
    #[serde(default)]
    pub cities: Vec<City>,
}

/// One row of the list, as it reads at an instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityRow {
    pub name: String,
    pub hour: u32,
    pub minute: u32,
    /// The city's offset minus the local one, in minutes.
    pub difference_min: i32,
    /// The city's date minus the local date, in days (-1, 0, 1).
    pub day_offset: i64,
    /// 06:00 to 18:00 there.
    pub daytime: bool,
    /// "CEST", "EDT", "+09" (as the database names it).
    pub abbreviation: String,
}

/// The zone of an IANA name.
#[must_use]
pub fn parse_zone(name: &str) -> Option<Tz> {
    name.trim().parse::<Tz>().ok()
}

/// The device's zone, by name (`None` when the OS does not say or names a
/// zone the database does not know).
#[must_use]
pub fn local_zone() -> Option<Tz> {
    iana_time_zone::get_timezone().ok().and_then(|name| parse_zone(&name))
}

/// The city a zone's name ends with: `America/Argentina/Buenos_Aires` is
/// "Buenos Aires".
#[must_use]
pub fn city_name(zone: &str) -> String {
    zone.rsplit('/').next().unwrap_or(zone).replace('_', " ")
}

/// The offset from UTC of `tz` at `at`, in minutes.
#[must_use]
pub fn offset_minutes<Z: TimeZone>(tz: &Z, at: DateTime<Utc>) -> i32 {
    tz.offset_from_utc_datetime(&at.naive_utc()).fix().local_minus_utc() / 60
}

/// The zone's abbreviation at `at` ("CEST" in summer, "CET" in winter).
#[must_use]
pub fn abbreviation(tz: Tz, at: DateTime<Utc>) -> String {
    tz.offset_from_utc_datetime(&at.naive_utc())
        .abbreviation()
        .unwrap_or("")
        .to_string()
}

/// Day there: 06:00 to 18:00.
#[must_use]
pub fn is_day(hour: u32) -> bool {
    (6..18).contains(&hour)
}

/// "Yesterday", "Today", "Tomorrow" for a day offset.
#[must_use]
pub fn day_label(day_offset: i64) -> &'static str {
    match day_offset {
        d if d < 0 => "Yesterday",
        0 => "Today",
        _ => "Tomorrow",
    }
}

/// How `city` reads at `now` from a place in `local`; `None` for a zone the
/// database does not know.
#[must_use]
pub fn row<L: TimeZone>(city: &City, local: &L, now: DateTime<Utc>) -> Option<CityRow> {
    let tz = parse_zone(&city.zone)?;
    let there = now.with_timezone(&tz);
    let here = now.with_timezone(local);
    Some(CityRow {
        name: city.name.clone(),
        hour: there.hour(),
        minute: there.minute(),
        difference_min: offset_minutes(&tz, now) - offset_minutes(local, now),
        day_offset: (there.date_naive() - here.date_naive()).num_days(),
        daytime: is_day(there.hour()),
        abbreviation: abbreviation(tz, now),
    })
}

/// The cities whose name or zone has every word of `query` (any case,
/// diacritics folded), by name, at most `limit`.
#[must_use]
pub fn search(query: &str, limit: usize) -> Vec<City> {
    let query = azul_pim::search::Query::parse(query);
    if query.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<City> = TZ_VARIANTS
        .iter()
        .map(|tz| tz.name())
        .filter(|name| AREAS.iter().any(|area| name.starts_with(area)))
        .map(City::of_zone)
        .filter(|city| query.matches(&format!("{} {}", city.name, city.zone)))
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.zone.cmp(&b.zone)));
    found.truncate(limit);
    found
}

/// The plan's sample cities (section 6), after the local one.
#[must_use]
pub fn sample_cities() -> Vec<City> {
    ["Atlantic/Reykjavik", "America/New_York", "Asia/Tokyo", "Australia/Sydney"]
        .iter()
        .map(|z| City::of_zone(z))
        .collect()
}

/// Moves city `i` one place down (`down`) or up - its menu's Move down / Move up:
/// `false` when it cannot (the first up, the last down, no such city).
pub fn step_city(_cities: &mut Vec<City>, _i: usize, _down: bool) -> bool {
    false
}

/// Moves the city at `from` to `to` (a drag, or the Up / Down keys).
pub fn move_city(cities: &mut Vec<City>, from: usize, to: usize) {
    if from >= cities.len() {
        return;
    }
    let city = cities.remove(from);
    let to = to.min(cities.len());
    cities.insert(to, city);
}

#[cfg(test)]
mod tests {
    use chrono_tz::Europe::Berlin;

    use super::*;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    fn zone(name: &str) -> Tz {
        parse_zone(name).unwrap()
    }

    #[test]
    fn each_zone_follows_its_own_rules() {
        let at = utc(2026, 9, 15, 12, 32);
        let offsets: Vec<i32> = ["Europe/Berlin", "Atlantic/Reykjavik", "America/New_York", "Asia/Tokyo", "Australia/Sydney"]
            .iter()
            .map(|z| offset_minutes(&zone(z), at))
            .collect();
        assert_eq!(offsets, vec![120, 0, -240, 540, 600]);
        assert_eq!(abbreviation(Berlin, at), "CEST");
        assert_eq!(abbreviation(Berlin, utc(2026, 1, 15, 12, 0)), "CET");
    }

    #[test]
    fn the_sample_cities_read_as_the_plan_draws_them_from_berlin() {
        let at = utc(2026, 9, 15, 12, 32);
        let rows: Vec<CityRow> = sample_cities().iter().filter_map(|c| row(c, &Berlin, at)).collect();
        let lines: Vec<String> = rows
            .iter()
            .map(|r| {
                format!(
                    "{} {:02}:{:02} {} {}",
                    r.name,
                    r.hour,
                    r.minute,
                    crate::fmt::offset_difference(r.difference_min),
                    if r.daytime { "day" } else { "night" }
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                "Reykjavik 12:32 -2 h day",
                "New York 08:32 -6 h day",
                "Tokyo 21:32 +7 h night",
                "Sydney 22:32 +8 h night",
            ]
        );
    }

    #[test]
    fn the_difference_changes_when_only_one_side_changes_its_clocks() {
        // Berlin is back on CET on 25 October 2026, New York on EST only on 1 November.
        let ny = City::of_zone("America/New_York");
        assert_eq!(row(&ny, &Berlin, utc(2026, 10, 28, 12, 0)).unwrap().difference_min, -300);
        // Sydney moved to AEDT on 4 October 2026, Berlin still on CEST.
        let sydney = City::of_zone("Australia/Sydney");
        assert_eq!(row(&sydney, &Berlin, utc(2026, 10, 10, 12, 0)).unwrap().difference_min, 540);
    }

    #[test]
    fn a_city_past_midnight_is_tomorrow_and_one_before_it_yesterday() {
        let tokyo = City::of_zone("Asia/Tokyo");
        let ny = City::of_zone("America/New_York");
        let r = row(&tokyo, &Berlin, utc(2026, 9, 15, 20, 0)).unwrap(); // Berlin 22:00
        assert_eq!((r.hour, r.day_offset), (5, 1));
        assert_eq!(day_label(r.day_offset), "Tomorrow");
        assert!(!r.daytime, "05:00 is night");
        let r = row(&ny, &Berlin, utc(2026, 9, 15, 23, 30)).unwrap(); // Berlin 01:30 the next day
        assert_eq!((r.hour, r.minute, r.day_offset), (19, 30, -1));
        assert_eq!(day_label(r.day_offset), "Yesterday");
        assert_eq!(day_label(0), "Today");
        assert!(row(&City::of_zone("Mars/Olympus_Mons"), &Berlin, utc(2026, 9, 15, 0, 0)).is_none());
    }

    #[test]
    fn a_city_is_the_last_part_of_its_zone() {
        assert_eq!(city_name("America/Argentina/Buenos_Aires"), "Buenos Aires");
        assert_eq!(city_name("Europe/Berlin"), "Berlin");
        assert_eq!(city_name("UTC"), "UTC");
    }

    #[test]
    fn the_search_finds_cities_by_any_word_of_their_name_or_zone() {
        let found = search("york", 20);
        assert!(found.contains(&City::of_zone("America/New_York")), "{found:?}");
        let buenos = search("buenos", 20);
        assert_eq!(buenos.first().map(|c| c.name.as_str()), Some("Buenos Aires"));
        assert!(search("etc", 50).iter().all(|c| !c.zone.starts_with("Etc/")), "no Etc/ zones");
        assert!(search("eastern", 50).iter().all(|c| c.zone != "US/Eastern"), "no legacy links");
        let europe = search("europe", 10);
        assert_eq!(europe.len(), 10, "limited");
        let names: Vec<&str> = europe.iter().map(|c| c.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "by name");
        assert!(search("", 10).is_empty(), "an empty query offers nothing");
    }

    #[test]
    fn a_city_steps_down_past_the_next_one_and_up_past_the_one_before() {
        let mut cities = sample_cities();
        assert!(step_city(&mut cities, 0, true), "Reykjavik moves down");
        let names: Vec<&str> = cities.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["New York", "Reykjavik", "Tokyo", "Sydney"]);
        assert!(step_city(&mut cities, 3, false), "Sydney moves up");
        let names: Vec<&str> = cities.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["New York", "Reykjavik", "Sydney", "Tokyo"]);
        assert!(!step_city(&mut cities, 3, true), "the last stays last");
        assert!(!step_city(&mut cities, 0, false), "the first stays first");
        assert!(!step_city(&mut cities, 9, true), "no such city");
        let names: Vec<&str> = cities.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["New York", "Reykjavik", "Sydney", "Tokyo"]);
    }

    #[test]
    fn a_city_moves_up_and_down_the_list() {
        let mut cities = sample_cities();
        move_city(&mut cities, 3, 0);
        assert_eq!(cities[0].name, "Sydney");
        assert_eq!(cities[1].name, "Reykjavik");
        move_city(&mut cities, 0, 9);
        assert_eq!(cities[3].name, "Sydney", "past the end: last");
    }

    #[test]
    fn the_world_file_round_trips() {
        let file = WorldFile {
            cities: sample_cities(),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.starts_with(r#"{"cities":[{"name":"Reykjavik","zone":"Atlantic/Reykjavik"}"#), "{json}");
        assert_eq!(serde_json::from_str::<WorldFile>(&json).unwrap(), file);
        assert_eq!(serde_json::from_str::<WorldFile>("{}").unwrap(), WorldFile::default());
    }
}
