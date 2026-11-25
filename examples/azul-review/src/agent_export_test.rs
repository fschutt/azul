//! The export task: the prompt a recorded session becomes, and the cut list a reply is read as.
//! The times are the recorder's (microseconds on one clock), the screen is 2880 x 1800 and the clock
//! stops for twenty seconds in the middle of a ten-minute session. Text in, text out: no process.

use super::*;
use crate::{
    agent_backend::{never_cancelled, run_task, Control, ManualClock, Request},
    agent_mock::MockBackend,
    recorder_edl::identity_edl,
    recorder_manifest::{
        AppInfo, ClockInfo, HostInfo, Manifest, Run, Segment, Track, TrackStatus, Workspace,
    },
};

const S: u64 = 1_000_000;

fn word(w: &str, t0: f64, t1: f64) -> Word {
    Word {
        w: w.to_string(),
        t0_us: (t0 * 1e6).round() as u64,
        t1_us: (t1 * 1e6).round() as u64,
    }
}

fn speech(text: &str, words: Vec<Word>) -> Speech {
    let t0_us = words.first().map_or(0, |w| w.t0_us);
    let t1_us = words.last().map_or(0, |w| w.t1_us);
    Speech {
        t0_us,
        t1_us,
        text: text.to_string(),
        words,
    }
}

fn export() -> ExportRequest {
    ExportRequest {
        conversation: 0,
        language: "de".to_string(),
        session: SessionFacts {
            id: "2026-11-26_10-00-00".to_string(),
            duration_us: 600 * S,
            runs: vec![(0, 300 * S), (320 * S, 600 * S)],
            screen: (2880, 1800),
            camera: true,
        },
        summary: Summary {
            files: vec![
                FileSpan {
                    file: "src/lib.rs".to_string(),
                    t0_us: 0,
                    t1_us: 250 * S,
                },
                FileSpan {
                    file: "src/main.rs".to_string(),
                    t0_us: 250 * S,
                    t1_us: 600 * S,
                },
            ],
            conversations: vec![ConvSpan {
                id: 3,
                file: "src/lib.rs".to_string(),
                line: 39,
                t0_us: 20 * S,
                t1_us: 60 * S,
                outcome: "accepted".to_string(),
            }],
            strokes: vec![StrokeBox {
                t_us: 15 * S,
                bbox: [100, 200, 900, 260],
            }],
            markers: vec![Marker {
                t_us: 100 * S,
                label: "the interesting part".to_string(),
            }],
            idle: vec![(120 * S, 180 * S)],
        },
        transcript: vec![
            speech(
                "this function leaks the buffer",
                vec![
                    word("this", 10.0, 10.4),
                    word("function", 10.5, 11.1),
                    word("leaks", 11.2, 11.7),
                    word("the", 11.8, 11.9),
                    word("buffer", 12.0, 12.8),
                ],
            ),
            speech(
                "and here is the fix",
                vec![
                    word("and", 200.0, 200.2),
                    word("here", 200.3, 200.6),
                    word("is", 200.7, 200.8),
                    word("the", 200.9, 201.0),
                    word("fix", 201.1, 201.6),
                ],
            ),
        ],
        target: Target {
            length_s: Some(180),
            style: "a tight walkthrough for colleagues".to_string(),
        },
    }
}

fn built(
    export: ExportRequest,
    limits: &Limits,
) -> (
    Result<(String, String, Tools, Vec<String>), AgentError>,
    Request,
) {
    let request = Request::Export(export.clone());
    let result =
        build_call(&request, &export, limits).map(|c| (c.system, c.prompt, c.tools, c.diagnostics));
    (result, request)
}

fn prompt_of(export: ExportRequest) -> String {
    built(export, &Limits::default()).0.expect("builds").1
}

// ==== the prompt ====

