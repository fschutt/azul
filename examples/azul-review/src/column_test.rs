//! The side column of notes and the markers on the pages, built without a window: what the column
//! says when it is empty, what a card of a note holds while it listens and after, that every control
//! has a name for the screen reader, where a marker sits.

use std::path::PathBuf;

use azul::{prelude::*, str::String as AzString};

use super::*;
use crate::{
    code::SourceFile,
    dictation::{Cause, Mic},
    dwell::Config,
    model::{Note, NoteState},
    voice::Running,
};

fn app() -> AppState {
    crate::l10n::in_english();
    let file = SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..100).map(|i| format!("fn f{i}() {{}}")).collect(),
    };
    AppState::create(PathBuf::new(), vec![file], RefAny::new(0u8), PathBuf::new())
}

/// `dom` and every box inside it, in tree order.
fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn with_id<'a>(dom: &'a Dom, id: &str) -> Option<&'a Dom> {
    let id = AzString::from(id);
    nodes(dom).into_iter().find(|n| n.root.has_id(id.clone()))
}

/// The words inside `dom` (an icon's glyph leaf is empty: no word).
fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

fn done(id: u64, page: usize, y: f32, text: &str) -> Note {
    Note {
        id,
        page,
        x: 120.0,
        y,
        text: text.to_string(),
        language: "en-US".to_string(),
        state: NoteState::Done,
    }
}

#[test]
fn the_empty_column_says_how_to_start_with_the_rest_time_of_the_settings() {
    let s = app();
    let col = column(&s, &RefAny::new(0u8));
    let hint = with_id(&col, "__azreview_notes-hint").expect("a hint when there is no note");
    let said = texts(hint).join(" ");
    assert!(said.contains("1.2 s"), "{said}");
    assert!(said.contains("pen"), "{said}");
    assert!(with_id(&col, "__azreview_dictation-stop").is_none(), "nothing listens: no stop button");
}

#[test]
fn with_the_pen_off_the_empty_column_says_so() {
    let mut s = app();
    s.voice.dwell.set_config(Config { allow: Allow::Off, ..s.voice.dwell.config() });
    let col = column(&s, &RefAny::new(0u8));
    let said = texts(with_id(&col, "__azreview_notes-hint").expect("a hint")).join(" ");
    assert!(said.contains("off"), "{said}");
}

#[test]
fn a_finished_note_is_a_card_with_its_place_and_a_text_area_that_has_a_name() {
    let mut s = app();
    // The fourth line of page 0: 3 lines under the page's padding; its words are the user's to edit.
    s.notes.push(done(1, 0, ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0, "the bound is off by one"));
    let col = column(&s, &RefAny::new(0u8));
    assert!(with_id(&col, "__azreview_note-1").is_some(), "a card");
    let place = texts(with_id(&col, "__azreview_note-1").expect("a card")).join(" | ");
    assert!(place.contains("Page 1, line 4"), "{place}");
    let area = with_id(&col, "__azreview_note-text-1").expect("a text area for the words");
    assert!(!area.root.accessibility.is_null(), "the text area names itself to a screen reader");
    assert!(with_id(&col, "__azreview_notes-hint").is_none(), "no hint once there is a note");
}

#[test]
fn a_note_that_listens_shows_its_words_but_does_not_let_them_be_edited() {
    let mut s = app();
    let mut note = done(1, 0, 40.0, "so far");
    note.state = NoteState::Listening;
    s.notes.push(note);
    s.voice.running = Some(Running::for_test(1, 1));
    let col = column(&s, &RefAny::new(0u8));
    assert!(with_id(&col, "__azreview_note-text-1").is_none(), "the dictation owns the text while it listens");
    assert!(texts(&col).iter().any(|t| t == "so far"), "{:?}", texts(&col));
    assert!(with_id(&col, "__azreview_listening").is_some(), "the listening pill");
    let stop = with_id(&col, "__azreview_dictation-stop").expect("a stop button");
    assert!(!stop.root.accessibility.is_null(), "the stop button has a name");
}

