use super::*;
use chrono::{NaiveDate, NaiveTime};

const ID: &str = "00000000-0000-4000-8000-000000000001";

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn at(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

#[test]
fn a_size_a_drag_reported_is_held_to_what_the_app_keeps() {
    assert_eq!(clamped_preview_size(320.0), 320.0);
    assert_eq!(clamped_preview_size(0.0), 0.0);
    assert_eq!(clamped_preview_size(-5.0), 0.0, "no size is the widget's default");
    assert_eq!(clamped_preview_size(f32::NAN), 0.0);
    assert_eq!(clamped_preview_size(f32::INFINITY), 0.0);
    assert_eq!(clamped_preview_size(1.0e9), PREVIEW_SIZE_MAX);
}

#[test]
fn a_desktops_dock_is_always_open_and_a_phones_pane_only_while_an_event_is_selected() {
    assert!(is_open(false, false), "the dock is empty and says so");
    assert!(is_open(false, true));
    assert!(!is_open(true, false), "a phone has no pane with nothing to show");
    assert!(is_open(true, true));
}

#[test]
fn an_event_says_when_it_is_in_the_windows_language() {
    crate::l10n::in_english();
    let timed = Event::create(ID, "Review", day(2026, 11, 28), at(10, 0), at(11, 0), None).unwrap();
    assert_eq!(when_text(&timed, day(2026, 11, 28)), "Sat 28 Nov, 10:00 - 11:00");
    let all_day = Event::create_all_day(ID, "Holiday", day(2026, 11, 28), day(2026, 11, 28)).unwrap();
    assert_eq!(when_text(&all_day, day(2026, 11, 28)), "Sat 28 Nov, all day");
    let long = Event::create_all_day(ID, "Trip", day(2026, 11, 27), day(2026, 11, 29)).unwrap();
    assert_eq!(when_text(&long, day(2026, 11, 27)), "Fri 27 Nov - Sun 29 Nov, all day");
}

#[test]
fn a_reminder_is_said_as_the_list_of_the_editor_says_it() {
    crate::l10n::in_english();
    assert_eq!(reminder_text(Some(15)), "15 minutes before");
    assert_eq!(reminder_text(Some(0)), "At the start");
    assert_eq!(reminder_text(Some(1440)), "1 day before");
    // a reminder that is no choice of the list is its nearest earlier one
    assert_eq!(reminder_text(Some(20)), "15 minutes before");
}

#[test]
fn the_title_is_the_events_else_untitled_else_event() {
    crate::l10n::in_english();
    let named = Event::create(ID, "Review", day(2026, 11, 28), at(10, 0), at(11, 0), None).unwrap();
    assert_eq!(title(Some(&named)), "Review");
    assert_eq!(title(None), t("azcalendar-preview-title"));
}
