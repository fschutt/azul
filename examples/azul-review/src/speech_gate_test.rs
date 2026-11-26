//! Where the window's speech session stands while a thread makes it, and what a dictation asked in that time does: it
//! waits for the session (a state, not a race), and a session that cannot be made is told once. Time is passed in; no
//! session, no thread and no microphone here.

use super::*;

/// What a dictation is asked at: a stand-in for the pen's rest.
type Rest = (u32, u32);

fn gate() -> Gate<Rest> {
    Gate::new()
}

#[test]
fn a_window_that_asked_for_no_session_starts_a_dictation_at_once() {
    // The tests and the runs that make no session: the engine's own call listens.
    let mut g = gate();
    assert_eq!(g.state(), Session::Unasked);
    assert_eq!(g.ask((10, 20), 0), Asked::Start { what: (10, 20), report: None });
    assert!(!g.is_waiting());
}

#[test]
fn a_dictation_asked_while_the_session_is_made_waits_and_starts_when_it_is_ready() {
    let mut g = gate();
    g.begin(100);
    assert_eq!(g.state(), Session::Making);
    assert_eq!(g.ask((10, 20), 130), Asked::Wait);
    assert!(g.is_waiting(), "the rest is kept, not dropped and not started");
    let made = g.made(Ok(()));
    assert_eq!(made, Made { start: Some((10, 20)), report: None });
    assert_eq!(g.state(), Session::Ready);
    assert!(!g.is_waiting(), "it starts once");
    // and nothing is left to start when a second message comes
    assert_eq!(g.made(Ok(())), Made { start: None, report: None });
}

#[test]
fn only_the_latest_rest_starts_when_several_wait() {
    let mut g = gate();
    g.begin(0);
    assert_eq!(g.ask((1, 1), 10), Asked::Wait);
    assert_eq!(g.ask((2, 2), 20), Asked::Wait);
    assert_eq!(g.made(Ok(())).start, Some((2, 2)));
}

#[test]
fn a_dictation_asked_after_the_session_is_ready_starts_at_once() {
    let mut g = gate();
    g.begin(0);
    assert_eq!(g.made(Ok(())), Made { start: None, report: None });
    assert_eq!(g.ask((3, 4), 50), Asked::Start { what: (3, 4), report: None });
}

#[test]
fn a_session_that_could_not_be_made_is_told_once_and_the_waiting_dictation_listens_through_the_engines_own_call() {
    let mut g = gate();
    g.begin(0);
    assert_eq!(g.ask((5, 6), 10), Asked::Wait);
    let made = g.made(Err("the speech service refused to start".to_string()));
    assert_eq!(made.start, Some((5, 6)), "the dictation is not lost with the session: it listens without one");
    assert_eq!(made.report.as_deref(), Some("the speech service refused to start"));
    assert_eq!(g.state(), Session::Failed);
    assert_eq!(g.failure(), Some("the speech service refused to start"));
    // told once: a second failure, and the dictations after it, say nothing more
    assert_eq!(g.made(Err("again".to_string())), Made { start: None, report: None });
    assert_eq!(g.ask((7, 8), 900), Asked::Start { what: (7, 8), report: None });
    assert_eq!(g.failure(), Some("the speech service refused to start"), "the first reason stays");
}

#[test]
fn a_thread_that_never_answers_is_given_up_on_at_the_limit_and_told_once() {
    let mut g = gate();
    g.begin(1_000);
    assert_eq!(g.ask((1, 1), 1_000 + MAKE_LIMIT_MS - 1), Asked::Wait, "just before the limit the dictation still waits");
    let asked = g.ask((2, 2), 1_000 + MAKE_LIMIT_MS);
    let Asked::Start { what, report } = asked else {
        panic!("at the limit the dictation starts on the engine's own call, got {asked:?}");
    };
    assert_eq!(what, (2, 2));
    assert!(report.is_some_and(|r| r.contains("5 s")), "the report names how long was waited");
    assert_eq!(g.state(), Session::Failed);
    // the rest that waited first is not started twice: it was replaced by the later one
    assert!(!g.is_waiting());
    // the next dictation does not tell it again
    assert_eq!(g.ask((3, 3), 1_000 + MAKE_LIMIT_MS + 10), Asked::Start { what: (3, 3), report: None });
}

#[test]
fn a_waiting_dictation_is_started_by_the_clock_when_the_thread_never_answers() {
    // Nobody asks again after a rest was kept: the window's timer asks the gate whether the limit is over.
    let mut g = gate();
    g.begin(200);
    assert_eq!(g.ask((4, 4), 300), Asked::Wait);
    assert_eq!(g.expire(200 + MAKE_LIMIT_MS - 1), None, "before the limit nothing happens");
    assert!(g.is_waiting());
    let made = g.expire(200 + MAKE_LIMIT_MS).expect("at the limit the wait ends");
    assert_eq!(made.start, Some((4, 4)));
    assert!(made.report.is_some_and(|r| r.contains("5 s")));
    assert_eq!(g.state(), Session::Failed);
    assert_eq!(g.expire(200 + MAKE_LIMIT_MS + 1), None, "told once");
    // a gate that is not making has nothing to give up on
    let mut ready = gate();
    ready.begin(0);
    let _ = ready.made(Ok(()));
    assert_eq!(ready.expire(MAKE_LIMIT_MS * 10), None);
}

#[test]
fn a_session_that_arrives_after_it_was_given_up_on_is_used_from_then_on_without_a_second_word() {
    let mut g = gate();
    g.begin(0);
    let _ = g.ask((1, 1), MAKE_LIMIT_MS);
    assert_eq!(g.state(), Session::Failed);
    let made = g.made(Ok(()));
    assert_eq!(made, Made { start: None, report: None });
    assert_eq!(g.state(), Session::Ready);
    assert_eq!(g.failure(), None, "it works after all");
}

#[test]
fn a_rest_nobody_wants_any_more_starts_nothing() {
    let mut g = gate();
    g.begin(0);
    assert_eq!(g.ask((9, 9), 5), Asked::Wait);
    assert_eq!(g.forget(), Some((9, 9)));
    assert!(!g.is_waiting());
    assert_eq!(g.made(Ok(())), Made { start: None, report: None });
    assert_eq!(g.forget(), None);
}

#[test]
fn the_states_have_names_for_the_scripts() {
    let names: Vec<&str> = [Session::Unasked, Session::Making, Session::Ready, Session::Failed].iter().map(|s| s.name()).collect();
    assert_eq!(names, vec!["unasked", "making", "ready", "failed"]);
}

#[test]
fn a_session_begun_again_waits_again_and_forgets_the_old_failure() {
    // A window that makes its session a second time (the engine library reloaded, a retry): the state is the new one's.
    let mut g = gate();
    g.begin(0);
    let _ = g.made(Err("no".to_string()));
    assert_eq!(g.state(), Session::Failed);
    g.begin(10_000);
    assert_eq!(g.state(), Session::Making);
    assert_eq!(g.failure(), None);
    assert_eq!(g.ask((1, 2), 10_001), Asked::Wait);
    assert_eq!(g.made(Err("no again".to_string())).report.as_deref(), Some("no again"), "a new failure is a new word");
}
