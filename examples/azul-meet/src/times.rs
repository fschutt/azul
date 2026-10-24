//! A meeting's times as people say them, in the window's language: when it is ("Fri 9 Oct
//! 2026, 14:00-15:00") and where it stands now ("starts in 25 min"). These need the kit's
//! words, so they live here and not in `rooms.rs`, which azul-calendar-core compiles without
//! the kit.

/// `starts` to `ends` (seconds since 1970) in the time zone `offset_secs` east of UTC: "Thu 9 Oct
/// 2026, 14:00-15:00", or both dates when it ends on another day.
#[must_use]
pub fn when_text(starts: u64, ends: u64, offset_secs: i32) -> String {
    use chrono::{FixedOffset, TimeZone};
    let Some(zone) = FixedOffset::east_opt(offset_secs) else {
        return String::new();
    };
    let at = |secs: u64| {
        i64::try_from(secs)
            .ok()
            .and_then(|secs| zone.timestamp_opt(secs, 0).single())
    };
    let (Some(a), Some(b)) = (at(starts), at(ends)) else {
        return String::new();
    };
    let day = day_text;
    if a.date_naive() == b.date_naive() {
        format!("{}-{}", day(&a), b.format("%H:%M"))
    } else {
        format!("{} - {}", day(&a), day(&b))
    }
}

/// "Fri 9 Oct 2026, 14:00": the kit's short date with the year, in the window's language, and
/// the time.
fn day_text(at: &chrono::DateTime<chrono::FixedOffset>) -> String {
    use chrono::Datelike;
    let date = azul_appkit::l10n::date_text(
        azul_appkit::l10n::DateStyle::ShortDateYear,
        at.year(),
        at.month(),
        at.day(),
        at.weekday().num_days_from_monday(),
    );
    format!("{date}, {}", at.format("%H:%M"))
}

/// The moment `at` (seconds since 1970) in the time zone `offset_secs` east of UTC: "Fri 9 Oct
/// 2026, 14:00" (a recent direct call).
#[must_use]
pub fn at_text(at: i64, offset_secs: i32) -> String {
    use chrono::{FixedOffset, TimeZone};
    FixedOffset::east_opt(offset_secs)
        .and_then(|zone| zone.timestamp_opt(at, 0).single())
        .map(|at| day_text(&at))
        .unwrap_or_default()
}

/// `n` minutes as people say it: "5 min", "2 h", "2 h 5 min", "3 days" (in the window's
/// language).
fn span(minutes: u64) -> String {
    use azul_appkit::l10n::{t_args, Arg};
    let n = |v: u64| Arg::from(v.to_string());
    match minutes {
        0..=59 => t_args("azmeet-span-min", &[("min", n(minutes.max(1)))]),
        60..=1439 if minutes % 60 == 0 => t_args("azmeet-span-h", &[("h", n(minutes / 60))]),
        60..=1439 => t_args("azmeet-span-h-min", &[("h", n(minutes / 60)), ("min", n(minutes % 60))]),
        _ => t_args("azmeet-span-days", &[("days", Arg::from(minutes / 1440))]),
    }
}

/// Where a meeting with these times (seconds since 1970) stands at `now`: "starts in 25 min",
/// "started 5 min ago, ends in 55 min", "ended 2 h ago".
#[must_use]
pub fn meeting_status(starts: u64, ends: u64, now: u64) -> String {
    let minutes = |a: u64, b: u64| a.saturating_sub(b).div_ceil(60);
    use azul_appkit::l10n::{t_args, Arg};
    if now < starts {
        t_args("azmeet-starts-in", &[("span", Arg::from(span(minutes(starts, now))))])
    } else if now < ends {
        t_args(
            "azmeet-started-ago",
            &[("ago", Arg::from(span(minutes(now, starts)))), ("left", Arg::from(span(minutes(ends, now))))],
        )
    } else {
        t_args("azmeet-ended-ago", &[("ago", Arg::from(span(minutes(now, ends))))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meetings_times_read_as_people_say_them() {
        crate::l10n::in_english();
        let starts = 1_760_000_000;
        let ends = starts + 3600;
        assert_eq!(meeting_status(starts, ends, starts - 25 * 60), "starts in 25 min");
        assert_eq!(meeting_status(starts, ends, starts - 125 * 60), "starts in 2 h 5 min");
        assert_eq!(meeting_status(starts, ends, starts - 120 * 60), "starts in 2 h");
        assert_eq!(meeting_status(starts, ends, starts - 3 * 86_400), "starts in 3 days");
        assert_eq!(meeting_status(starts, ends, starts - 30), "starts in 1 min", "never 0");
        assert_eq!(meeting_status(starts, ends, starts + 300), "started 5 min ago, ends in 55 min");
        assert_eq!(meeting_status(starts, ends, ends + 7200), "ended 2 h ago");
    }

    /// 2026-10-09T14:00:00Z, a Friday.
    const FRIDAY_14_UTC: u64 = 1_791_554_400;

    #[test]
    fn a_meetings_times_show_in_the_time_zone() {
        crate::l10n::in_english();
        let t = FRIDAY_14_UTC;
        assert_eq!(when_text(t, t + 3600, 0), "Fri 9 Oct 2026, 14:00-15:00");
        assert_eq!(when_text(t, t + 3600, 7200), "Fri 9 Oct 2026, 16:00-17:00");
        assert_eq!(
            when_text(t, t + 11 * 3600, 7200),
            "Fri 9 Oct 2026, 16:00 - Sat 10 Oct 2026, 03:00",
            "both dates when it ends on another day"
        );
    }
}
