//! The review session without a window: every event fed to every state against the table, the hover storm that starts nothing, the clock of
//! a session, and what the start-stop and pause keys mean.

use super::*;
use Capture::{Close, Open, Unchanged};
use State::{Idle, Paused, Recording, Stopped};

const STATES: [State; 4] = [Idle, Recording, Paused, Stopped];

/// Every transition that is not "nothing happens", spelled out on its own (the test does not use `step`'s own match).
const TRANSITIONS: [(State, Event, State, Capture); 11] = [
    (Idle, Event::Start, Recording, Open),
    (Stopped, Event::Start, Recording, Open),
    (Paused, Event::Resume, Recording, Open),
    (Recording, Event::Pause, Paused, Close),
    (Recording, Event::Stop, Stopped, Close),
    (Paused, Event::Stop, Stopped, Close),
    (Recording, Event::WindowClose, Stopped, Close),
    (Paused, Event::WindowClose, Stopped, Close),
    (Recording, Event::AppQuit, Stopped, Close),
    (Paused, Event::AppQuit, Stopped, Close),
    (Recording, Event::DeviceLost, Paused, Close),
];

fn expected(state: State, event: Event) -> (State, Capture) {
    TRANSITIONS.iter().find(|(s, e, _, _)| *s == state && *e == event).map_or((state, Unchanged), |(_, _, next, capture)| (*next, *capture))
}

#[test]
fn every_event_in_every_state_does_what_the_table_says_and_nothing_else() {
    for state in STATES {
        for event in Event::ALL {
            let got = step(state, event);
            assert_eq!((got.state, got.capture), expected(state, event), "{state:?} + {event:?}");
        }
    }
}

#[test]
fn a_window_starts_idle_and_nothing_listens() {
    let session = Session::new();
    assert_eq!(session.state(), Idle);
    assert!(!session.listens());
    assert_eq!(session.elapsed_ms(123_456), 0);
}

#[test]
fn only_the_persons_start_and_resume_open_the_microphone_and_never_from_a_session_that_runs() {
    for state in STATES {
        for event in Event::ALL {
            let got = step(state, event);
            if got.capture == Open {
                assert!(matches!(event, Event::Start | Event::Resume), "{state:?} + {event:?} opened the microphone");
                assert_ne!(state, Recording, "{event:?} opened a microphone that a running session has");
            }
        }
    }
}

#[test]
fn what_the_person_does_while_looking_at_the_code_annotates_a_running_session_and_changes_nothing() {
    for state in STATES {
        for event in Event::ALL.into_iter().filter(|e| e.is_annotation()) {
            let got = step(state, event);
            assert_eq!(got.state, state, "{state:?} + {event:?} changed the session");
            assert_eq!(got.capture, Unchanged, "{state:?} + {event:?} did something to the microphone");
            assert_eq!(got.annotates, state == Recording, "{state:?} + {event:?}: only a running session hears the pen");
        }
    }
}

#[test]
fn the_explicit_events_are_not_annotations_and_are_not_heard_by_what_listens() {
    for event in [Event::Start, Event::Pause, Event::Resume, Event::Stop, Event::WindowClose, Event::AppQuit, Event::DeviceLost] {
        assert!(!event.is_annotation(), "{event:?}");
        for state in STATES {
            assert!(!step(state, event).annotates, "{state:?} + {event:?}");
        }
    }
}

#[test]
fn a_storm_of_hovering_moving_and_pens_coming_near_in_a_window_that_was_not_started_opens_nothing_and_starts_nothing() {
    for start in [Idle, Paused, Stopped] {
        let mut session = Session { state: start, since_ms: None, banked_ms: 0 };
        let mut opened = 0;
        let mut heard = 0;
        for i in 0..20_000_u64 {
            // every kind of annotation, over and over, as a pen hovering over a page for a minute does
            let event = [Event::Hover, Event::PointerMove, Event::PenNear, Event::PointerEnter, Event::PointerLeave, Event::FocusGained, Event::FocusLost, Event::Click, Event::Key, Event::PenRest][usize::try_from(i % 10).unwrap_or(0)];
            let step = session.apply(event, i * 3);
            opened += usize::from(step.capture == Open);
            heard += usize::from(step.annotates);
        }
        assert_eq!((opened, heard), (0, 0), "from {start:?}");
        assert_eq!(session.state(), start);
        assert!(!session.listens());
        assert_eq!(session.elapsed_ms(1_000_000), 0, "no session ran");
    }
}

#[test]
fn a_session_runs_from_start_to_stop_with_a_pause_and_counts_only_what_it_recorded() {
    let mut s = Session::new();
    assert_eq!(s.apply(Event::Start, 1_000).state, Recording);
    assert_eq!(s.elapsed_ms(2_500), 1_500);
    assert_eq!(s.apply(Event::Pause, 4_000).state, Paused);
    assert_eq!(s.elapsed_ms(4_000), 3_000);
    assert_eq!(s.elapsed_ms(9_000), 3_000, "held: the clock does not run");
    assert!(!s.listens());
    assert_eq!(s.apply(Event::Resume, 10_000).state, Recording);
    assert_eq!(s.elapsed_ms(11_000), 4_000);
    assert_eq!(s.apply(Event::Stop, 12_000).state, Stopped);
    assert_eq!(s.elapsed_ms(99_000), 5_000, "ended: what it recorded stays");
    assert!(!s.listens());
    // a new session starts from nothing
    assert_eq!(s.apply(Event::Start, 100_000).state, Recording);
    assert_eq!(s.elapsed_ms(100_500), 500);
}

