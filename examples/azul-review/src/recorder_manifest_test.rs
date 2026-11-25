//! The manifest and the transcript: round trips, the strict checks, the lines of a killed
//! recording. No window, no azul, no disk.

use super::*;

fn app() -> AppInfo {
    AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") }
}

fn clock() -> ClockInfo {
    ClockInfo {
        unit: String::from("us"),
        origin_unix_ms: 1_764_108_000_000,
        origin_iso8601: String::from("2025-11-25T22:00:00Z"),
        utc_offset_minutes: 60,
    }
}

fn host() -> HostInfo {
    HostInfo { os: String::from("macos"), arch: String::from("aarch64") }
}

fn begin() -> Manifest {
    Manifest::begin(
        app(),
        "2025-11-25_22-00-00",
        Workspace { root: String::from("/Users/a/azul"), name: String::from("azul") },
        clock(),
        host(),
    )
}

fn segment(file: &str, start: u64, end: Option<u64>) -> Segment {
    Segment { file: file.to_string(), start_us: start, end_us: end, bytes: 1_000, samples: 10, complete: end.is_some(), recovered: false, width: 0, height: 0 }
}

fn video(width: u32, height: u32, segments: Vec<Segment>) -> Track {
    Track {
        source: String::from("frame-tap"),
        device: None,
        codec: String::from("h264"),
        encoder: String::from("VideoToolbox"),
        hardware: true,
        width,
        height,
        scale: 2.0,
        fps: 10,
        vfr: true,
        bitrate_kbps: 3_000,
        sample_rate: 0,
        channels: 0,
        timescale: 90_000,
        status: TrackStatus::Recording,
        note: None,
        segments,
    }
}

fn audio(segments: Vec<Segment>) -> Track {
    Track {
        source: String::from("microphone"),
        device: Some(String::from("MacBook Air Microphone")),
        codec: String::from("opus"),
        encoder: String::from("AudioToolbox"),
        hardware: false,
        width: 0,
        height: 0,
        scale: 1.0,
        fps: 0,
        vfr: false,
        bitrate_kbps: 32,
        sample_rate: 48_000,
        channels: 1,
        timescale: 48_000,
        status: TrackStatus::Recording,
        note: None,
        segments,
    }
}

/// A manifest with all three tracks, two runs, the two files and a diagnostic.
fn full() -> Manifest {
    let mut m = begin();
    m.tracks.screen = Some(video(
        2880,
        1800,
        vec![
            segment("screen/0001.mp4", 0, Some(300_000_000)),
            segment("screen/0002.mp4", 300_000_000, None),
        ],
    ));
    m.tracks.camera = Some(video(1280, 720, vec![segment("camera/0001.mp4", 120_000, Some(90_000_000))]));
    m.tracks.audio = Some(audio(vec![segment("audio/0001.mp4", 40_000, Some(90_000_000))]));
    m.runs = vec![
        Run { start_us: 0, end_us: Some(90_000_000), end_reason: Some(String::from("pause")) },
        Run { start_us: 600_000_000, end_us: None, end_reason: None },
    ];
    m.events = Some(FileInfo { file: String::from("events.jsonl"), count: 4_200, bytes: 380_000, dropped: 12 });
    m.transcript = Some(FileInfo { file: String::from("transcript.jsonl"), count: 31, bytes: 9_000, dropped: 0 });
    m.note(5_000_000, "warn", "the camera was denied");
    m
}

fn problems_of(m: &Manifest) -> String {
    m.problems().join(" | ")
}

// ---- round trips ----

#[test]
fn a_new_manifest_is_sound_and_round_trips() {
    let m = begin();
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    assert_eq!(m.schema, SCHEMA);
    assert_eq!(m.state, SessionState::Recording);
    assert_eq!(m.runs.len(), 1);
    assert!(m.runs[0].end_us.is_none(), "the first run has begun and not ended");
    let text = m.to_json();
    assert!(text.ends_with("}\n"));
    assert_eq!(Manifest::parse(&text), Ok(m));
}

