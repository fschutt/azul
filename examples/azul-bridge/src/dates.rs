//! The dates the three protocols write and read, in UTC, without a date crate: IMAP's
//! `date-time` (`17-Jul-1996 02:44:25 +0000`) and `date` (`1-Feb-1994`), a message's `Date:`
//! header (RFC 5322, read leniently), HTTP's IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`).

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];

/// Days since 1970-01-01 of a date (Howard Hinnant's `days_from_civil`).
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Year, month (1-12) and day (1-31) of a day count since 1970-01-01.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The day (since 1970-01-01, UTC) a time falls on.
#[must_use]
pub fn day_of(secs: i64) -> i64 {
    secs.div_euclid(86_400)
}

fn month_index(name: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))
        .map(|i| i as u32 + 1)
}

fn valid(year: i64, month: u32, day: u32) -> bool {
    if !(1..=12).contains(&month) || day == 0 || !(1..=9999).contains(&year) {
        return false;
    }
    // The day exists when it does not run into the next month.
    let (y, m, _) = civil_from_days(days_from_civil(year, month, day));
    y == year && m == month
}

fn digits(text: &str) -> Option<i64> {
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `17-Jul-1996 02:44:25 +0000`: IMAP's INTERNALDATE of a time (in UTC).
#[must_use]
pub fn imap_datetime(secs: i64) -> String {
    let (year, month, day) = civil_from_days(day_of(secs));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{day:2}-{}-{year:04} {:02}:{:02}:{:02} +0000",
        MONTHS[(month - 1) as usize],
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// `Sun, 06 Nov 1994 08:49:37 GMT`: HTTP's date of a time.
#[must_use]
pub fn http_date(secs: i64) -> String {
    let days = day_of(secs);
    let (year, month, day) = civil_from_days(days);
    let rest = secs.rem_euclid(86_400);
    format!(
        "{}, {day:02} {} {year:04} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// `+0200` / `-0700` in seconds east of UTC.
fn numeric_zone(zone: &str) -> Option<i64> {
    let (sign, digits4) = match zone.as_bytes().first()? {
        b'+' => (1, &zone[1..]),
        b'-' => (-1, &zone[1..]),
        _ => return None,
    };
    if digits4.len() != 4 {
        return None;
    }
    let value = digits(digits4)?;
    let (hours, minutes) = (value / 100, value % 100);
    if minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

/// `hh:mm:ss` (or `hh:mm`) in seconds.
fn clock(text: &str) -> Option<i64> {
    let mut parts = text.split(':');
    let hours = digits(parts.next()?)?;
    let minutes = digits(parts.next()?)?;
    let seconds = match parts.next() {
        Some(s) => digits(s)?,
        None => 0,
    };
    if parts.next().is_some() || hours > 23 || minutes > 59 || seconds > 60 {
        return None;
    }
    Some(hours * 3_600 + minutes * 60 + seconds)
}

/// The day number of IMAP's `date` (`1-Feb-1994`, `01-Feb-1994`), or of a quoted one.
#[must_use]
pub fn parse_imap_date(text: &str) -> Option<i64> {
    let text = text.trim().trim_matches('"').trim();
    let mut parts = text.split('-');
    let day = digits(parts.next()?.trim())?;
    let month = month_index(parts.next()?)?;
    let year = digits(parts.next()?)?;
    if parts.next().is_some() || !valid(year, month, day as u32) {
        return None;
    }
    Some(days_from_civil(year, month, day as u32))
}

/// Seconds since 1970 of IMAP's `date-time` (`17-Jul-1996 02:44:25 -0700`; the day may be
/// space-padded).
#[must_use]
pub fn parse_imap_datetime(text: &str) -> Option<i64> {
    let text = text.trim().trim_matches('"');
    let text = text.trim_start();
    let (date, rest) = text.split_once(' ')?;
    let mut rest = rest.split_whitespace();
    let time = clock(rest.next()?)?;
    let zone = numeric_zone(rest.next()?)?;
    if rest.next().is_some() {
        return None;
    }
    let day = parse_imap_date(date)?;
    Some(day * 86_400 + time - zone)
}

/// The zone names RFC 5322 still lets a `Date:` carry (obs-zone), in seconds east of UTC; an
/// unknown one (a military letter) counts as UTC.
fn named_zone(zone: &str) -> Option<i64> {
    let hours = match zone.to_ascii_uppercase().as_str() {
        "UT" | "UTC" | "GMT" | "Z" => 0,
        "EDT" => -4,
        "EST" | "CDT" => -5,
        "CST" | "MDT" => -6,
        "MST" | "PDT" => -7,
        "PST" => -8,
        z if z.len() == 1 && z.bytes().all(|b| b.is_ascii_alphabetic()) => 0,
        _ => return None,
    };
    Some(hours * 3_600)
}

/// Seconds since 1970 of a `Date:` header (`Thu, 01 Oct 2026 08:30:00 +0000`), read
/// leniently: no weekday, two-digit years, a missing seconds field, comments and zone names.
#[must_use]
pub fn parse_rfc5322_date(text: &str) -> Option<i64> {
    // Comments go first (`(CEST)`).
    let mut plain = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let plain = match plain.split_once(',') {
        Some((_weekday, rest)) => rest.to_string(),
        None => plain,
    };
    let mut parts = plain.split_whitespace();
    let day = digits(parts.next()?)?;
    let month = month_index(parts.next()?.get(..3)?)?;
    let year_text = parts.next()?;
    let mut year = digits(year_text)?;
    if year_text.len() <= 2 {
        year += if year < 50 { 2000 } else { 1900 };
    } else if year_text.len() == 3 {
        year += 1900;
    }
    let time = clock(parts.next()?)?;
    let zone = match parts.next() {
        Some(zone) => numeric_zone(zone).or_else(|| named_zone(zone)).unwrap_or(0),
        None => 0,
    };
    if !valid(year, month, day as u32) {
        return None;
    }
    Some(days_from_civil(year, month, day as u32) * 86_400 + time - zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-01T08:30:00Z
    const OCT_1: i64 = 1_790_843_400;

    #[test]
    fn days_and_dates_convert_both_ways() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(day_of(OCT_1) * 86_400 + 8 * 3_600 + 30 * 60, OCT_1);
        assert_eq!(civil_from_days(day_of(OCT_1)), (2026, 10, 1));
        assert_eq!(day_of(-1), -1);
    }

    #[test]
    fn imap_dates_are_written_and_read_back() {
        assert_eq!(imap_datetime(OCT_1), " 1-Oct-2026 08:30:00 +0000");
        assert_eq!(parse_imap_datetime(" 1-Oct-2026 08:30:00 +0000"), Some(OCT_1));
        assert_eq!(parse_imap_datetime("\"01-Oct-2026 10:30:00 +0200\""), Some(OCT_1));
        assert_eq!(parse_imap_datetime("1-Oct-2026 01:30:00 -0700"), Some(OCT_1));
        assert_eq!(parse_imap_datetime("1-Oct-2026 08:30:00"), None);
        assert_eq!(parse_imap_datetime("31-Feb-2026 08:30:00 +0000"), None);
        assert_eq!(parse_imap_date("1-Oct-2026"), Some(day_of(OCT_1)));
        assert_eq!(parse_imap_date("01-oct-2026"), Some(day_of(OCT_1)));
        assert_eq!(parse_imap_date("1-Foo-2026"), None);
        assert_eq!(parse_imap_date("1-Oct"), None);
    }

    #[test]
    fn http_dates_carry_the_weekday() {
        assert_eq!(http_date(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(http_date(OCT_1), "Thu, 01 Oct 2026 08:30:00 GMT");
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    #[test]
    fn date_headers_are_read_as_mail_programs_write_them() {
        for text in [
            "Thu, 01 Oct 2026 08:30:00 +0000",
            "Thu, 1 Oct 2026 10:30:00 +0200 (CEST)",
            "01 Oct 2026 08:30:00 GMT",
            "Thu, 01 Oct 26 04:30:00 EDT",
            "Thursday, 01 October 2026 08:30:00 Z",
        ] {
            assert_eq!(parse_rfc5322_date(text), Some(OCT_1), "{text}");
        }
        assert_eq!(
            parse_rfc5322_date("Thu, 01 Oct 2026 08:30 +0000"),
            Some(OCT_1)
        );
        assert_eq!(parse_rfc5322_date("yesterday"), None);
        assert_eq!(parse_rfc5322_date("Thu, 32 Oct 2026 08:30:00 +0000"), None);
    }
}
