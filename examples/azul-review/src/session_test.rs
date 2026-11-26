//! The session archive: what is written is what is read back, notes included, and an archive from
//! before notes existed (or with fields a later version adds) still loads. No window.

use azul::{json::Json, prelude::RefAny, vec::U8VecRef, zip::Zip};

use super::*;
use crate::{
    code::SourceFile,
    model::{InkPoint, Note, NoteState, Semantic, Stroke},
    ui, AppState,
};

fn state() -> AppState {
    let file = SourceFile {
        path: "src/f.rs".into(),
        display: "src/f.rs".to_string(),
        lines: (0..100).map(|i| format!("fn f{i}() {{}}")).collect(),
        big: None,
    };
    AppState::create(
        std::path::PathBuf::from("/work"),
        vec![file],
        RefAny::new(0u8),
        std::path::PathBuf::new(),
    )
}

fn stroke(id: u64, page: usize, epoch: u64, semantic: Semantic) -> Stroke {
    Stroke {
        page,
        semantic,
        points: vec![
            InkPoint { x: 10.0, y: 20.5, pressure: 0.25, tilt_x: 1.0, tilt_y: -2.0, t_ms: 0 },
            InkPoint { x: 30.0, y: 40.0, pressure: 0.75, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 },
        ],
        id,
        epoch,
        started_unix_ms: 0,
    }
}

fn note(id: u64, page: usize, text: &str) -> Note {
    Note {
        id,
        page,
        x: 120.5,
        y: 88.25,
        text: text.to_string(),
        language: "de-DE".to_string(),
        state: NoteState::Done,
        talk: crate::conversation::Conversation::default(),
    }
}

/// An archive whose `session.json` is `json`.
fn zip_of(json: &str) -> Vec<u8> {
    let mut zip = Zip::create();
    zip.add_file("session.json", bytes(json.as_bytes()));
    zip.to_bytes().as_slice().to_vec()
}

fn session_json(archive: &[u8]) -> Json {
    let zip = Zip::from_bytes(bytes(archive));
    let json = zip.get_file("session.json");
    Json::parse_bytes(U8VecRef::from(json.as_slice()))
        .into_result()
        .map_err(|_| "session.json parses")
        .expect("session.json is JSON")
}

#[test]
fn a_note_is_kept_in_the_archive_and_comes_back() {
    let mut s = state();
    s.strokes.push(stroke(7, 1, 2, Semantic::Issue));
    s.notes.push(note(3, 1, "the loop \"bound\"\nis off by one - \u{fc}berpr\u{fc}fen"));
    s.notes.push(Note::listening(4, 0, 10.0, 20.0, "en-US"));

    let (name, bytes) = archive(&s);
    assert_eq!(name, "src_f.rs.azreview.zip");
    let back = load(&bytes).expect("an archive this writer made loads");

    assert_eq!(back.file, "src/f.rs");
    assert_eq!(back.notes.len(), 2);
    assert_eq!(back.notes[0], s.notes[0], "the text, the anchor and the language survive");
    assert_eq!(back.notes[1].text, "");
    assert_eq!(
        back.notes[1].state,
        NoteState::Done,
        "a note saved while it listened comes back finished: nothing listens after a restart"
    );
    assert_eq!(back.strokes, s.strokes, "the ink travels with the notes");
}

#[test]
fn the_archive_says_which_line_of_the_file_a_note_is_anchored_to() {
    let mut s = state();
    // Page 1 starts at line 47; the third line of its code is 3 lines under the page's padding.
    s.notes.push(Note {
        y: ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0,
        ..note(1, 1, "here")
    });
    let (_, bytes) = archive(&s);
    let json = session_json(&bytes);
    let notes = json.get_key("notes").into_option().expect("a notes list");
    let first = notes.get_index(0).into_option().expect("one note");
    let line = first.get_key("line").into_option().and_then(|l| l.as_int().into_option());
    assert_eq!(line, Some(50), "line 47 + 3");
}

#[test]
fn the_writer_keeps_the_format_number_the_archives_before_it_have() {
    let (_, bytes) = archive(&state());
    let json = session_json(&bytes);
    let format = json.get_key("format").into_option().and_then(|f| f.as_string().into_option());
    assert_eq!(format.as_ref().map(|f| f.as_str()), Some("azreview/1"));
    assert!(json.get_key("notes").into_option().is_some_and(|n| n.is_array()), "notes: a list, empty or not");
}

