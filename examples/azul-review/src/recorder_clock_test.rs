//! The session clock and the way a moment of it is told: no window, no azul.

use super::*;

#[test]
fn a_mock_clock_stands_still_until_it_is_moved_and_never_goes_back() {
    let clock = MockClock::at(5_000);
    assert_eq!(clock.now_us(), 5_000);
    assert_eq!(clock.now_us(), 5_000, "reading it does not move it");
    clock.advance_ms(20);
    assert_eq!(clock.now_us(), 25_000);
    clock.advance_us(7);
    assert_eq!(clock.now_us(), 25_007);
    clock.set_us(1_000);
    assert_eq!(clock.now_us(), 25_007, "a time in the past is ignored: the clock is monotonic");
    clock.set_us(90_000);
    assert_eq!(clock.now_us(), 90_000);
}

#[test]
fn clones_of_a_mock_clock_share_one_time() {
    let test_side = MockClock::default();
    let recorder_side = test_side.clone();
    test_side.advance_ms(1_500);
    assert_eq!(recorder_side.now_us(), 1_500_000);
}

#[test]
fn the_real_clock_starts_near_zero_and_does_not_go_back() {
    let clock = SessionClock::start(1_700_000_000_000);
    let a = clock.now_us();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let b = clock.now_us();
    assert!(a < 1_000_000, "a session clock starts at its origin, not at the machine's boot: {a}");
    assert!(b >= a + 2_000, "three milliseconds went by: {a} -> {b}");
    assert_eq!(clock.origin_unix_ms(), 1_700_000_000_000);
}

#[test]
fn a_clock_is_usable_as_a_trait_object_from_another_thread() {
    let clock: Arc<dyn Clock> = Arc::new(MockClock::at(42));
    let seen = {
        let clock = Arc::clone(&clock);
        std::thread::spawn(move || clock.now_us()).join().unwrap()
    };
    assert_eq!(seen, 42);
}

#[test]
fn the_calendar_time_of_known_moments_is_right() {
    // The epoch, a leap day, the end of a year, and the day of the first plan of this recorder.
    let cases: [(u64, &str); 5] = [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400_000, "2000-02-29T00:00:00Z"),
        (1_767_225_599_000, "2025-12-31T23:59:59Z"),
        (1_764_108_000_000, "2025-11-25T22:00:00Z"),
        (4_102_444_800_000, "2100-01-01T00:00:00Z"),
    ];
    for (ms, text) in cases {
        assert_eq!(CivilTime::from_unix_ms(ms).iso8601(), text, "{ms} ms");
    }
}

#[test]
fn milliseconds_do_not_change_the_second() {
    assert_eq!(
        CivilTime::from_unix_ms(1_764_108_000_999).iso8601(),
        "2025-11-25T22:00:00Z"
    );
}

#[test]
fn a_session_folder_name_sorts_by_time_and_has_no_colon() {
    let a = CivilTime::from_unix_ms(1_764_108_000_000).folder_name();
    let b = CivilTime::from_unix_ms(1_764_108_001_000).folder_name();
    assert_eq!(a, "2025-11-25_22-00-00");
    assert!(a < b);
    assert!(!a.contains(':') && !a.contains('/'));
}

#[test]
fn the_elapsed_time_is_told_the_way_an_indicator_shows_it() {
    assert_eq!(clock_text(0), "0:00");
    assert_eq!(clock_text(59_999_999), "0:59");
    assert_eq!(clock_text(61_000_000), "1:01");
    assert_eq!(clock_text(3_599_000_000), "59:59");
    assert_eq!(clock_text(3_600_000_000), "1:00:00");
    assert_eq!(clock_text(8 * 3_600_000_000 + 5 * 60_000_000 + 9_000_000), "8:05:09");
}
