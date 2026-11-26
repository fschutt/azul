//! The recorder's panel as words and numbers: what the indicator, the panel and the list show, from what the
//! recorder reports. No window, no azul: the tests check keys and arguments, not a language.

use std::{collections::BTreeMap, path::PathBuf};

use azul_appkit::phrase::{Arg, Phrase, Text};

use super::*;
use crate::{
    recorder::{Indicator, Notice, PauseReason, StartError, State, TrackName},
    recorder_export_job::{JobState, Made},
    recorder_fs::RecordingInfo,
    recorder_manifest::SessionState,
    recorder_policy::DiskState,
    recorder_settings::{RecorderSettings, ScreenKind},
};

fn indicator(state: State) -> Indicator {
    Indicator {
        state,
        recorded_us: 754_000_000,
        level: 0.4,
        media_bytes: 340_000_000,
        free_bytes: Some(120_000_000_000),
        hours_left: Some(140.7),
        disk: DiskState::Fine,
        dropped_frames: 0,
        problems: Vec::new(),
    }
}

fn idle() -> Indicator {
    Indicator { state: State::Idle, recorded_us: 0, level: 0.0, media_bytes: 0, free_bytes: None, hours_left: None, disk: DiskState::Unknown, dropped_frames: 0, problems: Vec::new() }
}

fn keys(text: &Text) -> Vec<&str> {
    text.keys()
}

// ---- numbers ----

/// A time recorded is shown as minutes and seconds, hours when there are some; the seconds are whole ones and
/// never rounded up (a recording of 59.9 seconds has not lasted a minute).
///
/// Mutation: rounding up; hours without their own field; the seconds not padded.
#[test]
fn a_time_recorded_is_shown_in_whole_seconds_with_hours_when_there_are_some() {
    for (us, text) in [
        (0, "0:00"),
        (999_999, "0:00"),
        (1_000_000, "0:01"),
        (59_999_999, "0:59"),
        (61_000_000, "1:01"),
        (754_000_000, "12:34"),
        (3_599_000_000, "59:59"),
        (3_600_000_000, "1:00:00"),
        (3_723_000_000, "1:02:03"),
        (36_000_000_000, "10:00:00"),
        (360_000_000_000, "100:00:00"),
    ] {
        assert_eq!(duration_text(us), text, "{us} us");
    }
    assert_eq!(duration_text(u64::MAX).chars().filter(|c| *c == ':').count(), 2, "the largest time is still hours, minutes, seconds");
}

/// Sizes are in the decimal units the Finder uses; the decimal mark is the language's.
///
/// Mutation: units of 1024; the tenth rounded up into the next whole; the mark ignored.
#[test]
fn a_size_is_shown_in_kb_mb_or_gb_with_the_languages_decimal_mark() {
    for (bytes, text) in [
        (0, "0 kB"),
        (999, "0 kB"),
        (1_000, "1 kB"),
        (999_999, "999 kB"),
        (1_000_000, "1 MB"),
        (340_000_000, "340 MB"),
        (999_999_999, "999 MB"),
        (1_000_000_000, "1.0 GB"),
        (1_450_000_000, "1.4 GB"),
        (1_999_999_999, "1.9 GB"),
        (12_345_000_000, "12.3 GB"),
        (2_000_000_000_000, "2000.0 GB"),
    ] {
        assert_eq!(size_text(bytes, '.'), text, "{bytes} bytes");
    }
    assert_eq!(size_text(1_450_000_000, ','), "1,4 GB");
    assert_eq!(size_text(340_000_000, ','), "340 MB", "no mark where there is no tenth");
}

/// The time the disk lasts is said in minutes under an hour and in whole hours over it, with the count for the
/// plural; what is not a time (not a number, negative) is no time at all.
///
/// Mutation: a count of hours rounded up; minutes over an hour; a negative time said as a number.
#[test]
fn the_time_the_disk_lasts_is_minutes_under_an_hour_and_whole_hours_over_it() {
    assert_eq!(hours_phrase(0.5), Some(Phrase::new("azreview-rec-minutes-left").arg("minutes", 30_i64)));
    assert_eq!(hours_phrase(0.999), Some(Phrase::new("azreview-rec-minutes-left").arg("minutes", 59_i64)));
    assert_eq!(hours_phrase(0.0), Some(Phrase::new("azreview-rec-minutes-left").arg("minutes", 0_i64)));
    assert_eq!(hours_phrase(1.0), Some(Phrase::new("azreview-rec-hours-left").arg("hours", 1_i64)));
    assert_eq!(hours_phrase(140.7), Some(Phrase::new("azreview-rec-hours-left").arg("hours", 140_i64)));
    assert_eq!(hours_phrase(f64::NAN), None);
    assert_eq!(hours_phrase(-3.0), None);
    assert_eq!(hours_phrase(f64::INFINITY), Some(Phrase::new("azreview-rec-hours-left").arg("hours", 9_999_i64)), "the most that is said");
}