#[test]
fn an_archive_from_before_notes_existed_still_loads() {
    let old = r#"{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[{"id":1,"page":0,"semantic":"issue","points":[{"x":1.0,"y":2.0,"pressure":0.5,"tilt_x":0.0,"tilt_y":0.0}]}],"findings":[],"clips":[]}"#;
    let back = load(&zip_of(old)).expect("an old archive loads");
    assert_eq!(back.file, "a.rs");
    assert!(back.notes.is_empty());
    assert_eq!(back.strokes.len(), 1);
    assert_eq!(back.strokes[0].semantic, Semantic::Issue);
    assert_eq!(back.strokes[0].epoch, 0, "no epoch in the old files: the first one");
}

#[test]
fn the_time_of_each_sample_and_the_moment_a_stroke_began_are_kept_and_come_back() {
    let mut s = state();
    let mut st = stroke(1, 0, 0, Semantic::Issue);
    st.started_unix_ms = 1_764_000_123_456;
    st.points[1].t_ms = 87;
    s.strokes.push(st);
    let back = load(&archive(&s).1).expect("reads back");
    assert_eq!(back.strokes, s.strokes);
    assert_eq!(back.strokes[0].started_unix_ms, 1_764_000_123_456);
    assert_eq!(back.strokes[0].points.iter().map(|p| p.t_ms).collect::<Vec<_>>(), vec![0, 87]);
}

#[test]
fn an_archive_from_before_the_times_reads_with_none() {
    let old = r#"{"format":"azreview/1","file":"a.rs","strokes":[{"id":1,"page":0,"semantic":"issue","points":[{"x":1.0,"y":2.0,"pressure":0.5,"tilt_x":0.0,"tilt_y":0.0}]}]}"#;
    let back = load(&zip_of(old)).expect("an old archive loads");
    assert_eq!(back.strokes[0].started_unix_ms, 0);
    assert_eq!(back.strokes[0].points[0].t_ms, 0);
    assert!(back.slashes.is_empty(), "no deletions in the old files");
}

fn deletion(id: u64) -> crate::deletion::Deletion {
    crate::deletion::Deletion {
        id,
        page: 1,
        started_unix_ms: 1_764_000_000_123,
        points: vec![
            InkPoint { x: 296.0, y: 206.5, pressure: 0.35, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 },
            InkPoint { x: 334.0, y: 204.5, pressure: 0.75, tilt_x: 0.0, tilt_y: 0.0, t_ms: 55 },
            InkPoint { x: 372.0, y: 202.5, pressure: 0.35, tilt_x: 0.0, tilt_y: 0.0, t_ms: 110 },
        ],
        words: vec![
            crate::deletion::Deleted { line: 58, start: 33, end: 43, text: "abcdefghij".to_string() },
            crate::deletion::Deleted { line: 58, start: 44, end: 47, text: "k\"m\n".to_string() },
        ],
    }
}

#[test]
fn the_words_a_slash_took_are_kept_in_the_archive_and_come_back_with_the_stroke_that_took_them() {
    let mut s = state();
    s.slashes.push(deletion(4));
    s.slashes.push(deletion(9));
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("reads back");
    assert_eq!(back.slashes, s.slashes, "the words with their bytes and the samples with their times");
    let json = session_json(&bytes);
    assert!(json.get_key("slashes").into_option().is_some_and(|n| n.is_array()), "a list, empty or not");
}

#[test]
fn the_next_stroke_id_comes_after_the_deletions_of_the_archive_too() {
    let mut s = state();
    s.strokes.push(stroke(2, 0, 0, Semantic::Issue));
    s.slashes.push(deletion(11));
    let back = load(&archive(&s).1).expect("reads back");
    assert_eq!(back.next_stroke_id(), 12);
}

#[test]
fn a_deletion_that_cannot_be_read_is_skipped_and_the_rest_kept() {
    let bad = r#"{"format":"azreview/1","file":"a.rs","slashes":[{"page":0,"words":[]},{"id":2,"page":0,"started":5,"points":[],"words":[{"line":3,"start":1,"end":4,"text":"abc"},{"line":"x"}]},7]}"#;
    let back = load(&zip_of(bad)).expect("loads");
    assert_eq!(back.slashes.len(), 1, "the one with an id");
    assert_eq!(back.slashes[0].words.len(), 1, "and the word that can be read");
    assert_eq!(back.slashes[0].started_unix_ms, 5);
}

