//! Sample data for a first run (`--sample`): a "Work" calendar beside the default one and a
//! week of events around today - repeating ones (every weekday, every other Friday, monthly,
//! weekly), a reminder, a location, an all-day event of three days - so every view has
//! something to show. Written only into a calendar that has no events yet.

use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};

use crate::{
    calendars::{self, Calendar, Colour},
    event::{self, Event},
    rrule::{ByDay, Freq, Rule},
    week,
};

fn at(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or(NaiveTime::MIN)
}

/// The "Work" calendar of the sample, with the id `id`.
#[must_use]
pub fn work_calendar(id: &str) -> Calendar {
    Calendar {
        id: id.to_string(),
        name: String::from("Work"),
        colour: Colour::Green,
    }
}

/// The sample events around `today`, their ids drawn from `ids`, the work ones in the calendar
/// `work`.
pub fn sample_events(today: NaiveDate, work: &str, mut ids: impl FnMut() -> String) -> Vec<Event> {
    let monday = week::week_start(today);
    let day = |weekday: Weekday| monday + Duration::days(i64::from(weekday.num_days_from_monday()));
    let mut out = Vec::new();
    let mut add = |mut e: Event, calendar: &str, f: &dyn Fn(&mut Event)| {
        e.calendar = calendar.to_string();
        f(&mut e);
        if let Ok(e) = e.check() {
            out.push(e);
        }
    };
    if let Ok(e) = Event::create(
        &ids(),
        "Team standup",
        day(Weekday::Mon),
        at(9, 30),
        at(9, 45),
        None,
    ) {
        add(e, work, &|e: &mut Event| {
            e.repeat = Some(
                Rule::new(Freq::Weekly).with_by_day(
                    [
                        Weekday::Mon,
                        Weekday::Tue,
                        Weekday::Wed,
                        Weekday::Thu,
                        Weekday::Fri,
                    ]
                    .into_iter()
                    .map(ByDay::every)
                    .collect(),
                ),
            );
        });
    }
    if let Ok(e) = Event::create(
        &ids(),
        "Sprint review",
        day(Weekday::Fri),
        at(14, 0),
        at(15, 0),
        None,
    ) {
        add(e, work, &|e: &mut Event| {
            e.location = String::from("Room 4");
            e.repeat = Some(
                Rule::new(Freq::Weekly)
                    .with_interval(2)
                    .with_by_day(vec![ByDay::every(Weekday::Fri)]),
            );
        });
    }
    if let Ok(e) = Event::create(
        &ids(),
        "Lunch with Ana",
        day(Weekday::Wed),
        at(12, 30),
        at(13, 30),
        None,
    ) {
        add(e, "", &|e: &mut Event| {
            e.location = String::from("Corner cafe")
        });
    }
    if let Ok(e) = Event::create(
        &ids(),
        "Dentist",
        day(Weekday::Thu),
        at(16, 0),
        at(16, 45),
        None,
    ) {
        add(e, "", &|e: &mut Event| e.reminder = Some(60));
    }
    if let Ok(e) = Event::create(
        &ids(),
        "Yoga",
        day(Weekday::Tue),
        at(18, 0),
        at(19, 0),
        None,
    ) {
        add(e, "", &|e: &mut Event| {
            e.repeat = Some(Rule::new(Freq::Weekly))
        });
    }
    let next_friday = day(Weekday::Fri) + Duration::days(7);
    if let Ok(e) = Event::create_all_day(
        &ids(),
        "Long weekend",
        next_friday,
        next_friday + Duration::days(2),
    ) {
        add(e, "", &|_: &mut Event| {});
    }
    let first = today.with_day(1).unwrap_or(today);
    if let Ok(e) = Event::create_all_day(&ids(), "Rent", first, first) {
        add(e, "", &|e: &mut Event| {
            e.repeat = Some(Rule::new(Freq::Monthly))
        });
    }
    out
}

/// Writes the sample into `data_dir` unless it holds events already; answers how many events
/// were written.
pub fn write_sample(data_dir: &Path, today: NaiveDate) -> std::io::Result<usize> {
    let (existing, _) = event::load_all(data_dir);
    if !existing.is_empty() {
        return Ok(0);
    }
    let work = work_calendar(&calendars::new_calendar_id());
    calendars::save(data_dir, &work)?;
    let events = sample_events(today, &work.id, event::new_event_id);
    for e in &events {
        event::save(data_dir, e)?;
    }
    Ok(events.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_dir::TempDir, views};

    const WORK: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";

    fn counter() -> impl FnMut() -> String {
        let mut n = 0u32;
        move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }
    }

    #[test]
    fn the_sample_fills_every_view_of_todays_week() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let events = sample_events(today, WORK, counter());
        assert_eq!(events.len(), 7);
        let (from, to) = views::visible_range(views::ViewKind::Week, today);
        let week = views::occurrences(&events, from, to, |_| true);
        // five standups, the review, lunch, the dentist, yoga, and the rent on 1 October
        assert_eq!(week.len(), 10, "{week:?}");
        assert!(events.iter().any(|e| e.all_day && e.span_days() == 2));
        assert_eq!(events.iter().filter(|e| e.calendar == WORK).count(), 2);
        // and the next week has the long weekend and the standups again
        let next = views::occurrences(
            &events,
            from + Duration::days(7),
            to + Duration::days(7),
            |_| true,
        );
        assert!(next.iter().any(|o| events[o.index].title == "Long weekend"));
        assert!(
            next.iter()
                .filter(|o| events[o.index].title == "Team standup")
                .count()
                == 5
        );
    }

    #[test]
    fn the_sample_is_written_only_into_an_empty_calendar() {
        let dir = TempDir::create();
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        assert_eq!(write_sample(&dir.0, today).unwrap(), 7);
        assert_eq!(event::load_all(&dir.0).0.len(), 7);
        assert_eq!(calendars::load_all(&dir.0).len(), 2);
        assert_eq!(write_sample(&dir.0, today).unwrap(), 0);
        assert_eq!(event::load_all(&dir.0).0.len(), 7);
    }
}