#[test]
fn a_manifest_with_three_tracks_round_trips_and_keeps_its_order() {
    let m = full();
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    let text = m.to_json();
    assert!(text.find("\"schema\"").unwrap() < text.find("\"tracks\"").unwrap(), "the schema comes first");
    assert_eq!(Manifest::parse(&text), Ok(m));
}

#[test]
fn what_is_absent_is_left_out_of_the_file() {
    let text = full().to_json();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["tracks"]["screen"].get("device").is_none(), "no device, no key");
    assert!(v["tracks"]["audio"].get("device").is_some());
    assert!(v["tracks"]["screen"]["segments"][1].get("end_us").is_none(), "an open segment has no end");
    assert!(v["tracks"]["screen"]["segments"][0].get("recovered").is_none(), "false is the default");
    assert!(v.get("ended_unix_ms").is_none());
    let bare = begin().to_json();
    let v: serde_json::Value = serde_json::from_str(&bare).unwrap();
    assert!(v["tracks"].as_object().unwrap().is_empty(), "no tracks asked for");
}

#[test]
fn a_segment_that_is_another_size_than_its_track_says_so() {
    let mut m = full();
    let screen = m.tracks.screen.as_mut().unwrap();
    screen.segments[1].width = 2_560;
    screen.segments[1].height = 1_600;
    assert!(m.problems().is_empty());
    let text = m.to_json();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["tracks"]["screen"]["segments"][0].get("width").is_none(), "the track's size is left out");
    assert_eq!(v["tracks"]["screen"]["segments"][1]["width"], 2_560);
    assert_eq!(Manifest::parse(&text), Ok(m));
}

#[test]
fn a_field_this_build_does_not_know_is_ignored_when_reading() {
    let mut v: serde_json::Value = serde_json::from_str(&full().to_json()).unwrap();
    v["future_field"] = serde_json::json!({"a": 1});
    v["tracks"]["screen"]["extra"] = serde_json::json!(true);
    assert_eq!(Manifest::parse(&v.to_string()).unwrap(), full());
}

#[test]
fn the_track_folders_are_the_tracks() {
    let m = full();
    let names: Vec<&str> = m.tracks.present().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, TRACK_DIRS.to_vec());
}

// ---- what it says ----

#[test]
fn the_end_the_size_and_the_recorded_time_come_from_the_runs_and_segments() {
    let m = full();
    assert_eq!(m.end_us(), 300_000_000, "the latest end of a run or a segment");
    assert_eq!(m.media_bytes(), 4_000);
    // The first run is 90 s; the open one goes on until now (700 s): 100 s.
    assert_eq!(m.recorded_us(700_000_000), 190_000_000);
    assert_eq!(m.tracks.screen.as_ref().unwrap().start_us(), Some(0));
    assert_eq!(m.tracks.camera.as_ref().unwrap().start_us(), Some(120_000));
    assert_eq!(m.tracks.screen.as_ref().unwrap().end_us(), Some(300_000_000), "the open segment has none yet");
}

#[test]
fn segment_names_sort_by_order_and_stay_in_their_folder() {
    assert_eq!(segment_file("screen", 1), "screen/0001.mp4");
    assert_eq!(segment_file("audio", 12), "audio/0012.mp4");
    assert!(segment_file("camera", 9) < segment_file("camera", 10), "four digits sort as numbers");
}

#[test]
fn a_path_is_safe_when_it_is_plain_and_relative() {
    for ok in ["a", "screen/0001.mp4", "a/b/c.txt", "weird name/ü.mp4"] {
        assert!(is_safe_relative(ok), "{ok}");
    }
    for bad in ["", "/etc/passwd", "../x", "a/../b", "a//b", "./a", "a/", "a\\b", "a\nb", "C:\\x", "a/./b"] {
        assert!(!is_safe_relative(bad), "{bad:?}");
    }
}

// ---- the strict checks ----

#[test]
fn another_schema_or_unit_is_refused() {
    let mut m = full();
    m.schema = 2;
    m.clock.unit = String::from("ms");
    let p = problems_of(&m);
    assert!(p.contains("schema 2 is not the one this build reads (1)"), "{p}");
    assert!(p.contains("not \"us\""), "{p}");
    assert!(matches!(Manifest::parse(&m.to_json()), Err(ManifestError::Invalid(_))));
}

