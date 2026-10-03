//! How times and durations read in AzClock: the stopwatch's `00:04:17.36`,
//! a lap's `00:51.12`, the timer's `05:48`, the footer's "in 15 h 58 min",
//! a city's "+7 h".

/// The stopwatch's big text: hours, minutes, seconds and hundredths
/// (`00:04:17.36`), always with the hours.
#[must_use]
pub fn stopwatch(ms: i64) -> String {
    let _ = ms;
    String::new()
}

/// A lap or a lap's total: `00:51.12`; with hours once there are some
/// (`1:02:03.45`).
#[must_use]
pub fn lap(ms: i64) -> String {
    let _ = ms;
    String::new()
}

/// A timer's time left: whole seconds, rounded UP (it shows `00:00` only
/// when it is done): `05:48`, `1:05:48`.
#[must_use]
pub fn countdown(ms: i64) -> String {
    let _ = ms;
    String::new()
}

/// A time of day: `06:30` (24 h) or `6:30 AM` (12 h).
#[must_use]
pub fn clock(hour: u32, minute: u32, twelve_hour: bool) -> String {
    let _ = (hour, minute, twelve_hour);
    String::new()
}

/// How long until something: "less than a minute", "3 min", "15 h 58 min",
/// "1 day 2 h", "3 days".
#[must_use]
pub fn until(ms: i64) -> String {
    let _ = ms;
    String::new()
}

/// A city's offset from here, in minutes: "same time", "+7 h", "-6 h",
/// "+5:30 h", "-2:30 h".
#[must_use]
pub fn offset_difference(minutes: i32) -> String {
    let _ = minutes;
    String::new()
}

/// A preset's or a length's label: "1 min", "25 min", "1 h", "1 h 30 min".
#[must_use]
pub fn minutes(total: u32) -> String {
    let _ = total;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stopwatch_shows_hundredths_and_always_the_hours() {
        assert_eq!(stopwatch(0), "00:00:00.00");
        assert_eq!(stopwatch(257_369), "00:04:17.36", "hundredths are cut, not rounded");
        assert_eq!(stopwatch(3_723_450), "01:02:03.45");
    }

    #[test]
    fn a_lap_shows_hours_only_once_there_are_some() {
        assert_eq!(lap(51_120), "00:51.12");
        assert_eq!(lap(257_360), "04:17.36");
        assert_eq!(lap(3_723_450), "1:02:03.45");
    }

    #[test]
    fn a_countdown_rounds_up_to_the_second() {
        assert_eq!(countdown(347_001), "05:48");
        assert_eq!(countdown(348_000), "05:48");
        assert_eq!(countdown(1), "00:01");
        assert_eq!(countdown(0), "00:00");
        assert_eq!(countdown(-5), "00:00");
        assert_eq!(countdown(3_948_000), "1:05:48");
    }

    #[test]
    fn a_time_of_day_reads_in_24_or_12_hours() {
        assert_eq!(clock(6, 30, false), "06:30");
        assert_eq!(clock(6, 30, true), "6:30 AM");
        assert_eq!(clock(0, 5, true), "12:05 AM");
        assert_eq!(clock(12, 0, true), "12:00 PM");
        assert_eq!(clock(23, 59, true), "11:59 PM");
    }

    #[test]
    fn how_long_until_reads_in_days_hours_and_minutes() {
        assert_eq!(until(20_000), "less than a minute");
        assert_eq!(until(3 * 60_000 + 59_000), "3 min", "minutes are cut");
        assert_eq!(until((15 * 60 + 58) * 60_000), "15 h 58 min");
        assert_eq!(until(2 * 3_600_000), "2 h");
        assert_eq!(until(26 * 3_600_000), "1 day 2 h");
        assert_eq!(until(72 * 3_600_000 + 60_000), "3 days");
    }

    #[test]
    fn a_city_offset_reads_in_hours() {
        assert_eq!(offset_difference(0), "same time");
        assert_eq!(offset_difference(420), "+7 h");
        assert_eq!(offset_difference(-360), "-6 h");
        assert_eq!(offset_difference(330), "+5:30 h");
        assert_eq!(offset_difference(-150), "-2:30 h");
    }

    #[test]
    fn a_length_reads_in_minutes_and_hours() {
        assert_eq!(minutes(1), "1 min");
        assert_eq!(minutes(25), "25 min");
        assert_eq!(minutes(60), "1 h");
        assert_eq!(minutes(90), "1 h 30 min");
    }
}
