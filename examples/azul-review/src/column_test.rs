//! The markers on the pages and the dictation's settings, built without a window: where a marker sits, what the
//! settings page offers. (The conversations are tested in `conv_ui_test.rs`.)

use std::path::PathBuf;

use azul::{prelude::*, str::String as AzString};

use super::*;
use crate::{
    code::SourceFile,
    dictation::Mic,
    model::{Note, NoteState},
};

fn app() -> AppState {
    crate::l10n::in_english();
    let file = SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..100).map(|i| format!("fn f{i}() {{}}")).collect(),
        big: None,
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
        talk: crate::conversation::Conversation::default(),
    }
}

#[test]
fn a_marker_sits_on_its_anchor_and_the_markers_of_a_page_are_the_notes_of_that_page() {
    let notes = vec![done(1, 0, 100.0, "a"), done(2, 1, 50.0, "b"), done(3, 0, 200.0, "c")];
    let markers = markers(&notes, 0, None, &crate::line_map::LineMap::plain());
    assert_eq!(markers.len(), 2, "two notes are on page 0");
    assert!(with_id(&markers[0], "__azreview_marker-1").is_some());
    assert!(with_id(&markers[1], "__azreview_marker-3").is_some());
    // The numbers are the notes' places in the column: note 3 is the third card.
    assert!(texts(&markers[1]).iter().any(|t| t == "3"), "{:?}", texts(&markers[1]));
    let listening = super::markers(&notes, 0, Some(3), &crate::line_map::LineMap::plain());
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
