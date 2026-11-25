//! Dates and times for document metadata.
//!
//! A PDF stores the creation and modification date of a document as a text string such as
//! `D:20170505150224+02'00'`: `D:`, then year, month, day, hour, minute and second as digits,
//! then the offset from UTC as `+` or `-`, hours, `'` and minutes (ISO 32000-1, 7.9.4).
//! [`parse_pdf_date`] reads this format.
//!
//! This module has its own small date and time types. [`DateTime`] (also named
//! [`OffsetDateTime`], after the type of the `time` crate) is a calendar date, a time of day and
//! an offset from UTC. It is used for
//! [`PdfDocumentInfo::creation_date`](crate::PdfDocumentInfo) and the other dates of the
//! document. The fields are plain numbers and are not checked when you construct a value.
//! The `time` crate is used internally for the conversion to and from Unix timestamps and,
//! on native targets, to read the clock.
//!
//! A new document has all its dates set to the Unix epoch (1970-01-01), so set them yourself.
//! When a document is saved, the date and time fields are written followed by `+00'00'`, and
//! on WebAssembly targets the written date is always 1970-01-01.
//!
//! In JSON, a [`DateTime`] is the text `2017-05-05 15:02:24.000 +02:00:00`, a [`Date`] is
//! `2017-05-05`, a [`Time`] is `15:02:24.000`, an [`Offset`] is `+02:00:00.000`, a
//! [`UtcOffset`] is `+02:00:00` and a [`Month`] is its number.
//!
//! # Examples
//!
//! ```
//! use printpdf::*;
//!
//! let date = parse_pdf_date("D:20170505150224+02'00'").unwrap();
//! assert_eq!(date.year(), 2017);
//! assert_eq!(date.month(), Month::May);
//! assert_eq!(date.offset().whole_hours(), 2);
//! assert!(parse_pdf_date("D:2017").is_err()); // too short
//!
//! let mut doc = PdfDocument::new("Dated");
//! doc.metadata.info.creation_date = date;
//! ```

use serde::{Deserialize, Serialize};

/// A calendar date. The fields are not checked when constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    /// Year, for example `2017`.
    pub year: i32,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, starting at 1.
    pub day: u8,
}

/// A time of day. The fields are not checked when constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Time {
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Second, 0 to 59.
    pub second: u8,
    /// Millisecond, 0 to 999.
    pub millisecond: u16,
}

/// An offset from UTC, east of Greenwich positive. Offsets west of it have negative fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Offset {
    /// Hours, for example `2` for UTC+02:00.
    pub hours: i8,
    /// Minutes in addition to `hours`.
    pub minutes: i8,
    /// Seconds in addition to `minutes`.
    pub seconds: i8,
    /// Milliseconds in addition to `seconds`. Not used for timestamps.
    pub milliseconds: i16,
}

/// A date and time of day with an offset from UTC.
///
/// Serialized to JSON as a string, see the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateTime {
    /// Calendar date, in the time zone given by `offset`.
    pub date: Date,
    /// Time of day, in the time zone given by `offset`.
    pub time: Time,
    /// Offset of that time zone from UTC.
    pub offset: Offset,
}

/// A month of the year. Serialized to JSON as its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Month {
    /// Month 1.
    January = 1,
    /// Month 2.
    February,
    /// Month 3.
    March,
    /// Month 4.
    April,
    /// Month 5.
    May,
    /// Month 6.
    June,
    /// Month 7.
    July,
    /// Month 8.
    August,
    /// Month 9.
    September,
    /// Month 10.
    October,
    /// Month 11.
    November,
    /// Month 12.
    December,
}

impl From<u8> for Month {
    /// Converts a month number (1 to 12). Any other number gives [`Month::January`].
    fn from(month: u8) -> Self {
        match month {
            1 => Month::January,
            2 => Month::February,
            3 => Month::March,
            4 => Month::April,
            5 => Month::May,
            6 => Month::June,
            7 => Month::July,
            8 => Month::August,
            9 => Month::September,
            10 => Month::October,
            11 => Month::November,
            12 => Month::December,
            _ => Month::January,
        }
    }
}

impl From<Month> for u8 {
    fn from(month: Month) -> Self {
        month as u8
    }
}

impl std::fmt::Display for Month {
    /// Writes the month number, not its name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl DateTime {
    /// Combines a date, a time of day and the offset from UTC that they are given in. The values
    /// are not checked.
    pub fn new_in_offset(date: Date, time: Time, offset: Offset) -> Self {
        Self { date, time, offset }
    }