// ---- the indicator and the buttons ----

/// With nothing recording the indicator is quiet and the one button starts a recording.
///
/// Mutation: a dot that is red when idle; a pause button with nothing to pause.
#[test]
fn an_idle_recorder_shows_a_quiet_dot_and_one_button_that_starts() {
    let h = hud(&idle(), "Cmd+Shift+P", '.');
    assert_eq!(h.dot, Dot::Idle);
    assert_eq!(h.elapsed, "0:00");
    assert_eq!(h.status, Phrase::new("azreview-rec-status-idle"));
    assert_eq!(h.buttons.iter().map(|b| b.what).collect::<Vec<_>>(), vec![Do::Start]);
    assert_eq!(h.buttons[0].label, Phrase::new("azreview-rec-start"));
    assert!(h.notes.is_empty(), "nothing to say: {:?}", h.notes);
    assert_eq!(h.size, None);
}

/// A recording shows a red dot, the time, and the buttons that pause and stop it; the indicator says all of it as
/// a sentence for a person who does not see the dot.
///
/// Mutation: an amber dot while recording; the stop button missing; a sentence without the time.
#[test]
fn a_recording_shows_a_red_dot_the_time_and_the_buttons_that_pause_and_stop() {
    let h = hud(&indicator(State::Recording), "Cmd+Shift+P", '.');
    assert_eq!(h.dot, Dot::Recording);
    assert_eq!(h.elapsed, "12:34");
    assert_eq!(h.status, Phrase::new("azreview-rec-status-recording"));
    assert_eq!(h.buttons.iter().map(|b| b.what).collect::<Vec<_>>(), vec![Do::Pause, Do::Stop]);
    assert_eq!(h.buttons[0].label, Phrase::new("azreview-rec-pause"));
    assert_eq!(h.buttons[1].label, Phrase::new("azreview-rec-stop"));
    assert_eq!(h.described, Phrase::new("azreview-rec-described-recording").arg("time", "12:34"));
    assert_eq!(h.size.as_deref(), Some("340 MB"));
    assert!((h.level - 0.4).abs() < 1e-6);
    // The pause button tells a person who cannot see it which key does it.
    assert_eq!(h.buttons[0].described, Phrase::new("azreview-rec-pause-described").arg("hotkey", "Cmd+Shift+P"));
}

/// A pause says why in words of its own, and the buttons resume and stop; the dot is not the recording's.
///
/// Mutation: one reason said for every pause; a pause that offers pause again.
#[test]
fn a_pause_says_why_and_offers_to_resume() {
    for (why, key) in [
        (PauseReason::User, "azreview-rec-status-paused-user"),
        (PauseReason::Idle, "azreview-rec-status-paused-idle"),
        (PauseReason::Quota, "azreview-rec-status-paused-quota"),
        (PauseReason::Locked, "azreview-rec-status-paused-locked"),
    ] {
        let h = hud(&indicator(State::Paused(why)), "Cmd+Shift+P", '.');
        assert_eq!(h.dot, Dot::Paused, "{why:?}");
        assert_eq!(h.status, Phrase::new(key), "{why:?}");
        assert_eq!(h.buttons.iter().map(|b| b.what).collect::<Vec<_>>(), vec![Do::Resume, Do::Stop], "{why:?}");
        assert_eq!(h.buttons[0].label, Phrase::new("azreview-rec-resume"));
        let (word, english) = pause_word(why);
        assert_eq!(word, key, "{why:?}");
        assert_eq!(h.described, Phrase::new("azreview-rec-described-paused").arg("time", "12:34").arg("why", Arg::word(word, english)), "{why:?}");
    }
}

/// The level is a fraction: what is not one (below nothing, above the full scale, not a number) is cut to fit.
///
/// Mutation: a level that is passed on as it is.
#[test]
fn the_level_is_cut_to_nothing_and_full_scale() {
    for (given, shown) in [(0.4, 0.4), (-1.0, 0.0), (7.0, 1.0), (f32::NAN, 0.0), (f32::INFINITY, 1.0)] {
        let h = hud(&Indicator { level: given, ..indicator(State::Recording) }, "k", '.');
        assert!((h.level - shown).abs() < 1e-6 && h.level.is_finite(), "{given} shown as {}", h.level);
    }
}

// ---- what the person is told ----

