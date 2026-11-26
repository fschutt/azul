//! The dictation of a window: what a stop does to a running dictation and to the dwell. No window, no
//! thread (the worker's loop is `dictation_test.rs`'s).

use std::sync::atomic::Ordering;

use azul::{
    speech::{BuiltinSpeechStatus, HeardStatus, SpeechHeard, SpeechRecognizer, SpeechWord},
    vec::SpeechWordVec,
};

use super::*;
use crate::{
    dictation::{Mic, Out, Status},
    dwell::{Allow, Config},
    standby::{self, Action, Standby},
};

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
fn a_dictation_takes_words_only_once_its_microphone_is_ready_and_not_while_it_finishes() {
    let mut v = voice();
    v.running = Some(Running::for_test(1, 7));
    assert!(v.is_listening(), "it is going");
    assert!(!v.is_ready(), "its microphone is still opening");
    assert_eq!(v.ready_note(), None);

    assert!(v.told(&Out::Ready));
    assert!(v.is_ready());
    assert_eq!(v.ready_note(), Some(7));

    assert!(v.request_stop(Cause::Button));
    assert!(!v.is_ready(), "finishing takes no new words");
    assert_eq!(v.ready_note(), None);
}

#[test]
fn what_the_worker_tells_of_the_microphone_and_the_words_so_far_is_kept_and_a_change_asks_for_a_frame() {
    let mut v = voice();
    v.running = Some(Running::for_test(1, 7));
    assert_eq!(v.mic, Mic::Cold);
    assert!(v.told(&Out::State(Mic::Armed)));
    assert_eq!(v.mic, Mic::Armed);
    assert!(!v.told(&Out::State(Mic::Armed)), "nothing changed: no frame");

    assert!(v.told(&Out::Ready));
    assert_eq!(v.mic, Mic::Listening, "a microphone that takes words is listening");
    assert!(!v.told(&Out::Ready), "told once");

    assert!(v.told(&Out::Partial("this lo".to_string())));
    assert_eq!(v.partial(), "this lo");
    assert!(!v.told(&Out::Partial("this lo".to_string())));
    assert!(v.told(&Out::Partial("this loop".to_string())));
    assert!(v.told(&Out::Text("this loop is".to_string())), "the words are the note's now");
    assert_eq!(v.partial(), "", "the words so far became the text");
}

#[test]
fn words_so_far_that_come_after_a_stop_are_not_shown_and_none_come_to_a_dictation_that_is_not_there() {
    let mut v = voice();
    assert!(!v.told(&Out::Partial("nobody asked".to_string())));
    assert_eq!(v.partial(), "");
    v.running = Some(Running::for_test(1, 1));
    assert!(v.request_stop(Cause::Key));
    assert!(!v.told(&Out::Partial("late".to_string())));
    assert_eq!(v.partial(), "");
}

