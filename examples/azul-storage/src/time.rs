//! The few date conversions S3 needs, in UTC, without a date crate: the
//! `x-amz-date` stamp, ListObjectsV2's ISO 8601 `LastModified`, and HTTP's
//! `Last-Modified` (RFC 7231 IMF-fixdate).

/// Seconds since 1970-01-01 UTC, now.
#[must_use]
pub fn now_unix() -> u64 {
    todo!("RED")
}

/// `20150830T123600Z` for 2015-08-30 12:36:00 UTC.
#[must_use]
pub fn amz_date(unix_secs: u64) -> String {
    let _ = unix_secs;
    todo!("RED")
}

/// `2009-10-12T17:50:30.000Z` (milliseconds optional) to seconds since 1970.
#[must_use]
pub fn parse_iso8601(text: &str) -> Option<u64> {
    let _ = text;
    todo!("RED")
}

/// `Mon, 12 Oct 2009 17:50:30 GMT` to seconds since 1970.
#[must_use]
pub fn parse_http_date(text: &str) -> Option<u64> {
    let _ = text;
    todo!("RED")
}
