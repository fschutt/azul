//! The cut list: its strict reading, every rule it checks against a session, and the generator that
//! needs no judgement. No window, no azul, no ffmpeg.

use super::*;
use crate::recorder_manifest::{AppInfo, ClockInfo, HostInfo, Run, Segment, Track, TrackStatus, Workspace};

const SEC: u64 = 1_000_000;

fn track(video: bool, w: u32, h: u32, segments: Vec<Segment>) -> Track {
    Track {
        source: String::from("mock"),
        device: None,
        codec: String::from(if video { "h264" } else { "opus" }),
        encoder: String::from("mock"),
        hardware: false,
        width: w,
        height: h,
        scale: 2.0,
        fps: 10,
        vfr: video,
        bitrate_kbps: 100,
        sample_rate: if video { 0 } else { 48_000 },
        channels: if video { 0 } else { 1 },
        timescale: if video { 90_000 } else { 48_000 },
        status: TrackStatus::Stopped,
        note: None,
        segments,
    }
}

fn seg(file: &str, start: u64, end: u64) -> Segment {
    Segment { file: file.into(), start_us: start * SEC, end_us: Some(end * SEC), bytes: 1_000, samples: 10, complete: true, recovered: false, width: 0, height: 0 }
}

/// A session: two runs (0 to 100 s and 200 to 300 s), a screen of 2880 x 1800, a camera, a voice.
fn session() -> Manifest {
    let mut m = Manifest::begin(
        AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        "2025-11-25_22-00-00",
        Workspace { root: String::from("/w"), name: String::from("azul") },
        ClockInfo { unit: String::from("us"), origin_unix_ms: 0, origin_iso8601: String::from("1970-01-01T00:00:00Z"), utc_offset_minutes: 0 },
        HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
    );
    m.tracks.screen = Some(track(true, 2880, 1800, vec![seg("screen/0001.mp4", 0, 100), seg("screen/0002.mp4", 200, 300)]));
    m.tracks.camera = Some(track(true, 1280, 720, vec![seg("camera/0001.mp4", 0, 100), seg("camera/0002.mp4", 200, 300)]));
    m.tracks.audio = Some(track(false, 0, 0, vec![seg("audio/0001.mp4", 0, 100), seg("audio/0002.mp4", 200, 300)]));
    m.runs = vec![
        Run { start_us: 0, end_us: Some(100 * SEC), end_reason: Some(String::from("pause")) },
        Run { start_us: 200 * SEC, end_us: Some(300 * SEC), end_reason: Some(String::from("stop")) },
    ];
    m
}

fn files() -> Vec<(u64, String)> {
    vec![
        (10 * SEC, String::from("src/lib.rs")),
        (60 * SEC, String::from("src/ink.rs")),
        (150 * SEC, String::from("not-recorded.rs")), // in the gap between the runs
        (230 * SEC, String::from("README.md")),
    ]
}

fn one(id: &str, in_s: u64, out_s: u64) -> EdlSegment {
    EdlSegment { id: id.into(), in_us: in_s * SEC, out_us: out_s * SEC, camera: true, speed: 1.0, zoom: None, audio: AudioCut::default(), caption: None, title: None }
}

fn edl_of(segments: Vec<EdlSegment>) -> Edl {
    let m = session();
    let mut e = identity_edl(&m, &[]);
    e.segments = segments;
    e.chapters = Vec::new();
    e
}

fn problems(e: &Edl) -> String {
    e.problems(&session()).join(" | ")
}

// ---- the generator that needs no judgement ----

#[test]
fn the_identity_cut_list_is_one_segment_for_each_run_with_the_camera_and_the_sound_as_recorded() {
    let m = session();
    let e = identity_edl(&m, &files());
    assert_eq!(e.generator, Generator { name: String::from("identity"), version: 1, notes: None });
    let spans: Vec<(&str, u64, u64)> = e.segments.iter().map(|s| (s.id.as_str(), s.in_us / SEC, s.out_us / SEC)).collect();
    assert_eq!(spans, vec![("run1", 0, 100), ("run2", 200, 300)]);
    assert!(e.segments.iter().all(|s| s.camera && (s.speed - 1.0).abs() < f64::EPSILON && s.zoom.is_none() && s.audio == AudioCut::default()));
    assert_eq!(e.duration_us(), 200 * SEC, "the gap between the runs is not in the video");
    assert_eq!(e.session, "2025-11-25_22-00-00");
    assert_eq!(e.title.as_deref(), Some("azul"));
    assert_eq!(e.problems(&m), Vec::<String>::new());
}

