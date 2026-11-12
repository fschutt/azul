//! The few date conversions S3 needs, in UTC, without a date crate: the
//! `x-amz-date` stamp, ListObjectsV2's ISO 8601 `LastModified`, and HTTP's
//! `Last-Modified` (RFC 7231 IMF-fixdate).

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Seconds since 1970-01-01 UTC, now.
#[must_use]
pub fn now_unix() -> u64 {
    let now = system_unix();
    // A test steps the clock of ITS OWN thread (a clock corrected during an analysis); the other
    // tests' threads are not touched.
    #[cfg(test)]
    let now = now.saturating_add_signed(TEST_STEP.with(std::cell::Cell::get));
    now
}

/// Seconds since 1970-01-01 UTC by the system clock as it is - what a test's bucket answers with
/// while the clock of the code under test is stepped ([`step_clock`]).
#[must_use]
pub(crate) fn system_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
thread_local! {
    static TEST_STEP: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// How long something has taken by the MONOTONIC clock: what carries a time of day read at the start
/// forward to the moment it is compared with another clock, when the device's own clock may have been
/// stepped in between ([`crate::meta::reclaim::stamp_agrees`]).
#[cfg(feature = "encryption")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stopwatch {
    at: std::time::Instant,
    /// A test's extra monotonic seconds at the start ([`advance_monotonic`]).
    #[cfg(test)]
    extra_at_start: u64,
}

#[cfg(feature = "encryption")]
impl Stopwatch {
    /// Starts it now.
    #[must_use]
    pub(crate) fn start() -> Stopwatch {
        Stopwatch {
            at: std::time::Instant::now(),
            #[cfg(test)]
            extra_at_start: TEST_MONOTONIC.with(std::cell::Cell::get),
        }
    }

    /// The whole seconds since it started.
    #[must_use]
    pub(crate) fn secs(&self) -> u64 {
        let secs = self.at.elapsed().as_secs();
        #[cfg(test)]
        let secs = secs + TEST_MONOTONIC.with(std::cell::Cell::get) - self.extra_at_start;
        secs
    }
}

#[cfg(all(test, feature = "encryption"))]
thread_local! {
    static TEST_MONOTONIC: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Lets every [`Stopwatch`] of THIS thread read `secs` seconds more: a test that plays a long analysis
/// moves the device's clock AND the monotonic one, a test that plays a step moves the first only.
#[cfg(all(test, feature = "encryption"))]
pub(crate) fn advance_monotonic(secs: u64) {
    TEST_MONOTONIC.with(|extra| extra.set(extra.get() + secs));
}

/// Makes [`now_unix`] of THIS thread read `secs` seconds off the system clock (0: as it is). A test
/// only: the clock of a computer that was set wrong and is corrected while a round runs.
#[cfg(test)]
#[cfg_attr(not(feature = "encryption"), allow(dead_code))]
pub(crate) fn step_clock(secs: i64) {
    TEST_STEP.with(|step| step.set(secs));
}

/// Year, month (1-12), day (1-31) of a day count since 1970-01-01
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Days since 1970-01-01 of a date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Seconds since 1970 of a UTC date and time; `None` when a part is out of range.
fn to_unix(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<u64> {
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    // A leap second counts as the last second of its minute.
    let second = second.min(59);
    Some(days as u64 * 86_400 + u64::from(hour * 3_600 + minute * 60 + second))
}

/// Only ASCII digits (no sign), as a number.
fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `20150830T123600Z` for 2015-08-30 12:36:00 UTC.
#[must_use]
pub fn amz_date(unix_secs: u64) -> String {
    let (year, month, day) = civil_from_days((unix_secs / 86_400) as i64);
    let secs = unix_secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

/// `2015-08-30T12:36:00Z` for 2015-08-30 12:36:00 UTC: ISO 8601 / RFC 3339,
/// what [`parse_iso8601`] reads back (the dates in the apps' files).
#[must_use]
pub fn iso8601(unix_secs: u64) -> String {
    let (year, month, day) = civil_from_days((unix_secs / 86_400) as i64);
    let secs = unix_secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

/// `2009-10-12T17:50:30.000Z` (milliseconds optional) to seconds since 1970.
#[must_use]
pub fn parse_iso8601(text: &str) -> Option<u64> {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year = digits(text.get(0..4)?)?;
    let month = digits(text.get(5..7)?)?;
    let day = digits(text.get(8..10)?)?;
    let hour = digits(text.get(11..13)?)?;
    let minute = digits(text.get(14..16)?)?;
    let second = digits(text.get(17..19)?)?;
    let mut rest = text.get(19..)?;
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        if end == 0 {
            return None;
        }
        rest = &fraction[end..];
    }
    if !matches!(rest, "Z" | "z" | "+00:00" | "+0000" | "-00:00") {
        return None;
    }
    to_unix(i64::from(year), month, day, hour, minute, second)
}

/// `Mon, 12 Oct 2009 17:50:30 GMT` to seconds since 1970.
#[must_use]
pub fn parse_http_date(text: &str) -> Option<u64> {
    let (_weekday, rest) = text.trim().split_once(',')?;
    let mut parts = rest.split_whitespace();
    let day = digits(parts.next()?)?;
    let month_name = parts.next()?;
    let month = MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(month_name))? as u32
        + 1;
    let year = digits(parts.next()?)?;
    let time = parts.next()?;
    if !matches!(parts.next()?, "GMT" | "UTC") || parts.next().is_some() {
        return None;
    }
    let mut clock = time.split(':');
    let hour = digits(clock.next()?)?;
    let minute = digits(clock.next()?)?;
    let second = digits(clock.next()?)?;
    if clock.next().is_some() {
        return None;
    }
    to_unix(i64::from(year), month, day, hour, minute, second)
}