#[test]
fn the_end_of_a_dictation_closes_the_microphone_it_only_knew_as_listening() {
    let mut v = voice();
    v.running = Some(Running::for_test(1, 1));
    assert!(v.told(&Out::Ready));
    v.ended_microphone();
    assert_eq!(v.mic, Mic::Cold);

    // what the engine itself said stays: it knows whether it is still armed
    v.running = Some(Running::for_test(2, 2));
    assert!(v.told(&Out::State(Mic::Armed)));
    v.ended_microphone();
    assert_eq!(v.mic, Mic::Armed);
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

// ---- the microphone's standby, as the window asks it ----

/// A voice whose session can open the microphone ahead, for the pointers `allow` lets dictate.
fn standing(allow: Allow) -> Voice {
    let mut v = Voice::new(Config { allow, ..Config::default() }, 10_000);
    v.can_arm = true;
    v
}

#[test]
fn a_window_whose_session_cannot_open_the_microphone_ahead_asks_for_nothing() {
    let mut v = voice();
    assert!(!v.can_arm, "nothing is asked until the session has the calls (and under the e2e server, only counted)");
    assert_eq!(v.start_standby(), None);
    assert_eq!(v.pen_near(true), None);
    assert_eq!(v.standby_tick(true), None);
    assert_eq!(v.window_deactivated(), None);
    assert_eq!(v.dictation_busy(true), None);
    assert!(!v.arming.is_armed());
    assert_eq!((v.arms, v.disarms), (0, 0));
    assert!(!v.wants_standby_timer());
    // the choice is kept all the same, for the settings to show
    let cfg = standby::Config { standby: Standby::Off, idle_ms: 9_000 };
    assert_eq!(v.set_standby(cfg), None);
    assert_eq!(v.arming.config(), cfg);
}

#[test]
fn the_pen_near_a_page_opens_the_microphone_once_and_it_is_known_as_open() {
    let mut v = standing(Allow::Pen);
    assert_eq!(v.pen_near(true), Some(Action::Arm));
    assert_eq!((v.arms, v.disarms), (1, 0));
    assert_eq!(v.mic, Mic::Armed);
    assert!(v.arming.is_armed());
    assert_eq!(v.pen_near(true), None, "its moves do not ask again");
    assert_eq!(v.arms, 1);
}

#[test]
fn only_a_pointer_that_may_dictate_opens_the_microphone() {
    let mut v = standing(Allow::Pen);
    assert_eq!(v.pen_near(false), None, "a mouse cannot start a dictation here");
    assert!(!v.arming.is_armed());
    let mut v = standing(Allow::Any);
    assert_eq!(v.pen_near(false), Some(Action::Arm));
    let mut v = standing(Allow::Off);
    assert_eq!(v.pen_near(true), None, "no dictation, nothing to be ready for");
}

#[test]
fn the_window_going_to_the_back_closes_it_and_the_microphone_is_known_as_closed() {
    let mut v = standing(Allow::Pen);
    v.pen_near(true);
    assert_eq!(v.window_deactivated(), Some(Action::Disarm));
    assert_eq!((v.arms, v.disarms), (1, 1));
    assert_eq!(v.mic, Mic::Cold);
}

#[test]
fn a_dictation_that_runs_holds_the_microphone_open_and_its_end_lets_it_go() {
    let mut v = standing(Allow::Pen);
    v.pen_near(true);
    assert_eq!(v.dictation_busy(true), None);
    assert_eq!(v.window_deactivated(), None, "not under a dictation");
    assert!(v.arming.is_armed());
    assert_eq!(v.dictation_busy(false), Some(Action::Disarm), "the window went to the back meanwhile");
}

#[test]
fn what_the_session_says_of_a_listening_microphone_is_not_taken_back_by_the_policy() {
    let mut v = standing(Allow::Pen);
    v.mic = Mic::Listening;
    v.pen_near(true);
    assert_eq!(v.mic, Mic::Listening);
    v.window_deactivated();
    assert_eq!(v.mic, Mic::Listening, "the dictation is still taking words");
}

#[test]
fn a_choice_in_the_settings_applies_at_once() {
    let mut v = standing(Allow::Pen);
    let cfg = |standby| standby::Config { standby, idle_ms: standby::DEFAULT_IDLE_MS };
    assert_eq!(v.set_standby(cfg(Standby::Always)), Some(Action::Arm));
    assert_eq!(v.set_standby(cfg(Standby::Off)), Some(Action::Disarm));
    assert_eq!(v.pen_near(true), None);
}

#[test]
fn the_standby_timer_is_wanted_for_the_pen_near_only_and_once() {
    let mut v = standing(Allow::Pen);
    assert!(!v.wants_standby_timer(), "nothing is open");
    v.pen_near(true);
    assert!(v.wants_standby_timer());
    v.standby_ticking = true;
    assert!(!v.wants_standby_timer(), "one timer looks after it");

    let mut always = standing(Allow::Pen);
    always.set_standby(standby::Config { standby: Standby::Always, idle_ms: standby::DEFAULT_IDLE_MS });
    assert!(always.arming.is_armed());
    assert!(!always.wants_standby_timer(), "nothing closes it: no timer looks");
}

#[test]
fn a_pen_that_is_not_in_the_tablets_range_is_noticed_by_the_tick_and_closes_the_microphone_after_the_grace() {
    let mut v = standing(Allow::Pen);
    v.pen_near(true);
    assert_eq!(v.standby_tick(true), None, "the pen is here");
    assert_eq!(v.standby_tick(false), None, "away, but within the grace");
    assert!(v.arming.is_armed());
}
