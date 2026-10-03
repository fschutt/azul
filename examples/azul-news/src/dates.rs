//! The dates feeds write, read leniently.
//!
//! RSS says RFC 822 (`Mon, 12 Oct 2009 17:50:30 +0200`), Atom and JSON Feed say RFC 3339
//! (`2009-10-12T17:50:30+02:00`). Real feeds write a weekday that is wrong, a full month name
//! (`September`, `Sept`), one-digit hours, two-digit years, named zones (`CEST`, `PDT`), no zone
//! at all (read as UTC), `+02:00` in an RFC 822 date, a space for the `T`, no seconds, only the
//! date, or `September 30, 2026 8:42 PM`. chrono's own RFC 2822 parser refuses a wrong weekday
//! and a missing zone, so the parts are read here and chrono only adds them up.

use chrono::NaiveDate;

/// Seconds since 1970-01-01 UTC of a date as a feed writes it; `None` when it is no date.
#[must_use]
pub fn parse_date(text: &str) -> Option<i64> {
    let text = text.trim();
    let b = text.as_bytes();
    if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit) {
        parse_iso(text)
    } else {
        parse_words(text)
    }
}

/// Only ASCII digits (no sign), as a number.
fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Seconds since 1970 of a local date and time `offset_east` seconds ahead of UTC.
fn to_unix(
    year: i32,
    month: u32,
    day: u32,
    clock: (u32, u32, u32),
    offset_east: i64,
) -> Option<i64> {
    let (hour, minute, second) = clock;
    let at =
        NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second.min(59))?;
    Some(at.and_utc().timestamp() - offset_east)
}

/// `+02:00`, `+0200`, `+02`, `-5`, `Z` (empty: UTC) as seconds east of UTC.
fn offset(text: &str) -> Option<i64> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    let (sign, rest) = match t.as_bytes()[0] {
        b'+' => (1, &t[1..]),
        b'-' => (-1, &t[1..]),
        _ => return None,
    };
    let (hours, minutes) = if let Some((h, m)) = rest.split_once(':') {
        (digits(h)?, digits(m)?)
    } else if rest.len() == 4 {
        (digits(rest.get(..2)?)?, digits(rest.get(2..)?)?)
    } else if (1..=2).contains(&rest.len()) {
        (digits(rest)?, 0)
    } else {
        return None;
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * i64::from(hours * 3_600 + minutes * 60))
}

/// RFC 3339 / ISO 8601: `2026-09-30[(T| )08:42[:00[.123]]][ ][Z|+02:00|+0200|+02]`.
fn parse_iso(text: &str) -> Option<i64> {
    let year: i32 = text.get(0..4)?.parse().ok()?;
    let month = digits(text.get(5..7)?)?;
    let day = digits(text.get(8..10)?)?;
    let rest = text.get(10..)?;
    let rest = rest
        .strip_prefix(|c: char| c == 'T' || c == 't' || c == ' ')
        .unwrap_or(rest);
    let rb = rest.as_bytes();
    let (clock, zone) = if rb.len() >= 5 && rb[2] == b':' {
        let hour = digits(rest.get(..2)?)?;
        let minute = digits(rest.get(3..5)?)?;
        let mut after = rest.get(5..)?;
        let mut second = 0;
        if after.starts_with(':') && after.len() >= 3 {
            second = digits(after.get(1..3)?)?;
            after = after.get(3..)?;
        }
        if let Some(fraction) = after.strip_prefix('.') {
            let end = fraction
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(fraction.len());
            after = &fraction[end..];
        }
        ((hour, minute, second), after)
    } else {
        ((0, 0, 0), rest)
    };
    to_unix(year, month, day, clock, offset(zone)?)
}

/// The month of a name (`sep`, `sept`, `september`; lower case, at least three letters).
fn month_number(lower: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    if lower.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|m| m.starts_with(lower))
        .map(|i| i as u32 + 1)
}

/// Whether the word is a weekday (`wed`, `weds`, `wednesday`).
fn is_weekday(lower: &str) -> bool {
    const DAYS: [&str; 7] = [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ];
    lower.len() >= 3
        && (DAYS.iter().any(|d| d.starts_with(lower))
            || lower == "tues"
            || lower == "thur"
            || lower == "thurs")
}

