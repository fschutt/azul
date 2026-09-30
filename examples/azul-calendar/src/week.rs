//! Week math for the week view: which events fall in a week, and where each one sits in its
//! day's column. Weeks start on Monday; the view shows 08:00 to 20:00.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};

use crate::event::Event;

/// The first hour the week view shows.
pub const FIRST_HOUR: u32 = 8;
/// The hour the week view ends at: it shows 08:00 up to 20:00.
pub const END_HOUR: u32 = 20;

/// The Monday on or before `day`.
pub fn week_start(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// The Monday `weeks` weeks after (or, negative, before) the week of `day`.
pub fn shift_weeks(day: NaiveDate, weeks: i64) -> NaiveDate {
    week_start(day) + Duration::days(7 * weeks)
}

/// The seven days of the week of `day`, Monday first.
pub fn week_days(day: NaiveDate) -> [NaiveDate; 7] {
    let monday = week_start(day);
    std::array::from_fn(|i| monday + Duration::days(i as i64))
}

/// The events of the week of `day`, one list per day (Monday first), each in order of start,
/// end and title.
pub fn events_in_week(events: &[Event], day: NaiveDate) -> [Vec<&Event>; 7] {
    let monday = week_start(day);
    let mut days: [Vec<&Event>; 7] = Default::default();
    for event in events {
        let offset = (event.date - monday).num_days();
        if (0..7).contains(&offset) {
            days[offset as usize].push(event);
        }
    }
    for list in &mut days {
        list.sort_by(|a, b| {
            (a.start, a.end, &a.title, &a.id).cmp(&(b.start, b.end, &b.title, &b.id))
        });
    }
    days
}

/// Minutes since midnight.
pub fn minute_of_day(t: NaiveTime) -> u32 {
    t.hour() * 60 + t.minute()
}

/// Where an event sits in its day's column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The event's index in the day's list.
    pub index: usize,
    /// Minutes from `FIRST_HOUR` to the event's top edge, clipped to the view.
    pub top: u32,
    /// Minutes of the event inside the view.
    pub height: u32,
    /// Which of the `lanes` side-by-side columns the event takes; 0 is the leftmost.
    pub lane: u32,
    /// How many side-by-side columns its group of overlapping events needs.
    pub lanes: u32,
}

/// One day's column: the events in view, and how many lie wholly before or after the view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayLayout {
    pub placements: Vec<Placement>,
    pub earlier: usize,
    pub later: usize,
}

/// Lays out one day's events. Events that overlap in time sit side by side: a group of events
/// linked by overlaps shares the column's width in as many lanes as it needs at once, each
/// event in the leftmost lane that is free at its start. An event that ends as another starts
/// does not overlap it. Events are clipped to 08:00 - 20:00.
pub fn lay_out_day(events: &[&Event]) -> DayLayout {
    let view_start = FIRST_HOUR * 60;
    let view_end = END_HOUR * 60;
    let mut layout = DayLayout::default();
    let mut order: Vec<usize> = (0..events.len()).collect();
    order.sort_by_key(|&i| (minute_of_day(events[i].start), minute_of_day(events[i].end)));
    // The group being built: where its placements begin, when its last event ends, and when
    // each of its lanes is free again.
    let mut group_from = 0;
    let mut group_end = 0;
    let mut lane_free_at: Vec<u32> = Vec::new();
    for index in order {
        let start = minute_of_day(events[index].start);
        let end = minute_of_day(events[index].end);
        if end <= view_start {
            layout.earlier += 1;
            continue;
        }
        if start >= view_end {
            layout.later += 1;
            continue;
        }
        if !lane_free_at.is_empty() && start >= group_end {
            set_lanes(&mut layout.placements[group_from..], lane_free_at.len());
            group_from = layout.placements.len();
            lane_free_at.clear();
        }
        group_end = if lane_free_at.is_empty() {
            end
        } else {
            group_end.max(end)
        };
        let lane = match lane_free_at.iter().position(|&free| free <= start) {
            Some(lane) => {
                lane_free_at[lane] = end;
                lane
            }
            None => {
                lane_free_at.push(end);
                lane_free_at.len() - 1
            }
        };
        let top = start.max(view_start) - view_start;
        let bottom = end.min(view_end) - view_start;
        layout.placements.push(Placement {
            index,
            top,
            height: bottom - top,
            lane: lane as u32,
            lanes: 0,
        });
    }
    set_lanes(&mut layout.placements[group_from..], lane_free_at.len());
    layout
}

fn set_lanes(group: &mut [Placement], lanes: usize) {
    for placement in group {
        placement.lanes = lanes as u32;
    }
}

/// "Wed 30"
pub fn day_label(day: NaiveDate) -> String {
    day.format("%a %-d").to_string()
}

/// "28 September - 4 October 2026", "5 - 11 October 2026" or "28 December 2026 - 3 January 2027".
pub fn week_title(day: NaiveDate) -> String {
    let monday = week_start(day);
    let sunday = monday + Duration::days(6);
    let first = if monday.year() != sunday.year() {
        "%-d %B %Y"
    } else if monday.month() != sunday.month() {
        "%-d %B"
    } else {
        "%-d"
    };
    format!("{} - {}", monday.format(first), sunday.format("%-d %B %Y"))
}

