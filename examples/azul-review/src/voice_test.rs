//! The dictation of a window: what a stop does to a running dictation and to the dwell. No window, no
//! thread (the worker's loop is `dictation_test.rs`'s).

use std::sync::atomic::Ordering;

use super::*;
use crate::dwell::Config;

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
