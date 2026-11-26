//! The demo's own pure parts: the local time's offset.

use super::*;

/// What `date +%z` says, as minutes; anything else is no offset.
///
/// Mutation: a west offset with the wrong sign; hours and minutes mixed up.
#[test]
fn the_offset_the_date_command_says_is_read_as_minutes() {
    assert_eq!(parse_utc_offset("+0100"), Some(60));
    assert_eq!(parse_utc_offset("-0500"), Some(-300));
    assert_eq!(parse_utc_offset("+0530"), Some(330));
    assert_eq!(parse_utc_offset("-0930"), Some(-570));
    assert_eq!(parse_utc_offset("+0000"), Some(0));
    assert_eq!(parse_utc_offset("+1400"), Some(840));
}

/// What is not an offset is none, and a made-up one (an hour of 15, a minute of 60) is not taken.
///
/// Mutation: an offset beyond +14:00 taken; one with letters taken.
#[test]
fn what_is_no_offset_is_none() {
    for bad in ["", "0100", "+01", "+01:00", "+01000", "+0a00", "+1500", "+0160", "UTC", " +0100", "+0100\n"] {
        assert_eq!(parse_utc_offset(bad), None, "{bad:?}");
    }
}
