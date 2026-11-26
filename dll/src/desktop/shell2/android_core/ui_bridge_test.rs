//! The queues between the UI thread and the loop. Real threads, no device.

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use super::*;

fn mirror_with(text: &str) -> ImeMirror {
    ImeMirror {
        active: true,
        text: text.to_string(),
        caret: text.len(),
        ..ImeMirror::default()
    }
}

/// The type is what makes the rule: nothing in it is the window, and it can be shared between the UI
/// thread and the loop thread. A field that is not `Sync` (a `RefCell`, a raw pointer, a window) stops
/// this from compiling.
#[test]
fn the_queues_are_shared_between_threads_by_construction() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<UiQueues>();
    assert_send_sync::<UiEvent>();
    assert_send_sync::<ImeMirror>();
}

/// What the UI thread did, the loop sees in that order: a composition followed by its commit must not
/// arrive commit first.
///
/// Mutation: a stack; a map keyed by kind (a second commit replacing the first).
#[test]
fn events_arrive_in_the_order_they_happened() {
    let q = UiQueues::new();
    q.push(UiEvent::SetComposing { text: "he".into(), cursor: 1 });
    q.push(UiEvent::Commit("hello".into()));
    q.push(UiEvent::Commit(" world".into()));
    q.push(UiEvent::FinishComposing);
    let (events, pushed) = q.drain();
    assert_eq!(pushed, 4);
    assert_eq!(
        events,
        vec![
            UiEvent::SetComposing { text: "he".into(), cursor: 1 },
            UiEvent::Commit("hello".into()),
            UiEvent::Commit(" world".into()),
            UiEvent::FinishComposing,
        ]
    );
    assert!(q.drain().0.is_empty(), "taken once");
    assert!(!q.has_events());
}

/// Many UI-side pushers and one loop: every event arrives exactly once and each pusher's events stay
/// in its order.
///
/// Mutation: a push that is not under the lock.
#[test]
fn concurrent_pushes_are_all_delivered_once_and_in_each_threads_order() {
    let q = Arc::new(UiQueues::new());
    let mut pushers = Vec::new();
    for t in 0..4 {
        let q = Arc::clone(&q);
        pushers.push(thread::spawn(move || {
            for i in 0..500 {
                q.push(UiEvent::SelectionAction(t * 1000 + i));
            }
        }));
    }
    let mut seen = Vec::new();
    let started = Instant::now();
    while seen.len() < 2000 && started.elapsed() < Duration::from_secs(20) {
        seen.extend(q.drain().0);
        thread::yield_now();
    }
    for p in pushers {
        p.join().unwrap();
    }
    seen.extend(q.drain().0);
    assert_eq!(seen.len(), 2000);
    for t in 0..4 {
        let mine: Vec<i32> = seen
            .iter()
            .filter_map(|e| match e {
                UiEvent::SelectionAction(a) if a / 1000 == t => Some(a % 1000),
                _ => None,
            })
            .collect();
        assert_eq!(mine, (0..500).collect::<Vec<_>>(), "thread {t}");
    }
    assert_eq!(q.pushed(), 2000);
}

/// A read made after a push waits for the loop to have seen that push, so an IME that asks for the
/// text right after `commitText` gets the text WITH the commit.
///
/// Mutation: a read that answers the mirror at hand (the text before the commit); a publish that does
/// not wake the reader (it waits out the whole bound).
#[test]
fn a_read_after_a_push_waits_for_a_mirror_that_has_seen_it() {
    let q = Arc::new(UiQueues::new());
    q.publish(0, mirror_with(""));
    let loop_q = Arc::clone(&q);
    let looper = thread::spawn(move || {
        // the loop's tick: a few milliseconds later it drains, applies and publishes
        thread::sleep(Duration::from_millis(8));
        let (events, applied) = loop_q.drain();
        let mut text = String::new();
        for e in events {
            if let UiEvent::Commit(t) = e {
                text.push_str(&t);
            }
        }
        loop_q.publish(applied, mirror_with(&text));
    });
    q.push(UiEvent::Commit("a".into()));
    let started = Instant::now();
    let seen = q.read_mirror_within(Duration::from_secs(5));
    let waited = started.elapsed();
    looper.join().unwrap();
    assert_eq!(seen.text, "a", "the answer has the commit in it");
    assert!(waited < Duration::from_secs(2), "the reader was woken by the publish, not by its timeout ({waited:?})");
}