/// A track that is not recording is said with its name and the recorder's own words, and the recording goes on.
///
/// Mutation: a problem left out; the track's name missing; the words dropped.
#[test]
fn a_track_that_is_off_is_named_with_the_recorders_words() {
    let mut i = indicator(State::Recording);
    i.problems = vec![(String::from("camera"), String::from("no camera was found")), (String::from("audio"), String::from("Microphone access was refused"))];
    let h = hud(&i, "k", '.');
    let problems: Vec<&Note> = h.notes.iter().filter(|n| keys(&n.text) == vec!["azreview-rec-problem"]).collect();
    assert_eq!(problems.len(), 2, "{:?}", h.notes);
    let first = problems[0].text.phrase("azreview-rec-problem").unwrap();
    assert_eq!(first.get("track"), Some(&Arg::word("azreview-rec-track-camera", "camera")));
    assert_eq!(first.get("why"), Some(&Arg::from("no camera was found")));
    let second = problems[1].text.phrase("azreview-rec-problem").unwrap();
    assert_eq!(second.get("track"), Some(&Arg::word("azreview-rec-track-audio", "audio")));
    assert!(problems.iter().all(|n| n.severity == Severity::Warning));
}

/// The disk: how much is free and how long that lasts while all is well; a warning when it gets low, a stronger
/// one when the recording has paused for it; and a word that the disk cannot be asked.
///
/// Mutation: no warning when low; the low warning's free space missing; the same words for low and full.
#[test]
fn the_disk_is_said_when_fine_when_low_when_full_and_when_unknown() {
    let fine = hud(&indicator(State::Recording), "k", '.');
    let disk = fine.notes.iter().find(|n| n.text.phrase("azreview-rec-disk-free").is_some()).expect("the free space is said");
    assert_eq!(disk.severity, Severity::Info);
    let free = disk.text.phrase("azreview-rec-disk-free").unwrap();
    assert_eq!(free.get("free"), Some(&Arg::from("120.0 GB")));
    assert_eq!(disk.text.phrase("azreview-rec-hours-left"), Some(&Phrase::new("azreview-rec-hours-left").arg("hours", 140_i64)));

    let low = hud(&Indicator { disk: DiskState::Low, free_bytes: Some(15_000_000_000), hours_left: Some(0.8), ..indicator(State::Recording) }, "k", ',');
    let note = low.notes.iter().find(|n| n.severity == Severity::Warning).expect("a warning");
    let phrase = note.text.phrase("azreview-rec-disk-low").expect("the low-disk words");
    assert_eq!(phrase.get("free"), Some(&Arg::from("15,0 GB")), "the mark of the language");
    assert!(note.text.phrase("azreview-rec-minutes-left").is_some(), "{:?}", note.text);

    // (A full disk has paused the recording: how long the disk would last is not said, whatever the estimate is.)
    let full = hud(&Indicator { disk: DiskState::Full, free_bytes: Some(9_000_000_000), hours_left: Some(0.2), ..indicator(State::Paused(PauseReason::Quota)) }, "k", '.');
    assert!(full.notes.iter().any(|n| n.severity == Severity::Warning && n.text.phrase("azreview-rec-disk-full").is_some()), "{:?}", full.notes);
    assert!(!full.notes.iter().any(|n| n.text.phrase("azreview-rec-disk-low").is_some()), "full is not said as low");
    assert!(!full.notes.iter().any(|n| n.text.phrase("azreview-rec-minutes-left").is_some() || n.text.phrase("azreview-rec-hours-left").is_some()), "no time for a disk that is full");

    let unknown = hud(&Indicator { disk: DiskState::Unknown, free_bytes: None, hours_left: None, ..indicator(State::Recording) }, "k", '.');
    assert!(unknown.notes.iter().any(|n| n.text.phrase("azreview-rec-disk-unknown").is_some()), "{:?}", unknown.notes);
}

/// Pictures an encoder refused are said with their count, once there are some.
///
/// Mutation: said when there are none; the count missing.
#[test]
fn dropped_pictures_are_said_with_their_count_only_when_there_are_some() {
    assert!(hud(&indicator(State::Recording), "k", '.').notes.iter().all(|n| n.text.phrase("azreview-rec-dropped").is_none()));
    let h = hud(&Indicator { dropped_frames: 12, ..indicator(State::Recording) }, "k", '.');
    let note = h.notes.iter().find(|n| n.text.phrase("azreview-rec-dropped").is_some()).expect("said");
    assert_eq!(note.text.phrase("azreview-rec-dropped").unwrap().get("count"), Some(&Arg::Int(12)));
}

// ---- before a recording starts ----

/// Before the first press the person is told what will be recorded, in the words of the source: the whole display
/// says every app and every window, and the camera, the voice and the events are named when they are on.
///
/// Mutation: the display described as one window; the camera named when it is off.
#[test]
fn what_will_be_recorded_is_said_before_it_starts() {
    let all = RecorderSettings::default();
    let said: Vec<String> = will_record(&all).into_iter().map(|p| p.key).collect();
    assert_eq!(said, vec!["azreview-rec-will-display", "azreview-rec-will-camera", "azreview-rec-will-voice", "azreview-rec-will-events"]);
    let window = RecorderSettings { screen_source: ScreenKind::Window, camera: false, microphone: false, ..RecorderSettings::default() };
    let said: Vec<String> = will_record(&window).into_iter().map(|p| p.key).collect();
    assert_eq!(said, vec!["azreview-rec-will-window", "azreview-rec-will-events"]);
    let own = RecorderSettings { screen_source: ScreenKind::OwnFrames, ..RecorderSettings::default() };
    assert_eq!(will_record(&own)[0].key, "azreview-rec-will-own");
    let no_screen = RecorderSettings { screen: false, camera: false, microphone: false, ..RecorderSettings::default() };
    assert_eq!(will_record(&no_screen).into_iter().map(|p| p.key).collect::<Vec<_>>(), vec!["azreview-rec-will-events"], "the review's own events are always kept");
}

