//! The scripted backend: delays on a clock the test moves, scripted replies and failures, a
//! cancel in the middle of a delay, a request it checks, and what the auto mode answers - all of it
//! through `run_task`, so the same reader reads the mock's text as any backend's.

use std::{cell::Cell, rc::Rc};

use super::*;
use crate::{
    agent_backend::{never_cancelled, run_task, Clock, ManualClock, Response},
    agent_limits::Limits,
    agent_review::{Context, PageGrid, ReviewRequest, Speaker, Turn, Utterance, WordRef},
    agent_stroke::{Sample, StrokeInput},
};

fn stroke(id: u64, pinned: bool) -> StrokeInput {
    StrokeInput {
        id,
        page: 0,
        meaning: Some(Meaning::Issue),
        pinned,
        nib: "pen".to_string(),
        nib_width: 2.0,
        t0_ms: 0,
        samples: vec![
            Sample {
                x: 10.0,
                y: 10.0,
                pressure: 0.5,
                dt_ms: 0,
                tilt: None,
            },
            Sample {
                x: 60.0,
                y: 10.0,
                pressure: 0.5,
                dt_ms: 50,
                tilt: None,
            },
        ],
    }
}

fn request(task: Task, said: &str, pinned: bool) -> Request {
    Request::Review(ReviewRequest {
        task,
        conversation: 3,
        language: "en".to_string(),
        utterances: if said.is_empty() {
            Vec::new()
        } else {
            vec![Utterance {
                text: said.to_string(),
                start_ms: 0,
                end_ms: 40,
            }]
        },
        strokes: vec![stroke(1, pinned)],
        context: Context {
            file: "src/a.rs".to_string(),
            page: 0,
            first_line: 10,
            lines: vec![
                "let value = foo(1);".to_string(),
                "let other = 2;".to_string(),
                "// end".to_string(),
            ],
            total_lines: 40,
            focus: Some((10, 11)),
            words_under: vec![WordRef {
                stroke: 1,
                line: 10,
                col: 12,
                text: "foo".to_string(),
            }],
            grid: None,
        },
        turns: vec![Turn {
            from: Speaker::Reviewer,
            text: "hello".to_string(),
        }],
        workspace: None,
    })
}

fn run(
    backend: &mut MockBackend,
    request: &Request,
    clock: &mut ManualClock,
) -> Result<Response, AgentError> {
    let mut ctl = Control::new(clock, &never_cancelled);
    run_task(backend, request, &Limits::default(), &mut ctl)
}

fn reply_of(response: &Response) -> &Reply {
    response.reply().expect("a review answer")
}

#[test]
fn a_scripted_reply_arrives_after_its_delay_on_the_clock_the_test_moves() {
    let scripted = Reply {
        answer: "done".to_string(),
        suggestions: vec![Suggestion::InsertLines {
            after_line: 10,
            lines: vec!["// x".to_string()],
        }],
        intent: None,
    };
    let mut mock = MockBackend::scripted(vec![Step::reply(1_200, &scripted)]);
    let mut clock = ManualClock::new();
    let response = run(
        &mut mock,
        &request(Task::Editing, "add a comment", true),
        &mut clock,
    )
    .expect("answers");
    assert_eq!(reply_of(&response), &scripted);
    assert_eq!(response.usage.elapsed_ms, 1_200);
    assert_eq!(clock.now_ms(), 1_200);
    assert_eq!(response.identity.kind, BackendKind::Mock);
}

