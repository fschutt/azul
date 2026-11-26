//! The dictation's listening loop: a scripted listener and a clock the test moves, no microphone,
//! no thread, no azul.

use std::{cell::Cell, collections::VecDeque, rc::Rc};

use super::*;

type Time = Rc<Cell<u64>>;

/// A clock that only moves when the listener takes time or the loop sleeps.
struct FakeClock {
    now: Time,
    slept: Vec<u64>,
}

impl Clock for FakeClock {
    fn now_ms(&mut self) -> u64 {
        self.now.get()
    }

    fn sleep_ms(&mut self, ms: u64) {
        self.slept.push(ms);
        self.now.set(self.now.get() + ms);
    }
}

fn heard(status: Status, text: &str, message: &str) -> Heard {
    Heard::new(status, text, message)
}

fn said(text: &str) -> Heard {
    heard(Status::Heard, text, "")
}

/// A listener that answers from a script: each answer with the time the call took; a call past the
/// script hears nothing for the whole of its timeout.
struct Script {
    now: Time,
    answers: VecDeque<(u64, Heard)>,
    calls: Rc<Cell<usize>>,
    /// Run in each call, before it answers (a test's "the pen moved meanwhile").
    meanwhile: Option<Box<dyn FnMut(usize)>>,
}

impl Listener for Script {
    fn listen(&mut self, _language: &str, timeout_ms: u32) -> Heard {
        self.calls.set(self.calls.get() + 1);
        if let Some(f) = self.meanwhile.as_mut() {
            f(self.calls.get());
        }
        match self.answers.pop_front() {
            Some((took, h)) => {
                self.now.set(self.now.get() + took);
                h
            }
            None => {
                self.now.set(self.now.get() + u64::from(timeout_ms));
                heard(Status::Nothing, "", "nothing was said")
            }
        }
    }
}

fn plan() -> Plan {
    Plan {
        language: "en-US".to_string(),
        chunk_ms: 4_000,
        silence_ms: 8_000,
        min_gap_ms: 400,
    }
}

fn script(answers: Vec<(u64, Heard)>) -> (Script, FakeClock) {
    let now: Time = Rc::new(Cell::new(0));
    (
        Script {
            now: now.clone(),
            answers: answers.into(),
            calls: Rc::new(Cell::new(0)),
            meanwhile: None,
        },
        FakeClock { now, slept: Vec::new() },
    )
}

/// Runs the loop until it ends; `stop_after_calls`: the stop is asked for once that many calls were made.
fn run_with(
    plan: &Plan,
    listener: &mut Script,
    clock: &mut FakeClock,
    stop_after_calls: Option<usize>,
) -> Vec<Out> {
    let mut outs = Vec::new();
    let calls = listener.calls.clone();
    run(
        plan,
        listener,
        clock,
        &mut || stop_after_calls.is_some_and(|n| calls.get() >= n),
        &mut |out| outs.push(out),
    );
    outs
}

#[test]
fn what_is_heard_is_handed_over_as_it_comes_and_silence_ends_the_session() {
    // 2 s of speech, then nothing: the last text was at 2000, so 8000 ms of quiet end it at 10000
    // (two chunks of 4000 after the speech: 6000, 10000).
    let (mut listener, mut clock) = script(vec![(2_000, said("this loop")), (1_500, said("is off by one"))]);
    let outs = run_with(&plan(), &mut listener, &mut clock, None);
    assert_eq!(
        outs,
        vec![
            Out::Text("this loop".to_string()),
            Out::Text("is off by one".to_string()),
            Out::Ended(End::Silence),
        ]
    );
    assert_eq!(clock.now.get(), 3_500 + 8_000, "8 s of quiet after the last words");
}

#[test]
fn heard_text_restarts_the_silence_clock() {
    // quiet 4 s, words (at 4000+1000=5000), quiet 4 s, quiet 4 s: ended at 5000 + 8000.
    let (mut listener, mut clock) = script(vec![
        (4_000, heard(Status::Nothing, "", "")),
        (1_000, said("late words")),
    ]);
    let outs = run_with(&plan(), &mut listener, &mut clock, None);
    assert_eq!(outs, vec![Out::Text("late words".to_string()), Out::Ended(End::Silence)]);
    assert_eq!(clock.now.get(), 5_000 + 8_000);
}