/// A zone word as seconds east of UTC: an offset, `GMT` / `UTC` / `UT` (with an offset after
/// it, `GMT+0200`), `Z`, or a common abbreviation.
fn zone_offset(lower: &str) -> Option<i64> {
    if lower.starts_with('+') || lower.starts_with('-') {
        return offset(lower);
    }
    for prefix in ["gmt", "utc", "ut"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.is_empty() {
                return Some(0);
            }
            if rest.starts_with('+') || rest.starts_with('-') {
                return offset(rest);
            }
        }
    }
    let minutes: i64 = match lower {
        "z" | "wet" => 0,
        "est" => -300,
        "edt" => -240,
        "cst" => -360,
        "cdt" => -300,
        "mst" => -420,
        "mdt" => -360,
        "pst" => -480,
        "pdt" => -420,
        "akst" => -540,
        "akdt" => -480,
        "hst" => -600,
        "ast" => -240,
        "adt" => -180,
        "nst" => -210,
        "ndt" => -150,
        "west" | "bst" | "cet" | "met" => 60,
        "cest" | "mest" | "eet" => 120,
        "eest" | "msk" => 180,
        "ist" => 330,
        "hkt" | "sgt" | "awst" => 480,
        "jst" | "kst" => 540,
        "acst" => 570,
        "aest" => 600,
        "aedt" => 660,
        "nzst" => 720,
        "nzdt" => 780,
        _ => return None,
    };
    Some(minutes * 60)
}

/// `17:50:30`, `8:42`, `08:42:00.123`, with a zone written onto it (`08:42:00Z`).
fn clock(token: &str) -> Option<((u32, u32, u32), Option<i64>)> {
    let end = token
        .find(|c: char| !(c.is_ascii_digit() || c == ':' || c == '.'))
        .unwrap_or(token.len());
    let (time, zone) = token.split_at(end);
    let mut parts = time.split(':');
    let hour = digits(parts.next()?)?;
    let minute = digits(parts.next()?)?;
    let second = match parts.next() {
        Some(s) => digits(s.split('.').next().unwrap_or(s))?,
        None => 0,
    };
    let zone = if zone.is_empty() {
        None
    } else {
        Some(zone_offset(&zone.to_ascii_lowercase())?)
    };
    Some(((hour, minute, second), zone))
}

