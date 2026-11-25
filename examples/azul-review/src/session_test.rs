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
            InkPoint { x: 10.0, y: 20.5, pressure: 0.25, tilt_x: 1.0, tilt_y: -2.0 },
            InkPoint { x: 30.0, y: 40.0, pressure: 0.75, tilt_x: 0.0, tilt_y: 0.0 },
        ],
        id,
        epoch,
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