    /// The Unix epoch, 1970-01-01 00:00:00 UTC.
    pub const fn epoch() -> Self {
        Self {
            date: Date {
                year: 1970,
                month: 1,
                day: 1,
            },
            time: Time {
                hour: 0,
                minute: 0,
                second: 0,
                millisecond: 0,
            },
            offset: Offset {
                hours: 0,
                minutes: 0,
                seconds: 0,
                milliseconds: 0,
            },
        }
    }

    /// The current time from the system clock, in UTC (the offset is zero).
    ///
    /// With the `js-sys` feature on `wasm32` (not WASI) it is the browser's local time with the
    /// local offset. On other WebAssembly targets there is no clock and it returns
    /// [`DateTime::epoch`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn now() -> Self {
        use time::OffsetDateTime;

        let now = OffsetDateTime::now_utc();

        Self {
            date: Date {
                year: now.year(),
                month: now.month() as u8,
                day: now.day(),
            },
            time: Time {
                hour: now.hour(),
                minute: now.minute(),
                second: now.second(),
                millisecond: now.millisecond(),
            },
            offset: Offset {
                hours: now.offset().whole_hours(),
                minutes: now.offset().minutes_past_hour(),
                seconds: 0,
                milliseconds: 0,
            },
        }
    }

    /// The current time from the system clock, in UTC (the offset is zero).
    ///
    /// With the `js-sys` feature on `wasm32` (not WASI) it is the browser's local time with the
    /// local offset. On other WebAssembly targets there is no clock and it returns
    /// [`DateTime::epoch`].
    #[cfg(all(
        feature = "js-sys",
        target_arch = "wasm32",
        not(any(target_env = "p1", target_env = "p2"))
    ))]
    pub fn now() -> Self {
        use js_sys::Date as JsDate;

        let js_date = JsDate::new_0();
        let month = (js_date.get_month() + 1) as u8;
        let tz_offset_minutes = js_date.get_timezone_offset() as i16;
        let offset_hours = -(tz_offset_minutes / 60) as i8;
        let offset_minutes = -(tz_offset_minutes % 60) as i8;

        Self {
            date: Date {
                year: js_date.get_full_year() as i32,
                month,
                day: js_date.get_date() as u8,
            },
            time: Time {
                hour: js_date.get_hours() as u8,
                minute: js_date.get_minutes() as u8,
                second: js_date.get_seconds() as u8,
                millisecond: js_date.get_milliseconds() as u16,
            },
            offset: Offset {
                hours: offset_hours,
                minutes: offset_minutes,
                seconds: 0,
                milliseconds: 0,
            },
        }
    }

    /// The current time from the system clock, in UTC (the offset is zero).
    ///
    /// With the `js-sys` feature on `wasm32` (not WASI) it is the browser's local time with the
    /// local offset. On other WebAssembly targets there is no clock and it returns
    /// [`DateTime::epoch`].
    #[cfg(any(
        all(target_arch = "wasm32", any(target_env = "p1", target_env = "p2")),
        all(not(feature = "js-sys"), target_arch = "wasm32")
    ))]
    pub fn now() -> Self {
        Self::epoch()
    }

    /// Same as [`DateTime::now`].
    pub fn now_utc() -> Self {
        Self::now()
    }

    /// Formats the value as `2017-05-05 15:02:24.000 +02:00:00`.
    ///
    /// The `_format` argument is ignored. [`DateTime`] parses this format with `str::parse`.
    pub fn format(&self, _format: &str) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {:+03}:{:02}:{:02}",
            self.date.year,
            self.date.month,
            self.date.day,
            self.time.hour,
            self.time.minute,
            self.time.second,
            self.time.millisecond,
            self.offset.hours,
            self.offset.minutes,
            self.offset.seconds
        )
    }

    /// Seconds since 1970-01-01 00:00:00 UTC, taking the offset into account. Milliseconds are
    /// dropped.
    ///
    /// Returns `0` when the date or time fields are not valid. An offset that is not valid
    /// counts as UTC.
    pub fn unix_timestamp(&self) -> i64 {
        use time::{Date as TimeDate, Month as TimeMonth, PrimitiveDateTime, Time as TimeTime};

        let month = match self.date.month {
            1 => TimeMonth::January,
            2 => TimeMonth::February,
            3 => TimeMonth::March,
            4 => TimeMonth::April,
            5 => TimeMonth::May,
            6 => TimeMonth::June,
            7 => TimeMonth::July,
            8 => TimeMonth::August,
            9 => TimeMonth::September,
            10 => TimeMonth::October,
            11 => TimeMonth::November,
            12 => TimeMonth::December,
            _ => return 0,
        };

        let date = match TimeDate::from_calendar_date(self.date.year, month, self.date.day) {
            Ok(d) => d,
            Err(_) => return 0,
        };

        let time = match TimeTime::from_hms_milli(
            self.time.hour,
            self.time.minute,
            self.time.second,
            self.time.millisecond,
        ) {
            Ok(t) => t,
            Err(_) => return 0,
        };

        let offset = match time::UtcOffset::from_hms(
            self.offset.hours,
            self.offset.minutes,
            self.offset.seconds,
        ) {
            Ok(o) => o,
            Err(_) => time::UtcOffset::UTC,
        };

        let primitive_dt = PrimitiveDateTime::new(date, time);
        let offset_dt = primitive_dt.assume_offset(offset);

        offset_dt.unix_timestamp()
    }

    /// The UTC date and time of a Unix timestamp (seconds since 1970-01-01 00:00:00 UTC). The
    /// offset is zero and the milliseconds are 0. Negative timestamps are dates before 1970.
    ///
    /// # Errors
    ///
    /// Returns `"Invalid timestamp"` when the timestamp is outside the range of dates that
    /// the `time` crate supports (years -9999 to 9999 with the features used here).
    pub fn from_unix_timestamp(timestamp: i64) -> Result<Self, &'static str> {
        use time::OffsetDateTime;

        let dt = OffsetDateTime::from_unix_timestamp(timestamp).map_err(|_| "Invalid timestamp")?;

        Ok(Self {
            date: Date {
                year: dt.year(),
                month: dt.month() as u8,
                day: dt.day(),
            },
            time: Time {
                hour: dt.hour(),
                minute: dt.minute(),
                second: dt.second(),
                millisecond: dt.millisecond(),
            },
            offset: Offset {
                hours: dt.offset().whole_hours(),
                minutes: dt.offset().minutes_past_hour(),
                seconds: 0,
                milliseconds: 0,
            },
        })
    }

    /// The year.
    pub fn year(&self) -> i32 {
        self.date.year
    }
    /// The month. A month number outside 1 to 12 gives [`Month::January`].
    pub fn month(&self) -> Month {
        Month::from(self.date.month)
    }
    /// The day of the month, starting at 1.
    pub fn day(&self) -> u8 {
        self.date.day
    }
    /// The hour, 0 to 23.
    pub fn hour(&self) -> u8 {
        self.time.hour
    }
    /// The minute, 0 to 59.
    pub fn minute(&self) -> u8 {
        self.time.minute
    }
    /// The second, 0 to 59.
    pub fn second(&self) -> u8 {
        self.time.second
    }
    /// The millisecond, 0 to 999.
    pub fn millisecond(&self) -> u16 {
        self.time.millisecond
    }

    /// The offset from UTC, without its milliseconds.
    pub fn offset(&self) -> UtcOffset {
        UtcOffset {
            hours: self.offset.hours,
            minutes: self.offset.minutes,
            seconds: self.offset.seconds,
        }
    }
}

