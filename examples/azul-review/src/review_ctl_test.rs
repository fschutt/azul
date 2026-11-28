//! The key of the review session without a window: what it says in each state, the dot that is red only while audio is captured, the names a
//! screen reader reads, the arguments of the debug op, and the shortcuts being nobody else's.

use azul::{prelude::*, str::String as AzString};

use super::*;
use crate::{lib_test::app_with_file, review_session::Session};

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom).into_iter().filter_map(|n| n.root.node_type.get_text().into_option()).map(|t| t.as_str().to_string()).filter(|t| !t.trim().is_empty()).collect()
}

fn find<'a>(dom: &'a Dom, id: &AzString) -> Option<&'a Dom> {
    nodes(dom).into_iter().find(|n| n.root.has_id(id.clone()))
}

fn any() -> RefAny {
    RefAny::new(0u8)
}

fn app() -> AppState {
    crate::l10n::in_english();
    app_with_file()
}

const STATES: [State; 4] = [State::Idle, State::Recording, State::Paused, State::Stopped];

// ---- the key in the top bar ----

#[test]
fn a_window_that_was_not_started_shows_the_key_that_starts_and_nothing_that_stops_or_says_it_records() {
    let s = app();
    let dom = pill(&s, &any());
    let state = find(&dom, &ids::REVIEW_STATE).expect("the key that starts");
    assert!(find(&dom, &ids::REVIEW_GROUP).is_some());
    assert!(find(&dom, &ids::REVIEW_STOP).is_none(), "nothing to stop");
    assert!(find(&dom, &ids::REVIEW_DOT).is_none(), "no dot: a dot says that audio is captured");
    assert!(!state.root.accessibility.is_null(), "the key is named for a screen reader");
    let said = texts(&dom);
    assert!(said.iter().any(|t| t == "Start review"), "{said:?}");
    assert!(!said.iter().any(|t| t.contains("Recording")), "{said:?}");
}

#[test]
fn a_session_that_records_shows_its_length_a_dot_and_a_stop_key_each_with_a_name() {
    let mut s = app();
    s.voice.review = Session::recording_for_test(0);
    let dom = pill(&s, &any());
    for id in [&ids::REVIEW_STATE, &ids::REVIEW_STOP] {
        let key = find(&dom, id).expect("a key");
        assert!(!key.root.accessibility.is_null(), "{id:?} has no name");
    }
    assert!(find(&dom, &ids::REVIEW_DOT).is_some());
    let said = texts(&dom);
    assert!(said.iter().any(|t| t.starts_with("Recording ")), "{said:?}");
    assert!(said.iter().any(|t| t == "Stop"), "{said:?}");
}

#[test]
fn a_session_that_is_held_says_paused_and_still_has_stop() {
    let mut s = app();
    s.voice.review = Session::recording_for_test(0);
    s.voice.review.apply(Event::Pause, 5_000);
    let dom = pill(&s, &any());
    let said = texts(&dom);
    assert!(said.iter().any(|t| t == "Paused 0:05"), "{said:?}");
    assert!(find(&dom, &ids::REVIEW_STOP).is_some(), "a held session can be ended");
    assert!(!said.iter().any(|t| t.contains("Recording")), "{said:?}");
}

#[test]
fn an_ended_session_reads_like_none_and_a_new_one_needs_the_start_key() {
    let mut s = app();
    s.voice.review = Session::recording_for_test(0);
    s.voice.review.apply(Event::Stop, 1_000);
    let dom = pill(&s, &any());
    assert!(find(&dom, &ids::REVIEW_STOP).is_none());
    assert!(texts(&dom).iter().any(|t| t == "Start review"), "{:?}", texts(&dom));
}

// ---- the dot ----

#[test]
fn the_session_captures_audio_only_while_it_records_and_the_microphone_is_open() {
    for state in STATES {
        for mic in Mic::ALL {
            let open = matches!(mic, Mic::Armed | Mic::Listening);
            assert_eq!(capturing(state, mic), state == State::Recording && open, "{state:?} {mic:?}");
        }
    }
}

#[test]
fn audio_is_captured_when_a_voice_clip_records_whatever_the_session_says_and_otherwise_when_the_session_does() {
    for state in STATES {
        for mic in Mic::ALL {
            assert!(audio_captured(state, mic, true), "a clip records, {state:?} {mic:?}");
            assert_eq!(audio_captured(state, mic, false), capturing(state, mic), "{state:?} {mic:?}");
        }
    }
}

#[test]
fn the_dot_is_red_whenever_audio_is_captured_and_only_then() {
    for state in STATES {
        for captured in [false, true] {
            let css = dot_css(state, captured);
            assert_eq!(css.contains("system:danger"), captured, "{state:?} captured={captured}: {css}");
        }
    }
    assert!(dot_css(State::Paused, false).contains("system:warning"), "a held session that captures nothing is amber");
    assert!(dot_css(State::Recording, false).contains("system:separator"), "a session that records with the microphone closed is grey");
    assert!(dot_css(State::Idle, false).contains("system:separator") && dot_css(State::Stopped, false).contains("system:separator"));
    for state in STATES {
        for captured in [false, true] {
            assert!(!dot_css(state, captured).contains('#'), "a colour of its own instead of the theme's ink");
        }
    }
}

#[test]
fn a_voice_clip_that_records_with_no_session_shows_the_dot_on_the_key_that_starts_one() {
    let mut s = app();
    s.recording = Some(crate::model::VoiceClip::default());
    let dom = pill(&s, &any());
    assert!(find(&dom, &ids::REVIEW_DOT).is_some(), "audio is captured: the dot is there");
    assert!(texts(&dom).iter().any(|t| t == "Start review"), "{:?}", texts(&dom));
    assert!(find(&dom, &ids::REVIEW_STOP).is_none(), "the clip has its own Stop recording key; the session has none to stop");
    s.recording = None;
    assert!(find(&pill(&s, &any()), &ids::REVIEW_DOT).is_none(), "nothing is captured: no dot");
}

