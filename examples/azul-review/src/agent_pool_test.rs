//! The pool: the cap, the cancel flags, the replaced job, the reaper; and the worker's body with
//! the mock, a failing factory, a cancel, and a backend that panics. No thread of its own.

use super::*;
use crate::{
    agent_backend::{BackendKind, Body, Call, Completion, Identity, ManualClock},
    agent_mock::{MockBackend, Step},
    agent_review::{Context, Reply, ReviewRequest},
};

fn started(pool: &mut Pool, conversation: u64, now: u64) -> Started {
    pool.start(conversation, Task::Editing, now, 90)
        .expect("a slot")
}

#[test]
fn the_cap_is_two_by_default_and_a_third_job_is_busy() {
    let mut pool = Pool::default();
    assert_eq!(pool.cap(), 2);
    started(&mut pool, 1, 0);
    started(&mut pool, 2, 0);
    assert_eq!(pool.running(), 2);
    let err = pool.start(3, Task::Editing, 0, 90).expect_err("busy");
    assert_eq!(err, AgentError::Busy { running: 2, cap: 2 });
}

#[test]
fn a_finished_job_frees_its_slot_and_reports_how_long_it_ran() {
    let mut pool = Pool::new(1);
    let a = started(&mut pool, 7, 1_000);
    assert!(pool.start(8, Task::Editing, 1_000, 90).is_err());
    let ended = pool.finish(a.ticket.id, 4_500).expect("it was running");
    assert_eq!(
        ended,
        Ended {
            id: a.ticket.id,
            conversation: 7,
            task: Task::Editing,
            elapsed_ms: 3_500
        }
    );
    assert_eq!(pool.running(), 0);
    assert!(pool.start(8, Task::Editing, 5_000, 90).is_ok());
}

#[test]
fn a_job_that_is_not_there_finishes_as_nothing() {
    let mut pool = Pool::default();
    assert_eq!(pool.finish(99, 0), None);
}

#[test]
fn the_cap_is_held_to_one_to_eight_and_the_word_says_so() {
    assert_eq!(Pool::new(0).cap(), 1);
    assert_eq!(Pool::new(100).cap(), 8);
    let mut pool = Pool::default();
    assert_eq!(pool.set_cap(4), None);
    assert_eq!(pool.cap(), 4);
    let note = pool.set_cap(50).expect("held");
    assert!(note.contains("50") && note.contains("8"), "{note}");
    assert_eq!(pool.cap(), 8);
}

#[test]
fn asking_again_in_a_conversation_cancels_its_running_job_and_takes_its_slot() {
    let mut pool = Pool::new(1);
    let first = started(&mut pool, 5, 0);
    assert!(!first.ticket.cancelled());
    let second = started(&mut pool, 5, 100);
    assert_eq!(second.replaced, Some(first.ticket.id));
    assert!(first.ticket.cancelled(), "the first worker is told");
    assert!(!second.ticket.cancelled());
    assert_eq!(
        pool.running(),
        1,
        "a cancelled job does not count against the cap"
    );
    assert_eq!(pool.in_flight(), 2, "but its worker has not reported yet");
    assert!(pool.finish(first.ticket.id, 120).is_some());
    assert_eq!(pool.in_flight(), 1);
}

#[test]
fn a_conversation_that_asks_again_takes_the_slot_of_its_old_job_even_when_the_pool_is_full() {
    let mut pool = Pool::new(2);
    let a = started(&mut pool, 1, 0);
    let _b = started(&mut pool, 2, 0);
    // conversation 1 asks again: its old job is cancelled, which frees a slot, so this one starts
    let again = started(&mut pool, 1, 10);
    assert_eq!(again.replaced, Some(a.ticket.id));
    // a third conversation now finds the cap reached by 2 and 1's new job
    assert!(pool.start(3, Task::Editing, 10, 90).is_err());
}

#[test]
fn cancel_sets_the_flag_once_and_the_job_stays_until_it_reports() {
    let mut pool = Pool::default();
    let a = started(&mut pool, 1, 0);
    assert!(pool.cancel(a.ticket.id));
    assert!(!pool.cancel(a.ticket.id), "already cancelled");
    assert!(!pool.cancel(999));
    assert!(a.ticket.cancelled());
    assert!(!pool.is_running(1));
    assert_eq!(pool.in_flight(), 1);
}

#[test]
fn cancelling_a_conversation_or_everything_counts_what_it_cancelled() {
    let mut pool = Pool::new(4);
    let a = started(&mut pool, 1, 0);
    let b = started(&mut pool, 2, 0);
    let c = started(&mut pool, 3, 0);
    assert_eq!(pool.cancel_conversation(2), 1);
    assert!(b.ticket.cancelled() && !a.ticket.cancelled());
    assert_eq!(pool.cancel_all(), 2);
    assert!(a.ticket.cancelled() && c.ticket.cancelled());
    assert_eq!(pool.cancel_all(), 0);
}

#[test]
fn a_job_past_its_deadline_and_the_grace_time_is_given_up_on_and_its_late_message_is_ignored() {
    let mut pool = Pool::default();
    let slow = pool.start(1, Task::Editing, 0, 90).expect("a slot");
    let fresh = pool.start(2, Task::Editing, 0, 600).expect("a slot");
    assert!(
        pool.reap(90_000 + GRACE_MS).is_empty(),
        "exactly at the deadline it is still waited for"
    );
    let gone = pool.reap(90_000 + GRACE_MS + 1);
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0].id, slow.ticket.id);
    assert_eq!(gone[0].conversation, 1);
    assert!(slow.ticket.cancelled(), "the lost worker is told as well");
    assert!(!fresh.ticket.cancelled());
    assert_eq!(
        pool.finish(slow.ticket.id, 200_000),
        None,
        "its late message finds nothing"
    );
    assert_eq!(pool.running(), 1);
}