impl ToString for DateTime {
    fn to_string(&self) -> String {
        self.format("")
    }
}

// One part of an offset (`02` of `+02:30`) with the sign of the whole offset applied. A missing
// part is 0; a part that is not a number, or does not fit an `i8` once the sign is applied
// (`-128` with the sign `-`), gives `error`.
fn signed_offset_part(
    sign: i8,
    part: Option<&str>,
    error: &'static str,
) -> Result<i8, &'static str> {
    match part {
        Some(part) => part
            .parse::<i8>()
            .ok()
            .and_then(|value| sign.checked_mul(value))
            .ok_or(error),
        None => Ok(0),
    }
}

/// Parses the format written by [`DateTime::format`]: `YYYY-MM-DD HH:MM[:SS[.mmm]] +HH[:MM[:SS]]`,
/// separated by spaces. The offset needs a leading `+` or `-`.
///
/// The digits after the dot are read as a number of milliseconds, so write three of them. The
/// error is a short message when a part is missing or not a number, or when the date or time is
/// not a valid calendar date and time of day. The offset is not range-checked.
impl std::str::FromStr for DateTime {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        let [date_str, time_str, offset_str, ..] = parts.as_slice() else {
            return Err("Invalid datetime format".to_string());
        };

        let date_parts: Vec<&str> = date_str.split('-').collect();
        let [year, month, day] = date_parts.as_slice() else {
            return Err("Invalid date format".to_string());
        };

