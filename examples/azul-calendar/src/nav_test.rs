use super::*;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn input<'a>(calendars: &'a [(String, Colour, bool)], tasks: &'a [String]) -> NavInput<'a> {
    NavInput {
        calendars,
        tasks,
        calendars_title: String::from("Calendars"),
        tasks_title: String::from("To-Do"),
        add_calendar: String::from("Add a calendar"),
    }
}

#[test]
fn every_tag_names_what_it_stands_for_and_back() {
    for target in [
        NavTarget::Calendar(0),
        NavTarget::Calendar(41),
        NavTarget::AddCalendar,
        NavTarget::Task(0),
        NavTarget::Task(5),
    ] {
        assert_eq!(target_of(tag_of(target)), Some(target), "{target:?}");
    }
    // a tag no row carries
    assert_eq!(target_of(0), None);
    assert_eq!(target_of(998), None);
}

#[test]
fn the_calendars_are_check_rows_in_their_colours_with_add_a_calendar_last() {
    let calendars = vec![
        (String::from("Calendar"), Colour::Blue, true),
        (String::from("Work"), Colour::Green, false),
    ];
    let sections = plan(&input(&calendars, &[]));
    assert_eq!(sections.len(), 1, "no tasks, no section for them");
    let rows = &sections[0].rows;
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[0].checked, rows[0].colour), (Some(true), Some(Colour::Blue)));
    assert_eq!((rows[1].checked, rows[1].colour), (Some(false), Some(Colour::Green)));
    assert_eq!(rows[2].label, "Add a calendar");
    assert_eq!(rows[2].checked, None, "the add row has no box");
    assert_eq!(target_of(rows[2].tag), Some(NavTarget::AddCalendar));
    assert_eq!(target_of(rows[1].tag), Some(NavTarget::Calendar(1)));
}

#[test]
fn open_tasks_are_a_section_of_their_own_capped_at_a_few() {
    let calendars = vec![(String::from("Calendar"), Colour::Blue, true)];
    let tasks: Vec<String> = (0..20).map(|i| format!("Task {i}")).collect();
    let sections = plan(&input(&calendars, &tasks));
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[1].title, "To-Do");
    assert_eq!(sections[1].rows.len(), MAX_TASK_ROWS);
    assert!(sections[1].rows.iter().all(|r| r.checked == Some(false)));
    assert_eq!(target_of(sections[1].rows[3].tag), Some(NavTarget::Task(3)));
}

#[test]
fn a_folder_of_more_calendars_than_the_cap_keeps_the_first_ones_and_says_how_many_are_left_out() {
    let calendars: Vec<(String, Colour, bool)> = (0..MAX_CALENDAR_ROWS + 7)
        .map(|i| (format!("C{i}"), Colour::Grey, true))
        .collect();
    let sections = plan(&input(&calendars, &[]));
    assert_eq!(sections[0].rows.len(), MAX_CALENDAR_ROWS + 1, "the cap and the add row");
    assert_eq!(calendars_left_out(calendars.len()), 7);
    assert_eq!(calendars_left_out(3), 0);
}

#[test]
fn the_rail_has_today_and_the_new_appointment_nearest_the_thumb() {
    assert_eq!(QUICK.last().map(|q| q.2), Some(Quick::NewEvent), "the most used is the last");
    for (i, (_, _, quick)) in QUICK.iter().enumerate() {
        assert_eq!(quick_tag(*quick), i);
        assert_eq!(quick_of(i), Some(*quick));
    }
    assert_eq!(quick_of(QUICK.len()), None);
}

#[test]
fn an_events_sheet_offers_join_and_copy_link_for_a_meeting_and_one_occurrence_for_a_series() {
    let all = event_sheet_rows(true, true);
    assert_eq!(all.len(), EVENT_SHEET.len());
    let plain = event_sheet_rows(false, false);
    assert_eq!(plain.len(), 2, "edit and delete");
    assert_eq!(event_sheet_rows(true, false).len(), 4, "no occurrence to skip");
    assert_eq!(event_sheet_rows(false, true).len(), 3, "no link to join or copy");
    // the dangerous rows come last, the whole delete at the very end
    for rows in [&all, &plain] {
        let last = EVENT_SHEET[*rows.last().unwrap()];
        assert!(last.3, "the dangerous row is last: {last:?}");
        assert_eq!(last.2, EventAction::Delete);
    }
}