#[test]
fn a_session_name_must_be_a_plain_folder_name() {
    for bad in ["", "../x", "a/b", "/abs"] {
        let mut m = full();
        m.session = bad.to_string();
        assert!(problems_of(&m).contains("not a plain folder name"), "{bad:?}");
    }
}

#[test]
fn segments_must_be_in_time_order_and_closed_except_the_last() {
    let mut m = full();
    m.tracks.screen.as_mut().unwrap().segments = vec![
        segment("screen/0001.mp4", 100, Some(500)),
        segment("screen/0002.mp4", 400, Some(900)), // starts before the one before ends
    ];
    assert!(problems_of(&m).contains("starts at 400 before the one before it ends (500)"), "{}", problems_of(&m));

    m.tracks.screen.as_mut().unwrap().segments = vec![segment("screen/0001.mp4", 900, Some(100))];
    assert!(problems_of(&m).contains("ends at 100 before it starts (900)"));

    m.tracks.screen.as_mut().unwrap().segments = vec![segment("screen/0001.mp4", 0, None), segment("screen/0002.mp4", 10, Some(20))];
    assert!(problems_of(&m).contains("has no end but is not the last"));
}

#[test]
fn a_segment_file_must_be_a_plain_path_in_its_tracks_folder_and_listed_once() {
    for (file, word) in [
        ("../outside.mp4", "is not a file in the screen folder"),
        ("/abs/x.mp4", "is not a file in the screen folder"),
        ("camera/0001.mp4", "is not a file in the screen folder"),
        ("screen\\0001.mp4", "is not a file in the screen folder"),
    ] {
        let mut m = full();
        m.tracks.screen.as_mut().unwrap().segments = vec![segment(file, 0, Some(10))];
        assert!(problems_of(&m).contains(word), "{file}: {}", problems_of(&m));
    }
    let mut m = full();
    m.tracks.screen.as_mut().unwrap().segments = vec![segment("screen/0001.mp4", 0, Some(10)), segment("screen/0001.mp4", 20, Some(30))];
    assert!(problems_of(&m).contains("listed twice"));
}

#[test]
fn the_runs_must_be_in_order_and_only_the_last_may_be_open() {
    let mut m = full();
    m.runs = vec![
        Run { start_us: 0, end_us: Some(100), end_reason: None },
        Run { start_us: 50, end_us: Some(200), end_reason: None },
    ];
    assert!(problems_of(&m).contains("run 1 starts at 50 before the run before it ends (100)"));
    m.runs = vec![Run { start_us: 10, end_us: Some(5), end_reason: None }];
    assert!(problems_of(&m).contains("run 0 ends at 5 before it starts (10)"));
    m.runs = vec![Run { start_us: 0, end_us: None, end_reason: None }, Run { start_us: 5, end_us: None, end_reason: None }];
    assert!(problems_of(&m).contains("run 0 has no end but is not the last"));
}

#[test]
fn a_track_must_describe_what_it_holds() {
    let mut m = full();
    m.tracks.screen.as_mut().unwrap().width = 0;
    m.tracks.camera.as_mut().unwrap().codec = String::from("opus");
    m.tracks.audio.as_mut().unwrap().channels = 3;
    let p = problems_of(&m);
    assert!(p.contains("the screen track has no picture size"), "{p}");
    assert!(p.contains("the camera track's codec is \"opus\", a video track holds h264"), "{p}");
    assert!(p.contains("one or two channels at a rate"), "{p}");

    let mut m = full();
    m.tracks.screen.as_mut().unwrap().timescale = 0;
    m.tracks.screen.as_mut().unwrap().bitrate_kbps = 900_000;
    let p = problems_of(&m);
    assert!(p.contains("no timescale") && p.contains("cannot be"), "{p}");
}

#[test]
fn the_files_beside_the_manifest_must_be_plain_paths() {
    let mut m = full();
    m.events.as_mut().unwrap().file = String::from("../events.jsonl");
    assert!(problems_of(&m).contains("the events file"));
}