/// A refused permission says which track, and what the system said; the name of the program the system holds
/// responsible is in the words, since a program started from a terminal is not the app.
///
/// Mutation: the track missing; the program missing.
#[test]
fn a_refused_permission_names_the_track_the_program_and_the_systems_words() {
    let text = denied(TrackName::Screen, "Screen Recording is off", Arg::from("Terminal"));
    let phrase = text.phrase("azreview-rec-denied").expect("a phrase");
    assert_eq!(phrase.get("track"), Some(&Arg::word("azreview-rec-track-screen", "screen")));
    assert_eq!(phrase.get("who"), Some(&Arg::from("Terminal")));
    assert_eq!(phrase.get("why"), Some(&Arg::from("Screen Recording is off")));
    let track = |t| denied(t, "x", Arg::from("AzReview")).phrase("azreview-rec-denied").unwrap().get("track").cloned();
    assert_eq!(track(TrackName::Microphone), Some(Arg::word("azreview-rec-track-audio", "audio")));
    assert_eq!(track(TrackName::Camera), Some(Arg::word("azreview-rec-track-camera", "camera")));
}

/// The program the system holds responsible for a permission: the app when the binary runs inside an app bundle,
/// else the terminal it was started from, else a general word.
///
/// Mutation: a bundle that is called the terminal; an unknown terminal that is called the app.
#[test]
fn the_system_holds_the_app_responsible_in_a_bundle_and_the_terminal_otherwise() {
    let bare = "/Users/x/azul/target/release/AzReview";
    assert_eq!(attribution("/Applications/AzReview.app/Contents/MacOS/AzReview", Some("Apple_Terminal")), Arg::from("AzReview"));
    assert_eq!(attribution("/Applications/Review Tool.app/Contents/MacOS/AzReview", None), Arg::from("Review Tool"), "the bundle's own name");
    assert_eq!(attribution(bare, Some("Apple_Terminal")), Arg::from("Terminal"));
    assert_eq!(attribution(bare, Some("iTerm.app")), Arg::from("iTerm"));
    assert_eq!(attribution(bare, Some("vscode")), Arg::from("Visual Studio Code"));
    assert_eq!(attribution(bare, Some("WezTerm")), Arg::from("WezTerm"), "a terminal the list does not know is called as it calls itself");
    let unknown = Arg::word("azreview-rec-who-unknown", "the program that started AzReview");
    assert_eq!(attribution(bare, None), unknown);
    assert_eq!(attribution(bare, Some("")), unknown);
}

/// What the system allows is said for each track before a recording starts, and a refused one names whom to ask.
///
/// Mutation: a refusal that does not name the program; a restricted device said as refused.
#[test]
fn what_the_system_allows_is_said_for_each_track_and_a_refusal_names_the_program() {
    let who = Arg::from("Terminal");
    let screen = Arg::word("azreview-rec-track-screen", "screen");
    assert_eq!(access_phrase(TrackName::Screen, Access::Allowed, &who), Phrase::new("azreview-rec-access-allowed").arg("track", screen.clone()));
    assert_eq!(access_phrase(TrackName::Screen, Access::NotAsked, &who), Phrase::new("azreview-rec-access-not-asked").arg("track", screen.clone()));
    assert_eq!(access_phrase(TrackName::Screen, Access::Denied, &who), Phrase::new("azreview-rec-access-denied").arg("track", screen.clone()).arg("who", who.clone()));
    assert_eq!(access_phrase(TrackName::Screen, Access::Restricted, &who), Phrase::new("azreview-rec-access-restricted").arg("track", screen));
    assert_eq!(access_phrase(TrackName::Microphone, Access::Allowed, &who).get("track"), Some(&Arg::word("azreview-rec-track-audio", "audio")));
}