#[test]
fn a_voice_clip_that_records_in_a_held_session_makes_the_dot_red_not_amber() {
    // the pill's nodes cannot be read for their colour: the rule is `dot_css` of `audio_captured`
    let mut s = app();
    s.voice.review = Session::recording_for_test(0);
    s.voice.review.apply(Event::Pause, 1_000);
    s.recording = Some(crate::model::VoiceClip::default());
    let captured = audio_captured(s.voice.review.state(), s.voice.mic, s.recording.is_some());
    assert!(dot_css(s.voice.review.state(), captured).contains("system:danger"));
    assert!(find(&pill(&s, &any()), &ids::REVIEW_DOT).is_some());
}

// ---- the words ----

#[test]
fn every_state_has_its_words_and_its_name_with_the_shortcut_that_does_it() {
    crate::l10n::in_english();
    let start = words(State::Idle, 0);
    assert_eq!(start.text, "Start review");
    assert!(start.name.contains(&shown(KEYS_TOGGLE)), "{}", start.name);
    let recording = words(State::Recording, 42_000);
    assert_eq!(recording.text, "Recording 0:42");
    assert!(recording.name.contains("0:42") && recording.name.contains("pause") && recording.name.contains(&shown(KEYS_PAUSE)), "{}", recording.name);
    let held = words(State::Paused, 61_000);
    assert_eq!(held.text, "Paused 1:01");
    assert!(held.name.contains("1:01") && held.name.contains("resume") && held.name.contains(&shown(KEYS_PAUSE)), "{}", held.name);
    assert_eq!(words(State::Stopped, 5_000), start, "an ended session reads like none");
    assert!(stop_name().contains(&shown(KEYS_TOGGLE)) && stop_name().contains("microphone"), "{}", stop_name());
}

#[test]
fn the_words_are_there_in_german_too_and_name_the_same_keys() {
    crate::l10n::in_english();
    azul_appkit::l10n::set_locale("de-DE");
    let start = words(State::Idle, 0);
    let recording = words(State::Recording, 42_000);
    let held = words(State::Paused, 42_000);
    let stop = stop_name();
    // the keys as the same language shows them: a PC's key names are translated (Strg+Umschalt+M), a Mac's symbols are not (the first version of
    // this compared with the English ones and failed on Linux only)
    let (toggle, pause) = (shown(KEYS_TOGGLE), shown(KEYS_PAUSE));
    azul_appkit::l10n::set_locale("en-US");
    assert_eq!(start.text, "Review starten");
    assert_eq!(recording.text, "Aufnahme 0:42");
    assert_eq!(held.text, "Pausiert 0:42");
    assert!(start.name.contains(&toggle) && recording.name.contains(&pause) && held.name.contains(&pause), "{start:?} {recording:?} {held:?}");
    assert!(stop.contains(&toggle), "{stop}");
}

#[test]
fn the_shortcuts_are_listed_on_the_shortcuts_page_and_no_other_key_is_the_same() {
    let listed: Vec<&str> = crate::SHORTCUTS.iter().map(|s| s.keys).collect();
    for keys in [KEYS_TOGGLE, KEYS_PAUSE] {
        assert_eq!(listed.iter().filter(|k| **k == keys).count(), 1, "{keys} is listed once: {listed:?}");
    }
    let mut chords: Vec<&str> = listed.iter().copied().filter(|k| k.starts_with("Mod+")).collect();
    let all = chords.len();
    chords.sort_unstable();
    chords.dedup();
    assert_eq!(chords.len(), all, "two shortcuts are the same key: {listed:?}");
    // not the recorder window's global pause key (Cmd+Shift+P on a Mac): a key that is two things is neither
    assert!(!KEYS_PAUSE.ends_with("+P") && !KEYS_TOGGLE.ends_with("+P"));
}

// ---- the debug op ----

#[test]
fn the_ops_actions_are_the_four_explicit_events_and_a_toggle_that_follows_the_state() {
    for (action, event) in [("start", Event::Start), ("pause", Event::Pause), ("resume", Event::Resume), ("stop", Event::Stop)] {
        assert_eq!(parse_op(&format!("{{\"action\":\"{action}\"}}"), State::Idle), Ok(event), "{action}");
    }
    let toggle = "{\"action\":\"toggle\"}";
    assert_eq!(parse_op(toggle, State::Idle), Ok(Event::Start));
    assert_eq!(parse_op(toggle, State::Stopped), Ok(Event::Start));
    assert_eq!(parse_op(toggle, State::Recording), Ok(Event::Stop));
    assert_eq!(parse_op(toggle, State::Paused), Ok(Event::Stop));
}

#[test]
fn the_op_says_what_was_wrong_with_an_action_it_does_not_know_and_with_arguments_that_are_no_json() {
    let why = parse_op("{\"action\":\"hover\"}", State::Idle).expect_err("hover is no action");
    assert!(why.contains("start") && why.contains("stop"), "{why}");
    assert!(parse_op("{}", State::Idle).is_err(), "no action");
    assert!(parse_op("{\"action\":3}", State::Idle).is_err(), "an action that is no word");
    assert!(parse_op("not json", State::Idle).expect_err("no json").contains("JSON"));
    // none of the annotations is an action: the op cannot be made to start a session by what a hover is called
    for annotation in Event::ALL.into_iter().filter(|e| e.is_annotation()) {
        let name = format!("{annotation:?}").to_lowercase();
        assert!(parse_op(&format!("{{\"action\":\"{name}\"}}"), State::Idle).is_err(), "{name}");
    }
}