#[test]
fn a_stop_ends_the_session_between_two_calls() {
    let (mut listener, mut clock) = script(vec![(1_500, said("one")), (1_500, said("two")), (1_500, said("three"))]);
    // A stop asked for from the second call on: the second call's words still count.
    let stop = Rc::new(Cell::new(false));
    let flag = stop.clone();
    listener.meanwhile = Some(Box::new(move |call| {
        if call == 2 {
            flag.set(true);
        }
    }));
    let mut outs = Vec::new();
    run(&plan(), &mut listener, &mut clock, &mut || stop.get(), &mut |o| outs.push(o));
    assert_eq!(
        outs,
        vec![Out::Text("one".to_string()), Out::Text("two".to_string()), Out::Ended(End::Stopped)],
        "what was said while the stop was asked for is kept, nothing more is listened for"
    );
    assert_eq!(listener.calls.get(), 2);
}

#[test]
fn a_stop_before_the_first_call_listens_for_nothing() {
    let (mut listener, mut clock) = script(vec![(1_000, said("never"))]);
    let outs = run_with(&plan(), &mut listener, &mut clock, Some(0));
    assert_eq!(outs, vec![Out::Ended(End::Stopped)]);
    assert_eq!(listener.calls.get(), 0);
}

#[test]
fn a_refusal_is_told_with_its_words_and_ends_the_session() {
    for status in [Status::Denied, Status::Unavailable, Status::Failed] {
        let (mut listener, mut clock) = script(vec![(10, heard(status, "", "the microphone is off"))]);
        let outs = run_with(&plan(), &mut listener, &mut clock, None);
        assert_eq!(
            outs,
            vec![
                Out::Failed {
                    status,
                    message: "the microphone is off".to_string()
                },
                Out::Ended(End::Failed)
            ],
            "{status:?}"
        );
        assert_eq!(listener.calls.get(), 1, "{status:?}: no second try");
    }
}

#[test]
fn a_listener_that_answers_at_once_is_not_asked_in_a_tight_loop() {
    // A stand-in that hears the same words every time and takes no time: at least `min_gap_ms`
    // between two calls, or the notes would fill with copies faster than a person can read.
    let answers = (0..5).map(|_| (0, said("same words"))).collect();
    let (mut listener, mut clock) = script(answers);
    let outs = run_with(&plan(), &mut listener, &mut clock, Some(3));
    assert_eq!(listener.calls.get(), 3);
    assert_eq!(clock.slept, vec![400, 400], "a pause between two calls, none after the last");
    assert_eq!(clock.now.get(), 800);
    assert_eq!(outs.last(), Some(&Out::Ended(End::Stopped)), "{outs:?}");
}

#[test]
fn a_call_that_took_its_time_is_not_slept_after() {
    let (mut listener, mut clock) = script(vec![(2_000, said("slow"))]);
    let outs = run_with(&plan(), &mut listener, &mut clock, Some(1));
    assert!(clock.slept.is_empty(), "{:?}", clock.slept);
    assert_eq!(outs, vec![Out::Text("slow".to_string()), Out::Ended(End::Stopped)]);
}

#[test]
fn blank_text_is_nothing_heard() {
    let (mut listener, mut clock) = script(vec![(1_000, said("   ")), (1_000, said("real"))]);
    let outs = run_with(&plan(), &mut listener, &mut clock, None);
    assert_eq!(outs[0], Out::Text("real".to_string()), "no empty text is handed over: {outs:?}");
}

#[test]
fn words_join_into_a_note_with_single_spaces_and_keep_what_was_typed() {
    assert_eq!(join_text("", "hello"), "hello");
    assert_eq!(join_text("hello", "world"), "hello world");
    assert_eq!(join_text("hello  \n", "  world   again "), "hello world again");
    assert_eq!(join_text("line one\nline two", "three"), "line one\nline two three");
    assert_eq!(join_text("keep", "   "), "keep");
}

#[test]
fn the_causes_have_names_a_script_can_read() {
    for cause in Cause::ALL {
        assert!(!cause.name().is_empty());
        assert!(cause.name().chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{}", cause.name());
    }
    assert_eq!(Cause::from(End::Silence), Cause::Silence);
    assert_eq!(Cause::from(End::Failed), Cause::Failed);
    assert_eq!(Cause::from(End::Stopped), Cause::Stopped);
}

#[test]
fn the_silence_and_chunk_times_are_capped_into_what_a_person_can_mean() {
    assert_eq!(cap_silence_ms(10_000), (10_000, None));
    let (low, why) = cap_silence_ms(100);
    assert_eq!(low, MIN_SILENCE_MS);
    assert!(why.is_some_and(|w| w.contains("silence")));
    let (high, why) = cap_silence_ms(60 * 60 * 1_000);
    assert_eq!(high, MAX_SILENCE_MS);
    assert!(why.is_some());
}