#[test]
fn stopping_a_held_session_keeps_what_it_recorded() {
    let mut s = Session::new();
    s.apply(Event::Start, 0);
    s.apply(Event::Pause, 2_000);
    assert_eq!(s.apply(Event::Stop, 5_000).capture, Close);
    assert_eq!(s.state(), Stopped);
    assert_eq!(s.elapsed_ms(9_000), 2_000);
}

#[test]
fn a_start_of_a_session_that_runs_and_a_pause_of_one_that_is_held_do_nothing() {
    let mut s = Session::new();
    s.apply(Event::Start, 0);
    let again = s.apply(Event::Start, 500);
    assert_eq!((again.state, again.capture), (Recording, Unchanged));
    assert_eq!(s.elapsed_ms(1_000), 1_000, "the clock was not started again");
    s.apply(Event::Pause, 1_000);
    let twice = s.apply(Event::Pause, 1_500);
    assert_eq!((twice.state, twice.capture), (Paused, Unchanged));
    assert_eq!(s.elapsed_ms(2_000), 1_000);
}

#[test]
fn a_microphone_that_is_lost_holds_the_session_and_it_does_not_start_again_by_itself() {
    let mut s = Session::new();
    s.apply(Event::Start, 0);
    let lost = s.apply(Event::DeviceLost, 3_000);
    assert_eq!((lost.state, lost.capture), (Paused, Close));
    for event in [Event::Hover, Event::PenNear, Event::FocusGained, Event::Click, Event::PenRest] {
        assert_eq!(s.apply(event, 4_000).state, Paused, "{event:?} resumed a session whose microphone was lost");
    }
    assert_eq!(s.apply(Event::Resume, 5_000).capture, Open, "only the person's Resume does");
    // a loss with no session to hold is nothing
    let mut none = Session::new();
    assert_eq!(none.apply(Event::DeviceLost, 0).capture, Unchanged);
}

#[test]
fn closing_the_window_and_quitting_end_a_session_that_exists_and_a_window_with_none_has_nothing_to_end() {
    for event in [Event::WindowClose, Event::AppQuit] {
        let mut running = Session::new();
        running.apply(Event::Start, 0);
        let ended = running.apply(event, 2_000);
        assert_eq!((ended.state, ended.capture), (Stopped, Close), "{event:?}");
        assert_eq!(running.elapsed_ms(5_000), 2_000);
        let mut held = Session::new();
        held.apply(Event::Start, 0);
        held.apply(Event::Pause, 1_000);
        assert_eq!(held.apply(event, 2_000).capture, Close, "{event:?}: a held session may have left the microphone open");
        let mut idle = Session::new();
        let nothing = idle.apply(event, 0);
        assert_eq!((nothing.state, nothing.capture), (Idle, Unchanged), "{event:?}");
    }
}

#[test]
fn the_start_stop_key_starts_what_is_not_running_and_stops_what_is() {
    let mut s = Session::new();
    assert_eq!(s.toggle_event(), Event::Start);
    s.apply(Event::Start, 0);
    assert_eq!(s.toggle_event(), Event::Stop);
    s.apply(Event::Pause, 1);
    assert_eq!(s.toggle_event(), Event::Stop, "a held session is stopped, not started again");
    s.apply(Event::Stop, 2);
    assert_eq!(s.toggle_event(), Event::Start);
}

#[test]
fn the_pause_key_pauses_what_runs_resumes_what_is_held_and_is_nothing_otherwise() {
    let mut s = Session::new();
    assert_eq!(s.pause_event(), None, "nothing to pause");
    s.apply(Event::Start, 0);
    assert_eq!(s.pause_event(), Some(Event::Pause));
    s.apply(Event::Pause, 1);
    assert_eq!(s.pause_event(), Some(Event::Resume));
    s.apply(Event::Stop, 2);
    assert_eq!(s.pause_event(), None);
}

#[test]
fn a_length_is_shown_as_minutes_and_seconds_and_from_an_hour_on_with_hours() {
    assert_eq!(length_text(0), "0:00");
    assert_eq!(length_text(999), "0:00");
    assert_eq!(length_text(42_000), "0:42");
    assert_eq!(length_text(61_000), "1:01");
    assert_eq!(length_text(3_599_000), "59:59");
    assert_eq!(length_text(3_600_000), "1:00:00");
    assert_eq!(length_text(37_230_000), "10:20:30");
    assert!(!length_text(u64::MAX).is_empty(), "no length is too long to show");
}

#[test]
fn the_names_a_script_reads_are_the_four_states() {
    let names: Vec<&str> = STATES.iter().map(|s| s.name()).collect();
    assert_eq!(names, vec!["idle", "recording", "paused", "stopped"]);
}