/// A loop that never answers (it is busy, or the app is in the background) costs the reader its bound
/// and no more, and the answer is the last mirror: late-stale, never a deadlock.
///
/// Mutation: a wait without a bound.
#[test]
fn a_loop_that_never_answers_costs_the_reader_its_bound_and_no_more() {
    let q = UiQueues::new();
    q.publish(0, mirror_with("old"));
    q.push(UiEvent::Commit("x".into()));
    let started = Instant::now();
    let seen = q.read_mirror_within(Duration::from_millis(60));
    let waited = started.elapsed();
    assert_eq!(seen.text, "old");
    assert!(waited >= Duration::from_millis(50), "it did wait ({waited:?})");
    assert!(waited < Duration::from_secs(2), "and gave up ({waited:?})");
}

/// A mirror published for fewer events than were pushed does not satisfy a later push.
#[test]
fn a_mirror_made_before_a_push_does_not_answer_for_it() {
    let q = UiQueues::new();
    q.push(UiEvent::Commit("1".into()));
    let (_, applied) = q.drain();
    q.push(UiEvent::Commit("2".into()));
    q.publish(applied, mirror_with("1"));
    let started = Instant::now();
    let seen = q.read_mirror_within(Duration::from_millis(30));
    assert_eq!(seen.text, "1", "the second commit has not been seen");
    assert!(started.elapsed() >= Duration::from_millis(25));
    assert_eq!(q.published_through(), 1);
}

/// With nothing pushed since the publish a read answers at once.
#[test]
fn a_read_with_nothing_pending_does_not_wait() {
    let q = UiQueues::new();
    q.publish(0, mirror_with("abc"));
    let started = Instant::now();
    assert_eq!(q.read_mirror_within(Duration::from_secs(5)).text, "abc");
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// After the loop ended: no event is queued, a read answers the default at once and one that was waiting
/// is let go, and the queues stay valid for a UI callback that arrives late (the address Java holds
/// never dangles).
///
/// Mutation: a push that is accepted after close (it would grow without a reader); a reader that waits
/// out its bound on a closed bridge.
#[test]
fn a_closed_bridge_takes_nothing_and_answers_defaults() {
    let q = Arc::new(UiQueues::new());
    q.publish(0, mirror_with("text"));
    q.push(UiEvent::Commit("x".into()));
    let waiter = {
        let q = Arc::clone(&q);
        thread::spawn(move || {
            let started = Instant::now();
            let seen = q.read_mirror_within(Duration::from_secs(10));
            (seen, started.elapsed())
        })
    };
    thread::sleep(Duration::from_millis(20));
    q.close();
    let (seen, waited) = waiter.join().unwrap();
    assert!(waited < Duration::from_secs(5), "closing lets the reader go ({waited:?})");
    assert_eq!(seen, ImeMirror::default(), "the reader that was waiting gets the default");
    assert!(q.is_closed());
    q.push(UiEvent::Commit("late".into()));
    q.set_theme(2);
    assert!(q.drain().0.is_empty());
    assert_eq!(q.take_theme(), 0);
    assert_eq!(q.read_mirror(), ImeMirror::default());
}

/// The night mode is a latest-wins flag: 1 light, 2 dark; "undefined" and garbage are ignored; taken once.
#[test]
fn the_night_mode_is_latest_wins_and_taken_once() {
    let q = UiQueues::new();
    assert_eq!(q.take_theme(), 0);
    q.set_theme(1);
    q.set_theme(2);
    assert_eq!(q.take_theme(), 2);
    assert_eq!(q.take_theme(), 0);
    q.set_theme(0);
    q.set_theme(7);
    q.set_theme(-1);
    assert_eq!(q.take_theme(), 0, "no preference expressed is no change");
}

/// The soft keyboard the engine asks for while it handles events is raised by the loop AFTER it published
/// the mirror of the focused text (the IME asks the mirror which kind of field has the focus within
/// milliseconds of `showSoftInput`): the request is a slot, the last word wins, and it is taken once.
///
/// Mutation: the request raised on the spot, from inside the event handling (the mirror then still described the
/// previous field: a plain keyboard for an e-mail field).
#[test]
fn the_keyboard_request_is_a_slot_the_loop_takes_once_and_the_last_word_wins() {
    let q = UiQueues::new();
    assert_eq!(q.take_keyboard_request(), None, "nothing asked: nothing to raise");
    q.request_keyboard(true);
    assert_eq!(q.take_keyboard_request(), Some(true));
    assert_eq!(q.take_keyboard_request(), None, "taken once");
    q.request_keyboard(true);
    q.request_keyboard(false);
    assert_eq!(q.take_keyboard_request(), Some(false), "the app's later 'keep it down' overrides the focus default");
    q.request_keyboard(false);
    q.request_keyboard(true);
    assert_eq!(q.take_keyboard_request(), Some(true));
}