#[test]
fn the_prompt_has_the_facts_the_summary_the_words_and_the_target_in_order() {
    let p = prompt_of(export());
    let at = |needle: &str| {
        p.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing in:\n{p}"))
    };
    let order = [
        at("Task: export"),
        at("Language of the titles: de"),
        at("Session 2026-11-26_10-00-00: 600.0 s on the clock; the screen is 2880x1800 px; camera: yes"),
        at("Runs (the clock stops between them; a segment may not cross a gap): 0.0-300.0, 320.0-600.0 s"),
        at("Target: about 180 s; style: a tight walkthrough for colleagues"),
        at("Files open"),
        at("Conversations with an agent"),
        at("The pen's strokes"),
        at("Moments the reviewer marked"),
        at("Idle (nothing happened)"),
        at("What was said"),
        at("Reply with the JSON object."),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{p}");
    assert!(p.contains("- 0.0-250.0: src/lib.rs\n"), "{p}");
    assert!(
        p.contains("- 20.0-60.0: #3 at src/lib.rs line 39: accepted\n"),
        "{p}"
    );
    assert!(p.contains("- 15.0: 100,200,900,260\n"), "{p}");
    assert!(p.contains("- 100.0: the interesting part\n"), "{p}");
    assert!(p.contains("Idle (nothing happened): 120.0-180.0 s"), "{p}");
    assert!(
        p.contains("[10.0-12.8] this function leaks the buffer\n"),
        "{p}"
    );
}

#[test]
fn the_instructions_ask_for_seconds_and_a_cut_list_and_no_captions() {
    let (result, _) = built(export(), &Limits::default());
    let (system, _, tools, _) = result.expect("builds");
    assert_eq!(
        tools,
        Tools::None,
        "the agent never sees video and reads no files"
    );
    for needle in [
        "CUT LIST",
        "in_s",
        "never span a gap between two runs",
        "Do not write captions",
        "zoom",
        "exactly one JSON object",
    ] {
        assert!(system.contains(needle), "{needle:?} missing in:\n{system}");
    }
}

#[test]
fn a_session_without_a_camera_a_target_or_a_summary_has_no_empty_sections() {
    let mut e = export();
    e.session.camera = false;
    e.target = Target::default();
    e.summary = Summary::default();
    e.transcript.clear();
    e.language = String::new();
    let p = prompt_of(e);
    assert!(p.contains("camera: no"), "{p}");
    assert!(p.contains("Target: as long as the story needs\n"), "{p}");
    for absent in [
        "Language of the titles",
        "Files open",
        "Conversations",
        "strokes",
        "marked",
        "Idle",
        "What was said",
    ] {
        assert!(!p.contains(absent), "{absent:?} in:\n{p}");
    }
}

#[test]
fn a_long_transcript_and_many_strokes_are_capped_and_thinned_until_the_prompt_fits() {
    let mut e = export();
    e.transcript = (0..2_000)
        .map(|i| {
            let t = f64::from(i);
            speech(
                &format!("line {i} {}", "word ".repeat(70)),
                vec![word("x", t, t + 0.5)],
            )
        })
        .collect();
    e.summary.strokes = (0..300_u32)
        .map(|i| StrokeBox {
            t_us: u64::from(i) * S,
            bbox: [0, 0, 10, 10],
        })
        .collect();
    let limits = Limits::default();
    let (result, _) = built(e, &limits);
    let (system, prompt, _, notes) = result.expect("trimmed to fit");
    assert!(
        system.len() + prompt.len() <= limits.max_prompt_bytes,
        "{}",
        system.len() + prompt.len()
    );
    assert!(
        notes.iter().any(|n| n.contains("2000 stretches of speech")),
        "{notes:?}"
    );
    assert!(notes.iter().any(|n| n.contains("300 strokes")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("trimmed")), "{notes:?}");
}

#[test]
fn a_prompt_that_cannot_be_thinned_to_fit_is_a_too_large_error() {
    let limits = Limits {
        max_prompt_bytes: 900,
        ..Limits::default()
    };
    match built(export(), &limits).0 {
        Err(AgentError::TooLarge { what, limit, .. }) => {
            assert_eq!((what.as_str(), limit), ("the prompt", 900))
        }
        other => panic!("{other:?}"),
    }
}

// ==== reading a cut list ====

fn parse(text: &str) -> (Result<CutList, AgentError>, Vec<String>) {
    parse_for(&export(), text)
}

fn parse_for(export: &ExportRequest, text: &str) -> (Result<CutList, AgentError>, Vec<String>) {
    let mut notes = Notes::new();
    let result = parse_body(export, text, &Limits::default(), &mut notes);
    (result, notes.finish())
}

const FULL: &str = r##"{
  "title": "Ein Puffer, der leckt",
  "segments": [
    {"in_s": 9.0, "out_s": 13.0, "title": "src/lib.rs", "camera": true,
     "zoom": {"x": 100, "y": 200, "w": 800, "h": 100}},
    {"in_s": 199.0, "out_s": 203.0, "title": "src/lib.rs", "camera": false, "speed": 2}
  ],
  "chapters": [{"t_s": 0, "title": "src/lib.rs"}, {"t_s": 250, "title": "src/main.rs"}]
}"##;