#[test]
fn the_picture_is_the_screens_shape_up_to_full_hd_and_a_default_without_a_screen() {
    let m = session();
    let e = identity_edl(&m, &[]);
    assert_eq!((e.output.width, e.output.height), (1728, 1080), "2880 x 1800 scaled to fit 1920 x 1080");
    let mut small = session();
    small.tracks.screen.as_mut().unwrap().width = 800;
    small.tracks.screen.as_mut().unwrap().height = 600;
    assert_eq!((identity_edl(&small, &[]).output.width, identity_edl(&small, &[]).output.height), (800, 600), "never enlarged");
    let mut none = session();
    none.tracks.screen = None;
    assert_eq!((identity_edl(&none, &[]).output.width, identity_edl(&none, &[]).output.height), (1920, 1080));
}

#[test]
fn the_camera_is_shown_only_when_it_recorded_something() {
    let mut m = session();
    m.tracks.camera.as_mut().unwrap().segments.clear();
    assert!(identity_edl(&m, &[]).segments.iter().all(|s| !s.camera));
    m.tracks.camera = None;
    assert!(identity_edl(&m, &[]).segments.iter().all(|s| !s.camera));
}

#[test]
fn a_run_that_is_still_open_goes_to_the_end_of_what_was_recorded() {
    let mut m = session();
    m.runs[1].end_us = None;
    m.runs[1].end_reason = None;
    let e = identity_edl(&m, &[]);
    assert_eq!(e.segments[1].out_us, 300 * SEC);
}

#[test]
fn a_chapter_for_each_file_the_review_opened_on_the_videos_own_timeline() {
    let e = identity_edl(&session(), &files());
    let ch: Vec<(u64, &str)> = e.chapters.iter().map(|c| (c.t_us / SEC, c.title.as_str())).collect();
    // lib.rs at 10 s and ink.rs at 60 s of the first run; README.md at 230 s is 30 s into the second
    // run, which begins at 100 s of the video. The file opened in the gap is not in the video.
    assert_eq!(ch, vec![(10, "src/lib.rs"), (60, "src/ink.rs"), (130, "README.md")]);
}

#[test]
fn the_file_changes_come_from_the_navigation_events_and_a_file_opened_again_is_one_chapter() {
    let events = [
        r#"{"t":1000000,"k":"nav.file","file":"a.rs","lines":10}"#,
        r#"{"t":1500000,"k":"nav.page","file":"a.rs","page":1,"of":3}"#,
        r#"{"t":2000000,"k":"nav.file","file":"a.rs","lines":10}"#,
        "this line is torn {",
        r#"{"t":3000000,"k":"nav.file","file":"b.rs","lines":5}"#,
        r#"{"t":4000000,"k":"nav.file","file":"a.rs","lines":10}"#,
        r#"{"k":"nav.file","file":"no-time.rs"}"#,
    ]
    .join("\n");
    assert_eq!(
        file_changes(&events),
        vec![(SEC, String::from("a.rs")), (3 * SEC, String::from("b.rs")), (4 * SEC, String::from("a.rs"))]
    );
}

// ---- the strict reading ----

#[test]
fn a_cut_list_round_trips_through_its_text() {
    let m = session();
    let e = identity_edl(&m, &files());
    let text = e.to_json();
    assert!(text.ends_with("}\n"));
    assert_eq!(Edl::parse(&text, &m), Ok(e));
}

#[test]
fn a_field_the_schema_does_not_name_is_an_error_that_names_it() {
    let m = session();
    let mut v: serde_json::Value = serde_json::from_str(&identity_edl(&m, &[]).to_json()).unwrap();
    v["highlight_reel"] = serde_json::json!(true);
    let Err(EdlError::Json(e)) = Edl::parse(&v.to_string(), &m) else { panic!() };
    assert!(e.contains("unknown field `highlight_reel`"), "{e}");

    let mut v: serde_json::Value = serde_json::from_str(&identity_edl(&m, &[]).to_json()).unwrap();
    v["segments"][0]["zoom_out"] = serde_json::json!(2);
    let Err(EdlError::Json(e)) = Edl::parse(&v.to_string(), &m) else { panic!() };
    assert!(e.contains("unknown field `zoom_out`"), "{e}");

    let Err(EdlError::Json(e)) = Edl::parse("not json", &m) else { panic!() };
    assert!(e.contains("expected"), "{e}");
}