#[test]
fn the_deletions_an_archive_holds_are_bounded() {
    let mut s = state();
    s.slashes = (1..=(crate::deletion::MAX_DELETIONS as u64 + 50)).map(deletion).collect();
    let back = load(&archive(&s).1).expect("reads back");
    assert_eq!(back.slashes.len(), crate::deletion::MAX_DELETIONS, "the cap, not a crash");
}

#[test]
fn fields_a_later_version_adds_are_ignored_not_refused() {
    let later = r#"{"format":"azreview/1","file":"a.rs","future":{"a":1},"strokes":[],"notes":[{"id":2,"page":0,"x":1.5,"y":2.5,"text":"ok","language":"","line":3,"colour":"teal","tags":["a"]}]}"#;
    let back = load(&zip_of(later)).expect("extra fields are fine");
    assert_eq!(back.notes.len(), 1);
    assert_eq!(back.notes[0].text, "ok");
}

#[test]
fn a_note_that_cannot_be_read_is_skipped_and_the_rest_kept() {
    let mixed = r#"{"format":"azreview/1","file":"a.rs","strokes":[],"notes":[{"id":1,"page":0,"x":1,"y":2,"text":"kept"},{"text":"no id, no place"},{"id":"x"},{"id":9,"page":0,"x":3,"y":4,"text":"also kept"}]}"#;
    let back = load(&zip_of(mixed)).expect("one bad note is not a bad archive");
    let texts: Vec<&str> = back.notes.iter().map(|n| n.text.as_str()).collect();
    assert_eq!(texts, vec!["kept", "also kept"]);
}

#[test]
fn what_is_no_archive_is_refused_with_a_sentence() {
    let not_zip = load(b"this is not a zip").expect_err("not an archive");
    assert!(not_zip.contains("archive"), "{not_zip}");

    let mut empty = Zip::create();
    empty.add_file("other.txt", bytes(b"x"));
    let no_session = load(&empty.to_bytes().as_slice().to_vec()).expect_err("no session.json");
    assert!(no_session.contains("session.json"), "{no_session}");

    let broken = load(&zip_of("{ not json")).expect_err("not JSON");
    assert!(broken.contains("session.json"), "{broken}");
}

#[test]
fn a_newer_format_is_refused_rather_than_misread() {
    let newer = r#"{"format":"azreview/9","file":"a.rs","strokes":[]}"#;
    let why = load(&zip_of(newer)).expect_err("a format this build does not know");
    assert!(why.contains("azreview/9"), "{why}");
}

#[test]
fn the_next_ids_come_after_the_ones_in_the_archive() {
    let mut s = state();
    s.strokes.push(stroke(7, 0, 0, Semantic::Scope));
    s.notes.push(note(12, 0, "n"));
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("loads");
    assert_eq!(back.next_stroke_id(), 8);
    assert_eq!(back.next_note_id(), 13);
    assert_eq!(Loaded::default().next_stroke_id(), 1, "an empty archive starts at 1");
    assert_eq!(Loaded::default().next_note_id(), 1);
}

#[test]
fn a_note_with_quotes_backslashes_and_control_characters_comes_back_as_it_was() {
    let mut s = state();
    let tricky = "she said \"no\"\\n\tand\u{1}left\r\nit \u{2028} \u{1f600} \u{fc}";
    s.notes.push(note(1, 0, tricky));
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("loads");
    assert_eq!(back.notes[0].text, tricky);
}

#[test]
fn the_numbers_of_a_stroke_are_written_short_and_read_back_the_same() {
    let mut s = state();
    s.strokes.push(Stroke {
        points: vec![
            InkPoint { x: 0.55, y: -3.0, pressure: 1.0, tilt_x: 0.01, tilt_y: 123_456.78, t_ms: 0 },
            InkPoint { x: f32::NAN, y: f32::INFINITY, pressure: 0.0, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 },
        ],
        ..stroke(1, 0, 0, Semantic::Issue)
    });
    let (_, bytes) = archive(&s);
    let zip = Zip::from_bytes(bytes_of(&bytes));
    let text = String::from_utf8(zip.get_file("session.json").as_slice().to_vec()).expect("UTF-8");
    assert!(text.contains("\"x\":0.55,\"y\":-3.0,\"pressure\":1.0,\"tilt_x\":0.01,\"tilt_y\":123456.78"), "{text}");
    assert!(text.contains("\"x\":0.0,\"y\":0.0"), "no number of a NaN or an infinity: {text}");
    let back = load(&bytes).expect("loads");
    assert_eq!(back.strokes[0].points[0].x, 0.55);
    assert_eq!(back.strokes[0].points[0].y, -3.0);
}