#[test]
fn file_items_are_the_six_pages_in_their_order() {
    assert_eq!(FILE_ITEMS.len(), crate::args::BackstagePage::ALL.len());
    for (i, page) in crate::args::BackstagePage::ALL.iter().enumerate() {
        assert_eq!(FILE_ITEMS[i].2, *page);
        assert_eq!(FILE_ITEMS[i].0, page.label());
    }
}

#[test]
fn the_pen_goes_to_the_day_a_text_names() {
    let today = day(2026, 11, 28);
    let shown = day(2026, 11, 3);
    let go = |text: &str| parse_goto(text, today, shown);
    assert_eq!(go("2026-12-24"), Some(day(2026, 12, 24)));
    assert_eq!(go("2026/12/24"), Some(day(2026, 12, 24)));
    assert_eq!(go("24.12.2026"), Some(day(2026, 12, 24)));
    assert_eq!(go("24.12."), Some(day(2026, 12, 24)), "this year's, the shown day's");
    assert_eq!(go("24/12"), Some(day(2026, 12, 24)));
    assert_eq!(go("2027-02"), Some(day(2027, 2, 1)), "a month: its first day");
    assert_eq!(go("24 Dec"), Some(day(2026, 12, 24)));
    assert_eq!(go("dec 24"), Some(day(2026, 12, 24)));
    assert_eq!(go("24 december 2027"), Some(day(2027, 12, 24)));
    assert_eq!(go("24. Dezember"), Some(day(2026, 12, 24)));
    assert_eq!(go("3 Mär"), Some(day(2026, 3, 3)));
    assert_eq!(go("today"), Some(today));
    assert_eq!(go("Tomorrow"), Some(day(2026, 11, 29)));
    assert_eq!(go("gestern"), Some(day(2026, 11, 27)));
}

#[test]
fn the_pen_counts_from_the_shown_day() {
    let today = day(2026, 11, 28);
    let shown = day(2026, 1, 31);
    let go = |text: &str| parse_goto(text, today, shown);
    assert_eq!(go("+3"), Some(day(2026, 2, 3)));
    assert_eq!(go("-1"), Some(day(2026, 1, 30)));
    assert_eq!(go("+1w"), Some(day(2026, 2, 7)));
    assert_eq!(go("+1m"), Some(day(2026, 2, 28)), "the day is held to the month's length");
    assert_eq!(go("-2m"), Some(day(2025, 11, 30)));
}

#[test]
fn the_pen_refuses_a_text_that_is_no_day_without_crashing() {
    let today = day(2026, 11, 28);
    for text in ["", "  ", "abc", "13/13/2026", "99 Dec 99999", "2026-13-01", "+x", "+1y", "dec dec", "24 25 dec", "§§"] {
        assert_eq!(parse_goto(text, today, today), None, "{text:?}");
    }
    // a huge count is capped (5000 days), not an overflow
    assert_eq!(
        parse_goto("+999999999999", today, today),
        Some(today + chrono::Duration::days(5000))
    );
}

#[test]
fn a_calendars_box_is_tinted_with_its_edge_colour_in_the_modes_twin() {
    assert_eq!(rgb_of(Colour::Blue, false), (0x2f, 0x6d, 0xb0));
    assert_eq!(rgb_of(Colour::Blue, true), (0x6e, 0xa8, 0xff));
    assert_eq!(rgb_of(Colour::Green, false), (0x3a, 0x8a, 0x3a));
    // every colour has two valid twins (no grey fallback)
    for colour in Colour::ALL {
        for dark in [false, true] {
            assert_ne!(rgb_of(colour, dark), (128, 128, 128), "{colour:?} dark={dark}");
        }
    }
}