#[test]
fn a_cut_list_is_read_in_microseconds_with_its_zoom_fitted_and_its_captions_made_from_the_transcript(
) {
    let (result, notes) = parse(FULL);
    let list = result.expect("reads");
    assert_eq!(list.title, "Ein Puffer, der leckt");
    assert_eq!(list.segments.len(), 2);
    let first = &list.segments[0];
    assert_eq!((first.in_us, first.out_us), (9 * S, 13 * S));
    assert!(first.camera);
    // a wide, thin region of the 16:10 screen becomes a 16:10 region around the same centre
    assert_eq!(
        first.zoom,
        Some(Zoom {
            x: 100,
            y: 0,
            w: 800,
            h: 500,
            ease_ms: 400
        })
    );
    let caption = first.caption.as_ref().expect("the words said inside it");
    assert_eq!(caption.text, "this function leaks the buffer");
    assert_eq!(caption.words.len(), 5);
    assert_eq!(
        (caption.words[0].t0_us, caption.words[0].t1_us),
        (10 * S, 10_400_000)
    );
    let second = &list.segments[1];
    assert_eq!(
        (second.in_us, second.out_us, second.speed, second.camera),
        (199 * S, 203 * S, 2.0, false)
    );
    assert_eq!(
        second.caption.as_ref().map(|c| c.text.as_str()),
        Some("and here is the fix")
    );
    assert_eq!(
        list.chapters,
        vec![
            Chapter {
                t_us: 0,
                title: "src/lib.rs".to_string()
            },
            Chapter {
                t_us: 250 * S,
                title: "src/main.rs".to_string()
            }
        ]
    );
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn the_times_may_be_in_seconds_milliseconds_or_microseconds() {
    let text = r#"{"segments": [
        {"in_s": 1.5, "out_s": 4},
        {"in_ms": 5000, "out_ms": 8000},
        {"in_us": 9000000, "out_us": 12000000}]}"#;
    let list = parse(text).0.expect("reads");
    let times: Vec<(u64, u64)> = list.segments.iter().map(|s| (s.in_us, s.out_us)).collect();
    assert_eq!(
        times,
        vec![(1_500_000, 4 * S), (5 * S, 8 * S), (9 * S, 12 * S)]
    );
}

#[test]
fn the_cut_list_between_prose_and_a_fence_is_found_and_a_raw_line_break_is_repaired() {
    let text = "Here is the cut list:\n```json\n{\"title\": \"two\nlines\", \"segments\": [{\"in_s\": 1, \"out_s\": 5}]}\n```\nEnjoy.";
    let (result, notes) = parse(text);
    assert_eq!(result.expect("reads").title, "two\nlines");
    assert!(notes.iter().any(|n| n.contains("repaired")), "{notes:?}");
}

#[test]
fn a_segment_that_spans_a_gap_between_two_runs_is_cut_at_the_gap() {
    let (result, notes) = parse(r#"{"segments": [{"in_s": 290, "out_s": 330, "title": "x"}]}"#);
    let list = result.expect("reads");
    let times: Vec<(u64, u64)> = list.segments.iter().map(|s| (s.in_us, s.out_us)).collect();
    assert_eq!(times, vec![(290 * S, 300 * S), (320 * S, 330 * S)]);
    assert!(notes.iter().any(|n| n.contains("gap")), "{notes:?}");
}

#[test]
fn what_lies_in_the_gap_or_past_the_end_is_dropped_or_clamped() {
    let (result, notes) =
        parse(r#"{"segments": [{"in_s": 305, "out_s": 315}, {"in_s": 590, "out_s": 9000}]}"#);
    let list = result.expect("the second one is usable");
    assert_eq!(list.segments.len(), 1);
    assert_eq!(
        (list.segments[0].in_us, list.segments[0].out_us),
        (590 * S, 600 * S),
        "clamped to the end of the session"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("outside the recorded runs")),
        "{notes:?}"
    );
}

#[test]
fn segments_are_put_in_time_order_and_trimmed_where_they_overlap() {
    let text = r#"{"segments": [
        {"in_s": 100, "out_s": 140},
        {"in_s": 20, "out_s": 60},
        {"in_s": 130, "out_s": 170}]}"#;
    let (result, notes) = parse(text);
    let list = result.expect("reads");
    let times: Vec<(u64, u64)> = list.segments.iter().map(|s| (s.in_us, s.out_us)).collect();
    assert_eq!(
        times,
        vec![(20 * S, 60 * S), (100 * S, 140 * S), (140 * S, 170 * S)]
    );
    assert!(
        notes.iter().any(|n| n.contains("inside the one before")),
        "{notes:?}"
    );
}