        let year = year.parse::<i32>().map_err(|_| "Invalid year")?;
        let month = month.parse::<u8>().map_err(|_| "Invalid month")?;
        let day = day.parse::<u8>().map_err(|_| "Invalid day")?;

        let time_parts: Vec<&str> = time_str.split(':').collect();
        let [hour, minute, rest @ ..] = time_parts.as_slice() else {
            return Err("Invalid time format".to_string());
        };

        let hour = hour.parse::<u8>().map_err(|_| "Invalid hour")?;
        let minute = minute.parse::<u8>().map_err(|_| "Invalid minute")?;

        let (second, millisecond) = match rest.first() {
            Some(seconds_and_millis) => {
                let mut pieces = seconds_and_millis.split('.');
                let second = pieces
                    .next()
                    .unwrap_or_default()
                    .parse::<u8>()
                    .map_err(|_| "Invalid second")?;
                let millisecond = match pieces.next() {
                    Some(millis) => millis.parse::<u16>().map_err(|_| "Invalid millisecond")?,
                    None => 0,
                };
                (second, millisecond)
            }
            None => (0, 0),
        };

        if offset_str.len() < 2 {
            return Err("Invalid offset format".to_string());
        }

        let offset_sign: i8 = if offset_str.starts_with('-') { -1 } else { 1 };
        let Some(offset_body) = offset_str.get(1..) else {
            return Err("Invalid offset format".to_string());
        };
        let mut offset_parts = offset_body.split(':');

        let offset_hours =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset hours")?;
        let offset_minutes =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset minutes")?;
        let offset_seconds =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset seconds")?;

        let dt = DateTime {
            date: Date { year, month, day },
            time: Time {
                hour,
                minute,
                second,
                millisecond,
            },
            offset: Offset {
                hours: offset_hours,
                minutes: offset_minutes,
                seconds: offset_seconds,
                milliseconds: 0,
            },
        };

        if let Err(e) = check_date_valid(&dt) {
            Err(e)
        } else {
            Ok(dt)
        }
    }
}

impl Serialize for DateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Another name for [`DateTime`], used for the dates of a document.
pub type OffsetDateTime = DateTime;

/// An offset from UTC to the second, as returned by [`DateTime::offset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UtcOffset {
    /// Hours, negative west of Greenwich.
    pub hours: i8,
    /// Minutes in addition to `hours`.
    pub minutes: i8,
    /// Seconds in addition to `minutes`.
    pub seconds: i8,
}

impl UtcOffset {
    /// Creates an offset from its parts. Never fails and does not check the ranges.
    pub const fn from_hms(hours: i8, minutes: i8, seconds: i8) -> Result<Self, &'static str> {
        Ok(Self {
            hours,
            minutes,
            seconds,
        })
    }

    /// The hours part.
    pub const fn whole_hours(self) -> i8 {
        self.hours
    }
    /// The minutes part.
    pub const fn minutes_past_hour(self) -> i8 {
        self.minutes
    }
    /// The seconds part.
    pub const fn seconds_past_minute(self) -> i8 {
        self.seconds
    }
    /// Whether any part is negative.
    pub const fn is_negative(self) -> bool {
        self.hours < 0 || self.minutes < 0 || self.seconds < 0
    }
}

impl Serialize for Date {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split('-').collect();

        let [year, month, day] = parts.as_slice() else {
            return Err(serde::de::Error::custom("Invalid date format"));
        };

        let year = year
            .parse::<i32>()
            .map_err(|_| serde::de::Error::custom("Invalid year"))?;
        let month = month
            .parse::<u8>()
            .map_err(|_| serde::de::Error::custom("Invalid month"))?;
        let day = day
            .parse::<u8>()
            .map_err(|_| serde::de::Error::custom("Invalid day"))?;

        Ok(Date { year, month, day })
    }
}

