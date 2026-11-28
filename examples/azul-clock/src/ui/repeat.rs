//! Which weekdays an alarm repeats on: the seven chips of the editor and the RRULE text the alarm keeps.
//!
//! An alarm rings once, every day, or on the weekdays that are lit (what Android's clock offers); the
//! presets above the chips light several at once. A rule the chips cannot say (monthly, yearly, every
//! second week, a rule a version before the chips saved) lights no day and stays as it is until a chip
//! or a preset is touched.

use azul_pim::dates::{weekday_code, weekday_from_code};
use chrono::Weekday;

/// The week of the chips, Monday first: bit 0 is Monday, bit 6 Sunday.
pub const WEEK: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

/// Every weekday lit.
pub const EVERY_DAY: u8 = 0b111_1111;

/// The weekdays an RRULE repeats on, one bit each (Monday = bit 0): every day for a plain daily rule,
/// the `BYDAY` days of a plain weekly one, none for no repeat and for what a chip cannot say.
#[must_use]
pub fn days_of(rrule: &str) -> u8 {
    let rrule = rrule.trim();
    let part = |key: &str| {
        rrule
            .split(';')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.trim())
    };
    // Every day, every week: an interval other than 1 or a count or an end is something else.
    let plain = part("INTERVAL").is_none_or(|i| i == "1")
        && part("COUNT").is_none()
        && part("UNTIL").is_none()
        && part("BYMONTH").is_none()
        && part("BYMONTHDAY").is_none();
    match part("FREQ") {
        Some(f) if plain && f.eq_ignore_ascii_case("DAILY") && part("BYDAY").is_none() => EVERY_DAY,
        Some(f) if plain && f.eq_ignore_ascii_case("WEEKLY") => part("BYDAY").map_or(0, |list| {
            list.split(',')
                .filter_map(weekday_from_code)
                .fold(0, |days, d| days | (1 << d.num_days_from_monday()))
        }),
        _ => 0,
    }
}

/// The RRULE text of the lit weekdays: empty (once) for none, `FREQ=DAILY` for all seven, else a weekly
/// rule on those days.
#[must_use]
pub fn rrule_of(days: u8) -> String {
    match days & EVERY_DAY {
        0 => String::new(),
        EVERY_DAY => "FREQ=DAILY".to_string(),
        lit => {
            let names: Vec<&str> = WEEK
                .iter()
                .filter(|d| lit & (1 << d.num_days_from_monday()) != 0)
                .map(|d| weekday_code(*d))
                .collect();
            format!("FREQ=WEEKLY;BYDAY={}", names.join(","))
        }
    }
}

/// `days` with the weekday `chip` (0 = Monday .. 6 = Sunday) lit if it was out and out if it was lit;
/// a number past Sunday changes nothing.
#[must_use]
pub const fn toggled(days: u8, chip: u8) -> u8 {
    if chip >= 7 {
        days & EVERY_DAY
    } else {
        (days ^ (1 << chip)) & EVERY_DAY
    }
}

#[cfg(test)]
#[path = "repeat_test.rs"]
mod repeat_test;