#[test]
fn a_dictation_says_listening_when_its_microphone_is_ready_and_before_that_that_it_is_opening() {
    let mut s = app();
    let mut note = done(1, 0, 40.0, "");
    note.state = NoteState::Listening;
    s.notes.push(note);
    s.voice.running = Some(Running::for_test(1, 1));
    let col = column(&s, &RefAny::new(0u8));
    let pill = texts(with_id(&col, "__azreview_listening").expect("the pill")).join(" ");
    assert!(pill.contains("Opening the microphone"), "{pill}");
    assert!(!pill.contains("Listening"), "nobody can be heard yet: {pill}");
    assert!(with_id(&col, "__azreview_dictation-stop").is_some(), "a dictation that is opening can be called off");

    if let Some(r) = s.voice.running.as_mut() {
        r.ready = true;
    }
    let col = column(&s, &RefAny::new(0u8));
    let pill = texts(with_id(&col, "__azreview_listening").expect("the pill")).join(" ");
    assert!(pill.contains("Listening") && !pill.contains("Opening"), "{pill}");
}

#[test]
fn the_words_so_far_show_under_the_words_of_the_note_while_it_listens_and_nowhere_else() {
    let mut s = app();
    let mut note = done(1, 0, 40.0, "this loop");
    note.state = NoteState::Listening;
    s.notes.push(note);
    let mut running = Running::for_test(1, 1);
    running.ready = true;
    running.partial = "is off by".to_string();
    s.voice.running = Some(running);
    let col = column(&s, &RefAny::new(0u8));
    let live = with_id(&col, "__azreview_note-partial-1").expect("the words so far");
    assert_eq!(texts(live), vec!["is off by".to_string()]);
    assert!(texts(&col).iter().any(|t| t == "this loop"), "the text of the note stays: {:?}", texts(&col));

    // none while nothing is being said, and none for a note that is not the one that listens
    if let Some(r) = s.voice.running.as_mut() {
        r.partial.clear();
    }
    assert!(with_id(&column(&s, &RefAny::new(0u8)), "__azreview_note-partial-1").is_none());
    s.notes.push(done(2, 0, 80.0, "another"));
    if let Some(r) = s.voice.running.as_mut() {
        r.partial = "so far".to_string();
    }
    assert!(with_id(&column(&s, &RefAny::new(0u8)), "__azreview_note-partial-2").is_none());
}

#[test]
fn a_dictation_that_finishes_has_no_stop_button_and_says_so() {
    let mut s = app();
    s.notes.push(done(1, 0, 40.0, "words"));
    let mut running = Running::for_test(1, 1);
    running.stopping = Some(Cause::Key);
    s.voice.running = Some(running);
    let col = column(&s, &RefAny::new(0u8));
    assert!(with_id(&col, "__azreview_dictation-stop").is_none(), "it was stopped already");
    let pill = texts(with_id(&col, "__azreview_listening").expect("the pill stays until the last call ends")).join(" ");
    assert!(pill.contains("Finishing"), "{pill}");
}

#[test]
fn every_button_of_the_column_is_named_for_the_screen_reader() {
    let mut s = app();
    s.notes.push(done(1, 0, 40.0, "one"));
    s.notes.push(done(2, 0, 80.0, "two"));
    let col = column(&s, &RefAny::new(0u8));
    let buttons: Vec<&Dom> = nodes(&col)
        .into_iter()
        .filter(|n| matches!(n.root.node_type, NodeType::Button))
        .collect();
    assert_eq!(buttons.len(), 3, "a delete button per note and the column's own close key");
    for b in buttons {
        assert!(!b.root.accessibility.is_null(), "a button without a name: {:?}", texts(b));
    }
}

