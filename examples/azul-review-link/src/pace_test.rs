//! The pacer: at most N a second, the newest wins, nothing queues.

use super::*;

#[test]
fn thirty_a_second_is_one_every_thirty_four_milliseconds_so_it_is_never_more_than_thirty() {
    assert_eq!(Pacer::<u32>::new(30).min_interval_ms(), 34);
    assert_eq!(Pacer::<u32>::new(60).min_interval_ms(), 17);
    assert_eq!(Pacer::<u32>::new(1).min_interval_ms(), 1000);
    assert_eq!(Pacer::<u32>::new(0).min_interval_ms(), 1000, "no rate is one a second");
    assert_eq!(Pacer::<u32>::new(5000).min_interval_ms(), 1);
}

#[test]
fn the_first_value_goes_at_once_and_the_next_waits_for_its_turn() {
    let mut pacer = Pacer::new(30);
    pacer.offer(1);
    assert_eq!(pacer.take(100), Some(1));
    pacer.offer(2);
    assert_eq!(pacer.take(110), None, "10 ms later it is too soon");
    assert_eq!(pacer.take(133), None);
    assert_eq!(pacer.take(134), Some(2));
    assert_eq!(pacer.take(500), None, "a value goes once");
}

#[test]
fn a_value_that_waits_is_replaced_by_a_newer_one_and_the_ones_in_between_are_counted() {
    let mut pacer = Pacer::new(30);
    pacer.offer(0);
    assert_eq!(pacer.take(0), Some(0));
    for value in 1..=100 {
        pacer.offer(value);
    }
    assert!(pacer.has_pending());
    assert_eq!(pacer.dropped(), 99);
    assert_eq!(pacer.take(34), Some(100), "the newest wins");
    assert!(!pacer.has_pending());
}

#[test]
fn a_thousand_offers_in_a_second_let_out_at_most_thirty_and_about_thirty() {
    let mut pacer = Pacer::new(30);
    let mut sent = 0;
    let mut last = None;
    for ms in 0..1000u64 {
        pacer.offer(ms);
        if let Some(value) = pacer.take(ms) {
            sent += 1;
            last = Some(value);
        }
    }
    assert!((28..=30).contains(&sent), "{sent} in a second");
    assert!(last.unwrap() >= 966, "it ends on a late value: {last:?}");
}

#[test]
fn sixty_a_second_for_the_gesture_updates() {
    let mut pacer = Pacer::new(60);
    let mut sent = 0;
    for ms in 0..1000u64 {
        pacer.offer(ms);
        if pacer.take(ms).is_some() {
            sent += 1;
        }
    }
    assert!((56..=60).contains(&sent), "{sent}");
}

#[test]
fn a_flush_takes_the_waiting_value_now_and_counts_as_a_send() {
    let mut pacer = Pacer::new(30);
    pacer.offer('a');
    assert_eq!(pacer.take(10), Some('a'));
    pacer.offer('b');
    assert_eq!(pacer.flush(12), Some('b'));
    pacer.offer('c');
    assert_eq!(pacer.take(40), None, "the flush at 12 is the last send: 34 ms are not over");
    assert_eq!(pacer.take(46), Some('c'));
    assert_eq!(pacer.flush(50), None, "nothing waits");
}

#[test]
fn the_pacer_says_when_the_waiting_value_may_go() {
    let mut pacer = Pacer::new(30);
    assert_eq!(pacer.next_due(5), None);
    pacer.offer(1);
    assert_eq!(pacer.next_due(5), Some(5), "nothing was sent yet: now");
    assert_eq!(pacer.take(5), Some(1));
    pacer.offer(2);
    assert_eq!(pacer.next_due(6), Some(39));
    assert_eq!(pacer.next_due(60), Some(60), "a value past its time goes now");
}

#[test]
fn a_clock_that_goes_backwards_waits_and_never_panics() {
    let mut pacer = Pacer::new(30);
    pacer.offer(1);
    assert_eq!(pacer.take(1_000_000), Some(1));
    pacer.offer(2);
    assert_eq!(pacer.take(5), None);
    assert_eq!(pacer.next_due(5), Some(1_000_034));
    assert_eq!(pacer.take(u64::MAX), Some(2));
}

#[test]
fn clear_forgets_what_waits() {
    let mut pacer = Pacer::new(30);
    pacer.offer(1);
    pacer.clear();
    assert!(!pacer.has_pending());
    assert_eq!(pacer.take(1000), None);
}