#[test]
fn a_segment_with_nothing_left_or_with_no_length_is_dropped_with_a_note() {
    let text = r#"{"segments": [
        {"in_s": 50, "out_s": 40},
        {"in_s": 60, "out_s": 60.1},
        {"in_s": 70},
        {"in_s": 80, "out_s": 90}]}"#;
    let (result, notes) = parse(text);
    let list = result.expect("one is usable");
    assert_eq!(list.segments.len(), 1);
    assert_eq!(list.segments[0].in_us, 80 * S);
    assert_eq!(notes.len(), 3, "{notes:?}");
}

#[test]
fn a_cut_inside_a_word_moves_to_the_edge_of_the_word() {
    // 10.8 is inside "function" (10.5 to 11.1); 11.5 is inside "leaks" (11.2 to 11.7)
    let list = parse(r#"{"segments": [{"in_s": 10.8, "out_s": 11.5}]}"#)
        .0
        .expect("reads");
    assert_eq!(
        (list.segments[0].in_us, list.segments[0].out_us),
        (10_500_000, 11_700_000)
    );
    assert_eq!(
        list.segments[0].caption.as_ref().map(|c| c.text.as_str()),
        Some("function leaks")
    );
}

#[test]
fn a_cut_between_words_stays_where_it_is() {
    let list = parse(r#"{"segments": [{"in_s": 11.15, "out_s": 11.75}]}"#)
        .0
        .expect("reads");
    assert_eq!(
        (list.segments[0].in_us, list.segments[0].out_us),
        (11_150_000, 11_750_000)
    );
}

#[test]
fn a_zoom_is_fitted_to_the_screen_in_every_case() {
    let zoom_of = |z: &str| -> (Option<Zoom>, Vec<String>) {
        let text = format!("{{\"segments\": [{{\"in_s\": 20, \"out_s\": 40, \"zoom\": {z}}}]}}");
        let (result, notes) = parse(&text);
        (result.expect("reads").segments[0].zoom.clone(), notes)
    };
    // a region already of the screen's shape stays
    assert_eq!(
        zoom_of(r#"{"x": 400, "y": 300, "w": 1440, "h": 900}"#).0,
        Some(Zoom {
            x: 400,
            y: 300,
            w: 1440,
            h: 900,
            ease_ms: 400
        })
    );
    // outside the screen: moved inside, its size kept
    assert_eq!(
        zoom_of(r#"{"x": 5000, "y": 5000, "w": 400, "h": 250}"#).0,
        Some(Zoom {
            x: 2480,
            y: 1550,
            w: 400,
            h: 250,
            ease_ms: 400
        })
    );
    // bigger than the screen: the whole screen
    assert_eq!(
        zoom_of(r#"{"x": 0, "y": 0, "w": 9000, "h": 9000}"#).0,
        Some(Zoom {
            x: 0,
            y: 0,
            w: 2880,
            h: 1800,
            ease_ms: 400
        })
    );
    // another ease
    assert_eq!(
        zoom_of(r#"{"x": 0, "y": 0, "w": 1440, "h": 900, "ease_ms": 900}"#)
            .0
            .map(|z| z.ease_ms),
        Some(900)
    );
    // no region at all
    let (none, notes) = zoom_of(r#"{"x": 10, "y": 10, "w": 0, "h": 0}"#);
    assert_eq!(none, None);
    assert!(
        notes.iter().any(|n| n.contains("zoom was dropped")),
        "{notes:?}"
    );
}

#[test]
fn a_speed_is_held_to_its_range_and_a_camera_that_is_not_there_is_not_asked_for() {
    let mut e = export();
    e.session.camera = false;
    let text = r#"{"segments": [{"in_s": 20, "out_s": 30, "speed": 10, "camera": true}, {"in_s": 40, "out_s": 50, "speed": 0.1}]}"#;
    let (result, notes) = parse_for(&e, text);
    let list = result.expect("reads");
    assert_eq!((list.segments[0].speed, list.segments[1].speed), (4.0, 0.5));
    assert!(!list.segments[0].camera && !list.segments[1].camera);
    assert!(notes.iter().any(|n| n.contains("held to 4")), "{notes:?}");
    assert!(
        notes.iter().any(|n| n.contains("the session has none")),
        "{notes:?}"
    );
}

#[test]
fn chapters_that_are_missing_are_made_from_the_titles_of_the_segments() {
    let text = r#"{"segments": [
        {"in_s": 20, "out_s": 40, "title": "a"},
        {"in_s": 50, "out_s": 70, "title": "a"},
        {"in_s": 80, "out_s": 90, "title": "b"}]}"#;
    let (result, notes) = parse(text);
    assert_eq!(
        result.expect("reads").chapters,
        vec![
            Chapter {
                t_us: 20 * S,
                title: "a".to_string()
            },
            Chapter {
                t_us: 80 * S,
                title: "b".to_string()
            }
        ]
    );
    assert!(notes.iter().any(|n| n.contains("chapters")), "{notes:?}");
}

#[test]
fn chapters_beyond_the_session_are_dropped_and_the_rest_is_in_order() {
    let text = r#"{"segments": [{"in_s": 20, "out_s": 40}], "chapters": [{"t_s": 500, "title": "late"}, {"t_s": 9999, "title": "beyond"}, {"t_s": 10, "title": "early"}, {"t_s": 5, "title": ""}]}"#;
    let list = parse(text).0.expect("reads");
    let titles: Vec<&str> = list.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, vec!["early", "late"]);
}

#[test]
fn a_list_longer_than_the_target_is_said_to_be() {
    let mut e = export();
    e.target.length_s = Some(30);
    let (result, notes) = parse_for(&e, r#"{"segments": [{"in_s": 20, "out_s": 120}]}"#);
    assert_eq!(result.expect("reads").output_us(), 100 * S);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("100 s long") && n.contains("about 30 s")),
        "{notes:?}"
    );
}

#[test]
fn the_length_of_the_video_counts_each_segment_at_its_speed() {
    let list = parse(
        r#"{"segments": [{"in_s": 20, "out_s": 40, "speed": 2}, {"in_s": 50, "out_s": 60}]}"#,
    )
    .0
    .expect("reads");
    assert_eq!(list.output_us(), 20 * S);
}

#[test]
fn a_reply_with_no_usable_segment_or_no_cut_list_is_a_bad_output_with_an_excerpt() {
    for text in [
        "",
        "no cut list here, just words",
        "{\"title\": \"x\"}",
        "{\"segments\": []}",
        "{\"segments\": [{\"in_s\": 5, \"out_s\": 1}]}",
        "{\"segments\": [{\"in_s\": 1",
    ] {
        match parse(text).0 {
            Err(AgentError::BadOutput { .. }) => {}
            other => panic!("{text:?}: {other:?}"),
        }
    }
}

#[test]
fn more_segments_than_the_cap_keep_the_first_ones_and_say_so() {
    let many: Vec<String> = (0..260)
        .map(|i| format!("{{\"in_s\": {}, \"out_s\": {}}}", i, f64::from(i) + 0.5))
        .collect();
    let (result, notes) = parse(&format!("{{\"segments\": [{}]}}", many.join(",")));
    assert_eq!(
        result.expect("reads").segments.len(),
        Limits::default().max_cut_segments
    );
    assert!(
        notes.iter().any(|n| n.contains("260 segments")),
        "{notes:?}"
    );
}

#[test]
fn a_cut_list_written_by_this_module_reads_back_the_same() {
    let list = parse(FULL).0.expect("reads");
    let again = parse(&cut_list_text(&list)).0.expect("reads back");
    assert_eq!(again, list);
}

// ==== the recorder's cut list ====

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

fn piece(file: &str, start: u64, end: u64) -> Segment {
    Segment {
        file: file.into(),
        start_us: start * S,
        end_us: Some(end * S),
        bytes: 1_000,
        samples: 10,
        complete: true,
        recovered: false,
        width: 0,
        height: 0,
    }
}

/// The session `export()` is about: two runs (0 to 300 s and 320 to 600 s), a screen of 2880 x 1800,
/// a camera and a voice.
fn session() -> Manifest {
    let mut m = Manifest::begin(
        AppInfo {
            name: String::from("AzReview"),
            version: String::from("0.1.0"),
        },
        "2026-11-26_10-00-00",
        Workspace {
            root: String::from("/w"),
            name: String::from("azul"),
        },
        ClockInfo {
            unit: String::from("us"),
            origin_unix_ms: 0,
            origin_iso8601: String::from("1970-01-01T00:00:00Z"),
            utc_offset_minutes: 0,
        },
        HostInfo {
            os: String::from("macos"),
            arch: String::from("aarch64"),
        },
    );
    let both = |dir: &str| {
        vec![
            piece(&format!("{dir}/0001.mp4"), 0, 300),
            piece(&format!("{dir}/0002.mp4"), 320, 600),
        ]
    };
    m.tracks.screen = Some(track(true, 2880, 1800, both("screen")));
    m.tracks.camera = Some(track(true, 1280, 720, both("camera")));
    m.tracks.audio = Some(track(false, 0, 0, both("audio")));
    m.runs = vec![
        Run {
            start_us: 0,
            end_us: Some(300 * S),
            end_reason: Some(String::from("pause")),
        },
        Run {
            start_us: 320 * S,
            end_us: Some(600 * S),
            end_reason: Some(String::from("stop")),
        },
    ];
    m
}

fn base() -> Edl {
    identity_edl(&session(), &[])
}

#[test]
fn the_cut_list_is_put_on_the_recorders_own_edl_on_top_of_what_the_recorder_decides() {
    let list = parse(FULL).0.expect("reads");
    let base = base();
    let edl = list.to_edl(&base, Some("claude-haiku via claude -p"));
    // what the recorder decides is kept
    assert_eq!(edl.schema, 1);
    assert_eq!(edl.session, "2026-11-26_10-00-00");
    assert_eq!(edl.output, base.output);
    assert_eq!(edl.style, base.style);
    assert_eq!(edl.captions, base.captions);
    // what the agent decides is put on it
    assert_eq!(edl.title.as_deref(), Some("Ein Puffer, der leckt"));
    assert_eq!(edl.generator.name, "agent");
    assert_eq!(edl.generator.version, 1);
    assert_eq!(
        edl.generator.notes.as_deref(),
        Some("claude-haiku via claude -p")
    );
    assert_eq!(edl.segments.len(), 2);
    let (a, b) = (&edl.segments[0], &edl.segments[1]);
    assert_eq!((a.id.as_str(), a.in_us, a.out_us), ("s1", 9 * S, 13 * S));
    assert_eq!((b.id.as_str(), b.in_us, b.out_us), ("s2", 199 * S, 203 * S));
    assert!(a.camera && !b.camera);
    assert_eq!((a.speed, b.speed), (1.0, 2.0));
    assert_eq!(a.zoom, list.segments[0].zoom);
    assert_eq!(a.caption, list.segments[0].caption);
    assert_eq!(a.title.as_deref(), Some("src/lib.rs"));
    assert_eq!(a.audio, AudioCut::default(), "the sound is as recorded");
    assert_eq!(edl.duration_us(), 6 * S, "4 s and 4 s at twice the speed");
}

#[test]
fn what_the_agent_makes_passes_the_recorders_strict_check_and_reads_back_as_itself() {
    let list = parse(FULL).0.expect("reads");
    let edl = list.to_edl(&base(), None);
    let m = session();
    assert_eq!(edl.problems(&m), Vec::<String>::new());
    let again = Edl::parse(&edl.to_json(), &m).expect("the recorder reads it");
    assert_eq!(again, edl);
}

#[test]
fn chapters_move_from_the_session_clock_to_the_videos() {
    let mut list = parse(FULL).0.expect("reads");
    // pieces: 9 to 13 s (4 s long in the video: 0 to 4 s) and 199 to 203 s at twice the speed (2 s:
    // 4 to 6 s of the video)
    list.chapters = vec![
        Chapter {
            t_us: 0,
            title: "before the first piece".to_string(),
        },
        Chapter {
            t_us: 11 * S,
            title: "inside the first".to_string(),
        },
        Chapter {
            t_us: 100 * S,
            title: "in the stretch that was cut away".to_string(),
        },
        Chapter {
            t_us: 201 * S,
            title: "inside the second, at its middle".to_string(),
        },
        Chapter {
            t_us: 250 * S,
            title: "after the last piece".to_string(),
        },
    ];
    let edl = list.to_edl(&base(), None);
    let places: Vec<(u64, &str)> = edl
        .chapters
        .iter()
        .map(|c| (c.t_us, c.title.as_str()))
        .collect();
    assert_eq!(
        places,
        vec![
            (0, "before the first piece"),
            (2 * S, "inside the first"),
            (4 * S, "in the stretch that was cut away"),
            (5 * S, "inside the second, at its middle"),
        ],
        "the one after the last piece has no place in the video"
    );
}

#[test]
fn two_chapters_in_one_cut_away_stretch_leave_the_one_that_began_last() {
    let mut list = parse(FULL).0.expect("reads");
    list.chapters = vec![
        Chapter {
            t_us: 100 * S,
            title: "first".to_string(),
        },
        Chapter {
            t_us: 150 * S,
            title: "second".to_string(),
        },
    ];
    let edl = list.to_edl(&base(), None);
    let places: Vec<(u64, &str)> = edl
        .chapters
        .iter()
        .map(|c| (c.t_us, c.title.as_str()))
        .collect();
    assert_eq!(places, vec![(4 * S, "second")]);
}

#[test]
fn chapters_given_out_of_order_come_out_in_the_videos_order() {
    let mut list = parse(FULL).0.expect("reads");
    list.chapters = vec![
        Chapter {
            t_us: 200 * S,
            title: "late".to_string(),
        },
        Chapter {
            t_us: 10 * S,
            title: "early".to_string(),
        },
    ];
    let edl = list.to_edl(&base(), None);
    let titles: Vec<&str> = edl.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, vec!["early", "late"]);
    assert!(edl.chapters[0].t_us < edl.chapters[1].t_us);
    assert_eq!(edl.problems(&session()), Vec::<String>::new());
}

#[test]
fn a_cut_list_without_a_title_keeps_the_recorders_and_the_speed_is_written_plainly() {
    let mut list = parse(FULL).0.expect("reads");
    list.title = "  ".to_string();
    list.segments[1].speed = 1.3;
    let base = base();
    let edl = list.to_edl(&base, None);
    assert_eq!(edl.title, base.title);
    assert_eq!(edl.segments[1].speed, 1.3, "not 1.2999999523162842");
}

#[test]
fn a_cut_list_with_no_segment_is_an_edl_the_recorder_refuses_in_words() {
    let list = CutList {
        title: "Nothing".to_string(),
        segments: Vec::new(),
        chapters: Vec::new(),
    };
    let edl = list.to_edl(&base(), None);
    let problems = edl.problems(&session());
    assert!(
        problems.iter().any(|p| p.contains("no segments")),
        "{problems:?}"
    );
}

#[test]
fn what_the_parser_lets_through_is_always_sound_for_the_session() {
    // a model's worst: a segment across the gap between the runs, one outside the session, a zoom
    // off the screen, a speed beyond the range, captions around a cut word
    let text = r#"{"title": "Wild", "segments": [
        {"in_s": 290, "out_s": 330, "zoom": {"x": 9000, "y": 9000, "w": 100, "h": 100}, "speed": 50},
        {"in_s": 595, "out_s": 900, "camera": true, "zoom": {"x": 0, "y": 0, "w": 9999, "h": 9999}},
        {"in_s": 10.2, "out_s": 11.0, "speed": 0.01}
      ], "chapters": [{"t_s": -5, "title": "negative"}, {"t_s": 99999, "title": "far"}]}"#;
    let list = parse(text).0.expect("reads");
    let edl = list.to_edl(&base(), None);
    assert_eq!(edl.problems(&session()), Vec::<String>::new(), "{edl:#?}");
}

// ==== through the boundary ====

#[test]
fn the_mock_answers_an_export_with_one_fixed_piece_through_the_same_reader() {
    let request = Request::Export(export());
    let mut backend = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    let response = run_task(&mut backend, &request, &Limits::default(), &mut ctl).expect("answers");
    let list = response.cut_list().expect("a cut list");
    assert_eq!(list.title, "Mock cut");
    assert_eq!(list.segments.len(), 1);
    assert_eq!(
        (list.segments[0].in_us, list.segments[0].out_us),
        (0, 5 * S)
    );
    assert!(list.segments[0].camera);
    assert_eq!(list.chapters.len(), 1);
    assert_eq!(response.first_words(40), "1 segments: Mock cut");
    assert!(response.reply().is_none());
    assert_eq!(request.task(), crate::agent_backend::Task::Export);
}