// ==== the worker ====

fn review_request() -> Request {
    Request::Review(ReviewRequest {
        task: Task::Editing,
        conversation: 4,
        language: "en".to_string(),
        utterances: Vec::new(),
        strokes: Vec::new(),
        context: Context {
            file: "a.rs".to_string(),
            page: 0,
            first_line: 1,
            lines: vec!["fn a() {}".to_string()],
            total_lines: 1,
            focus: None,
            words_under: Vec::new(),
            grid: None,
        },
        turns: Vec::new(),
        workspace: None,
    })
}

fn spec(make: Factory) -> JobSpec {
    JobSpec {
        conversation: 4,
        request: review_request(),
        limits: Limits::default(),
        make,
    }
}

fn run(spec: JobSpec, ticket: &Ticket) -> Vec<JobMessage> {
    let mut clock = ManualClock::new();
    let mut messages = Vec::new();
    run_job(spec, ticket, &mut clock, &mut |m| messages.push(m));
    messages
}

fn ticket(pool: &mut Pool) -> Ticket {
    pool.start(4, Task::Editing, 0, 90).expect("a slot").ticket
}

#[test]
fn a_job_reports_that_it_began_and_then_its_answer() {
    let mut pool = Pool::default();
    let t = ticket(&mut pool);
    let reply = Reply {
        answer: "fine".to_string(),
        ..Reply::default()
    };
    let make: Factory = Box::new(move || {
        Ok(Box::new(MockBackend::scripted(vec![Step::reply(
            100, &reply,
        )])))
    });
    let messages = run(spec(make), &t);
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0].event, JobEvent::Started));
    assert_eq!((messages[0].job, messages[0].conversation), (t.id, 4));
    match &messages[1].event {
        JobEvent::Done(result) => match result.as_ref() {
            Ok(response) => {
                let Body::Review(reply) = &response.body;
                assert_eq!(reply.answer, "fine");
                assert_eq!(response.usage.elapsed_ms, 100);
            }
            Err(e) => panic!("{e:?}"),
        },
        JobEvent::Started => panic!("two starts"),
    }
}

#[test]
fn a_backend_that_cannot_be_made_is_the_jobs_failure() {
    let mut pool = Pool::default();
    let t = ticket(&mut pool);
    let make: Factory = Box::new(|| {
        Err(AgentError::NotInstalled {
            program: "claude".to_string(),
            searched: Vec::new(),
        })
    });
    let messages = run(spec(make), &t);
    match &messages[1].event {
        JobEvent::Done(result) => {
            assert!(
                matches!(result.as_ref(), Err(AgentError::NotInstalled { .. })),
                "{result:?}"
            );
        }
        JobEvent::Started => panic!("two starts"),
    }
}

#[test]
fn a_job_cancelled_before_it_runs_ends_cancelled_and_the_backend_is_never_asked() {
    let mut pool = Pool::default();
    let t = ticket(&mut pool);
    pool.cancel(t.id);
    let make: Factory = Box::new(|| Ok(Box::new(MockBackend::scripted(Vec::new()))));
    let messages = run(spec(make), &t);
    match &messages[1].event {
        JobEvent::Done(result) => assert!(
            matches!(result.as_ref(), Err(AgentError::Cancelled)),
            "{result:?}"
        ),
        JobEvent::Started => panic!("two starts"),
    }
}

/// A backend that panics.
struct Panics;

impl Backend for Panics {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::Mock,
            model: String::new(),
        }
    }

    fn complete(
        &mut self,
        _call: &Call<'_>,
        _ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        panic!("a bug in a backend");
    }
}

#[test]
fn a_backend_that_panics_ends_the_job_with_a_failure_instead_of_silence() {
    let mut pool = Pool::default();
    let t = ticket(&mut pool);
    let make: Factory = Box::new(|| Ok(Box::new(Panics)));
    let messages = run(spec(make), &t);
    assert_eq!(messages.len(), 2, "it still reports its end");
    match &messages[1].event {
        JobEvent::Done(result) => assert!(
            matches!(result.as_ref(), Err(AgentError::Unsupported { .. })),
            "{result:?}"
        ),
        JobEvent::Started => panic!("two starts"),
    }
}

#[test]
fn the_worker_runs_on_a_thread_of_its_own_and_the_ticket_cancels_it_from_here() {
    let mut pool = Pool::default();
    let t = ticket(&mut pool);
    let worker_ticket = t.clone();
    let make: Factory = Box::new(|| {
        Ok(Box::new(MockBackend::scripted(vec![Step::raw(
            10_000_000,
            "{\"answer\": \"never\"}",
        )])))
    });
    let job = spec(make);
    let handle = std::thread::spawn(move || {
        // a clock that really lets time pass, so the worker is still in its wait when the flag is set
        let mut clock = ManualClock::with_real_pause(1);
        let mut messages = Vec::new();
        run_job(job, &worker_ticket, &mut clock, &mut |m| messages.push(m));
        messages
    });
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert!(pool.cancel(t.id));
    let messages = handle.join().expect("the worker ended");
    match &messages[1].event {
        JobEvent::Done(result) => assert!(
            matches!(result.as_ref(), Err(AgentError::Cancelled)),
            "{result:?}"
        ),
        JobEvent::Started => panic!("two starts"),
    }
}