#[test]
fn the_steps_play_in_order_and_a_used_up_script_is_a_failure() {
    let one = Reply {
        answer: "one".to_string(),
        ..Reply::default()
    };
    let two = Reply {
        answer: "two".to_string(),
        ..Reply::default()
    };
    let mut mock = MockBackend::scripted(vec![Step::reply(10, &one), Step::reply(10, &two)]);
    let mut clock = ManualClock::new();
    let req = request(Task::Editing, "x", true);
    assert_eq!(
        reply_of(&run(&mut mock, &req, &mut clock).expect("one")).answer,
        "one"
    );
    assert_eq!(
        reply_of(&run(&mut mock, &req, &mut clock).expect("two")).answer,
        "two"
    );
    match run(&mut mock, &req, &mut clock) {
        Err(AgentError::Settings { why }) => assert!(why.contains("used up"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_scripted_failure_arrives_after_its_delay() {
    let mut mock =
        MockBackend::scripted(vec![Step::fail(90_000, AgentError::Timeout { secs: 90 })]);
    let mut clock = ManualClock::new();
    let err = run(&mut mock, &request(Task::Editing, "x", true), &mut clock).expect_err("fails");
    assert_eq!(err, AgentError::Timeout { secs: 90 });
    assert_eq!(clock.now_ms(), 90_000);
}

#[test]
fn scripted_garbage_goes_through_the_reader_and_fails_there() {
    let mut mock = MockBackend::scripted(vec![Step::raw(5, "{\"answer\": \"cut off")]);
    let mut clock = ManualClock::new();
    let err = run(&mut mock, &request(Task::Editing, "x", true), &mut clock).expect_err("garbage");
    assert!(matches!(err, AgentError::BadOutput { .. }), "{err:?}");
}

/// A clock whose time the cancel check can read too.
struct SharedClock(Rc<Cell<u64>>);

impl Clock for SharedClock {
    fn now_ms(&mut self) -> u64 {
        self.0.get()
    }

    fn sleep_ms(&mut self, ms: u64) {
        self.0.set(self.0.get() + ms);
    }
}

#[test]
fn a_cancel_in_the_middle_of_a_delay_stops_the_wait_at_once() {
    let time = Rc::new(Cell::new(0));
    let mut clock = SharedClock(time.clone());
    let cancel_time = time.clone();
    let cancel = move || cancel_time.get() >= 300;
    let mut ctl = Control::new(&mut clock, &cancel);
    let mut mock = MockBackend::scripted(vec![Step::raw(60_000, "{\"answer\": \"late\"}")]);
    let req = request(Task::Editing, "x", true);
    let result = run_task(&mut mock, &req, &Limits::default(), &mut ctl);
    assert_eq!(result.expect_err("cancelled"), AgentError::Cancelled);
    assert_eq!(
        time.get(),
        300,
        "the wait stopped at the slice that saw the cancel, not at 60 s"
    );
}

#[test]
fn the_mock_checks_the_request_it_gets_and_names_what_is_missing() {
    let mut clock = ManualClock::new();
    let req = request(Task::Editing, "rename it", true);
    let mut ok = MockBackend::auto(0).expecting(&["src/a.rs", "<svg", "rename it", "insert_lines"]);
    assert!(run(&mut ok, &req, &mut clock).is_ok());
    let mut missing = MockBackend::auto(0).expecting(&["src/a.rs", "no such words"]);
    match run(&mut missing, &req, &mut clock) {
        Err(AgentError::Settings { why }) => assert!(why.contains("no such words"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_log_outlives_the_move_of_the_backend_to_another_thread() {
    let mock = MockBackend::auto(0);
    let log = mock.log();
    let req = request(Task::Exploration, "what is foo?", true);
    let worker = std::thread::spawn(move || {
        let mut mock = mock;
        let mut clock = ManualClock::new();
        run(&mut mock, &req, &mut clock).map(|r| r.first_words(200))
    });
    let said = worker.join().expect("no panic").expect("answers");
    assert!(
        said.starts_with("Mock answer about src/a.rs: what is foo?"),
        "{said}"
    );
    assert_eq!(log.count(), 1);
    let seen = &log.calls()[0];
    assert_eq!(seen.task, Task::Exploration);
    assert_eq!(seen.conversation, 3);
    assert!(seen.prompt.contains("what is foo?"));
}

#[test]
fn auto_removes_the_word_under_the_stroke_when_asked_to() {
    let mut mock = MockBackend::auto(800);
    let mut clock = ManualClock::new();
    let response = run(
        &mut mock,
        &request(Task::Editing, "please remove this", true),
        &mut clock,
    )
    .expect("answers");
    assert_eq!(
        reply_of(&response).suggestions,
        vec![Suggestion::RemoveRange {
            start: Pos { line: 10, col: 12 },
            end: Pos { line: 10, col: 15 },
            old: "foo".to_string()
        }]
    );
    assert_eq!(response.usage.elapsed_ms, 800);
    assert!(
        response.diagnostics.is_empty(),
        "{:?}",
        response.diagnostics
    );
}

#[test]
fn auto_renames_the_word_under_the_stroke_when_asked_to() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let response = run(
        &mut mock,
        &request(Task::Editing, "rename this to count", true),
        &mut clock,
    )
    .expect("answers");
    assert_eq!(
        reply_of(&response).suggestions,
        vec![Suggestion::ReplaceRange {
            start: Pos { line: 10, col: 12 },
            end: Pos { line: 10, col: 15 },
            old: "foo".to_string(),
            text: "renamed".to_string()
        }]
    );
}

#[test]
fn auto_otherwise_inserts_a_line_after_the_lines_the_strokes_cover() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let response = run(
        &mut mock,
        &request(Task::Editing, "this looks odd", true),
        &mut clock,
    )
    .expect("answers");
    assert_eq!(
        reply_of(&response).suggestions,
        vec![Suggestion::InsertLines {
            after_line: 11,
            lines: vec!["// mock: this looks odd".to_string()]
        }]
    );
}

#[test]
fn auto_answers_an_exploration_with_text_and_no_suggestions() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let response = run(
        &mut mock,
        &request(Task::Exploration, "what does foo do", true),
        &mut clock,
    )
    .expect("answers");
    let reply = reply_of(&response);
    assert!(
        reply.answer.contains("src/a.rs") && reply.answer.contains("what does foo do"),
        "{}",
        reply.answer
    );
    assert!(reply.suggestions.is_empty());
}

#[test]
fn auto_guesses_an_intent_only_for_strokes_the_reviewer_did_not_pin() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let unpinned = run(
        &mut mock,
        &request(Task::Editing, "why is this here?", false),
        &mut clock,
    )
    .expect("answers");
    let intent = reply_of(&unpinned).intent.as_ref().expect("an intent");
    assert_eq!(intent.meaning, Some(Meaning::Question));
    let pinned = run(
        &mut mock,
        &request(Task::Editing, "why is this here?", true),
        &mut clock,
    )
    .expect("answers");
    assert!(reply_of(&pinned).intent.is_none());
}

/// A stroke along the points, unpinned, whatever its pen was.
fn drawn(id: u64, pinned: bool, from: (f32, f32), to: (f32, f32)) -> StrokeInput {
    let mut s = stroke(id, pinned);
    s.samples = (0..12)
        .map(|i| {
            let t = i as f32 / 11.0;
            Sample {
                x: from.0 + (to.0 - from.0) * t,
                y: from.1 + (to.1 - from.1) * t,
                pressure: 0.5,
                dt_ms: i * 30,
                tilt: None,
            }
        })
        .collect();
    s
}

/// The request of `request()` with these strokes on a page whose lines are 16 px high and 8 px wide
/// (line 10, `let value = foo(1);`, is y 100 to 116 and `foo` is x 96 to 120).
fn on_the_grid(said: &str, strokes: Vec<StrokeInput>) -> Request {
    let mut req = request(Task::Editing, said, false);
    let Request::Review(r) = &mut req else {
        panic!("a review request")
    };
    r.strokes = strokes;
    r.context.grid = Some(PageGrid {
        first_line: 10,
        origin_x: 0.0,
        origin_y: 100.0,
        line_height: 16.0,
        char_width: 8.0,
    });
    req
}

fn intent_of(response: &Response) -> Intent {
    reply_of(response).intent.clone().expect("an intent")
}

#[test]
fn auto_reads_the_shape_when_the_words_say_nothing_plain() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    // a line through the middle of `foo`: a strike-through, which means an issue
    let strike = drawn(1, false, (92.0, 108.0), (124.0, 108.0));
    let response = run(
        &mut mock,
        &on_the_grid("this looks odd", vec![strike]),
        &mut clock,
    )
    .expect("answers");
    let intent = intent_of(&response);
    assert_eq!(intent.meaning, Some(Meaning::Issue));
    assert!(intent.rationale.contains("rule of shape") && intent.rationale.contains("strike-through"), "{}", intent.rationale);
    assert!(intent.confidence > 0.0 && intent.confidence <= 0.5);
    // and under it: an underline, which means scope
    let under = drawn(1, false, (92.0, 114.4), (124.0, 114.4));
    let response = run(&mut mock, &on_the_grid("this looks odd", vec![under]), &mut clock).expect("answers");
    assert_eq!(intent_of(&response).meaning, Some(Meaning::Scope));
}

#[test]
fn auto_prefers_what_the_reviewer_said_to_what_a_shape_may_mean() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let strike = drawn(1, false, (92.0, 108.0), (124.0, 108.0));
    let response = run(
        &mut mock,
        &on_the_grid("why is this here?", vec![strike]),
        &mut clock,
    )
    .expect("answers");
    let intent = intent_of(&response);
    assert_eq!(intent.meaning, Some(Meaning::Question));
    assert_eq!(intent.rationale, "the mock's rule of thumb");
}

#[test]
fn auto_never_reads_the_shape_of_a_stroke_the_reviewer_chose_a_meaning_for() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let pinned = drawn(1, true, (92.0, 108.0), (124.0, 108.0));
    let response = run(&mut mock, &on_the_grid("this looks odd", vec![pinned]), &mut clock).expect("answers");
    assert!(reply_of(&response).intent.is_none());
    // two strokes: the one that is pinned is left out of the guess, the other one is guessed at
    let pinned_underline = drawn(1, true, (92.0, 114.4), (124.0, 114.4));
    let strike = drawn(2, false, (92.0, 108.0), (124.0, 108.0));
    let response = run(
        &mut mock,
        &on_the_grid("this looks odd", vec![pinned_underline, strike]),
        &mut clock,
    )
    .expect("answers");
    assert_eq!(intent_of(&response).meaning, Some(Meaning::Issue));
}

#[test]
fn auto_falls_back_to_scope_when_neither_the_words_nor_the_shape_say_anything() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    // a short slanted line: no rule fits
    let slanted = drawn(1, false, (92.0, 100.0), (124.0, 140.0));
    let response = run(&mut mock, &on_the_grid("this looks odd", vec![slanted]), &mut clock).expect("answers");
    let intent = intent_of(&response);
    assert_eq!(intent.meaning, Some(Meaning::Scope));
    assert_eq!(intent.rationale, "the mock's rule of thumb");
}

#[test]
fn a_mock_that_says_nothing_still_answers_something_readable() {
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let mut req = request(Task::Editing, "", true);
    let Request::Review(r) = &mut req else {
        panic!("a review request")
    };
    r.turns.clear();
    let response = run(&mut mock, &req, &mut clock).expect("answers");
    assert_eq!(
        reply_of(&response).suggestions,
        vec![Suggestion::InsertLines {
            after_line: 11,
            lines: vec!["// mock: reviewed".to_string()]
        }]
    );
}