fn bytes_of(v: &[u8]) -> azul::vec::U8Vec {
    super::bytes(v)
}

/// Not a check: `cargo test -p AzReview --release -- --ignored --nocapture archive_timing` says what the
/// archive of a long session costs on the window's thread every time the pen lifts.
#[test]
#[ignore = "X-EDGE-C-2: a measurement, not a check (run by hand, see the doc comment)"]
fn archive_timing() {
    use std::time::Instant;
    for (strokes, per) in [(10usize, 300usize), (40, 300), (100, 300)] {
        let mut s = state();
        for id in 0..strokes {
            let mut st = stroke(id as u64 + 1, 0, 0, Semantic::Issue);
            st.points = (0..per)
                .map(|i| InkPoint {
                    x: 140.0 + i as f32 * 1.8,
                    y: 60.0 + (id % 40) as f32 * 14.0 + 5.0 * (i as f32 / 6.0).sin(),
                    pressure: 0.55,
                    tilt_x: 0.0,
                    tilt_y: 0.0,
                    t_ms: 0,
                })
                .collect();
            s.strokes.push(st);
        }
        s.findings = Vec::new();
        let clips: Vec<&crate::model::VoiceClip> = Vec::new();
        let t = Instant::now();
        let text = session_text(&s, &clips);
        let text_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let (_, bytes) = archive(&s);
        let all_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{strokes} strokes x {per} points = {} points: session.json {} KB in {text_ms:.2} ms, the whole archive {} KB in {all_ms:.2} ms",
            strokes * per,
            text.len() / 1024,
            bytes.len() / 1024
        );
    }

    // What a minute of recorded voice does to a save, and what the zip level changes.
    let mut s = state();
    let samples: Vec<f32> = (0..48_000 * 60)
        .map(|i| ((i as f32) * 0.05).sin() * 0.3 + ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.1)
        .collect();
    s.clips.push(crate::model::VoiceClip { samples, sample_rate: 48_000, stroke_ids: vec![1] });
    let clips: Vec<&crate::model::VoiceClip> = s.clips.iter().collect();
    let t = Instant::now();
    let wav_bytes = wav(&clips[0].samples, 48_000);
    println!("one minute of voice: wav {} KB encoded in {:.2} ms", wav_bytes.len() / 1024, t.elapsed().as_secs_f64() * 1000.0);
    let mut zip = Zip::create();
    zip.add_file("clip-0.wav", bytes(&wav_bytes));
    zip.add_file("session.json", bytes(session_text(&s, &clips).as_bytes()));
    for level in [0u8, 1, 6] {
        let t = Instant::now();
        let out = zip.to_bytes_with_level(level);
        println!("zip level {level}: {} KB in {:.2} ms", out.as_slice().len() / 1024, t.elapsed().as_secs_f64() * 1000.0);
    }
}

// ==== What the conversations keep ====

#[test]
fn the_conversation_of_a_note_travels_with_it() {
    use crate::{
        conversation::{Phase, Said, Turn, Who},
        dictation::HeardWord,
    };
    let mut s = state();
    let mut n = note(3, 0, "this loop is off by one");
    n.talk.hear(Said {
        text: "this loop is off by one".to_string(),
        started_unix_ms: 1_700_000_000_123,
        words: vec![HeardWord { text: "this".to_string(), start_ms: 40, end_ms: 310, probability: 0.75, biased: false }],
    });
    n.talk.turns.push(Turn { who: Who::Agent, text: "It is read \"twice\".\nSee line 4.".to_string(), at_unix_ms: 1_700_000_005_000 });
    n.talk.turns.push(Turn { who: Who::Reviewer, text: "and the second?".to_string(), at_unix_ms: 1_700_000_009_000 });
    n.talk.heard_as = Some("this loop is off by one".to_string());
    n.talk.phase = Phase::Suggested;
    n.talk.proposals = vec![
        crate::conversation::Proposal { suggestion: crate::agent_review::Suggestion::InsertLines { after_line: 1, lines: vec!["a".to_string()] }, decision: crate::conversation::Decision::Waiting },
        crate::conversation::Proposal { suggestion: crate::agent_review::Suggestion::InsertLines { after_line: 2, lines: vec!["b".to_string()] }, decision: crate::conversation::Decision::Waiting },
    ];
    n.talk.asked = 1;
    s.notes.push(n.clone());
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("it loads");
    assert_eq!(back.notes.len(), 1);
    let got = &back.notes[0].talk;
    assert_eq!(got.phase, Phase::Suggested);
    assert_eq!((got.pending(), got.asked), (2, 1));
    assert_eq!(got.turns, n.talk.turns, "the words, their speakers and their moments");
    assert_eq!(got.heard_as.as_deref(), Some("this loop is off by one"));
    assert_eq!(got.said.len(), 1);
    assert_eq!(got.said[0].started_unix_ms, 1_700_000_000_123);
    assert_eq!(got.said[0].words, n.talk.said[0].words);
}

