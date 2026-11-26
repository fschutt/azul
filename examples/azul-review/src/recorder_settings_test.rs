//! The recorder's settings: defaults, caps with a word about each, the encoders' targets. No
//! window, no azul, no environment.

use super::*;

fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

fn resolved(pairs: &[(&str, &str)]) -> Resolved {
    resolve(&values(pairs))
}

#[test]
fn nothing_set_is_the_defaults_with_nothing_to_say() {
    let r = resolved(&[]);
    assert_eq!(r.settings, RecorderSettings::default());
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    let d = r.settings;
    assert_eq!(d.min_free_bytes, 10 * GB, "conservative: this disk has about 20 GB free");
    assert_eq!(d.segment_us, 300_000_000, "five minutes");
    assert_eq!(d.idle_pause_us, Some(300_000_000));
    assert!(d.screen && d.camera && d.microphone);
    assert_eq!((d.quality, d.screen_fps), (Quality::Standard, 10));
    assert_eq!(d.max_session_us, 24 * 3_600_000_000);
    assert_eq!(d.folder, None);
    assert_eq!(d.ffmpeg, PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    // The screen is the whole display, 1920 wide at most, with the Notification Center left out; the camera
    // is cut in chunks of five seconds; a locked screen pauses; the pointer is sampled ten times a second.
    assert_eq!((d.screen_source, d.screen_display, d.screen_width), (ScreenKind::Display, 0, 1_920));
    assert_eq!(d.screen_exclude, vec![String::from("com.apple.notificationcenterui")]);
    assert_eq!((d.camera_chunk_us, d.pause_on_lock, d.pointer_hz), (5_000_000, true, 10));
    // The camera's files carry a copy of the voice.
    assert!(d.camera_voice);
}

#[test]
fn every_setting_is_read() {
    let r = resolved(&[
        (KEY_FOLDER, "/Volumes/Recordings/azreview"),
        (KEY_MIN_FREE_GB, "50"),
        (KEY_SEGMENT_MIN, "10"),
        (KEY_IDLE_MIN, "15"),
        (KEY_SCREEN, "off"),
        (KEY_CAMERA, "false"),
        (KEY_MICROPHONE, "yes"),
        (KEY_QUALITY, "High"),
        (KEY_SCREEN_FPS, "20"),
        (KEY_MAX_HOURS, "48"),
        (KEY_FFMPEG, "/usr/local/bin/ffmpeg"),
        (KEY_SCREEN_SOURCE, "Window"),
        (KEY_SCREEN_DISPLAY, "69733382"),
        (KEY_SCREEN_WIDTH, "2560"),
        (KEY_SCREEN_EXCLUDE, "com.apple.dock, org.mozilla.firefox"),
        (KEY_CAMERA_CHUNK_SECS, "10"),
        (KEY_PAUSE_ON_LOCK, "off"),
        (KEY_POINTER_HZ, "20"),
        (KEY_CAMERA_VOICE, "off"),
    ]);
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    let s = r.settings;
    assert_eq!((s.screen_source, s.screen_display, s.screen_width), (ScreenKind::Window, 69_733_382, 2_560));
    assert_eq!(s.screen_exclude, vec![String::from("com.apple.dock"), String::from("org.mozilla.firefox")]);
    assert_eq!((s.camera_chunk_us, s.pause_on_lock, s.pointer_hz), (10_000_000, false, 20));
    assert!(!s.camera_voice);
    assert_eq!(s.folder, Some(PathBuf::from("/Volumes/Recordings/azreview")));
    assert_eq!(s.min_free_bytes, 50 * GB);
    assert_eq!(s.segment_us, 600_000_000);
    assert_eq!(s.idle_pause_us, Some(900_000_000));
    assert_eq!((s.screen, s.camera, s.microphone), (false, false, true));
    assert_eq!((s.quality, s.screen_fps), (Quality::High, 20));
    assert_eq!(s.max_session_us, 48 * 3_600_000_000);
    assert_eq!(s.ffmpeg, PathBuf::from("/usr/local/bin/ffmpeg"));
}

#[test]
fn an_idle_pause_of_zero_minutes_is_no_idle_pause() {
    assert_eq!(resolved(&[(KEY_IDLE_MIN, "0")]).settings.idle_pause_us, None);
}

#[test]
fn a_value_outside_what_a_person_can_mean_is_capped_and_said() {
    let r = resolved(&[
        (KEY_MIN_FREE_GB, "0"),
        (KEY_SEGMENT_MIN, "500"),
        (KEY_SCREEN_FPS, "240"),
        (KEY_MAX_HOURS, "0"),
        (KEY_IDLE_MIN, "100000"),
    ]);
    let s = &r.settings;
    assert_eq!(s.min_free_bytes, GB, "never less than a gigabyte");
    assert_eq!(s.segment_us, 30 * 60_000_000);
    assert_eq!(s.screen_fps, 30);
    assert_eq!(s.max_session_us, 3_600_000_000);
    assert_eq!(s.idle_pause_us, Some(240 * 60_000_000));
    assert_eq!(r.notes.len(), 5, "{:#?}", r.notes);
    assert!(r.notes.iter().any(|n| n.contains("recorder-min-free-gb is 0 GB, under the least of 1: using 1")));
    assert!(r.notes.iter().any(|n| n.contains("recorder-segment-min is 500 minutes, over the most of 30: using 30")));
}

#[test]
fn a_value_that_is_no_number_is_the_default_and_said() {
    let r = resolved(&[(KEY_MIN_FREE_GB, "lots"), (KEY_SEGMENT_MIN, "-5"), (KEY_SCREEN_FPS, "2.5"), (KEY_MAX_HOURS, "")]);
    assert_eq!(r.settings, RecorderSettings::default());
    assert_eq!(r.notes.len(), 4, "{:#?}", r.notes);
    assert!(r.notes[0].contains("\"lots\", no whole number of GB: using 10"));
}

#[test]
fn a_switch_that_is_neither_on_nor_off_keeps_its_default_and_is_said() {
    let r = resolved(&[(KEY_SCREEN, "maybe"), (KEY_CAMERA, " ON "), (KEY_MICROPHONE, "0")]);
    assert!(r.settings.screen, "the default is on");
    assert!(r.settings.camera);
    assert!(!r.settings.microphone);
    assert_eq!(r.notes.len(), 1);
    assert!(r.notes[0].contains("recorder-screen is \"maybe\", not on or off: using on"));
}

#[test]
fn a_blank_folder_or_ffmpeg_is_the_default() {
    let r = resolved(&[(KEY_FOLDER, "   "), (KEY_FFMPEG, "")]);
    assert_eq!(r.settings.folder, None);
    assert_eq!(r.settings.ffmpeg, PathBuf::from(DEFAULT_FFMPEG));
    assert!(r.notes.is_empty());
}

/// The three places the screen can come from, by their words in the settings file, and what is said
/// of a word that is none of them.
#[test]
fn the_screen_source_is_the_display_a_window_or_the_apps_own_frames() {
    for (word, kind, source) in [("display", ScreenKind::Display, "display"), ("window", ScreenKind::Window, "window"), ("own", ScreenKind::OwnFrames, "frame-tap")] {
        assert_eq!(ScreenKind::parse(word), Some(kind));
        assert_eq!(ScreenKind::parse(&format!(" {} ", word.to_uppercase())), Some(kind), "case and blanks do not matter");
        assert_eq!(kind.name(), word);
        assert_eq!(kind.source(), source, "what the manifest calls the track's source");
    }
    assert_eq!(ScreenKind::parse("monitor"), None);
    let r = resolved(&[(KEY_SCREEN_SOURCE, "monitor")]);
    assert_eq!(r.settings.screen_source, ScreenKind::Display);
    assert!(r.notes[0].contains("not display, window or own: using display"), "{:?}", r.notes);
}

/// The new settings are capped like the old ones, each with a word: a width no encoder takes, a chunk
/// that is no chunk, a pointer sampled a thousand times a second, a list of apps without end, a name that
/// is no bundle id. A blank list is a list of nobody (the person cleared the default).
///
/// Mutation: no cap on the width (7680 and 640 are the ends); no cap on the list (64); a blank exclusion
/// list that gives the default list back.
#[test]
fn the_screen_width_the_chunk_the_pointer_rate_and_the_exclusion_list_are_capped_with_a_word() {
    let r = resolved(&[(KEY_SCREEN_WIDTH, "100"), (KEY_CAMERA_CHUNK_SECS, "0"), (KEY_POINTER_HZ, "1000"), (KEY_SCREEN_DISPLAY, "two")]);
    let s = &r.settings;
    assert_eq!((s.screen_width, s.camera_chunk_us, s.pointer_hz, s.screen_display), (640, 1_000_000, 20, 0));
    assert_eq!(r.notes.len(), 4, "{:?}", r.notes);
    let r = resolved(&[(KEY_SCREEN_WIDTH, "99999"), (KEY_CAMERA_CHUNK_SECS, "600"), (KEY_POINTER_HZ, "0")]);
    assert_eq!((r.settings.screen_width, r.settings.camera_chunk_us, r.settings.pointer_hz), (7_680, 60_000_000, 1));

    // The list: blank is nobody, a missing key is the default, bad names are dropped, too many are cut.
    assert!(resolved(&[(KEY_SCREEN_EXCLUDE, "")]).settings.screen_exclude.is_empty());
    assert!(resolved(&[(KEY_SCREEN_EXCLUDE, " , ")]).settings.screen_exclude.is_empty());
    let r = resolved(&[(KEY_SCREEN_EXCLUDE, "com.apple.dock,not a bundle id,com.example/bad,,com.apple.dock")]);
    assert_eq!(r.settings.screen_exclude, vec![String::from("com.apple.dock")], "duplicates and bad names are left out");
    assert_eq!(r.notes.len(), 2, "one word for each bad name: {:?}", r.notes);
    let long = "a".repeat(300);
    let r = resolved(&[(KEY_SCREEN_EXCLUDE, long.as_str())]);
    assert!(r.settings.screen_exclude.is_empty() && r.notes[0].contains("256"), "{:?}", r.notes);
    let many: Vec<String> = (0..70).map(|i| format!("com.example.app{i}")).collect();
    let r = resolved(&[(KEY_SCREEN_EXCLUDE, many.join(",").as_str())]);
    assert_eq!(r.settings.screen_exclude.len(), 64);
    assert_eq!(r.settings.screen_exclude[63], "com.example.app63");
    assert!(r.notes[0].contains("64"), "{:?}", r.notes);
}

#[test]
fn a_quality_is_one_of_three_words() {
    for q in [Quality::Low, Quality::Standard, Quality::High] {
        assert_eq!(Quality::parse(q.name()), Some(q));
        assert_eq!(Quality::parse(&q.name().to_uppercase()), Some(q), "case does not matter");
    }
    let r = resolved(&[(KEY_QUALITY, "ultra")]);
    assert_eq!(r.settings.quality, Quality::Standard);
    assert!(r.notes[0].contains("not low, standard or high: using standard"));
}

#[test]
fn the_screen_target_follows_the_pixels_and_the_quality_within_bounds() {
    // The reference: a display scaled down to 1920 x 1200, what a review needs of it.
    assert_eq!(Quality::Standard.screen_kbps(1_920, 1_200), 2_500);
    assert_eq!(Quality::Low.screen_kbps(1_920, 1_200), 1_250);
    assert_eq!(Quality::High.screen_kbps(1_920, 1_200), 5_000);
    // The default capture of a 2560 x 1664 display, 1920 wide at its aspect: 2.6 Mbit/s, the 2-3 asked for.
    assert_eq!(Quality::Standard.screen_kbps(1_920, 1_248), 2_600);
    // Half the pixels, half the budget; four times, double and no more.
    assert_eq!(Quality::Standard.screen_kbps(960, 1_200), 1_250);
    assert_eq!(Quality::Standard.screen_kbps(3_840, 2_400), 5_000);
    assert_eq!(Quality::Standard.screen_kbps(10_000, 10_000), 5_000, "never more than double");
    // A tiny window still gets a quarter of the reference, and never less than 300.
    assert_eq!(Quality::Standard.screen_kbps(100, 100), 625);
    assert_eq!(Quality::Low.screen_kbps(10, 10), 312);
    assert!(Quality::Low.screen_kbps(1, 1) >= 300);
    for (w, h) in [(1_280, 800), (2_560, 1_664), (1_920, 1_080)] {
        assert!(Quality::Low.screen_kbps(w, h) < Quality::Standard.screen_kbps(w, h));
        assert!(Quality::Standard.screen_kbps(w, h) < Quality::High.screen_kbps(w, h));
    }
}

#[test]
fn the_camera_presets_trade_size_for_bytes() {
    let (low, std, high) = (Quality::Low.camera(), Quality::Standard.camera(), Quality::High.camera());
    assert_eq!((std.width, std.height, std.fps, std.kbps), (1_280, 720, 15, 800), "720p15 at 800 kbit/s: 0.36 GB an hour");
    assert!(low.kbps < std.kbps && std.kbps < high.kbps);
    assert!(low.width < std.width && std.width < high.width);
    // 800 kbit/s is 0.36 GB an hour: the figure in RECORDER.md.
    assert_eq!(std.kbps * 3_600 / 8 / 1_000, 360, "MB an hour");
    assert_eq!(AUDIO_KBPS * 3_600 / 8 / 1_000, 14, "MB an hour for the voice");
}