impl Serialize for Time {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            self.hour, self.minute, self.second, self.millisecond
        )
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Time {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split(':').collect();

        let [hour, minute, rest @ ..] = parts.as_slice() else {
            return Err(serde::de::Error::custom("Invalid time format"));
        };

        let hour = hour
            .parse::<u8>()
            .map_err(|_| serde::de::Error::custom("Invalid hour"))?;
        let minute = minute
            .parse::<u8>()
            .map_err(|_| serde::de::Error::custom("Invalid minute"))?;

        let (second, millisecond) = match rest.first() {
            Some(seconds_and_millis) => {
                let mut pieces = seconds_and_millis.split('.');
                let second = pieces
                    .next()
                    .unwrap_or_default()
                    .parse::<u8>()
                    .map_err(|_| serde::de::Error::custom("Invalid second"))?;
                let millisecond = match pieces.next() {
                    Some(millis) => millis
                        .parse::<u16>()
                        .map_err(|_| serde::de::Error::custom("Invalid millisecond"))?,
                    None => 0,
                };
                (second, millisecond)
            }
            None => (0, 0),
        };

        Ok(Time {
            hour,
            minute,
            second,
            millisecond,
        })
    }
}

impl Offset {
    fn is_negative(&self) -> bool {
        self.hours < 0 || self.minutes < 0 || self.seconds < 0 || self.milliseconds < 0
    }
}

impl Serialize for Offset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let sign = if self.is_negative() { '-' } else { '+' };
        format!(
            "{}{:02}:{:02}:{:02}.{:03}",
            sign,
            self.hours.unsigned_abs(),
            self.minutes.unsigned_abs(),
            self.seconds.unsigned_abs(),
            self.milliseconds.unsigned_abs()
        )
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Offset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if s.len() < 2 {
            return Err(serde::de::Error::custom("Invalid offset format"));
        }

        let offset_sign: i8 = if s.starts_with('-') { -1 } else { 1 };
        let Some(body) = s.get(1..) else {
            return Err(serde::de::Error::custom("Invalid offset format"));
        };
        let mut offset_parts = body.split(':');

        let hours = signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset hours")
            .map_err(|msg| serde::de::Error::custom(msg))?;

        let minutes =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset minutes")
                .map_err(|msg| serde::de::Error::custom(msg))?;

        let (seconds, milliseconds) = match offset_parts.next() {
            Some(seconds_and_millis) => {
                let mut pieces = seconds_and_millis.split('.');
                let seconds =
                    signed_offset_part(offset_sign, pieces.next(), "Invalid offset seconds")
                        .map_err(|msg| serde::de::Error::custom(msg))?;
                let milliseconds = match pieces.next() {
                    Some(millis) => millis
                        .parse::<i16>()
                        .ok()
                        .and_then(|value| i16::from(offset_sign).checked_mul(value))
                        .ok_or_else(|| serde::de::Error::custom("Invalid offset milliseconds"))?,
                    None => 0,
                };
                (seconds, milliseconds)
            }
            None => (0, 0),
        };

        Ok(Offset {
            hours,
            minutes,
            seconds,
            milliseconds,
        })
    }
}

impl Serialize for Month {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (*self as u8).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Month {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let month = u8::deserialize(deserializer)?;
        Ok(Month::from(month))
    }
}

impl Serialize for UtcOffset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!("{:+03}:{:02}:{:02}", self.hours, self.minutes, self.seconds).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UtcOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if s.len() < 2 {
            return Err(serde::de::Error::custom("Invalid offset format"));
        }

        let offset_sign: i8 = if s.starts_with('-') { -1 } else { 1 };
        let Some(body) = s.get(1..) else {
            return Err(serde::de::Error::custom("Invalid offset format"));
        };
        let mut offset_parts = body.split(':');

        let hours = signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset hours")
            .map_err(|msg| serde::de::Error::custom(msg))?;

        let minutes =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset minutes")
                .map_err(|msg| serde::de::Error::custom(msg))?;

        let seconds =
            signed_offset_part(offset_sign, offset_parts.next(), "Invalid offset seconds")
                .map_err(|msg| serde::de::Error::custom(msg))?;

        Ok(UtcOffset {
            hours,
            minutes,
            seconds,
        })
    }
}

