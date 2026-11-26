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

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

/// A recording the demo makes on its own (`--auto SECONDS`): taken out of the arguments, the rest left in order; the
/// last one given counts; none given is none.
///
/// Mutation: the number left in the app's arguments; the first of two taken; a recording asked for when none was.
#[test]
fn the_auto_switch_is_taken_out_with_its_seconds_and_the_rest_is_left_in_order() {
    assert_eq!(take_auto(args(&["--record", "camera,mic", "--auto", "12", "--data-dir", "/d"])), Ok((Some(Auto { seconds: 12, delay_ms: 0, note: None }), args(&["--record", "camera,mic", "--data-dir", "/d"]))));
    assert_eq!(take_auto(args(&["--language", "de"])), Ok((None, args(&["--language", "de"]))));
    assert_eq!(take_auto(Vec::new()), Ok((None, Vec::new())));
    assert_eq!(take_auto(args(&["--auto", "5", "--auto", "9"])).unwrap().0, Some(Auto { seconds: 9, delay_ms: 0, note: None }));
}

/// A time outside what an unattended recording is for is brought into the range and said so, not refused and not
/// obeyed: nothing shorter than two seconds (no fragment is whole in it), nothing longer than ten minutes (a recording
/// left running by mistake); the ends themselves are taken as they are.
///
/// Mutation: a limit that is off by one; a time over the limit that is taken; a time under it that is taken; a
/// note that is missing.
#[test]
fn a_time_outside_the_range_is_brought_into_it_and_said_so() {
    let seconds = |text: &str| take_auto(args(&["--auto", text])).unwrap().0.unwrap();
    assert_eq!(seconds("2"), Auto { seconds: AUTO_SECONDS_MIN, delay_ms: 0, note: None });
    assert_eq!(seconds("600"), Auto { seconds: AUTO_SECONDS_MAX, delay_ms: 0, note: None });
    let low = seconds("1");
    assert_eq!(low.seconds, AUTO_SECONDS_MIN);
    assert!(low.note.as_deref().is_some_and(|n| n.contains("--auto 1:") && n.contains("recording 2 seconds instead")), "{low:?}");
    let zero = seconds("0");
    assert_eq!(zero.seconds, AUTO_SECONDS_MIN);
    assert!(zero.note.is_some());
    let high = seconds("601");
    assert_eq!(high.seconds, AUTO_SECONDS_MAX);
    assert!(high.note.as_deref().is_some_and(|n| n.contains("--auto 601:") && n.contains("recording 600 seconds instead")), "{high:?}");
    let huge = seconds("99999999999999999999");
    assert_eq!(huge.seconds, AUTO_SECONDS_MAX, "a number that does not fit is the most, not an error");
    assert!(huge.note.is_some());
}

/// What is no number of seconds is refused in words, and a switch without one too.
///
/// Mutation: a word taken as zero; a switch without a number taken.
#[test]
fn what_is_no_number_of_seconds_is_refused() {
    for bad in ["", "ten", "-5", "1.5", "5s", " 5"] {
        let why = take_auto(args(&["--auto", bad])).unwrap_err();
        assert!(why.contains("--auto") && why.contains("seconds"), "{bad:?}: {why}");
    }
    assert!(take_auto(args(&["--record", "mic", "--auto"])).unwrap_err().contains("--auto"));
}

/// The recording can start a while after the window is up (`--auto-delay-ms`), so that the start of the devices is
/// measured apart from the start of the program: it is taken out of the arguments with the recording's time, in
/// either order, brought into 0 to a minute with a note when it was not, and refused without a recording to delay.
///
/// Mutation: a delay left in the app's arguments; a delay over the limit taken; a delay without a recording taken; a
/// delay that is not a number taken.
#[test]
fn the_recording_can_be_asked_to_start_a_while_after_the_window_is_up() {
    assert_eq!(take_auto(args(&["--auto", "10", "--auto-delay-ms", "2500", "--language", "de"])), Ok((Some(Auto { seconds: 10, delay_ms: 2_500, note: None }), args(&["--language", "de"]))));
    assert_eq!(take_auto(args(&["--auto-delay-ms", "0", "--auto", "4"])).unwrap().0, Some(Auto { seconds: 4, delay_ms: 0, note: None }));
    assert_eq!(take_auto(args(&["--auto", "5", "--auto-delay-ms", "100", "--auto-delay-ms", "900"])).unwrap().0.unwrap().delay_ms, 900, "the last one given counts");
    let capped = take_auto(args(&["--auto", "10", "--auto-delay-ms", "600000"])).unwrap().0.unwrap();
    assert_eq!(capped.delay_ms, AUTO_DELAY_MS_MAX);
    assert!(capped.note.as_deref().is_some_and(|n| n.contains("--auto-delay-ms 600000") && n.contains("60000 ms")), "{capped:?}");
    assert!(take_auto(args(&["--auto-delay-ms", "500"])).unwrap_err().contains("--auto"), "a delay of nothing");
    for bad in ["", "soon", "-1", "1.5"] {
        let why = take_auto(args(&["--auto", "10", "--auto-delay-ms", bad])).unwrap_err();
        assert!(why.contains("--auto-delay-ms") && why.contains("milliseconds"), "{bad:?}: {why}");
    }
    assert!(take_auto(args(&["--auto", "10", "--auto-delay-ms"])).unwrap_err().contains("--auto-delay-ms"));
}