#[test]
fn text_that_is_no_manifest_is_a_json_error_in_words() {
    let Err(ManifestError::Json(e)) = Manifest::parse("this is not json") else { panic!() };
    assert!(e.contains("expected"), "{e}");
    let mut v: serde_json::Value = serde_json::from_str(&full().to_json()).unwrap();
    v.as_object_mut().unwrap().remove("tracks");
    let Err(ManifestError::Json(e)) = Manifest::parse(&v.to_string()) else { panic!() };
    assert!(e.contains("missing field `tracks`"), "{e}");
    let msg = ManifestError::Invalid(vec![String::from("a"), String::from("b")]).to_string();
    assert!(msg.contains("a; b"));
}

// ---- diagnostics ----

#[test]
fn the_diagnostics_stay_under_their_cap_and_lose_the_oldest_info_first() {
    let mut m = begin();
    m.note(1, "error", "the one that matters");
    for i in 0..MAX_DIAGNOSTICS + 50 {
        m.note(i as u64 + 2, "info", &format!("note {i}"));
    }
    assert_eq!(m.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(m.diagnostics[0].text, "the one that matters", "an error outlives the infos");
    assert_eq!(m.diagnostics.last().unwrap().text, format!("note {}", MAX_DIAGNOSTICS + 49));
    assert!(m.problems().is_empty());
}

#[test]
fn a_long_diagnostic_is_cut() {
    let mut m = begin();
    m.note(0, "warn", &"x".repeat(2_000));
    assert_eq!(m.diagnostics[0].text.len(), 500);
}

// ---- the transcript ----

fn word(w: &str, t0: u64, t1: u64, p: Option<f32>) -> Word {
    Word { w: w.to_string(), t0_us: t0, t1_us: t1, p }
}

fn line() -> TranscriptLine {
    TranscriptLine {
        t0_us: 1_000_000,
        t1_us: 3_000_000,
        text: String::from("um this function leaks"),
        lang: String::from("en"),
        engine: String::from("whistle"),
        conv: Some(3),
        words: vec![
            word("um", 1_000_000, 1_400_000, Some(0.73)),
            word("this", 1_500_000, 1_700_000, Some(0.99)),
            word("function", 1_700_000, 2_300_000, None),
            word("leaks", 2_400_000, 3_000_000, Some(0.9)),
        ],
    }
}

#[test]
fn a_transcript_line_round_trips_as_one_line_of_json() {
    let l = line();
    assert!(l.problems().is_empty(), "{:?}", l.problems());
    let text = l.to_json();
    assert!(!text.contains('\n'));
    let (lines, problems) = parse_transcript(&format!("{text}\n"));
    assert!(problems.is_empty());
    assert_eq!(lines, vec![l]);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["words"][2].get("p").is_none(), "an unknown probability is left out");
}

#[test]
fn a_line_whose_times_or_words_cannot_be_is_named() {
    let mut l = line();
    l.t1_us = 500;
    l.words[1].t1_us = 1_400_000;
    l.words[2].t0_us = 1_000_000;
    l.words[3].p = Some(1.5);
    let p = l.problems().join(" | ");
    assert!(p.contains("the line ends at 500 before it starts at 1000000"), "{p}");
    assert!(p.contains("word 1 (\"this\") ends before it starts"), "{p}");
    assert!(p.contains("word 2 (\"function\") starts before the word before it"), "{p}");
    assert!(p.contains("a probability of 1.5"), "{p}");
}

#[test]
fn a_transcript_of_a_killed_recording_reads_up_to_its_torn_last_line() {
    let good = line().to_json();
    let text = format!("{good}\n\n{good}\n{{\"t0_us\":5,\"t1_us\":9,\"text\":\"cut o");
    let (lines, problems) = parse_transcript(&text);
    assert_eq!(lines.len(), 2);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].starts_with("line 4:"), "{problems:?}");
}

#[test]
fn a_line_with_problems_is_left_out_and_named() {
    let mut bad = line();
    bad.t1_us = 0;
    let text = format!("{}\n{}\n", line().to_json(), bad.to_json());
    let (lines, problems) = parse_transcript(&text);
    assert_eq!(lines.len(), 1);
    assert!(problems[0].starts_with("line 2:") && problems[0].contains("before it starts"));
}
