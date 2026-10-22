//! Times on the wire and what ends at them.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::InvalidValue;

/// A UTC time as RFC 3339 writes it (`2026-10-21T08:00:00Z`; fractions and an offset are read
/// too). The text is kept as it came, so an answer reads back as it was sent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rfc3339 {
    text: String,
    unix: i64,
}

impl Rfc3339 {
    /// `unix` (seconds since 1970, UTC) as `YYYY-MM-DDTHH:MM:SSZ`.
    #[must_use]
    pub fn from_unix(unix: i64) -> Rfc3339 {
        let days = unix.div_euclid(86_400);
        let secs = unix.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        let text = format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        );
        Rfc3339 { text, unix }
    }

    /// The time `text` names.
    ///
    /// # Errors
    ///
    /// Not `YYYY-MM-DDTHH:MM:SS`, an optional fraction, then `Z` or `+HH:MM` / `-HH:MM`; or a
    /// date or time that does not exist.
    pub fn parse(text: &str) -> Result<Rfc3339, InvalidValue> {
        let unix = parse_unix(text.trim())
            .ok_or_else(|| InvalidValue::new("RFC 3339 time", text))?;
        Ok(Rfc3339 {
            text: text.trim().to_string(),
            unix,
        })
    }

    /// Seconds since 1970, UTC (a fraction dropped).
    #[must_use]
    pub fn unix(&self) -> i64 {
        self.unix
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl PartialOrd for Rfc3339 {
    fn partial_cmp(&self, other: &Rfc3339) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rfc3339 {
    fn cmp(&self, other: &Rfc3339) -> std::cmp::Ordering {
        self.unix
            .cmp(&other.unix)
            .then_with(|| self.text.cmp(&other.text))
    }
}

impl TryFrom<String> for Rfc3339 {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<Rfc3339, InvalidValue> {
        Rfc3339::parse(&text)
    }
}

impl From<Rfc3339> for String {
    fn from(time: Rfc3339) -> String {
        time.text
    }
}

impl fmt::Display for Rfc3339 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

fn digits(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(
        bytes
            .iter()
            .fold(0i64, |n, b| n * 10 + i64::from(b - b'0')),
    )
}

fn parse_unix(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, m, d) = (digits(&b[0..4])?, digits(&b[5..7])?, digits(&b[8..10])?);
    let (hh, mm, ss) = (digits(&b[11..13])?, digits(&b[14..16])?, digits(&b[17..19])?);
    let mut rest = &b[19..];
    if rest.first() == Some(&b'.') {
        let frac = rest[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if frac == 0 {
            return None;
        }
        rest = &rest[1 + frac..];
    }
    let offset = match rest {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hours = digits(&[*h1, *h2])?;
            let minutes = digits(&[*m1, *m2])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let secs = hours * 3600 + minutes * 60;
            if *sign == b'+' {
                secs
            } else {
                -secs
            }
        }
        _ => return None,
    };
    // 60 is a leap second, as RFC 3339 allows.
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) || hh > 23 || mm > 59 || ss > 60
    {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// What ends at a [`Deadline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineKind {
    /// The session's S3 credentials run out: the client refreshes before.
    Credentials,
    /// A token family that is not used until then ends.
    FamilyIdle,
    /// The paid period ends: the drive turns read-only after it.
    PeriodEnd,
    /// The read-only time of an unpaid drive ends: it is closed and its data deleted then.
    ReadOnlyEnd,
    /// A paid drive can be picked up until then.
    ClaimWindow,
    /// A handed-over drive's old data is purged then.
    HandoffPurge,
    /// A recovery lockdown's notice ends: the drive is handed over then.
    LockdownEnd,
    /// The writable grace after the period ends: the drive turns read-only then.
    ReadOnlyStart,
    /// A kind this build does not know: a newer service sent it.
    #[serde(other)]
    Unknown,
}

/// A time and what ends at it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Deadline {
    pub at: Rfc3339,
    pub kind: DeadlineKind,
}

impl Deadline {
    /// Whether it is over at `now` (seconds since 1970).
    #[must_use]
    pub fn is_past(&self, now: i64) -> bool {
        now >= self.at.unix()
    }
}
