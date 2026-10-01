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
    todo!()
}

/// The sample events around `today`, their ids drawn from `ids`, the work ones in the calendar
/// `work`.
pub fn sample_events(today: NaiveDate, work: &str, mut ids: impl FnMut() -> String) -> Vec<Event> {
    todo!()
}

/// Writes the sample into `data_dir` unless it holds events already; answers how many events
/// were written.
pub fn write_sample(data_dir: &Path, today: NaiveDate) -> std::io::Result<usize> {
    todo!()
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
