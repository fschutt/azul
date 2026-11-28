use super::*;

#[test]
fn the_big_clock_sets_am_and_pm_apart_from_the_time() {
    assert_eq!(clock_parts(6, 30, false), ("06:30".to_string(), None));
    assert_eq!(clock_parts(6, 30, true), ("6:30".to_string(), Some("AM")));
    assert_eq!(clock_parts(0, 5, true), ("12:05".to_string(), Some("AM")));
    assert_eq!(clock_parts(12, 0, true), ("12:00".to_string(), Some("PM")));
    assert_eq!(clock_parts(23, 59, true), ("11:59".to_string(), Some("PM")));
}

#[test]
fn a_time_outside_a_day_wraps_instead_of_panicking() {
    assert_eq!(clock_parts(25, 61, false), ("01:01".to_string(), None));
    assert_eq!(clock(25, 61, true), "1:01 AM");
}

#[test]
fn the_one_line_clock_is_the_two_parts_joined() {
    for hour in 0..24 {
        for twelve in [false, true] {
            let (time, half) = clock_parts(hour, 7, twelve);
            let joined = match half {
                Some(half) => format!("{time} {half}"),
                None => time,
            };
            assert_eq!(clock(hour, 7, twelve), joined);
        }
    }
}
