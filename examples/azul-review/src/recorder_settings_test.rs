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
    ]);
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    let s = r.settings;
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
    // The reference window.
    assert_eq!(Quality::Standard.screen_kbps(2_880, 1_800), 3_000);
    assert_eq!(Quality::Low.screen_kbps(2_880, 1_800), 1_500);
    assert_eq!(Quality::High.screen_kbps(2_880, 1_800), 6_000);
    // Half the pixels, half the budget; four times, double and no more.
    assert_eq!(Quality::Standard.screen_kbps(1_440, 1_800), 1_500);
    assert_eq!(Quality::Standard.screen_kbps(5_760, 3_600), 6_000);
    assert_eq!(Quality::Standard.screen_kbps(10_000, 10_000), 6_000, "never more than double");
    // A tiny window still gets a quarter of the reference, and never less than 300.
    assert_eq!(Quality::Standard.screen_kbps(100, 100), 750);
    assert_eq!(Quality::Low.screen_kbps(10, 10), 375);
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