/// What the recorder is told of a permission: allowed, or not yet asked (the system asks when the device opens),
/// is no refusal; a refusal is the words the manifest and the panel keep, naming the track and the program.
///
/// Mutation: a device that was never asked is refused; a refusal without its words.
#[test]
fn a_refusal_goes_to_the_recorder_in_words_and_anything_else_is_allowed() {
    use crate::recorder::Perm;
    assert_eq!(access_perm(TrackName::Screen, Access::Allowed, "Terminal"), Perm::Granted);
    assert_eq!(access_perm(TrackName::Camera, Access::NotAsked, "Terminal"), Perm::Granted);
    for (track, name, pane) in [(TrackName::Screen, "Screen Recording", "Screen Recording"), (TrackName::Camera, "Camera access", "Camera"), (TrackName::Microphone, "Microphone access", "Microphone")] {
        let Perm::Denied(why) = access_perm(track, Access::Denied, "Terminal") else { panic!("{track:?} refused") };
        assert!(why.contains(name) && why.contains("Terminal") && why.contains(&format!("Privacy & Security > {pane}")), "{why}");
    }
    let Perm::Denied(why) = access_perm(TrackName::Camera, Access::Restricted, "AzReview") else { panic!("restricted is a refusal") };
    assert!(why.contains("restricted") && !why.contains("switch"), "a restriction is not the person's to switch off: {why}");
}

// ---- what the recorder tells once ----

/// Every notice of the recorder is a line of words with its weight: what needs the person's action is a warning,
/// what only happened is information; the numbers in it are said in the language's units.
///
/// Mutation: a disk warning that is information; a pause for the disk that is not a warning; a notice with a
/// value missing.
#[test]
fn a_notice_of_the_recorder_is_a_line_of_words_with_its_weight() {
    let note = |n: Notice| notice_note(&n, '.');
    let low = note(Notice::DiskLow { free: 15_000_000_000, min_free: 10_000_000_000 });
    assert_eq!(low.severity, Severity::Warning);
    let phrase = low.text.phrase("azreview-rec-notice-disk-low").expect("the words");
    assert_eq!((phrase.get("free"), phrase.get("min")), (Some(&Arg::from("15.0 GB")), Some(&Arg::from("10.0 GB"))));
    assert_eq!(note(Notice::DiskUnknown), Note { severity: Severity::Info, text: Text::key("azreview-rec-disk-unknown") });
    assert_eq!(note(Notice::Paused(PauseReason::User)), Note { severity: Severity::Info, text: Text::key("azreview-rec-status-paused-user") });
    assert_eq!(note(Notice::Paused(PauseReason::Idle)), Note { severity: Severity::Info, text: Text::key("azreview-rec-status-paused-idle") });
    assert_eq!(note(Notice::Paused(PauseReason::Quota)).severity, Severity::Warning, "a disk that is full needs the person");
    assert_eq!(note(Notice::Paused(PauseReason::Locked)).severity, Severity::Info);
    assert_eq!(note(Notice::Resumed(PauseReason::Idle)), Note { severity: Severity::Info, text: Text::key("azreview-rec-notice-resumed") });
    let ending = note(Notice::Ending { left_us: 754_000_000 });
    assert_eq!(ending.severity, Severity::Warning);
    assert_eq!(ending.text.phrase("azreview-rec-notice-ending").unwrap().get("time"), Some(&Arg::from("12:34")));
    assert_eq!(note(Notice::LengthReached), Note { severity: Severity::Warning, text: Text::key("azreview-rec-notice-length-reached") });
    for failed in [Notice::TrackFailed { track: String::from("camera"), why: String::from("unplugged") }, Notice::TrackOff { track: "audio", why: String::from("refused") }] {
        let n = note(failed);
        assert_eq!(n.severity, Severity::Warning);
        let p = n.text.phrase("azreview-rec-problem").expect("said as a problem of the track");
        assert!(matches!(p.get("track"), Some(Arg::Word { .. })) && p.get("why").is_some());
    }
    let gap = note(Notice::ScreenGap { why: String::from("the display slept") });
    assert_eq!((gap.severity, gap.text.phrase("azreview-rec-notice-screen-gap").unwrap().get("why")), (Severity::Info, Some(&Arg::from("the display slept"))));
    let manifest = note(Notice::ManifestError(String::from("disk error")));
    assert_eq!((manifest.severity, manifest.text.phrase("azreview-rec-notice-manifest-error").unwrap().get("why")), (Severity::Warning, Some(&Arg::from("disk error"))));
}

/// A recording that did not start says why in words that say what to do.
///
/// Mutation: the numbers of a full disk missing; the system's words dropped.
#[test]
fn a_recording_that_did_not_start_says_why() {
    let text = |e: StartError| start_error_text(&e, ',');
    assert_eq!(text(StartError::Busy), Text::key("azreview-rec-error-busy"));
    assert_eq!(text(StartError::NothingToRecord), Text::key("azreview-rec-error-nothing"));
    let full = text(StartError::DiskFull { free: 4_500_000_000, min_free: 10_000_000_000 });
    let p = full.phrase("azreview-rec-error-disk-full").expect("words");
    assert_eq!((p.get("free"), p.get("min")), (Some(&Arg::from("4,5 GB")), Some(&Arg::from("10,0 GB"))));
    let io = text(StartError::Io(String::from("Permission denied (os error 13)")));
    assert_eq!(io.phrase("azreview-rec-error-io").unwrap().get("why"), Some(&Arg::from("Permission denied (os error 13)")));
}