#[test]
fn a_note_nobody_talked_to_writes_no_conversation_keys() {
    let mut s = state();
    s.notes.push(note(1, 0, "just a note"));
    let (_, bytes) = archive(&s);
    let json = session_json(&bytes);
    let first = json.get_key("notes").into_option().and_then(|n| n.get_index(0).into_option()).expect("one note");
    for key in ["phase", "turns", "said", "heard_as", "problem", "asked", "suggestions"] {
        assert!(first.get_key(key).into_option().is_none(), "{key} is not written for a draft nobody asked about");
    }
}

#[test]
fn an_agent_that_was_working_when_the_window_closed_comes_back_as_a_failure_to_ask_again() {
    use crate::conversation::Phase;
    let mut s = state();
    let mut n = note(1, 0, "rename this");
    n.talk.asking();
    s.notes.push(n);
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("it loads");
    assert_eq!(back.notes[0].talk.phase, Phase::Failed);
    assert!(back.notes[0].talk.problem.as_deref().is_some_and(|p| p.contains("closed")));
}

#[test]
fn an_archive_from_before_conversations_loads_its_notes_as_drafts() {
    use crate::conversation::Phase;
    let old = r#"{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{"id":4,"page":0,"x":10.0,"y":20.0,"line":2,"language":"en","text":"hello"}],"findings":[],"clips":[]}"#;
    let back = load(&zip_of(old)).expect("an old archive loads");
    assert_eq!(back.notes.len(), 1);
    assert_eq!(back.notes[0].talk.phase, Phase::Draft);
    assert!(back.notes[0].talk.turns.is_empty());
}