#[test]
fn the_links_pie_says_how_many_links_the_server_has_and_what_it_means() {
    // no link, nothing to say
    assert_eq!(link_status(0, 0, 0, false), None);
    // all registered
    let all = link_status(4, 0, 0, false).unwrap();
    assert_eq!((all.percent, all.tone), (100, LinkTone::Good));
    assert!((all.fraction - 1.0).abs() < f32::EPSILON);
    // two of four wait: nothing wrong yet
    let waiting = link_status(4, 2, 0, false).unwrap();
    assert_eq!((waiting.percent, waiting.tone), (50, LinkTone::Warning));
    // on their way
    assert_eq!(link_status(4, 2, 1, false).unwrap().tone, LinkTone::Busy);
    // the server did not take them
    assert_eq!(link_status(4, 4, 0, true).unwrap().tone, LinkTone::Danger);
    assert_eq!(link_status(4, 4, 0, true).unwrap().percent, 0);
    // a failure with nothing pending is no failure; more pending than events is held to the events
    assert_eq!(link_status(2, 0, 0, true).unwrap().tone, LinkTone::Good);
    assert_eq!(link_status(2, 9, 0, false).unwrap().percent, 0);
}

#[test]
fn a_calendars_box_hides_a_shown_calendar_and_shows_a_hidden_one() {
    let mut hidden = std::collections::BTreeSet::new();
    assert!(toggle_hidden(&mut hidden, "work"), "hidden now");
    assert!(hidden.contains("work"));
    assert!(!toggle_hidden(&mut hidden, "work"), "shown again");
    assert!(hidden.is_empty());
    // the default calendar has the empty id
    assert!(toggle_hidden(&mut hidden, ""));
    assert!(hidden.contains(""));
}

/// English, as a test says a place.
struct English;

impl Say for English {
    fn year(&self, year: i32) -> String {
        year.to_string()
    }
    fn month(&self, month: u32) -> String {
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
            [(month as usize + 11) % 12]
            .to_string()
    }
    fn day(&self, day: NaiveDate) -> String {
        day.format("%a %-d %b").to_string()
    }
    fn range(&self, first: NaiveDate, last: NaiveDate) -> String {
        format!("{} - {}", first.format("%-d %b"), last.format("%-d %b"))
    }
    fn list(&self) -> String {
        String::from("Next 7 Days")
    }
}

#[test]
fn the_month_is_the_years_child_and_a_week_or_a_day_the_months() {
    use crate::views::ViewKind;
    let anchor = day(2026, 11, 28);
    let say = &English;
    let month = place_of(ViewKind::Month, anchor, (day(2026, 11, 1), day(2026, 11, 30)), say);
    assert_eq!(month.crumbs, vec![(String::from("2026"), TAG_YEAR)]);
    assert_eq!(month.name, "November");
    let week = place_of(ViewKind::Week, anchor, (day(2026, 11, 23), day(2026, 11, 29)), say);
    assert_eq!(
        week.crumbs,
        vec![(String::from("2026"), TAG_YEAR), (String::from("November"), TAG_MONTH)]
    );
    assert_eq!(week.name, "23 Nov - 29 Nov");
    let one = place_of(ViewKind::Day, anchor, (anchor, anchor), say);
    assert_eq!(one.crumbs.len(), 2);
    assert_eq!(one.name, "Sat 28 Nov");
    let list = place_of(ViewKind::Agenda, anchor, (anchor, day(2026, 12, 4)), say);
    assert_eq!(list.name, "Next 7 Days");
    assert_eq!(list.crumbs[1].0, "November");
}

#[test]
fn a_week_across_two_months_stands_in_the_month_it_starts_in() {
    use crate::views::ViewKind;
    let anchor = day(2026, 12, 2);
    let week = place_of(ViewKind::Week, anchor, (day(2026, 11, 30), day(2026, 12, 6)), &English);
    assert_eq!(week.crumbs[1], (String::from("November"), TAG_MONTH));
    // a day stands in its own month
    let one = place_of(ViewKind::Day, anchor, (anchor, anchor), &English);
    assert_eq!(one.crumbs[1], (String::from("December"), TAG_MONTH));
}