/// Parses a PDF date string such as `D:20170505150224+02'00'` (ISO 32000-1, 7.9.4).
///
/// The leading `D:` is optional. The year, month, day, hour, minute and second are required, as
/// 14 digits. The offset is optional: after the seconds, `+` or `-`, two digits of hours and
/// optionally `'` and two digits of minutes (and `'` and seconds). `Z` or any other character
/// there means UTC, and an offset without apostrophes (`+0200`) keeps only its hours.
/// Anything after the recognized part is ignored. Digits of the offset that cannot be read count
/// as 0. The milliseconds are 0.
///
/// # Errors
///
/// Returns a message when the string is shorter than 14 characters, a field is not a number, or
/// the date or time is not valid (month 13, 30 February, hour 24, ...). Dates that the PDF
/// format allows to be shortened, such as `D:2017`, are rejected.
pub fn parse_pdf_date(s: &str) -> Result<OffsetDateTime, String> {
    let d = parse_pdf_date_inner(s)?;
    if let Err(e) = check_date_valid(&d) {
        Err(e)
    } else {
        Ok(d)
    }
}

fn parse_pdf_date_inner(s: &str) -> Result<OffsetDateTime, String> {
    let s = s.strip_prefix("D:").unwrap_or(s);
    // Slice bytes, not the `&str`: strings from arbitrary PDFs can have multibyte
    // characters where a `&str` range would panic.
    let Some((fields, offset)) = s.as_bytes().split_at_checked(14) else {
        return Err("Date string too short".to_string());
    };

    fn digits<T: core::str::FromStr>(b: &[u8]) -> Result<T, String>
    where
        T::Err: core::fmt::Display,
    {
        core::str::from_utf8(b)
            .map_err(|e| e.to_string())?
            .parse::<T>()
            .map_err(|e| e.to_string())
    }

    // `fields` holds the 14 digits `YYYYMMDDHHMMSS`.
    let field = |range: core::ops::Range<usize>| {
        fields
            .get(range)
            .ok_or_else(|| "Date string too short".to_string())
    };
    let year: i32 = digits(field(0..4)?)?;
    let month: u8 = digits(field(4..6)?)?;
    let day: u8 = digits(field(6..8)?)?;
    let hour: u8 = digits(field(8..10)?)?;
    let minute: u8 = digits(field(10..12)?)?;
    let second: u8 = digits(field(12..14)?)?;

    let mut offset_hours = 0i8;
    let mut offset_minutes = 0i8;
    let mut offset_seconds = 0i8;

    // `offset` is what follows the seconds: the sign, then `HH'MM'SS'`.
    if let Some((&sign, rest)) = offset.split_first() {
        let tz_sign = match sign {
            b'+' => 1i8,
            b'-' => -1i8,
            _ => 0i8,
        };

        if let Some(hours) = rest.get(0..2) {
            offset_hours = tz_sign.saturating_mul(digits::<i8>(hours).unwrap_or(0));

            if let (Some(b'\''), Some(minutes)) = (rest.get(2), rest.get(3..5)) {
                offset_minutes = tz_sign.saturating_mul(digits::<i8>(minutes).unwrap_or(0));

                if let (Some(b'\''), Some(seconds)) = (rest.get(5), rest.get(6..8)) {
                    offset_seconds = tz_sign.saturating_mul(digits::<i8>(seconds).unwrap_or(0));
                }
            }
        }
    }

    let date = Date { year, month, day };
    let time = Time {
        hour,
        minute,
        second,
        millisecond: 0,
    };
    let offset = Offset {
        hours: offset_hours,
        minutes: offset_minutes,
        seconds: offset_seconds,
        milliseconds: 0,
    };

    Ok(OffsetDateTime::new_in_offset(date, time, offset))
}

fn check_date_valid(d: &DateTime) -> Result<(), String> {
    if !is_valid_date(d.date.year, d.date.month, d.date.day) {
        return Err(format!(
            "Invalid date: {}-{:02}-{:02}",
            d.date.year, d.date.month, d.date.day
        ));
    }

    if d.time.hour > 23 {
        return Err(format!("Invalid hour: {}", d.time.hour));
    }

    if d.time.minute > 59 {
        return Err(format!("Invalid minute: {}", d.time.minute));
    }

    if d.time.second > 59 {
        return Err(format!("Invalid second: {}", d.time.second));
    }

    if d.time.millisecond > 999 {
        return Err(format!("Invalid millisecond: {}", d.time.millisecond));
    }

    Ok(())
}

fn is_valid_date(year: i32, month: u8, day: u8) -> bool {
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };

    day >= 1 && day <= days_in_month
}
