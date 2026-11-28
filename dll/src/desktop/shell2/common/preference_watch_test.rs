//! The watch asks at its interval, from the shell's own start.

use std::time::{Duration, Instant};

use super::*;

#[test]
fn the_first_ask_is_due_and_the_next_one_waits_its_interval() {
    let mut watch = PreferenceWatch::every(Duration::from_millis(250));
    let start = Instant::now();
    assert!(watch.due_at(start));
    assert!(!watch.due_at(start + Duration::from_millis(100)));
    assert!(watch.due_at(start + Duration::from_millis(400)));
    watch.ask_again_now();
    assert!(watch.due_at(start + Duration::from_millis(401)));
    assert!(!watch.due_at(start + Duration::from_millis(402)));
}

#[test]
fn an_instant_before_the_start_is_the_start() {
    let mut watch = PreferenceWatch::every(Duration::from_secs(1));
    let before = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    assert!(watch.due_at(before), "saturates to zero: due");
    assert!(!watch.due_at(before), "and not due again at once");
}