// ---- the list of recordings ----

fn info(name: &str, state: Option<SessionState>, recorded_us: u64, bytes: u64, tracks: &[&'static str], problem: Option<&str>) -> RecordingInfo {
    RecordingInfo { name: name.to_string(), state, started_unix_ms: None, utc_offset_minutes: 0, recorded_us, bytes, tracks: tracks.to_vec(), problem: problem.map(str::to_string), video_bytes: None }
}

fn no_exports() -> BTreeMap<String, JobState> {
    BTreeMap::new()
}

/// A row says when (the folder's name is the local time), how long, how big, which tracks and how it stands.
///
/// Mutation: the time left as the folder's name; a recovered recording said to be complete.
#[test]
fn a_row_of_the_list_says_when_how_long_how_big_and_how_it_stands() {
    let list = [
        info("2026-11-26_10-00-00", Some(SessionState::Complete), 3_723_000_000, 2_500_000_000, &["screen", "camera", "audio"], None),
        info("2026-11-25_09-30-05", Some(SessionState::Recovered), 61_000_000, 12_000_000, &["screen"], None),
        info("2026-11-24_08-00-00", Some(SessionState::Recording), 5_000_000, 1_000_000, &["screen", "audio"], None),
    ];
    let rows = rows(&list, '.', &no_exports());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "2026-11-26_10-00-00");
    assert_eq!(rows[0].when, "2026-11-26 10:00:00");
    assert_eq!(rows[0].length, "1:02:03");
    assert_eq!(rows[0].size, "2.5 GB");
    assert_eq!(rows[0].tracks, vec!["screen", "camera", "audio"]);
    assert_eq!(rows[0].state, Phrase::new("azreview-rec-state-complete"));
    assert_eq!(rows[1].state, Phrase::new("azreview-rec-state-recovered"));
    assert_eq!(rows[2].state, Phrase::new("azreview-rec-state-unfinished"), "a recording that says it is still being written, found when none is going");
    assert!(rows.iter().all(|r| r.problem.is_none()));
}

/// A folder whose manifest cannot be read is a row with the words of what is wrong, and a name that is not a time
/// stays as it is.
///
/// Mutation: a damaged recording left out of the list; a strange name cut to a time.
#[test]
fn a_damaged_recording_is_a_row_with_the_words_of_what_is_wrong() {
    let list = [info("old-copy", None, 0, 4_096, &[], Some("the manifest does not parse"))];
    let rows = rows(&list, '.', &no_exports());
    assert_eq!(rows[0].when, "old-copy");
    assert_eq!(rows[0].state, Phrase::new("azreview-rec-state-damaged"));
    let problem = rows[0].problem.as_ref().expect("the words");
    assert_eq!(problem.phrase("azreview-rec-row-problem").unwrap().get("why"), Some(&Arg::from("the manifest does not parse")));
    // A name with the shape of a time but not a time is not made into one.
    for not_a_time in ["2026-13-26_10-00-00", "2026-00-26_10-00-00", "2026-11-32_10-00-00", "2026-11-00_10-00-00", "2026-11-26_24-00-00", "2026-11-26_10-60-00", "2026-11-26_10-00-60", "2026-13-45_99-99-99", "2026-11-26T10-00-00", "2026x11-26_10-00-00", "abcd-11-26_10-00-00", "2026-1a-26_10-00-00", "2026-11-26_10-00-00-x", "2026-11-26_10-00-00-", "2026-11-26_10-00-00_2", "short"] {
        assert_eq!(rows_when(not_a_time), not_a_time);
    }
    assert_eq!(rows_when("2026-11-26_10-00-00"), "2026-11-26 10:00:00");
    assert_eq!(rows_when("2026-12-31_23-59-59"), "2026-12-31 23:59:59", "the last second of the year is a time");
    assert_eq!(rows_when("2026-01-01_00-00-00"), "2026-01-01 00:00:00", "and so is the first");
    assert_eq!(rows_when("2026-11-26_10-00-00-2"), "2026-11-26 10:00:00 (2)", "a second session of the same second is numbered");
}

fn rows_when(name: &str) -> String {
    rows(&[info(name, Some(SessionState::Complete), 0, 0, &[], None)], '.', &no_exports()).remove(0).when
}

// ---- the export of a recording ----

const NAME: &str = "2026-11-26_10-00-00";

fn made(bytes: u64) -> Made {
    Made { file: PathBuf::from(format!("/rec/{NAME}/export/video.mp4")), bytes, notes: Vec::new() }
}

