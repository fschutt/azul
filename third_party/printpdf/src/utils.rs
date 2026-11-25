//! Internal helpers. Nothing in this module is public.
//!
//! It holds pseudo-random ids for the PDF `/ID` entry, PDF date strings and gzip.

use std::{
    io::Read,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::date::OffsetDateTime;

/// Counter that feeds [`random_number`]. File identifiers need no cryptographic security, so a
/// xorshift generator replaces a dependency on a random number crate.
static RAND_SEED: AtomicUsize = AtomicUsize::new(2100);

/// Next number of the xorshift generator. Every call advances a global counter, so the result
/// differs from call to call and is not cryptographically secure.
pub(crate) fn random_number() -> usize {
    let mut x = RAND_SEED.fetch_add(21, Ordering::SeqCst);
    #[cfg(target_pointer_width = "64")]
    {
        x ^= x << 21;
        x ^= x >> 35;
        x ^= x << 4;
        x
    }

    #[cfg(target_pointer_width = "32")]
    {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    }
}

/// 32 pseudo-random characters from `A` to `J` (the digits of [`random_number`], mapped to letters).
pub(crate) fn random_character_string_32() -> String {
    const MAX_CHARS: usize = 32;
    let mut final_string = String::with_capacity(MAX_CHARS);
    let mut char_pos = 0;

    'outer: while char_pos < MAX_CHARS {
        let rand = format!("{}", crate::utils::random_number());
        // The decimal digits of a number, so `to_digit` always has an answer; each pass of the
        // outer loop adds at least one character.
        for digit in rand.chars().filter_map(|ch| ch.to_digit(10)) {
            if char_pos < MAX_CHARS {
                final_string.push(u8_to_char(digit as u8));
                char_pos += 1;
            } else {
                break 'outer;
            }
        }
    }

    final_string
}

/// Formats `date` as a PDF date string such as `D:20170505150224+00'00'`. The time zone is always
/// written as UTC. On wasm the result is the Unix epoch.
#[cfg(target_family = "wasm")]
pub(crate) fn to_pdf_time_stamp_metadata(_date: &OffsetDateTime) -> String {
    "D:19700101000000+00'00'".to_string()
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn to_pdf_time_stamp_metadata(date: &OffsetDateTime) -> String {
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}+00'00'",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
    )
}

/// Formats `date` as an XMP date such as `D:2018-09-19T10:05:05+00'00'`. On wasm the result is the
/// Unix epoch.
#[cfg(target_family = "wasm")]
pub(crate) fn to_pdf_xmp_date(_date: &OffsetDateTime) -> String {
    "D:1970-01-01T00:00:00+00'00'".to_string()
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn to_pdf_xmp_date(date: &OffsetDateTime) -> String {
    // Since the time is in UTC, we know that the time zone
    // difference to UTC is 0 min, 0 sec, hence the 00'00
    format!(
        "D:{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00'00'",
        date.year(),
        // `u8`, not the `Month` enum: Month's Display ignores the `{:02}` width, so months 1 to 9
        // would have one digit, a date that veraPDF rejects.
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
    )
}

/// `0 => A`, `1 => B`, and so on
#[inline(always)]
fn u8_to_char(input: u8) -> char {
    b'A'.saturating_add(input) as char
}

/// Compresses `bytes` with gzip at the best level.
#[allow(dead_code)]
pub(crate) fn compress(bytes: &[u8]) -> Vec<u8> {
    use std::io::prelude::*;

    use flate2::{write::GzEncoder, Compression};
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// Decompresses gzip `bytes`. Returns what could be read if the data is damaged.
pub(crate) fn uncompress(bytes: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    let mut gz = GzDecoder::new(bytes);
    let mut s = Vec::<u8>::new();
    let _ = gz.read_to_end(&mut s);
    s
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn xmp_date_zero_pads_single_digit_month() {
        use crate::date::{Date, DateTime, Offset, Time};

        let d = DateTime::new_in_offset(
            Date {
                year: 2018,
                month: 9,
                day: 3,
            },
            Time {
                hour: 1,
                minute: 2,
                second: 3,
                millisecond: 0,
            },
            Offset {
                hours: 0,
                minutes: 0,
                seconds: 0,
                milliseconds: 0,
            },
        );
        assert_eq!(super::to_pdf_xmp_date(&d), "D:2018-09-03T01:02:03+00'00'");
    }
}