#[test]
fn a_marker_sits_on_its_anchor_and_the_markers_of_a_page_are_the_notes_of_that_page() {
    let notes = vec![done(1, 0, 100.0, "a"), done(2, 1, 50.0, "b"), done(3, 0, 200.0, "c")];
    let markers = markers(&notes, 0, None);
    assert_eq!(markers.len(), 2, "two notes are on page 0");
    assert!(with_id(&markers[0], "__azreview_marker-1").is_some());
    assert!(with_id(&markers[1], "__azreview_marker-3").is_some());
    // The numbers are the notes' places in the column: note 3 is the third card.
    assert!(texts(&markers[1]).iter().any(|t| t == "3"), "{:?}", texts(&markers[1]));
    let listening = super::markers(&notes, 0, Some(3));
    assert!(texts(&listening[1]).is_empty(), "the listening marker shows a microphone, not a number");
}

#[test]
fn the_dictation_settings_have_a_row_for_each_choice_and_the_nearest_preset_is_lit() {
    let s = app();
    let section = settings_section(&s, &RefAny::new(0u8));
    for id in ["__azreview_setting-input", "__azreview_setting-rest", "__azreview_setting-quiet"] {
        assert!(with_id(&section.content, id).is_some(), "{id}");
    }
    assert_eq!(section.category, 0, "the app's first category");
    assert_eq!(nearest(&REST_PRESETS, 1_200), 1);
    assert_eq!(nearest(&REST_PRESETS, 400), 0, "below the shortest: the shortest");
    assert_eq!(nearest(&REST_PRESETS, 2_400), 2, "between two: the nearer");
    assert_eq!(nearest(&SILENCE_PRESETS, 12_000), 1);
}

#[test]
fn the_settings_section_lists_the_recognisers_and_with_no_session_only_says_what_the_list_is() {
    let s = app();
    let section = settings_section(&s, &RefAny::new(0u8));
    let said = texts(&section.content);
    assert!(said.iter().any(|t| t.contains("Speech recognisers")), "{said:?}");
    assert!(with_id(&section.content, "__azreview_speech-status").is_some());
}

#[test]
fn the_settings_section_tells_the_state_of_the_microphone_in_words() {
    let mut s = app();
    let said = |s: &AppState| {
        let section = settings_section(s, &RefAny::new(0u8));
        let line = with_id(&section.content, "__azreview_mic-status").expect("the microphone's line");
        texts(line).join(" ")
    };
    assert_eq!(said(&s), "Microphone: closed");
    s.voice.mic = Mic::Warm;
    assert!(said(&s).contains("recogniser is loaded"), "{}", said(&s));
    s.voice.mic = Mic::Armed;
    assert!(said(&s).contains("open") && said(&s).contains("indicator"), "{}", said(&s));
    s.voice.mic = Mic::Listening;
    assert_eq!(said(&s), "Microphone: open, listening");
}

#[test]
fn the_standby_rows_are_offered_only_where_the_session_can_open_the_microphone_ahead() {
    let mut s = app();
    let section = settings_section(&s, &RefAny::new(0u8));
    for id in ["__azreview_setting-standby", "__azreview_setting-standby-idle"] {
        assert!(with_id(&section.content, id).is_none(), "{id}: a choice that does nothing is not one");
    }
    assert!(
        !texts(&section.content).iter().any(|t| t.contains("microphone indicator")),
        "no sentence about an indicator that does not light"
    );

    s.voice.can_arm = true;
    let section = settings_section(&s, &RefAny::new(0u8));
    for id in ["__azreview_setting-standby", "__azreview_setting-standby-idle"] {
        assert!(with_id(&section.content, id).is_some(), "{id}");
    }
    let said = texts(&section.content);
    for word in ["When the pen is near", "Always", "Close after the pen was away", "microphone indicator"] {
        assert!(said.iter().any(|t| t.contains(word)), "{word}: {said:?}");
    }
    assert_eq!(nearest(&IDLE_PRESETS, crate::standby::DEFAULT_IDLE_MS), 1, "30 s is a preset");
    assert_eq!(IDLE_PRESETS.len(), 4);
}
