//! The recorder demo's own command line and clock: pure, no window.

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

/// The sources the demo is asked to record: any of the three, in any order, spaces allowed; what is none of them is
/// refused in words.
///
/// Mutation: a source that is read as another; an unknown word that is taken.
#[test]
fn the_sources_are_any_of_the_three_and_a_strange_word_is_refused() {
    assert_eq!(parse_sources("screen,camera,mic"), Ok((true, true, true)));
    assert_eq!(parse_sources("mic"), Ok((false, false, true)));
    assert_eq!(parse_sources("camera, screen"), Ok((true, true, false)));
    assert_eq!(parse_sources(""), Ok((false, false, false)), "nothing asked is nothing recorded (the recorder says so at the start)");
    assert!(parse_sources("screen,audio").unwrap_err().contains("\"audio\""));
}

/// The demo's own switch is taken out of the arguments and the rest is left for the app.
///
/// Mutation: the list left in the app's arguments; a switch without a list taken.
#[test]
fn the_record_switch_is_taken_out_and_the_rest_is_left_for_the_app() {
    let args = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<String>>();
    assert_eq!(own_arguments(args(&["--record", "camera,mic", "--data-dir", "/d"])), Ok((Some((false, true, true)), args(&["--data-dir", "/d"]))));
    assert_eq!(own_arguments(args(&["--language", "de"])), Ok((None, args(&["--language", "de"]))));
    assert!(own_arguments(args(&["--record"])).is_err());
    assert!(own_arguments(args(&["--record", "video"])).is_err());
}