// ---- every rule ----

#[test]
fn the_schema_the_session_and_the_output_are_checked() {
    let mut e = edl_of(vec![one("a", 0, 10)]);
    e.schema = 2;
    e.session = String::from("another");
    e.output = Output { width: 1921, height: 10, fps: 0, video_kbps: 5, audio_kbps: 9_000 };
    let p = problems(&e);
    for word in ["schema 2 is not the one this build reads (1)", "it is for the session \"another\"", "must be even", "0 frames a second", "bitrates are out of range"] {
        assert!(p.contains(word), "{word}: {p}");
    }
}

#[test]
fn the_style_is_checked() {
    let mut e = edl_of(vec![one("a", 0, 10)]);
    e.style.background = String::from("red");
    e.style.screen.fit = String::from("stretch");
    e.style.camera.corner = String::from("middle");
    e.style.camera.scale = 0.9;
    let p = problems(&e);
    for word in ["is not #rrggbb", "not contain or cover", "not one of the four corners", "scale must be 0.05 to 0.6"] {
        assert!(p.contains(word), "{word}: {p}");
    }
}

#[test]
fn a_segment_must_be_inside_one_run_and_say_what_it_is() {
    let p = problems(&edl_of(vec![
        one("spans the gap", 50, 250),
        one("dup", 0, 10),
        one("dup", 20, 30),
        one("backwards", 40, 30),
        one("past-the-end", 250, 400),
    ]));
    for word in [
        "an id is 1 to 64 letters",
        "is not inside one run of the recording",
        "the id \"dup\" is used twice",
        "it ends at 30000000 before it begins at 40000000",
    ] {
        assert!(p.contains(word), "{word}: {p}");
    }
    assert!(problems(&edl_of(Vec::new())).contains("no segments"));
    // Inside a run is fine, at its very edges too.
    assert_eq!(problems(&edl_of(vec![one("a", 0, 100), one("b", 200, 300)])), "");
}

#[test]
fn speed_zoom_gain_crossfade_and_caption_words_are_checked() {
    let mut s = one("a", 0, 10);
    s.speed = 20.0;
    s.zoom = Some(Zoom { x: 2800, y: 0, w: 200, h: 100, ease_ms: 400 });
    s.audio = AudioCut { gain_db: 90.0, crossfade_ms: 6_000 };
    s.caption = Some(Caption { text: String::from("x"), words: vec![CaptionWord { w: String::from("late"), t0_us: 11 * SEC, t1_us: 12 * SEC }] });
    let p = problems(&edl_of(vec![s]));
    for word in [
        "the speed 20 is not between 0.25 and 8",
        "is not inside the screen track's 2880x1800",
        "a gain of 90 dB",
        "a crossfade of 6000 ms is longer than half the segment",
        "caption word 0 (\"late\") is not inside the segment",
    ] {
        assert!(p.contains(word), "{word}: {p}");
    }
    let mut ok = one("a", 0, 10);
    ok.zoom = Some(Zoom { x: 100, y: 100, w: 1440, h: 900, ease_ms: 400 });
    ok.audio.crossfade_ms = 4_000;
    assert_eq!(problems(&edl_of(vec![ok])), "");
}

#[test]
fn chapters_must_be_in_order_and_inside_the_video() {
    let mut e = edl_of(vec![one("a", 0, 10)]);
    e.chapters = vec![Chapter { t_us: 5 * SEC, title: String::from("one") }, Chapter { t_us: 2 * SEC, title: String::from("two") }];
    assert!(problems(&e).contains("chapter 1 (\"two\") is out of order"));
    e.chapters = vec![Chapter { t_us: 11 * SEC, title: String::from("late") }];
    assert!(problems(&e).contains("past the end of the video"));
}

#[test]
fn the_length_of_a_segment_in_the_video_follows_its_speed() {
    let mut s = one("a", 0, 10);
    assert_eq!(s.duration_us(), 10 * SEC);
    s.speed = 2.0;
    assert_eq!(s.duration_us(), 5 * SEC);
    s.speed = 0.5;
    assert_eq!(s.duration_us(), 20 * SEC);
    s.speed = f64::NAN;
    assert_eq!(s.duration_us(), 10 * SEC, "a speed that is no number counts as 1 here; the check names it");
}
