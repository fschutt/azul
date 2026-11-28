//! The snooze steps: no window.

use super::*;
use crate::alarm::{DEFAULT_SNOOZE_MINUTES, MAX_SNOOZE_MINUTES};

#[test]
fn the_list_runs_up_inside_what_an_alarm_may_snooze() {
    assert!(SNOOZE_MINUTES.windows(2).all(|w| w[0] < w[1]), "{SNOOZE_MINUTES:?}");
    assert!(SNOOZE_MINUTES.iter().all(|m| (1..=MAX_SNOOZE_MINUTES).contains(m)), "{SNOOZE_MINUTES:?}");
    assert!(SNOOZE_MINUTES.contains(&DEFAULT_SNOOZE_MINUTES), "a new alarm's snooze is a step of the list");
}

#[test]
fn a_step_goes_to_the_neighbour_in_the_list() {
    assert_eq!(stepped(10, -1), 5);
    assert_eq!(stepped(10, 1), 15);
    assert_eq!(stepped(30, 1), 60);
    assert_eq!(stepped(5, -1), 1);
}

#[test]
fn a_length_off_the_list_steps_to_the_entry_beside_it() {
    assert_eq!(stepped(7, -1), 5);
    assert_eq!(stepped(7, 1), 10);
    assert_eq!(stepped(0, 1), 1);
    // More than an alarm may snooze (data of another version): shorter lands in the list.
    assert_eq!(stepped(90, -1), 60);
}

#[test]
fn the_ends_stay_and_no_step_changes_nothing() {
    assert_eq!(stepped(1, -1), 1);
    assert_eq!(stepped(60, 1), 60);
    assert_eq!(stepped(10, 0), 10);
    assert_eq!(stepped(90, 1), 90);
}

#[test]
fn a_step_exists_except_past_an_end() {
    assert!(can_step(10, -1));
    assert!(can_step(10, 1));
    assert!(!can_step(1, -1));
    assert!(!can_step(60, 1));
    assert!(!can_step(10, 0));
}

#[test]
fn a_number_of_steps_walks_the_whole_list_and_back() {
    let mut at = SNOOZE_MINUTES[0];
    for expected in &SNOOZE_MINUTES[1..] {
        at = stepped(at, 1);
        assert_eq!(at, *expected);
    }
    for expected in SNOOZE_MINUTES[..SNOOZE_MINUTES.len() - 1].iter().rev() {
        at = stepped(at, -1);
        assert_eq!(at, *expected);
    }
}