/// "09:00 - 10:00"
pub fn time_range(start: NaiveTime, end: NaiveTime) -> String {
    format!("{} - {}", start.format("%H:%M"), end.format("%H:%M"))
}

/// The day a new event starts on: today when today is in the shown week, else the shown week's
/// Monday.
pub fn default_day(today: NaiveDate, shown: NaiveDate) -> NaiveDate {
    let monday = week_start(shown);
    if week_start(today) == monday {
        today
    } else {
        monday
    }
}

/// The date a date picker shows, with the day cut to the month's length (the picker keeps its
/// day when it turns the month); `None` for a month that does not exist.
pub fn picked_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    (28..=day.clamp(1, 31))
        .rev()
        .chain(std::iter::once(day.clamp(1, 28)))
        .find_map(|d| NaiveDate::from_ymd_opt(year, month, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// An event with a made-up UUID (`n` in its last group).
    fn event(n: u32, title: &str, date: NaiveDate, start: NaiveTime, end: NaiveTime) -> Event {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        Event::create(&id, title, date, start, end, None).unwrap()
    }

    /// (lane, lanes) of each placed event, by title, in the order of the day's list.
    fn lanes(day: &[&Event]) -> Vec<(String, u32, u32)> {
        let layout = lay_out_day(day);
        let mut out: Vec<(usize, String, u32, u32)> = layout
            .placements
            .iter()
            .map(|p| (p.index, day[p.index].title.clone(), p.lane, p.lanes))
            .collect();
        out.sort();
        out.into_iter().map(|(_, t, l, n)| (t, l, n)).collect()
    }

    #[test]
    fn a_week_starts_on_monday() {
        let monday = day(2026, 9, 28);
        assert_eq!(week_start(day(2026, 9, 30)), monday);
        assert_eq!(week_start(monday), monday);
        assert_eq!(week_start(day(2026, 10, 4)), monday);
        assert_eq!(week_start(day(2026, 10, 5)), day(2026, 10, 5));
        assert_eq!(week_start(day(2027, 1, 1)), day(2026, 12, 28));
    }

    #[test]
    fn the_week_view_steps_a_week_at_a_time_across_months_and_years() {
        assert_eq!(shift_weeks(day(2026, 9, 30), 1), day(2026, 10, 5));
        assert_eq!(shift_weeks(day(2026, 9, 30), -1), day(2026, 9, 21));
        assert_eq!(shift_weeks(day(2026, 9, 30), 0), day(2026, 9, 28));
        assert_eq!(shift_weeks(day(2026, 12, 30), 1), day(2027, 1, 4));
        assert_eq!(shift_weeks(day(2027, 1, 4), -1), day(2026, 12, 28));
    }

    #[test]
    fn a_week_is_seven_days_from_monday() {
        let days = week_days(day(2026, 10, 1));
        assert_eq!(days[0], day(2026, 9, 28));
        assert_eq!(days[3], day(2026, 10, 1));
        assert_eq!(days[6], day(2026, 10, 4));
    }

    #[test]
    fn only_the_events_of_the_shown_week_are_listed_each_under_its_day() {
        let events = vec![
            event(1, "Sunday before", day(2026, 9, 27), at(9, 0), at(10, 0)),
            event(2, "Late Wednesday", day(2026, 9, 30), at(15, 0), at(16, 0)),
            event(3, "Monday", day(2026, 9, 28), at(9, 0), at(10, 0)),
            event(4, "Early Wednesday", day(2026, 9, 30), at(8, 0), at(9, 0)),
            event(5, "Sunday", day(2026, 10, 4), at(19, 0), at(20, 0)),
            event(6, "Monday after", day(2026, 10, 5), at(9, 0), at(10, 0)),
            event(
                7,
                "Also early Wednesday",
                day(2026, 9, 30),
                at(8, 0),
                at(8, 30),
            ),
        ];
        let week = events_in_week(&events, day(2026, 10, 1));
        let titles: Vec<Vec<&str>> = week
            .iter()
            .map(|d| d.iter().map(|e| e.title.as_str()).collect())
            .collect();
        assert_eq!(
            titles,
            vec![
                vec!["Monday"],
                vec![],
                vec!["Also early Wednesday", "Early Wednesday", "Late Wednesday"],
                vec![],
                vec![],
                vec![],
                vec!["Sunday"],
            ]
        );
    }

    #[test]
    fn events_that_do_not_overlap_take_the_whole_column() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(11, 0), at(12, 30));
        let layout = lay_out_day(&[&a, &b]);
        assert_eq!(
            layout.placements,
            vec![
                Placement {
                    index: 0,
                    top: 60,
                    height: 60,
                    lane: 0,
                    lanes: 1
                },
                Placement {
                    index: 1,
                    top: 180,
                    height: 90,
                    lane: 0,
                    lanes: 1
                },
            ]
        );
        assert_eq!((layout.earlier, layout.later), (0, 0));
    }

    #[test]
    fn overlapping_events_sit_side_by_side() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(10, 0), at(11, 0));
        let b = event(2, "B", d, at(10, 0), at(10, 30));
        let c = event(3, "C", d, at(10, 15), at(12, 0));
        let alone = event(4, "Alone", d, at(14, 0), at(15, 0));
        let week = [a, b, c, alone];
        let listed = events_in_week(&week, d);
        assert_eq!(
            lanes(&listed[2]),
            vec![
                (String::from("B"), 0, 3),
                (String::from("A"), 1, 3),
                (String::from("C"), 2, 3),
                (String::from("Alone"), 0, 1),
            ]
        );
    }

    #[test]
    fn a_chain_of_overlaps_shares_its_lanes() {
        let d = day(2026, 9, 30);
        // A overlaps B, B overlaps C, A does not overlap C: two lanes, C reuses A's.
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(9, 30), at(10, 30));
        let c = event(3, "C", d, at(10, 0), at(11, 0));
        assert_eq!(
            lanes(&[&a, &b, &c]),
            vec![
                (String::from("A"), 0, 2),
                (String::from("B"), 1, 2),
                (String::from("C"), 0, 2),
            ]
        );
    }

    #[test]
    fn an_event_that_ends_as_another_starts_does_not_overlap_it() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(10, 0), at(11, 0));
        assert_eq!(
            lanes(&[&a, &b]),
            vec![(String::from("A"), 0, 1), (String::from("B"), 0, 1)]
        );
    }

    #[test]
    fn events_are_clipped_to_the_view_or_counted_when_wholly_outside() {
        let d = day(2026, 9, 30);
        let dawn = event(1, "Dawn", d, at(6, 0), at(7, 30));
        let into = event(2, "Into the view", d, at(7, 0), at(9, 0));
        let out_of = event(3, "Out of the view", d, at(19, 30), at(21, 0));
        let night = event(4, "Night", d, at(20, 0), at(22, 0));
        let day_list = [&dawn, &into, &out_of, &night];
        let layout = lay_out_day(&day_list);
        assert_eq!(layout.earlier, 1);
        assert_eq!(layout.later, 1);
        let spans: Vec<(usize, u32, u32)> = layout
            .placements
            .iter()
            .map(|p| (p.index, p.top, p.height))
            .collect();
        assert_eq!(spans, vec![(1, 0, 60), (2, 690, 30)]);
        assert_eq!(minute_of_day(at(20, 0)), (END_HOUR * 60));
        assert_eq!(minute_of_day(at(8, 0)), (FIRST_HOUR * 60));
    }

    #[test]
    fn an_event_wholly_before_the_view_does_not_narrow_the_events_in_it() {
        let d = day(2026, 9, 30);
        // Early overlaps Dawn, but only before 08:00, where nothing is shown.
        let dawn = event(1, "Dawn", d, at(6, 0), at(9, 0));
        let early = event(2, "Early", d, at(7, 0), at(7, 45));
        let nine = event(3, "Nine", d, at(9, 0), at(10, 0));
        assert_eq!(
            lanes(&[&dawn, &early, &nine]),
            vec![(String::from("Dawn"), 0, 1), (String::from("Nine"), 0, 1)]
        );
    }

    #[test]
    fn labels_read_as_a_person_writes_them() {
        assert_eq!(day_label(day(2026, 9, 30)), "Wed 30");
        assert_eq!(day_label(day(2026, 10, 4)), "Sun 4");
        assert_eq!(
            week_title(day(2026, 9, 30)),
            "28 September - 4 October 2026"
        );
        assert_eq!(week_title(day(2026, 10, 7)), "5 - 11 October 2026");
        assert_eq!(
            week_title(day(2026, 12, 31)),
            "28 December 2026 - 3 January 2027"
        );
        assert_eq!(time_range(at(9, 0), at(10, 30)), "09:00 - 10:30");
    }

    #[test]
    fn a_new_event_starts_today_in_this_week_else_on_the_shown_monday() {
        let today = day(2026, 9, 30);
        assert_eq!(default_day(today, day(2026, 9, 28)), today);
        assert_eq!(default_day(today, day(2026, 10, 3)), today);
        assert_eq!(default_day(today, day(2026, 10, 7)), day(2026, 10, 5));
        assert_eq!(default_day(today, day(2026, 9, 21)), day(2026, 9, 21));
    }

    #[test]
    fn a_picked_day_past_the_end_of_its_month_is_cut_to_the_last_day() {
        assert_eq!(picked_date(2026, 9, 30), Some(day(2026, 9, 30)));
        assert_eq!(picked_date(2026, 2, 31), Some(day(2026, 2, 28)));
        assert_eq!(picked_date(2028, 2, 30), Some(day(2028, 2, 29)));
        assert_eq!(picked_date(2026, 4, 31), Some(day(2026, 4, 30)));
        assert_eq!(picked_date(2026, 1, 0), Some(day(2026, 1, 1)));
        assert_eq!(picked_date(2026, 13, 1), None);
        assert_eq!(picked_date(2026, 0, 1), None);
    }
}