#[test]
fn a_conversation_in_an_archive_is_read_within_its_bounds() {
    use crate::conversation::{MAX_SAID, MAX_TURNS};
    // an archive somebody edited: far more turns and utterances than a conversation keeps
    let turns: Vec<String> = (0..MAX_TURNS + 50).map(|i| format!(r#"{{"who":"agent","text":"t{i}","at":{i}}}"#)).collect();
    let said: Vec<String> = (0..MAX_SAID + 50).map(|i| format!(r#"{{"text":"u{i}","started":{i}}}"#)).collect();
    let json = format!(
        r#"{{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{{"id":1,"page":0,"x":1.0,"y":2.0,"language":"en","text":"x","phase":"done","turns":[{}],"said":[{}]}}],"findings":[],"clips":[]}}"#,
        turns.join(","),
        said.join(",")
    );
    let back = load(&zip_of(&json)).expect("it loads");
    let talk = &back.notes[0].talk;
    assert_eq!(talk.turns.len(), MAX_TURNS);
    assert_eq!(talk.turns[0].text, "t50", "the newest are kept");
    assert_eq!(talk.said.len(), MAX_SAID);
    assert_eq!(talk.said[0].text, "u50");
}

#[test]
fn a_turn_or_a_phase_that_cannot_be_read_is_left_out_and_the_rest_kept() {
    use crate::conversation::Phase;
    let json = r#"{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{"id":1,"page":0,"x":1.0,"y":2.0,"language":"en","text":"x","phase":"sleeping","turns":[{"who":"robot","text":"a","at":1},{"who":"agent","text":"b","at":2},{"text":"no speaker"}]}],"findings":[],"clips":[]}"#;
    let back = load(&zip_of(json)).expect("it loads");
    let talk = &back.notes[0].talk;
    assert_eq!(talk.phase, Phase::Draft, "an unknown phase is a draft");
    assert_eq!(talk.turns.len(), 1);
    assert_eq!(talk.turns[0].text, "b");
}

#[test]
fn what_the_words_were_understood_as_travels_beside_each_utterance() {
    use crate::dictation::Said;
    let mut s = state();
    let mut n = note(3, 0, "compute stale flag and then rename it");
    n.talk.hear_understood(Said { text: "compute stale flag".to_string(), started_unix_ms: 10, words: Vec::new() }, Some("compute_stale_flag".to_string()));
    n.talk.hear_understood(Said { text: "and then rename it".to_string(), started_unix_ms: 20, words: Vec::new() }, None);
    s.notes.push(n);
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("it loads");
    let talk = &back.notes[0].talk;
    assert_eq!(talk.said.len(), 2);
    assert_eq!(talk.understood, vec![Some("compute_stale_flag".to_string()), None]);
    assert_eq!(talk.said[0].text, "compute stale flag", "the words as heard are what is kept as they were");
}

#[test]
fn an_archive_from_before_the_index_has_nothing_understood_and_keeps_its_utterances() {
    let old = r#"{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{"id":1,"page":0,"x":1.0,"y":2.0,"language":"en","text":"x","said":[{"text":"hello there","started":5}]}],"findings":[],"clips":[]}"#;
    let back = load(&zip_of(old)).expect("it loads");
    let talk = &back.notes[0].talk;
    assert_eq!(talk.said.len(), 1);
    assert_eq!(talk.understood, vec![None]);
}

#[test]
fn the_suggestions_of_an_agent_travel_with_what_the_reviewer_decided_about_each() {
    use crate::{
        agent_review::{Pos, Suggestion},
        conversation::{Decision, Phase, Proposal},
    };
    let mut s = state();
    let mut n = note(5, 0, "rename it and drop the stale one");
    n.talk.phase = Phase::Suggested;
    n.talk.proposals = vec![
        Proposal { suggestion: Suggestion::InsertLines { after_line: 41, lines: vec!["    let n = items.len();".to_string(), "    // two\"quoted\"".to_string()] }, decision: Decision::Accepted },
        Proposal {
            suggestion: Suggestion::ReplaceRange { start: Pos { line: 39, col: 8 }, end: Pos { line: 39, col: 11 }, old: "foo".to_string(), text: "bar".to_string() },
            decision: Decision::Waiting,
        },
        Proposal {
            suggestion: Suggestion::RemoveRange { start: Pos { line: 40, col: 4 }, end: Pos { line: 40, col: 9 }, old: "stale".to_string() },
            decision: Decision::Rejected,
        },
    ];
    s.notes.push(n.clone());
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("it loads");
    assert_eq!(back.notes[0].talk.proposals, n.talk.proposals, "the edits, their places and what was decided");
    assert_eq!(back.notes[0].talk.pending(), 1);
}

#[test]
fn a_suggestion_that_cannot_be_read_is_left_out_and_the_number_is_bounded() {
    use crate::conversation::MAX_PROPOSALS;
    let good = r#"{"k":"ins","after":3,"lines":["x"],"d":"waiting"}"#;
    let junk = [r#"{"k":"teleport","after":3}"#, r#"{"k":"ins"}"#, r#"{"k":"rem","sl":1,"sc":0,"el":1,"ec":3}"#, r#"{"k":"ins","after":1,"lines":["y"],"d":"maybe"}"#];
    let many: Vec<String> = std::iter::repeat(good.to_string()).take(MAX_PROPOSALS + 10).collect();
    let json = format!(
        r#"{{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{{"id":1,"page":0,"x":1.0,"y":2.0,"language":"en","text":"x","phase":"suggested","suggestions":[{},{}]}}],"findings":[],"clips":[]}}"#,
        junk.join(","),
        many.join(",")
    );
    let back = load(&zip_of(&json)).expect("it loads");
    let got = &back.notes[0].talk.proposals;
    assert_eq!(got.len(), MAX_PROPOSALS, "the readable ones, no more than a conversation keeps");
    assert!(got.iter().all(|p| matches!(&p.suggestion, crate::agent_review::Suggestion::InsertLines { after_line: 3, .. })));
}

#[test]
fn whether_a_resolution_was_written_and_which_patch_it_made_come_back_from_the_archive() {
    use crate::conversation::Phase;
    let mut s = state();
    let mut written = note(1, 0, "rename it");
    written.talk.phase = Phase::Done;
    written.talk.mark_logged(Some("20251126-054012-c1.patch".to_string()));
    let mut waiting = note(2, 0, "and this");
    waiting.talk.phase = Phase::Done;
    waiting.talk.logged = false;
    s.notes.push(written);
    s.notes.push(waiting);
    let (_, bytes) = archive(&s);
    let back = load(&bytes).expect("it loads");
    assert!(back.notes[0].talk.logged && !back.notes[0].talk.due(), "written: not written again");
    assert_eq!(back.notes[0].talk.patch.as_deref(), Some("20251126-054012-c1.patch"));
    assert!(back.notes[1].talk.due(), "a resolution that was never written (the window closed first) is written when it is read");
}

#[test]
fn a_conversation_finished_in_an_archive_from_before_the_log_counts_as_written() {
    use crate::conversation::Phase;
    let old = r#"{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{"id":4,"page":0,"x":10.0,"y":20.0,"line":2,"language":"en","text":"hello","phase":"done","turns":[{"who":"agent","text":"ok","at":1}]},{"id":5,"page":0,"x":10.0,"y":40.0,"line":4,"language":"en","text":"hi","phase":"suggested"}],"findings":[],"clips":[]}"#;
    let back = load(&zip_of(old)).expect("an old archive loads");
    assert_eq!(back.notes[0].talk.phase, Phase::Done);
    assert!(!back.notes[0].talk.due(), "it was finished before there was a log: no patch of it is made now");
    assert!(back.notes[1].talk.phase == Phase::Suggested && !back.notes[1].talk.due());
}

#[test]
fn a_word_the_recogniser_was_told_to_favour_is_kept_as_biased_and_the_others_say_nothing_of_it() {
    use crate::{
        conversation::Said,
        dictation::HeardWord,
    };
    let mut s = state();
    let mut n = note(1, 0, "compute stale flag");
    n.talk.hear(Said {
        text: "compute stale flag".to_string(),
        started_unix_ms: 5,
        words: vec![
            HeardWord { text: "compute_stale_flag".to_string(), start_ms: 0, end_ms: 300, probability: 0.3, biased: true },
            HeardWord { text: "flag".to_string(), start_ms: 300, end_ms: 600, probability: 0.9, biased: false },
        ],
    });
    s.notes.push(n);
    let (_, bytes) = archive(&s);
    let json = session_json(&bytes);
    let text = String::from_utf8_lossy(&bytes);
    let _ = text;
    let back = load(&bytes).expect("it loads");
    let words = &back.notes[0].talk.said[0].words;
    assert!(words[0].biased && !words[1].biased, "{words:?}");
    // only a word that is biased writes the key
    let note = json.get_key("notes").into_option().and_then(|n| n.get_index(0).into_option()).expect("a note");
    let said = note.get_key("said").into_option().and_then(|n| n.get_index(0).into_option()).expect("an utterance");
    let written = said.get_key("words").into_option().expect("words");
    assert!(written.get_index(0).into_option().and_then(|w| w.get_key("b").into_option()).is_some());
    assert!(written.get_index(1).into_option().and_then(|w| w.get_key("b").into_option()).is_none());
}

#[test]
fn the_revision_a_conversation_began_at_travels_with_it_and_what_is_no_hash_is_dropped() {
    let mut s = state();
    let mut n = note(1, 0, "rename it");
    n.talk.base = Some("0123456789abcdef0123456789abcdef01234567".to_string());
    s.notes.push(n);
    s.notes.push(note(2, 0, "and this"));
    let (_, bytes) = archive(&s);
    let json = session_json(&bytes);
    let second = json.get_key("notes").into_option().and_then(|n| n.get_index(1).into_option()).expect("a second note");
    assert!(second.get_key("base").into_option().is_none(), "a note with no revision writes none");
    let back = load(&bytes).expect("it loads");
    assert_eq!(back.notes[0].talk.base.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    assert_eq!(back.notes[1].talk.base, None);
    // an archive somebody edited: a base that is no hash is not kept
    let long = "a".repeat(200);
    for bad in ["../../etc/passwd", "not a hash", "", long.as_str()] {
        let text = format!(
            r#"{{"format":"azreview/1","root":"/w","file":"a.rs","strokes":[],"notes":[{{"id":1,"page":0,"x":1.0,"y":2.0,"language":"en","text":"x","base":"{bad}"}}],"findings":[],"clips":[]}}"#
        );
        let loaded = load(&zip_of(&text)).expect("it loads");
        assert_eq!(loaded.notes[0].talk.base, None, "{bad:?}");
    }
}
