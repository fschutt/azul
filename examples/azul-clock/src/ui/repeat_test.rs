//! The chips' weekdays and the RRULE text: no window.

use super::*;

const WEEKDAYS: u8 = 0b001_1111;
const WEEKEND: u8 = 0b110_0000;

#[test]
fn the_presets_rules_light_the_days_they_name() {
    assert_eq!(days_of(""), 0);
    assert_eq!(days_of("FREQ=DAILY"), EVERY_DAY);
    assert_eq!(days_of("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"), WEEKDAYS);
    assert_eq!(days_of("FREQ=WEEKLY;BYDAY=SA,SU"), WEEKEND);
    // Any case, and spaces round the text.
    assert_eq!(days_of(" freq=weekly;byday=mo,we "), 0b000_0101);
}

#[test]
fn what_a_chip_cannot_say_lights_no_day() {
    for rule in [
        "FREQ=MONTHLY;BYMONTHDAY=1",
        "FREQ=YEARLY",
        "FREQ=DAILY;INTERVAL=2",
        "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
        "FREQ=WEEKLY;BYDAY=MO;COUNT=4",
        "FREQ=WEEKLY;BYDAY=MO;UNTIL=20270101",
        "FREQ=WEEKLY",
        "nonsense",
    ] {
        assert_eq!(days_of(rule), 0, "{rule}");
    }
}

#[test]
fn the_rule_of_the_lit_days_is_read_back_as_the_same_days() {
    for days in 0..=EVERY_DAY {
        assert_eq!(days_of(&rrule_of(days)), days, "{days:07b}");
    }
}

#[test]
fn no_day_is_once_every_day_is_daily_and_the_rest_is_a_weekly_rule() {
    assert_eq!(rrule_of(0), "");
    assert_eq!(rrule_of(EVERY_DAY), "FREQ=DAILY");
    assert_eq!(rrule_of(0b001_0101), "FREQ=WEEKLY;BYDAY=MO,WE,FR");
    assert_eq!(rrule_of(WEEKEND), "FREQ=WEEKLY;BYDAY=SA,SU");
    // Bits past Sunday are nothing.
    assert_eq!(rrule_of(0b1000_0000), "");
}

#[test]
fn a_chip_lights_a_day_and_puts_it_out_again() {
    assert_eq!(toggled(0, 0), 0b000_0001);
    assert_eq!(toggled(0b000_0001, 0), 0);
    assert_eq!(toggled(WEEKDAYS, 5), WEEKDAYS | 0b010_0000);
    assert_eq!(toggled(EVERY_DAY, 6), 0b011_1111);
    // A number past Sunday changes nothing.
    assert_eq!(toggled(WEEKDAYS, 7), WEEKDAYS);
    assert_eq!(toggled(WEEKDAYS, 200), WEEKDAYS);
}