fn row_with(state: Option<SessionState>, tracks: &[&'static str], job: Option<JobState>, video_bytes: Option<u64>) -> Row {
    let mut one = info(NAME, state, 60_000_000, 5_000_000, tracks, None);
    one.video_bytes = video_bytes;
    let jobs: BTreeMap<String, JobState> = job.map(|j| (NAME.to_string(), j)).into_iter().collect();
    rows(&[one], '.', &jobs).remove(0)
}

fn whats(row: &Row) -> Vec<RowDo> {
    row.actions.iter().map(|b| b.what).collect()
}

/// A finished recording that has a screen can be exported; one that is still being written, one that cannot be read
/// and one with no screen cannot (the video is made of the screen's pictures), and the words of each button are the
/// ones the language files have.
///
/// Mutation: an export offered for a recording that is not finished; one offered for a recording with no screen; a
/// recovered recording left out.
#[test]
fn only_a_finished_recording_with_a_screen_is_offered_for_export() {
    let all = ["screen", "camera", "audio"];
    assert_eq!(whats(&row_with(Some(SessionState::Complete), &all, None, None)), vec![RowDo::Export, RowDo::Reveal, RowDo::Delete]);
    assert_eq!(whats(&row_with(Some(SessionState::Recovered), &all, None, None)), vec![RowDo::Export, RowDo::Reveal, RowDo::Delete]);
    assert_eq!(whats(&row_with(Some(SessionState::Recording), &all, None, None)), vec![RowDo::Reveal, RowDo::Delete], "not finished");
    assert_eq!(whats(&row_with(None, &all, None, None)), vec![RowDo::Reveal, RowDo::Delete], "cannot be read");
    assert_eq!(whats(&row_with(Some(SessionState::Complete), &["camera", "audio"], None, None)), vec![RowDo::Reveal, RowDo::Delete], "no screen");
    let labels: Vec<_> = row_with(Some(SessionState::Complete), &all, None, None).actions.iter().map(|b| b.label.key.clone()).collect();
    assert_eq!(labels, vec!["azreview-rec-export", "azreview-rec-reveal", "azreview-rec-delete"]);
}

/// A running export says how far it is in whole percents (never 100 before it is over), offers to cancel it instead
/// of starting another, and keeps the recording from being deleted under it.
///
/// Mutation: a running export that can be started again; a recording that can be deleted while it is exported; a
/// percent that rounds up to 100.
#[test]
fn a_running_export_says_how_far_it_is_and_can_be_cancelled_and_not_deleted() {
    let row = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Running { fraction: 0.376 }), None);
    assert_eq!(row.export, ExportLine::Running { percent: 37 });
    assert_eq!(whats(&row), vec![RowDo::CancelExport, RowDo::Reveal]);
    let text = export_text(&row.export).expect("words");
    assert_eq!(text.phrase("azreview-rec-export-running").unwrap().get("percent"), Some(&Arg::from(37_u32)));
    let nearly = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Running { fraction: 0.9999 }), None);
    assert_eq!(nearly.export, ExportLine::Running { percent: 99 }, "100 is for when the video exists");
    let all = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Running { fraction: 1.0 }), None);
    assert_eq!(all.export, ExportLine::Running { percent: 99 }, "the last piece is still being joined");
    let start = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Running { fraction: 0.0 }), None);
    assert_eq!(start.export, ExportLine::Running { percent: 0 });
}

/// A video that was made is named with its size and can be opened (and made again); the one an earlier run of the app
/// left in the folder is shown the same way; a job's word is newer than the folder's.
///
/// Mutation: a video that cannot be opened; one on disk that is not shown; the folder's video preferred to the job's.
#[test]
fn a_video_that_was_made_is_named_with_its_size_and_can_be_opened_or_made_again() {
    let done = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Done(made(48_300_000))), None);
    assert_eq!(done.export, ExportLine::Done { name: String::from("video.mp4"), size: String::from("48 MB"), notes: Vec::new() });
    assert_eq!(whats(&done), vec![RowDo::Export, RowDo::OpenVideo, RowDo::Reveal, RowDo::Delete]);
    let text = export_text(&done.export).expect("words");
    let p = text.phrase("azreview-rec-export-done").unwrap();
    assert_eq!((p.get("name"), p.get("size")), (Some(&Arg::from("video.mp4")), Some(&Arg::from("48 MB"))));
    // An earlier run's video, no job in this one.
    let before = row_with(Some(SessionState::Complete), &["screen"], None, Some(2_100_000_000));
    assert_eq!(before.export, ExportLine::Done { name: String::from("video.mp4"), size: String::from("2.1 GB"), notes: Vec::new() });
    assert!(whats(&before).contains(&RowDo::OpenVideo));
    // A job that is running now speaks for the recording, not the video that is on the disk from before.
    let again = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Running { fraction: 0.5 }), Some(1_000_000));
    assert_eq!(again.export, ExportLine::Running { percent: 50 });
    assert!(!whats(&again).contains(&RowDo::OpenVideo), "the old video is being made again: it is not offered while it is");
}

