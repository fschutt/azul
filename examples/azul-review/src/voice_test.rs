//! The dictation of a window: what a stop does to a running dictation and to the dwell. No window, no
//! thread (the worker's loop is `dictation_test.rs`'s).

use std::sync::atomic::Ordering;

use azul::{
    speech::{BuiltinSpeechStatus, HeardStatus, SpeechHeard, SpeechRecognizer, SpeechWord},
    vec::SpeechWordVec,
};

use super::*;
use crate::{dictation::Status, dwell::Config};

fn voice() -> Voice {
    Voice::new(Config::default(), 10_000)
}

#[test]
fn nothing_runs_until_a_dictation_starts() {
    let mut v = voice();
    assert!(!v.is_listening());
    assert_eq!(v.running_note(), None);
    assert!(!v.request_stop(Cause::Button), "nothing to stop");
}

#[test]
fn a_stop_is_asked_once_it_tells_the_worker_and_keeps_the_dictation_until_the_worker_ends() {
    let mut v = voice();
    v.running = Some(Running::for_test(1, 7));
    assert!(v.is_listening());
    assert_eq!(v.running_note(), Some(7));

    assert!(v.request_stop(Cause::Key));
    let running = v.running.as_ref().expect("still there: its worker has yet to finish its call");
    assert!(running.stop.load(Ordering::Relaxed), "the worker looks at this between two calls");
    assert_eq!(running.stopping, Some(Cause::Key));
    assert!(!v.is_listening(), "finishing is not listening");
    assert_eq!(v.running_note(), Some(7), "its words are still coming in");

    assert!(!v.request_stop(Cause::Button), "asked once: the first cause stays");
    assert_eq!(v.running.as_ref().and_then(|r| r.stopping), Some(Cause::Key));
}

#[test]
fn a_stop_frees_the_dwell_to_judge_the_pen_afresh() {
    use crate::dwell::{Anchor, Reaction, Sample};
    let mut v = voice();
    v.running = Some(Running::for_test(1, 1));
    v.dwell.session_started(Anchor { page: 0, x: 100.0, y: 50.0, contact: false });
    assert!(v.request_stop(Cause::Moved));
    // The session is over for the dwell: a pen far away is no news.
    let far = Sample { page: 0, x: 900.0, y: 50.0, pen: true, contact: false };
    assert_eq!(v.dwell.observe(far, 0), Reaction::Stay);
}

#[test]
fn the_clock_of_the_dwell_runs_forward() {
    let v = voice();
    let a = v.now_ms();
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert!(v.now_ms() > a);
}

#[test]
fn a_refusal_is_told_in_this_apps_words_with_the_engines_after_them() {
    crate::l10n::in_english();
    let denied = refusal(Status::Denied, "no answer to the prompt");
    assert!(denied.contains("not allowed") && denied.contains("no answer to the prompt"), "{denied}");
    let unavailable = refusal(Status::Unavailable, "no Info.plist");
    assert!(unavailable.starts_with("Dictation is not available"), "{unavailable}");
    assert!(unavailable.contains("no Info.plist"), "{unavailable}");
    let failed = refusal(Status::Failed, "the microphone did not start");
    assert!(failed.starts_with("Dictation failed"), "{failed}");
}

// ---- what the speech session answers, as the dictation sees it ----

fn answer(status: HeardStatus, recognizer: SpeechRecognizer, text: &str, words: &[(&str, u32, u32, f32)]) -> SpeechHeard {
    let items: Vec<SpeechWord> = words
        .iter()
        .map(|(w, s, e, p)| SpeechWord { text: (*w).to_string().into(), start_ms: *s, end_ms: *e, probability: *p })
        .collect();
    SpeechHeard {
        text: text.to_string().into(),
        message: "".to_string().into(),
        language: "en-US".to_string().into(),
        words: if items.is_empty() { SpeechWordVec::create() } else { SpeechWordVec::copy_from_ptr(&items[0], items.len()) },
        started_unix_ms: 1_764_000_000_000,
        confidence: 0.9,
        status,
        recognizer,
    }
}

#[test]
fn a_session_answer_keeps_its_words_with_their_times_and_says_who_answered() {
    let heard = heard_of(&answer(
        HeardStatus::Heard,
        SpeechRecognizer::Builtin,
        "enum await",
        &[("enum", 0, 300, 0.99), ("await", 320, 700, 0.8)],
    ));
    assert_eq!(heard.status, Status::Heard);
    assert_eq!(heard.text, "enum await");
    assert_eq!(heard.recognizer, crate::dictation::Recognizer::Builtin);
    assert_eq!(heard.started_unix_ms, 1_764_000_000_000);
    assert_eq!(heard.words.len(), 2);
    assert_eq!((heard.words[1].text.as_str(), heard.words[1].start_ms, heard.words[1].end_ms), ("await", 320, 700));
}

#[test]
fn every_status_and_every_recogniser_has_its_own_word() {
    for (from, to) in [
        (HeardStatus::Heard, Status::Heard),
        (HeardStatus::NothingHeard, Status::Nothing),
        (HeardStatus::Denied, Status::Denied),
        (HeardStatus::Unavailable, Status::Unavailable),
        (HeardStatus::Failed, Status::Failed),
    ] {
        assert_eq!(heard_of(&answer(from, SpeechRecognizer::Native, "", &[])).status, to);
    }
    for (from, to) in [
        (SpeechRecognizer::Native, crate::dictation::Recognizer::Native),
        (SpeechRecognizer::Builtin, crate::dictation::Recognizer::Builtin),
        (SpeechRecognizer::Scripted, crate::dictation::Recognizer::Scripted),
        (SpeechRecognizer::None, crate::dictation::Recognizer::None),
    ] {
        assert_eq!(heard_of(&answer(HeardStatus::Heard, from, "x", &[])).recognizer, to);
    }
}

#[test]
fn every_state_of_the_built_in_recogniser_has_its_own_word() {
    let words: std::collections::BTreeSet<&str> = [
        BuiltinSpeechStatus::Ready,
        BuiltinSpeechStatus::EngineMissing,
        BuiltinSpeechStatus::EngineIncompatible,
        BuiltinSpeechStatus::ModelMissing,
        BuiltinSpeechStatus::ModelRejected,
    ]
    .into_iter()
    .map(builtin_word)
    .collect();
    assert_eq!(words.len(), 5);
}
