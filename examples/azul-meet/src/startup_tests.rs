//! The window opens at once: the meetings service is asked after it is up, not before, and the
//! start screen says nothing about the service until its first answer.

use super::*;

const WORKER: &str = "http://127.0.0.1:8790";

fn starting() -> MeetState {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = Some(room_at_start(WORKER.to_string(), rooms::Relay::Default, "--worker"));
    s
}

#[test]
fn the_meetings_service_is_asked_as_soon_as_the_window_is_up() {
    let mut s = starting();
    assert!(server_retry(&mut s).is_some(), "the first check goes out at once, not after a retry period");
}

#[test]
fn the_start_screen_says_nothing_about_the_service_while_it_is_first_asked() {
    let s = starting();
    let lobby = snapshot(&s).lobby.expect("the start screen");
    assert_eq!(ui::server_problem(&lobby), None, "no 'does not answer' before it was asked");
}

#[test]
fn without_a_meetings_service_the_start_screen_says_so_at_once() {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = Some(room_at_start(String::new(), rooms::Relay::Default, ""));
    assert!(server_retry(&mut s).is_none(), "nothing to ask");
    let lobby = snapshot(&s).lobby.expect("the start screen");
    assert_eq!(ui::server_problem(&lobby).as_deref(), Some("No meetings service is set."));
}