/// RFC 822 and the other word forms: the parts in any order - a weekday (ignored), a day, a
/// month name, a year, a clock, AM / PM, a zone. Day, month and year are needed.
fn parse_words(text: &str) -> Option<i64> {
    let mut day = None;
    let mut month = None;
    let mut year: Option<(u32, usize)> = None;
    let mut time = None;
    let mut pm = None;
    let mut zone = 0;
    for token in text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
    {
        let lower = token.trim_end_matches('.').to_ascii_lowercase();
        if let Some(m) = month_number(&lower) {
            month = month.or(Some(m));
            continue;
        }
        if is_weekday(&lower) {
            continue;
        }
        match lower.as_str() {
            "am" | "a.m" => {
                pm = Some(false);
                continue;
            }
            "pm" | "p.m" => {
                pm = Some(true);
                continue;
            }
            _ => {}
        }
        if token.contains(':') && token.as_bytes()[0].is_ascii_digit() {
            let (at, attached) = clock(token)?;
            time = Some(at);
            if let Some(z) = attached {
                zone = z;
            }
            continue;
        }
        if let Some(seconds) = zone_offset(&lower) {
            zone = seconds;
            continue;
        }
        // `30th`
        let number = lower.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        if let Some(n) = digits(number) {
            if number.len() >= 3 || n > 31 {
                year = year.or(Some((n, number.len())));
            } else if day.is_none() {
                day = Some(n);
            } else if year.is_none() {
                year = Some((n, number.len()));
            }
        }
        // Any other word is ignored.
    }
    let (y, len) = year?;
    let y = match len {
        1 | 2 if y < 50 => y + 2000,
        1 | 2 | 3 => y + 1900,
        _ => y,
    };
    let (mut hour, minute, second) = time.unwrap_or((0, 0, 0));
    match pm {
        Some(true) if hour < 12 => hour += 12,
        Some(false) if hour == 12 => hour = 0,
        _ => {}
    }
    to_unix(
        i32::try_from(y).ok()?,
        month?,
        day?,
        (hour, minute, second),
        zone,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_822_dates_with_numeric_and_named_zones() {
        assert_eq!(
            parse_date("Mon, 12 Oct 2009 17:50:30 +0200"),
            Some(1_255_362_630)
        );
        assert_eq!(
            parse_date("Mon, 12 Oct 2009 17:50:30 GMT"),
            Some(1_255_369_830)
        );
        assert_eq!(
            parse_date("Mon, 12 Oct 2009 17:50:30 UT"),
            Some(1_255_369_830)
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 10:00:00 CEST"),
            Some(1_790_755_200)
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 20:15 PDT"),
            Some(1_790_824_500),
            "no seconds"
        );
        assert_eq!(
            parse_date("Tue, 1 Sep 2026 9:05:00 -0500"),
            Some(1_788_271_500),
            "one-digit day and hour"
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 08:42:00 +02:00"),
            Some(1_790_750_520),
            "a colon in the zone"
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 08:42:00 -04:30"),
            Some(1_790_773_920)
        );
    }

    #[test]
    fn what_real_rss_feeds_get_wrong_is_read_anyway() {
        // 30 September 2026 is a Wednesday: the weekday is ignored.
        assert_eq!(
            parse_date("Fri, 30 Sep 2026 08:42:00 GMT"),
            Some(1_790_757_720),
            "a wrong weekday"
        );
        assert_eq!(
            parse_date("Wednesday, 30 September 2026 08:42:00 GMT"),
            Some(1_790_757_720),
            "full names"
        );
        assert_eq!(
            parse_date("Wed, 30 Sept 2026 08:42:00 GMT"),
            Some(1_790_757_720),
            "Sept"
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 08:42:00"),
            Some(1_790_757_720),
            "no zone is UTC"
        );
        assert_eq!(
            parse_date("wed, 30 sep 2026 08:42:00 gmt"),
            Some(1_790_757_720),
            "lower case"
        );
        assert_eq!(
            parse_date("  Wed,30 Sep 2026 08:42:00 Z "),
            Some(1_790_757_720),
            "no space after the comma"
        );
        assert_eq!(
            parse_date("Mon, 07 Mar 05 14:00:00 +0000"),
            Some(1_110_204_000),
            "a two-digit year"
        );
        assert_eq!(
            parse_date("30 Sep 2026"),
            Some(1_790_726_400),
            "only the date"
        );
        assert_eq!(
            parse_date("Wed, 30 Sep 2026 08:42:00 GMT+0200"),
            Some(1_790_750_520),
            "GMT+0200"
        );
    }

    #[test]
    fn rfc_3339_and_iso_8601_dates() {
        assert_eq!(parse_date("2026-09-30T08:42:00Z"), Some(1_790_757_720));
        assert_eq!(
            parse_date("2026-09-30T08:42:00.123Z"),
            Some(1_790_757_720),
            "fractions"
        );
        assert_eq!(parse_date("2026-09-30T08:42:00+02:00"), Some(1_790_750_520));
        assert_eq!(parse_date("2026-09-30T08:42:00+0200"), Some(1_790_750_520));
        assert_eq!(
            parse_date("2026-09-30T08:42:00 +02:00"),
            Some(1_790_750_520),
            "a space before the zone"
        );
        assert_eq!(
            parse_date("2026-09-30 08:42:00"),
            Some(1_790_757_720),
            "a space for the T, no zone"
        );
        assert_eq!(
            parse_date("2026-09-30t08:42z"),
            Some(1_790_757_720),
            "no seconds, lower case"
        );
        assert_eq!(
            parse_date("2026-09-30"),
            Some(1_790_726_400),
            "only the date"
        );
    }

    #[test]
    fn us_style_dates_with_am_and_pm() {
        assert_eq!(
            parse_date("September 30, 2026 8:42 PM"),
            Some(1_790_800_920)
        );
        assert_eq!(parse_date("Sep 30, 2026"), Some(1_790_726_400));
        assert_eq!(
            parse_date("September 30th, 2026 12:00 AM UTC"),
            Some(1_790_726_400)
        );
    }

    #[test]
    fn what_is_no_date_gives_none() {
        for text in [
            "",
            "yesterday",
            "2026-13-01T00:00:00Z",
            "31 Feb 2026",
            "Wed, 30 Sep 08:42:00 GMT",
            "12:00",
            "2026",
        ] {
            assert_eq!(parse_date(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_date_is_never_read_from_a_bogus_hour() {
        assert_eq!(parse_date("Wed, 30 Sep 2026 25:00:00 GMT"), None);
        assert_eq!(parse_date("2026-09-30T08:61:00Z"), None);
    }
}