/// A failed export says why, in the words it gave, and can be tried again; a cancelled one says so and can too; a
/// recording nothing was done with says nothing.
///
/// Mutation: a failure that loses its words; a cancelled export that cannot be started again.
#[test]
fn a_failed_or_cancelled_export_says_so_and_can_be_tried_again() {
    let failed = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Failed(String::from("ffmpeg was not found"))), None);
    assert_eq!(failed.export, ExportLine::Failed(String::from("ffmpeg was not found")));
    assert_eq!(whats(&failed), vec![RowDo::Export, RowDo::Reveal, RowDo::Delete]);
    let text = export_text(&failed.export).expect("words");
    assert_eq!(text.phrase("azreview-rec-export-failed").unwrap().get("why"), Some(&Arg::from("ffmpeg was not found")));
    let cancelled = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Cancelled), None);
    assert_eq!(cancelled.export, ExportLine::Cancelled);
    assert_eq!(export_text(&cancelled.export), Some(Text::key("azreview-rec-export-cancelled")));
    assert_eq!(whats(&cancelled), vec![RowDo::Export, RowDo::Reveal, RowDo::Delete]);
    let none = row_with(Some(SessionState::Complete), &["screen"], None, None);
    assert_eq!(none.export, ExportLine::None);
    assert_eq!(export_text(&none.export), None);
}

/// What did not work as asked is said under the video that was made (a rounded camera that came out as a rectangle),
/// one line each; a video from an earlier run has none to say.
///
/// Mutation: notes that are not kept; notes that are not drawn.
#[test]
fn what_did_not_work_as_asked_is_said_under_the_video() {
    let mut with_notes = made(1_000_000);
    with_notes.notes = vec![String::from("this ffmpeg cannot round the camera: it is a rectangle"), String::from("second thing")];
    let row = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Done(with_notes)), None);
    let ExportLine::Done { notes, .. } = &row.export else { panic!("{:?}", row.export) };
    assert_eq!(notes.len(), 2);
    let lines = export_notes(&row.export);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].phrase("azreview-rec-export-note").unwrap().get("note"), Some(&Arg::from("this ffmpeg cannot round the camera: it is a rectangle")));
    let plain = row_with(Some(SessionState::Complete), &["screen"], Some(JobState::Done(made(1_000_000))), None);
    assert!(export_notes(&plain.export).is_empty());
    assert!(export_notes(&ExportLine::Failed(String::from("x"))).is_empty());
}

/// The list says when a recording began on the clock of the person who made it: the folder's name is the UTC time (two
/// sessions of a machine never collide across a change of time zone), the manifest says the moment and the offset, and
/// the row says the time on the wall, the date included (a recording made just after midnight is of the next day
/// where the offset is positive, of the day before where it is negative). A second session of the same second keeps
/// its number; a recording whose notes cannot be read says its folder's name.
///
/// Mutation: the offset not added; the offset taken the wrong way; the date not following the time; the number lost.
#[test]
fn the_list_says_the_time_on_the_wall_of_the_person_who_recorded() {
    let at = |name: &str, unix_ms: Option<u64>, offset: i32| {
        let mut one = info(name, Some(SessionState::Complete), 0, 0, &[], None);
        one.started_unix_ms = unix_ms;
        one.utc_offset_minutes = offset;
        rows(&[one], '.', &no_exports()).remove(0).when
    };
    // 1_764_108_000_000 ms is 2025-11-25T22:00:00Z.
    let utc = "2025-11-25_22-00-00";
    assert_eq!(at(utc, Some(1_764_108_000_000), 0), "2025-11-25 22:00:00");
    assert_eq!(at(utc, Some(1_764_108_000_000), 60), "2025-11-25 23:00:00", "an hour east of Greenwich");
    assert_eq!(at(utc, Some(1_764_108_000_000), 180), "2025-11-26 01:00:00", "the next day where the offset carries over midnight");
    assert_eq!(at(utc, Some(1_764_108_000_000), -480), "2025-11-25 14:00:00", "west of Greenwich");
    assert_eq!(at(utc, Some(1_764_108_000_000), -23 * 60), "2025-11-24 23:00:00", "the day before");
    assert_eq!(at(utc, Some(1_764_108_000_000), 330), "2025-11-26 03:30:00", "an offset that is not whole hours");
    assert_eq!(at("2025-11-25_22-00-00-2", Some(1_764_108_000_000), 60), "2025-11-25 23:00:00 (2)", "the number of a second session of the second");
    // What cannot be said by the clock is said by the name.
    assert_eq!(at(utc, None, 60), "2025-11-25 22:00:00", "notes that could not be read");
    assert_eq!(at("old-copy", Some(1_764_108_000_000), 60), "old-copy", "a folder that is not named by a time");
    assert_eq!(at("a-copy-of-it-from-the-backup", Some(1_764_108_000_000), 60), "a-copy-of-it-from-the-backup", "not even a short one");
}

